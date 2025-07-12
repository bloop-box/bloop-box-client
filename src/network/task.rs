use crate::hardware::nfc::NfcUid;
use crate::network::message::{
    Capabilities, ClientMessage, DataHash, ErrorResponse, Message, ServerMessage,
};
use crate::network::skip_certificate_verification::SkipCertificateVerification;
use crate::network::{AudioResponse, BloopResponse, PreloadCheckResponse};
use crate::state::PersistedState;
use anyhow::{bail, Context, Error, Result};
use local_ip_address::local_ip;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, RootCertStore};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{lookup_host, TcpStream};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::{select, time};
use tokio_graceful_shutdown::{FutureExt, IntoSubsystem, SubsystemHandle};
use tokio_io_timeout::TimeoutStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

type PinnedStream = Pin<Box<TimeoutStream<TlsStream<TcpStream>>>>;

#[derive(Debug, Default)]
pub enum RootCertSource {
    #[default]
    BuiltIn,
    Native,
    DangerousDisabled,
}

#[derive(Clone, Copy, Debug)]
pub enum NetworkStatus {
    Unconfigured,
    InvalidCredentials,
    Disconnected,
    Connected { capabilities: Capabilities },
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct NetworkState {
    connection: Option<ConnectionState>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnectionState {
    pub host: String,
    pub port: u16,
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug)]
pub enum Command {
    SetConnectionState(ConnectionState),
    Bloop {
        nfc_uid: NfcUid,
        response: oneshot::Sender<BloopResponse>,
    },
    RetrieveAudio {
        achievement_id: Uuid,
        response: oneshot::Sender<AudioResponse>,
    },
    PreloadCheck {
        audio_manifest_hash: Option<DataHash>,
        response: oneshot::Sender<PreloadCheckResponse>,
    },
    Shutdown {
        response: oneshot::Sender<()>,
    },
}

pub struct NetworkTask {
    connector: TlsConnector,
    connection: Option<Connection<PinnedStream>>,
    rx: mpsc::Receiver<Command>,
    status_tx: watch::Sender<NetworkStatus>,
    state: PersistedState<NetworkState>,
    credentials_invalid: bool,
    shutdown: bool,
}

impl Debug for NetworkTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkTask")
            .field("connection", &self.connection)
            .field("state", &self.state)
            .field("credentials_invalid", &self.credentials_invalid)
            .finish_non_exhaustive()
    }
}

impl NetworkTask {
    pub async fn new(
        root_cert_source: RootCertSource,
        rx: mpsc::Receiver<Command>,
        status_tx: watch::Sender<NetworkStatus>,
    ) -> Result<Self> {
        let connector = create_tls_connector(root_cert_source)?;
        let state = PersistedState::new("network", None).await?;

        Ok(Self {
            connector,
            connection: None,
            rx,
            status_tx,
            state,
            credentials_invalid: false,
            shutdown: false,
        })
    }

    pub async fn process(&mut self) -> Result<()> {
        self.status_tx.send(if self.state.connection.is_some() {
            NetworkStatus::Disconnected
        } else {
            NetworkStatus::Unconfigured
        })?;

        let mut ticker = time::interval(Duration::from_secs(3));

        loop {
            select! {
                command = self.rx.recv() => match command {
                    Some(command) => self.handle_command(command).await?,
                    None => break,
                },
                _ = ticker.tick() => self.handle_tick().await?,
            }
        }

        Ok(())
    }

    async fn handle_command(&mut self, command: Command) -> Result<()> {
        debug!("received command: {:?}", command);

        match command {
            Command::SetConnectionState(new_state) => {
                self.connection = None;
                self.state.mutate(|state| {
                    state.connection.replace(new_state);
                })?;
                self.credentials_invalid = false;
            }

            Command::Bloop { nfc_uid, response } => {
                let Some(ref mut connection) = self.connection else {
                    let _ = response.send(BloopResponse::Rejected {});
                    return Ok(());
                };
                let result = bloop(&mut connection.stream, nfc_uid).await;
                self.handle_response(response, result, BloopResponse::Rejected);
            }
            Command::RetrieveAudio {
                achievement_id,
                response,
            } => {
                let Some(ref mut connection) = self.connection else {
                    let _ = response.send(AudioResponse::Disconnected);
                    return Ok(());
                };
                let result = retrieve_audio(&mut connection.stream, achievement_id).await;
                self.handle_response(response, result, AudioResponse::Disconnected);
            }
            Command::PreloadCheck {
                audio_manifest_hash,
                response,
            } => {
                let Some(ref mut connection) = self.connection else {
                    let _ = response.send(PreloadCheckResponse::Match);
                    return Ok(());
                };
                let result = preload_check(&mut connection.stream, audio_manifest_hash).await;
                self.handle_response(response, result, PreloadCheckResponse::Match);
            }
            Command::Shutdown { response } => {
                let Some(ref mut connection) = self.connection else {
                    let _ = response.send(());
                    return Ok(());
                };

                write_to_stream(&mut connection.stream, ClientMessage::Quit).await?;
                self.connection = None;
                self.shutdown = true;
            }
        }

        Ok(())
    }

    async fn handle_tick(&mut self) -> Result<()> {
        if self.shutdown {
            return Ok(());
        }

        if let Some(ref mut connection) = self.connection {
            if let Err(err) = ping(&mut connection.stream).await {
                warn!("ping timeout: {}", err);
                self.connection = None;
                let _ = self.status_tx.send(NetworkStatus::Disconnected);
            };

            return Ok(());
        }

        self.try_connect().await;

        Ok(())
    }

    fn handle_response<T>(&mut self, response: oneshot::Sender<T>, result: Result<T>, fallback: T) {
        match result {
            Ok(result) => {
                let _ = response.send(result);
            }
            Err(error) => {
                warn!("lost connection due to: {}", error);
                self.connection = None;
                let _ = self.status_tx.send(NetworkStatus::Disconnected);
                let _ = response.send(fallback);
            }
        }
    }

    async fn try_connect(&mut self) {
        if self.credentials_invalid {
            return;
        }

        let Some(state) = self.state.connection.as_ref() else {
            return;
        };

        info!("trying to connect to server");

        match self.connect(state).await {
            Ok(Some(connection)) => {
                self.credentials_invalid = false;
                let _ = self.status_tx.send(NetworkStatus::Connected {
                    capabilities: connection.capabilities,
                });
                self.connection = Some(connection);
            }
            Ok(None) => {
                self.credentials_invalid = true;
                let _ = self.status_tx.send(NetworkStatus::InvalidCredentials);
            }
            Err(err) => {
                error!("failed to connect to server: {}", err);
                self.connection = None;
            }
        }
    }

    async fn connect(&self, state: &ConnectionState) -> Result<Option<Connection<PinnedStream>>> {
        let address = lookup_host((state.host.as_str(), state.port))
            .await
            .with_context(|| format!("could not look up {}", state.host))?
            .next()
            .with_context(|| format!("no addresses found for {}", state.host))?;

        let tcp_stream = TcpStream::connect(&address)
            .await
            .with_context(|| format!("unable to connect to {address}"))?;
        let domain = ServerName::try_from(state.host.as_str())
            .with_context(|| format!("invalid DNS name: {}", state.host))?
            .to_owned();

        let tls_stream = self.connector.connect(domain, tcp_stream).await?;
        let mut timeout_stream = TimeoutStream::new(tls_stream);
        timeout_stream.set_read_timeout(Some(Duration::from_secs(5)));
        timeout_stream.set_write_timeout(Some(Duration::from_secs(5)));

        let mut pinned_stream = Box::pin(timeout_stream);
        let (version, capabilities) = negotiate_version(&mut pinned_stream).await?;

        if !authenticate(&mut pinned_stream, state).await? {
            return Ok(None);
        }

        Ok(Some(Connection {
            stream: pinned_stream,
            version,
            capabilities,
        }))
    }
}

#[derive(Debug)]
enum ProtocolVersion {
    Three,
}

#[derive(Debug)]
struct Connection<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    stream: S,
    #[allow(dead_code)]
    version: ProtocolVersion,
    capabilities: Capabilities,
}

async fn bloop<S>(stream: &mut S, nfc_uid: NfcUid) -> Result<BloopResponse>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    write_to_stream(stream, ClientMessage::Bloop { nfc_uid }).await?;

    match read_from_stream(stream).await? {
        ServerMessage::BloopAccepted { achievements } => {
            Ok(BloopResponse::Accepted { achievements })
        }
        ServerMessage::Error(ErrorResponse::UnknownNfcUid) => Ok(BloopResponse::Rejected {}),
        ServerMessage::Error(ErrorResponse::NfcUidThrottled) => Ok(BloopResponse::Throttled {}),
        message => bail!("unexpected message from server: {:?}", message),
    }
}

async fn retrieve_audio<S>(stream: &mut S, achievement_id: Uuid) -> Result<AudioResponse>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    write_to_stream(stream, ClientMessage::RetrieveAudio { achievement_id }).await?;

    match read_from_stream(stream).await? {
        ServerMessage::AudioData { data } => Ok(AudioResponse::Data(data)),
        ServerMessage::Error(ErrorResponse::AudioUnavailable) => Ok(AudioResponse::NotFound),
        message => bail!("unexpected message from server: {:?}", message),
    }
}

async fn preload_check<S>(
    stream: &mut S,
    audio_manifest_hash: Option<DataHash>,
) -> Result<PreloadCheckResponse>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    write_to_stream(
        stream,
        ClientMessage::PreloadCheck {
            audio_manifest_hash,
        },
    )
    .await?;

    match read_from_stream(stream).await? {
        ServerMessage::PreloadMatch => Ok(PreloadCheckResponse::Match),
        ServerMessage::PreloadMismatch {
            audio_manifest_hash,
            achievements,
        } => Ok(PreloadCheckResponse::Mismatch {
            audio_manifest_hash,
            achievements,
        }),
        message => bail!("unexpected message from server: {:?}", message),
    }
}

async fn ping<S>(stream: &mut S) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    write_to_stream(stream, ClientMessage::Ping).await?;

    match read_from_stream(stream).await? {
        ServerMessage::Pong => Ok(()),
        message => bail!("unexpected message from server: {:?}", message),
    }
}

async fn negotiate_version<S>(stream: &mut S) -> Result<(ProtocolVersion, Capabilities)>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    write_to_stream(
        stream,
        ClientMessage::ClientHandshake {
            min_version: 3,
            max_version: 3,
        },
    )
    .await?;

    match read_from_stream(stream).await? {
        ServerMessage::ServerHandshake {
            accepted_version,
            capabilities,
        } => {
            if accepted_version != 3 {
                bail!("server accepted unsupported version {}", accepted_version);
            }

            Ok((ProtocolVersion::Three, capabilities))
        }
        ServerMessage::Error(ErrorResponse::UnsupportedVersionRange) => {
            bail!("server does not support any version in the range 3 - 3")
        }
        message => bail!("unexpected message from server: {:?}", message),
    }
}

async fn authenticate<S>(stream: &mut S, state: &ConnectionState) -> Result<bool>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    write_to_stream(
        stream,
        ClientMessage::Authentication {
            client_id: state.client_id.clone(),
            client_secret: state.client_secret.clone(),
            ip_addr: local_ip()?,
        },
    )
    .await?;

    match read_from_stream(stream).await? {
        ServerMessage::AuthenticationAccepted => Ok(true),
        ServerMessage::Error(ErrorResponse::InvalidCredentials) => Ok(false),
        message => bail!("unexpected message from server: {:?}", message),
    }
}

async fn read_from_stream<S: AsyncRead + Unpin + Send>(stream: &mut S) -> Result<ServerMessage> {
    let message_type = stream.read_u8().await?;
    let payload_length = stream.read_u32_le().await?;

    if payload_length == 0 {
        return Ok(Message::new(message_type, vec![]).try_into()?);
    }

    let mut message = vec![0; payload_length as usize];
    stream.read_exact(&mut message).await?;

    Ok(Message::new(message_type, message).try_into()?)
}

async fn write_to_stream<S: AsyncWrite + Unpin + Send>(
    stream: &mut S,
    message: impl Into<Message>,
) -> Result<()> {
    let message: Message = message.into();
    stream.write_all(&message.into_bytes()).await?;
    stream.flush().await?;

    Ok(())
}

fn create_tls_connector(source: RootCertSource) -> Result<TlsConnector> {
    let root_cert_store = match source {
        RootCertSource::BuiltIn => RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.into(),
        },
        RootCertSource::Native => {
            let mut root_cert_store = RootCertStore::empty();

            for cert in
                rustls_native_certs::load_native_certs().expect("could not load native certs")
            {
                root_cert_store.add(cert)?;
            }

            root_cert_store
        }
        RootCertSource::DangerousDisabled => {
            warn!("certificate verification is disabled; only use this for testing!");

            let client_config = ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(SkipCertificateVerification::new())
                .with_no_client_auth();

            return Ok(TlsConnector::from(Arc::new(client_config)));
        }
    };

    let client_config = ClientConfig::builder()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth();

    Ok(TlsConnector::from(Arc::new(client_config)))
}

#[async_trait::async_trait]
impl IntoSubsystem<Error> for NetworkTask {
    async fn run(mut self, subsys: SubsystemHandle) -> Result<()> {
        if let Ok(result) = self.process().cancel_on_shutdown(&subsys).await {
            result?;
        }

        Ok(())
    }
}
