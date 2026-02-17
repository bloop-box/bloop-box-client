use crate::hardware::nfc::NfcUid;
use anyhow::anyhow;
use bitmask_enum::bitmask;
use byteorder::{LittleEndian, ReadBytesExt};
use serde::{Deserialize, Deserializer, Serialize};
use std::io::{self, Cursor, Read};
use std::net::IpAddr;
use hex::{decode_to_slice, FromHex, FromHexError};
use uuid::Uuid;
use crate::network::AudioResponse::Data;

#[derive(Debug, Clone)]
pub struct Message {
    message_type: u8,
    payload: Vec<u8>,
}

impl Message {
    pub fn new(message_type: u8, payload: Vec<u8>) -> Self {
        Self {
            message_type,
            payload,
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(1 + 4 + self.payload.len());
        bytes.push(self.message_type);
        bytes.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
        bytes.extend(self.payload);

        bytes
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataHash(Vec<u8>);

impl From<Vec<u8>> for DataHash {
    fn from(value: Vec<u8>) -> Self {
        Self(value)
    }
}

impl<'de> Deserialize<'de> for DataHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex = String::deserialize(deserializer)?;
        DataHash::from_hex(&hex).map_err(serde::de::Error::custom)
    }
}

impl Serialize for DataHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&hex::encode(self.as_bytes()))
    }
}

impl FromHex for DataHash {
    type Error = FromHexError;

    fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
        let hex_bytes = hex.as_ref();
        let mut decoded = vec![0u8; hex_bytes.len() / 2];
        decode_to_slice(hex, &mut decoded)?;

        Ok(DataHash::from(decoded))
    }
}

impl DataHash {
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }

    fn into_tagged_bytes(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.0.len() + 1);
        bytes.push(self.0.len() as u8);
        bytes.extend(self.0);

        bytes
    }

    fn from_cursor_opt(cursor: &mut Cursor<Vec<u8>>) -> Result<Option<Self>, io::Error> {
        let length = cursor.read_u8()?;

        if length == 0 {
            return Ok(None);
        }
        let mut bytes = vec![0; length as usize];
        cursor.read_exact(&mut bytes)?;

        Ok(Some(Self(bytes)))
    }
}

#[bitmask(u64)]
#[bitmask_config(vec_debug)]
pub enum Capabilities {
    PreloadCheck = 0x1,
}

#[derive(Debug)]
pub enum ClientMessage {
    ClientHandshake {
        min_version: u8,
        max_version: u8,
    },
    Authentication {
        client_id: String,
        client_secret: String,
        ip_addr: IpAddr,
    },
    Ping,
    Quit,
    Bloop {
        nfc_uid: NfcUid,
    },
    RetrieveAudio {
        achievement_id: Uuid,
    },
    PreloadCheck {
        audio_manifest_hash: Option<DataHash>,
    },
}

impl From<ClientMessage> for Message {
    fn from(client_message: ClientMessage) -> Message {
        match client_message {
            ClientMessage::ClientHandshake {
                min_version,
                max_version,
            } => Message::new(0x01, vec![min_version, max_version]),
            ClientMessage::Authentication {
                client_id,
                client_secret,
                ip_addr,
            } => {
                let ip_addr_bytes = match ip_addr {
                    IpAddr::V4(addr) => {
                        let mut bytes = Vec::with_capacity(5);
                        bytes.push(4);
                        bytes.extend_from_slice(&addr.octets());
                        bytes
                    }
                    IpAddr::V6(addr) => {
                        let mut bytes = Vec::with_capacity(17);
                        bytes.push(6);
                        bytes.extend_from_slice(&addr.octets());
                        bytes
                    }
                };

                let mut payload = Vec::with_capacity(
                    1 + client_id.len() + 1 + client_secret.len() + 1 + ip_addr_bytes.len(),
                );
                payload.push(client_id.len() as u8);
                payload.extend(client_id.as_bytes());
                payload.push(client_secret.len() as u8);
                payload.extend(client_secret.as_bytes());
                payload.extend(ip_addr_bytes);

                Message::new(0x03, payload)
            }
            ClientMessage::Ping => Message::new(0x05, vec![]),
            ClientMessage::Quit => Message::new(0x07, vec![]),
            ClientMessage::Bloop { nfc_uid } => Message::new(0x08, nfc_uid.as_tagged_bytes()),
            ClientMessage::RetrieveAudio { achievement_id } => {
                Message::new(0x0a, achievement_id.into())
            }
            ClientMessage::PreloadCheck {
                audio_manifest_hash,
            } => {
                let payload = match audio_manifest_hash {
                    Some(hash) => hash.into_tagged_bytes(),
                    None => vec![0],
                };

                Message::new(0x0c, payload)
            }
        }
    }
}

#[derive(Debug)]
pub struct AchievementRecord {
    pub id: Uuid,
    pub audio_file_hash: Option<DataHash>,
}

impl AchievementRecord {
    pub fn filename(&self) -> anyhow::Result<String> {
        let audio_file_hash = self
            .audio_file_hash
            .as_ref()
            .ok_or(anyhow!("achievement has no audio file hash"))?;
        Ok(format!(
            "{}-{}.mp3",
            hex::encode(self.id),
            hex::encode(audio_file_hash.as_bytes())
        ))
    }
}

#[derive(Debug)]
pub enum ErrorResponse {
    UnexpectedMessage,
    MalformedMessage,
    UnsupportedVersionRange,
    InvalidCredentials,
    UnknownNfcUid,
    NfcUidThrottled,
    AudioUnavailable,
}

impl TryFrom<u8> for ErrorResponse {
    type Error = io::Error;

    fn try_from(code: u8) -> Result<Self, Self::Error> {
        match code {
            0 => Ok(Self::UnexpectedMessage),
            1 => Ok(Self::MalformedMessage),
            2 => Ok(Self::UnsupportedVersionRange),
            3 => Ok(Self::InvalidCredentials),
            4 => Ok(Self::UnknownNfcUid),
            5 => Ok(Self::NfcUidThrottled),
            6 => Ok(Self::AudioUnavailable),
            code => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown error code: {code}"),
            )),
        }
    }
}

#[derive(Debug)]
pub enum ServerMessage {
    Error(ErrorResponse),
    ServerHandshake {
        accepted_version: u8,
        capabilities: Capabilities,
    },
    AuthenticationAccepted,
    Pong,
    BloopAccepted {
        achievements: Vec<AchievementRecord>,
    },
    AudioData {
        data: Vec<u8>,
    },
    PreloadMatch,
    PreloadMismatch {
        audio_manifest_hash: DataHash,
        achievements: Vec<AchievementRecord>,
    },
}

impl TryFrom<Message> for ServerMessage {
    type Error = io::Error;

    fn try_from(message: Message) -> Result<Self, <Self as TryFrom<Message>>::Error> {
        let mut cursor = Cursor::new(message.payload);

        match message.message_type {
            0x00 => {
                let error_code = cursor.read_u8()?;

                Ok(Self::Error(ErrorResponse::try_from(error_code)?))
            }
            0x02 => {
                let accepted_version = cursor.read_u8()?;
                let capabilities = cursor.read_u64::<LittleEndian>()?;

                Ok(Self::ServerHandshake {
                    accepted_version,
                    capabilities: Capabilities::from(capabilities),
                })
            }
            0x04 => Ok(Self::AuthenticationAccepted),
            0x06 => Ok(Self::Pong),
            0x09 => {
                let num_achievements = cursor.read_u8()?;
                let achievements =
                    read_achievement_recordset(num_achievements as usize, &mut cursor)?;

                Ok(Self::BloopAccepted { achievements })
            }
            0x0b => {
                let data_length = cursor.read_u32::<LittleEndian>()? as usize;
                let mut data = vec![0; data_length];
                cursor.read_exact(&mut data)?;

                Ok(Self::AudioData { data })
            }
            0x0d => Ok(Self::PreloadMatch),
            0x0e => {
                let audio_manifest_hash = DataHash::from_cursor_opt(&mut cursor)?.ok_or(
                    io::Error::new(io::ErrorKind::InvalidData, "missing audio manifest hash"),
                )?;

                let num_achievements = cursor.read_u32::<LittleEndian>()?;
                let achievements =
                    read_achievement_recordset(num_achievements as usize, &mut cursor)?;

                Ok(Self::PreloadMismatch {
                    audio_manifest_hash,
                    achievements,
                })
            }
            code => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown message type: {code}"),
            )),
        }
    }
}

fn read_achievement_recordset(
    num_achievements: usize,
    cursor: &mut Cursor<Vec<u8>>,
) -> Result<Vec<AchievementRecord>, io::Error> {
    let mut achievements = Vec::with_capacity(num_achievements);

    for _ in 0..num_achievements {
        let mut uuid_bytes = [0; 16];
        cursor.read_exact(&mut uuid_bytes)?;
        let id = Uuid::from_bytes(uuid_bytes);
        let audio_file_hash = DataHash::from_cursor_opt(cursor)?;
        achievements.push(AchievementRecord {
            id,
            audio_file_hash,
        });
    }

    Ok(achievements)
}
