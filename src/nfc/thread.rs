use linux_embedded_hal as hal;

use crate::etc_config::NfcConfig;
use hal::spidev::{SpiModeFlags, SpidevOptions};
use hal::Spidev;
use mfrc522::Mfrc522;
use rppal::gpio::Gpio;
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::oneshot::error::TryRecvError;

use crate::nfc::reader::{NfcReader, Uid};

#[derive(Debug)]
pub enum NfcCommand {
    Poll {
        responder: oneshot::Sender<Uid>,
        cancel_rx: oneshot::Receiver<()>,
    },
    Read {
        responder: oneshot::Sender<Option<String>>,
    },
    Release {
        responder: oneshot::Sender<()>,
        cancel_rx: oneshot::Receiver<()>,
    },
}

pub fn start_nfc_listener(mut nfc_rx: mpsc::Receiver<NfcCommand>, config: NfcConfig) {
    thread::spawn(move || {
        let mut spi = Spidev::open(config.device).unwrap();
        let options = SpidevOptions::new()
            .max_speed_hz(config.max_speed)
            .mode(SpiModeFlags::SPI_MODE_0)
            .build();
        spi.configure(&options).unwrap();
        let mfrc522 = Mfrc522::new(spi).unwrap();
        let mut nfc_reader = NfcReader::new(mfrc522);

        let mut reset_pin = Gpio::new()
            .unwrap()
            .get(config.reset_pin)
            .unwrap()
            .into_output_low();
        sleep(Duration::from_nanos(150));
        reset_pin.set_high();
        sleep(Duration::from_micros(50));

        'command: while let Some(command) = nfc_rx.blocking_recv() {
            use NfcCommand::*;

            match command {
                Poll {
                    responder,
                    mut cancel_rx,
                } => {
                    let uid = loop {
                        if cancel_rx.try_recv() != Err(TryRecvError::Empty) {
                            continue 'command;
                        }

                        if let Some(uid) = nfc_reader.select_target() {
                            break uid;
                        }

                        sleep(Duration::from_millis(150));
                    };

                    let _ = responder.send(uid);
                }
                Read { responder } => {
                    let result = nfc_reader.read_first_plain_text_ndef_record();

                    match result {
                        Ok(value) => responder.send(Some(value)).unwrap(),
                        _ => {
                            let _ = responder.send(None);
                        }
                    }
                }
                Release {
                    responder,
                    mut cancel_rx,
                } => {
                    loop {
                        if cancel_rx.try_recv() != Err(TryRecvError::Empty) {
                            return;
                        }

                        if nfc_reader.check_for_release() {
                            break;
                        }

                        sleep(Duration::from_millis(150));
                    }

                    let _ = responder.send(());
                }
            }
        }
    });
}
