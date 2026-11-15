//! The editor driven headless: dock geometry, F1, the two-stage Esc, the splitters and every glyph it writes.

use egui::{pos2, Event, Key, Modifiers, PointerButton, Pos2, Rect};

use super::*;
use crate::ui::widgets::test_support::{button, click, input, moved};

const NATIVE: HostCaps = HostCaps {
    quit: true,
    screenshot: true,
    hot_reload: true,
};

/// The editor, the data a host hands it, and a frame loop in the host's order.
struct Harness {
    ctx: Context,
    editor: Editor,
    settings: RendererSettings,
    camera: OrbitCamera,
    stats: SceneStats,
    frames: FrameHistory,
    device: DeviceInfo,
    model: ModelSource,
    caps: HostCaps,
    /// The window in points, and its pixels per point.
    size: (f32, f32),
    ppp: f32,
}

impl Harness {
    fn new(size: (f32, f32), ppp: f32) -> Self {
        let mut frames = FrameHistory::default();
        for frame in 0..=FRAME_HISTORY {
            frames.tick(frame as f64 / 60.0);
        }
        let stats = SceneStats {
            occlusion: (3, 7),
            shadow_casters: (5, 9),
            gpu_passes: vec![("shadow", 0.42), ("forward", 1.25), ("tonemap", 0.1)],
            gpu_timing: true,
            trace: crate::render::tracer::TraceStatus::Idle,
        };
        Self {
            ctx: crate::ui::widgets::test_support::context(),
            editor: Editor::default(),
            settings: RendererSettings::default(),
            camera: OrbitCamera::default(),
            stats,
            frames,
            device: DeviceInfo {
                api: "Vulkan".into(),
                name: "llvmpipe (LLVM 21.1.8, 128 bits)".into(),
                driver: "llvmpipe Mesa 26.0".into(),
                cpu: true,
            },
            model: ModelSource::from_path(std::path::Path::new("Models/Dinosaurs/dinosaurs.glb")),
            caps: NATIVE,
            size,
            ppp,
        }
    }

    /// One pass; Esc is armed before it, as the host does.
    fn frame(&mut self, events: Vec<Event>) -> EditorOutput {
        let armed = self.editor.esc_armed(&self.ctx);
        let raw = input(self.size, self.ppp, events);
        let surface_px = [
            (self.size.0 * self.ppp) as u32,
            (self.size.1 * self.ppp) as u32,
        ];
        let mut output = EditorOutput::default();
        let full = self.ctx.run_ui(raw, |ui| {
            let mut cx = EditorContext {
                settings: &mut self.settings,
                camera: &mut self.camera,
                stats: &self.stats,
                frames: &self.frames,
                device: &self.device,
                model: Some(&self.model),
                caps: self.caps,
                surface_px,
            };
            output = self.editor.show(ui, &mut cx, armed);
        });
        full.drop_without_applying_deltas();
        output
    }

    fn scene_rect(&self, viewport: SceneViewport) -> Rect {
        let (w, h) = self.size;
        Rect::from_min_max(
            pos2(viewport.x0 * w, viewport.y0 * h),
            pos2(viewport.x1 * w, viewport.y1 * h),
        )
    }
}

fn tap(key: Key) -> Vec<Event> {
    [true, false]
        .map(|pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        })
        .to_vec()
}

fn assert_near(actual: Rect, expected: Rect) {
    let close =
        (actual.min - expected.min).length() < 0.01 && (actual.max - expected.max).length() < 0.01;
    assert!(close, "{actual:?} is not {expected:?}");
}

#[test]
fn the_scene_is_the_window_less_the_bars_and_docks_at_both_scales() {
    for ppp in [1.0, 1.5] {
        let mut studio = Harness::new((1600.0, 900.0), ppp);
        studio.frame(Vec::new());
        let viewport = studio.frame(Vec::new()).viewport;
        // Menu bar 29, status bar 23, scene dock 248, inspector 336, profiler 172.
        let expected = Rect::from_min_max(
            pos2(248.0, 29.0),
            pos2(1600.0 - 336.0, 900.0 - 23.0 - 172.0),
        );
        assert_near(studio.scene_rect(viewport), expected);
    }
}

#[test]
fn a_docs_iframe_keeps_the_inspector_and_drops_the_scene_dock() {
    let mut studio = Harness::new((800.0, 600.0), 1.0);
    studio.frame(Vec::new());
    let viewport = studio.frame(Vec::new()).viewport;
    let expected = Rect::from_min_max(pos2(0.0, 29.0), pos2(800.0 - 320.0, 600.0 - 23.0 - 172.0));
    assert_near(studio.scene_rect(viewport), expected);
    // The same canvas on a 150 % display: the scene dock's minimum fits where the inspector's does not.
    let mut dense = Harness::new((800.0 / 1.5, 600.0 / 1.5), 1.5);
    dense.frame(Vec::new());
    let viewport = dense.frame(Vec::new()).viewport;
    assert!(
        (viewport.x0 * 800.0 / 1.5 - 180.0).abs() < 0.01,
        "{viewport:?}"
    );
    assert_eq!(viewport.x1, 1.0);
}

#[test]
fn f1_hides_and_brings_back_the_editor_but_not_while_typing() {
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    studio.frame(Vec::new());
    let hidden = studio.frame(tap(Key::F1));
    assert!(!studio.editor.is_visible());
    assert!(
        hidden.viewport.is_full(),
        "a hidden editor gives the camera the whole frame"
    );
    assert!(studio.frame(tap(Key::F1)).viewport != SceneViewport::FULL);
    assert!(studio.editor.is_visible());
    let filter = filter_rect(&mut studio);
    studio.frame(click(filter.center().x, filter.center().y));
    assert!(
        studio.ctx.text_edit_focused(),
        "the filter must hold focus, or this proves nothing"
    );
    studio.frame(tap(Key::F1));
    assert!(studio.editor.is_visible(), "F1 typed into a field is text");
}

fn filter_rect(studio: &mut Harness) -> Rect {
    studio.frame(Vec::new());
    studio
        .ctx
        .read_response(library::filter_id())
        .expect("the library shows its filter")
        .rect
}

#[test]
fn esc_first_leaves_the_field_then_quits_and_never_quits_in_a_browser() {
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    let filter = filter_rect(&mut studio);
    studio.frame(click(filter.center().x, filter.center().y));
    assert!(studio.frame(tap(Key::Escape)).actions.is_empty());
    assert!(!studio.ctx.egui_wants_keyboard_input());
    assert_eq!(studio.frame(tap(Key::Escape)).actions, [EditorAction::Quit]);
    studio.caps = HostCaps::default();
    assert!(studio.frame(tap(Key::Escape)).actions.is_empty());
}

#[test]
fn esc_first_closes_the_about_modal_and_a_menu() {
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    studio.editor.show_about();
    studio.frame(Vec::new());
    assert!(studio.frame(tap(Key::Escape)).actions.is_empty());
    assert!(!studio.editor.about_open());
    assert_eq!(studio.frame(tap(Key::Escape)).actions, [EditorAction::Quit]);
    let file = menu_rect(&mut studio);
    studio.frame(click(file.center().x, file.center().y));
    assert!(
        egui::Popup::is_any_open(&studio.ctx),
        "the File menu must be open"
    );
    assert!(studio.frame(tap(Key::Escape)).actions.is_empty());
    assert!(!egui::Popup::is_any_open(&studio.ctx));
}

/// Where the menu bar's File button sits: right of the brand and its divider.
fn menu_rect(studio: &mut Harness) -> Rect {
    studio.frame(Vec::new());
    let brand = 12.0 + 15.0 + 8.0 + 118.5 + 8.0 + 1.0 + 8.0;
    Rect::from_min_max(pos2(brand, 3.0), pos2(brand + 28.0, 26.0))
}

/// A drag of the scene dock's splitter from `from` to `to`, a frame per step.
fn drag(studio: &mut Harness, from: Pos2, to: Pos2) -> Vec<EditorOutput> {
    vec![
        studio.frame(vec![
            moved(from.x, from.y),
            button(from.x, from.y, PointerButton::Primary, true),
        ]),
        studio.frame(vec![moved(to.x, to.y)]),
        studio.frame(vec![moved(to.x, to.y)]),
        studio.frame(vec![button(to.x, to.y, PointerButton::Primary, false)]),
    ]
}

#[test]
fn a_splitter_drag_follows_the_pointer_and_moves_the_scene_edge() {
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    studio.frame(Vec::new());
    studio.frame(Vec::new());
    let outputs = drag(&mut studio, pos2(250.0, 400.0), pos2(300.0, 400.0));
    // The grab sits 2 points into the handle, and the edge stays under it.
    assert_eq!(studio.editor.prefs.scene_width, 298.0);
    let after = studio.frame(Vec::new()).viewport;
    assert!((after.x0 * 1600.0 - 298.0).abs() < 0.01, "{after:?}");
    assert!(outputs.iter().all(|output| output.actions.is_empty()));
}

#[test]
fn a_click_on_a_splitter_never_rewrites_a_clamped_preference() {
    let mut studio = Harness::new((800.0, 600.0), 1.0);
    studio.frame(Vec::new());
    studio.frame(Vec::new());
    // The 336-point preference shows as 320, the 40 % cap; the handle is the scene's right edge.
    let x = 800.0 - 320.0 - 3.0;
    drag(&mut studio, pos2(x, 300.0), pos2(x, 300.0));
    assert_eq!(
        studio.editor.prefs.inspector_width,
        DockPrefs::default().inspector_width
    );
}

#[test]
fn a_splitter_ignores_a_pointer_another_button_holds() {
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    studio.frame(Vec::new());
    studio.frame(Vec::new());
    studio.frame(vec![
        moved(600.0, 400.0),
        button(600.0, 400.0, PointerButton::Secondary, true),
    ]);
    studio.frame(vec![moved(250.0, 400.0)]);
    drag(&mut studio, pos2(250.0, 400.0), pos2(300.0, 400.0));
    assert_eq!(
        studio.editor.prefs.scene_width,
        DockPrefs::default().scene_width
    );
}

#[test]
fn the_scene_is_the_cameras_but_a_splitter_is_egui_s() {
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    studio.frame(Vec::new());
    studio.frame(vec![moved(700.0, 400.0)]);
    assert!(
        !studio.ctx.egui_wants_pointer_input(),
        "a press in the scene must reach the camera"
    );
    studio.frame(vec![moved(250.0, 400.0)]);
    assert!(
        studio.ctx.egui_wants_pointer_input(),
        "a press on a splitter must not orbit"
    );
    studio.frame(vec![moved(100.0, 400.0)]);
    assert!(studio.ctx.egui_wants_pointer_input(), "a dock is egui's");
}

#[test]
fn every_character_the_editor_writes_has_a_glyph() {
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    // egui's own style and inspection windows draw emoji the studio faces lack; they are egui's text, not the editor's.
    studio.editor.show_about();
    studio.settings.occlusion_queries_enabled = true;
    for object in SceneObject::ALL {
        for profiler_tab in [0, 1] {
            studio.editor.select(object);
            studio.editor.profiler_tab = profiler_tab;
            // Two passes: a new window lays its contents out in an invisible sizing pass first.
            studio.frame(Vec::new());
            studio.frame(Vec::new());
        }
    }
    studio.caps = HostCaps::default();
    studio.device.cpu = false;
    studio.frame(Vec::new());
}

#[test]
fn the_status_bar_names_the_device_the_frame_and_the_build() {
    let studio = Harness::new((1600.0, 900.0), 1.0);
    let mut settings = RendererSettings::default();
    let mut camera = OrbitCamera::default();
    let cx = EditorContext {
        settings: &mut settings,
        camera: &mut camera,
        stats: &studio.stats,
        frames: &studio.frames,
        device: &studio.device,
        model: None,
        caps: NATIVE,
        surface_px: [1600, 900],
    };
    assert_eq!(
        status_bar::device_text(&cx),
        "Vulkan  ·  llvmpipe (LLVM 21.1.8, 128 bits)"
    );
    let frame = status_bar::frame_text(&cx);
    assert!(frame.starts_with("1600 × 900  ·  "), "{frame}");
    assert!(frame.ends_with(&format!(
        "v{}  ·  F1 hides the editor",
        env!("CARGO_PKG_VERSION")
    )));
    assert_eq!(outliner::detail(SceneObject::Model, &cx), "none");
    assert_eq!(outliner::detail(SceneObject::Camera, &cx), "45°");
    assert_eq!(inspector::subtitle(SceneObject::Model, &cx), "No model");
}

#[test]
fn the_about_modal_names_the_egui_this_crate_builds_against() {
    let manifest = include_str!("../../../Cargo.toml");
    assert!(manifest.contains(&format!("egui = \"{}\"", windows::EGUI_VERSION)));
}
