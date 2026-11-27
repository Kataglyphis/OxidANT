//! The forward main pass: opaque draws, the sky, then sorted blended draws, all MSAA and all indirect.
//! Its draw loops and pipeline builder also serve the deferred path's passes.

use super::frame::FrameView;
use super::primitive::GpuPrimitive;
use super::{ForwardRenderer, DEPTH_FORMAT, HDR_FORMAT, MSAA_SAMPLE_COUNT};
use crate::render::bind_layout;
use crate::render::gpu_timing::TimedPass;
use crate::render::graph::Resource;
use crate::scene::{InstanceRaw, Vertex, VertexLayout as _};

/// Where the sky's background starts before anything draws, in both raster paths.
pub(super) const HDR_CLEAR: wgpu::Color = wgpu::Color {
    r: 0.05,
    g: 0.05,
    b: 0.08,
    a: 1.0,
};

/// One winding's four scene pipelines: back-culled or double-sided, opaque or blended.
pub(super) struct ForwardPipelineSet {
    opaque: wgpu::RenderPipeline,
    double_sided: wgpu::RenderPipeline,
    blend: wgpu::RenderPipeline,
    blend_double_sided: wgpu::RenderPipeline,
}

impl ForwardPipelineSet {
    /// Builds the four from the opaque and the blended target spec; `label` prefixes each pipeline's name.
    pub(super) fn build(
        device: &wgpu::Device,
        source: &PipelineSource<'_>,
        front_face: wgpu::FrontFace,
        [opaque, blend]: [&SceneTargets<'_>; 2],
        label: &str,
    ) -> Self {
        let make = |targets: &SceneTargets<'_>, cull_mode: Option<wgpu::Face>, suffix: &str| {
            let label = format!("{label}{suffix}");
            scene_pipeline(device, source, targets, (front_face, cull_mode), &label)
        };
        let back = Some(wgpu::Face::Back);
        Self {
            opaque: make(opaque, back, ""),
            double_sided: make(opaque, None, "_double_sided"),
            blend: make(blend, back, "_blend"),
            blend_double_sided: make(blend, None, "_blend_double_sided"),
        }
    }

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
    /// SSAO's depth prepass on the forward path, both windings as above; its blended slots repeat the opaque ones.
    pub(super) prepass: ForwardPipelineSet,
    pub(super) prepass_mirrored: Option<ForwardPipelineSet>,
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
        let prepass = create_prepass_pipeline_set(device, &shader, &layout, wgpu::FrontFace::Ccw);
        Self {
            upright,
            mirrored: None,
            prepass,
            prepass_mirrored: None,
            shader,
            layout,
            material_layout,
        }
    }

    /// The forward pass's sets for both windings.
    pub(super) fn sets(&self) -> WindingSets<'_> {
        WindingSets {
            upright: &self.upright,
            mirrored: self.mirrored.as_ref(),
        }
    }

    /// The depth prepass's sets for both windings.
    pub(super) fn prepass_sets(&self) -> WindingSets<'_> {
        WindingSets {
            upright: &self.prepass,
            mirrored: self.prepass_mirrored.as_ref(),
        }
    }
}

/// The pipeline sets a pass draws primitives with, one per front-face winding.
#[derive(Clone, Copy)]
pub(super) struct WindingSets<'a> {
    pub(super) upright: &'a ForwardPipelineSet,
    pub(super) mirrored: Option<&'a ForwardPipelineSet>,
}

impl<'a> WindingSets<'a> {
    /// The set whose front faces wind clockwise when `clockwise`, else counter-clockwise.
    fn for_winding(self, clockwise: bool) -> &'a ForwardPipelineSet {
        if !clockwise {
            return self.upright;
        }
        debug_assert!(
            self.mirrored.is_some(),
            "the mirrored set must be built before a mirrored primitive is drawn"
        );
        self.mirrored.unwrap_or(self.upright)
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
        if self.main.prepass_mirrored.is_none() {
            self.main.prepass_mirrored = Some(create_prepass_pipeline_set(
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
        self.draw_opaque(&mut pass, view, self.main.sets());
        self.sky.draw(&mut pass);
        self.draw_blended(&mut pass, view, self.main.sets());
    }

    /// SSAO's single-sample depth of the opaque and MASK primitives, before the forward pass shades them.
    pub(super) fn encode_depth_prepass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &FrameView,
        timestamp_writes: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("depth_prepass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: self.targets.view(Resource::PrepassDepth),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.bind_frame_groups(&mut pass);
        self.draw_opaque(&mut pass, view, self.main.prepass_sets());
    }

    fn begin_forward_pass<'e>(
        &self,
        encoder: &'e mut wgpu::CommandEncoder,
    ) -> wgpu::RenderPass<'e> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("forward_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.targets.view(Resource::HdrMsaa),
                resolve_target: Some(self.targets.view(Resource::HdrColor)),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(HDR_CLEAR),
                    store: wgpu::StoreOp::Discard,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: self.targets.view(Resource::DepthMsaa),
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

    pub(super) fn bind_frame_groups(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(1, &self.ibl.bind_group, &[]);
        pass.set_bind_group(2, &self.frame.bind_group, &[]);
        pass.set_bind_group(3, &self.lights.bind_group, &[]);
    }

    /// Outside the camera frustum, unless frustum culling is off: the draw cull would zero it too, so it is never encoded.
    fn frustum_culls(&self, prim: &GpuPrimitive, view: &FrameView) -> bool {
        self.frustum_culling && !view.frustum.intersects_aabb(prim.aabb_min, prim.aabb_max)
    }

    /// Opaque primitives in upload order; the draw cull zeroed the instances of those occlusion culled.
    pub(super) fn draw_opaque(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        view: &FrameView,
        sets: WindingSets<'_>,
    ) {
        for (i, prim) in self.primitives.iter().enumerate() {
            if !prim.alpha_blend && !self.frustum_culls(prim, view) {
                self.draw_primitive(pass, sets, i, view);
            }
        }
    }

    /// Transparents last, farthest first, no depth writes.
    pub(super) fn draw_blended(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        view: &FrameView,
        sets: WindingSets<'_>,
    ) {
        let eye = view.eye;
        let mut blended: Vec<usize> = (0..self.primitives.len())
            .filter(|&i| {
                let prim = &self.primitives[i];
                prim.alpha_blend && !self.frustum_culls(prim, view)
            })
            .collect();
        let distance = |i: usize| self.primitives[i].world_center.distance_squared(eye);
        blended.sort_by(|&a, &b| {
            distance(b)
                .partial_cmp(&distance(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if !blended.is_empty() {
            self.bind_frame_groups(pass);
        }
        for i in blended {
            self.draw_primitive(pass, sets, i, view);
        }
    }

    /// Binds primitive `index`'s group 0 and LOD-selected geometry, then draws each slot from its camera arguments.
    fn draw_primitive(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        sets: WindingSets<'_>,
        index: usize,
        view: &FrameView,
    ) {
        let prim = &self.primitives[index];
        let draw_cull = &self.culling.draw_cull;
        let (vertex_buffer, index_buffer, _) = prim.geometry_for(view.eye);
        pass.set_bind_group(0, &prim.bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        for (slot_index, slot) in self.culling.slots_of(index) {
            let instances = prim.instance_buffer.slice(slot.instance_offset()..);
            pass.set_vertex_buffer(1, instances);
            let set = sets.for_winding(slot.clockwise);
            pass.set_pipeline(set.pick(prim.double_sided, prim.alpha_blend));
            pass.draw_indexed_indirect(draw_cull.args(), draw_cull.args_offset(0, slot_index));
        }
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
    let source = PipelineSource {
        shader,
        layout: pipeline_layout,
    };
    let opaque = [Some(hdr_target(false))];
    let blend = [Some(hdr_target(true))];
    ForwardPipelineSet::build(
        device,
        &source,
        front_face,
        [
            &SceneTargets::new("fs_main", &opaque, true, MSAA_SAMPLE_COUNT),
            &SceneTargets::new("fs_main", &blend, false, MSAA_SAMPLE_COUNT),
        ],
        "forward_pipeline",
    )
}

/// SSAO's depth prepass: `fs_depth_prepass`, no colour, one sample; the blended slots repeat the opaque pipelines.
pub(super) fn create_prepass_pipeline_set(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    pipeline_layout: &wgpu::PipelineLayout,
    front_face: wgpu::FrontFace,
) -> ForwardPipelineSet {
    let source = PipelineSource {
        shader,
        layout: pipeline_layout,
    };
    let targets = SceneTargets::new("fs_depth_prepass", &[], true, 1);
    let opaque = scene_pipeline(
        device,
        &source,
        &targets,
        (front_face, Some(wgpu::Face::Back)),
        "depth_prepass",
    );
    let double_sided = scene_pipeline(
        device,
        &source,
        &targets,
        (front_face, None),
        "depth_prepass_double_sided",
    );
    ForwardPipelineSet {
        blend: opaque.clone(),
        blend_double_sided: double_sided.clone(),
        opaque,
        double_sided,
    }
}

/// The HDR colour target, alpha-blended for BLEND materials.
pub(super) fn hdr_target(blend: bool) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format: HDR_FORMAT,
        blend: Some(if blend {
            wgpu::BlendState::ALPHA_BLENDING
        } else {
            wgpu::BlendState::REPLACE
        }),
        write_mask: wgpu::ColorWrites::ALL,
    }
}

/// The forward module and pipeline layout every scene pipeline is built from.
pub(super) struct PipelineSource<'a> {
    pub(super) shader: &'a wgpu::ShaderModule,
    pub(super) layout: &'a wgpu::PipelineLayout,
}

/// What a scene pipeline writes: its fragment entry point, colour targets, depth writes and samples.
pub(super) struct SceneTargets<'a> {
    fragment: &'a str,
    targets: &'a [Option<wgpu::ColorTargetState>],
    depth_write: bool,
    samples: u32,
}

impl<'a> SceneTargets<'a> {
    pub(super) fn new(
        fragment: &'a str,
        targets: &'a [Option<wgpu::ColorTargetState>],
        depth_write: bool,
        samples: u32,
    ) -> Self {
        Self {
            fragment,
            targets,
            depth_write,
            samples,
        }
    }
}

/// One pipeline over `vs_main` and the scene vertex layout, depth-tested against [`DEPTH_FORMAT`].
fn scene_pipeline(
    device: &wgpu::Device,
    source: &PipelineSource<'_>,
    spec: &SceneTargets<'_>,
    (front_face, cull_mode): (wgpu::FrontFace, Option<wgpu::Face>),
    label: &str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(source.layout),
        vertex: wgpu::VertexState {
            module: source.shader,
            entry_point: Some("vs_main"),
            buffers: &[Some(Vertex::LAYOUT), Some(InstanceRaw::LAYOUT)],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: source.shader,
            entry_point: Some(spec.fragment),
            targets: spec.targets,
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            front_face,
            cull_mode,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(spec.depth_write),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: spec.samples,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}
