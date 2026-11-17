//! Quality presets over the renderer's existing knobs, and the one an adapter starts at.

use crate::adapter::AdapterClass;
use crate::render::forward::RasterPath;
use crate::render::fxaa::FxaaMode;
use crate::render::render_scale::RenderScale;
use crate::render::shadow_settings::ShadowSettings;

/// The Renderer tab's presets; High is the renderer's defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QualityPreset {
    Low,
    Medium,
    High,
    Ultra,
}

/// What a preset sets; every other setting keeps its value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QualityKnobs {
    pub raster_path: RasterPath,
    pub fxaa: FxaaMode,
    pub ssao_strength: f32,
    pub bloom_strength: f32,
    pub shadows_enabled: bool,
    pub shadow_resolution: u32,
    pub shadow_cascades: u32,
    pub pcf_radius: u32,
    /// Auto and its starting scale, or a fixed scale.
    pub render_scale: RenderScale,
}

impl QualityPreset {
    /// The editor's order.
    pub const ALL: [Self; 4] = [Self::Low, Self::Medium, Self::High, Self::Ultra];

    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Ultra => "Ultra",
        }
    }

    /// Low and Medium leave MSAA for the deferred path and FXAA, Low also SSAO and bloom; both steer the scale.
    pub fn knobs(self) -> QualityKnobs {
        let high = QualityKnobs {
            raster_path: RasterPath::Forward,
            fxaa: FxaaMode::Deferred,
            ssao_strength: 0.7,
            bloom_strength: 0.6,
            shadows_enabled: true,
            shadow_resolution: 2048,
            shadow_cascades: 3,
            pcf_radius: 2,
            render_scale: RenderScale::default(),
        };
        let auto = |scale| RenderScale {
            scale,
            auto: true,
            ..RenderScale::default()
        };
        match self {
            Self::Low => QualityKnobs {
                raster_path: RasterPath::Deferred,
                ssao_strength: 0.0,
                bloom_strength: 0.0,
                shadow_resolution: 1024,
                shadow_cascades: 1,
                pcf_radius: 1,
                render_scale: auto(0.75),
                ..high
            },
            Self::Medium => QualityKnobs {
                raster_path: RasterPath::Deferred,
                shadow_resolution: 1024,
                shadow_cascades: 2,
                pcf_radius: 1,
                render_scale: auto(1.0),
                ..high
            },
            Self::High => high,
            Self::Ultra => QualityKnobs {
                fxaa: FxaaMode::Always,
                shadow_resolution: 4096,
                pcf_radius: 3,
                ..high
            },
        }
    }

    /// Natively High; in a browser High on a discrete GPU, Medium where it may share power with the CPU, Low on a CPU.
    pub fn for_adapter(class: AdapterClass, browser: bool) -> Self {
        if !browser {
            return Self::High;
        }
        match class {
            AdapterClass::Discrete => Self::High,
            AdapterClass::Cpu => Self::Low,
            AdapterClass::Integrated | AdapterClass::Virtual | AdapterClass::Unknown => {
                Self::Medium
            }
        }
    }

    /// The preset `knobs` are, or `None` (Custom); Auto's moving scale is not compared.
    pub fn matching(knobs: &QualityKnobs) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|preset| preset.knobs().same_as(knobs))
    }
}

impl QualityKnobs {
    /// Read from the renderer's settings.
    pub fn of(
        (raster_path, fxaa): (RasterPath, FxaaMode),
        (ssao_strength, bloom_strength): (f32, f32),
        shadows: &ShadowSettings,
        render_scale: RenderScale,
    ) -> Self {
        Self {
            raster_path,
            fxaa,
            ssao_strength,
            bloom_strength,
            shadows_enabled: shadows.enabled,
            shadow_resolution: shadows.resolution,
            shadow_cascades: shadows.cascade_count,
            pcf_radius: shadows.pcf_radius,
            render_scale,
        }
    }

    /// Writes the shadow fields a preset owns into `shadows`, keeping its strength, distance and split.
    pub fn apply_shadows(&self, shadows: &mut ShadowSettings) {
        shadows.enabled = self.shadows_enabled;
        shadows.resolution = self.shadow_resolution;
        shadows.cascade_count = self.shadow_cascades;
        shadows.pcf_radius = self.pcf_radius;
    }

    fn same_as(&self, other: &Self) -> bool {
        let scale = if self.render_scale.auto {
            other.render_scale.auto
        } else {
            !other.render_scale.auto && self.render_scale.scale == other.render_scale.scale
        };
        scale
            && Self {
                render_scale: other.render_scale,
                ..*self
            } == *other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> QualityKnobs {
        let settings = crate::ui::editor::RendererSettings::default();
        QualityKnobs::of(
            (settings.raster_path, settings.fxaa),
            (settings.ssao_strength, settings.bloom_strength),
            &settings.shadows,
            settings.render_scale,
        )
    }

    #[test]
    fn high_is_the_renderers_defaults() {
        assert_eq!(QualityPreset::High.knobs(), defaults());
        assert_eq!(
            QualityPreset::matching(&defaults()),
            Some(QualityPreset::High)
        );
    }

    #[test]
    fn every_preset_matches_itself_and_only_itself() {
        for preset in QualityPreset::ALL {
            assert_eq!(QualityPreset::matching(&preset.knobs()), Some(preset));
        }
    }

    #[test]
    fn low_leaves_msaa_ssao_and_bloom_and_steers_the_scale() {
        let low = QualityPreset::Low.knobs();
        assert_eq!(
            low.raster_path,
            RasterPath::Deferred,
            "deferred has no MSAA"
        );
        assert!(low.fxaa.filters(true), "FXAA smooths the deferred frame");
        assert_eq!((low.ssao_strength, low.bloom_strength), (0.0, 0.0));
        assert_eq!(low.shadow_cascades, 1);
        assert!(low.render_scale.auto && low.render_scale.scale < 1.0);
    }

    #[test]
    fn auto_matches_at_any_scale_and_an_edit_is_custom() {
        let mut low = QualityPreset::Low.knobs();
        low.render_scale.scale = 0.4;
        assert_eq!(QualityPreset::matching(&low), Some(QualityPreset::Low));
        let mut high = QualityPreset::High.knobs();
        high.render_scale.scale = 0.9;
        assert_eq!(
            QualityPreset::matching(&high),
            None,
            "a fixed scale is compared"
        );
        let mut medium = QualityPreset::Medium.knobs();
        medium.bloom_strength = 0.5;
        assert_eq!(QualityPreset::matching(&medium), None);
    }

    #[test]
    fn a_preset_keeps_the_shadow_fields_it_does_not_own() {
        let mut shadows = ShadowSettings {
            strength: 0.3,
            distance: 120.0,
            ..ShadowSettings::default()
        };
        QualityPreset::Low.knobs().apply_shadows(&mut shadows);
        assert_eq!((shadows.cascade_count, shadows.resolution), (1, 1024));
        assert_eq!((shadows.strength, shadows.distance), (0.3, 120.0));
    }

    #[test]
    fn the_startup_preset_follows_the_adapter_in_a_browser_only() {
        use AdapterClass as A;
        for class in [A::Discrete, A::Integrated, A::Cpu, A::Unknown] {
            assert_eq!(
                QualityPreset::for_adapter(class, false),
                QualityPreset::High
            );
        }
        assert_eq!(
            QualityPreset::for_adapter(A::Discrete, true),
            QualityPreset::High
        );
        assert_eq!(
            QualityPreset::for_adapter(A::Integrated, true),
            QualityPreset::Medium
        );
        assert_eq!(
            QualityPreset::for_adapter(A::Unknown, true),
            QualityPreset::Medium
        );
        assert_eq!(QualityPreset::for_adapter(A::Cpu, true), QualityPreset::Low);
    }
}
