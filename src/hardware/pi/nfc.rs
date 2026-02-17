use crate::hardware::nfc::{NfcReaderRequest, NfcUid};
use crate::hardware::pi::ndef::{parse_ndef_text_record, NdefMessageParser};
use crate::thread::{supervised_thread, SupervisedThread};
use anyhow::{anyhow, Context, Result};
use gpiocdev::line::Value;
use gpiocdev::Request;
use linux_embedded_hal::spidev::{SpiModeFlags, Spidev, SpidevOptions};
use linux_embedded_hal::SpidevDevice;
use mfrc522::comm::blocking::spi::SpiInterface;
use mfrc522::comm::Interface;
use mfrc522::{Initialized, Mfrc522};
use serde::Deserialize;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{instrument, warn};

#[derive(Debug, Deserialize)]
pub struct NfcReaderConfig {
    #[serde(default = "NfcReaderConfig::default_spi_dev_path")]
    spi_dev_path: PathBuf,
    #[serde(default = "NfcReaderConfig::default_gpio_dev_path")]
    gpio_dev_path: PathBuf,
    #[serde(default = "NfcReaderConfig::default_reset_pin_line")]
    reset_pin_line: u32,
}

impl Default for NfcReaderConfig {
    fn default() -> Self {
        Self {
            spi_dev_path: Self::default_spi_dev_path(),
            gpio_dev_path: Self::default_gpio_dev_path(),
            reset_pin_line: Self::default_reset_pin_line(),
        }
    }
}

impl NfcReaderConfig {
    fn default_spi_dev_path() -> PathBuf {
        "/dev/spidev0.0".into()
    }

    fn default_gpio_dev_path() -> PathBuf {
        "/dev/gpiochip0".into()
    }

    fn default_reset_pin_line() -> u32 {
        25
    }
}

pub fn run_nfc_reader(
    rx: mpsc::Receiver<NfcReaderRequest>,
    shutdown_token: CancellationToken,
    config: NfcReaderConfig,
) -> Result<SupervisedThread> {
    Ok(supervised_thread(
        "nfc_reader",
        shutdown_token,
        move || nfc_reader_thread(rx, config),
    )?)
}

#[instrument]
fn nfc_reader_thread(
    mut rx: mpsc::Receiver<NfcReaderRequest>,
    config: NfcReaderConfig,
) -> Result<()> {
    let options = SpidevOptions::new()
        .max_speed_hz(1_000_000)
        .mode(SpiModeFlags::SPI_MODE_0)
        .build();

    let mut spi = Spidev::open(config.spi_dev_path)?;
    spi.configure(&options)?;

    let request = Request::builder()
        .on_chip(config.gpio_dev_path)
        .with_consumer("bloop-box")
        .with_line(config.reset_pin_line)
        .as_output(Value::Inactive)
        .request()
        .context("Failed to create GPIO request")?;

    sleep(Duration::from_millis(150));
    request
        .set_lone_value(Value::Active)
        .context("Failed to set reset pin to active")?;
    sleep(Duration::from_millis(50));

    let spi = SpidevDevice(spi);
    let interface = SpiInterface::new(spi);
    let mfrc522 = Mfrc522::new(interface)
        .init()
        .map_err(|err| anyhow!("Failed to initialize MFRC522: {:?}", err))?;
    let mut adapter = Adapter::new(mfrc522);

    'main_loop: while let Some(command) = rx.blocking_recv() {
        match command {
            NfcReaderRequest::WaitForCardPresent(response) => {
                let uid = loop {
                    if response.is_closed() {
                        continue 'main_loop;
                    }

                    if let Some(uid) = adapter.select_target() {
                        break uid;
                    }

                    sleep(Duration::from_millis(50));
                };

                let _ = response.send(uid);
            }

            NfcReaderRequest::WaitForCardAbsent(response) => {
                loop {
                    if response.is_closed() {
                        continue 'main_loop;
                    }

                    if adapter.check_for_release() {
                        break;
                    }

                    sleep(Duration::from_millis(50));
                }

                let _ = response.send(());
            }

            NfcReaderRequest::ReadData(response) => {
                let data = match adapter.read_data() {
                    Ok(data) => data,
                    Err(err) => {
                        warn!("Failed to read data: {:?}", err);
                        let _ = response.send(None);
                        continue 'main_loop;
                    }
                };

                let _ = response.send(Some(data));
            }
        }
    }

    Ok(())
}

struct Adapter<COMM: Interface> {
    mfrc522: Mfrc522<COMM, Initialized>,
}

impl<E, COMM: Interface<Error = E>> Adapter<COMM> {
    pub fn new(mfrc522: Mfrc522<COMM, Initialized>) -> Self {
        Self { mfrc522 }
    }

    pub fn select_target(&mut self) -> Option<NfcUid> {
        let Ok(atqa) = self.mfrc522.reqa() else {
            return None;
        };

        let Ok(uid) = self.mfrc522.select(&atqa) else {
            return None;
        };

        Some(uid.into())
    }

    pub fn check_for_release(&mut self) -> bool {
        // For some bizarre reason, the MFRC522 chip switches between found and not
        // found state, so we have to check twice. This is documented in multiple
        // issues of several libraries.
        //
        // See: https://github.com/pimylifeup/MFRC522-python/issues/15#issuecomment-511671924
        if self.mfrc522.wupa().is_ok() {
            return false;
        }

        match self.mfrc522.wupa() {
            Ok(_) => false,
            Err(err) => !matches!(err, mfrc522::Error::Collision),
        }
    }

    pub fn read_data(&mut self) -> Result<String> {
        let mut ndef_message_parser = NdefMessageParser::new();

        let capabilities = self
            .mfrc522
            .mf_read(3)
            .map_err(|_| anyhow!("Failed to read block 3"))?;

        let total_bytes = (capabilities[2] as usize) * 8;
        let total_pages = total_bytes / 4;
        let total_quads = (total_pages / 4) as u8;

        for quad in 1..total_quads + 1 {
            let quad_data = self
                .mfrc522
                .mf_read(4 * quad)
                .map_err(|_| anyhow!("Failed to read block {}", 4 * quad))?;
            ndef_message_parser.add_data(&quad_data);

            if ndef_message_parser.is_done() {
                break;
            }

            if quad == 2 && !ndef_message_parser.has_started() {
                return Err(anyhow!("No NDEF message found in first sector"));
            }
        }

        if !ndef_message_parser.is_done() {
            return Err(anyhow!("NDEF message incomplete"));
        }

        let record = parse_ndef_text_record(ndef_message_parser.data.as_slice())?;
        record.text()
    }
}
