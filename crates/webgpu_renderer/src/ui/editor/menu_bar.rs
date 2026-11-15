//! The C++ main menu bar: brand, File/View/Render/Help, a centred renderer switch and the frame rate.

use egui::{pos2, vec2, Align2, Button, Margin, MenuBar, Rect, Style, TextStyle, Ui, UiBuilder};

use super::docks::{self, Side};
use super::{
    Editor, EditorAction, EditorContext, RendererSettings, SceneObject, BRAND, FRAME_RATE_WINDOW,
};
use crate::render::forward::RasterPath;
use crate::ui::icons::{self, StudioIcon};
use crate::ui::palette as pal;
use crate::ui::theme;
use crate::ui::widgets::{frame_height, galley, line_height, Segmented};

/// The C++ bar is three points taller than a frame above and below, so the switch sits in it with a margin.
const BAR_EXTRA: f32 = 3.0;
/// The C++ View › UI scale steps.
pub(super) const UI_SCALES: [(f32, &str); 5] = [
    (0.85, "85 %"),
    (1.0, "100 %"),
    (1.15, "115 %"),
    (1.3, "130 %"),
    (1.5, "150 %"),
];
pub(super) const RENDER_MODES: [&str; 3] = ["Raster", "Ray traced", "Path traced"];
/// WebGPU has no ray tracing pipeline, so only the raster mode can be picked.
pub(super) const RENDER_MODES_ENABLED: [bool; 3] = [true, false, false];

pub(super) fn bar_height(ui: &Ui) -> f32 {
    frame_height(ui) + 2.0 * BAR_EXTRA
}

pub(super) fn show(ui: &mut Ui, editor: &mut Editor, cx: &mut EditorContext<'_>) {
    let margin = Margin::symmetric(theme::WINDOW_PADDING.x as i8, 0);
    docks::panel(ui, "studio_menu", Side::Top, bar_height(ui), margin, |ui| {
        let bar = ui.max_rect();
        let menus_right = ui
            .horizontal_centered(|ui| {
                brand(ui);
                // The bar claims the whole width, so the last menu's edge is where the menus end.
                MenuBar::new()
                    .style(bar_style)
                    .ui(ui, |ui| menus(ui, editor, cx))
                    .inner
            })
            .inner;
        let switch_right = renderer_switch(ui, bar, menus_right);
        frame_rate(ui, bar, switch_right.unwrap_or(menus_right), cx);
    });
}

/// egui's menu style, with the C++ spacing: labels 16 points apart.
fn bar_style(style: &mut Style) {
    egui::containers::menu::menu_style(style);
    style.spacing.button_padding = vec2(4.0, 0.0);
    style.spacing.item_spacing.x = 8.0;
}

/// The neutral brand mark, the name and a divider; mint is for selection and live values.
fn brand(ui: &mut Ui) {
    let size = line_height(ui);
    icons::icon_text(ui, StudioIcon::Cube, size, pal::TEXT_MUTED);
    ui.label(egui::RichText::new(BRAND).color(pal::TEXT));
    let (rect, _) = ui.allocate_exact_size(vec2(theme::hairline(ui), size), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, pal::BORDER);
}

/// File, View, Render and Help; returns the right edge of the last.
fn menus(ui: &mut Ui, editor: &mut Editor, cx: &mut EditorContext<'_>) -> f32 {
    ui.menu_button("File", |ui| file_menu(ui, cx, &mut editor.actions));
    ui.menu_button("View", |ui| view_menu(ui, editor));
    ui.menu_button("Render", |ui| render_menu(ui, cx.settings));
    let help = ui.menu_button("Help", |ui| {
        if ui.button("Key bindings").clicked() {
            editor.select(SceneObject::Camera);
        }
        if ui.button("About").clicked() {
            editor.show_about();
        }
    });
    help.response.rect.right()
}

fn file_menu(ui: &mut Ui, cx: &EditorContext<'_>, actions: &mut Vec<EditorAction>) {
    let reload = ui
        .add_enabled(
            cx.caps.hot_reload,
            Button::new("Reload shaders").shortcut_text("F5"),
        )
        .on_disabled_hover_text("Needs the native viewer");
    if reload.clicked() {
        actions.push(EditorAction::ReloadShaders);
    }
    ui.separator();
    let quit = ui
        .add_enabled(cx.caps.quit, Button::new("Quit").shortcut_text("Esc"))
        .on_disabled_hover_text("A browser tab cannot quit");
    if quit.clicked() {
        actions.push(EditorAction::Quit);
    }
}

fn view_menu(ui: &mut Ui, editor: &mut Editor) {
    ui.checkbox(&mut editor.prefs.show_scene, "Scene");
    ui.checkbox(&mut editor.prefs.show_inspector, "Inspector");
    ui.checkbox(&mut editor.prefs.show_profiler, "Profiler");
    if ui
        .add(Button::new("Hide editor").shortcut_text("F1"))
        .clicked()
    {
        editor.visible = false;
    }
    ui.separator();
    ui.menu_button("UI scale", |ui| {
        let zoom = ui.ctx().zoom_factor();
        for (scale, label) in UI_SCALES {
            if ui.radio((zoom - scale).abs() < 0.01, label).clicked() {
                ui.ctx().set_zoom_factor(scale);
            }
        }
    });
    if ui.button("Reset layout").clicked() {
        editor.reset_layout(ui.ctx());
    }
    if ui
        .button("Frame scene")
        .on_hover_text("Back to the scene's start view: its active camera, else its bounds")
        .clicked()
    {
        editor.actions.push(EditorAction::FrameScene);
    }
    ui.separator();
    ui.checkbox(&mut editor.windows.style, "Style editor");
    ui.checkbox(&mut editor.windows.inspection, "egui inspection");
}

/// The C++ Render menu; what WebGPU lacks shows disabled, as in a C++ build without hardware ray tracing.
fn render_menu(ui: &mut Ui, settings: &mut RendererSettings) {
    for (index, label) in RENDER_MODES.iter().enumerate() {
        ui.add_enabled(
            RENDER_MODES_ENABLED[index],
            egui::RadioButton::new(index == 0, *label),
        );
    }
    ui.separator();
    for option in RasterPath::ALL {
        if ui
            .radio(settings.raster_path == option, option.label())
            .clicked()
        {
            settings.raster_path = option;
        }
    }
    ui.separator();
    ui.checkbox(&mut settings.frustum_culling, "Frustum culling");
}

/// The centred mode switch, dropped when it would touch the menus; returns its right edge.
fn renderer_switch(ui: &mut Ui, bar: Rect, menus_right: f32) -> Option<f32> {
    let segmented = Segmented::new(&RENDER_MODES).enabled(&RENDER_MODES_ENABLED);
    let font = TextStyle::Body.resolve(ui.style());
    let widest = RENDER_MODES
        .iter()
        .map(|label| galley(ui, *label, font.clone(), pal::TEXT).size().x)
        .fold(0.0, f32::max);
    let width = ((widest + theme::FRAME_PADDING.x * 3.0) * RENDER_MODES.len() as f32).round();
    let left = (bar.center().x - width * 0.5).round();
    if left <= menus_right + ui.spacing().item_spacing.x {
        return None;
    }
    let height = frame_height(ui);
    let rect = Rect::from_min_size(
        pos2(left, (bar.center().y - height * 0.5).round()),
        vec2(width, height),
    );
    let mut slot = ui.new_child(UiBuilder::new().max_rect(rect).id_salt("renderer_switch"));
    let mut mode = 0;
    segmented.width(width).show(&mut slot, &mut mode);
    Some(rect.right())
}

/// `N FPS · x ms`, muted and right-aligned, when it fits beside whatever is left of it.
fn frame_rate(ui: &Ui, bar: Rect, occupied_right: f32, cx: &EditorContext<'_>) {
    let Some(frame_ms) = cx.frames.mean_ms(FRAME_RATE_WINDOW) else {
        return;
    };
    let fps = if frame_ms > 0.0 {
        1000.0 / frame_ms
    } else {
        0.0
    };
    let text = format!("{fps:.0} FPS  ·  {frame_ms:.2} ms");
    let font = TextStyle::Body.resolve(ui.style());
    let width = galley(ui, &text, font.clone(), pal::TEXT_MUTED).size().x;
    let right = bar.right();
    if right - width > occupied_right + ui.spacing().item_spacing.x {
        let pos = pos2(right, bar.center().y);
        ui.painter()
            .text(pos, Align2::RIGHT_CENTER, text, font, pal::TEXT_MUTED);
    }
}
