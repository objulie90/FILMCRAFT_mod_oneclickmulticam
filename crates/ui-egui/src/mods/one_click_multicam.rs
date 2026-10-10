//! One-Click Multicam Mod.
//!
//! Provides a streamlined dialog and atomic action that takes selected clips,
//! synchronizes them, nests them into a multicam source sequence, places them
//! onto a new edit sequence, and arms the Program monitor for live recording.

use std::sync::Arc;

use egui::{Color32, vec2};
use filmcraft_project::{ItemId, TrackKind};
use filmcraft_time::{Tick, TimeRange};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::FilmcraftApp;

/// Draft state for the One-Click Multicam setup dialog.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OneClickMulticamDraft {
    pub name: String,
    /// Sync method: "audio", "timecode", or "in"
    pub method: String,
    /// Audio handling: "camera1" (master audio), "switch" (audio follows video), or "all"
    pub audio_mode: String,
    /// Error or validation message if creation fails
    pub error_message: Option<String>,
}

impl Default for OneClickMulticamDraft {
    fn default() -> Self {
        Self {
            name: "Multicam Cut".to_string(),
            method: "audio".to_string(),
            audio_mode: "camera1".to_string(),
            error_message: None,
        }
    }
}

/// Modal dialog for configuring and launching a One-Click Multicam session.
pub fn show_dialog(app: &mut FilmcraftApp, ctx: &egui::Context) {
    let mut open = app.ui.one_click_multicam_dialog.is_some();
    let t = app.tokens;

    let Some(mut draft) = app.ui.one_click_multicam_dialog.clone() else {
        return;
    };

    let mut execute_setup = false;
    let mut cancel = false;

    egui::Window::new("One Click Multicam Setup")
        .id(egui::Id::new("one-click-multicam-dialog"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_size(vec2(460.0, 380.0))
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Automatically create, nest, and lay down your multicam sequence ready for live recording.")
                    .size(11.0)
                    .color(t.text_dim),
            );
            ui.add_space(10.0);

            // Sequence Name
            ui.label(egui::RichText::new("Sequence Name").strong().size(12.0));
            ui.text_edit_singleline(&mut draft.name);
            ui.add_space(10.0);

            // Sync Method
            ui.label(egui::RichText::new("Synchronize By").strong().size(12.0));
            ui.horizontal(|ui| {
                ui.radio_value(&mut draft.method, "audio".to_string(), "Audio Waveform");
                ui.radio_value(&mut draft.method, "timecode".to_string(), "Timecode");
                ui.radio_value(&mut draft.method, "in".to_string(), "In Points");
            });
            ui.add_space(10.0);

            // Audio Routing
            ui.label(egui::RichText::new("Audio Handling").strong().size(12.0));
            ui.vertical(|ui| {
                ui.radio_value(&mut draft.audio_mode, "camera1".to_string(), "Camera 1 Audio (Master Track)");
                ui.radio_value(&mut draft.audio_mode, "switch".to_string(), "Audio Follows Video (Switch with angles)");
                ui.radio_value(&mut draft.audio_mode, "all".to_string(), "All Cameras (Multi-track bed)");
            });

            if let Some(err) = &draft.error_message {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(err).color(Color32::from_rgb(0xe0, 0x3a, 0x3a)).size(11.0));
            }

            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(egui::RichText::new("Create & Open Recorder").strong()).clicked() {
                        execute_setup = true;
                    }
                });
            });
        });

    if cancel || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        open = false;
    }

    if execute_setup {
        match execute_one_click_multicam(app, &draft) {
            Ok(()) => {
                open = false;
            }
            Err(e) => {
                draft.error_message = Some(e);
                open = true;
            }
        }
    }

    if open {
        app.ui.one_click_multicam_dialog = Some(draft);
    } else {
        app.ui.one_click_multicam_dialog = None;
    }
}

/// Executes the full atomic multi-step setup.
fn execute_one_click_multicam(app: &mut FilmcraftApp, draft: &OneClickMulticamDraft) -> Result<(), String> {
    let items = app.session.state.project_selection.clone();
    if items.len() < 2 {
        return Err("Select 2 or more clips in the Project panel to synchronize".to_string());
    }

    // 1. Create the Multicam Source Sequence via engine command
    let source_seq_name = format!("{} (Source)", draft.name);
    let multicam_params = json!({
        "items": items.iter().map(|id| id.0).collect::<Vec<_>>(),
        "name": source_seq_name,
        "method": draft.method,
        "audio": draft.audio_mode,
        "cameraNames": "clip",
    });

    let res = app.session.execute("clip.createMulticam", multicam_params)
        .map_err(|e| format!("Synchronization failed: {e}"))?;

    let source_seq_id = res.get("sequence")
        .and_then(serde_json::Value::as_u64)
        .map(ItemId)
        .ok_or_else(|| "Failed to retrieve created multicam sequence ID".to_string())?;

    // 2. Query source sequence settings & total duration
    let (settings, duration) = {
        let q = app.session.project.sequence(source_seq_id)
            .ok_or_else(|| "Created multicam sequence not found in project".to_string())?;
        (q.settings.clone(), q.duration())
    };

    // 3. Create Target Edit Sequence and place the Multicam Source Sequence on V1 & A1
    let target_seq_id = {
        let mut pr = (*app.session.project).clone();
        let sid = pr.new_sequence(&draft.name, settings.clone(), 1, 1, None);

        let time_range = TimeRange::new(Tick::ZERO, duration);
        if let Some(mut v_item) = pr.make_track_item(source_seq_id, TrackKind::Video, Tick::ZERO, time_range, settings.frame_rate) {
            let link_id = pr.alloc_id();
            v_item.link = Some(link_id);
            if let Some(mut a_item) = pr.make_track_item(source_seq_id, TrackKind::Audio, Tick::ZERO, time_range, settings.frame_rate) {
                a_item.link = Some(link_id);
                if let Some(q) = pr.sequence_mut(sid) {
                    if let Some(vt) = q.video_tracks.first_mut() {
                        vt.items.push(v_item);
                    }
                    if let Some(at) = q.audio_tracks.first_mut() {
                        at.items.push(a_item);
                    }
                }
            }
        }
        app.session.project = Arc::new(pr);
        sid
    };

    // 4. Set active and open sequence to our newly created cut
    app.session.state.active_sequence = Some(target_seq_id);
    if !app.session.state.open_sequences.contains(&target_seq_id) {
        app.session.state.open_sequences.push(target_seq_id);
    }

    // 5. Configure audio follows video if selected
    if draft.audio_mode == "switch" {
        app.session.state.multicam_audio_follows_video = true;
    }

    // 6. Arm Program Monitor for Multicam View and Live Recording
    app.ui.program.multicam = true;
    app.ui.multicam_record = true;

    // 7. Park playhead at frame 0
    let _ = app.session.execute("playhead.set", json!({"frame": 0}));

    app.ui.status = format!("One-Click Multicam ready: '{}' is active and armed for recording!", draft.name);
    Ok(())
}
