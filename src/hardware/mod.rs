use crate::hardware::buttons::ButtonReceiver;
use crate::hardware::led::LedController;
use crate::hardware::nfc::NfcReader;
use anyhow::{Context, Result};
use directories::ProjectDirs;
use std::env;
use std::panic::UnwindSafe;
use std::path::PathBuf;
use tokio::fs;
use tokio_graceful_shutdown::SubsystemHandle;

#[cfg(feature = "hardware-emulation")]
pub use emulated::*;
#[cfg(not(feature = "hardware-emulation"))]
pub use pi::*;

pub mod buttons;
#[cfg(feature = "hardware-emulation")]
mod emulated;
pub mod led;
pub mod nfc;
#[cfg(not(feature = "hardware-emulation"))]
mod pi;

pub struct Peripherals {
    pub led_controller: LedController,
    pub nfc_reader: NfcReader,
    pub button_receiver: ButtonReceiver,
}

pub type InitSubsystems = Box<dyn FnOnce() -> Result<StartSubsystems> + Send + UnwindSafe>;
pub type StartSubsystems = Box<dyn FnOnce(&SubsystemHandle) + Send>;

pub async fn data_path() -> Result<PathBuf> {
    if let Ok(dir) = env::var("BLOOP_BOX_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }

    let project_dirs =
        ProjectDirs::from("", "", "bloop-box").context("failed to get project dirs")?;
    let data_dir = project_dirs.data_dir();

    if fs::metadata(data_dir).await.is_err() {
        fs::create_dir_all(data_dir)
            .await
            .with_context(|| format!("failed to create data dir {}", data_dir.display()))?;
    }

    Ok(data_dir.to_path_buf())
}
