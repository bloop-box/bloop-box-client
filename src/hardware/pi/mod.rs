use crate::hardware::led::LedController;
use crate::hardware::nfc::NfcReader;
use crate::hardware::pi::buttons::{Buttons, ButtonsConfig};
use crate::hardware::pi::led::{start_led_controller_thread, LedControllerConfig};
use crate::hardware::{InitSubsystems, Peripherals, StartSubsystems};
use crate::thread::{supervised_thread, SupervisedThread};
use anyhow::{Context, Result};
use bloop_client_framework::nfc::serve_mfrc522;
use serde::Deserialize;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio_graceful_shutdown::{IntoSubsystem, SubsystemBuilder, SubsystemHandle};
use tokio_util::sync::CancellationToken;
use tracing::info;

pub mod asset;
mod buttons;
mod led;
pub mod system;

pub struct HardwareContext {
    pub peripherals: Peripherals,
    pub threads: Vec<SupervisedThread>,
    pub init_subsystems: InitSubsystems,
}

pub fn init_hardware(shutdown_token: CancellationToken) -> Result<HardwareContext> {
    let (led_state_tx, led_state_rx) = mpsc::channel(32);
    let (button_tx, button_rx) = mpsc::channel(32);
    let (nfc_reader, nfc_backend) = NfcReader::channel();

    let peripherals = Peripherals {
        led_controller: LedController::new(led_state_tx),
        nfc_reader,
        button_receiver: button_rx,
    };

    let config = load_config()?;
    let nfc_reader_config = config.nfc_reader.into();

    let threads = vec![
        start_led_controller_thread(led_state_rx, shutdown_token.clone(), config.led_controller)?,
        supervised_thread("nfc_reader", shutdown_token, move || {
            Ok(serve_mfrc522(nfc_reader_config, nfc_backend)?)
        })?,
    ];

    let init_subsystems = Box::new(move || -> Result<StartSubsystems> {
        let buttons = Buttons::new(button_tx, config.buttons)?;

        Ok(Box::new(move |s: &SubsystemHandle| {
            s.start(SubsystemBuilder::new("Buttons", buttons.into_subsystem()));
        }))
    });

    Ok(HardwareContext {
        peripherals,
        threads,
        init_subsystems,
    })
}

#[derive(Debug, Deserialize, Default)]
struct Config {
    #[serde(default)]
    buttons: ButtonsConfig,
    #[serde(default)]
    led_controller: LedControllerConfig,
    #[serde(default)]
    nfc_reader: NfcReaderConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct NfcReaderConfig {
    spi_dev_path: PathBuf,
    gpio_dev_path: PathBuf,
    reset_pin_line: u32,
}

impl Default for NfcReaderConfig {
    fn default() -> Self {
        let defaults = bloop_client_framework::nfc::Mfrc522Config::default();

        Self {
            spi_dev_path: defaults.spi_dev_path,
            gpio_dev_path: defaults.gpio_dev_path,
            reset_pin_line: defaults.reset_pin_line,
        }
    }
}

impl From<NfcReaderConfig> for bloop_client_framework::nfc::Mfrc522Config {
    fn from(config: NfcReaderConfig) -> Self {
        let mut framework_config = Self::default();
        framework_config.spi_dev_path = config.spi_dev_path;
        framework_config.gpio_dev_path = config.gpio_dev_path;
        framework_config.reset_pin_line = config.reset_pin_line;

        framework_config
    }
}

fn load_config() -> Result<Config> {
    let path: PathBuf = "/etc/bloop-box.conf".into();

    let mut file = match File::open(&path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            info!(
                "Config file {} not found, using default config",
                path.display()
            );
            return Ok(Config::default());
        }
        Err(err) => {
            return Err(err).with_context(|| format!("Failed to open {}", path.display()))?
        }
    };

    let mut toml_config = String::new();
    file.read_to_string(&mut toml_config)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let config: Config = toml::from_str(&toml_config)
        .with_context(|| format!("Failed to parse {}", path.display()))?;

    Ok(config)
}
