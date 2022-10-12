# Boop Box

Boop box client written in Rust with Tokio.

## LED Status Codes

### Static

- Green: Ready to read attendee tags
- Yellow: Processing tag
- Cyan: Config tag accepted
- Red: Config tag denied (malformed data)

### Blinking

- Magenta: Awaiting new config tag
- Yellow: Awaiting connection config
- Blue: Connecting to server
- Red: Invalid server credentials

## Deployment

### Cross compilation

In order to compile Boop Box for the Raspberry Zero W, run the following command:

```bash
cross build --target arm-unknown-linux-gnueabihf
```

This will generate a debug build. In order to create a release build, add `--release` to the command.

Copy the resulting binary from the target folder to `/home/pi/bin/boop-box`.

### Systemd

To have the boop box automatically start when the system boots, create a systemd file in
`/lib/systemd/system/boop-box.service`:

```
[Unit]
Description=BoopBox
After=network.target

[Service]
Type=simple
User=pi
ExecStart=/home/pi/bin/boop-box
Environment="RUST_LOG=info"
WorkingDirectory=/home/pi
Restart=always

[Install]
WantedBy=multi-user.target
```

Then run the following two commands to register and start the daemon:

```bash
sudo systemctl daemon-reload
sudo systemctl start boop-box
```

## System setup

- Flash an SD card with the Raspian Buster Lite image.
- Create an empty file `/boot/ssh`.

Create a file `/boot/wpa_supplicant.conf` with the following contents and adjust SSID and PSK:

```
country=DE
ctrl_interface=DIR=/var/run/wpa_supplicant GROUP=netdev
update_config=1

network={
    ssid="NETWORK-NAME"
    psk="NETWORK-PASSWORD"
}
```

Then insert the SD card into the Boop Box and boot it up. After that, connect to it via SSH (pi:raspberry). The first
thing you should do is to add your public key to `./.ssh/authorized_keys`. Then you should disable password
authentication for security reasons.

To do so, edit `/etc/ssh/sshd_config`, uncomment `PasswordAuthentication` and set the value to `no`. Then you can reload
the SSH daemon: `sudo systemctl reload sshd`.

Then install the audio driver according to these instructions:
https://learn.adafruit.com/adafruit-max98357-i2s-class-d-mono-amp/raspberry-pi-usage

Next set up libnfc and audio for the pi user:

```bash
sudo apt install libnfc5 libnfc-dev mpg123
sudo raspi-config nonint do_spi 0
sudo usermod -a -G audio pi
```

Now you just have to configure the NFC device. Do so by editing `/etc/nfc/libnfc.conf` and modify the last two lines:

```
device.name = "pn532"
device.connstring = "pn532_spi:/dev/spidev0.0:50000"
```
