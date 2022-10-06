use anyhow::Result;

pub fn set_wifi(ssid: String, password: String) -> Result<()> {
    let mut wpa = wpactrl::Client::builder().open()?;

    wpa.request(format!("SET_NETWORK 0 ssid \"{}\"", ssid).as_str())?;
    wpa.request(format!("SET_NETWORK 0 psk \"{}\"", password).as_str())?;
    wpa.request("SAVE_CONFIG")?;

    Ok(())
}
