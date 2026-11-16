//! The deferred raster path: a single-sample G-buffer pass, then one pass that lights it
//! full-screen with the forward lighting and draws the sky and blended geometry over it.

use super::frame::FrameView;
use super::main_pass::{
    hdr_target, ForwardPipelineSet, PipelineSource, SceneTargets, WindingSets, HDR_CLEAR,
};
use super::primitive::GpuPrimitive;
use super::{sky, ForwardRenderer, DEPTH_FORMAT};
use crate::context::GpuContext;
use crate::render::bind_layout;
use crate::render::buffer_desc;
use crate::render::gpu_timing::TimedPass;
use crate::render::graph::Resource;

/// The C++ engine's two raster paths, switched at run time like its Forward/Deferred control.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RasterPath {
    /// Shades every draw as it rasterises, at 4x MSAA.
    #[default]
    Forward,
    /// Writes a G-buffer at one sample, then shades each covered pixel once.
    Deferred,
}

impl RasterPath {
    /// Both paths, in the C++ editor's order.
    pub const ALL: [Self; 2] = [Self::Forward, Self::Deferred];

    /// The C++ editor's name for the path.
    pub fn label(self) -> &'static str {
        match self {
            Self::Forward => "Forward",
            Self::Deferred => "Deferred",
        }
    }
}

/// `fs_gbuffer`'s targets in order: normal and metallic, albedo and occlusion, roughness and emissive.
pub(super) const GBUFFER_FORMATS: [wgpu::TextureFormat; 3] = [
    wgpu::TextureFormat::Rgba16Float,
    wgpu::TextureFormat::Rgba8UnormSrgb,
    wgpu::TextureFormat::Rgba16Float,
];

/// The G-buffer targets in `GBUFFER_FORMATS` order.
const GBUFFER: [Resource; 3] = [
    Resource::GBufferNormal,
    Resource::GBufferAlbedo,
    Resource::GBufferMaterial,
];

/// Group 0 binding 18 of the lighting pass, mirroring `DeferredUniforms` in forward.slang.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct DeferredUniforms {
    inv_view_proj: [[f32; 4]; 4],
}

struct DeferredPipelines {
    /// The G-buffer pipelines for opaque draws and single-sample blend ones, counter-clockwise fronts.
    upright: ForwardPipelineSet,
    /// Clockwise fronts, built the first frame a mirrored primitive is drawn.
    mirrored: Option<ForwardPipelineSet>,
    lighting: wgpu::RenderPipeline,
    /// The sky at one sample, drawn in the lighting pass.
    sky: wgpu::RenderPipeline,
}

impl DeferredPipelines {
    fn sets(&self) -> WindingSets<'_> {
        WindingSets {
            upright: &self.upright,
            mirrored: self.mirrored.as_ref(),
        }
    }
}

/// What the deferred path adds, built on its first frame so a forward-only renderer never pays for it.
pub(super) struct DeferredResources {
    pipelines: DeferredPipelines,
    lighting_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    /// The lighting group over this frame's G-buffer; `None` while no frame defers.
    targets: Option<GBufferTargets>,
}

/// The pool's G-buffer views and the lighting pass's group 0 over them.
struct GBufferTargets {
    /// Compared by identity with the pool's, so a moved target rebuilds the group.
    views: [wgpu::TextureView; 3],
    /// The depth target `bind_group` reads, compared by identity so a resize rebuilds both.
    depth: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

/// What the lighting pass's group 0 binds besides the G-buffer itself.
struct LightingInputs<'a> {
    layout: &'a wgpu::BindGroupLayout,
    depth: &'a wgpu::TextureView,
    shadow_map: &'a wgpu::TextureView,
    shadow_sampler: &'a wgpu::Sampler,
    uniforms: &'a wgpu::Buffer,
}

impl GBufferTargets {
    fn new(
        device: &wgpu::Device,
        inputs: &LightingInputs<'_>,
        views: [wgpu::TextureView; 3],
    ) -> Self {
        let bind_group = lighting_bind_group(device, inputs, &views);
        Self {
            views,
            depth: inputs.depth.clone(),
            bind_group,
        }
    }

    /// Cleared, then stored for the lighting pass; uncovered texels stay cleared and are never read.
    fn color_attachments(&self) -> [Option<wgpu::RenderPassColorAttachment<'_>>; 3] {
        self.views.each_ref().map(|view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })
        })
    }
}

impl ForwardRenderer {
    /// The G-buffer pass: culled opaque draws, which report what the camera drew.
    pub(super) fn encode_gbuffer(&mut self, encoder: &mut wgpu::CommandEncoder, view: &FrameView) {
        let (drawn, considered) = self.encode_gbuffer_pass(encoder, view);
        self.culling.drawn = drawn;
        self.culling.considered = considered;
    }

    /// The lighting pass, in place of the forward pass's shading; its blended draws add to the G-buffer's.
    pub(super) fn encode_lighting(&mut self, encoder: &mut wgpu::CommandEncoder, view: &FrameView) {
        let blended = self.encode_deferred_lighting(encoder, view);
        self.culling.primitives = (self.culling.drawn + blended, self.primitives.len() as u32);
    }

    /// Lets the last deferred frame's G-buffer go, which the lighting group would keep alive.
    pub(super) fn drop_gbuffer_bindings(&mut self) {
        if let Some(deferred) = self.deferred.as_mut() {
            deferred.targets = None;
        }
    }

    /// The lighting group around the shadow map `sync_shadow_map` reallocated; nothing before the first deferred frame.
    pub(super) fn rebind_deferred_shadow_map(&mut self, device: &wgpu::Device) {
        let Some(deferred) = self.deferred.as_mut() else {
            return;
        };
        let Some(gbuffer) = deferred.targets.as_mut() else {
            return;
        };
        let inputs = LightingInputs {
            layout: &deferred.lighting_layout,
            depth: &gbuffer.depth,
            shadow_map: &self.shadow.view,
            shadow_sampler: &self.shadow.sampler,
            uniforms: &deferred.uniform_buffer,
        };
        gbuffer.bind_group = lighting_bind_group(device, &inputs, &gbuffer.views);
    }

    /// The deferred resources over `shader` and `sky_shader`: the renderer's own, or a reload's.
    pub(super) fn build_deferred(
        &self,
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        sky_shader: &wgpu::ShaderModule,
    ) -> DeferredResources {
        let lighting_layout = create_lighting_bind_group_layout(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("deferred_lighting_pipeline_layout"),
            bind_group_layouts: &[
                Some(&lighting_layout),
                Some(&self.ibl.layout),
                Some(&self.frame.layout),
                Some(&self.lights.layout),
            ],
            immediate_size: 0,
        });
        let source = PipelineSource {
            shader,
            layout: &self.main.layout,
        };
        let pipelines = DeferredPipelines {
            upright: create_deferred_pipeline_set(device, &source, wgpu::FrontFace::Ccw),
            mirrored: None,
            lighting: create_lighting_pipeline(device, shader, &pipeline_layout),
            sky: sky::sky_pipeline(device, sky_shader, &self.sky.pipeline_layout, 1),
        };
        let uniform_buffer = buffer_desc::uniform(
            device,
            "deferred_uniforms",
            std::mem::size_of::<DeferredUniforms>() as wgpu::BufferAddress,
        );
        DeferredResources {
            pipelines,
            lighting_layout,
            uniform_buffer,
            targets: None,
        }
    }

    /// Builds what this frame needs, follows a resize and a mirrored primitive, and writes the view inverse.
    pub(super) fn prepare_deferred(&mut self, gpu: &GpuContext, view: &FrameView) {
        let device = &gpu.device;
        if self.deferred.is_none() {
            self.deferred = Some(self.build_deferred(device, &self.main.shader, &self.sky.shader));
        }
        let mirrored = self.primitives.iter().any(GpuPrimitive::needs_clockwise);
        let deferred = self.deferred.as_mut().expect("built above");
        if mirrored && deferred.pipelines.mirrored.is_none() {
            let source = PipelineSource {
                shader: &self.main.shader,
                layout: &self.main.layout,
            };
            let set = create_deferred_pipeline_set(device, &source, wgpu::FrontFace::Cw);
            deferred.pipelines.mirrored = Some(set);
        }
        let depth = self.targets.view(Resource::Depth);
        let views = GBUFFER.map(|resource| self.targets.view(resource).clone());
        let stale = |t: &GBufferTargets| t.depth != *depth || t.views != views;
        if deferred.targets.as_ref().is_none_or(stale) {
            let inputs = LightingInputs {
                layout: &deferred.lighting_layout,
                depth,
                shadow_map: &self.shadow.view,
                shadow_sampler: &self.shadow.sampler,
                uniforms: &deferred.uniform_buffer,
            };
            deferred.targets = Some(GBufferTargets::new(device, &inputs, views));
        }
        let uniforms = DeferredUniforms {
            inv_view_proj: view.view_proj.inverse().to_cols_array_2d(),
        };
        gpu.queue
            .write_buffer(&deferred.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    /// The prepared resources and this frame's G-buffer.
    fn deferred_frame(&self) -> (&DeferredResources, &GBufferTargets) {
        let deferred = self.deferred.as_ref().expect("prepare_deferred runs first");
        let targets = deferred
            .targets
            .as_ref()
            .expect("prepare_deferred runs first");
        (deferred, targets)
    }

    /// Culled opaque draws into the G-buffer and the single-sample depth; returns (drawn, considered).
    fn encode_gbuffer_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &FrameView,
    ) -> (u32, u32) {
        let (deferred, gbuffer) = self.deferred_frame();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gbuffer_pass"),
            color_attachments: &gbuffer.color_attachments(),
            // The single-sample target SSAO and culling read, so the forward path's resolve has nothing to do.
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: self.targets.view(Resource::Depth),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: self
                .gpu_timing
                .scope(TimedPass::GBuffer)
                .render_writes(0, 1),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.bind_frame_groups(&mut pass);
        self.draw_opaque(&mut pass, view, deferred.pipelines.sets())
    }

    /// Lights the G-buffer, then draws the sky and blended geometry against its depth; returns the blended draws.
    fn encode_deferred_lighting(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &FrameView,
    ) -> u32 {
        let (deferred, gbuffer) = self.deferred_frame();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("deferred_lighting_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.targets.view(Resource::HdrColor),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(HDR_CLEAR),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            // Read-only, so the lighting draw can sample the depth the sky and blended draws test.
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: self.targets.view(Resource::Depth),
                depth_ops: None,
                stencil_ops: None,
            }),
            timestamp_writes: self
                .gpu_timing
                .scope(TimedPass::DeferredLighting)
                .render_writes(0, 1),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&deferred.pipelines.lighting);
        pass.set_bind_group(0, &gbuffer.bind_group, &[]);
        self.bind_frame_groups(&mut pass);
        pass.draw(0..3, 0..1);
        self.sky.draw_with(&mut pass, &deferred.pipelines.sky);
        self.draw_blended(&mut pass, view, deferred.pipelines.sets())
    }
}

/// The lighting pass's group 0: the shadow map, the G-buffer `views`, its depth and the uniforms.
fn lighting_bind_group(
    device: &wgpu::Device,
    inputs: &LightingInputs<'_>,
    views: &[wgpu::TextureView; 3],
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("deferred_lighting_bind_group"),
        layout: inputs.layout,
        entries: &[
            texture_entry(1, inputs.shadow_map),
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(inputs.shadow_sampler),
            },
            texture_entry(14, &views[0]),
            texture_entry(15, &views[1]),
            texture_entry(16, &views[2]),
            texture_entry(17, inputs.depth),
            wgpu::BindGroupEntry {
                binding: 18,
                resource: inputs.uniforms.as_entire_binding(),
            },
        ],
    })
}

fn texture_entry(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}

/// G-buffer pipelines for opaque materials and single-sample blend ones for the lighting pass.
fn create_deferred_pipeline_set(
    device: &wgpu::Device,
    source: &PipelineSource<'_>,
    front_face: wgpu::FrontFace,
) -> ForwardPipelineSet {
    let gbuffer = GBUFFER_FORMATS.map(|format| {
        Some(wgpu::ColorTargetState {
            format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })
    });
    let blend = [Some(hdr_target(true))];
    ForwardPipelineSet::build(
        device,
        source,
        front_face,
        [
            &SceneTargets::new("fs_gbuffer", &gbuffer, true, 1),
            &SceneTargets::new("fs_main", &blend, false, 1),
        ],
        "deferred_pipeline",
    )
}

/// Bindings 1-2 are the shadow map as the forward group 0 has them, 14-17 the G-buffer and its depth.
fn create_lighting_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let fragment = wgpu::ShaderStages::FRAGMENT;
    let depth = |binding: u32, dimension: wgpu::TextureViewDimension| {
        let sample_type = wgpu::TextureSampleType::Depth;
        bind_layout::texture(binding, fragment, sample_type, dimension, false)
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("deferred_lighting_bind_group_layout"),
        entries: &[
            depth(1, wgpu::TextureViewDimension::D2Array),
            bind_layout::sampler(2, fragment, wgpu::SamplerBindingType::Comparison),
            bind_layout::texture_2d(14, fragment, false),
            bind_layout::texture_2d(15, fragment, false),
            bind_layout::texture_2d(16, fragment, false),
            depth(17, wgpu::TextureViewDimension::D2),
            bind_layout::uniform(18, fragment),
        ],
    })
}

fn create_lighting_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("deferred_lighting_pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_deferred_lighting"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_deferred_lighting"),
            targets: &[Some(hdr_target(false))],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        // The pass attaches the depth read-only for the sky and blend draws, so this pipeline declares it unwritten.
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The colour bytes a sample writes, counted as wgpu-core counts them against the limit.
    fn bytes_per_sample(formats: &[wgpu::TextureFormat]) -> u32 {
        formats.iter().fold(0, |total, format| {
            let alignment = format
                .target_component_alignment()
                .expect("a colour format");
            let cost = format.target_pixel_byte_cost().expect("a colour format");
            total.next_multiple_of(alignment) + cost
        })
    }

    #[test]
    fn the_gbuffer_fits_webgpus_colour_bytes_per_sample() {
        let limit = wgpu::Limits::downlevel_webgl2_defaults().max_color_attachment_bytes_per_sample;
        assert_eq!(bytes_per_sample(&GBUFFER_FORMATS), 24);
        assert!(bytes_per_sample(&GBUFFER_FORMATS) <= limit, "limit {limit}");
        // Storing positions instead of rebuilding them from depth would not fit.
        let positions = [
            GBUFFER_FORMATS.as_slice(),
            &[wgpu::TextureFormat::Rgba32Float],
        ]
        .concat();
        assert!(bytes_per_sample(&positions) > limit);
    }

    /// `fs_deferred_lighting`'s rebuild on the CPU: a pixel's NDC and stored depth through the inverse.
    fn rebuilt(inv_view_proj: glam::Mat4, ndc: glam::Vec3) -> glam::Vec3 {
        inv_view_proj.project_point3(ndc)
    }

    /// The depth the G-buffer stores, not an interpolated position, is all deferred has to light with.
    #[test]
    fn the_depth_rebuild_keeps_the_arena_within_a_centimetre() {
        use crate::app::framing::NEAR_FAR_RATIO;
        use glam::Vec3;
        // An 8 km scene's clip planes, as the framing fits them.
        let far = 16_000.0;
        let camera = crate::scene::camera::OrbitCamera {
            radius: 150.0,
            near: far / NEAR_FAR_RATIO,
            far,
            ..Default::default()
        };
        let view_proj = camera.view_projection(16.0 / 9.0);
        let (eye, inverse) = (camera.eye(), view_proj.inverse());
        let ahead = (camera.target - eye).normalize();
        let side = ahead.cross(Vec3::Y).normalize();
        // Measured: 0.3 mm at 10 m, 6 mm at the arena's 150 m, 40 cm at 1 km.
        for (distance, tolerance) in [(10.0, 1e-3), (150.0, 1e-2), (1000.0, 1.0)] {
            let world = eye + ahead * distance + side * (0.2 * distance);
            let error = rebuilt(inverse, view_proj.project_point3(world)).distance(world);
            eprintln!("{distance} m: rebuilt {error:.5} m off");
            assert!(error < tolerance, "{distance} m: rebuilt {error} m off");
        }
    }

    #[test]
    fn the_paths_carry_the_cpp_editor_labels() {
        let labels: Vec<&str> = RasterPath::ALL.iter().map(|p| p.label()).collect();
        assert_eq!(labels, ["Forward", "Deferred"]);
        assert_eq!(RasterPath::default(), RasterPath::Forward);
    }
}
