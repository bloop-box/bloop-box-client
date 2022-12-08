use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

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
