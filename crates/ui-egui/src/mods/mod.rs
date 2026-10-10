//! Mods framework and central registry.
//!
//! Provides isolated configuration, safety kill-switches, and dialog dispatch
//! for user-installed extensions. Individual mods reside in dedicated submodules.

use egui::{Color32, Stroke, vec2};
use serde::{Deserialize, Serialize};

use crate::FilmcraftApp;

pub mod one_click_multicam;
pub use one_click_multicam::OneClickMulticamDraft;

/// Persistent configuration for custom user mods.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ModsConfig {
    /// Toggle switch for the One-Click Multicam wizard mod.
    pub one_click_multicam_enabled: bool,
}

impl Default for ModsConfig {
    fn default() -> Self {
        Self {
            one_click_multicam_enabled: true,
        }
    }
}

/// Renders all active mod-related dialogs.
pub fn show_dialogs(app: &mut FilmcraftApp, ctx: &egui::Context) {
    if app.ui.show_manage_mods {
        show_manage_mods_dialog(app, ctx);
    }
    if app.ui.mods_config.one_click_multicam_enabled && app.ui.one_click_multicam_dialog.is_some() {
        one_click_multicam::show_dialog(app, ctx);
    }
}

/// Modal dialog for managing and toggling user mods.
pub fn show_manage_mods_dialog(app: &mut FilmcraftApp, ctx: &egui::Context) {
    let mut open = app.ui.show_manage_mods;
    let t = app.tokens;

    egui::Window::new("Manage Mods")
        .id(egui::Id::new("manage-mods-dialog"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_size(vec2(480.0, 320.0))
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Toggle custom features on or off. If a mod causes unexpected behavior, switch it off here to instantly restore default app behavior.")
                    .size(11.0)
                    .color(t.text_dim),
            );
            ui.add_space(12.0);

            // Mod item card: One Click Multicam
            let card_fill = Color32::from_rgb(0x28, 0x28, 0x28);
            let stroke = Stroke::new(1.0, Color32::from_rgb(0x3a, 0x3a, 0x3a));

            egui::Frame::none()
                .fill(card_fill)
                .stroke(stroke)
                .rounding(6.0)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("One Click Multicam").strong().size(13.0).color(t.text));
                                ui.label(egui::RichText::new("v1.0").size(10.0).color(t.text_dim));
                            });
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new("Automates multi-track clip sync, sequence nesting, timeline placement, and Program monitor record arming into a single step.")
                                    .size(11.0)
                                    .color(t.text_dim),
                            );
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let mut enabled = app.ui.mods_config.one_click_multicam_enabled;
                            if ui.checkbox(&mut enabled, "").changed() {
                                app.ui.mods_config.one_click_multicam_enabled = enabled;
                            }
                        });
                    });
                });

            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        app.ui.show_manage_mods = false;
                    }
                });
            });
        });

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        open = false;
    }
    app.ui.show_manage_mods = open;
}
