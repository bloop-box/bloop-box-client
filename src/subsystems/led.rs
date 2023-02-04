use std::time::Duration;
use anyhow::{Error, Result};
use aw2013::{Aw2013, Current, Timing};
use log::info;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_graceful_shutdown::{IntoSubsystem, SubsystemHandle};

pub type Color = (u8, u8, u8);

pub const RED: Color = (255, 0, 0);
pub const GREEN: Color = (0, 255, 0);
pub const BLUE: Color = (0, 0, 255);
pub const YELLOW: Color = (255, 255, 0);
pub const MAGENTA: Color = (255, 0, 255);
pub const CYAN: Color = (0, 255, 255);

#[derive(Debug)]
pub enum LedState {
    On { color: Color },
    Blink { color: Color },
}

pub struct Led {
    rx: mpsc::Receiver<LedState>,
}

impl Led {
    pub fn new(rx: mpsc::Receiver<LedState>) -> Self {
        Self { rx }
    }

    async fn process(
        &mut self,
        tx: mpsc::Sender<InternalLedState>,
    ) -> Result<()> {
        while let Some(led_state) = self.rx.recv().await {
            match led_state {
                LedState::On { color } => {
                    tx.send(InternalLedState::On { color }).await?
                },
                LedState::Blink { color } => {
                    tx.send(InternalLedState::Blink { color }).await?
                }
            }
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl IntoSubsystem<Error> for Led {
    async fn run(mut self, subsys: SubsystemHandle) -> Result<()> {
        let (internal_tx, internal_rx) = mpsc::channel(8);

        subsys.start(
            "InternalLed",
            InternalLed::new(internal_rx).into_subsystem(),
        );

        tokio::select! {
            _ = subsys.on_shutdown_requested() => {
                info!("LED shutting down");
            },
            res = self.process(internal_tx) => res?,
        }

        Ok(())
    }
}

struct InternalLed {
    rx: mpsc::Receiver<InternalLedState>,
}

#[derive(Debug)]
enum InternalLedState {
    On { color: Color },
    Blink { color: Color },
}

impl InternalLed {
    pub fn new(rx: mpsc::Receiver<InternalLedState>) -> Self {
        Self { rx }
    }

    async fn process(&mut self, aw2013: &mut Aw2013) -> Result<()> {
        while let Some(led_state) = self.rx.recv().await {
            sleep(Duration::from_millis(10)).await;

            match led_state {
                InternalLedState::On { color } => {
                    aw2013.set_static_rgb(
                        [color.0 / 4, color.1 / 4, color.2 / 4],
                        None,
                        None,
                    )?;
                }
                InternalLedState::Blink { color } => {
                    aw2013.set_breathing_rgb(
                        [color.0 / 2, color.1 / 2, color.2 / 2],
                        &Timing {
                            delay: 0,
                            rise: 2,
                            hold: 2,
                            fall: 2,
                            off: 2,
                            cycles: 0,
                        },
                    )?;
                }
            }
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl IntoSubsystem<Error> for InternalLed {
    async fn run(mut self, subsys: SubsystemHandle) -> Result<()> {
        let mut aw2013 = Aw2013::from_default_address([Current::Five; 3])?;
        aw2013.reset()?;
        sleep(Duration::from_millis(10)).await;
        aw2013.enable()?;
        sleep(Duration::from_millis(10)).await;

        tokio::select! {
            _ = subsys.on_shutdown_requested() => {
                info!("Internal LED shutting down");
            },
            res = self.process(&mut aw2013) => res?,
        }

        sleep(Duration::from_millis(10)).await;
        aw2013.reset()?;

        Ok(())
    }
}
