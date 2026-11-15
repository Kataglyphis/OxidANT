//! The Studio Pro style in egui points: BeschleunigerBallett's `applyKataglyphisImGuiDarkTheme` at scale 1.

use std::collections::BTreeMap;

use egui::style::{Selection, Spacing, WidgetVisuals, Widgets};
use egui::{
    vec2, Color32, CornerRadius, FontId, Margin, Shadow, Stroke, Style, SurrenderFocusOn,
    TextStyle, Theme, ThemePreference, Visuals,
};

use super::fonts;
use super::palette as pal;

/// The `TextStyle::Name` of the profiler's headline figure.
pub const FIGURE_STYLE: &str = "figure";
/// Windows, popups and menus, the C++ WindowRounding.
const WINDOW_RADIUS: u8 = 6;
/// Frames and buttons, the C++ FrameRounding.
pub const FRAME_RADIUS: u8 = 5;
/// The C++ FramePadding.
pub const FRAME_PADDING: egui::Vec2 = egui::Vec2::new(8.0, 4.0);
/// The C++ GetFrameHeight: one body line and the frame padding above and below it.
pub const FRAME_HEIGHT: f32 = fonts::LINE_HEIGHT + 2.0 * FRAME_PADDING.y;
/// The C++ WindowPadding, every dock's and bar's margin.
pub const WINDOW_PADDING: egui::Vec2 = egui::Vec2::new(12.0, 10.0);
/// The C++ ItemInnerSpacing.
pub const INNER_SPACING: egui::Vec2 = egui::Vec2::new(6.0, 4.0);

/// Installs the style for the dark theme and pins that theme, so an OS in light mode cannot swap it out.
pub fn install(ctx: &egui::Context) {
    ctx.set_theme(ThemePreference::Dark);
    ctx.set_style_of(Theme::Dark, studio_style());
    ctx.options_mut(|options| {
        // The C++ editor has no keyboard zoom, and the browser's own Ctrl +/- would zoom twice.
        options.zoom_with_keyboard = false;
        // A right press into the scene drops a field's focus, as the C++ editor's right click does.
        options.input_options.surrender_focus_on = SurrenderFocusOn::Presses;
    });
}

pub fn studio_style() -> Style {
    let mut style = Theme::Dark.default_style();
    style.text_styles = text_styles();
    spacing(&mut style.spacing);
    style.interaction.selectable_labels = false;
    style.interaction.tooltip_delay = 0.15;
    style.visuals = visuals();
    style
}

/// The C++ type scale: captions 0.8, titles 1.2 and the profiler figure 1.8 times the body size.
fn text_styles() -> BTreeMap<TextStyle, FontId> {
    BTreeMap::from([
        (TextStyle::Small, fonts::caption_font()),
        (TextStyle::Body, fonts::body_font()),
        (TextStyle::Button, fonts::body_font()),
        (TextStyle::Heading, fonts::title_font()),
        (TextStyle::Monospace, fonts::mono_font()),
        (TextStyle::Name(FIGURE_STYLE.into()), fonts::figure_font()),
    ])
}

fn spacing(spacing: &mut Spacing) {
    spacing.item_spacing = vec2(8.0, 6.0);
    spacing.button_padding = FRAME_PADDING;
    // DragValues and menu bars take this height, so they line up with the frame-height widgets.
    spacing.interact_size.y = FRAME_HEIGHT;
    spacing.window_margin = Margin::symmetric(12, 10);
    spacing.menu_margin = Margin::same(6);
    spacing.indent = 14.0;
    spacing.scroll.bar_width = 10.0;
}

fn visuals() -> Visuals {
    let mut visuals = Visuals::dark();
    visuals.widgets = widgets();
    visuals.selection = Selection {
        bg_fill: pal::with_alpha(pal::ACCENT, 0.35),
        stroke: Stroke::new(1.0, pal::TEXT),
    };
    visuals.weak_text_color = Some(pal::TEXT_MUTED);
    visuals.hyperlink_color = pal::LINK;
    visuals.faint_bg_color = pal::with_alpha(Color32::WHITE, 0.02);
    visuals.extreme_bg_color = pal::BASE;
    visuals.text_edit_bg_color = Some(pal::FRAME);
    visuals.code_bg_color = pal::FRAME;
    visuals.warn_fg_color = pal::WARNING;
    visuals.error_fg_color = pal::DANGER;
    visuals.window_corner_radius = CornerRadius::same(WINDOW_RADIUS);
    visuals.menu_corner_radius = CornerRadius::same(WINDOW_RADIUS);
    visuals.window_shadow = Shadow::NONE;
    visuals.popup_shadow = Shadow::NONE;
    visuals.window_fill = pal::PANEL;
    visuals.panel_fill = pal::PANEL;
    visuals.window_stroke = Stroke::new(1.0, pal::BORDER);
    visuals.text_cursor.stroke = Stroke::new(1.0, pal::TEXT);
    visuals.disabled_alpha = 0.45;
    visuals
}

/// Borderless frames, as the C++ FrameBorderSize 0; only windows and separators get the hairline.
fn widgets() -> Widgets {
    let frame = |fill: Color32| WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: Stroke::NONE,
        corner_radius: CornerRadius::same(FRAME_RADIUS),
        fg_stroke: Stroke::new(1.0, pal::TEXT),
        expansion: 0.0,
    };
    Widgets {
        noninteractive: WidgetVisuals {
            bg_stroke: Stroke::new(1.0, pal::BORDER),
            corner_radius: CornerRadius::same(WINDOW_RADIUS),
            ..frame(pal::PANEL)
        },
        inactive: frame(pal::FRAME),
        hovered: frame(pal::FRAME_HOVER),
        active: frame(pal::FRAME_ACTIVE),
        open: frame(pal::RAISED),
    }
}

/// One physical pixel or more, in points: the C++ `max(1, round(scale))` hairline.
pub fn hairline(ui: &egui::Ui) -> f32 {
    let pixels_per_point = ui.ctx().pixels_per_point();
    pixels_per_point.round().max(1.0) / pixels_per_point
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_pins_the_dark_studio_style_and_turns_keyboard_zoom_off() {
        let ctx = egui::Context::default();
        install(&ctx);
        assert_eq!(ctx.options(|o| o.theme_preference), ThemePreference::Dark);
        assert!(!ctx.options(|o| o.zoom_with_keyboard));
        let style = ctx.style_of(Theme::Dark);
        assert_eq!(style.visuals.panel_fill, pal::PANEL);
        assert_eq!(style.visuals.widgets.inactive.weak_bg_fill, pal::FRAME);
        assert_eq!(style.text_styles[&TextStyle::Body], fonts::body_font());
    }

    #[test]
    fn the_type_scale_follows_the_cpp_editor() {
        let styles = text_styles();
        let size = |style: TextStyle| styles[&style].size;
        // In C++ pixels of line height: 15, 12, 18 and 27; egui sizes the em, 2048/2400 of that.
        let line = |style: TextStyle| size(style) * 2400.0 / 2048.0;
        assert!((line(TextStyle::Body) - 15.0).abs() < 1e-4);
        assert!((line(TextStyle::Small) - 12.0).abs() < 1e-4);
        assert!((line(TextStyle::Heading) - 18.0).abs() < 1e-4);
        assert!((line(TextStyle::Name(FIGURE_STYLE.into())) - 27.0).abs() < 1e-4);
        assert_eq!(styles[&TextStyle::Monospace], fonts::mono_font());
        assert_eq!(FRAME_HEIGHT, 23.0);
    }
}
