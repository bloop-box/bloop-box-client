use crate::hardware::led::LedState;
use anyhow::{Error, Result};
use eframe::epaint::Color32;
use std::time::Duration;
use tokio::select;
use tokio::sync::{mpsc, watch};
use tokio::time::{interval, Instant};
use tokio_graceful_shutdown::{FutureExt, IntoSubsystem, SubsystemHandle};

const BREATHING_PERIOD: Duration = Duration::from_secs(4);

#[derive(Debug)]
pub struct LedControllerTask {
    rx: mpsc::Receiver<LedState>,
    tx: watch::Sender<Color32>,
    state: Option<LedState>,
    breathing_start: Instant,
}

impl LedControllerTask {
    pub fn new(rx: mpsc::Receiver<LedState>, tx: watch::Sender<Color32>) -> Self {
        Self {
            rx,
            tx,
            state: None,
            breathing_start: Instant::now(),
        }
    }

    async fn process(&mut self) -> Result<()> {
        let mut ticker = interval(Duration::from_millis(60));

        loop {
            select! {
                state = self.rx.recv() => match state {
                    Some(state) => {
                        self.state = Some(state);
                        self.breathing_start = Instant::now();
                    }
                    None => break,
                },
                _ = ticker.tick() => self.handle_tick().await?,
            }
        }

        Ok(())
    }

    async fn handle_tick(&mut self) -> Result<()> {
        let Some(state) = self.state.as_ref() else {
            return Ok(());
        };

        let new_color = match state {
            LedState::Static(color) => {
                let (r, g, b) = color.rgb();
                Color32::from_rgb(r, g, b)
            }
            LedState::Breathing(color) => {
                let (r, g, b) = color.rgb();

                let elapsed = self.breathing_start.elapsed().as_secs_f32();
                let t = (elapsed % BREATHING_PERIOD.as_secs_f32()) / BREATHING_PERIOD.as_secs_f32();
                let brightness = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * t).cos());

                let (r, g, b) = (
                    (r as f32 * brightness) as u8,
                    (g as f32 * brightness) as u8,
                    (b as f32 * brightness) as u8,
                );
                Color32::from_rgb(r, g, b)
            }
        };

        if *self.tx.borrow() != new_color {
            let _ = self.tx.send(new_color);
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl IntoSubsystem<Error> for LedControllerTask {
    async fn run(mut self, subsys: SubsystemHandle) -> Result<()> {
        if let Ok(result) = self.process().cancel_on_shutdown(&subsys).await {
            result?;
        }

        Ok(())
    }
}
