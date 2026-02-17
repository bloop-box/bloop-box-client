use hex::{decode_to_slice, FromHex, FromHexError};
#[cfg(not(feature = "hardware-emulation"))]
use mfrc522::Uid;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use std::fmt::Display;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Error)]
pub enum NfcUidError {
    #[error("Invalid UID length, must be 4, 7 or 10")]
    InvalidLength,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NfcUid {
    Single([u8; 4]),
    Double([u8; 7]),
    Triple([u8; 10]),
}

impl Display for NfcUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NfcUid::Single(data) => write!(
                f,
                "{:02X}:{:02X}:{:02X}:{:02X}",
                data[0], data[1], data[2], data[3]
            ),
            NfcUid::Double(data) => write!(
                f,
                "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                data[0], data[1], data[2], data[3], data[4], data[5]
            ),
            NfcUid::Triple(data) => write!(
                f,
                "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8]
            ),
        }
    }
}

impl<'de> Deserialize<'de> for NfcUid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex = String::deserialize(deserializer)?;
        NfcUid::from_hex(&hex).map_err(serde::de::Error::custom)
    }
}

impl Serialize for NfcUid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&hex::encode(self.as_bytes()))
    }
}

#[cfg(not(feature = "hardware-emulation"))]
impl From<Uid> for NfcUid {
    fn from(uid: Uid) -> Self {
        match uid {
            Uid::Single(bytes) => Self::Single(bytes.as_bytes().try_into().unwrap()),
            Uid::Double(bytes) => Self::Double(bytes.as_bytes().try_into().unwrap()),
            Uid::Triple(bytes) => Self::Triple(bytes.as_bytes().try_into().unwrap()),
        }
    }
}

impl TryFrom<&[u8]> for NfcUid {
    type Error = NfcUidError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match value.len() {
            4 => Ok(NfcUid::Single(value.try_into().unwrap())),
            7 => Ok(NfcUid::Double(value.try_into().unwrap())),
            10 => Ok(NfcUid::Triple(value.try_into().unwrap())),
            _ => Err(NfcUidError::InvalidLength),
        }
    }
}

impl FromHex for NfcUid {
    type Error = FromHexError;

    fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
        let hex_bytes = hex.as_ref();
        let mut decoded = vec![0u8; hex_bytes.len() / 2];
        decode_to_slice(hex, &mut decoded)?;

        NfcUid::try_from(decoded.as_slice()).map_err(|_| FromHexError::InvalidStringLength)
    }
}

impl NfcUid {
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            NfcUid::Single(data) => data,
            NfcUid::Double(data) => data,
            NfcUid::Triple(data) => data,
        }
    }

    pub fn as_tagged_bytes(&self) -> Vec<u8> {
        let raw_bytes: Vec<u8> = match self {
            NfcUid::Single(data) => data.into(),
            NfcUid::Double(data) => data.into(),
            NfcUid::Triple(data) => data.into(),
        };

        let mut bytes = Vec::with_capacity(raw_bytes.len() + 1);
        bytes.push(raw_bytes.len() as u8);
        bytes.extend(raw_bytes);
        bytes
    }
}

#[derive(Debug, Error)]
pub enum NfcReaderError {
    #[error("NFC reader task is no longer running")]
    Disconnected,
}

#[derive(Debug, Clone)]
pub struct NfcReader {
    tx: mpsc::Sender<NfcReaderRequest>,
}

impl NfcReader {
    pub(super) fn new(tx: mpsc::Sender<NfcReaderRequest>) -> Self {
        Self { tx }
    }

    pub async fn wait_for_card(&self) -> Result<NfcUid, NfcReaderError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.tx
            .send(NfcReaderRequest::WaitForCardPresent(response_tx))
            .await
            .map_err(|_| NfcReaderError::Disconnected)?;

        response_rx.await.map_err(|_| NfcReaderError::Disconnected)
    }

    pub async fn wait_for_removal(&self) -> Result<(), NfcReaderError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.tx
            .send(NfcReaderRequest::WaitForCardAbsent(response_tx))
            .await
            .map_err(|_| NfcReaderError::Disconnected)?;

        response_rx.await.map_err(|_| NfcReaderError::Disconnected)
    }

    pub async fn read_card_data(&self) -> Result<Option<String>, NfcReaderError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.tx
            .send(NfcReaderRequest::ReadData(response_tx))
            .await
            .map_err(|_| NfcReaderError::Disconnected)?;

        response_rx.await.map_err(|_| NfcReaderError::Disconnected)
    }
}

#[derive(Debug)]
pub(super) enum NfcReaderRequest {
    WaitForCardPresent(oneshot::Sender<NfcUid>),
    WaitForCardAbsent(oneshot::Sender<()>),
    ReadData(oneshot::Sender<Option<String>>),
}
