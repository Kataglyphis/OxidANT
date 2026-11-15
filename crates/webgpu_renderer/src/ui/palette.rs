//! The Studio Pro palette: BeschleunigerBallett's `KataglyphisImGuiStyle.ixx` tokens as sRGB bytes.

use egui::Color32;

pub const BASE: Color32 = Color32::from_rgb(14, 16, 19);
pub const PANEL: Color32 = Color32::from_rgb(21, 24, 29);
pub const POPUP: Color32 = Color32::from_rgb(26, 30, 36);
pub const FRAME: Color32 = Color32::from_rgb(28, 32, 39);
pub const FRAME_HOVER: Color32 = Color32::from_rgb(35, 40, 51);
pub const FRAME_ACTIVE: Color32 = Color32::from_rgb(42, 48, 59);
pub const RAISED: Color32 = Color32::from_rgb(37, 42, 50);
pub const RAISED_HOVER: Color32 = Color32::from_rgb(44, 50, 60);
pub const RAISED_ACTIVE: Color32 = Color32::from_rgb(51, 58, 70);
pub const BORDER: Color32 = Color32::from_rgb(42, 47, 56);
pub const BORDER_LIGHT: Color32 = Color32::from_rgb(34, 38, 45);
pub const GRAB: Color32 = Color32::from_rgb(96, 104, 118);
pub const TEXT: Color32 = Color32::from_rgb(230, 232, 235);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(155, 163, 174);
pub const TEXT_DISABLED: Color32 = Color32::from_rgb(107, 116, 128);
/// DocumANTation's `brand.tokens.json` `colors.accent`: selection and live values only.
pub const ACCENT: Color32 = Color32::from_rgb(106, 240, 173);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(138, 245, 193);
/// The brand's `accent_strong`.
pub const ACCENT_ACTIVE: Color32 = Color32::from_rgb(42, 212, 136);
pub const WARNING: Color32 = Color32::from_rgb(245, 180, 85);
pub const DANGER: Color32 = Color32::from_rgb(242, 119, 122);
pub const INFO: Color32 = Color32::from_rgb(122, 167, 255);
pub const LINK: Color32 = Color32::from_rgb(34, 211, 238);
pub const AXIS_X: Color32 = Color32::from_rgb(232, 92, 100);
pub const AXIS_Y: Color32 = Color32::from_rgb(138, 196, 88);
pub const AXIS_Z: Color32 = Color32::from_rgb(90, 148, 236);

/// `color` at `alpha` in 0..=1, like the C++ `studioWithAlpha`.
pub fn with_alpha(color: Color32, alpha: f32) -> Color32 {
    let [r, g, b, _] = color.to_array();
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_keep_the_cpp_bytes() {
        assert_eq!(PANEL.to_array(), [21, 24, 29, 255]);
        assert_eq!(ACCENT.to_array(), [106, 240, 173, 255]);
        assert_eq!(AXIS_Z.to_array(), [90, 148, 236, 255]);
    }

    #[test]
    fn with_alpha_keeps_the_colour_and_sets_the_opacity() {
        assert_eq!(
            with_alpha(ACCENT, 0.35),
            Color32::from_rgba_unmultiplied(106, 240, 173, 89)
        );
        assert_eq!(with_alpha(TEXT, 2.0).a(), 255);
        assert_eq!(with_alpha(TEXT, -1.0).a(), 0);
    }
}
