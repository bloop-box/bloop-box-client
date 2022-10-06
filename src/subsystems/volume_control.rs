use anyhow::{Error, Result};
use log::info;
use rppal::gpio::Gpio;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_graceful_shutdown::{IntoSubsystem, SubsystemHandle};

use crate::subsystems::audio_player::PlayerCommand;
use crate::subsystems::button::Button;

pub struct VolumeControl {
    audio_player: mpsc::Sender<PlayerCommand>,
}

const GPIO_VOLUME_DOWN: u8 = 23;
const GPIO_VOLUME_UP: u8 = 24;

impl VolumeControl {
    pub fn new(audio_player: mpsc::Sender<PlayerCommand>) -> Self {
        Self { audio_player }
    }

    async fn process(&mut self, mut button_rx: mpsc::Receiver<f32>) -> Result<()> {
        while let Some(delta) = button_rx.recv().await {
            let (get_length_tx, get_length_rx) = oneshot::channel();
            self.audio_player.send(PlayerCommand::GetVolume { responder: get_length_tx }).await?;

            let current_volume = get_length_rx.await?;
            self.audio_player.send(PlayerCommand::SetVolume {
                volume: (current_volume + delta).clamp(0., 1.),
            }).await?;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl IntoSubsystem<Error> for VolumeControl {
    async fn run(mut self, subsys: SubsystemHandle) -> Result<()> {
        let gpio = Gpio::new()?;
        let (button_tx, button_rx) = mpsc::channel::<f32>(1);
        let volume_down_pin = gpio.get(GPIO_VOLUME_DOWN)?;
        let volume_up_pin = gpio.get(GPIO_VOLUME_UP)?;

        subsys.start("VolumeDownButton", Button::new(volume_down_pin, button_tx.clone(), -0.05).into_subsystem());
        subsys.start("VolumeUpButton", Button::new(volume_up_pin, button_tx, 0.05).into_subsystem());

        tokio::select! {
            _ = subsys.on_shutdown_requested() => {
                info!("Volume control shutting down");
            },
            res = self.process(button_rx) => res?
        }

        Ok(())
    }
}
