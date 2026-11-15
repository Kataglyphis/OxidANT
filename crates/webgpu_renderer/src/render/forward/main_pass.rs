//! The forward main pass: culled opaque draws, the sky, then sorted blended draws, all MSAA.

use glam::Vec3;

use super::frame::FrameView;
use super::primitive::GpuPrimitive;
use super::{ForwardRenderer, DEPTH_FORMAT, HDR_FORMAT, MSAA_SAMPLE_COUNT};
use crate::render::bind_layout;
use crate::render::bounds::aabb_contains_point;
use crate::render::gpu_timing::TimedPass;
use crate::scene::{InstanceRaw, Vertex};

/// One winding's four forward pipelines: back-culled or double-sided, opaque or blended.
pub(super) struct ForwardPipelineSet {
    opaque: wgpu::RenderPipeline,
    double_sided: wgpu::RenderPipeline,
    blend: wgpu::RenderPipeline,
    blend_double_sided: wgpu::RenderPipeline,
}

impl ForwardPipelineSet {
    /// The pipeline for a material's sidedness and blending.
    fn pick(&self, double_sided: bool, blend: bool) -> &wgpu::RenderPipeline {
        match (double_sided, blend) {
            (false, false) => &self.opaque,
            (true, false) => &self.double_sided,
            (false, true) => &self.blend,
            (true, true) => &self.blend_double_sided,
        }
    }
}

/// The forward pipelines for both windings, and their layouts.
pub(super) struct MainPipelines {
    /// Counter-clockwise front faces, glTF's default.
    pub(super) upright: ForwardPipelineSet,
    /// Clockwise front faces, for mirrored transforms; built the first frame one is drawn.
    pub(super) mirrored: Option<ForwardPipelineSet>,
    /// The module every pipeline here was built from, kept for the lazily built mirrored set.
    pub(super) shader: wgpu::ShaderModule,
    pub(super) layout: wgpu::PipelineLayout,
    /// Group 0: per-primitive uniforms, the shadow map, five material slots and the joints.
    pub(super) material_layout: wgpu::BindGroupLayout,
}

impl MainPipelines {
    pub(super) fn new(
        device: &wgpu::Device,
        shader: wgpu::ShaderModule,
        ibl_layout: &wgpu::BindGroupLayout,
        frame_layout: &wgpu::BindGroupLayout,
        light_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let material_layout = create_material_bind_group_layout(device);
        // The shadow entry points share this module, so their layouts need group 2 as well.
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("forward_pipeline_layout"),
            bind_group_layouts: &[
                Some(&material_layout),
                Some(ibl_layout),
                Some(frame_layout),
                Some(light_layout),
            ],
            immediate_size: 0,
        });
        let upright = create_forward_pipeline_set(device, &shader, &layout, wgpu::FrontFace::Ccw);
        Self {
            upright,
            mirrored: None,
            shader,
            layout,
            material_layout,
        }
    }

    /// The set whose front faces wind clockwise when `clockwise`, else counter-clockwise.
    fn for_winding(&self, clockwise: bool) -> &ForwardPipelineSet {
        if !clockwise {
            return &self.upright;
        }
        debug_assert!(
            self.mirrored.is_some(),
            "ensure_mirrored_pipelines must run first"
        );
        self.mirrored.as_ref().unwrap_or(&self.upright)
    }
}

pub(super) fn create_forward_shader(device: &wgpu::Device) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("forward_shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/forward.wgsl").into()),
    })
}

impl ForwardRenderer {
    /// Builds the clockwise-front pipelines the first frame a mirrored transform is drawn.
    pub(super) fn ensure_mirrored_pipelines(&mut self, device: &wgpu::Device) {
        if !self.primitives.iter().any(GpuPrimitive::needs_clockwise) {
            return;
        }
        if self.main.mirrored.is_none() {
            self.main.mirrored = Some(create_forward_pipeline_set(
                device,
                &self.main.shader,
                &self.main.layout,
                wgpu::FrontFace::Cw,
            ));
        }
        self.shadow.pipelines.ensure_mirrored(
            device,
            &self.main.shader,
            &self.shadow.pipeline_layout,
        );
    }

    /// Opaque geometry, the sky and blended geometry in one render pass, resolved into `hdr_view`.
    pub(super) fn encode_forward_pass(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &FrameView,
    ) {
        let mut pass = self.begin_forward_pass(encoder);
        // Per-frame groups 1-3; rebound after the sky, whose own layout invalidates them.
        self.bind_frame_groups(&mut pass);
        let (drawn, considered) = self.draw_opaque(&mut pass, view);
        self.sky.draw(&mut pass);
        let blended = self.draw_blended(&mut pass, view);
        self.culling.drawn = drawn;
        self.culling.considered = considered;
        self.culling.primitives = (drawn + blended, self.primitives.len() as u32);
    }

    /// Outside the camera frustum, unless frustum culling is off.
    fn frustum_culls(&self, prim: &GpuPrimitive, view: &FrameView) -> bool {
        self.frustum_culling && !view.frustum.intersects_aabb(prim.aabb_min, prim.aabb_max)
    }

    fn begin_forward_pass<'e>(
        &self,
        encoder: &'e mut wgpu::CommandEncoder,
    ) -> wgpu::RenderPass<'e> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("forward_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.targets.hdr_msaa,
                resolve_target: Some(&self.targets.hdr_view),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.05,
                        g: 0.05,
                        b: 0.08,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Discard,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.targets.depth_msaa,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    // Not Discard: the depth resolve is a later, separate pass that reads this.
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: self
                .gpu_timing
                .scope(TimedPass::Forward)
                .render_writes(0, 1),
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }

    fn bind_frame_groups(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(1, &self.ibl.bind_group, &[]);
        pass.set_bind_group(2, &self.frame.bind_group, &[]);
        pass.set_bind_group(3, &self.lights.bind_group, &[]);
    }

    /// Frustum-tested, then occlusion-skipped opaque draws; returns (drawn, considered).
    fn draw_opaque(&self, pass: &mut wgpu::RenderPass<'_>, view: &FrameView) -> (u32, u32) {
        let mut occ_drawn = 0u32;
        let mut occ_considered = 0u32;
        for (i, prim) in self.primitives.iter().enumerate() {
            // Frustum first (cheap, no dependency on last frame).
            if prim.alpha_blend || self.frustum_culls(prim, view) {
                continue;
            }
            occ_considered += 1;
            let occluded = self.occluded_last_frame(i);
            // Eye inside the box: only back faces rasterise and fail depth, so never skip.
            if occluded && !aabb_contains_point(prim.aabb_min, prim.aabb_max, view.eye) {
                continue;
            }
            occ_drawn += 1;
            draw_primitive(pass, &self.main, prim, view.eye);
        }
        (occ_drawn, occ_considered)
    }

    /// Transparents last, farthest first, no depth writes; returns how many it drew.
    fn draw_blended(&self, pass: &mut wgpu::RenderPass<'_>, view: &FrameView) -> u32 {
        let eye = view.eye;
        let mut blended: Vec<&GpuPrimitive> = self
            .primitives
            .iter()
            .filter(|p| p.alpha_blend && !self.frustum_culls(p, view))
            .collect();
        blended.sort_by(|a, b| {
            let da = a.world_center.distance_squared(eye);
            let db = b.world_center.distance_squared(eye);
            db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
        });
        if !blended.is_empty() {
            self.bind_frame_groups(pass);
        }
        for prim in &blended {
            draw_primitive(pass, &self.main, prim, eye);
        }
        blended.len() as u32
    }
}

/// Binds `prim`'s group 0 and its LOD-selected geometry, then draws each winding's instances.
fn draw_primitive(
    pass: &mut wgpu::RenderPass<'_>,
    pipelines: &MainPipelines,
    prim: &GpuPrimitive,
    eye: Vec3,
) {
    let (vertex_buffer, index_buffer, index_count) = prim.geometry_for(eye);
    pass.set_bind_group(0, &prim.bind_group, &[]);
    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
    pass.set_vertex_buffer(1, prim.instance_buffer.slice(..));
    pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
    for (instances, clockwise) in prim.instance_runs() {
        let set = pipelines.for_winding(clockwise);
        pass.set_pipeline(set.pick(prim.double_sided, prim.alpha_blend));
        pass.draw_indexed(0..index_count, 0, instances);
    }
}

/// Bindings 0-2 sample the shadow map, 3..=12 are the material slot pairs and 13 the joints.
fn create_material_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let mut entries: Vec<wgpu::BindGroupLayoutEntry> = vec![
        bind_layout::uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
        bind_layout::texture(
            1,
            wgpu::ShaderStages::FRAGMENT,
            wgpu::TextureSampleType::Depth,
            wgpu::TextureViewDimension::D2Array,
            false,
        ),
        bind_layout::sampler(
            2,
            wgpu::ShaderStages::FRAGMENT,
            wgpu::SamplerBindingType::Comparison,
        ),
    ];
    // Bindings 3..=12: base color, metallic-roughness, normal, emissive, occlusion pairs.
    for slot in 0..5u32 {
        entries.push(bind_layout::texture_2d(
            3 + slot * 2,
            wgpu::ShaderStages::FRAGMENT,
            true,
        ));
        entries.push(bind_layout::sampler(
            4 + slot * 2,
            wgpu::ShaderStages::FRAGMENT,
            wgpu::SamplerBindingType::Filtering,
        ));
    }

    entries.push(bind_layout::storage_buffer(
        13,
        wgpu::ShaderStages::VERTEX,
        true,
    ));

    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("forward_bind_group_layout"),
        entries: &entries,
    })
}

pub(super) fn create_forward_pipeline_set(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    pipeline_layout: &wgpu::PipelineLayout,
    front_face: wgpu::FrontFace,
) -> ForwardPipelineSet {
    let make = |cull_mode: Option<wgpu::Face>, blend: bool, label: &str| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(Vertex::LAYOUT), Some(InstanceRaw::LAYOUT)],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    blend: Some(if blend {
                        wgpu::BlendState::ALPHA_BLENDING
                    } else {
                        wgpu::BlendState::REPLACE
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                front_face,
                cull_mode,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(!blend),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: MSAA_SAMPLE_COUNT,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        })
    };
    ForwardPipelineSet {
        opaque: make(Some(wgpu::Face::Back), false, "forward_pipeline"),
        double_sided: make(None, false, "forward_pipeline_double_sided"),
        blend: make(Some(wgpu::Face::Back), true, "forward_pipeline_blend"),
        blend_double_sided: make(None, true, "forward_pipeline_blend_double_sided"),
    }
}
