use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy)]
pub enum Button {
    VolumeUp,
    VolumeDown,
}

pub type ButtonReceiver = mpsc::Receiver<Button>;
