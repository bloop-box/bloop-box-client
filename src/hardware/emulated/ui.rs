use crate::hardware::buttons::Button;
use crate::hardware::nfc::NfcUid;
use anyhow::Result;
use eframe::epaint::Color32;
use egui::{Sense, TextEdit, ViewportBuilder, ViewportCommand};
use hex::FromHex;
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct UiChannels {
    pub led_color_rx: watch::Receiver<Color32>,
    pub emulated_card_tx: watch::Sender<Option<EmulatedCard>>,
    pub button_tx: mpsc::Sender<Button>,
}

pub fn run_ui(shutdown_token: CancellationToken, channels: UiChannels) -> Result<()> {
    let viewport = ViewportBuilder::default()
        .with_inner_size([400., 400.])
        .with_resizable(false);

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Bloop Box Emulator",
        options,
        Box::new(|cc| {
            Ok(Box::new(BloopBoxEmulator::new(
                cc,
                shutdown_token,
                channels,
            )))
        }),
    )
    .map_err(|err| anyhow::anyhow!("UI crashed: {:?}", err))?;

    Ok(())
}

#[derive(Debug, Clone)]
pub struct EmulatedCard {
    pub uid: NfcUid,
    pub data: String,
}

struct BloopBoxEmulator {
    channels: UiChannels,
    scanning: bool,
    uid_input: String,
    tag_data_input: String,
    uid: Option<NfcUid>,
}

impl BloopBoxEmulator {
    fn new(
        cc: &eframe::CreationContext<'_>,
        shutdown_token: CancellationToken,
        channels: UiChannels,
    ) -> Self {
        cc.egui_ctx.set_pixels_per_point(1.2);

        let ctx = cc.egui_ctx.clone();
        let mut led_color_rx = channels.led_color_rx.clone();

        thread::spawn(move || loop {
            if shutdown_token.is_cancelled() {
                ctx.send_viewport_cmd(ViewportCommand::Close);
                break;
            }

            if led_color_rx.has_changed().unwrap_or(false) {
                led_color_rx.mark_unchanged();
                ctx.request_repaint();
            }

            sleep(Duration::from_millis(100));
        });

        Self {
            channels,
            scanning: false,
            uid_input: Default::default(),
            tag_data_input: Default::default(),
            uid: None,
        }
    }

    fn parse_uid_input(&mut self) {
        let cleaned: String = self.uid_input.chars().filter(|c| *c != ':').collect();
        let len = cleaned.len();

        if (len != 8 && len != 14 && len != 20) || !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
            self.uid = None;
            return;
        }

        self.uid = NfcUid::from_hex(&cleaned).ok();
    }
}

impl eframe::App for BloopBoxEmulator {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if ui.button("Vol -").clicked() {
                        let _ = self.channels.button_tx.blocking_send(Button::VolumeDown);
                    }

                    if ui.button("Vol +").clicked() {
                        let _ = self.channels.button_tx.blocking_send(Button::VolumeUp);
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (rect, _response) =
                        ui.allocate_exact_size(egui::vec2(24.0, 24.0), Sense::hover());
                    ui.painter()
                        .rect_filled(rect, 12.0, *self.channels.led_color_rx.borrow());

                    if ui
                        .add_enabled(self.uid.is_some(), egui::Button::new("Scan UID"))
                        .is_pointer_button_down_on()
                    {
                        if !self.scanning {
                            self.scanning = true;

                            if let Some(uid) = self.uid {
                                let _ = self.channels.emulated_card_tx.send(Some(EmulatedCard {
                                    uid,
                                    data: self.tag_data_input.clone(),
                                }));
                            }
                        }
                    } else if self.scanning {
                        self.scanning = false;
                        let _ = self.channels.emulated_card_tx.send(None);
                    }
                });
            });

            ui.separator();

            ui.label("UID:");
            ui.add_space(5.0);
            let uid_response = ui.add_sized(
                [ui.available_width(), 0.],
                TextEdit::singleline(&mut self.uid_input)
                    .hint_text("AA:BB:CC:DD")
                    .char_limit(29)
                    .text_color_opt(if self.uid.is_none() {
                        Some(Color32::RED)
                    } else {
                        None
                    }),
            );

            if uid_response.changed() {
                self.parse_uid_input();
            }

            ui.add_space(10.0);

            ui.label("Tag Data:");
            ui.add_space(5.0);
            ui.add_sized(
                ui.available_size(),
                TextEdit::multiline(&mut self.tag_data_input).code_editor(),
            );
        });
    }
}
