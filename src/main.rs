extern crate core;

use std::future::Future;
use std::path::Path;

use crate::etc_config::load_etc_config;
use anyhow::Result;
use clap::Parser;
use env_logger::{Builder, Env};
use log::warn;
use tokio::fs;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio::time::Duration;
use tokio_graceful_shutdown::{IntoSubsystem, Toplevel};

use crate::subsystems::audio_player::AudioPlayer;
use crate::subsystems::config_manager::ConfigManager;
use crate::subsystems::controller::Controller;
use crate::subsystems::led::Led;
use crate::subsystems::networker::Networker;
use crate::subsystems::volume_control::VolumeControl;

mod etc_config;
mod nfc;
mod subsystems;
mod utils;
mod wifi;

#[derive(Parser)]
struct Args {
    /// DANGEROUS: Disable server cert verification
    #[arg(long, default_value_t = false)]
    dangerous_disable_cert_verification: bool,
}

struct RuntimeWithInstantShutdown(Option<Runtime>);

impl RuntimeWithInstantShutdown {
    pub fn new() -> Self {
        Self(Some(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
        ))
    }

    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        self.0.as_ref().unwrap().block_on(future)
    }
}

impl Drop for RuntimeWithInstantShutdown {
    fn drop(&mut self) {
        self.0.take().unwrap().shutdown_background()
    }
}

fn main() -> Result<()> {
    Builder::from_env(Env::default().default_filter_or("debug")).init();

    let args = Args::parse();

    if args.dangerous_disable_cert_verification {
        warn!("Server certificate verification has been disabled. Make sure to not use this flag in production!");
    }

    RuntimeWithInstantShutdown::new().block_on(async {
        let etc_config = load_etc_config().await?;
        let share_dir = Path::new("/usr/share/bloop-box");
        let data_dir = Path::new("/var/lib/bloop-box");
        let cache_dir = Path::new(&data_dir).join("cache");

        if !cache_dir.is_dir() {
            fs::create_dir(&cache_dir).await?;
        }

        let (audio_player_tx, audio_player_rx) = mpsc::channel(8);
        let (led_tx, led_rx) = mpsc::channel(8);
        let (config_tx, config_rx) = mpsc::channel(8);
        let (networker_tx, networker_rx) = mpsc::channel(8);
        let (networker_status_tx, networker_status_rx) = mpsc::channel(8);

        Toplevel::new()
            .start("Led", Led::new(led_rx).into_subsystem())
            .start(
                "ConfigManager",
                ConfigManager::new(data_dir, config_rx).into_subsystem(),
            )
            .start(
                "AudioPlayer",
                AudioPlayer::new(
                    share_dir.to_path_buf(),
                    cache_dir.clone(),
                    config_tx.clone(),
                    audio_player_rx,
                )
                .into_subsystem(),
            )
            .start(
                "VolumeControl",
                VolumeControl::new(etc_config.clone(), audio_player_tx.clone()).into_subsystem(),
            )
            .start(
                "Networker",
                Networker::new(
                    networker_rx,
                    networker_status_tx,
                    config_tx.clone(),
                    args.dangerous_disable_cert_verification,
                )
                .into_subsystem(),
            )
            .start(
                "Controller",
                Controller::new(
                    etc_config,
                    cache_dir,
                    audio_player_tx,
                    led_tx,
                    config_tx,
                    networker_tx,
                    networker_status_rx,
                )
                .into_subsystem(),
            )
            .catch_signals()
            .handle_shutdown_requests(Duration::from_millis(1000))
            .await
    })
}
