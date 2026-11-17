//! The Renderer tab's quality presets and render scale, and the chip's internal size.

use super::*;
use crate::render::forward::RasterPath;
use crate::render::quality::QualityPreset;

#[test]
fn a_preset_click_sets_its_knobs_and_the_chip_shows_the_internal_size() {
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    studio.editor.select(SceneObject::Renderer);
    studio.frame(Vec::new());
    assert_eq!(studio.settings.quality_preset(), Some(QualityPreset::High));
    click_text(&mut studio, "Low");
    assert_eq!(studio.settings.quality_preset(), Some(QualityPreset::Low));
    assert_eq!(studio.settings.raster_path, RasterPath::Deferred);
    assert!(studio.settings.render_scale.auto);
    let texts = painted_texts(&mut studio);
    let scaled = texts.iter().any(|(text, _)| text.ends_with("(75 %)"));
    assert!(scaled, "the chip shows the internal size: {texts:?}");
    assert!(texts
        .iter()
        .any(|(text, _)| text.starts_with("Renders 1200 × 675 of 1600 × 900")));
    studio.settings.bloom_strength = 0.3;
    assert_eq!(studio.settings.quality_preset(), None, "an edit is Custom");
    click_text(&mut studio, "High");
    assert_eq!(studio.settings, RendererSettings::default());
}
