use crate::hardware::nfc::NfcUid;
pub use crate::network::message::{AchievementRecord, Capabilities, DataHash};
use crate::network::task::{Command, ConnectionState};
use anyhow::Result;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

mod message;
mod skip_certificate_verification;
pub mod task;

#[derive(Debug)]
pub enum BloopResponse {
    Accepted {
        achievements: Vec<AchievementRecord>,
    },
    Throttled,
    Rejected,
}

#[derive(Debug)]
pub enum AudioResponse {
    Data(Vec<u8>),
    NotFound,
    Disconnected,
}

#[derive(Debug)]
pub enum PreloadCheckResponse {
    Match,
    Mismatch {
        audio_manifest_hash: DataHash,
        achievements: Vec<AchievementRecord>,
    },
}

#[derive(Debug)]
pub struct NetworkClient {
    tx: mpsc::Sender<Command>,
}

impl NetworkClient {
    pub fn new(tx: mpsc::Sender<Command>) -> Self {
        Self { tx }
    }

    pub async fn set_connection_state(&self, connection_state: ConnectionState) -> Result<()> {
        self.tx
            .send(Command::SetConnectionState(connection_state))
            .await?;
        Ok(())
    }

    pub async fn bloop(&self, nfc_uid: NfcUid) -> Result<BloopResponse> {
        let (response_tx, response_rx) = oneshot::channel();
        self.tx
            .send(Command::Bloop {
                nfc_uid,
                response: response_tx,
            })
            .await?;

        let result = response_rx.await?;
        Ok(result)
    }

    pub async fn retrieve_audio(&self, achievement_id: Uuid) -> Result<AudioResponse> {
        let (response_tx, response_rx) = oneshot::channel();
        self.tx
            .send(Command::RetrieveAudio {
                achievement_id,
                response: response_tx,
            })
            .await?;

        let result = response_rx.await?;
        Ok(result)
    }

    pub async fn preload_check(
        &self,
        audio_manifest_hash: Option<DataHash>,
    ) -> Result<PreloadCheckResponse> {
        let (response_tx, response_rx) = oneshot::channel();
        self.tx
            .send(Command::PreloadCheck {
                audio_manifest_hash,
                response: response_tx,
            })
            .await?;

        let result = response_rx.await?;
        Ok(result)
    }

    pub async fn shutdown(&self) -> Result<()> {
        let (response_tx, response_rx) = oneshot::channel();
        self.tx
            .send(Command::Shutdown {
                response: response_tx,
            })
            .await?;

        response_rx.await?;
        Ok(())
    }
}
