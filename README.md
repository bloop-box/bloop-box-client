# Bloop Box

[![ci](https://github.com/bloop-box/bloop-box-client/actions/workflows/ci.yml/badge.svg)](https://github.com/bloop-box/bloop-box-client/actions/workflows/ci.yml)

Bloop box client written in Rust with Tokio.

## NFC tag support

For pure game-related badges, any NFC tag supporting Iso14443a with a baud rate of 106 is supported. The client can
handle both 4, 7 and 10-byte UIDs.

Config tags need to use either NTAG 213, 215 or 216. Other NTAG formats may work but are not tested.

## Run-time configuration

You can change all configuration at run-time via text records on an NTAG tag. A helpful utility to automatically
generate the text records [is available here](https://github.com/bloop-box/bloop-box-config). If you prefer to generate the config tags in another way, the
following table describes the text format.

Each record begins with a single letter denoting the command. It is followed by a JSON array with zero or more
arguments.

| Command | Description                       | Arguments                        |
|---------|-----------------------------------|----------------------------------|
| w       | Set WiFi Credentials              | SSID, Password                   |
| c       | Set Connection Details            | Host, Port, Client ID, Secret    |
| v       | Set Volume Range                  | Min (0.0 - 0.1), Max (0.0 - 0.1) |
| u       | Add additional config tag         |                                  |
| r       | Remove all but current config tag |                                  |
| s       | Shut down system                  |                                  |

## LED status codes

The status RGB LED will display the current status of the Bloop Box. If no user interaction is required, you'll get a
static light, otherwise a breathing one.

### Static

- Green: Ready to read player tags
- Magenta: Processing tag
- Cyan: Config tag accepted
- Red: Config tag denied (no or malformed data)

### Breathing

- Magenta: Awaiting new config tag
- Yellow: Awaiting connection config
- Blue: Connecting to server
- Red: Invalid server credentials

## Pre-requisites

Before you can deploy the Bloop Box client, you need to set up the Raspberry Pi, including audio and NFC. If you are
using our reference design for the hardware, the steps to take are documented in the
[System setup section](./docs/system-setup.md).

## Shared data

You'll need to have a data package for the bloop box installed. For more information about this, please check the
[Bloop Box Data Example](https://github.com/bloop-box/bloop-box-data-example)

## Development

If you are testing against a locally hosted server with a self-signed certificate, you have to disable certificate
verification to connect to that server. You can do so through the following call:

```bash
BLOOP_BOX_ROOT_CERT_SOURCE=dangerous_disable bloop-box
```

Be really careful to only use this flag in development. If you use this in production, the client **will not** know if
the server certificate is genuine and be open to man-in-the-middle attacks!

Also, if you need additional debug output, you can update the `RUST_LOG` env variable to e.g. `debug` or
`error,bloop_box=debug`.

## Running Bloop Box on your desktop

Bloop Box contains an emulation feature which allows you to run it on your desktop. You can find pre-built binaries
for Windows and Linux (x86-64) in the release section. If need to run the emulator on another platform, you can clone
this repository and run:

```bash
cargo run --no-default-features --features hardware-emulation
```

## Deployment

You can find pre-compiled `.deb` files in the
[release section](https://github.com/bloop-box/bloop-box-client/releases). These will automatically set everything
up for you.

## Root Certificate Source

By default, Bloop Box will use a built-in root certificate source (webpki). If this source does not support your server
certificate, you can switch to the system's certificate store. The best way to do this is to add a systemd override:

```bash
sudo systemctl edit bloop-box.service
```

Add the following configuration:

```
[Service]
Environment="BLOOP_BOX_ROOT_CERT_STORE=native"
```

Then apply the changes:

```bash
sudo systemctl daemon-reload
sudo systemctl restart bloop-box.service
```

## Manual setup

If you want to set Bloop Box up manually, you can follow [these instructions](./docs/manual-setup.md).
