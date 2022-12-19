use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

fn default_nfc_device() -> String {
    "/dev/spidev0.0".to_string()
}
fn default_nfc_max_speed() -> u32 {
    1_000_000
}
fn default_nfc_reset_pin() -> u8 {
    25
}

#[derive(Debug, Clone, Deserialize)]
pub struct NfcConfig {
    #[serde(default = "default_nfc_device")]
    pub device: String,
    #[serde(default = "default_nfc_max_speed")]
    pub max_speed: u32,
    #[serde(default = "default_nfc_reset_pin")]
    pub reset_pin: u8,
}

fn default_volume_down_button() -> u8 {
    24
}
fn default_volume_up_button() -> u8 {
    23
}
fn default_red_led() -> u8 {
    16
}
fn default_green_led() -> u8 {
    20
}
fn default_blue_led() -> u8 {
    26
}

#[derive(Debug, Clone, Deserialize)]
pub struct GpioConfig {
    #[serde(default = "default_volume_down_button")]
    pub volume_down_button: u8,
    #[serde(default = "default_volume_up_button")]
    pub volume_up_button: u8,
    #[serde(default = "default_red_led")]
    pub red_led: u8,
    #[serde(default = "default_green_led")]
    pub green_led: u8,
    #[serde(default = "default_blue_led")]
    pub blue_led: u8,
}

#[derive(Clone, Deserialize)]
pub struct EtcConfig {
    pub nfc: NfcConfig,
    pub gpio: GpioConfig,
}

pub async fn load_etc_config() -> Result<EtcConfig> {
    let path = Path::new("/etc/bloop-box.conf");
    let mut file = File::open(&path)
        .await
        .with_context(|| format!("Failed to open {}", path.display()))?;
    let mut toml_config = String::new();
    file.read_to_string(&mut toml_config).await?;
    let config: EtcConfig = toml::from_str(&toml_config)?;
    Ok(config)
}
