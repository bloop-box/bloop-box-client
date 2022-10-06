use anyhow::{Error, Result};
use log::info;
use rppal::gpio::{InputPin, Level, Pin};
use tokio::sync::mpsc;
use tokio::time::{self, Duration};
use tokio_graceful_shutdown::{IntoSubsystem, SubsystemHandle};

pub struct Button {
    pin: InputPin,
    tx: mpsc::Sender<f32>,
    value: f32,
}

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const BUTTON_DEBOUNCE: Duration = Duration::from_millis(50);

impl Button {
    pub fn new(pin: Pin, tx: mpsc::Sender<f32>, value: f32) -> Self {
        Button {
            pin: pin.into_input(),
            tx,
            value,
        }
    }

    async fn process(&mut self) -> Result<()> {
        let mut previous_level = Level::High;

        loop {
            time::sleep(POLL_INTERVAL).await;

            let level = self.pin.read();

            if level == previous_level {
                continue;
            }

            time::sleep(BUTTON_DEBOUNCE).await;

            if self.pin.read() != level {
                continue;
            }

            previous_level = level;

            if level == Level::High {
                continue;
            }

            self.tx.send(self.value).await?;
        }
    }
}

#[async_trait::async_trait]
impl IntoSubsystem<Error> for Button {
    async fn run(mut self, subsys: SubsystemHandle) -> Result<()> {
        tokio::select! {
            _ = subsys.on_shutdown_requested() => {
                info!("Button shutting down");
            },
            res = self.process() => res?
        }

        Ok(())
    }
}
