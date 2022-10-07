use std::io;
use std::net::ToSocketAddrs;

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Error, Result};

use log::info;

use thiserror;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time;
use tokio_graceful_shutdown::{IntoSubsystem, SubsystemHandle};
use tokio_io_timeout::TimeoutStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::{self, ClientConfig, OwnedTrustAnchor};
use tokio_rustls::TlsConnector;

use crate::nfc::reader::Uid;
use crate::subsystems::config_manager::{ConfigCommand, ConnectionConfig};

pub type AchievementId = [u8; 20];

#[derive(thiserror::Error, Debug)]
#[error("Invalid credentials")]
struct InvalidCredentialsError;

#[derive(Debug)]
pub enum NetworkerStatus {
    NoConfig,
    InvalidCredentials,
    Disconnected,
    Connected,
}

type Stream = TimeoutStream<TlsStream<TcpStream>>;

#[derive(Debug)]
pub enum NetworkerCommand {
    SetConnection {
        connection_config: ConnectionConfig,
    },
    CheckUid {
        uid: Uid,
        responder: oneshot::Sender<Option<Vec<AchievementId>>>,
    },
    GetAudio {
        id: AchievementId,
        responder: oneshot::Sender<Option<Vec<u8>>>,
    },
}

pub struct Networker {
    rx: mpsc::Receiver<NetworkerCommand>,
    status_tx: mpsc::Sender<NetworkerStatus>,
    config: mpsc::Sender<ConfigCommand>,
}

impl Networker {
    pub fn new(
        rx: mpsc::Receiver<NetworkerCommand>,
        status_tx: mpsc::Sender<NetworkerStatus>,
        config: mpsc::Sender<ConfigCommand>,
    ) -> Self {
        Self {
            rx,
            status_tx,
            config,
        }
    }

    async fn process(&mut self) -> Result<()> {
        let (config_tx, config_rx) = oneshot::channel();
        self.config
            .send(ConfigCommand::GetConnection {
                responder: config_tx,
            })
            .await?;
        let mut maybe_connection_config = config_rx.await?;

        let mut root_cert_store = rustls::RootCertStore::empty();

        root_cert_store.add_server_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(
            |ta| {
                OwnedTrustAnchor::from_subject_spki_name_constraints(
                    ta.subject,
                    ta.spki,
                    ta.name_constraints,
                )
            },
        ));

        let client_config = ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_cert_store)
            .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(client_config));
        let mut maybe_stream: Option<Pin<Box<Stream>>> = None;

        if maybe_connection_config.is_some() {
            self.status_tx.send(NetworkerStatus::Disconnected).await?;
        } else {
            self.status_tx.send(NetworkerStatus::NoConfig).await?;
        }

        let mut interval = time::interval(Duration::from_secs(3));

        loop {
            tokio::select! {
                maybe_command = self.rx.recv() => {
                    let stream = match maybe_stream {
                        Some(ref mut stream) => stream,
                        None => continue,
                    };

                    use NetworkerCommand::*;

                    match maybe_command {
                        Some(command) => {
                            match command {
                                SetConnection { connection_config } => {
                                    maybe_connection_config = Some(connection_config.clone());
                                    maybe_stream = None;

                                    let (config_tx, config_rx) = oneshot::channel();
                                    self.config.send(ConfigCommand::SetConnection {
                                        connection_config,
                                        responder: config_tx,
                                    }).await?;
                                    config_rx.await?;
                                },
                                CheckUid { uid, responder } => {
                                    match self.check_uid(stream, &uid).await {
                                        Ok(achievements) => responder.send(achievements).unwrap(),
                                        Err(_) => {
                                            maybe_stream = None;
                                            self.status_tx.send(NetworkerStatus::Disconnected).await?;
                                        },
                                    }
                                },
                                GetAudio { id, responder } => {
                                    match self.get_audio(stream, &id).await {
                                        Ok(data) => responder.send(data).unwrap(),
                                        Err(_) => {
                                            maybe_stream = None;
                                            self.status_tx.send(NetworkerStatus::Disconnected).await?;
                                        },
                                    }
                                },
                            }
                        },
                        None => break,
                    }
                },
                _ = interval.tick() => {
                    match maybe_stream {
                        Some(ref mut stream) => {
                            if self.ping(stream).await.is_err() {
                                maybe_stream = None;
                                self.status_tx.send(NetworkerStatus::Disconnected).await?;
                            };
                        },
                        None => {
                            if let Some(connection_config) = maybe_connection_config.as_ref() {
                                if let Ok(maybe_connected_stream) = self.connect(
                                    &connector,
                                    connection_config
                                ).await {
                                    match maybe_connected_stream {
                                        Some(connected_stream) => {
                                            maybe_stream = Some(connected_stream);
                                            self.status_tx.send(NetworkerStatus::Connected).await?
                                        },
                                        None => {
                                            self.status_tx.send(NetworkerStatus::InvalidCredentials).await?
                                        },
                                    }
                                }
                            }
                        },
                    }
                },
            }
        }

        Ok(())
    }

    async fn check_uid(
        &self,
        stream: &mut Pin<Box<Stream>>,
        uid: &Uid,
    ) -> Result<Option<Vec<AchievementId>>> {
        stream.write_u8(0x00).await?;
        stream.write_all(uid).await?;

        let result = stream.read_u8().await?;

        if result == 0x00 {
            return Ok(None);
        }

        let achievements_count = stream.read_u8().await?;
        let mut achievements = Vec::with_capacity(achievements_count as usize);

        for _ in 0..achievements_count {
            let mut achievement_id = [0; 20];
            stream.read_exact(&mut achievement_id).await?;
            achievements.push(achievement_id);
        }

        Ok(Some(achievements))
    }

    async fn get_audio(
        &self,
        stream: &mut Pin<Box<Stream>>,
        id: &AchievementId,
    ) -> Result<Option<Vec<u8>>> {
        stream.write_u8(0x01).await?;
        stream.write_all(id).await?;

        let result = stream.read_u8().await?;

        if result == 0x00 {
            return Ok(None);
        }

        let length = stream.read_u32_le().await?;
        let mut data = vec![0u8; length as usize];
        stream.read_exact(&mut data).await?;

        Ok(Some(data))
    }

    async fn ping(&self, stream: &mut Pin<Box<Stream>>) -> Result<()> {
        stream.write_u8(0x02).await?;
        stream.read_u8().await?;

        Ok(())
    }

    async fn connect(
        &self,
        connector: &TlsConnector,
        connection_config: &ConnectionConfig,
    ) -> Result<Option<Pin<Box<Stream>>>> {
        let address = (connection_config.host.as_str(), connection_config.port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;

        let tcp_stream = TcpStream::connect(&address).await?;
        let domain = rustls::ServerName::try_from(connection_config.host.as_str())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid DNS name"))?;

        let tls_stream = connector.connect(domain, tcp_stream).await?;
        let mut timeout_stream = TimeoutStream::new(tls_stream);
        timeout_stream.set_read_timeout(Some(Duration::from_secs(2)));
        timeout_stream.set_write_timeout(Some(Duration::from_secs(2)));

        let mut pinned_stream = Box::pin(timeout_stream);

        if !self
            .authenticate(&mut pinned_stream, connection_config)
            .await?
        {
            return Ok(None);
        }

        Ok(Some(pinned_stream))
    }

    async fn authenticate(
        &self,
        stream: &mut Pin<Box<Stream>>,
        connection_config: &ConnectionConfig,
    ) -> Result<bool> {
        let auth_string = format!("{}:{}", connection_config.user, connection_config.secret);
        stream.write_u8(auth_string.len() as u8).await?;
        stream.write_all(auth_string.as_bytes()).await?;

        Ok(stream.read_u8().await? == 0x00)
    }
}

#[async_trait::async_trait]
impl IntoSubsystem<Error> for Networker {
    async fn run(mut self, subsys: SubsystemHandle) -> Result<()> {
        tokio::select! {
            _ = subsys.on_shutdown_requested() => {
                info!("Networker shutting down");
            },
            res = self.process() => res?,
        }

        Ok(())
    }
}
