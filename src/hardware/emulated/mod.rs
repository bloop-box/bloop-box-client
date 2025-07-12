use crate::hardware::emulated::led::LedControllerTask;
use crate::hardware::emulated::nfc::NfcReaderTask;
use crate::hardware::emulated::ui::{run_ui, UiChannels};
use crate::hardware::led::LedController;
use crate::hardware::nfc::NfcReader;
use crate::hardware::{InitSubsystems, Peripherals, StartSubsystems};
use crate::thread::SupervisedThread;
use anyhow::Result;
use egui::Color32;
use std::panic::AssertUnwindSafe;
use tokio::sync::{mpsc, watch};
use tokio_graceful_shutdown::{IntoSubsystem, SubsystemBuilder, SubsystemHandle};
use tokio_util::sync::CancellationToken;

pub mod asset;
mod led;
mod nfc;
pub mod system;
mod ui;

pub struct HardwareContext {
    pub peripherals: Peripherals,
    pub threads: Vec<SupervisedThread>,
    pub init_subsystems: InitSubsystems,
    pub run_ui: Box<dyn FnOnce() -> Result<()>>,
}

pub fn init_hardware(shutdown_token: CancellationToken) -> Result<HardwareContext> {
    let (led_state_tx, led_state_rx) = mpsc::channel(32);
    let (button_tx, button_rx) = mpsc::channel(32);
    let (nfc_reader_tx, nfc_reader_rx) = mpsc::channel(32);
    let (led_ui_tx, led_ui_rx) = watch::channel(Color32::BLACK);
    let (emulated_card_tx, emulated_card_rx) = watch::channel(None);

    let ui_channels = UiChannels {
        button_tx,
        led_color_rx: led_ui_rx,
        emulated_card_tx,
    };

    let peripherals = Peripherals {
        led_controller: LedController::new(led_state_tx),
        nfc_reader: NfcReader::new(nfc_reader_tx),
        button_receiver: button_rx,
    };

    let led_ui_tx = AssertUnwindSafe(led_ui_tx);
    let emulated_card_rx = AssertUnwindSafe(emulated_card_rx);

    let init_subsystems = Box::new(move || -> Result<StartSubsystems> {
        let led_controller = LedControllerTask::new(led_state_rx, led_ui_tx.clone());
        let nfc_reader = NfcReaderTask::new(nfc_reader_rx, emulated_card_rx.clone());

        Ok(Box::new(move |s: &SubsystemHandle| {
            s.start(SubsystemBuilder::new(
                "LedController",
                led_controller.into_subsystem(),
            ));
            s.start(SubsystemBuilder::new(
                "NfcReader",
                nfc_reader.into_subsystem(),
            ));
        }))
    });

    Ok(HardwareContext {
        peripherals,
        threads: vec![],
        init_subsystems,
        run_ui: Box::new(move || run_ui(shutdown_token, ui_channels)),
    })
}
