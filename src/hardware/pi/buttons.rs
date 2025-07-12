use crate::hardware::buttons::Button;
use anyhow::{Context, Error, Result};
use gpiocdev::line::EdgeDetection;
use gpiocdev::tokio::AsyncRequest;
use gpiocdev::Request;
use serde::Deserialize;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_graceful_shutdown::{FutureExt, IntoSubsystem, SubsystemHandle};
use tracing::warn;

#[derive(Debug, Deserialize)]
pub struct ButtonsConfig {
    #[serde(default = "ButtonsConfig::default_gpio_dev_path")]
    gpio_dev_path: PathBuf,
    #[serde(default = "ButtonsConfig::default_volume_up_line")]
    volume_up_line: u32,
    #[serde(default = "ButtonsConfig::default_volume_down_line")]
    volume_down_line: u32,
}

impl Default for ButtonsConfig {
    fn default() -> Self {
        Self {
            gpio_dev_path: Self::default_gpio_dev_path(),
            volume_up_line: Self::default_volume_up_line(),
            volume_down_line: Self::default_volume_down_line(),
        }
    }
}

impl ButtonsConfig {
    fn default_gpio_dev_path() -> PathBuf {
        "/dev/gpiochip0".into()
    }

    fn default_volume_up_line() -> u32 {
        23
    }

    fn default_volume_down_line() -> u32 {
        24
    }
}

pub struct Buttons {
    tx: mpsc::Sender<Button>,
    request: AsyncRequest,
    config: ButtonsConfig,
}

impl Buttons {
    pub fn new(tx: mpsc::Sender<Button>, config: ButtonsConfig) -> Result<Buttons> {
        let request = AsyncRequest::new(
            Request::builder()
                .on_chip(config.gpio_dev_path.clone())
                .with_consumer("bloop-box")
                .with_lines(&[config.volume_up_line, config.volume_down_line])
                .with_edge_detection(EdgeDetection::FallingEdge)
                .with_debounce_period(Duration::from_millis(50))
                .request()
                .context("Failed to create GPIO request")?,
        );

        Ok(Self {
            tx,
            request,
            config,
        })
    }

    async fn listen(&mut self) -> Result<()> {
        loop {
            let event = self.request.read_edge_event().await?;
            let button = match event.offset {
                offset if offset == self.config.volume_up_line => Button::VolumeUp,
                offset if offset == self.config.volume_down_line => Button::VolumeDown,
                offset => {
                    warn!("Unexpected GPIO line: {offset}");
                    continue;
                }
            };

            let _ = self.tx.send(button).await;
        }
    }
}

#[async_trait::async_trait]
impl IntoSubsystem<Error> for Buttons {
    async fn run(mut self, subsys: SubsystemHandle) -> Result<()> {
        if let Ok(result) = self.listen().cancel_on_shutdown(&subsys).await {
            result?;
        }

        Ok(())
    }
}
