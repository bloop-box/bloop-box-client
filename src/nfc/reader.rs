use anyhow::{anyhow, Result};
use nfc1::target_info::TargetInfo;
use nfc1::BaudRate::Baud106;
use nfc1::Error::Timeout as TimeoutError;
use nfc1::ModulationType::Iso14443a;
use nfc1::Property::EasyFraming;
use nfc1::{Modulation, Property, Timeout};

use crate::nfc::ndef::{parse_ndef_text_record, NdefMessageParser};

pub type Uid = [u8; 7];

pub struct NfcReader<'a> {
    device: nfc1::Device<'a>,
}

impl<'a> NfcReader<'a> {
    pub fn new(device: nfc1::Device<'a>) -> Self {
        Self { device }
    }

    pub fn select_target(&mut self) -> Option<Uid> {
        self.device
            .set_property_bool(Property::InfiniteSelect, false)
            .unwrap();
        let result = self.device.initiator_select_passive_target(&Modulation {
            modulation_type: Iso14443a,
            baud_rate: Baud106,
        });

        let info = match result {
            Ok(target) => match target.target_info {
                TargetInfo::Iso14443a(info) => info,
                _ => return None,
            },
            Err(TimeoutError {}) => return None,
            Err(other) => panic!("Failed to select target: {:?}", other),
        };

        if info.uid == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0] {
            return None;
        }

        Some(info.uid[0..7].try_into().unwrap())
    }

    pub fn check_for_release(&mut self) -> bool {
        self.device.initiator_target_is_present_any().is_err()
    }

    pub fn read_first_plain_text_ndef_record(&mut self) -> Result<String> {
        let mut ndef_message_parser = NdefMessageParser::new();

        self.check_version()?;
        self.device.set_property_bool(EasyFraming, true)?;
        let capabilities = self.read_block(3)?;

        let total_bytes = (capabilities[2] as u32) * 8;
        let total_pages = total_bytes / 4;
        let total_quads = (total_pages / 4) as u8;

        for quad in 1..total_quads + 1 {
            let quad_data = self.read_block(4 * quad)?;
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

    fn check_version(&mut self) -> Result<()> {
        self.device.set_property_bool(EasyFraming, false)?;
        let version = self
            .device
            .initiator_transceive_bytes(&[0x60], 8, Timeout::Default)?;

        if version[0..6] != [0x00, 0x04, 0x04, 0x02, 0x01, 0x00] {
            return Err(anyhow!("Version mismatch, unsupported tag"));
        }

        if ![0x0f, 0x11, 0x13].contains(&version[6]) {
            return Err(anyhow!("Version mismatch, unsupported subtype of tag"));
        }

        if version[7] != 0x03 {
            return Err(anyhow!("Version mismatch, unsupported protocol"));
        }

        Ok(())
    }

    fn read_block(&mut self, block_number: u8) -> Result<[u8; 16]> {
        let packet: [u8; 2] = [0x30, block_number];

        let response = self
            .device
            .initiator_transceive_bytes(&packet, 16, Timeout::Default)?;
        Ok(response[..].try_into()?)
    }
}
