use anyhow::{anyhow, Result};
use embedded_hal as hal;
use hal::blocking::spi;
use hal::digital::v2::OutputPin;
use mfrc522::Mfrc522;

use crate::nfc::ndef::{parse_ndef_text_record, NdefMessageParser};

pub type Uid = [u8; 7];

pub struct NfcReader<SPI, NSS> {
    mfrc522: Mfrc522<SPI, NSS>,
}

impl<E, SPI, NSS> NfcReader<SPI, NSS>
where
    SPI: spi::Transfer<u8, Error = E> + spi::Write<u8, Error = E>,
    NSS: OutputPin,
{
    pub fn new(mfrc522: Mfrc522<SPI, NSS>) -> Self {
        Self { mfrc522 }
    }

    pub fn select_target(&mut self) -> Option<Uid> {
        let atqa = match self.mfrc522.reqa() {
            Ok(atqa) => atqa,
            Err(_) => return None,
        };

        let raw_uid = match self.mfrc522.select(&atqa) {
            Ok(uid) => uid,
            Err(_) => return None,
        };

        let mut uid: Uid = [0; 7];

        match raw_uid {
            mfrc522::Uid::Single(raw_uid) => uid[0..4].copy_from_slice(raw_uid.as_bytes()),
            mfrc522::Uid::Double(raw_uid) => uid.copy_from_slice(raw_uid.as_bytes()),
            mfrc522::Uid::Triple(raw_uid) => uid.copy_from_slice(&raw_uid.as_bytes()[0..7]),
        }

        Some(uid)
    }

    pub fn check_for_release(&mut self) -> bool {
        // For some bizarre reason the MFRC522 chip ping-pongs between found and not found state, so we have to check
        // twice. This is documented in multiple issue of several libraries.
        // @see https://github.com/pimylifeup/MFRC522-python/issues/15#issuecomment-511671924
        if self.mfrc522.wupa().is_ok() {
            return false;
        }

        match self.mfrc522.wupa() {
            Ok(_) => false,
            Err(e) => !matches!(e, mfrc522::Error::Collision),
        }
    }

    pub fn read_first_plain_text_ndef_record(&mut self) -> Result<String> {
        let mut ndef_message_parser = NdefMessageParser::new();

        // @todo verify that tag is NTAG and check NTAG version
        // @see https://gitlab.com/jspngh/rfid-rs/-/issues/10
        // @see https://github.com/bloop-box/bloop-box-client/blob/v1.2.1/src/nfc/reader.rs#L83-L102

        let capabilities = self
            .mfrc522
            .mf_read(3)
            .map_err(|_| anyhow!("Failed to read block {}", 3))?;

        let total_bytes = (capabilities[2] as u32) * 8;
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

        let record = parse_ndef_text_record(&ndef_message_parser.data)?;
        record.text()
    }
}
