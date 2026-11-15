//! Forward PBR pass into an HDR target, ACES-tonemapped to the output, plus headless readback.
//! The frame's passes and the resources each one owns live in the submodules, one per pass.

mod animate;
mod culling;
mod environment;
mod frame;
mod light_grid;
mod main_pass;
mod post;
mod primitive;
mod readback;
mod shadow;
mod sky;
mod targets;
mod upload;

use glam::{Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt as _;

use crate::context::GpuContext;
use crate::render::bounds::instanced_bounds;
use crate::render::gpu_occlusion::GpuCulling;
use crate::render::gpu_timing::GpuTiming;
use crate::render::ibl::{BrdfLut, EquirectImage, IblEnvironment};
pub use crate::render::lights::MAX_PUNCTUAL_LIGHTS;
use crate::render::texture::create_2d_texture;
use crate::render::tonemap::TonemapPass;
use crate::scene::camera::OrbitCamera;
use crate::scene::{CpuScene, InstanceRaw};

use animate::SceneAnimation;
use culling::Culling;
use environment::{create_ibl_bind_group, IblBinding, IblUniforms};
use frame::FrameBinding;
use light_grid::LightResources;
use main_pass::MainPipelines;
use post::PostChain;
use primitive::GpuPrimitive;
use shadow::ShadowResources;
use sky::SkyResources;
use targets::{DepthResolve, RenderTargets};
use upload::{MaterialDefaults, MaterialTextureCache};

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// MSAA sample count of the forward HDR and depth targets (1 disables MSAA).
const MSAA_SAMPLE_COUNT: u32 = 4;
pub const SHADOW_MAP_SIZE: u32 = 2048;
/// Cascaded shadow map layers (split by view distance).
pub const CASCADE_COUNT: usize = 3;

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
    culling: Culling,
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
    /// Multiplier on the environment's radiance; the ambient slider is `light_dir_ambient.w`.
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
}

impl ForwardRenderer {
    pub fn new(gpu: &GpuContext, width: u32, height: u32) -> Self {
        let device = &gpu.device;
        let shader = main_pass::create_forward_shader(device);
        let frame = FrameBinding::new(device);
        let lights = LightResources::new(device);
        let ibl = IblBinding::new(device);
        let main = MainPipelines::new(device, &shader, &ibl.layout, &frame.layout, &lights.layout);
        let shadow = ShadowResources::new(device, &shader, &frame.layout);
        let targets = RenderTargets::new(device, width, height);
        let depth_resolve = DepthResolve::new(device, &targets.depth_msaa);
        let post = PostChain::new(gpu, &targets.hdr_view);
        Self {
            main,
            sky: SkyResources::new(device),
            shadow,
            frame,
            lights,
            ibl,
            materials: MaterialDefaults::new(gpu),
            targets,
            depth_resolve,
            post,
            culling: Culling::new(device),
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
        }
    }

    /// Replaces one primitive's instance transforms; an empty slice restores the identity instance.
    pub fn set_instances(&mut self, gpu: &GpuContext, primitive_index: usize, transforms: &[Mat4]) {
        let Some(primitive) = self.primitives.get_mut(primitive_index) else {
            return;
        };

        if transforms.is_empty() {
            gpu.queue.write_buffer(
                &primitive.instance_buffer,
                0,
                bytemuck::bytes_of(&InstanceRaw::IDENTITY),
            );
            primitive.instance_count = 1;
            // Single identity instance again: bounds collapse to the posed box.
            primitive.instance_transforms.clear();
            let (min, max) = primitive.pre_instance_aabb;
            primitive.aabb_min = min;
            primitive.aabb_max = max;
            primitive.world_center = (min + max) * 0.5;
            self.recompute_scene_bounds();
            return;
        }

        let raw: Vec<InstanceRaw> = transforms
            .iter()
            .map(|m| InstanceRaw {
                model: m.to_cols_array_2d(),
            })
            .collect();
        let bytes = bytemuck::cast_slice(&raw);

        if (bytes.len() as u64) <= primitive.instance_buffer.size() {
            gpu.queue.write_buffer(&primitive.instance_buffer, 0, bytes);
        } else {
            primitive.instance_buffer =
                gpu.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("instances"),
                        contents: bytes,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    });
            // The bundle captured the old buffer by reference, and this replaced it.
            self.shadow.caster_bundle = None;
        }
        primitive.instance_count = raw.len() as u32;
        // Culling bounds must span every instance, not just the base position.
        primitive.instance_transforms = transforms.to_vec();
        let (min, max) = instanced_bounds(primitive.pre_instance_aabb, transforms);
        primitive.aabb_min = min;
        primitive.aabb_max = max;
        primitive.world_center = (min + max) * 0.5;
        self.recompute_scene_bounds();
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

    /// Whether the shadow-caster bundle is cached, so tests can pin the cache/invalidate contract.
    pub fn shadow_caster_bundle_is_cached(&self) -> bool {
        self.shadow.caster_bundle.is_some()
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

        gpu.queue.write_buffer(
            &ibl.uniform_buffer,
            0,
            bytemuck::bytes_of(&IblUniforms::enabled(
                environment.max_prefiltered_mip(),
                self.ibl_intensity,
            )),
        );
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
    }

    /// Drops the environment and returns to the analytic hemisphere path.
    pub fn clear_environment(&mut self, gpu: &GpuContext) {
        let ibl = &mut self.ibl;
        ibl.environment = None;
        gpu.queue.write_buffer(
            &ibl.uniform_buffer,
            0,
            bytemuck::bytes_of(&IblUniforms::disabled()),
        );
        ibl.bind_group = ibl.fallback_bind_group(&gpu.device);
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
        let view = self.prepare_frame(gpu, tonemap, width, height, camera);

        // Before any scope: begin_frame picks the ring slot every scope below writes into.
        self.gpu_timing.begin_frame();

        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("forward_encoder"),
            });
        // Pass wiring is validated once by `graph::tests::forward_graph_is_valid`, not per frame.
        self.encode_shadow_cascades(gpu, &mut encoder);
        self.encode_forward_pass(&mut encoder, &view);
        self.encode_depth_resolve(&mut encoder);
        self.encode_occlusion(gpu, &mut encoder, &view);
        self.encode_bloom(&mut encoder);
        self.encode_ssao(&mut encoder);
        self.encode_exposure(&mut encoder, view.width, view.height);
        self.encode_tonemap(&mut encoder, tonemap, output_view);
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
    pub fn render_to_pixels_with_format(
        &mut self,
        gpu: &GpuContext,
        width: u32,
        height: u32,
        camera: &OrbitCamera,
        format: wgpu::TextureFormat,
    ) -> anyhow::Result<Vec<u8>> {
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

        // Force a rebind: this pass instance has never seen the HDR view.
        self.post.rebound_needed = true;
        self.render_tonemapped(gpu, &mut tonemap, &view, width, height, camera);

        readback::read_rgba8(gpu, &texture, width, height)
    }

    /// Rebuilds the scene/shadow/sky pipelines from new WGSL; invalid shaders keep the old ones.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn reload_shaders(
        &mut self,
        gpu: &GpuContext,
        forward_wgsl: &str,
        sky_wgsl: &str,
    ) -> anyhow::Result<()> {
        let device = &gpu.device;
        let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("forward_shader_reloaded"),
            source: wgpu::ShaderSource::Wgsl(forward_wgsl.into()),
        });
        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky_shader_reloaded"),
            source: wgpu::ShaderSource::Wgsl(sky_wgsl.into()),
        });
        let set = main_pass::create_forward_pipeline_set(device, &shader, &self.main.layout);
        let shadow = shadow::create_shadow_pipeline(device, &shader, &self.shadow.pipeline_layout);
        let shadow_masked = shadow::create_masked_shadow_pipeline(
            device,
            &shader,
            &self.shadow.masked_pipeline_layout,
        );
        let sky = sky::create_sky_pipeline(device, &sky_shader, &self.sky.pipeline_layout);
        if let Some(err) = pollster::block_on(error_scope.pop()) {
            anyhow::bail!("shader reload rejected: {err}");
        }
        (
            self.main.opaque,
            self.main.double_sided,
            self.main.blend,
            self.main.blend_double_sided,
        ) = set;
        self.shadow.pipeline = shadow;
        self.shadow.masked_pipeline = shadow_masked;
        self.sky.pipeline = sky;
        Ok(())
    }

    /// True when the uploaded scene carries animations.
    pub fn has_animations(&self) -> bool {
        !self.animation.animations.is_empty()
    }

    /// Samples every animation at `time` (s) and retargets transforms, bounds and blend centres.
    pub fn set_animation_time(&mut self, time: f32) {
        if self.animation.animations.is_empty() || self.animation.nodes.is_empty() {
            return;
        }
        self.animation.sample_node_channels(time);
        self.sample_morph_channels(time);

        let world = CpuScene::compute_world_transforms(&self.animation.nodes);
        self.animation.pending_joint_world = Some(world.clone());
        self.retarget_primitives(&world);
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
