use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::thread;
use std::thread::sleep;
use std::time::Duration;

use crate::subsystems::config_manager::ConfigCommand;
use anyhow::{anyhow, Error, Result};
use glob::glob;
use log::info;
use rand::seq::SliceRandom;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::oneshot;
use tokio_graceful_shutdown::{IntoSubsystem, SubsystemHandle};

pub struct AudioPlayer {
    share_path: PathBuf,
    cache_path: PathBuf,
    bloop_paths: Vec<PathBuf>,
    confirm_paths: Vec<PathBuf>,
    rx: mpsc::Receiver<PlayerCommand>,
    config: mpsc::Sender<ConfigCommand>,
}

pub type Done = oneshot::Sender<()>;

#[derive(Debug)]
pub enum PlayerCommand {
    PlayBloop { done: Done },
    PlayConfirm { done: Done },
    PlayAsset { path: PathBuf, done: Done },
    PlayCached { path: PathBuf, done: Done },
    SetVolume { volume: f32 },
    GetVolume { responder: oneshot::Sender<f32> },
    SetMaxVolume { volume: f32 },
}

#[derive(Debug)]
enum InternalCommand {
    PlayFile { path: PathBuf, done: Done },
    SetVolume { volume: f32 },
    GetVolume { responder: oneshot::Sender<f32> },
}

struct PlayState {
    _stream: OutputStream,
    _handle: OutputStreamHandle,
    sink: Sink,
    done: Done,
}

struct InternalPlayer {
    rx: mpsc::Receiver<InternalCommand>,
    volume: f32,
    volume_change_path: PathBuf,
}

impl InternalPlayer {
    pub fn new(rx: mpsc::Receiver<InternalCommand>, volume_change_path: PathBuf) -> Result<Self> {
        Ok(Self {
            rx,
            volume: 1.0,
            volume_change_path,
        })
    }

    pub fn run(mut self) -> Result<()> {
        while let Some(command) = self.rx.blocking_recv() {
            let play_state = self.handle_command(command, None)?;

            if let Some(mut play_state) = play_state {
                while !play_state.sink.empty() {
                    sleep(Duration::from_millis(100));
                    let maybe_command = self.rx.try_recv();

                    match maybe_command {
                        Ok(command) => {
                            self.handle_command(command, Some(&mut play_state))?;
                        }
                        Err(TryRecvError::Empty) => {
                            continue;
                        }
                        Err(TryRecvError::Disconnected) => {
                            return Ok(());
                        }
                    }
                }

                let _ = play_state.done.send(());
            }
        }

        Ok(())
    }

    fn handle_command(
        &mut self,
        command: InternalCommand,
        play_state: Option<&mut PlayState>,
    ) -> Result<Option<PlayState>> {
        use InternalCommand::*;

        match command {
            PlayFile { path, done } => {
                if play_state.is_some() {
                    panic!("New playback requested while other sound is already playing");
                }

                let (stream, handle) = OutputStream::try_default()?;
                let sink = Sink::try_new(&handle)?;
                let file = File::open(path)?;
                sink.set_volume(self.volume);
                sink.append(Decoder::new(BufReader::new(file))?);
                return Ok(Some(PlayState {
                    _stream: stream,
                    _handle: handle,
                    sink,
                    done,
                }));
            }
            SetVolume { volume } => {
                if let Some(ref play_state) = play_state {
                    play_state.sink.set_volume(volume);
                }

                self.volume = volume;
                let volume_change_path = self.volume_change_path.to_owned();

                thread::spawn(move || {
                    let (_stream, handle) = OutputStream::try_default().unwrap();
                    let sink = Sink::try_new(&handle).unwrap();
                    let file = File::open(&volume_change_path).unwrap();
                    sink.set_volume(volume);
                    sink.append(Decoder::new(BufReader::new(file)).unwrap());
                    sink.sleep_until_end();
                });
            }
            GetVolume { responder } => {
                let _ = responder.send(self.volume);
            }
        }

        Ok(None)
    }
}

impl AudioPlayer {
    pub fn new(
        share_path: PathBuf,
        cache_path: PathBuf,
        config: mpsc::Sender<ConfigCommand>,
        rx: mpsc::Receiver<PlayerCommand>,
    ) -> Self {
        Self {
            share_path: share_path.clone(),
            cache_path,
            bloop_paths: Self::collect_paths(&share_path, "bloop").expect(""),
            confirm_paths: Self::collect_paths(&share_path, "confirm").expect(""),
            rx,
            config,
        }
    }

    async fn process(&mut self) -> Result<()> {
        let (internal_tx, internal_rx) = mpsc::channel(8);
        let share_path = self.share_path.to_owned();

        thread::spawn(move || {
            let internal_player =
                InternalPlayer::new(internal_rx, share_path.join(Path::new("volume-change.mp3")))
                    .unwrap();
            internal_player.run().unwrap();
        });

        let (config_tx, config_rx) = oneshot::channel();
        self.config
            .send(ConfigCommand::GetVolume {
                responder: config_tx,
            })
            .await?;
        let mut volume_config = config_rx.await?;

        internal_tx
            .send(InternalCommand::SetVolume {
                volume: volume_config.current,
            })
            .await?;

        while let Some(play_command) = self.rx.recv().await {
            use PlayerCommand::*;

            match play_command {
                PlayBloop { done } => {
                    let path = self
                        .bloop_paths
                        .choose(&mut rand::thread_rng())
                        .ok_or_else(|| anyhow!("No boop files available"))?
                        .clone();
                    internal_tx
                        .send(InternalCommand::PlayFile {
                            path: self.share_path.join(path),
                            done,
                        })
                        .await?;
                }
                PlayConfirm { done } => {
                    let path = self
                        .confirm_paths
                        .choose(&mut rand::thread_rng())
                        .ok_or_else(|| anyhow!("No confirm files available"))?
                        .clone();
                    internal_tx
                        .send(InternalCommand::PlayFile {
                            path: self.share_path.join(path),
                            done,
                        })
                        .await?;
                }
                PlayAsset { path, done } => {
                    internal_tx
                        .send(InternalCommand::PlayFile {
                            path: self.share_path.join(path),
                            done,
                        })
                        .await?;
                }
                PlayCached { path, done } => {
                    internal_tx
                        .send(InternalCommand::PlayFile {
                            path: self.cache_path.join(path),
                            done,
                        })
                        .await?;
                }
                SetVolume { volume } => {
                    volume_config.current = volume.clamp(0., volume_config.max);
                    internal_tx
                        .send(InternalCommand::SetVolume {
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
                    internal_tx
                        .send(InternalCommand::GetVolume { responder })
                        .await?;
                }
                SetMaxVolume { volume } => {
                    volume_config.max = volume.clamp(0., 1.);
                    volume_config.current = volume_config.max;
                    internal_tx
                        .send(InternalCommand::SetVolume {
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

    fn collect_paths(share_path: &Path, dir_name: &str) -> Result<Vec<PathBuf>> {
        let mut paths: Vec<PathBuf> = Vec::new();

        for entry in
            glob(format!("{}/{}/*.mp3", share_path.to_str().unwrap(), dir_name).as_str()).unwrap()
        {
            paths.push(entry.unwrap().as_path().try_into()?);
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
