use crate::audio::AudioPlayer;
use crate::hardware::data_path;
use crate::hardware::led::{Color, LedController};
use crate::hardware::nfc::{NfcReader, NfcUid};
use crate::hardware::system::{set_wifi_credentials, shutdown_system};
use crate::state::PersistedState;
use anyhow::{bail, Error, Result};
use bloop_client_framework::{
    AudioCache, BloopClient, ConnectionConfig, ConnectionStatus, PreloadOutcome, RequestError,
};
use bloop_protocol::message::ErrorResponse;
use bloop_protocol::{Capabilities, DataHash};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::time::sleep;
use tokio::{join, select};
use tokio_graceful_shutdown::{FutureExt, IntoSubsystem, SubsystemHandle};
use tracing::{error, info, instrument, warn};

/// Persisted connection details, written by the `c` config card.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnectionState {
    pub host: String,
    pub port: u16,
    pub client_id: String,
    pub client_secret: String,
}

impl From<ConnectionState> for ConnectionConfig {
    fn from(state: ConnectionState) -> Self {
        Self {
            host: state.host,
            port: state.port,
            client_id: state.client_id,
            client_secret: state.client_secret,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct NetworkState {
    connection: Option<ConnectionState>,
}

pub struct EngineProps {
    pub led_controller: LedController,
    pub nfc_reader: NfcReader,
    pub network_client: BloopClient,
    pub audio_player: AudioPlayer,
    pub network_status: watch::Receiver<ConnectionStatus>,
    pub volume_range_tx: mpsc::Sender<(f32, f32)>,
}

pub struct Engine {
    led_controller: LedController,
    nfc_reader: NfcReader,
    network_client: BloopClient,
    audio_player: AudioPlayer,
    network_status: watch::Receiver<ConnectionStatus>,
    audio_cache: AudioCache,
    volume_range_tx: mpsc::Sender<(f32, f32)>,
    state: PersistedState<EngineState>,
    network_state: PersistedState<NetworkState>,
}

impl Engine {
    pub async fn new(props: EngineProps) -> Result<Self> {
        let state = PersistedState::new("engine", None).await?;
        let network_state: PersistedState<NetworkState> =
            PersistedState::new("network", None).await?;
        let audio_cache = AudioCache::new(data_path().await?.join("cache"));

        if let Some(connection) = network_state.connection.clone() {
            props
                .network_client
                .configure(Some(connection.into()))
                .await?;
        }

        Ok(Self {
            led_controller: props.led_controller,
            nfc_reader: props.nfc_reader,
            network_client: props.network_client,
            audio_player: props.audio_player,
            network_status: props.network_status,
            volume_range_tx: props.volume_range_tx,
            audio_cache,
            state,
            network_state,
        })
    }

    #[instrument(skip(self, subsys))]
    async fn process(&mut self, subsys: &SubsystemHandle) -> Result<()> {
        if self.state.config_nfc_uids.is_empty() {
            self.add_config_uid().await?;
        }

        loop {
            self.set_idle_led().await?;

            select! {
                nfc_uid = self.nfc_reader.wait_for_card() => {
                    self.handle_nfc_scan(nfc_uid?, subsys).await?;
                }
                _ = self.network_status.changed() => {
                    self.handle_network_status_change().await?;
                }
            }
        }
    }

    #[instrument(skip(self, nfc_uid, subsys))]
    async fn handle_nfc_scan(&mut self, nfc_uid: NfcUid, subsys: &SubsystemHandle) -> Result<()> {
        info!("handling nfc scan: {}", hex::encode(nfc_uid.as_bytes()));
        self.led_controller.set_static(Color::Magenta).await?;

        if self.state.config_nfc_uids.contains(&nfc_uid) {
            self.led_controller.set_static(Color::Magenta).await?;

            match self.handle_config_command(nfc_uid, subsys).await {
                Ok(()) => {
                    self.led_controller.set_static(Color::Cyan).await?;
                }
                Err(err) => {
                    error!("error handling config card: {}", err);
                    self.led_controller.set_static(Color::Red).await?;
                }
            }

            sleep(Duration::from_millis(500)).await;
            self.nfc_reader.wait_for_removal().await?;
            return Ok(());
        }

        if !matches!(
            *self.network_status.borrow(),
            ConnectionStatus::Connected { .. }
        ) {
            self.nfc_reader.wait_for_removal().await?;
            return Ok(());
        }

        self.handle_bloop(nfc_uid).await
    }

    #[instrument(skip(self, nfc_uid))]
    async fn handle_bloop(&mut self, nfc_uid: NfcUid) -> Result<()> {
        self.led_controller.set_static(Color::Magenta).await?;

        let (bloop_response, _) = join!(
            self.network_client.bloop(nfc_uid),
            self.audio_player.play_bloop(),
        );

        match bloop_response {
            Ok(achievements) => {
                info!("NFC UID accepted, achievements awarded: {:?}", achievements);

                for achievement in achievements {
                    self.audio_player.play_award().await?;

                    match self
                        .audio_cache
                        .ensure(&self.network_client, &achievement)
                        .await
                    {
                        Ok(Some(path)) => self.audio_player.play_file(path).await?,
                        Ok(None) => continue,
                        Err(error) => {
                            warn!(
                                "failed to load audio for achievement {}: {}",
                                achievement.id, error
                            );
                            continue;
                        }
                    }
                }
            }

            Err(RequestError::Error(ErrorResponse::NfcUidThrottled)) => {
                info!("NFC UID throttled");
                self.audio_player.play_throttled().await?;
            }

            Err(RequestError::Error(ErrorResponse::UnknownNfcUid)) => {
                info!("NFC UID rejected");
                self.audio_player.play_error().await?;
            }

            Err(error) => {
                warn!("bloop failed: {}", error);
                self.audio_player.play_error().await?;
            }
        }

        self.nfc_reader.wait_for_removal().await?;
        Ok(())
    }

    #[instrument(skip(self, nfc_uid, subsys))]
    async fn handle_config_command(
        &mut self,
        nfc_uid: NfcUid,
        subsys: &SubsystemHandle,
    ) -> Result<()> {
        info!("handling config card");
        let mut data = self.nfc_reader.read_ndef_text().await?;

        if data.is_empty() {
            bail!("empty card data");
        }

        info!("card data: {}", data);

        let command = data.chars().next().unwrap();
        data.remove(0);

        match command {
            'w' => {
                let (ssid, password): (String, String) = serde_json::from_str(data.as_str())?;
                set_wifi_credentials(ssid, password).await?;
                info!("wifi credentials set");
            }
            'c' => {
                let (host, port, client_id, client_secret): (String, u16, String, String) =
                    serde_json::from_str(data.as_str())?;

                let connection = ConnectionState {
                    host,
                    port,
                    client_id,
                    client_secret,
                };

                self.network_state.mutate(|state| {
                    state.connection.replace(connection.clone());
                })?;
                self.network_client
                    .configure(Some(connection.into()))
                    .await?;
                info!("connection details set");
            }
            'v' => {
                let range: (f32, f32) = serde_json::from_str(data.as_str())?;
                self.volume_range_tx.send(range).await?;
            }
            'u' => {
                self.nfc_reader.wait_for_removal().await?;
                self.add_config_uid().await?;
            }
            'r' => {
                self.state.mutate(|state| {
                    state.config_nfc_uids.clear();
                    state.config_nfc_uids.insert(nfc_uid);
                })?;
                info!("config cards reset");
            }
            's' => {
                self.network_client.clone().shutdown().await;
                shutdown_system().await?;
                subsys.request_shutdown();
                info!("system shutdown requested");
            }
            command => bail!("unknown command: {}", command),
        }

        Ok(())
    }

    async fn handle_network_status_change(&mut self) -> Result<()> {
        let status = *self.network_status.borrow_and_update();
        info!("network status changed to {status:?}");

        if let ConnectionStatus::Connected { capabilities, .. } = status {
            if capabilities.contains(Capabilities::PreloadCheck) {
                self.led_controller.set_breathing(Color::Cyan).await?;
                self.preload().await?;
            }
        }

        Ok(())
    }

    async fn add_config_uid(&mut self) -> Result<()> {
        self.led_controller.set_breathing(Color::Magenta).await?;
        let nfc_uid = self.nfc_reader.wait_for_card().await?;
        self.nfc_reader.wait_for_removal().await?;

        self.state.mutate(|state| {
            state.config_nfc_uids.insert(nfc_uid);
        })?;

        info!("config card {} added", hex::encode(nfc_uid.as_bytes()));
        Ok(())
    }

    #[instrument(skip(self))]
    async fn preload(&mut self) -> Result<()> {
        info!("starting audio preload");

        let audio_manifest_hash = self.state.audio_manifest_hash.clone();

        let outcome = match self.network_client.preload_check(audio_manifest_hash).await {
            Ok(outcome) => outcome,
            Err(error) => {
                warn!("preload check failed: {}", error);
                return Ok(());
            }
        };

        let PreloadOutcome::Mismatch {
            audio_manifest_hash,
            achievements,
        } = outcome
        else {
            info!("audio update not required");
            return Ok(());
        };

        info!("preloading audio files");

        match self
            .audio_cache
            .sync(&self.network_client, &achievements)
            .await
        {
            Ok(skipped) if skipped.is_empty() => {
                info!("audio preload succeeded");
                self.state
                    .mutate(|state| state.audio_manifest_hash = Some(audio_manifest_hash))?;
            }
            Ok(skipped) => {
                info!("audio preload partial, {} files skipped", skipped.len());
            }
            Err(error) => {
                warn!("audio preload failed: {}", error);
            }
        }

        Ok(())
    }

    async fn set_idle_led(&mut self) -> Result<()> {
        let network_status = *self.network_status.borrow();

        match network_status {
            ConnectionStatus::Connected { .. } => {
                self.led_controller.set_static(Color::Green).await?;
            }
            ConnectionStatus::Unconfigured => {
                self.led_controller.set_breathing(Color::Yellow).await?;
            }
            ConnectionStatus::InvalidCredentials => {
                self.led_controller.set_breathing(Color::Red).await?;
            }
            ConnectionStatus::Shutdown => {
                self.led_controller.set_off().await?;
            }
            _ => {
                self.led_controller.set_breathing(Color::Blue).await?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct EngineState {
    audio_manifest_hash: Option<DataHash>,
    config_nfc_uids: HashSet<NfcUid>,
}

impl IntoSubsystem<Error> for Engine {
    async fn run(mut self, subsys: &mut SubsystemHandle) -> Result<()> {
        if let Ok(result) = self.process(subsys).cancel_on_shutdown(subsys).await {
            result?;
        }

        Ok(())
    }
}
