//! What the host hands the editor each frame, all plain data, so the editor runs and tests without a GPU.

use glam::Vec4;
use kataglyphis_render_core::sun::SunSettings;

use crate::adapter::{self, AdapterClass, AdapterHint, AdapterIdentity};
use crate::context::GpuContext;

use crate::app::library::ModelLibrary;
use crate::app::loader::LoadProgress;
use crate::app::Notice;
use crate::render::clouds::CloudSettings;
use crate::render::forward::{ForwardRenderer, RasterPath};
use crate::render::fxaa::FxaaMode;
use crate::render::gpu_timing::TimedPass;
use crate::render::model_transform::ModelTransform;
use crate::render::quality::{QualityKnobs, QualityPreset};
use crate::render::render_scale::RenderScale;
use crate::render::shadow_settings::ShadowSettings;
use crate::render::tracer::{RenderMode, TraceSettings, TraceStatus};

/// The renderer's editable settings: read before the egui pass, written back after it.
#[derive(Clone, Debug, PartialEq)]
pub struct RendererSettings {
    /// Towards the light (xyz), and the ambient strength (w).
    pub light_dir_ambient: Vec4,
    /// Light colour (rgb) and intensity (w).
    pub light_color_intensity: Vec4,
    pub ibl_intensity: f32,
    pub bloom_strength: f32,
    pub ssao_strength: f32,
    pub exposure_ev: f32,
    pub auto_exposure: bool,
    pub auto_exposure_speed: f32,
    pub occlusion_queries_enabled: bool,
    pub gpu_culling_enabled: bool,
    pub shadows: ShadowSettings,
    pub frustum_culling: bool,
    pub skybox_enabled: bool,
    pub model_transform: ModelTransform,
    pub raster_path: RasterPath,
    pub fxaa: FxaaMode,
    pub render_scale: RenderScale,
    pub clouds: CloudSettings,
    /// The user's pick; [`SceneStats::drawn_mode`] is what the frame draws.
    pub render_mode: RenderMode,
    pub trace: TraceSettings,
    /// Set by the Renderer tab's Reset; `apply` starts the path tracer's mean over.
    pub reset_accumulation: bool,
}

impl Default for RendererSettings {
    /// `ForwardRenderer::new`'s values; a GPU test keeps the two equal.
    fn default() -> Self {
        Self {
            light_dir_ambient: SunSettings::default().light_dir_ambient(),
            light_color_intensity: SunSettings::default().light_color_intensity(),
            ibl_intensity: 1.0,
            bloom_strength: 0.6,
            ssao_strength: 0.7,
            exposure_ev: 0.0,
            auto_exposure: false,
            auto_exposure_speed: 3.0,
            occlusion_queries_enabled: false,
            gpu_culling_enabled: false,
            shadows: ShadowSettings::default(),
            frustum_culling: true,
            skybox_enabled: true,
            model_transform: ModelTransform::default(),
            raster_path: RasterPath::Forward,
            fxaa: FxaaMode::Always,
            render_scale: RenderScale::default(),
            clouds: CloudSettings::default(),
            render_mode: RenderMode::Raster,
            trace: TraceSettings::default(),
            reset_accumulation: false,
        }
    }
}

impl RendererSettings {
    pub fn of(renderer: &ForwardRenderer) -> Self {
        Self {
            light_dir_ambient: renderer.light_dir_ambient,
            light_color_intensity: renderer.light_color_intensity,
            ibl_intensity: renderer.ibl_intensity,
            bloom_strength: renderer.bloom_strength,
            ssao_strength: renderer.ssao_strength,
            exposure_ev: renderer.exposure_ev,
            auto_exposure: renderer.auto_exposure,
            auto_exposure_speed: renderer.auto_exposure_speed,
            occlusion_queries_enabled: renderer.occlusion_queries_enabled,
            gpu_culling_enabled: renderer.gpu_culling_enabled,
            shadows: renderer.shadows,
            frustum_culling: renderer.frustum_culling,
            skybox_enabled: renderer.skybox_enabled,
            model_transform: renderer.model_transform,
            raster_path: renderer.raster_path,
            fxaa: renderer.fxaa,
            render_scale: renderer.render_scale,
            clouds: renderer.clouds,
            render_mode: renderer.render_mode,
            trace: renderer.trace_settings,
            reset_accumulation: false,
        }
    }

    pub fn apply(&self, renderer: &mut ForwardRenderer) {
        renderer.light_dir_ambient = self.light_dir_ambient;
        renderer.light_color_intensity = self.light_color_intensity;
        renderer.ibl_intensity = self.ibl_intensity;
        renderer.bloom_strength = self.bloom_strength;
        renderer.ssao_strength = self.ssao_strength;
        renderer.exposure_ev = self.exposure_ev;
        renderer.auto_exposure = self.auto_exposure;
        renderer.auto_exposure_speed = self.auto_exposure_speed;
        renderer.occlusion_queries_enabled = self.occlusion_queries_enabled;
        renderer.gpu_culling_enabled = self.gpu_culling_enabled;
        renderer.shadows = self.shadows;
        renderer.frustum_culling = self.frustum_culling;
        renderer.skybox_enabled = self.skybox_enabled;
        renderer.model_transform = self.model_transform;
        renderer.raster_path = self.raster_path;
        renderer.fxaa = self.fxaa;
        renderer.render_scale = self.render_scale;
        renderer.clouds = self.clouds;
        renderer.render_mode = self.render_mode;
        renderer.trace_settings = self.trace.clamped();
        if self.reset_accumulation {
            renderer.reset_accumulation();
        }
    }
}

impl RendererSettings {
    /// The knobs the quality presets set.
    pub fn quality_knobs(&self) -> QualityKnobs {
        QualityKnobs::of(
            (self.raster_path, self.fxaa),
            (self.ssao_strength, self.bloom_strength),
            &self.shadows,
            self.render_scale,
        )
    }

    /// The preset the settings are at, or `None` (Custom).
    pub fn quality_preset(&self) -> Option<QualityPreset> {
        QualityPreset::matching(&self.quality_knobs())
    }

    /// Sets what `preset` owns; Auto's target frame rate stays the user's.
    pub fn apply_preset(&mut self, preset: QualityPreset) {
        let knobs = preset.knobs();
        self.raster_path = knobs.raster_path;
        self.fxaa = knobs.fxaa;
        self.ssao_strength = knobs.ssao_strength;
        self.bloom_strength = knobs.bloom_strength;
        knobs.apply_shadows(&mut self.shadows);
        self.render_scale = RenderScale {
            target_fps: self.render_scale.target_fps,
            ..knobs.render_scale
        };
    }
}

/// Last frame's renderer readings, for the profiler and the culling rows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneStats {
    /// Opaque primitives drawn and considered.
    pub occlusion: (u32, u32),
    /// Shadow casters drawn and considered, over every cascade.
    pub shadow_casters: (u32, u32),
    /// Smoothed GPU milliseconds per timed pass, by its label; a pass without a sample is absent, not zero.
    pub gpu_passes: Vec<(&'static str, f32)>,
    pub gpu_timing: bool,
    /// Primitives the camera pass drew, and primitives uploaded.
    pub primitives: (u32, u32),
    /// A cube map is loaded, so the skybox is the C++ one rather than the procedural sky.
    pub cubemap: bool,
    /// Where the traced modes' scene is.
    pub trace: TraceStatus,
    /// What the frame draws: raster while a traced pick waits for its scene, or while its accumulation does not fit.
    pub drawn_mode: RenderMode,
    /// Why a traced pick with its scene ready rasters, see [`ForwardRenderer::traced_fallback`].
    pub trace_fallback: Option<String>,
    /// Samples per pixel in the path tracer's running mean.
    pub accumulated_samples: u64,
}

impl SceneStats {
    pub fn of(renderer: &ForwardRenderer) -> Self {
        Self {
            occlusion: renderer.occlusion_cull_stats(),
            shadow_casters: renderer.shadow_caster_stats(),
            gpu_passes: labelled(renderer.gpu_timings_ms()),
            gpu_timing: renderer.gpu_timing_available(),
            primitives: renderer.primitive_stats(),
            cubemap: renderer.skybox_loaded(),
            trace: renderer.trace_status(),
            drawn_mode: renderer.drawn_mode(),
            trace_fallback: renderer.traced_fallback(),
            accumulated_samples: renderer.accumulated_samples(),
        }
    }
}

/// The profiler's readable pass names for the renderer's identifiers.
fn labelled(passes: Vec<(&'static str, f32)>) -> Vec<(&'static str, f32)> {
    passes
        .into_iter()
        .map(|(name, ms)| {
            let pass = TimedPass::ALL.into_iter().find(|pass| pass.name() == name);
            (pass.map_or(name, TimedPass::label), ms)
        })
        .collect()
}

/// The scenes as the scene dock, the status bar, the viewport chip and About show them.
#[derive(Clone, Copy, Debug)]
pub struct SceneStatus<'a> {
    pub library: &'a ModelLibrary,
    /// The row the host actually loaded, which the library marks.
    pub loaded: Option<usize>,
    pub loading: Option<&'a LoadProgress>,
    pub notice: Option<&'a Notice>,
    /// The loaded scene's `asset.copyright`, which CC BY asks to show.
    pub credit: Option<&'a str>,
}

impl SceneStatus<'static> {
    /// No library, nothing loaded or loading.
    pub fn empty() -> Self {
        static EMPTY: ModelLibrary = ModelLibrary::empty();
        Self {
            library: &EMPTY,
            loaded: None,
            loading: None,
            notice: None,
            credit: None,
        }
    }
}

/// The C++ profiler's frame-time history.
pub const FRAME_HISTORY: usize = 120;
/// The C++ `io.Framerate` averages this many frames.
pub const FRAME_RATE_WINDOW: usize = 60;

/// Frame times in milliseconds, recorded by the host before the pass so the UI stays a pure function of it.
#[derive(Clone, Debug)]
pub struct FrameHistory {
    samples_ms: [f32; FRAME_HISTORY],
    next: usize,
    len: usize,
    last_s: Option<f64>,
}

impl Default for FrameHistory {
    fn default() -> Self {
        Self {
            samples_ms: [0.0; FRAME_HISTORY],
            next: 0,
            len: 0,
            last_s: None,
        }
    }
}

impl FrameHistory {
    /// Records the time since the previous tick; the first tick, or a clock that did not advance, records nothing.
    pub fn tick(&mut self, now_s: f64) {
        if let Some(last) = self.last_s.replace(now_s).filter(|last| now_s > *last) {
            self.record_ms(((now_s - last) * 1000.0) as f32);
        }
    }

    pub fn record_ms(&mut self, milliseconds: f32) {
        self.samples_ms[self.next] = milliseconds;
        self.next = (self.next + 1) % FRAME_HISTORY;
        self.len = (self.len + 1).min(FRAME_HISTORY);
    }

    /// The C++ double-click window: one and a half of the last two frames, 0.3 to 2 seconds.
    pub fn double_click_delay_s(&self) -> f64 {
        let last_two: f32 = self.samples_ms().skip(self.len.saturating_sub(2)).sum();
        (1.5 * f64::from(last_two) / 1000.0).clamp(0.3, 2.0)
    }

    /// Oldest first.
    pub fn samples_ms(&self) -> impl Iterator<Item = f32> + '_ {
        let start = if self.len == FRAME_HISTORY {
            self.next
        } else {
            0
        };
        (0..self.len).map(move |i| self.samples_ms[(start + i) % FRAME_HISTORY])
    }

    /// The mean of the newest `count` samples, or `None` before the first.
    pub fn mean_ms(&self, count: usize) -> Option<f32> {
        let count = count.min(self.len);
        if count == 0 {
            return None;
        }
        let newest = self.samples_ms().skip(self.len - count);
        Some(newest.sum::<f32>() / count as f32)
    }

    /// The median of the newest `count` samples, which one hitch does not move; `None` before the first.
    pub fn median_ms(&self, count: usize) -> Option<f32> {
        let count = count.min(self.len);
        let mut newest: Vec<f32> = self.samples_ms().skip(self.len - count).collect();
        newest.sort_by(f32::total_cmp);
        newest.get(count / 2).copied()
    }
}

/// What the status bar, About and the Renderer tab say about the adapter.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Vulkan, Metal, Direct3D 12, OpenGL or WebGPU.
    pub api: String,
    pub name: String,
    pub driver: String,
    /// A software rasterizer, whose frame rates say nothing about the renderer.
    pub cpu: bool,
    pub class: AdapterClass,
    /// The status bar's word on an integrated adapter, with its tooltip.
    pub hint: Option<AdapterHint>,
    /// Natively, the adapters a relaunch can pick, by [`crate::adapter::entry_label`]; empty in a browser.
    pub adapters: Vec<String>,
    /// Which of `adapters` the renderer runs on.
    pub current: Option<usize>,
}

impl DeviceInfo {
    /// From the adapter alone, as a native context without a list describes it.
    pub fn of(info: &wgpu::AdapterInfo) -> Self {
        Self::describe(info, &AdapterIdentity::of(info, None))
    }

    /// The context's adapter, its browser-side name, the hint and the adapters it was chosen from.
    pub fn of_context(gpu: &GpuContext) -> Self {
        let mut device = Self::describe(&gpu.adapter_info, &gpu.identity);
        let browser = gpu.adapter_info.backend == wgpu::Backend::BrowserWebGpu;
        let discrete = gpu
            .adapters
            .iter()
            .any(|info| info.device_type == wgpu::DeviceType::DiscreteGpu);
        device.hint = adapter::hint(device.class, browser, discrete);
        device.adapters = gpu.adapters.iter().map(adapter::entry_label).collect();
        device.current = gpu.adapter_index;
        device
    }

    fn describe(info: &wgpu::AdapterInfo, identity: &AdapterIdentity) -> Self {
        let driver = format!("{} {}", info.driver, info.driver_info);
        Self {
            api: adapter::api_name(info.backend).to_owned(),
            name: identity.label.clone(),
            driver: driver.trim().to_owned(),
            cpu: identity.class == AdapterClass::Cpu,
            class: identity.class,
            hint: None,
            adapters: Vec::new(),
            current: None,
        }
    }
}

/// The loaded model as the outliner and the inspector name it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelSource {
    /// Short, such as `Dinosaurs/dinosaurs`.
    pub name: String,
    /// Where it came from: a path natively, a scene name in the browser.
    pub path: String,
}

impl ModelSource {
    /// Named by its folder and file stem, the C++ display name.
    pub fn from_path(path: &std::path::Path) -> Self {
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
        let folder = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned());
        let name = match (folder, stem) {
            (Some(folder), Some(stem)) => format!("{folder}/{stem}"),
            (None, Some(stem)) => stem,
            _ => path.display().to_string(),
        };
        Self {
            name,
            path: path.display().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::tracer::FurnaceSettings;

    #[test]
    fn the_history_keeps_the_newest_120_frames_oldest_first() {
        let mut history = FrameHistory::default();
        assert_eq!(history.mean_ms(FRAME_RATE_WINDOW), None);
        history.tick(10.0);
        assert_eq!(
            history.samples_ms().count(),
            0,
            "the first tick only starts the clock"
        );
        for frame in 1..=130 {
            history.tick(10.0 + f64::from(frame) * 0.02);
        }
        let samples: Vec<f32> = history.samples_ms().collect();
        assert_eq!(samples.len(), FRAME_HISTORY);
        assert!(
            samples.iter().all(|ms| (ms - 20.0).abs() < 1e-3),
            "{samples:?}"
        );
        history.record_ms(80.0);
        assert_eq!(history.samples_ms().last(), Some(80.0));
        let mean = history.mean_ms(2).unwrap();
        assert!((mean - 50.0).abs() < 1e-3, "{mean}");
        // 100 ms of two frames would give 0.15 s, under the 0.3 s floor.
        assert_eq!(history.double_click_delay_s(), 0.3);
        history.record_ms(900.0);
        assert_eq!(history.double_click_delay_s(), 1.47);
        assert_eq!(history.median_ms(3), Some(80.0), "20, 80 and 900");
        history.tick(9.0);
        assert_eq!(
            history.samples_ms().last(),
            Some(900.0),
            "a clock that steps back records nothing"
        );
    }

    #[test]
    fn the_profiler_shows_labels_and_keeps_a_name_it_does_not_know() {
        let passes = labelled(vec![
            ("ShadowCascades", 0.5),
            ("Main", 1.0),
            ("Forward", 1.0),
            ("Gui", 0.1),
        ]);
        assert_eq!(
            passes,
            [
                ("Shadow cascades", 0.5),
                ("Main (raster/traced)", 1.0),
                ("  Forward + sky", 1.0),
                ("Gui", 0.1)
            ]
        );
        let labels: std::collections::HashSet<&str> =
            TimedPass::ALL.into_iter().map(TimedPass::label).collect();
        assert_eq!(labels.len(), TimedPass::ALL.len());
        // BeschleunigerBallett's timing script reads the arms of the first `fn name(self)`.
        let source = include_str!("../../render/gpu_timing.rs");
        let name = source.find("fn name(self)").expect("name() exists");
        assert!(name < source.find("fn label(self)").expect("label() exists"));
    }

    #[test]
    fn a_model_is_named_by_its_folder_and_stem() {
        let source = ModelSource::from_path(std::path::Path::new("Models/Dinosaurs/dinosaurs.glb"));
        assert_eq!(source.name, "Dinosaurs/dinosaurs");
        assert_eq!(source.path, "Models/Dinosaurs/dinosaurs.glb");
        assert_eq!(
            ModelSource::from_path(std::path::Path::new("cube.gltf")).name,
            "cube"
        );
    }

    /// Every field away from its default, written as a literal so a new field must join it.
    fn changed_settings() -> RendererSettings {
        let defaults = RendererSettings::default();
        RendererSettings {
            light_dir_ambient: Vec4::new(0.0, 1.0, 0.0, 0.5),
            light_color_intensity: Vec4::new(0.5, 0.25, 1.0, 2.0),
            ibl_intensity: 0.25,
            bloom_strength: 0.1,
            ssao_strength: 0.1,
            exposure_ev: 1.5,
            auto_exposure: !defaults.auto_exposure,
            auto_exposure_speed: 1.0,
            occlusion_queries_enabled: !defaults.occlusion_queries_enabled,
            gpu_culling_enabled: !defaults.gpu_culling_enabled,
            shadows: ShadowSettings {
                enabled: false,
                resolution: 512,
                cascade_count: 1,
                pcf_radius: 5,
                strength: 0.2,
                distance: 100.0,
                split_lambda: 0.5,
                punctual: false,
            },
            frustum_culling: !defaults.frustum_culling,
            skybox_enabled: !defaults.skybox_enabled,
            model_transform: ModelTransform {
                position: glam::Vec3::X,
                rotation_deg: glam::Vec3::new(0.0, 90.0, 0.0),
            },
            raster_path: RasterPath::Deferred,
            fxaa: FxaaMode::Deferred,
            render_scale: RenderScale {
                scale: 0.5,
                auto: true,
                target_fps: 30.0,
            },
            clouds: CloudSettings {
                enabled: true,
                march_steps: 16,
                ..defaults.clouds
            },
            render_mode: RenderMode::PathTraced,
            trace: TraceSettings {
                samples_per_pixel: 2,
                max_bounces: 3,
                furnace: Some(FurnaceSettings::default()),
            },
            reset_accumulation: true,
        }
    }

    #[test]
    fn the_changed_settings_differ_from_the_defaults_in_every_field() {
        let (changed, defaults) = (changed_settings(), RendererSettings::default());
        macro_rules! same {
            ($($field:ident),*) => { [$((stringify!($field), changed.$field == defaults.$field)),*] };
        }
        let same = same!(
            light_dir_ambient,
            light_color_intensity,
            ibl_intensity,
            bloom_strength,
            ssao_strength,
            exposure_ev,
            auto_exposure,
            auto_exposure_speed,
            occlusion_queries_enabled,
            gpu_culling_enabled,
            shadows,
            frustum_culling,
            skybox_enabled,
            model_transform,
            raster_path,
            fxaa,
            render_scale,
            clouds,
            render_mode,
            trace,
            reset_accumulation
        );
        let unchanged: Vec<&str> = same
            .iter()
            .filter(|(_, same)| *same)
            .map(|(name, _)| *name)
            .collect();
        assert!(unchanged.is_empty(), "still at the default: {unchanged:?}");
    }

    #[test]
    fn the_default_sun_is_render_cores() {
        let (defaults, sun) = (RendererSettings::default(), SunSettings::default());
        assert_eq!(defaults.light_dir_ambient, sun.light_dir_ambient());
        assert_eq!(defaults.light_color_intensity, sun.light_color_intensity());
    }

    #[test]
    fn the_settings_start_from_the_renderer_and_write_back_what_they_show() {
        let Some(gpu) = crate::GpuContext::headless_or_skip() else {
            return;
        };
        let mut renderer = ForwardRenderer::new(&gpu, 8, 8);
        assert_eq!(RendererSettings::of(&renderer), RendererSettings::default());
        let settings = changed_settings();
        settings.apply(&mut renderer);
        assert_eq!(renderer.shadows, settings.shadows);
        assert_eq!(renderer.model_transform, settings.model_transform);
        assert_eq!(renderer.clouds, settings.clouds);
        assert_eq!(renderer.render_mode, settings.render_mode);
        assert_eq!(renderer.trace_settings, settings.trace);
        // A Reset is a one-frame request the renderer does not keep.
        let kept = RendererSettings {
            reset_accumulation: false,
            ..settings
        };
        assert_eq!(RendererSettings::of(&renderer), kept);
    }
}
