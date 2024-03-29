use anyhow::Result;
use std::process::Command;

pub fn set_wifi(ssid: String, password: String) -> Result<()> {
    Command::new("sudo")
        .args([
            &"nmcli",
            &"dev",
            &"wifi",
            &"connect",
            ssid.as_str(),
            &"password",
            password.as_str(),
            &"ifname",
            &"wlan0",
        ])
        .output()
        .expect("Failed to set WiFi credentials");

    Ok(())
}
