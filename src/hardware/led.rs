use thiserror::Error;
use tokio::sync::mpsc;

#[allow(dead_code)]
#[derive(Debug)]
pub enum Color {
    Red,
    Green,
    Blue,
    Yellow,
    Magenta,
    Cyan,
}

impl Color {
    pub(super) fn rgb(&self) -> (u8, u8, u8) {
        match self {
            Self::Red => (255, 0, 0),
            Self::Green => (0, 255, 0),
            Self::Blue => (0, 0, 255),
            Self::Yellow => (255, 255, 0),
            Self::Magenta => (255, 0, 255),
            Self::Cyan => (0, 255, 255),
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("LED controller task is no longer running")]
    Disconnected,
}

#[derive(Debug, Clone)]
pub struct LedController {
    tx: mpsc::Sender<LedState>,
}

impl LedController {
    pub(super) fn new(tx: mpsc::Sender<LedState>) -> Self {
        Self { tx }
    }

    pub async fn set_static(&self, color: Color) -> Result<(), Error> {
        self.tx
            .send(LedState::Static(color))
            .await
            .map_err(|_| Error::Disconnected)
    }

    pub async fn set_breathing(&self, color: Color) -> Result<(), Error> {
        self.tx
            .send(LedState::Breathing(color))
            .await
            .map_err(|_| Error::Disconnected)
    }
}

#[derive(Debug)]
pub(super) enum LedState {
    Static(Color),
    Breathing(Color),
}
