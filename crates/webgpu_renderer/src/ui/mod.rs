//! The Studio Pro editor and its look: BeschleunigerBallett's palette, style, fonts, widgets and icons, in egui.

pub mod editor;
pub mod fonts;
pub mod icons;
pub mod palette;
pub mod theme;
pub mod widgets;

pub use editor::{
    DeviceInfo, Editor, EditorAction, EditorContext, EditorOutput, FrameHistory, InspectorTab,
    ModelSource, RendererSettings, SceneObject, SceneStats,
};

/// Gives `ctx` the studio fonts, style and options; without the fonts egui 0.37 draws no text at all.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts::studio_fonts());
    theme::install(ctx);
}

#[cfg(test)]
mod tests {
    use egui::{pos2, vec2, Rect, RichText};

    use super::*;

    /// What the C++ editor writes beyond plain words: separators, units and ellipses.
    const EDITOR_STRINGS: &[&str] = &[
        "60 FPS  ·  16.67 ms",
        "1920 × 1080  ·  Release  ·  v3.0.0  ·  F1 hides the editor",
        "45°",
        "Loading Colosseum…",
        "Vulkan  ·  llvmpipe (LLVM 21.1.8, 128 bits)",
    ];

    fn studio_context() -> egui::Context {
        let ctx = egui::Context::default();
        install(&ctx);
        ctx.set_missing_glyph_policy(egui::MissingGlyphPolicy::Panic);
        ctx
    }

    fn screen() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1600.0, 900.0))),
            ..Default::default()
        }
    }

    fn show_in_every_font(ui: &mut egui::Ui, text: &str) {
        for font in [
            fonts::body_font(),
            fonts::caption_font(),
            fonts::title_font(),
            fonts::figure_font(),
            fonts::mono_font(),
        ] {
            ui.label(RichText::new(text).font(font));
        }
    }

    #[test]
    fn every_printable_character_and_editor_symbol_has_a_glyph_in_every_face() {
        let ctx = studio_context();
        let printable: String = (' '..='~').collect();
        let output = ctx.run_ui(screen(), |ui| {
            show_in_every_font(ui, &printable);
            for text in EDITOR_STRINGS {
                show_in_every_font(ui, text);
            }
        });
        output.drop_without_applying_deltas();
    }

    #[test]
    #[should_panic(expected = "No glyph for")]
    fn a_character_neither_face_has_fails_the_glyph_test() {
        let ctx = studio_context();
        let output = ctx.run_ui(screen(), |ui| {
            ui.label("\u{5b57}");
        });
        output.drop_without_applying_deltas();
    }
}
