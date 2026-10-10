//! Headless integration test for the One-Click Multicam Mod and Manage Mods framework.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};

use egui_kittest::Harness;
use filmcraft_engine::Session;
use filmcraft_engine::project::{ItemKind, Label, MediaClip, MediaRef, Project, SequenceSettings, TrackKind};
use filmcraft_media::generators::GeneratorSource;
use filmcraft_media::{DemoScene, Generator, MediaSource};
use filmcraft_time::{FrameRate, Tick, TimeRange};
use filmcraft_ui_egui::FilmcraftApp;
use filmcraft_ui_egui::control::ControlRequest;
use serde_json::{Value, json};

const W: u32 = 320;
const H: u32 = 180;

fn tmp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("filmcraft-ui-{name}-{}-{nanos}", std::process::id()));
    let _ = std::fs::create_dir_all(&d);
    d
}

fn make_movie(path: &Path, scene: DemoScene, frames: i64) {
    let rate = FrameRate::FPS_24;
    let dur = rate.tick_of(frames);
    let src = GeneratorSource::new(Generator::Demo(scene), W, H, rate, dur);
    let clip = MediaClip {
        media: MediaRef::Generator(Generator::Demo(scene)),
        info: src.info().clone(),
        interpret: Default::default(),
        mark_in: None,
        mark_out: None,
        markers: vec![],
        offline: false,
        proxy: None,
        identity: None,
    };
    let mut p = Project::new("fixture");
    let item = p.add_item("scene", Label::Iris, ItemKind::Media(clip), None);
    let seq = p.new_sequence("s", SequenceSettings { width: W, height: H, frame_rate: rate, ..Default::default() }, 1, 0, None);
    let mut v = p.make_track_item(item, TrackKind::Video, Tick::ZERO, TimeRange::new(Tick::ZERO, dur), rate).unwrap();
    for e in &mut v.effects {
        filmcraft_engine::project::resolve_auto_points(e, (W, H), (W, H));
    }
    p.sequence_mut(seq).unwrap().video_tracks[0].items.push(v);
    let settings = filmcraft_engine::export::ExportSettings {
        format: filmcraft_engine::export::Format::ProRes,
        path: path.to_string_lossy().into_owned(),
        include_audio: false,
        ..Default::default()
    };
    let shared: filmcraft_media::SharedSource = Arc::new(src);
    let provider = move |id| (id == item).then(|| shared.clone());
    let _ = filmcraft_engine::export::export(&Arc::new(p), seq, &settings, &provider, &Default::default());
}

fn session() -> (Session, Vec<u64>) {
    let dir = tmp_dir("one-click-mod");
    let scenes = [DemoScene::OceanSunset, DemoScene::CityNight, DemoScene::Aurora];
    let files: Vec<PathBuf> = (0..3).map(|i| dir.join(format!("Cam_{}.mov", (b'A' + i as u8) as char))).collect();
    for (f, sc) in files.iter().zip(scenes) {
        make_movie(f, sc, 72);
    }
    let mut s = Session::default();
    let r = s.execute("file.import", json!({"paths": files.iter().map(|f| f.to_string_lossy()).collect::<Vec<_>>()})).unwrap();
    let items: Vec<u64> = r["items"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    (s, items)
}

struct Driver {
    harness: Harness<'static, FilmcraftApp>,
    tx: Sender<ControlRequest>,
    snapshots: Option<PathBuf>,
}

impl Driver {
    fn new(session: Session) -> Self {
        let (tx, rx) = channel();
        let app = FilmcraftApp::new(session).with_control(rx);
        let snapshots = std::env::var_os("FILMCRAFT_UI_SNAPSHOT_DIR").map(PathBuf::from);
        let mut b = Harness::builder().with_size(egui::vec2(1600.0, 980.0)).with_max_steps(10_000);
        if snapshots.is_some() {
            b = b.wgpu();
        }
        let harness = b.build_eframe(move |_cc| app);
        let mut d = Driver { harness, tx, snapshots };
        d.frames(4);
        d
    }

    fn snapshot(&mut self, name: &str) {
        let Some(dir) = self.snapshots.clone() else { return };
        self.frames(3);
        let img = match self.harness.render() {
            Ok(i) => i,
            Err(e) => {
                eprintln!("snapshot {name} skipped: {e}");
                return;
            }
        };
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("{name}.png"));
        let _ = img.save(&path);
        eprintln!("snapshot: {}", path.display());
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            let ctx = self.harness.ctx.clone();
            let mut raw = std::mem::take(self.harness.input_mut());
            eframe::App::raw_input_hook(self.harness.state_mut(), &ctx, &mut raw);
            *self.harness.input_mut() = raw;
            self.harness.step();
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let (req, reply) = ControlRequest::new(method, params.clone());
        let _ = self.tx.send(req);
        for _ in 0..600 {
            self.frames(1);
            if let Ok(v) = reply.try_recv() {
                return v;
            }
        }
        panic!("no reply to {method} {params}");
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        let v = self.call(method, params.clone());
        assert_eq!(v["ok"], json!(true), "{method} {params} failed: {v}");
        v["result"].clone()
    }

    fn exec(&mut self, command: &str, params: Value) -> Value {
        self.ok("engine.execute", json!({"command": command, "params": params}))
    }

    fn app(&mut self) -> &mut FilmcraftApp {
        self.harness.state_mut()
    }
}

#[test]
fn test_one_click_multicam_mod_workflow() {
    let (s, items) = session();
    let mut d = Driver::new(s);

    // 1. Verify Manage Mods menu toggle
    assert!(d.app().ui.mods_config.one_click_multicam_enabled);
    d.ok("ui.menu.invoke", json!({"id": "mods.manage"}));
    d.frames(2);
    assert!(d.app().ui.show_manage_mods);
    d.snapshot("01-manage-mods-dialog");
    d.app().ui.show_manage_mods = false;

    // 2. Select 3 camera clips in Project panel
    d.exec("project.select", json!({"items": items}));
    assert_eq!(d.app().session.state.project_selection.len(), 3);

    // 3. Trigger One-Click Multicam wizard
    d.ok("ui.menu.invoke", json!({"id": "mods.oneClickMulticam"}));
    d.frames(2);
    assert!(d.app().ui.one_click_multicam_dialog.is_some());
    d.snapshot("02-one-click-multicam-dialog");

    // 4. Simulate user clicking "Create & Open Recorder"
    let draft = d.app().ui.one_click_multicam_dialog.take().unwrap();
    // Auto-detected 'in' sync method because test media has no audio tracks
    assert_eq!(draft.method, "in");
    let res = filmcraft_ui_egui::mods::one_click_multicam::execute_one_click_multicam(d.app(), &draft);
    assert!(res.is_ok(), "one-click multicam setup succeeded: {:?}", res);

    // 5. Verify Program monitor is armed for recording & multi-camera view, and sequence is active
    assert!(d.app().ui.program.multicam);
    assert!(d.app().ui.multicam_record);
    let target_seq_id = d.app().session.state.active_sequence.unwrap();
    let seq = d.app().session.project.sequence(target_seq_id).unwrap();
    assert_eq!(seq.video_tracks.len(), 1);
    assert_eq!(seq.video_tracks[0].items.len(), 1, "V1 has the multicam track item placed");
    assert!(seq.video_tracks[0].items[0].multicam.is_some(), "track item is marked as multicam");
    assert!(d.app().ui.status.contains("armed for recording"));
    d.frames(4);
    d.snapshot("03-armed-multicam-view");

    // 6. Test Kill-Switch: disabling the mod in config blocks invocation
    d.app().ui.mods_config.one_click_multicam_enabled = false;
    let res_disabled = d.call("ui.menu.invoke", json!({"id": "mods.oneClickMulticam"}));
    assert_eq!(res_disabled["result"]["error"], "mod disabled");
}
