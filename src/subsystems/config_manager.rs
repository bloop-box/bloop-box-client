use std::path::{Path, PathBuf};

use crate::nfc::reader::Uid;
use anyhow::{Error, Result};
use log::info;
use serde::{Deserialize, Serialize};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_graceful_shutdown::{IntoSubsystem, SubsystemHandle};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConnectionConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub secret: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VolumeConfig {
    pub max: f32,
    pub current: f32,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Config {
    pub config_uids: Vec<Uid>,
    pub connection: Option<ConnectionConfig>,
    pub volume: VolumeConfig,
}

#[derive(Debug)]
pub enum ConfigCommand {
    GetConfigUids {
        responder: oneshot::Sender<Vec<Uid>>,
    },
    SetConfigUids {
        config_uids: Vec<Uid>,
        responder: oneshot::Sender<()>,
    },
    GetVolume {
        responder: oneshot::Sender<VolumeConfig>,
    },
    SetVolume {
        volume_config: VolumeConfig,
        responder: oneshot::Sender<()>,
    },
    GetConnection {
        responder: oneshot::Sender<Option<ConnectionConfig>>,
    },
    SetConnection {
        connection_config: ConnectionConfig,
        responder: oneshot::Sender<()>,
    },
}

pub struct ConfigManager {
    config_path: PathBuf,
    rx: mpsc::Receiver<ConfigCommand>,
}

impl ConfigManager {
    pub fn new(local_dir: &Path, rx: mpsc::Receiver<ConfigCommand>) -> Self {
        Self {
            config_path: local_dir.join("config.toml"),
            rx,
        }
    }

    async fn process(&mut self) -> Result<()> {
        let mut config = match File::open(&self.config_path).await {
            Ok(mut file) => {
                let mut toml_config = String::new();
                file.read_to_string(&mut toml_config).await?;
                let config: Config = toml::from_str(&toml_config)?;
                config
            }
            Err(_) => Config {
                config_uids: vec![],
                connection: None,
                volume: VolumeConfig {
                    max: 1.0,
                    current: 1.0,
                },
            },
        };

        while let Some(command) = self.rx.recv().await {
            use ConfigCommand::*;

            match command {
                GetConfigUids { responder } => {
                    responder.send(config.config_uids.clone()).unwrap();
                }
                SetConfigUids {
                    config_uids,
                    responder,
                } => {
                    config.config_uids = config_uids;
                    self.store_config(&config).await?;
                    responder.send(()).unwrap();
                }
                GetVolume { responder } => {
                    responder.send(config.volume.clone()).unwrap();
                }
                SetVolume {
                    volume_config,
                    responder,
                } => {
                    config.volume = volume_config;
                    self.store_config(&config).await?;
                    responder.send(()).unwrap();
                }
                GetConnection { responder } => {
                    responder.send(config.connection.clone()).unwrap();
                }
                SetConnection {
                    connection_config,
                    responder,
                } => {
                    config.connection = Some(connection_config);
                    self.store_config(&config).await?;
                    responder.send(()).unwrap();
                }
            }
        }

        Ok(())
    }

    async fn store_config(&self, config: &Config) -> Result<()> {
        let toml_config = toml::to_string(&config)?;
        let mut file = File::create(&self.config_path).await?;
        file.write_all(toml_config.as_bytes()).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl IntoSubsystem<Error> for ConfigManager {
    async fn run(mut self, subsys: SubsystemHandle) -> Result<()> {
        tokio::select! {
            _ = subsys.on_shutdown_requested() => {
                info!("Config Manager shutting down");
            },
            res = self.process() => res?,
        }

        Ok(())
    }
}
