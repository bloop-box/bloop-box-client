use std::thread;
use std::thread::sleep;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::oneshot::error::TryRecvError;

use crate::nfc::reader::{NfcReader, Uid};

#[derive(Debug)]
pub enum NfcCommand {
    Poll {
        responder: oneshot::Sender<Uid>,
    },
    Read {
        uid: Uid,
        responder: oneshot::Sender<Option<String>>,
    },
    Release {
        responder: oneshot::Sender<()>,
    },
}

pub fn start_nfc_listener(
    mut nfc_rx: mpsc::Receiver<NfcCommand>,
    mut cancel_rx: oneshot::Receiver<()>,
) {
    thread::spawn(move || {
        let mut context = nfc1::Context::new().unwrap();
        let device = context.open().unwrap();
        let mut nfc_reader = NfcReader::new(device);

        while let Some(command) = nfc_rx.blocking_recv() {
            use NfcCommand::*;

            match command {
                Poll { responder } => {
                    let uid = loop {
                        if cancel_rx.try_recv() != Err(TryRecvError::Empty) {
                            return;
                        }

                        if let Some(uid) = nfc_reader.select_target() {
                            break uid;
                        }

                        sleep(Duration::from_millis(150));
                    };

                    responder.send(uid).unwrap();
                }
                Read { uid, responder } => {
                    let result = nfc_reader.read_first_plain_text_ndef_record(&uid);

                    match result {
                        Ok(value) => responder.send(Some(value)).unwrap(),
                        _ => responder.send(None).unwrap(),
                    }
                }
                Release { responder } => {
                    loop {
                        if cancel_rx.try_recv() != Err(TryRecvError::Empty) {
                            return;
                        }

                        if nfc_reader.check_for_release() {
                            break;
                        }

                        sleep(Duration::from_millis(150));
                    }

                    responder.send(()).unwrap();
                }
            }
        }
    });
}
