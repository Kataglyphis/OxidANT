//! The editor's look: BeschleunigerBallett's Studio Pro palette, style and fonts, in egui.

pub mod fonts;
pub mod palette;
pub mod theme;

/// Gives `ctx` the studio fonts, style and options; without the fonts egui 0.37 draws no text at all.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts::studio_fonts());
    theme::install(ctx);
}

#[cfg(test)]
mod tests {
    use egui::{pos2, vec2, Rect, RichText};
    use glam::Vec4;

    use super::*;
    use crate::render::overlay::OverlayControls;

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
    fn every_character_the_editor_writes_has_a_glyph() {
        let ctx = studio_context();
        let printable: String = (' '..='~').collect();
        let mut controls = OverlayControls::from_light(Vec4::new(0.5, 0.8, 0.3, 0.4), Vec4::ONE);
        controls.occlusion_culling = true;
        controls.occlusion_stats = Some((3, 7));
        // Two passes: a new window lays its contents out in an invisible sizing pass first.
        for _ in 0..2 {
            let output = ctx.run_ui(screen(), |ui| {
                show_in_every_font(ui, &printable);
                for text in EDITOR_STRINGS {
                    show_in_every_font(ui, text);
                }
                controls.ui(ui.ctx());
            });
            output.drop_without_applying_deltas();
        }
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
