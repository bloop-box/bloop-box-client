use crate::etc_config::EtcConfig;
use anyhow::{anyhow, Error, Result};
use log::info;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tokio::fs::metadata;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::sleep;
use tokio_graceful_shutdown::{IntoSubsystem, SubsystemHandle};

use crate::nfc::reader::Uid;
use crate::nfc::thread::{start_nfc_listener, NfcCommand};
use crate::subsystems::audio_player::PlayerCommand;
use crate::subsystems::config_manager::{ConfigCommand, ConnectionConfig};
use crate::subsystems::led::{LedState, BLUE, CYAN, GREEN, MAGENTA, RED, YELLOW};
use crate::subsystems::networker::{CheckUidResponse, NetworkerCommand, NetworkerStatus};
use crate::wifi::wpa_supplicant::set_wifi;

pub struct Controller {
    etc_config: EtcConfig,
    cache_path: PathBuf,
    audio_player: mpsc::Sender<PlayerCommand>,
    led: mpsc::Sender<LedState>,
    config: mpsc::Sender<ConfigCommand>,
    networker: mpsc::Sender<NetworkerCommand>,
    networker_status_rx: mpsc::Receiver<NetworkerStatus>,
    networker_status: NetworkerStatus,
}

impl Controller {
    pub fn new(
        etc_config: EtcConfig,
        cache_path: PathBuf,
        audio_player: mpsc::Sender<PlayerCommand>,
        led: mpsc::Sender<LedState>,
        config: mpsc::Sender<ConfigCommand>,
        networker: mpsc::Sender<NetworkerCommand>,
        networker_status_rx: mpsc::Receiver<NetworkerStatus>,
    ) -> Self {
        Self {
            etc_config,
            cache_path,
            audio_player,
            led,
            config,
            networker,
            networker_status_rx,
            networker_status: NetworkerStatus::Disconnected,
        }
    }

    async fn process(&mut self, nfc: mpsc::Sender<NfcCommand>) -> Result<()> {
        let (config_tx, config_rx) = oneshot::channel();
        self.config
            .send(ConfigCommand::GetConfigUids {
                responder: config_tx,
            })
            .await?;
        let mut config_uids = config_rx.await?;

        if config_uids.is_empty() {
            self.add_config_uid(&mut config_uids, nfc.clone()).await?;
            self.wait_for_release(&nfc).await?;
        }

        loop {
            self.set_idle_led().await?;

            let (uid_tx, uid_rx) = oneshot::channel();
            let (_cancel_tx, cancel_rx) = oneshot::channel::<()>();
            nfc.send(NfcCommand::Poll {
                responder: uid_tx,
                cancel_rx,
            })
            .await?;

            tokio::select! {
                result = uid_rx => {
                    let uid = result?;

                    if config_uids.contains(&uid) {
                        self.led.send(LedState::On { color: YELLOW }).await?;

                        if self.process_config_command(uid, &mut config_uids, nfc.clone()).await.is_ok() {
                            self.led.send(LedState::On { color: CYAN }).await?;
                        } else {
                            self.led.send(LedState::On { color: RED }).await?;
                        }

                        sleep(Duration::from_millis(500)).await;
                        self.wait_for_release(&nfc).await?;
                        continue;
                    }

                    if self.networker_status != NetworkerStatus::Connected {
                        self.wait_for_release(&nfc).await?;
                        continue;
                    }

                    self.led.send(LedState::On { color: YELLOW }).await?;

                    let (done_tx, done_rx) = oneshot::channel();
                    self.audio_player.send(PlayerCommand::PlayBloop { done: done_tx }).await?;
                    done_rx.await?;

                    let (response_tx, response_rx) = oneshot::channel();
                    self.networker.send(NetworkerCommand::CheckUid { uid, responder: response_tx }).await?;
                    let check_uid_response = response_rx.await?;

                    match check_uid_response {
                        CheckUidResponse::Ok {achievements} => {
                            for achievement_id in achievements.iter() {
                                let (done_tx, done_rx) = oneshot::channel();
                                self.audio_player.send(PlayerCommand::PlayConfirm { done: done_tx }).await?;
                                done_rx.await?;

                                let filename = format!("{}.mp3", hex::encode(achievement_id));
                                let path = self.cache_path.join(&filename);

                                if metadata(&path).await.is_err() {
                                    let (response_tx, response_rx) = oneshot::channel();
                                    self.networker.send(NetworkerCommand::GetAudio {
                                        id: *achievement_id,
                                        responder: response_tx,
                                    }).await?;
                                    let maybe_data = response_rx.await?;

                                    match maybe_data {
                                        Some(data) => {
                                            let mut file = File::create(&path).await?;
                                            file.write_all(&data).await?;
                                        }
                                        None => continue,
                                    }
                                }

                                let (done_tx, done_rx) = oneshot::channel();
                                self.audio_player.send(PlayerCommand::PlayCached {
                                    path: PathBuf::from(filename),
                                    done: done_tx,
                                }).await?;
                                done_rx.await?;
                            }
                        },
                        CheckUidResponse::Error {} => {
                            let (done_tx, done_rx) = oneshot::channel();
                            self.audio_player.send(PlayerCommand::PlayAsset {
                                path: PathBuf::from("error.mp3"),
                                done: done_tx,
                            }).await?;
                            done_rx.await?;
                        },
                        CheckUidResponse::Throttle {} => {
                            let (done_tx, done_rx) = oneshot::channel();
                            self.audio_player.send(PlayerCommand::PlayAsset {
                                path: PathBuf::from("throttle.mp3"),
                                done: done_tx,
                            }).await?;
                            done_rx.await?;
                        },
                    }

                    self.wait_for_release(&nfc).await?;
                },
                maybe_networker_status = self.networker_status_rx.recv() => {
                    if let Some(networker_status) = maybe_networker_status {
                        self.networker_status = networker_status;
                    }
                },
            }
        }
    }

    async fn set_idle_led(&mut self) -> Result<()> {
        match self.networker_status {
            NetworkerStatus::Connected => {
                self.led.send(LedState::On { color: GREEN }).await?;
            }
            NetworkerStatus::Disconnected => {
                self.led.send(LedState::Blink { color: BLUE }).await?;
            }
            NetworkerStatus::NoConfig => {
                self.led.send(LedState::Blink { color: YELLOW }).await?;
            }
            NetworkerStatus::InvalidCredentials => {
                self.led.send(LedState::Blink { color: RED }).await?;
            }
        }

        Ok(())
    }

    async fn process_config_command(
        &mut self,
        uid: Uid,
        config_uids: &mut Vec<Uid>,
        nfc: mpsc::Sender<NfcCommand>,
    ) -> Result<()> {
        let (value_tx, value_rx) = oneshot::channel();
        nfc.send(NfcCommand::Read {
            responder: value_tx,
        })
        .await?;
        let maybe_value = value_rx.await?;

        let mut value = match maybe_value {
            Some(value) => value,
            None => {
                return Err(anyhow!("Unable to read value"));
            }
        };

        if value.is_empty() {
            return Err(anyhow!("Value too short"));
        }

        let command = value.chars().next().unwrap();
        value.remove(0);

        match command {
            'w' => {
                let (ssid, password): (String, String) = serde_json::from_str(value.as_str())?;
                set_wifi(ssid, password)?;
            }
            'c' => {
                let (host, port, user, secret): (String, u16, String, String) =
                    serde_json::from_str(value.as_str())?;

                self.networker
                    .send(NetworkerCommand::SetConnection {
                        connection_config: ConnectionConfig {
                            host,
                            port,
                            user,
                            secret,
                        },
                    })
                    .await?;
            }
            'v' => {
                let (volume,): (f32,) = serde_json::from_str(value.as_str())?;
                self.audio_player
                    .send(PlayerCommand::SetMaxVolume { volume })
                    .await?;
            }
            'u' => {
                self.wait_for_release(&nfc).await?;
                self.add_config_uid(config_uids, nfc.clone()).await?;
            }
            'r' => {
                config_uids.clear();
                config_uids.push(uid);

                let (config_tx, config_rx) = oneshot::channel();
                self.config
                    .send(ConfigCommand::SetConfigUids {
                        responder: config_tx,
                        config_uids: config_uids.clone(),
                    })
                    .await?;
                config_rx.await?;
            }
            's' => {
                Command::new("sudo")
                    .args(["shutdown", "now"])
                    .output()
                    .expect("Failed to shut down system");
            }
            _ => {
                return Err(anyhow!("Value too short"));
            }
        }

        Ok(())
    }

    async fn wait_for_release(&self, nfc: &mpsc::Sender<NfcCommand>) -> Result<()> {
        let (released_tx, released_rx) = oneshot::channel();
        let (_cancel_tx, cancel_rx) = oneshot::channel::<()>();
        nfc.send(NfcCommand::Release {
            responder: released_tx,
            cancel_rx,
        })
        .await?;
        released_rx.await?;

        Ok(())
    }

    async fn add_config_uid(
        &mut self,
        config_uids: &mut Vec<Uid>,
        nfc: mpsc::Sender<NfcCommand>,
    ) -> Result<()> {
        self.led.send(LedState::Blink { color: MAGENTA }).await?;

        let (uid_tx, uid_rx) = oneshot::channel();
        let (_cancel_tx, cancel_rx) = oneshot::channel::<()>();
        nfc.send(NfcCommand::Poll {
            responder: uid_tx,
            cancel_rx,
        })
        .await?;
        let uid = uid_rx.await?;

        if !config_uids.contains(&uid) {
            config_uids.push(uid);
        }

        let (config_tx, config_rx) = oneshot::channel();
        self.config
            .send(ConfigCommand::SetConfigUids {
                responder: config_tx,
                config_uids: config_uids.clone(),
            })
            .await?;
        config_rx.await?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl IntoSubsystem<Error> for Controller {
    async fn run(mut self, subsys: SubsystemHandle) -> Result<()> {
        let (nfc_tx, nfc_rx) = mpsc::channel(1);
        start_nfc_listener(nfc_rx, self.etc_config.nfc.clone());

        tokio::select! {
            _ = subsys.on_shutdown_requested() => {
                info!("Controller shutting down");
            },
            res = self.process(nfc_tx) => res?,
        }

        Ok(())
    }
}
