use anyhow::{bail, Context, Result};
use tokio::process::Command;

pub async fn set_wifi_credentials(
    ssid: impl Into<String>,
    password: impl Into<String>,
) -> Result<()> {
    let output = Command::new("sudo")
        .args([
            "nmcli",
            "dev",
            "wifi",
            "connect",
            &ssid.into(),
            "password",
            &password.into(),
            "ifname",
            "wlan0",
        ])
        .output()
        .await
        .context("Failed to set Wi-Fi credentials")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to set Wi-Fi credentials: {}", stderr);
    }

    Ok(())
}

pub async fn shutdown_system() -> Result<()> {
    let output = Command::new("sudo")
        .args(["shutdown", "now"])
        .output()
        .await
        .context("Failed to shut down system")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to shut down system: {}", stderr);
    }

    Ok(())
}
