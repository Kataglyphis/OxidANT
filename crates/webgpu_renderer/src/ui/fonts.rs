//! The C++ editor's faces: Roboto Medium for the UI, Cousine for paths and figures; licences in `assets/fonts/`.
//! Two small subsets of egui's own emoji faces follow both, for the symbols egui's debug windows draw.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily, FontId};

/// Byte for byte BeschleunigerBallett's `third_party/IMGUI/misc/fonts/Roboto-Medium.ttf` (Apache-2.0).
pub const ROBOTO_MEDIUM: &[u8] = include_bytes!("../../assets/fonts/Roboto-Medium.ttf");
/// Byte for byte BeschleunigerBallett's `third_party/IMGUI/misc/fonts/Cousine-Regular.ttf` (OFL-1.1).
pub const COUSINE_REGULAR: &[u8] = include_bytes!("../../assets/fonts/Cousine-Regular.ttf");
/// Noto Emoji (OFL-1.1) cut to the symbols egui's style and inspection windows draw that it has; see `NOTICE`.
pub const NOTO_EMOJI_SUBSET: &[u8] = include_bytes!("../../assets/fonts/NotoEmoji-Subset.ttf");
/// emoji-icon-font (MIT) cut to the rest of those symbols, the ones Noto Emoji lacks; see `NOTICE`.
pub const EMOJI_ICON_SUBSET: &[u8] = include_bytes!("../../assets/fonts/EmojiIcon-Subset.ttf");

/// The C++ editor's 15 px body line, in points: one row of body text.
pub const LINE_HEIGHT: f32 = 15.0;
/// ImGui sizes a face by ascent minus descent, egui by the em: Roboto's 2400-unit line on a 2048-unit em.
const ROBOTO_EM_PER_LINE: f32 = 2048.0 / 2400.0;
/// Cousine's line is 2320 units on the same em.
const COUSINE_EM_PER_LINE: f32 = 2048.0 / 2320.0;
/// The em that gives Roboto the C++ 15 px line, 12.8 points.
pub const BODY_SIZE: f32 = LINE_HEIGHT * ROBOTO_EM_PER_LINE;
pub const CAPTION_SCALE: f32 = 0.8;
pub const TITLE_SCALE: f32 = 1.2;
pub const FIGURE_SCALE: f32 = 1.8;

const UI_FACE: &str = "Roboto-Medium";
const MONO_FACE: &str = "Cousine-Regular";
const NOTO_FACE: &str = "NotoEmoji-Subset";
const ICON_FACE: &str = "EmojiIcon-Subset";

/// Each family falls back on the other face, then on the symbol subsets, so a glyph one lacks still draws.
pub fn studio_fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::empty();
    let faces = [
        (UI_FACE, ROBOTO_MEDIUM),
        (MONO_FACE, COUSINE_REGULAR),
        (NOTO_FACE, NOTO_EMOJI_SUBSET),
        (ICON_FACE, EMOJI_ICON_SUBSET),
    ];
    for (name, bytes) in faces {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    let chain = |first: &str, second: &str| {
        [first, second, NOTO_FACE, ICON_FACE]
            .map(str::to_owned)
            .to_vec()
    };
    fonts
        .families
        .insert(FontFamily::Proportional, chain(UI_FACE, MONO_FACE));
    fonts
        .families
        .insert(FontFamily::Monospace, chain(MONO_FACE, UI_FACE));
    fonts
}

pub fn body_font() -> FontId {
    FontId::proportional(BODY_SIZE)
}

pub fn caption_font() -> FontId {
    FontId::proportional(BODY_SIZE * CAPTION_SCALE)
}

pub fn title_font() -> FontId {
    FontId::proportional(BODY_SIZE * TITLE_SCALE)
}

pub fn figure_font() -> FontId {
    FontId::proportional(BODY_SIZE * FIGURE_SCALE)
}

pub fn mono_font() -> FontId {
    FontId::monospace(LINE_HEIGHT * COUSINE_EM_PER_LINE)
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;

    fn sha256_hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn the_fonts_are_the_cpp_editors_files() {
        // Pinned, so this copy cannot drift from BeschleunigerBallett's unnoticed.
        assert_eq!(
            sha256_hex(ROBOTO_MEDIUM),
            "8559132c89ad51d8a2ba5b171887a44a7ba93776e205f553573de228e64b45f8"
        );
        assert_eq!(
            sha256_hex(COUSINE_REGULAR),
            "0d5d5eeb6a342432bd63a3c0d16e8470160e019933ee5af3e159d06d665dacce"
        );
    }

    #[test]
    fn the_symbol_subsets_are_the_ones_notice_records() {
        // The pyftsubset commands in NOTICE reproduce these from epaint_default_fonts 0.37.0.
        assert_eq!(
            sha256_hex(NOTO_EMOJI_SUBSET),
            "4b987c39e020f32176272c3de2a09036339b17b497451f2923b33df879219a54"
        );
        assert_eq!(
            sha256_hex(EMOJI_ICON_SUBSET),
            "273ab4ce72295371d872a362d92a836cbffdfe5e08e06d264c012ae30a87c5d8"
        );
        let notice = include_str!("../../assets/fonts/NOTICE");
        for hash in [sha256_hex(NOTO_EMOJI_SUBSET), sha256_hex(EMOJI_ICON_SUBSET)] {
            assert!(notice.contains(&hash), "NOTICE does not record {hash}");
        }
    }

    /// The symbols egui 0.37's style, settings, memory, texture and input UIs put in their labels and headers.
    const DEBUG_WINDOW_SYMBOLS: &str = "↕☀☝⚠✒🌙🎑🎨🐛💻📊📋📏📥🔃🔄🔠🔥⏴⏵⏶⏷⚙⟲🖱🖼";

    #[test]
    fn every_symbol_egui_s_debug_windows_use_has_a_glyph_in_both_families() {
        let ctx = egui::Context::default();
        ctx.set_fonts(studio_fonts());
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            for font in [body_font(), mono_font()] {
                let missing: String = DEBUG_WINDOW_SYMBOLS
                    .chars()
                    .filter(|&c| !ui.ctx().fonts_mut(|f| f.has_glyph(&font, c)))
                    .collect();
                assert!(missing.is_empty(), "{:?} lacks {missing}", font.family);
            }
        });
        output.drop_without_applying_deltas();
    }

    /// Every character of every galley in `shapes` that its section's font has no glyph for.
    fn tofu(ctx: &egui::Context, shapes: &[egui::epaint::ClippedShape]) -> String {
        let mut galleys = Vec::new();
        let mut stack: Vec<&egui::Shape> = shapes.iter().map(|clipped| &clipped.shape).collect();
        while let Some(shape) = stack.pop() {
            match shape {
                egui::Shape::Text(text) => galleys.push(text.galley.clone()),
                egui::Shape::Vec(inner) => stack.extend(inner),
                _ => {}
            }
        }
        let mut missing = String::new();
        for galley in galleys {
            for section in &galley.job.sections {
                let range = &section.byte_range;
                let text = &galley.job.text[range.start.0..range.end.0];
                let font = &section.format.font_id;
                let lacking = text
                    .chars()
                    .filter(|&c| !c.is_whitespace() && c != '\u{FE0F}')
                    .filter(|&c| !ctx.fonts_mut(|f| f.has_glyph(font, c)));
                missing.extend(lacking);
            }
        }
        missing
    }

    #[test]
    fn egui_s_style_and_inspection_windows_draw_no_tofu() {
        let ctx = crate::render::overlay::studio_context();
        let mut missing = String::new();
        // The first pass lays the windows out, the second paints them at their sizes.
        for _ in 0..2 {
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let ctx = ui.ctx().clone();
                egui::Window::new("Style editor")
                    .show(&ctx, |ui| ctx.style_ui(ui, egui::Theme::Dark));
                egui::Window::new("egui inspection").show(&ctx, |ui| ctx.inspection_ui(ui));
            });
            missing = tofu(&ctx, &output.shapes);
            output.drop_without_applying_deltas();
        }
        assert!(missing.is_empty(), "drawn without a glyph: {missing}");
    }

    #[test]
    fn the_licence_texts_beside_the_fonts_are_the_originals() {
        // Embedded, because the Windows arm64 lane stages no assets/ beside the test binary.
        assert_eq!(
            sha256_hex(include_bytes!("../../assets/fonts/Apache-2.0.txt")),
            "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30"
        );
        assert_eq!(
            sha256_hex(include_bytes!("../../assets/fonts/OFL-1.1.txt")),
            "1d361a8f8e8ce6e68457dcd93fb56e162e6baa3bbb7e7573a290d44399f6b57e"
        );
    }

    #[test]
    fn a_body_row_is_the_cpp_15_pixel_line_and_text_keeps_its_width() {
        let ctx = egui::Context::default();
        ctx.set_fonts(studio_fonts());
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let galley = ui.painter().layout_no_wrap(
                "BeschleunigerBallett".into(),
                body_font(),
                egui::Color32::WHITE,
            );
            assert_eq!(galley.size().y, LINE_HEIGHT);
            // The C++ menu bar sets the brand 118 pixels wide.
            assert!((galley.size().x - 118.5).abs() < 1.0, "{}", galley.size().x);
            let mono = ui.ctx().fonts_mut(|f| f.row_height(&mono_font()));
            assert!((mono - LINE_HEIGHT).abs() < 0.5, "{mono}");
        });
        output.drop_without_applying_deltas();
    }

    #[test]
    fn each_family_starts_with_its_own_face() {
        let fonts = studio_fonts();
        assert_eq!(fonts.families[&FontFamily::Proportional][0], UI_FACE);
        assert_eq!(fonts.families[&FontFamily::Monospace][0], MONO_FACE);
        assert_eq!(fonts.font_data.len(), 4);
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            assert_eq!(fonts.families[&family][2..], [NOTO_FACE, ICON_FACE]);
        }
    }
}
