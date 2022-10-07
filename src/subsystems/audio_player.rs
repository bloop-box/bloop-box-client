use std::ffi::OsStr;
use std::path::PathBuf;
use std::thread;
use std::thread::sleep;
use std::time::Duration;

use crate::subsystems::config_manager::ConfigCommand;
use anyhow::{anyhow, Error, Result};
use include_dir::{include_dir, Dir};
use log::info;
use rand::seq::SliceRandom;
use soloud::*;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::oneshot;
use tokio_graceful_shutdown::{IntoSubsystem, SubsystemHandle};

pub struct AudioPlayer {
    cache_path: PathBuf,
    boop_paths: Vec<PathBuf>,
    confirm_paths: Vec<PathBuf>,
    rx: mpsc::Receiver<PlayerCommand>,
    config: mpsc::Sender<ConfigCommand>,
}

const ASSETS_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/assets");

pub type Done = oneshot::Sender<()>;

#[derive(Debug)]
pub enum PlayerCommand {
    PlayBoop { done: Done },
    PlayConfirm { done: Done },
    PlayAsset { path: PathBuf, done: Done },
    PlayCached { path: PathBuf, done: Done },
    SetVolume { volume: f32 },
    GetVolume { responder: oneshot::Sender<f32> },
    SetMaxVolume { volume: f32 },
}

#[derive(Debug)]
enum SoloudCommand {
    PlayAsset { path: PathBuf, done: Done },
    PlayFile { path: PathBuf, done: Done },
    SetVolume { volume: f32 },
    GetVolume { responder: oneshot::Sender<f32> },
}

impl AudioPlayer {
    pub fn new(
        cache_path: PathBuf,
        config: mpsc::Sender<ConfigCommand>,
        rx: mpsc::Receiver<PlayerCommand>,
    ) -> Self {
        Self {
            cache_path,
            boop_paths: Self::collect_paths("boop").expect(""),
            confirm_paths: Self::collect_paths("confirm").expect(""),
            rx,
            config,
        }
    }

    async fn process(&mut self) -> Result<()> {
        let (soloud_tx, mut soloud_rx) = mpsc::channel(8);

        thread::spawn(move || {
            struct PlayState {
                handle: Handle,
                done: Done,
            }

            let mut soloud = Soloud::default().unwrap();
            let mut play_wav = audio::Wav::default();
            let mut volume_change_wav = audio::Wav::default();
            volume_change_wav
                .load_mem(
                    ASSETS_DIR
                        .get_file(PathBuf::from("volume-change.mp3"))
                        .unwrap()
                        .contents(),
                )
                .unwrap();

            let mut handle_command = |soloud: &mut Soloud, command| {
                use SoloudCommand::*;

                match command {
                    PlayAsset { path, done } => {
                        let file = ASSETS_DIR
                            .get_file(&path)
                            .ok_or_else(|| anyhow!("Asset file missing: {}", path.display()))
                            .unwrap();
                        play_wav.load_mem(file.contents()).unwrap();
                        return Some(PlayState {
                            handle: soloud.play(&play_wav),
                            done,
                        });
                    }
                    PlayFile { path, done } => {
                        play_wav.load(path).unwrap();
                        return Some(PlayState {
                            handle: soloud.play(&play_wav),
                            done,
                        });
                    }
                    SetVolume { volume } => {
                        soloud.set_global_volume(volume);
                        soloud.play(&volume_change_wav);
                    }
                    GetVolume { responder } => {
                        let _ = responder.send(soloud.global_volume());
                    }
                }

                None
            };

            while let Some(command) = soloud_rx.blocking_recv() {
                let play_state = handle_command(&mut soloud, command);

                if let Some(current_play_state) = play_state {
                    while soloud.is_valid_voice_handle(current_play_state.handle) {
                        sleep(Duration::from_millis(100));
                        let maybe_command = soloud_rx.try_recv();

                        match maybe_command {
                            Ok(command) => {
                                let maybe_play_state = handle_command(&mut soloud, command);

                                if maybe_play_state.is_some() {
                                    panic!("New playback requested while other sound is already playing");
                                }
                            }
                            Err(TryRecvError::Empty) => {
                                continue;
                            }
                            Err(TryRecvError::Disconnected) => {
                                return;
                            }
                        }
                    }

                    current_play_state.done.send(()).unwrap();
                }
            }
        });

        let (config_tx, config_rx) = oneshot::channel();
        self.config
            .send(ConfigCommand::GetVolume {
                responder: config_tx,
            })
            .await?;
        let mut volume_config = config_rx.await?;

        soloud_tx
            .send(SoloudCommand::SetVolume {
                volume: volume_config.current,
            })
            .await?;

        while let Some(play_command) = self.rx.recv().await {
            use PlayerCommand::*;

            match play_command {
                PlayBoop { done } => {
                    let path = self
                        .boop_paths
                        .choose(&mut rand::thread_rng())
                        .ok_or_else(|| anyhow!("No boop files available"))?
                        .clone();
                    soloud_tx
                        .send(SoloudCommand::PlayAsset { path, done })
                        .await?;
                }
                PlayConfirm { done } => {
                    let path = self
                        .confirm_paths
                        .choose(&mut rand::thread_rng())
                        .ok_or_else(|| anyhow!("No confirm files available"))?
                        .clone();
                    soloud_tx
                        .send(SoloudCommand::PlayAsset { path, done })
                        .await?;
                }
                PlayAsset { path, done } => {
                    soloud_tx
                        .send(SoloudCommand::PlayAsset { path, done })
                        .await?;
                }
                PlayCached { path, done } => {
                    soloud_tx
                        .send(SoloudCommand::PlayFile {
                            path: self.cache_path.join(path),
                            done,
                        })
                        .await?;
                }
                SetVolume { volume } => {
                    volume_config.current = volume.clamp(0., volume_config.max);
                    soloud_tx
                        .send(SoloudCommand::SetVolume {
                            volume: volume_config.current,
                        })
                        .await?;

                    let (config_tx, config_rx) = oneshot::channel();
                    self.config
                        .send(ConfigCommand::SetVolume {
                            volume_config: volume_config.clone(),
                            responder: config_tx,
                        })
                        .await?;
                    config_rx.await?;
                }
                GetVolume { responder } => {
                    soloud_tx
                        .send(SoloudCommand::GetVolume { responder })
                        .await?;
                }
                SetMaxVolume { volume } => {
                    volume_config.max = volume.clamp(0., 1.);
                    volume_config.current = volume_config.max;
                    soloud_tx
                        .send(SoloudCommand::SetVolume {
                            volume: volume_config.current,
                        })
                        .await?;

                    let (config_tx, config_rx) = oneshot::channel();
                    self.config
                        .send(ConfigCommand::SetVolume {
                            volume_config: volume_config.clone(),
                            responder: config_tx,
                        })
                        .await?;
                    config_rx.await?;
                }
            }
        }

        Ok(())
    }

    fn collect_paths(dir_name: &str) -> Result<Vec<PathBuf>> {
        let mut paths: Vec<PathBuf> = Vec::new();

        for file in ASSETS_DIR
            .get_dir(dir_name)
            .ok_or_else(|| anyhow!("Directory missing: {}", dir_name))?
            .files()
        {
            let path = file.path();

            if path.extension() != Some(OsStr::new("mp3")) {
                continue;
            }

            paths.push(path.try_into()?);
        }

        Ok(paths)
    }
}

#[async_trait::async_trait]
impl IntoSubsystem<Error> for AudioPlayer {
    async fn run(mut self, subsys: SubsystemHandle) -> Result<()> {
        tokio::select! {
            _ = subsys.on_shutdown_requested() => {
                info!("Audio player shutting down");
            },
            res = self.process() => res?
        }

        Ok(())
    }
}
