extern crate core;

use anyhow::{anyhow, Result};

// References:
//
// NDEF parser: https://github.com/TapTrack/NdefLibrary
// NDEF message spec: https://www.netes.com.tr/netes/dosyalar/dosya/B6159F60458582512B16EF1263ADE707.pdf
// Mifare reader: https://github.com/hackeriet/pyhackeriet
// Mifare specs: https://www.puntoflotante.net/TUTORIAL-RFID-ISO-14443A-TAGS-13.56-MHZ.htm

#[derive(PartialEq)]
enum NdefMessageParserState {
    Init,
    Length,
    Value,
}

pub struct NdefMessageParser {
    state: NdefMessageParserState,
    length: i32,
    pub data: Vec<u8>,
}

impl NdefMessageParser {
    pub fn new() -> Self {
        Self {
            state: NdefMessageParserState::Init,
            length: -1,
            data: Vec::new(),
        }
    }

    pub fn add_data(&mut self, data: &[u8]) {
        for byte in data {
            match self.state {
                NdefMessageParserState::Init => {
                    if *byte == 0x00 {
                        continue;
                    }

                    if *byte == 0x03 {
                        self.state = NdefMessageParserState::Length;
                    }
                }

                NdefMessageParserState::Length => {
                    if self.length == -1 {
                        if *byte == 0xff {
                            self.length = -2;
                        } else {
                            self.length = *byte as i32;
                            self.state = NdefMessageParserState::Value;
                        }

                        continue;
                    }

                    if self.length == -2 {
                        self.length = *byte as i32;
                    } else {
                        self.length = (self.length << 8) & *byte as i32;
                        self.state = NdefMessageParserState::Value;
                    }
                }

                NdefMessageParserState::Value => {
                    self.data.push(*byte);

                    if self.data.len() as i32 == self.length {
                        return;
                    }
                }
            }
        }
    }

    pub fn is_done(&self) -> bool {
        self.data.len() as i32 == self.length
    }

    pub fn has_started(&self) -> bool {
        self.state != NdefMessageParserState::Init
    }
}

pub struct NdefTextRecord {
    _id: Vec<u8>,
    value: Vec<u8>,
}

impl NdefTextRecord {
    pub fn text(&self) -> Result<String> {
        if self.value.is_empty() {
            return Ok(String::new());
        }

        let code_length = self.value[0] & 0b00111111;
        Ok(String::from_utf8(
            self.value[(code_length as usize + 1)..].to_vec(),
        )?)
    }
}

pub fn parse_ndef_text_record(data: &Vec<u8>) -> Result<NdefTextRecord> {
    let record_length = data.len();
    let mut index = 0;

    if record_length <= index {
        return Err(anyhow!("Flags missing"));
    }

    let flags = data[index];

    let _is_message_begin = (flags & 0b10000000) != 0;
    let _is_message_end = (flags & 0b01000000) != 0;
    let is_chunked = (flags & 0b00100000) != 0;
    let is_short_record = (flags & 0b00010000) != 0;
    let has_id_length = (flags & 0b00001000) != 0;
    let type_name_format = flags & 0b00000111;

    if is_chunked {
        return Err(anyhow!("Chunked records are not supported"));
    }

    index += 1;

    if record_length <= index {
        return Err(anyhow!("Type length missing"));
    }

    let type_length = data[index];

    index += 1;

    let payload_length = if is_short_record {
        if record_length <= index {
            return Err(anyhow!("Payload length missing"));
        }

        let payload_length_index = index;
        index += 1;
        data[payload_length_index] as u32
    } else {
        if record_length <= index + 3 {
            return Err(anyhow!("Payload length missing"));
        }

        let payload_length_index = index;
        index += 4;
        u32::from_be_bytes(data[payload_length_index..(payload_length_index + 4)].try_into()?)
    };

    let id_length = if has_id_length {
        if record_length <= index {
            return Err(anyhow!("ID length missing"));
        }

        let id_length_index = index;
        index += 1;
        data[id_length_index]
    } else {
        0
    };

    let payload_type = if type_length > 0 {
        let type_index = index;
        index += type_length as usize;

        if record_length < type_index + type_length as usize {
            return Err(anyhow!("Type missing"));
        }

        data[type_index..(type_index + type_length as usize)].to_vec()
    } else {
        vec![]
    };

    let payload_id = if id_length > 0 {
        let id_index = index;
        index += id_length as usize;

        if record_length < id_index + id_length as usize {
            return Err(anyhow!("ID missing"));
        }

        data[id_index..(id_index + id_length as usize)].to_vec()
    } else {
        vec![]
    };

    let payload_value = if payload_length > 0 {
        let value_index = index;

        if record_length < value_index + payload_length as usize {
            return Err(anyhow!("Payload missing"));
        }

        data[value_index..(value_index + payload_length as usize)].to_vec()
    } else {
        vec![]
    };

    if type_name_format != 1 {
        return Err(anyhow!("Not a well known payload type"));
    }

    if payload_type != [0x54] {
        return Err(anyhow!("Not a text record"));
    }

    Ok(NdefTextRecord {
        _id: payload_id,
        value: payload_value,
    })
}
