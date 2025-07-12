use crate::audio::AudioPlayer;
use crate::hardware::data_path;
use crate::hardware::led::{Color, LedController};
use crate::hardware::nfc::{NfcReader, NfcUid};
use crate::hardware::system::{set_wifi_credentials, shutdown_system};
use crate::network::task::{ConnectionState, NetworkStatus};
use crate::network::{
    AudioResponse, BloopResponse, Capabilities, DataHash, NetworkClient, PreloadCheckResponse,
};
use crate::state::PersistedState;
use anyhow::{anyhow, bail, Context, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs::metadata;
use tokio::sync::{mpsc, watch};
use tokio::time::sleep;
use tokio::{fs, join, select};
use tokio_graceful_shutdown::{FutureExt, IntoSubsystem, SubsystemHandle};
use tracing::{debug, error, info, instrument, warn};

pub struct EngineProps {
    pub led_controller: LedController,
    pub nfc_reader: NfcReader,
    pub network_client: NetworkClient,
    pub audio_player: AudioPlayer,
    pub network_status: watch::Receiver<NetworkStatus>,
    pub volume_range_tx: mpsc::Sender<(f32, f32)>,
}

pub struct Engine {
    led_controller: LedController,
    nfc_reader: NfcReader,
    network_client: NetworkClient,
    audio_player: AudioPlayer,
    network_status: watch::Receiver<NetworkStatus>,
    cache_path: PathBuf,
    volume_range_tx: mpsc::Sender<(f32, f32)>,
    state: PersistedState<EngineState>,
}

impl Engine {
    pub async fn new(props: EngineProps) -> Result<Self> {
        let state = PersistedState::new("engine", None).await?;
        let cache_path = data_path().await?.join("cache");
        fs::create_dir_all(&cache_path).await.with_context(|| {
            format!("failed to create cache directory: {}", cache_path.display())
        })?;

        Ok(Self {
            led_controller: props.led_controller,
            nfc_reader: props.nfc_reader,
            network_client: props.network_client,
            audio_player: props.audio_player,
            network_status: props.network_status,
            volume_range_tx: props.volume_range_tx,
            cache_path,
            state,
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
        info!("handling nfc scan: {}", nfc_uid);
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
            NetworkStatus::Connected { .. }
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

        match bloop_response? {
            BloopResponse::Accepted { achievements } => {
                info!("NFC UID accepted, achievements awarded: {:?}", achievements);

                for achievement in achievements {
                    self.audio_player.play_award().await?;

                    if achievement.audio_file_hash.is_none() {
                        continue;
                    }

                    let filename = achievement.filename()?;
                    let path = self.cache_path.join(&filename);

                    if metadata(&path).await.is_err() {
                        let response = self.network_client.retrieve_audio(achievement.id).await?;

                        match response {
                            AudioResponse::Data(data) => fs::write(&path, data).await?,
                            AudioResponse::NotFound => {
                                warn!("audio file not found for achievement {}", achievement.id);
                                continue;
                            }
                            AudioResponse::Disconnected => {
                                warn!("disconnected while loading achievement: {}", achievement.id);
                                continue;
                            }
                        }
                    }

                    self.audio_player.play_file(path).await?;
                }
            }

            BloopResponse::Throttled => {
                info!("NFC UID throttled");
                self.audio_player.play_throttled().await?;
            }

            BloopResponse::Rejected => {
                info!("NFC UID rejected");
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
        let mut data = self
            .nfc_reader
            .read_card_data()
            .await?
            .ok_or(anyhow!("card is empty"))?;

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

                self.network_client
                    .set_connection_state(ConnectionState {
                        host,
                        port,
                        client_id,
                        client_secret,
                    })
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
                self.network_client.shutdown().await?;
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

        if let NetworkStatus::Connected { capabilities } = status {
            if capabilities.contains(Capabilities::PreloadCheck) {
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

        info!("config card {} added", nfc_uid);
        Ok(())
    }

    #[instrument(skip(self))]
    async fn preload(&mut self) -> Result<()> {
        info!("starting audio preload");

        let audio_manifest_hash = self.state.audio_manifest_hash.clone();
        let response = self
            .network_client
            .preload_check(audio_manifest_hash)
            .await?;

        let PreloadCheckResponse::Mismatch {
            audio_manifest_hash,
            achievements,
        } = response
        else {
            info!("audio update not required");
            return Ok(());
        };

        info!("preloading audio files");
        let mut succeeded = true;

        for achievement in achievements {
            if achievement.audio_file_hash.is_none() {
                debug!(
                    "audio file hash not present for achievement {}",
                    achievement.id
                );
                continue;
            }

            let path = self.cache_path.join(achievement.filename()?);

            if metadata(&path).await.is_ok() {
                debug!("audio file {} already exists", path.display());
                continue;
            }

            let response = self.network_client.retrieve_audio(achievement.id).await?;

            match response {
                AudioResponse::Data(data) => {
                    fs::write(path, data).await?;
                    info!("audio for achievement {} downloaded", achievement.id);
                }
                AudioResponse::NotFound => {
                    info!("audio for achievement not found: {}", achievement.id);
                    succeeded = false;
                }
                AudioResponse::Disconnected => {
                    warn!("disconnected while loading achievement: {}", achievement.id);
                    succeeded = false;
                }
            }
        }

        if succeeded {
            info!("audio preload succeeded");
            self.state
                .mutate(|state| state.audio_manifest_hash = Some(audio_manifest_hash))?;
        }

        Ok(())
    }

    async fn set_idle_led(&mut self) -> Result<()> {
        let network_status = *self.network_status.borrow();

        match network_status {
            NetworkStatus::Connected { .. } => {
                self.led_controller.set_static(Color::Green).await?;
            }
            NetworkStatus::Disconnected => {
                self.led_controller.set_breathing(Color::Blue).await?;
            }
            NetworkStatus::Unconfigured => {
                self.led_controller.set_breathing(Color::Yellow).await?;
            }
            NetworkStatus::InvalidCredentials => {
                self.led_controller.set_breathing(Color::Red).await?;
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

#[async_trait::async_trait]
impl IntoSubsystem<Error> for Engine {
    async fn run(mut self, subsys: SubsystemHandle) -> Result<()> {
        if let Ok(result) = self.process(&subsys).cancel_on_shutdown(&subsys).await {
            result?;
        }

        Ok(())
    }
}
