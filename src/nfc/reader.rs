use anyhow::{anyhow, Result};
use hex::{decode_to_slice, encode, FromHex, FromHexError};
use mfrc522::comm::Interface;
use mfrc522::{Initialized, Mfrc522};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::nfc::ndef::{parse_ndef_text_record, NdefMessageParser};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NfcUid([u8; 7]);

impl NfcUid {
    pub fn as_bytes(&self) -> &[u8; 7] {
        &self.0
    }
}

impl<'de> Deserialize<'de> for NfcUid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self(hex::deserialize(deserializer)?))
    }
}

impl Serialize for NfcUid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Ok(serializer.serialize_str(&encode(&self.0))?)
    }
}

impl FromHex for NfcUid {
    type Error = FromHexError;

    fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
        let mut out = Self([0; 7]);
        decode_to_slice(hex, &mut out.0 as &mut [u8])?;
        Ok(out)
    }
}

pub struct NfcReader<COMM: Interface> {
    mfrc522: Mfrc522<COMM, Initialized>,
}

impl<E, COMM: Interface<Error = E>> NfcReader<COMM> {
    pub fn new(mfrc522: Mfrc522<COMM, Initialized>) -> Self {
        Self { mfrc522 }
    }

    pub fn select_target(&mut self) -> Option<NfcUid> {
        let atqa = match self.mfrc522.reqa() {
            Ok(atqa) => atqa,
            Err(_) => return None,
        };

        let raw_uid = match self.mfrc522.select(&atqa) {
            Ok(uid) => uid,
            Err(_) => return None,
        };

        let mut uid: [u8; 7] = [0; 7];

        match raw_uid {
            mfrc522::Uid::Single(raw_uid) => uid[0..4].copy_from_slice(raw_uid.as_bytes()),
            mfrc522::Uid::Double(raw_uid) => uid.copy_from_slice(raw_uid.as_bytes()),
            mfrc522::Uid::Triple(raw_uid) => uid.copy_from_slice(&raw_uid.as_bytes()[0..7]),
        }

        Some(NfcUid(uid))
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
            Err(e) => !matches!(e, mfrc522::error::Error::Collision),
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
