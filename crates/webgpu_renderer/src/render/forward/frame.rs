//! The frame's CPU prologue and epilogue: per-frame uniforms, the camera view, submit and readbacks.

use glam::{Mat4, Vec3};

use super::frame_graph::{FramePass, FramePlan};
use super::ForwardRenderer;
use crate::context::GpuContext;
use crate::render::bind_layout;
use crate::render::bounds::Frustum;
use crate::render::buffer_desc;
use crate::render::tile_grid::TILE_SIZE;
use crate::render::tonemap::TonemapPass;
use crate::scene::camera::OrbitCamera;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct FrameUniforms {
    view_proj: [[f32; 4]; 4],
    light_space: [[f32; 4]; 4],
    light_space_1: [[f32; 4]; 4],
    light_space_2: [[f32; 4]; 4],
    light_dir_ambient: [f32; 4],
    light_color_intensity: [f32; 4],
    camera_position: [f32; 4],
    cascade_splits: [f32; 4],
    /// x: cascades (0 = off), y: PCF radius, z: strength.
    shadow_params: [f32; 4],
    shadow_view_dir: [f32; 4],
    shadow_bias_scale: [f32; 4],
}

/// Group 2 of the forward and shadow layouts: the per-frame uniforms.
pub(super) struct FrameBinding {
    /// Read only to build the forward and shadow pipeline layouts.
    pub(super) layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    pub(super) bind_group: wgpu::BindGroup,
}

impl FrameBinding {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame_bind_group_layout"),
            entries: &[bind_layout::uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let uniform_buffer = buffer_desc::uniform(
            device,
            "frame_uniforms",
            std::mem::size_of::<FrameUniforms>() as wgpu::BufferAddress,
        );
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame_bind_group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        Self {
            layout,
            uniform_buffer,
            bind_group,
        }
    }
}

/// What every pass of one frame reads from the camera and the scene targets' size.
pub(super) struct FrameView {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) view_proj: Mat4,
    /// The two factors of `view_proj`, which the light binning bounds spheres in.
    pub(super) view: Mat4,
    pub(super) projection: Mat4,
    pub(super) frustum: Frustum,
    pub(super) eye: Vec3,
}

impl ForwardRenderer {
    /// Everything the passes read, written before the frame's encoder exists, and the frame's graph.
    pub(super) fn prepare_frame(
        &mut self,
        gpu: &GpuContext,
        tonemap: &mut TonemapPass,
        (scene, output): ((u32, u32), (u32, u32)),
        camera: &OrbitCamera,
    ) -> (FrameView, FramePlan) {
        let (width, height) = (scene.0.max(1), scene.1.max(1));
        let output = (output.0.max(1), output.1.max(1));
        self.sync_shadow_map(&gpu.device);
        self.apply_model_transform(gpu);
        // Before the plan: whether the noise still has to be filled decides its row.
        self.ensure_cloud_march(gpu);
        let plan = self.plan_frame(gpu, tonemap.output_format(), ((width, height), output));
        self.prepare_traced();
        self.prepare_clouds(gpu);
        self.rebuild_post_inputs(gpu);
        self.bind_tonemap_inputs(gpu, tonemap);
        self.prepare_fxaa(gpu, tonemap);
        self.write_post_settings(gpu, tonemap);
        self.write_ibl_uniforms(gpu);

        self.update_joint_matrices(gpu);
        self.apply_morph_targets(gpu);
        self.ensure_mirrored_pipelines(&gpu.device);
        self.reset_culling_on_mode_change();

        let view = self.prepare_view(gpu, camera, width, height);
        // Before any pass records: a finer level swaps views in the primitives' bind groups.
        self.stream_textures(gpu, &view);
        if plan.runs(FramePass::DrawCull) {
            self.prepare_draw_cull(gpu, &view);
        }
        if plan.runs(FramePass::GBuffer) {
            self.prepare_deferred(gpu, &view);
        }
        self.write_sky_uniforms(gpu, &view);
        self.write_frame_uniforms(gpu, &view);
        self.write_light_grid(gpu, &view);
        self.write_dirty_prim_uniforms(gpu);
        (view, plan)
    }

    /// The camera's view for this frame, plus the SSAO projection and the cascade fit it drives.
    fn prepare_view(
        &mut self,
        gpu: &GpuContext,
        camera: &OrbitCamera,
        width: u32,
        height: u32,
    ) -> FrameView {
        let aspect = width as f32 / height as f32;
        let (view, projection) = (camera.view(), camera.projection(aspect));
        let view_proj = projection * view;
        let frustum = Frustum::from_view_proj(&view_proj);
        self.post
            .ssao
            .write_uniforms(&gpu.queue, projection, 0.6, 0.02, 1.0);
        self.update_cascades(camera, aspect);
        FrameView {
            width,
            height,
            view_proj,
            view,
            projection,
            frustum,
            eye: camera.eye(),
        }
    }

    /// Per-frame data: write ONCE, shared by every primitive via @group(2).
    fn write_frame_uniforms(&mut self, gpu: &GpuContext, view: &FrameView) {
        // Before the frame uniforms, so this frame's tile counts land in cascade_splits.zw.
        let tx = view.width.div_ceil(TILE_SIZE);
        let ty = view.height.div_ceil(TILE_SIZE);
        self.lights.tile_counts = (tx, ty);

        let cascades = &self.shadow.cascade_matrices;
        let eye = view.eye;
        let shadows = self.shadows.sanitized();
        let [bias_0, bias_1, bias_2] = self.shadow.bias_scales;
        let frame_uniforms = FrameUniforms {
            view_proj: view.view_proj.to_cols_array_2d(),
            light_space: cascades[0].to_cols_array_2d(),
            light_space_1: cascades[1].to_cols_array_2d(),
            light_space_2: cascades[2].to_cols_array_2d(),
            light_dir_ambient: self.light_dir_ambient.to_array(),
            light_color_intensity: self.light_color_intensity.to_array(),
            camera_position: [eye.x, eye.y, eye.z, self.lights.punctual_count as f32],
            cascade_splits: [
                self.shadow.cascade_splits[0],
                self.shadow.cascade_splits[1],
                self.lights.tile_counts.0 as f32,
                self.lights.tile_counts.1 as f32,
            ],
            shadow_params: [
                self.shadows.active_cascades() as f32,
                shadows.pcf_radius as f32,
                shadows.strength,
                0.0,
            ],
            shadow_view_dir: self.shadow.view_dir.extend(0.0).to_array(),
            shadow_bias_scale: [bias_0, bias_1, bias_2, 0.0],
        };
        gpu.queue.write_buffer(
            &self.frame.uniform_buffer,
            0,
            bytemuck::bytes_of(&frame_uniforms),
        );
    }

    /// Re-uploads the uniforms of every primitive whose model or material moved.
    fn write_dirty_prim_uniforms(&mut self, gpu: &GpuContext) {
        for prim in &mut self.primitives {
            if !prim.uniforms_dirty {
                continue;
            }
            gpu.queue.write_buffer(
                &prim.uniform_buffer,
                0,
                bytemuck::bytes_of(&prim.uniforms()),
            );
            prim.uniforms_dirty = false;
        }
    }

    /// Resolves the timestamps, submits the frame and starts its non-blocking readbacks.
    pub(super) fn finish_frame(&mut self, gpu: &GpuContext, mut encoder: wgpu::CommandEncoder) {
        // In the frame's encoder: a second submit would serialise against the frame it measures.
        self.gpu_timing.resolve(&mut encoder);
        gpu.queue.submit(Some(encoder.finish()));
        self.gpu_timing.end_frame(&gpu.device);
        self.culling.draw_cull.end_frame(&gpu.device);
        self.apply_draw_stats();
        // Non-blocking, and only diagnostics: the next frame's draw cull reads the measurements on the GPU.
        self.culling.occlusion.end_frame(&gpu.device);
        // The culling paths are exclusive, so only the enabled one has readbacks in flight.
        if self.gpu_culling_enabled {
            if let Some(gpu_cull) = self.culling.gpu_culling.as_mut() {
                gpu_cull.end_frame(&gpu.device);
            }
        }
    }
}
