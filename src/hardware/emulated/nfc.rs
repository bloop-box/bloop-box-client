use crate::hardware::emulated::ui::EmulatedCard;
use crate::hardware::nfc::NfcReaderRequest;
use anyhow::{Error, Result};
use tokio::sync::{mpsc, watch};
use tokio_graceful_shutdown::{FutureExt, IntoSubsystem, SubsystemHandle};

#[derive(Debug)]
pub struct NfcReaderTask {
    request_rx: mpsc::Receiver<NfcReaderRequest>,
    ui_rx: watch::Receiver<Option<EmulatedCard>>,
}

impl NfcReaderTask {
    pub fn new(
        cmd_rx: mpsc::Receiver<NfcReaderRequest>,
        ui_rx: watch::Receiver<Option<EmulatedCard>>,
    ) -> Self {
        Self {
            request_rx: cmd_rx,
            ui_rx,
        }
    }

    async fn process(&mut self) -> Result<()> {
        while let Some(request) = self.request_rx.recv().await {
            match request {
                NfcReaderRequest::WaitForCardPresent(response) => loop {
                    if let Some(card) = self.ui_rx.borrow().clone() {
                        let _ = response.send(card.uid);
                        break;
                    }

                    let Ok(_) = self.ui_rx.changed().await else {
                        break;
                    };
                },

                NfcReaderRequest::WaitForCardAbsent(response) => loop {
                    if self.ui_rx.borrow().is_none() {
                        let _ = response.send(());
                        break;
                    }

                    let Ok(_) = self.ui_rx.changed().await else {
                        break;
                    };
                },

                NfcReaderRequest::ReadData(response) => match self.ui_rx.borrow().clone() {
                    Some(card) => {
                        let _ = response.send(Some(card.data));
                    }
                    None => {
                        let _ = response.send(None);
                    }
                },
            }
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl IntoSubsystem<Error> for NfcReaderTask {
    async fn run(mut self, subsys: SubsystemHandle) -> Result<()> {
        if let Ok(result) = self.process().cancel_on_shutdown(&subsys).await {
            result?;
        }

        Ok(())
    }
}
