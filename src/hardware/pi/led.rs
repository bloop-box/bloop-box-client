use crate::hardware::led::LedState;
use crate::thread::{supervised_thread, SupervisedThread};
use anyhow::Result;
use aw2013::{Aw2013, Current, Timing};
use linux_embedded_hal::I2cdev;
use serde::Deserialize;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize)]
pub struct LedControllerConfig {
    #[serde(default = "LedControllerConfig::default_i2c_dev_path")]
    i2c_dev_path: PathBuf,
}

impl Default for LedControllerConfig {
    fn default() -> Self {
        Self {
            i2c_dev_path: Self::default_i2c_dev_path(),
        }
    }
}

impl LedControllerConfig {
    fn default_i2c_dev_path() -> PathBuf {
        "/dev/i2c-1".into()
    }
}

pub fn start_led_controller_thread(
    rx: mpsc::Receiver<LedState>,
    shutdown_token: CancellationToken,
    config: LedControllerConfig,
) -> Result<SupervisedThread> {
    Ok(supervised_thread(
        "led_controller",
        shutdown_token,
        move || led_controller_thread(rx, config),
    )?)
}

fn led_controller_thread(
    mut rx: mpsc::Receiver<LedState>,
    config: LedControllerConfig,
) -> Result<()> {
    let i2c = I2cdev::new(config.i2c_dev_path)?;
    let mut aw2013 = Aw2013::from_default_address(i2c, [Current::Five; 3]);

    aw2013.reset()?;
    sleep(Duration::from_millis(10));
    aw2013.enable()?;

    while let Some(command) = rx.blocking_recv() {
        match command {
            LedState::Static(color) => {
                let rgb = color.rgb();
                aw2013.set_static_rgb([rgb.0 / 4, rgb.1 / 4, rgb.2 / 4], None, None)?;
            }
            LedState::Breathing(color) => {
                let rgb = color.rgb();
                aw2013.set_breathing_rgb(
                    [rgb.0 / 2, rgb.1 / 2, rgb.2 / 2],
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

    sleep(Duration::from_millis(10));
    aw2013.reset()?;

    Ok(())
}
