# System setup

To set up a Raspberry Pi for use with the Bloop Box client, follow these steps:

## Flash image

Flash an SD card with the Raspian (Bookworm) Lite image using the official imager. This will allow you to set up
initial Wi-Fi credentials and configure your public key. You should completely disable password authentication.

## Install audio drivers

Install the audio driver according to these instructions:
https://learn.adafruit.com/adafruit-max98357-i2s-class-d-mono-amp/raspberry-pi-usage

## Enable SPI and I2C

Run the following commands to enable SPI (for MFRC522 NFC reader) and I2C (for the AW2013 LED):

```bash
sudo raspi-config nonint do_spi 0
sudo raspi-config nonint do_i2c 0
```

## Configure power LED (optional)

You should configure one of the power board LEDs to act as an activity LED for the system. This allows you to see
when a box has fully shut down before you unplug it. To do so, add the following line at the end of `/boot/config.txt`:

```
dtparam=act_led_gpio=27,act_led_activelow=off
```
