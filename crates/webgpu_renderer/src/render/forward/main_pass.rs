//! The forward main pass: culled opaque draws, the sky, then sorted blended draws, all MSAA.

use glam::Vec3;

use super::frame::FrameView;
use super::primitive::GpuPrimitive;
use super::{ForwardRenderer, DEPTH_FORMAT, HDR_FORMAT, MSAA_SAMPLE_COUNT};
use crate::render::bind_layout;
use crate::render::bounds::aabb_contains_point;
use crate::render::gpu_timing::TimedPass;
use crate::scene::{InstanceRaw, Vertex};

/// The four forward pipelines (back-culled or double-sided, opaque or blended) and their layouts.
pub(super) struct MainPipelines {
    pub(super) opaque: wgpu::RenderPipeline,
    pub(super) double_sided: wgpu::RenderPipeline,
    pub(super) blend: wgpu::RenderPipeline,
    pub(super) blend_double_sided: wgpu::RenderPipeline,
    // Read only by the native shader hot-reload path, so unread on wasm32.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(super) layout: wgpu::PipelineLayout,
    /// Group 0: per-primitive uniforms, the shadow map, five material slots and the joints.
    pub(super) material_layout: wgpu::BindGroupLayout,
}

impl MainPipelines {
    pub(super) fn new(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
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
        let (opaque, double_sided, blend, blend_double_sided) =
            create_forward_pipeline_set(device, shader, &layout);
        Self {
            opaque,
            double_sided,
            blend,
            blend_double_sided,
            layout,
            material_layout,
        }
    }
}

pub(super) fn create_forward_shader(device: &wgpu::Device) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("forward_shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/forward.wgsl").into()),
    })
}

impl ForwardRenderer {
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
        self.draw_blended(&mut pass, view);
        self.culling.drawn = drawn;
        self.culling.considered = considered;
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
            if prim.alpha_blend || !view.frustum.intersects_aabb(prim.aabb_min, prim.aabb_max) {
                continue;
            }
            occ_considered += 1;
            let occluded = self.occluded_last_frame(i);
            // Eye inside the box: only back faces rasterise and fail depth, so never skip.
            if occluded && !aabb_contains_point(prim.aabb_min, prim.aabb_max, view.eye) {
                continue;
            }
            occ_drawn += 1;
            let pipeline = if prim.double_sided {
                &self.main.double_sided
            } else {
                &self.main.opaque
            };
            draw_primitive(pass, pipeline, prim, view.eye);
        }
        (occ_drawn, occ_considered)
    }

    /// Transparents last, farthest first, no depth writes.
    fn draw_blended(&self, pass: &mut wgpu::RenderPass<'_>, view: &FrameView) {
        let eye = view.eye;
        let mut blended: Vec<&GpuPrimitive> = self
            .primitives
            .iter()
            .filter(|p| p.alpha_blend && view.frustum.intersects_aabb(p.aabb_min, p.aabb_max))
            .collect();
        blended.sort_by(|a, b| {
            let da = a.world_center.distance_squared(eye);
            let db = b.world_center.distance_squared(eye);
            db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
        });
        if !blended.is_empty() {
            self.bind_frame_groups(pass);
        }
        for prim in blended {
            let pipeline = if prim.double_sided {
                &self.main.blend_double_sided
            } else {
                &self.main.blend
            };
            draw_primitive(pass, pipeline, prim, eye);
        }
    }
}

/// Binds `prim`'s group 0 and its LOD-selected geometry, then draws every instance.
fn draw_primitive(
    pass: &mut wgpu::RenderPass<'_>,
    pipeline: &wgpu::RenderPipeline,
    prim: &GpuPrimitive,
    eye: Vec3,
) {
    let (vertex_buffer, index_buffer, index_count) = prim.geometry_for(eye);
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &prim.bind_group, &[]);
    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
    pass.set_vertex_buffer(1, prim.instance_buffer.slice(..));
    pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..index_count, 0, 0..prim.instance_count);
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

type ForwardPipelineSet = (
    wgpu::RenderPipeline,
    wgpu::RenderPipeline,
    wgpu::RenderPipeline,
    wgpu::RenderPipeline,
);

pub(super) fn create_forward_pipeline_set(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    pipeline_layout: &wgpu::PipelineLayout,
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
    (
        make(Some(wgpu::Face::Back), false, "forward_pipeline"),
        make(None, false, "forward_pipeline_double_sided"),
        make(Some(wgpu::Face::Back), true, "forward_pipeline_blend"),
        make(None, true, "forward_pipeline_blend_double_sided"),
    )
}
