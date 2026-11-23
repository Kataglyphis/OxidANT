//! BeschleunigerBallett's all-maximum settings sweep (plan X17): the corner, then seeded random settings over every mode.
//! Weekly and on demand only (`KATAGLYPHIS_SETTINGS_SWEEP=1`): the corner path-traces 64 spp at 16 bounces.

use std::sync::Arc;
use std::time::Duration;

use glam::Vec3;
use kataglyphis_webgpu_renderer::render::clouds::CloudSettings;
use kataglyphis_webgpu_renderer::render::forward::RasterPath;
use kataglyphis_webgpu_renderer::render::fxaa::FxaaMode;
use kataglyphis_webgpu_renderer::render::shadow_settings::ShadowSettings;
use kataglyphis_webgpu_renderer::render::tracer::{RenderMode, TraceSettings};
use kataglyphis_webgpu_renderer::scene::{CpuLight, CpuLightKind};
use kataglyphis_webgpu_renderer::ui::RendererSettings;
use kataglyphis_webgpu_renderer::{load_gltf, CpuScene, ForwardRenderer, GpuContext, OrbitCamera};

/// 256 px reach no buffer limit; this size does, as BB's sweep runs it.
const SIZE: (u32, u32) = (1200, 768);
const RANDOM_CASES: u64 = 16;
const MODES: [RenderMode; 3] = [
    RenderMode::Raster,
    RenderMode::RayTraced,
    RenderMode::PathTraced,
];
const PATHS: [RasterPath; 2] = [RasterPath::Forward, RasterPath::Deferred];

fn sweep_requested() -> bool {
    if std::env::var_os("KATAGLYPHIS_SETTINGS_SWEEP").is_some_and(|v| v == "1") {
        return true;
    }
    eprintln!("SKIP: the settings sweep runs weekly; KATAGLYPHIS_SETTINGS_SWEEP=1 runs it here");
    false
}

fn light(kind: CpuLightKind, position: Vec3) -> CpuLight {
    CpuLight {
        kind,
        color: [1.0, 0.9, 0.8],
        intensity: 40.0,
        range: 20.0,
        position: position.to_array(),
        direction: (-position).normalize().to_array(),
        node: None,
    }
}

/// The test cube on its plane with two point and two spot lights, all shadowed in the corner.
fn scene() -> CpuScene {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../assets/tests/assets/cube_on_plane.gltf");
    let mut scene = load_gltf(path).expect("the test scene loads");
    let spot = CpuLightKind::Spot {
        cos_inner: 0.95,
        cos_outer: 0.85,
    };
    scene.lights.extend([
        light(CpuLightKind::Point, Vec3::new(2.0, 2.0, 0.0)),
        light(CpuLightKind::Point, Vec3::new(-2.0, 1.5, 1.0)),
        light(spot, Vec3::new(0.0, 4.0, 2.0)),
        light(spot, Vec3::new(1.0, 3.0, -2.0)),
    ]);
    scene
}

struct Sweep {
    gpu: GpuContext,
    renderer: ForwardRenderer,
    camera: OrbitCamera,
}

impl Sweep {
    fn new() -> Option<Self> {
        let gpu = GpuContext::headless_or_skip()?;
        let scene = scene();
        let mut renderer = ForwardRenderer::new(&gpu, SIZE.0, SIZE.1);
        renderer.upload_scene(&gpu, &scene);
        renderer.set_trace_source(Arc::new(scene));
        let built = renderer
            .tracer_mut()
            .finish_build(&gpu, Duration::from_secs(600));
        assert!(built, "{:?}", renderer.trace_status());
        Some(Self {
            gpu,
            renderer,
            camera: OrbitCamera::default(),
        })
    }

    /// `frames` frames under `settings` inside an error scope: no validation error, no loss, more than one colour.
    fn check(&mut self, name: &str, settings: &RendererSettings, frames: u32) {
        settings.apply(&mut self.renderer, &mut self.gpu);
        let scope = self
            .gpu
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let mut pixels = Vec::new();
        for _ in 0..frames {
            pixels = self
                .renderer
                .render_to_pixels(&self.gpu, SIZE.0, SIZE.1, &self.camera)
                .unwrap_or_else(|err| panic!("{name}: {err:#}"));
        }
        let error = pollster::block_on(scope.pop());
        assert!(error.is_none(), "{name}: {error:?}");
        assert!(!self.gpu.is_lost(), "{name}: the device was lost");
        let first = &pixels[..4];
        assert!(
            pixels.as_chunks::<4>().0.iter().any(|p| p != first),
            "{name}: one colour over the whole frame"
        );
    }
}

fn corner() -> RendererSettings {
    let defaults = RendererSettings::default();
    RendererSettings {
        shadows: ShadowSettings {
            enabled: true,
            resolution: 4096,
            cascade_count: 3,
            pcf_radius: 5,
            punctual: true,
            ..defaults.shadows
        },
        clouds: CloudSettings {
            enabled: true,
            march_steps: 128,
            light_steps: 128,
            half_resolution: false,
            ..defaults.clouds
        },
        raster_path: RasterPath::Deferred,
        fxaa: FxaaMode::Always,
        trace: TraceSettings {
            samples_per_pixel: 64,
            max_bounces: 16,
            furnace: None,
        },
        ..defaults
    }
}

/// xorshift64: the cases repeat for a seed, so a failure names a case that can be run again.
struct Seeded(u64);

impl Seeded {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[(self.next() % items.len() as u64) as usize]
    }

    fn unit(&mut self) -> f32 {
        (self.next() % 10_001) as f32 / 10_000.0
    }
}

/// Case `index`: mode and raster path walk every pair, the rest is drawn from the seed; `apply` clamps the rest.
fn random_settings(index: u64, rng: &mut Seeded) -> RendererSettings {
    let defaults = RendererSettings::default();
    RendererSettings {
        render_mode: MODES[(index % 3) as usize],
        raster_path: PATHS[(index / 3 % 2) as usize],
        fxaa: rng.pick(&[FxaaMode::Off, FxaaMode::Deferred, FxaaMode::Always]),
        bloom_strength: rng.unit(),
        ssao_strength: rng.unit(),
        exposure_ev: rng.unit() * 4.0 - 2.0,
        auto_exposure: rng.pick(&[false, true]),
        occlusion_queries_enabled: rng.pick(&[false, true]),
        gpu_culling_enabled: rng.pick(&[false, true]),
        frustum_culling: rng.pick(&[false, true]),
        skybox_enabled: rng.pick(&[false, true]),
        shadows: ShadowSettings {
            enabled: rng.pick(&[false, true]),
            resolution: rng.pick(&[512, 1024, 2048, 4096]),
            cascade_count: rng.pick(&[1, 2, 3]),
            pcf_radius: rng.pick(&[0, 1, 3, 5]),
            punctual: rng.pick(&[false, true]),
            ..defaults.shadows
        },
        clouds: CloudSettings {
            enabled: rng.pick(&[false, true]),
            march_steps: rng.pick(&[4, 32, 128]),
            light_steps: rng.pick(&[1, 16, 128]),
            half_resolution: rng.pick(&[false, true]),
            ..defaults.clouds
        },
        trace: TraceSettings {
            samples_per_pixel: rng.pick(&[1, 4, 8]),
            max_bounces: rng.pick(&[1, 4, 16]),
            furnace: None,
        },
        ..defaults
    }
}

#[test]
fn every_setting_at_its_maximum_and_sixteen_random_ones_render_without_an_error() {
    if !sweep_requested() {
        return;
    }
    let Some(mut sweep) = Sweep::new() else {
        return;
    };
    for mode in MODES {
        let settings = RendererSettings {
            render_mode: mode,
            ..corner()
        };
        sweep.check(&format!("the corner, {mode:?}"), &settings, 2);
    }
    let mut rng = Seeded(0x5EED_2026_1120);
    for index in 0..RANDOM_CASES {
        let settings = random_settings(index, &mut rng);
        sweep.check(&format!("random case {index}: {settings:?}"), &settings, 3);
    }
}

/// The negative control: the scope the sweep reads reports a validation error on the same device.
#[test]
fn the_sweeps_error_scope_reports_a_validation_error() {
    if !sweep_requested() {
        return;
    }
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    // MAP_READ beside MAP_WRITE is invalid for any buffer.
    let _ = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("invalid on purpose"),
        size: 16,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::MAP_WRITE,
        mapped_at_creation: false,
    });
    assert!(
        pollster::block_on(scope.pop()).is_some(),
        "the scope missed a validation error"
    );
}
