use anyhow::Result;
use tracing::info;

pub async fn set_wifi_credentials(ssid: String, password: String) -> Result<()> {
    info!(ssid, password, "emulated: set Wi-Fi credentials");
    Ok(())
}

pub async fn shutdown_system() -> Result<()> {
    info!("emulated: shutdown system");
    Ok(())
}
