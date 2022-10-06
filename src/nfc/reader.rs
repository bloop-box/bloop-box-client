use anyhow::{anyhow, Result};
use nfc1::BaudRate::Baud106;
use nfc1::{Modulation, Timeout};
use nfc1::ModulationType::Iso14443a;
use nfc1::target_info::TargetInfo;

use crate::nfc::ndef::{NdefMessageParser, parse_ndef_text_record};

pub type Uid = [u8; 4];

#[derive(Copy, Clone)]
struct AuthOption {
    key_type: u8,
    key : [u8; 6],
}

pub struct NfcReader<'a> {
    device: nfc1::Device<'a>,
}

const KEY_TYPE_A: u8 = 0x60;
const KEY_TYPE_B: u8 = 0x61;
const DEFAULT_KEY_A: [u8; 6] = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff];
const DEFAULT_KEY_B: [u8; 6] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

const AUTH_OPTIONS: [AuthOption; 4] = [
    AuthOption { key_type: KEY_TYPE_A, key: DEFAULT_KEY_A },
    AuthOption { key_type: KEY_TYPE_A, key: DEFAULT_KEY_B },
    AuthOption { key_type: KEY_TYPE_B, key: DEFAULT_KEY_A },
    AuthOption { key_type: KEY_TYPE_B, key: DEFAULT_KEY_B },
];

impl<'a> NfcReader<'a> {
    pub fn new(device: nfc1::Device<'a>) -> Self {
        Self { device }
    }

    pub fn select_target(&mut self) -> Option<Uid> {
        let result = self.device.initiator_select_passive_target(
            &Modulation { modulation_type: Iso14443a, baud_rate: Baud106 },
        );

        let info = match result {
            Ok(target) => match target.target_info {
                TargetInfo::Iso14443a(info) => info,
                _ => return None,
            },
            Err(other) => panic!("Failed to select target: {:?}", other),
        };

        if info.uid == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0] {
            return None;
        }

        Some(info.uid[0..4].try_into().unwrap())
    }

    pub fn check_for_release(&mut self) -> bool {
        self.device.initiator_target_is_present_any().is_err()
    }

    pub fn read_first_plain_text_ndef_record(&mut self, uid: &Uid) -> Result<String> {
        let mut ndef_message_parser = NdefMessageParser::new();
        let mut maybe_auth_option: Option<AuthOption> = None;

        'sector: for sector in 1..16 {
            match maybe_auth_option {
                Some(auth_option) => {
                    self.authenticate_block(
                        sector * 4 + 3,
                        auth_option.key_type,
                        &auth_option.key,
                        &uid
                    )?;
                },
                None => {
                    let auth_option = self.try_authenticate_block(sector * 4 + 3, &uid)?;
                    maybe_auth_option = Some(auth_option);
                },
            }

            for block in 0..3 {
                let block_data = self.read_block(sector * 4 + block)?;
                ndef_message_parser.add_data(&block_data);

                if ndef_message_parser.is_done() {
                    break 'sector;
                }
            }

            if !ndef_message_parser.has_started() {
                return Err(anyhow!("No NDEF message found in first sector"));
            }
        }

        if !ndef_message_parser.is_done() {
            return Err(anyhow!("NDEF message incomplete"));
        }

        let record = parse_ndef_text_record(&ndef_message_parser.data)?;
        Ok(record.text()?)
    }

    fn read_block(&mut self, block_number: u8) -> Result<[u8; 16]> {
        let packet: [u8; 2] = [
            0x30,
            block_number,
        ];

        let response = self.device.initiator_transceive_bytes(&packet, 16, Timeout::Default)?;
        Ok(response[..].try_into()?)
    }

    fn try_authenticate_block(&mut self, block_number: u8, uid: &[u8; 4]) -> Result<AuthOption> {
        for auth_option in AUTH_OPTIONS {
            let auth_result = self.authenticate_block(block_number, auth_option.key_type, &auth_option.key, uid);

            match auth_result {
                Ok(()) => {
                    return Ok(auth_option);
                },
                Err(_) => {
                    self.device.initiator_select_passive_target(
                        &Modulation { modulation_type: Iso14443a, baud_rate: Baud106 },
                    )?;
                },
            }
        }

        Err(anyhow!("No key matched sector"))
    }

    fn authenticate_block(&mut self, block_number: u8, key_type: u8, key: &[u8; 6], uid: &[u8; 4]) -> Result<()> {
        let packet: [u8; 12] = [
            key_type,
            block_number,
            key[0],
            key[1],
            key[2],
            key[3],
            key[4],
            key[5],
            uid[0],
            uid[1],
            uid[2],
            uid[3],
        ];

        self.device.initiator_transceive_bytes(&packet, 0, Timeout::Default)?;

        Ok(())
    }
}
