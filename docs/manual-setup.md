# Manual setup

## Cross compilation

To compile Bloop Box for the Raspberry Zero 2 W, run the following command:

```bash
cross build --target aarch64-unknown-linux-gnu
```

Or if you still have a legacy Raspberry Zero W:

```bash
cross build --target arm-unknown-linux-gnueabihf
```

This will generate a debug build. To create a release build, add `--release` to the command. Copy the resulting binary
from the target folder to `/usr/bin/bloop-box` on your Raspberry.

## Shared data

As with the automatic setup, you need to place shared audio files in `/usr/share/bloop-box`. This folder should have the
same layout as in our [Bloop Box Data Example](https://github.com/bloop-box/bloop-box-data-example).

## User setup

Next, you'll need to set up the bloop-box user and its lib directory:

```bash
adduser --system --home /nonexistent --gecos "bloop-box" \
        --no-create-home --disabled-password \
        --quiet bloop-box
usermod -a -G gpio,spi,i2c,audio,netdev bloop-box
mkdir -p /var/lib/bloop-box
chown bloop-box:nogroup /var/lib/bloop-box
```

On a development system, you might want to give the bloop-box user a login shell and a home directory.

## Allow bloop-box user to call system commands

The bloop box client can shut down the system and set Wi-Fi credentials when it receives the right config command. To do
so, it needs sudo privilege to call the `shutdown` and `nmcli` binary. You can achieve this by copying the
`011_bloop-box` file from the `etc` directory to `/etc/sudoers.d` and change its permissions to `440`.

## Hardware config

To configure the Bloop Box for your specific hardware, you need to copy the `etc/bloop-box.conf` file to via to
`/etc/bloop-box.conf`. If you are using our open hardware motherboard, you can leave all values in it as is or omit the
entire file. Otherwise, you need to adjust it to match your hardware.

## Systemd

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
Environment="BLOOP_BOX_DATA_DIR=/var/lib/bloop-box"
Restart=always

[Install]
WantedBy=multi-user.target
```

Then run the following two commands to register and start the daemon:

```bash
sudo systemctl daemon-reload
sudo systemctl start bloop-box
```
