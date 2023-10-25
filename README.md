# Bloop Box

[![ci](https://github.com/bloop-box/bloop-box-client/actions/workflows/ci.yml/badge.svg)](https://github.com/bloop-box/bloop-box-client/actions/workflows/ci.yml)

Bloop box client written in Rust with Tokio.

## Table of Contents

1. [NFC tag support](#nfc-tag-support)
2. [Run-time configuration](#run-time-configuration)
3. [LED status codes](#led-status-codes)
4. [Pre-requisites](#pre-requisites)
5. [Shared data](#shared-data)
6. [Development](#development)
7. [Deployment](#deployment)
   1. [Automatic](#automatic)
   2. [Manual](#manual)
8. [System Setup](#system-setup)

## NFC tag support

While for reading UIDs any NFC tag supporting Iso14443a with a baud rate of 106 is supported, it is recommended to use
tags with a 7-byte UID. Tags with shorter UIDs will be padded with zeroes, while tags with longer UIDs will be
truncated.

When it comes to config tags though you have to use either NTAG 213, 215 or 216. Other NTAG formats may work but are
not tested.

## Run-time configuration

You can change all configuration at run-time via text records on an NTAG tag. A helpful utility to automatically
generate the text records [is available here](https://github.com/bloop-box/bloop-box-config). If you prefer to generate
the config tags in another way, following is the format for the text record:

Each record begins with a single letter denoting the command. It is followed by a JSON array with zero or more
arguments.

| Command | Description                       | Arguments                     |
|---------|-----------------------------------|-------------------------------|
| w       | Set WiFi Credentials              | SSID, Password                |
| c       | Set Connection Details            | Host, Port, Client ID, Secret |
| v       | Set Max Volume                    | Volume (0.0 - 0.1)            |
| u       | Add additional config tag         |                               |
| r       | Remove all but current config tag |                               |
| s       | Shut down system                  |                               |

## LED status codes

The status RGB LED will display the current status of the Bloop Box. If no user interaction is required, you'll get a
static light, otherwise a blinking one.

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

## Pre-requisites

Before you can deploy the Bloop Box client you need to set up the Raspberry Pi of course, including audio and NFC. If
you are using our reference design for the hardware, the steps to take are documented in the
[System setup section](#system-setup).

## Shared data

You'll need to have a data package for the bloop box installed. For more information about this, please check the
[Bloop Box Data Example](https://github.com/bloop-box/bloop-box-data-example)

## Development

If you are testing against a locally hosted server with a self-signed certificate, you have to disable certificate
verification in order to connect to that server. You can do so through the following call:

```bash
bloop-box --dangerous-disable-cert-verification
```

Be really careful to only use this flag in development. If you use this in production, the client **will not** know if
the server certificate is genuine and be open to man-in-the-middle attacks!

## Deployment

### Automatic

You can find pre-compiled `.deb` files in the
[release section](https://github.com/bloop-box/bloop-box-client/releases). These will automatically set everything
up for you.

The deb files come pre-compiled for two different architectures:

- `armhf`: Build for Raspberry Zero W running on 32 bit Raspian
- `arm64`: Build for Raspberry Zero 2 W running on 64 bit Raspbian

### Manual

#### Cross compilation

In order to compile Bloop Box for the Raspberry Zero W, run the following command:

```bash
cross build --target arm-unknown-linux-gnueabihf
```

Alternatively you can compile for the Raspberry Zero 2 W:

```bash
cross build --target aarch64-unknown-linux-gnu
```

This will generate a debug build. In order to create a release build, add `--release` to the command.

Copy the resulting binary from the target folder to `/usr/bin/bloop-box`.

#### User setup

Apart from the shared data, you'll need to set up the bloop-box user and its lib directory:

```bash
adduser --system --home /nonexistent --gecos "bloop-box" \
        --no-create-home --disabled-password \
        --quiet bloop-box
usermod -a -G gpio,spi,i2c,audio,netdev bloop-box
mkdir -p /var/lib/bloop-box
chown bloop-box:nogroup /var/lib/bloop-box
```

On a development system, you might want to give the bloop-box user a login shell and a home directory.

#### Allow bloop-box user to call system commands

The bloop box client has the capability to shut down the system and set wifi credentials when it receives the right
config command. In order to do so it needs sudo privilege to call the `shutdown` and `nmcli` binary. You can accomplish
this by copying the `011_bloop-box` file from the `etc` directory to `/etc/sudoers.d` and change its permissions to
`440`.

#### Hardware config

In order to configure the Bloop Box for your specific hardware, you need to copy the `etc/bloop-box.conf` file to via
SSH to `/etc/bloop-box`. If you are using our open hardware motherboard, you can leave all values in it as is, otherwise
you need to adjust it to match your hardware.

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
Environment="RUST_LOG=error,bloop_box=info"
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

First flash an SD card with the Raspian (Bookworm) Lite image using the official imager. This will allow you to set up 
Wi-Fi credentials and configure your public key. You should completely disable password authentication.

Then install the audio driver according to these instructions:
https://learn.adafruit.com/adafruit-max98357-i2s-class-d-mono-amp/raspberry-pi-usage

Next you need to enable SPI and I2C for the MFRC522 NFC reader and the status LED:

```bash
sudo raspi-config nonint do_spi 0
sudo raspi-config nonint do_i2c 0
```

Lastly you should configure one of the power board LEDs to act as an activity LED for the system. This allows you to see
when a box has fully shut down before you unplug it. To do so, add the following line at the end of `/boot/config.txt`:

```
dtparam=act_led_gpio=27,act_led_activelow=off
```
