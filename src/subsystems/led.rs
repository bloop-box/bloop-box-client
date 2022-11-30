use crate::etc_config::EtcConfig;
use anyhow::{Error, Result};
use log::info;
use rppal::gpio::{Gpio, Level, OutputPin};
use tokio::sync::mpsc;
use tokio::time::{self, Duration};
use tokio_graceful_shutdown::{IntoSubsystem, NestedSubsystem, SubsystemHandle};

pub type Color = (Level, Level, Level);

pub const RED: Color = (Level::High, Level::Low, Level::Low);
pub const GREEN: Color = (Level::Low, Level::High, Level::Low);
pub const BLUE: Color = (Level::Low, Level::Low, Level::High);
pub const YELLOW: Color = (Level::High, Level::High, Level::Low);
pub const MAGENTA: Color = (Level::High, Level::Low, Level::High);
pub const CYAN: Color = (Level::Low, Level::High, Level::High);

#[derive(Debug)]
pub enum LedState {
    On { color: Color },
    Blink { color: Color },
}

pub struct Led {
    etc_config: EtcConfig,
    rx: mpsc::Receiver<LedState>,
}

impl Led {
    pub fn new(etc_config: EtcConfig, rx: mpsc::Receiver<LedState>) -> Self {
        Self { etc_config, rx }
    }

    async fn process(
        &mut self,
        subsys: SubsystemHandle,
        tx: mpsc::Sender<InternalLedState>,
    ) -> Result<()> {
        let mut maybe_blink_subsys: Option<NestedSubsystem> = None;

        while let Some(led_state) = self.rx.recv().await {
            if let Some(blink_subsys) = maybe_blink_subsys {
                subsys.perform_partial_shutdown(blink_subsys).await?;
                maybe_blink_subsys = None;
            }

            match led_state {
                LedState::On { color } => tx.send(InternalLedState::On { color }).await?,
                LedState::Blink { color } => {
                    let tx = tx.clone();
                    maybe_blink_subsys =
                        Some(subsys.start("Blink", move |subsys| blink(subsys, color, tx)));
                }
            }
        }

        Ok(())
    }
}

async fn blink(
    subsys: SubsystemHandle,
    color: Color,
    tx: mpsc::Sender<InternalLedState>,
) -> Result<()> {
    let mut interval = time::interval(Duration::from_millis(500));
    let mut on = false;

    tx.send(InternalLedState::Off).await?;

    loop {
        tokio::select! {
            _ = interval.tick() => if on {
                tx.send(InternalLedState::Off).await?;
                on = false;
            } else {
                tx.send(InternalLedState::On { color }).await?;
                on = true;
            },
            _ = subsys.on_shutdown_requested() => break,
        }
    }

    Ok(())
}

#[async_trait::async_trait]
impl IntoSubsystem<Error> for Led {
    async fn run(mut self, subsys: SubsystemHandle) -> Result<()> {
        let (internal_tx, internal_rx) = mpsc::channel(8);

        subsys.start(
            "InternalLed",
            InternalLed::new(self.etc_config.clone(), internal_rx).into_subsystem(),
        );

        tokio::select! {
            _ = subsys.on_shutdown_requested() => {
                info!("LED shutting down");
            },
            res = self.process(subsys.clone(), internal_tx) => res?,
        }

        Ok(())
    }
}

struct InternalLed {
    etc_config: EtcConfig,
    rx: mpsc::Receiver<InternalLedState>,
}

#[derive(Debug)]
enum InternalLedState {
    On { color: Color },
    Off,
}

impl InternalLed {
    pub fn new(etc_config: EtcConfig, rx: mpsc::Receiver<InternalLedState>) -> Self {
        Self { etc_config, rx }
    }

    async fn process(
        &mut self,
        red_pin: &mut OutputPin,
        green_pin: &mut OutputPin,
        blue_pin: &mut OutputPin,
    ) -> Result<()> {
        while let Some(led_state) = self.rx.recv().await {
            match led_state {
                InternalLedState::On { color } => {
                    red_pin.write(color.0);
                    green_pin.write(color.1);
                    blue_pin.write(color.2);
                }
                InternalLedState::Off => {
                    red_pin.write(Level::Low);
                    green_pin.write(Level::Low);
                    blue_pin.write(Level::Low);
                }
            }
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl IntoSubsystem<Error> for InternalLed {
    async fn run(mut self, subsys: SubsystemHandle) -> Result<()> {
        let gpio = Gpio::new()?;
        let mut red_pin = gpio.get(self.etc_config.gpio.red_led)?.into_output_low();
        let mut green_pin = gpio.get(self.etc_config.gpio.green_led)?.into_output_low();
        let mut blue_pin = gpio.get(self.etc_config.gpio.blue_led)?.into_output_low();

        tokio::select! {
            _ = subsys.on_shutdown_requested() => {
                info!("Internal LED shutting down");
            },
            res = self.process(&mut red_pin, &mut green_pin, &mut blue_pin) => res?,
        }

        red_pin.write(Level::Low);
        green_pin.write(Level::Low);
        blue_pin.write(Level::Low);

        Ok(())
    }
}
