//! What the host hands the editor each frame, all plain data, so the editor runs and tests without a GPU.

use glam::Vec4;

use crate::render::forward::ForwardRenderer;
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
            light_dir_ambient: Vec4::new(0.5, 0.8, 0.3, 0.35),
            light_color_intensity: Vec4::new(1.0, 0.97, 0.92, 5.0),
            ibl_intensity: 1.0,
            bloom_strength: 0.6,
            ssao_strength: 0.7,
            exposure_ev: 0.0,
            auto_exposure: false,
            auto_exposure_speed: 3.0,
            occlusion_queries_enabled: false,
            gpu_culling_enabled: false,
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
        renderer.render_mode = self.render_mode;
        renderer.trace_settings = self.trace.clamped();
        if self.reset_accumulation {
            renderer.reset_accumulation();
        }
    }
}

/// Last frame's renderer readings, for the profiler and the culling rows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneStats {
    /// Opaque primitives drawn and considered.
    pub occlusion: (u32, u32),
    /// Shadow casters drawn and considered, over every cascade.
    pub shadow_casters: (u32, u32),
    /// Smoothed GPU milliseconds per timed pass; a pass without a sample is absent, not zero.
    pub gpu_passes: Vec<(&'static str, f32)>,
    pub gpu_timing: bool,
    /// Where the traced modes' scene is.
    pub trace: TraceStatus,
    /// What the frame draws: raster while a traced pick waits for its scene.
    pub drawn_mode: RenderMode,
    /// Samples per pixel in the path tracer's running mean.
    pub accumulated_samples: u64,
}

impl SceneStats {
    pub fn of(renderer: &ForwardRenderer) -> Self {
        Self {
            occlusion: renderer.occlusion_cull_stats(),
            shadow_casters: renderer.shadow_caster_stats(),
            gpu_passes: renderer.gpu_timings_ms(),
            gpu_timing: renderer.gpu_timing_available(),
            trace: renderer.trace_status(),
            drawn_mode: renderer.drawn_mode(),
            accumulated_samples: renderer.accumulated_samples(),
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
}

/// What the status bar and About say about the adapter.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Vulkan, Metal, Direct3D 12, OpenGL or WebGPU.
    pub api: String,
    pub name: String,
    pub driver: String,
    /// A software rasterizer, whose frame rates say nothing about the renderer.
    pub cpu: bool,
}

impl DeviceInfo {
    pub fn of(info: &wgpu::AdapterInfo) -> Self {
        let api = match info.backend {
            wgpu::Backend::Vulkan => "Vulkan",
            wgpu::Backend::Metal => "Metal",
            wgpu::Backend::Dx12 => "Direct3D 12",
            wgpu::Backend::Gl => "OpenGL",
            wgpu::Backend::BrowserWebGpu => "WebGPU",
            wgpu::Backend::Noop => "No-op",
        };
        let driver = format!("{} {}", info.driver, info.driver_info);
        Self {
            api: api.to_owned(),
            name: info.name.clone(),
            driver: driver.trim().to_owned(),
            cpu: info.device_type == wgpu::DeviceType::Cpu,
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
        history.tick(9.0);
        assert_eq!(
            history.samples_ms().last(),
            Some(900.0),
            "a clock that steps back records nothing"
        );
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

    #[test]
    fn the_settings_start_from_the_renderer_and_write_back_what_they_show() {
        let Some(gpu) = crate::GpuContext::headless_or_skip() else {
            return;
        };
        let mut renderer = ForwardRenderer::new(&gpu, 8, 8);
        assert_eq!(RendererSettings::of(&renderer), RendererSettings::default());
        let mut settings = RendererSettings::of(&renderer);
        settings.ssao_strength = 0.1;
        settings.light_dir_ambient = Vec4::new(0.0, 1.0, 0.0, 0.5);
        settings.render_mode = RenderMode::PathTraced;
        settings.apply(&mut renderer);
        assert_eq!(renderer.render_mode, RenderMode::PathTraced);
        assert_eq!(renderer.ssao_strength, 0.1);
        assert_eq!(renderer.light_dir_ambient, Vec4::new(0.0, 1.0, 0.0, 0.5));
        assert_eq!(RendererSettings::of(&renderer), settings);
    }
}
