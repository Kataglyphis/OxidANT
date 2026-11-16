//! Forward PBR pass into an HDR target, ACES-tonemapped to the output, plus headless readback.
//! The frame's passes and the resources each one owns live in the submodules, one per pass.

mod animate;
mod cloud_pass;
mod culling;
mod deferred;
mod environment;
mod frame;
mod light_grid;
mod main_pass;
mod placement;
mod post;
mod primitive;
pub(crate) mod readback;
mod shadow;
mod shadow_casters;
mod sky;
mod targets;
mod traced;
mod upload;

use glam::{Mat4, Vec3, Vec4};

use crate::context::GpuContext;
use crate::render::clouds::CloudSettings;
use crate::render::fxaa::FxaaMode;
use crate::render::gpu_occlusion::GpuCulling;
use crate::render::gpu_timing::GpuTiming;
use crate::render::ibl::{BrdfLut, EquirectImage, IblEnvironment};
pub use crate::render::lights::MAX_PUNCTUAL_LIGHTS;
use crate::render::model_transform::ModelTransform;
use crate::render::shadow_settings::ShadowSettings;
use crate::render::texture::create_2d_texture;
use crate::render::tonemap::TonemapPass;
#[cfg(not(target_arch = "wasm32"))]
use crate::render::tracer::modes::{TracedPasses, TracedSources};
use crate::render::tracer::{RenderMode, TraceSettings, Tracer};
use crate::scene::camera::OrbitCamera;
use crate::scene::CpuScene;

use animate::SceneAnimation;
use cloud_pass::CloudPass;
#[cfg(not(target_arch = "wasm32"))]
use cloud_pass::CloudPipelines;
use culling::Culling;
use deferred::DeferredResources;
pub use deferred::RasterPath;
use environment::{create_ibl_bind_group, IblBinding};
use frame::FrameBinding;
use light_grid::LightResources;
use main_pass::MainPipelines;
use post::PostChain;
use primitive::GpuPrimitive;
use shadow::ShadowResources;
pub use sky::SkyMode;
use sky::SkyResources;
use targets::{DepthResolve, RenderTargets};
use traced::TracedState;
use upload::{MaterialDefaults, MaterialTextureCache};

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// MSAA sample count of the forward HDR and depth targets. Never 1: the forward pass always
/// resolves colour, and the depth-resolve shader and layout read a multisampled depth texture.
const MSAA_SAMPLE_COUNT: u32 = 4;
const _: () = assert!(
    MSAA_SAMPLE_COUNT > 1,
    "the forward pass resolves MSAA colour and depth, so it needs more than one sample"
);
/// The default texels per side of each cascade's map; [`ShadowSettings::resolution`] picks another.
pub const SHADOW_MAP_SIZE: u32 = 2048;
/// The most cascades, the C++ `MAX_CASCADES`; [`ShadowSettings::cascade_count`] renders 1 to this many.
pub const CASCADE_COUNT: usize = 3;

/// The WGSL a shader reload rebuilds the scene, shadow, sky, deferred, cloud and traced pipelines from.
/// The tonemap, FXAA, bloom, SSAO, histogram and depth-resolve passes keep the WGSL the crate was built with.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug)]
pub struct ShaderSources<'a> {
    pub forward: &'a str,
    pub sky: &'a str,
    pub cloud_march: &'a str,
    pub cloud_noise: &'a str,
    /// Both traced modes' kernels.
    pub traced: &'a str,
    pub trace_resolve: &'a str,
}

#[cfg(not(target_arch = "wasm32"))]
impl ShaderSources<'static> {
    /// The generated files, in field order, as a hot reload watches them.
    pub const FILES: [&'static str; 6] = [
        "forward.wgsl",
        "sky.wgsl",
        "cloud_march.wgsl",
        "cloud_noise.wgsl",
        "traced.wgsl",
        "trace_resolve.wgsl",
    ];

    /// The WGSL this crate was built with.
    pub fn shipped() -> Self {
        let traced = TracedSources::shipped();
        Self {
            forward: include_str!("../shaders/forward.wgsl"),
            sky: include_str!("../shaders/sky.wgsl"),
            cloud_march: include_str!("../shaders/cloud_march.wgsl"),
            cloud_noise: include_str!("../shaders/cloud_noise.wgsl"),
            traced: traced.traced,
            trace_resolve: traced.resolve,
        }
    }
}

pub struct ForwardRenderer {
    /// The forward pass's four pipelines and its group 0 layout.
    main: MainPipelines,
    sky: SkyResources,
    shadow: ShadowResources,
    /// Group 2: per-frame uniforms shared by the forward and shadow pipelines.
    frame: FrameBinding,
    /// Group 3: punctual lights and the tile light grid.
    lights: LightResources,
    /// Group 1: the IBL maps, baked or fallback.
    ibl: IblBinding,
    materials: MaterialDefaults,
    targets: RenderTargets,
    depth_resolve: DepthResolve,
    /// Bloom, SSAO and exposure, which the tonemap composes.
    post: PostChain,
    /// The cloud noise, march and target, which the tonemap composites.
    cloud_pass: CloudPass,
    culling: Culling,
    /// The deferred path's G-buffer and pipelines, built by its first frame.
    deferred: Option<DeferredResources>,
    primitives: Vec<GpuPrimitive>,
    scene_bounds: Option<(Vec3, Vec3)>,
    /// GPU textures created by the last `upload_scene`, so tests can prove the dedup.
    uploaded_texture_count: usize,
    animation: SceneAnimation,
    /// Bloom contribution mixed in by the tonemap pass.
    pub bloom_strength: f32,
    /// SSAO strength applied by the tonemap pass (0 = off).
    pub ssao_strength: f32,
    /// Exposure in EV stops (0 = neutral).
    pub exposure_ev: f32,
    /// Drive exposure from the scene histogram instead of `exposure_ev`.
    pub auto_exposure: bool,
    /// Adaptation rate; higher settles faster.
    pub auto_exposure_speed: f32,
    /// Seconds since the previous frame, for exposure adaptation (defaults to 60 Hz).
    pub frame_delta_seconds: f32,
    /// Direction towards the light (world space) + ambient strength.
    pub light_dir_ambient: Vec4,
    /// Light color (rgb) + intensity (w); HDR, so values above 1 are expected.
    pub light_color_intensity: Vec4,
    /// Build and draw per-primitive LOD chains; off by default.
    /// Read only by `upload_scene`, so set it (and `lod_switch_distances`) before uploading.
    pub lod_enabled: bool,
    /// Multiplier on the environment's radiance, read every frame; `light_dir_ambient.w` is the ambient.
    pub ibl_intensity: f32,
    /// Per-pass GPU timestamps. Inert until [`Self::enable_gpu_timing`].
    gpu_timing: GpuTiming,
    /// Record occlusion detection after the forward pass; off by default since it adds a pass.
    /// Results land in [`Self::occlusion_visibility`].
    pub occlusion_queries_enabled: bool,
    /// Use compute-shader culling ([`GpuCulling`]) instead of hardware occlusion queries.
    pub gpu_culling_enabled: bool,
    /// Camera distances at which successive LOD levels take over, ascending.
    /// Defaults suit unit-scale assets; a scene in metres wants larger values.
    pub lod_switch_distances: Vec<f32>,
    /// The Sun's shadows; read every frame, a new resolution or cascade count reallocates the maps.
    pub shadows: ShadowSettings,
    /// Skip what lies outside the camera frustum, and casters outside every cascade.
    pub frustum_culling: bool,
    /// The skybox, or with it off the C++ navy; without a cube map the procedural sky stands in.
    pub skybox_enabled: bool,
    /// Where the Model tab places the uploaded scene; an upload resets it.
    pub model_transform: ModelTransform,
    /// Forward or deferred shading, read every frame.
    pub raster_path: RasterPath,
    /// Which frames FXAA smooths after the tonemap, read every frame.
    pub fxaa: FxaaMode,
    /// The volumetric clouds, off by default as in the C++ engine; the noise is made the first frame they are on.
    pub clouds: CloudSettings,
    /// The root matrix the instances and lights carry now.
    placed_root: Mat4,
    /// A traced mode builds the traced scene on first use; until it is ready the frame rasters.
    pub render_mode: RenderMode,
    tracer: Tracer,
    /// The path-traced mode's samples, bounces and white furnace.
    pub trace_settings: TraceSettings,
    traced: TracedState,
}

impl ForwardRenderer {
    pub fn new(gpu: &GpuContext, width: u32, height: u32) -> Self {
        let device = &gpu.device;
        let shader = main_pass::create_forward_shader(device);
        let frame = FrameBinding::new(device);
        let lights = LightResources::new(device);
        let ibl = IblBinding::new(device);
        let shadow = ShadowResources::new(device, &shader, &frame.layout);
        let main = MainPipelines::new(device, shader, &ibl.layout, &frame.layout, &lights.layout);
        let targets = RenderTargets::new(device, width, height);
        let depth_resolve = DepthResolve::new(device, &targets.depth_msaa);
        let post = PostChain::new(gpu, &targets.hdr_view);
        Self {
            main,
            sky: SkyResources::new(gpu),
            shadow,
            frame,
            lights,
            ibl,
            materials: MaterialDefaults::new(gpu),
            targets,
            depth_resolve,
            post,
            cloud_pass: CloudPass::new(device),
            culling: Culling::new(device),
            deferred: None,
            primitives: Vec::new(),
            scene_bounds: None,
            uploaded_texture_count: 0,
            animation: SceneAnimation::default(),
            bloom_strength: 0.6,
            ssao_strength: 0.7,
            exposure_ev: 0.0,
            auto_exposure: false,
            auto_exposure_speed: 3.0,
            frame_delta_seconds: 1.0 / 60.0,
            light_dir_ambient: Vec4::new(0.5, 0.8, 0.3, 0.35),
            // ~5 because the BRDF divides diffuse by PI.
            light_color_intensity: Vec4::new(1.0, 0.97, 0.92, 5.0),
            lod_enabled: false,
            ibl_intensity: 1.0,
            gpu_timing: GpuTiming::unavailable(),
            occlusion_queries_enabled: false,
            gpu_culling_enabled: false,
            lod_switch_distances: vec![8.0, 24.0],
            shadows: ShadowSettings::default(),
            frustum_culling: true,
            skybox_enabled: true,
            model_transform: ModelTransform::default(),
            raster_path: RasterPath::Forward,
            fxaa: FxaaMode::default(),
            clouds: CloudSettings::default(),
            placed_root: Mat4::IDENTITY,
            render_mode: RenderMode::Raster,
            tracer: Tracer::default(),
            trace_settings: TraceSettings::default(),
            traced: TracedState::new(&gpu.device.limits()),
        }
    }

    /// Replaces one primitive's instance transforms; an empty slice restores the identity instance.
    /// The traced scene instances them too, see [`Tracer::set_instances`] for what it moves in place.
    pub fn set_instances(&mut self, gpu: &GpuContext, primitive_index: usize, transforms: &[Mat4]) {
        let Some(primitive) = self.primitives.get_mut(primitive_index) else {
            return;
        };
        // The bundles bake each draw's instance buffer, range and winding.
        if primitive.write_instances(gpu, transforms) {
            self.shadow.caster_bundles = None;
        }
        self.recompute_scene_bounds();
        self.tracer.set_instances(gpu, primitive_index, transforms);
    }

    /// Instance count of a primitive, for tests and diagnostics.
    pub fn instance_count(&self, primitive_index: usize) -> u32 {
        self.primitives
            .get(primitive_index)
            .map(|p| p.instance_count)
            .unwrap_or(0)
    }

    /// GPU textures created by the last [`Self::upload_scene`]; shared images count once.
    pub fn uploaded_texture_count(&self) -> usize {
        self.uploaded_texture_count
    }

    /// Whole-scene world bounds as cascade fitting reads them; `None` before any upload.
    pub fn scene_bounds(&self) -> Option<(Vec3, Vec3)> {
        self.scene_bounds
    }

    /// A primitive's world culling bounds, exactly as the frustum test reads them.
    pub fn primitive_world_aabb(&self, primitive_index: usize) -> Option<(Vec3, Vec3)> {
        self.primitives
            .get(primitive_index)
            .map(|p| (p.aabb_min, p.aabb_max))
    }

    /// Number of pre-uploaded LOD levels for a primitive (0 when LOD is off).
    pub fn lod_level_count(&self, primitive_index: usize) -> usize {
        self.primitives
            .get(primitive_index)
            .map(|p| p.lod_levels.len())
            .unwrap_or(0)
    }

    /// Index count of one pre-uploaded level, `None` if it does not exist.
    pub fn lod_level_index_count(&self, primitive_index: usize, level: usize) -> Option<u32> {
        self.primitives
            .get(primitive_index)?
            .lod_levels
            .get(level)
            .map(|l| l.index_count)
    }

    /// Index count the draw path issues for a primitive from `eye` (the pass's own `geometry_for`).
    pub fn selected_index_count(&self, primitive_index: usize, eye: Vec3) -> Option<u32> {
        let prim = self.primitives.get(primitive_index)?;
        Some(prim.geometry_for(eye).2)
    }

    /// Starts collecting per-pass GPU timings (off by default: it is not free).
    /// Returns `false` when the adapter lacks `TIMESTAMP_QUERY`; the frame then renders unchanged.
    pub fn enable_gpu_timing(&mut self, gpu: &GpuContext) -> bool {
        self.gpu_timing = GpuTiming::new(&gpu.device, &gpu.queue);
        self.gpu_timing.is_available()
    }

    /// Shadow caster draws submitted / considered last frame, summed over cascades.
    pub fn shadow_caster_stats(&self) -> (u32, u32) {
        (self.shadow.casters_drawn, self.shadow.casters_considered)
    }

    /// Whether the per-cascade shadow-caster bundles are cached, for the cache-contract tests.
    pub fn shadow_caster_bundle_is_cached(&self) -> bool {
        self.shadow.caster_bundles.is_some()
    }

    /// Opaque primitives drawn / considered by the camera pass last frame (equal with culling off).
    /// Fed by `gpu_culling_enabled` (takes priority) or `occlusion_queries_enabled`.
    pub fn occlusion_cull_stats(&self) -> (u32, u32) {
        (self.culling.drawn, self.culling.considered)
    }

    /// Averaged per-pass GPU durations in ms, in record order; empty until the first readback.
    /// A pass absent from the list has not reported yet, which is not zero.
    pub fn gpu_timings_ms(&self) -> Vec<(&'static str, f32)> {
        self.gpu_timing.timings_ms()
    }

    /// Per-primitive occlusion visibility from the latest readback, index-aligned to primitives.
    /// Empty until the first async readback lands; a missing entry is unmeasured, not occluded.
    pub fn occlusion_visibility(&self) -> &[bool] {
        self.culling.occlusion.visibility()
    }

    /// Like [`Self::occlusion_visibility`], but fed by the compute-shader culling path.
    pub fn gpu_culling_visibility(&self) -> &[bool] {
        self.culling
            .gpu_culling
            .as_ref()
            .map(GpuCulling::visibility)
            .unwrap_or(&[])
    }

    /// Raw fragment counts behind [`Self::occlusion_visibility`].
    pub fn occlusion_samples(&self) -> &[u64] {
        self.culling.occlusion.samples()
    }

    /// True while per-pass GPU timings are being collected.
    pub fn gpu_timing_available(&self) -> bool {
        self.gpu_timing.is_available()
    }

    /// Bakes `equirect` into IBL maps that light the scene from the next frame on.
    /// Blocks for the bake (one submit): a load-time call, not a per-frame one.
    pub fn set_environment(&mut self, gpu: &GpuContext, equirect: &EquirectImage) {
        let ibl = &mut self.ibl;
        // The BRDF table never touches the environment, so it is baked once and reused.
        let brdf_lut = ibl.brdf_lut.get_or_insert_with(|| BrdfLut::new(gpu));
        let environment = IblEnvironment::bake(gpu, equirect);
        ibl.bind_group = create_ibl_bind_group(
            &gpu.device,
            &ibl.layout,
            &ibl.uniform_buffer,
            environment.irradiance_view(),
            environment.prefiltered_view(),
            brdf_lut.view(),
            &ibl.fallback.sampler,
        );
        ibl.environment = Some(environment);
        ibl.generation += 1;
        self.write_ibl_uniforms(gpu);
    }

    /// Drops the environment and returns to the analytic hemisphere path.
    pub fn clear_environment(&mut self, gpu: &GpuContext) {
        let ibl = &mut self.ibl;
        ibl.environment = None;
        ibl.generation += 1;
        ibl.bind_group = ibl.fallback_bind_group(&gpu.device);
        self.write_ibl_uniforms(gpu);
    }

    /// True when a baked environment is lighting the scene.
    pub fn environment_enabled(&self) -> bool {
        self.ibl.environment.is_some()
    }

    /// The baked environment the frame is sampling, for tests and diagnostics.
    pub fn environment(&self) -> Option<&IblEnvironment> {
        self.ibl.environment.as_ref()
    }

    /// The shared split-sum BRDF table, once an environment has been set.
    pub fn brdf_lut(&self) -> Option<&BrdfLut> {
        self.ibl.brdf_lut.as_ref()
    }

    /// Uploads a CPU scene, replacing any previously uploaded one.
    pub fn upload_scene(&mut self, gpu: &GpuContext, scene: &CpuScene) {
        self.reset_scene_state(scene);
        // The traced scene is of the old scene until `set_trace_source` hands over this one.
        self.tracer.reset();
        // A new scene must not inherit the last one's adapted exposure.
        self.reset_exposure(gpu);
        // Skinned primitives need the joint nodes' world matrices to size their bounds.
        let upload_node_world = CpuScene::compute_world_transforms(&scene.nodes);
        let mut textures = MaterialTextureCache::default();
        for (i, prim) in scene.primitives.iter().enumerate() {
            let primitive =
                self.upload_primitive(gpu, scene, i, prim, &upload_node_world, &mut textures);
            self.primitives.push(primitive);
        }
        self.uploaded_texture_count = textures.uploaded;
        self.settle_uploaded_pose(scene, upload_node_world);
    }

    /// Renders the scene HDR->tonemap into `output_view`, which must be `width` x `height`.
    pub fn render_tonemapped(
        &mut self,
        gpu: &GpuContext,
        tonemap: &mut TonemapPass,
        output_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        camera: &OrbitCamera,
    ) {
        self.tracer.advance(gpu, self.render_mode.is_traced());
        let view = self.prepare_frame(gpu, tonemap, width, height, camera);
        self.forget_timings_of_another_frame();

        // Before any scope: begin_frame picks the ring slot every scope below writes into.
        self.gpu_timing.begin_frame();

        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("forward_encoder"),
            });
        // Pass wiring is validated once by the `graph` tests, per mode and path, not per frame.
        let traced = self.encode_traced(gpu, &mut encoder, &view);
        if !traced {
            self.encode_shadow_cascades(gpu, &mut encoder);
            if self.raster_path == RasterPath::Deferred {
                self.encode_deferred(gpu, &mut encoder, &view);
            } else {
                self.encode_forward_pass(&mut encoder, &view);
                self.encode_depth_resolve(&mut encoder);
            }
            self.encode_occlusion(gpu, &mut encoder, &view);
        }
        // Before bloom: its bright pass thresholds at this frame's exposure, as the tonemap exposes.
        self.encode_exposure(&mut encoder, view.width, view.height);
        // Before bloom too: its bright pass thresholds the clouded scene.
        self.encode_clouds(gpu, &mut encoder, (&view, traced));
        self.encode_bloom(&mut encoder);
        if !traced {
            self.encode_ssao(&mut encoder);
        }
        self.encode_tonemap(gpu, &mut encoder, tonemap, output_view, (&view, traced));
        self.finish_frame(gpu, encoder);
    }

    /// Renders one tonemapped frame headless and returns tightly packed row-major RGBA8 bytes.
    pub fn render_to_pixels(
        &mut self,
        gpu: &GpuContext,
        width: u32,
        height: u32,
        camera: &OrbitCamera,
    ) -> anyhow::Result<Vec<u8>> {
        self.render_to_pixels_with_format(
            gpu,
            width,
            height,
            camera,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        )
    }

    /// As [`Self::render_to_pixels`] with an explicit format, for the non-sRGB path browsers use.
    /// Only `Rgba8Unorm` and `Rgba8UnormSrgb` match the RGBA8 bytes it returns; others are an error.
    pub fn render_to_pixels_with_format(
        &mut self,
        gpu: &GpuContext,
        width: u32,
        height: u32,
        camera: &OrbitCamera,
        format: wgpu::TextureFormat,
    ) -> anyhow::Result<Vec<u8>> {
        // BGRA would come back channel-swapped, and wider texels overrun the 4-byte row pitch.
        anyhow::ensure!(
            matches!(
                format,
                wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb
            ),
            "render_to_pixels returns RGBA8 bytes, so it renders only to Rgba8Unorm or \
             Rgba8UnormSrgb, not {format:?}"
        );
        let mut tonemap = TonemapPass::new(gpu, format);

        let texture = create_2d_texture(
            &gpu.device,
            Some("offscreen_color"),
            width,
            height,
            format,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            1,
            1,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        // The fresh pass binds the live bloom and AO outputs; nothing is rebuilt under the window's pass.
        self.render_tonemapped(gpu, &mut tonemap, &view, width, height, camera);

        readback::read_rgba8(gpu, &texture, width, height)
    }

    /// Rebuilds the scene, shadow, sky, deferred, cloud and traced pipelines from new WGSL; invalid shaders keep the old ones.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn reload_shaders(
        &mut self,
        gpu: &GpuContext,
        sources: &ShaderSources<'_>,
    ) -> anyhow::Result<()> {
        let device = &gpu.device;
        let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("forward_shader_reloaded"),
            source: wgpu::ShaderSource::Wgsl(sources.forward.into()),
        });
        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky_shader_reloaded"),
            source: wgpu::ShaderSource::Wgsl(sources.sky.into()),
        });
        let set = main_pass::create_forward_pipeline_set(
            device,
            &shader,
            &self.main.layout,
            wgpu::FrontFace::Ccw,
        );
        let shadow = shadow::ShadowPipelines::new(
            device,
            &shader,
            &self.shadow.pipeline_layout,
            &self.shadow.masked_pipeline_layout,
        );
        let sky = sky::create_sky_pipeline(device, &sky_shader, &self.sky.pipeline_layout);
        // Even before deferred first runs: its pipelines are built lazily, outside any error scope.
        let deferred = self.build_deferred(device, &shader, &sky_shader);
        // Even with clouds off: a cloud shader that cannot build is refused now, not on the first cloud frame.
        let clouds = CloudPipelines::new(device, sources.cloud_march, sources.cloud_noise);
        // Both kernels now, as for deferred: each otherwise compiles on its mode's first frame, outside any scope.
        let traced = TracedPasses::compiled(
            device,
            TracedSources {
                traced: sources.traced,
                resolve: sources.trace_resolve,
            },
        );
        let traced_sky = sky::sky_pipeline(device, &sky_shader, &self.sky.pipeline_layout, 1);
        if let Some(err) = pollster::block_on(error_scope.pop()) {
            anyhow::bail!("shader reload rejected: {err}");
        }
        self.main.upright = set;
        // The mirrored sets are rebuilt from the new module the next frame one is drawn.
        self.main.mirrored = None;
        self.main.shader = shader;
        self.shadow.pipelines = shadow;
        // The cached bundles still hold the old shadow pipelines.
        self.shadow.caster_bundles = None;
        self.sky.pipeline = sky;
        self.sky.shader = sky_shader;
        if self.deferred.is_some() {
            self.deferred = Some(deferred);
        }
        self.cloud_pass.replace_pipelines(device, clouds);
        self.replace_traced_pipelines(traced, traced_sky);
        Ok(())
    }

    /// True when the uploaded scene carries animations.
    pub fn has_animations(&self) -> bool {
        !self.animation.animations.is_empty()
    }

    /// Samples the active clip at `time` (s) and retargets transforms, bounds and blend centres.
    /// Takes f64 too: f32 seconds step by 1/64 s after 36 hours, which stutters a long-running app.
    pub fn set_animation_time(&mut self, time: impl Into<f64>) {
        let time = time.into();
        if self.animation.animations.is_empty() || self.animation.nodes.is_empty() {
            return;
        }
        self.animation.sample_node_channels(time);
        self.sample_morph_channels(time);

        let world = CpuScene::compute_world_transforms(&self.animation.nodes);
        self.animation.pending_joint_world = Some(world.clone());
        self.retarget_primitives(&world);
        self.retarget_lights(&world);
        self.animation.world = world;
        self.recompute_scene_bounds();
    }

    /// Re-derives `scene_bounds`, the sole cascade-fitting input; every box mutator must call it.
    /// See `../../docs/renderer-bounds-invariant.md` § Maintainers — everything that must update bounds
    fn recompute_scene_bounds(&mut self) {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for prim in &self.primitives {
            min = min.min(prim.aabb_min);
            max = max.max(prim.aabb_max);
        }
        if min.x <= max.x {
            self.scene_bounds = Some((min, max));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::bounds::normal_matrix_of;

    #[test]
    fn set_animation_time_recomputes_and_dirties_the_cached_normal_matrix() {
        let Some(gpu) = GpuContext::headless_or_skip() else {
            return;
        };

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/assets/cube_animated.gltf");
        let scene = crate::load_gltf(&path).expect("cube_animated.gltf must load");
        let mut renderer = ForwardRenderer::new(&gpu, 4, 4);
        renderer.upload_scene(&gpu, &scene);
        assert!(
            renderer.primitives.iter().any(|p| p.node_index.is_some()),
            "cube_animated.gltf must have at least one node-driven primitive"
        );

        // As if uploaded this frame, so only set_animation_time can re-dirty the flag.
        for prim in &mut renderer.primitives {
            prim.uniforms_dirty = false;
        }

        renderer.set_animation_time(1.0);

        for prim in &renderer.primitives {
            if prim.node_index.is_none() {
                continue;
            }
            assert!(
                prim.uniforms_dirty,
                "an animated primitive's model changed, so its uniforms must be re-marked dirty"
            );
            let expected = normal_matrix_of(prim.model);
            assert!(
                prim.normal_matrix.is_finite(),
                "cached normal matrix must stay finite"
            );
            assert!(
                (prim.normal_matrix - expected).abs_diff_eq(Mat4::ZERO, 1e-6),
                "cached normal matrix must match a fresh computation from the current model"
            );
        }
    }
}
