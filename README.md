# Bloop Box

[![ci](https://github.com/bloop-box/bloop-box-client/actions/workflows/ci.yml/badge.svg)](https://github.com/bloop-box/bloop-box-client/actions/workflows/ci.yml)

Bloop box client written in Rust with Tokio.

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

## Shared data

You'll need to have a data package for the bloop box installed. For more information about this, please check the
[Bloop Box Data Example](https://github.com/bloop-box/bloop-box-data-example)

## Deployment

### Automatic

You can find pre-compiled `.deb` files in the
[release section](https://github.com/bloop-box/bloop-box-client/releases). These will automatically set everything
up for you.

### Manual

#### Cross compilation

In order to compile Bloop Box for the Raspberry Zero W, run the following command:

```bash
cross build --target arm-unknown-linux-gnueabihf
```

This will generate a debug build. In order to create a release build, add `--release` to the command.

Copy the resulting binary from the target folder to `/usr/bin/bloop-box`.

#### User setup

Apart from the shared data, you'll need to set up the bloop-box user and its run directory:

```bash
adduser --system --home /nonexistent --gecos "bloop-box" \
        --no-create-home --disabled-password \
        --quiet bloop-box
usermod -a -G gpio,spi,audio bloop-box
mkdir -p /run/bloop-box
chown bloop-box:bloop-box /run/bloop-box
```

On a development system, you might want to give the bloop-box user a login shell and a home directory.

#### Systemd

To have the bloop box automatically start when the system boots, create a systemd file in
`/lib/systemd/system/bloop-box.service`:

```
[Unit]
Description=BloopBox
After=network.target

[Service]
Type=simple
User=bloop-box
ExecStart=/usr/bin/bloop-box
Environment="RUST_LOG=info"
Restart=always

[Install]
WantedBy=multi-user.target
```

Then run the following two commands to register and start the daemon:

```bash
sudo systemctl daemon-reload
sudo systemctl start bloop-box
```

## System setup

- Flash an SD card with the Raspian Bullseye Lite image.
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

Then insert the SD card into the Bloop Box and boot it up. After that, connect to it via SSH (pi:raspberry). The first
thing you should do is to add your public key to `./.ssh/authorized_keys`. Then you should disable password
authentication for security reasons.

To do so, edit `/etc/ssh/sshd_config`, uncomment `PasswordAuthentication` and set the value to `no`. Then you can
reload the SSH daemon: `sudo systemctl reload sshd`.

Then install the audio driver according to these instructions:
https://learn.adafruit.com/adafruit-max98357-i2s-class-d-mono-amp/raspberry-pi-usage

Next set up libnfc:

```bash
sudo apt install libnfc6
sudo raspi-config nonint do_spi 0
```

Now you just have to configure the NFC device. Do so by editing `/etc/nfc/libnfc.conf` and modify the last two lines:

```
device.name = "pn532"
device.connstring = "pn532_spi:/dev/spidev0.0:50000"
```
