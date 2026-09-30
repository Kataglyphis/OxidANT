//! Constructors for the pipeline shapes the fullscreen passes share, so only differing fields show.

/// A fullscreen-triangle pipeline's varying fields: one color target, no vertex buffers, no depth.
pub struct FullscreenPipeline<'a> {
    pub label: &'a str,
    pub layout: &'a wgpu::PipelineLayout,
    pub module: &'a wgpu::ShaderModule,
    pub vs_entry: &'a str,
    pub fs_entry: &'a str,
    pub format: wgpu::TextureFormat,
    pub blend: Option<wgpu::BlendState>,
}

/// Builds a fullscreen-triangle pipeline; passes with vertex buffers, MSAA or depth roll their own.
pub fn create_fullscreen_pipeline(
    device: &wgpu::Device,
    desc: FullscreenPipeline<'_>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(desc.label),
        layout: Some(desc.layout),
        vertex: wgpu::VertexState {
            module: desc.module,
            entry_point: Some(desc.vs_entry),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: desc.module,
            entry_point: Some(desc.fs_entry),
            targets: &[Some(wgpu::ColorTargetState {
                format: desc.format,
                blend: desc.blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// Builds a `PipelineLayout` from a single bind group layout with no immediate data.
pub fn single_layout(
    device: &wgpu::Device,
    label: &str,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::PipelineLayout {
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    })
}

#[cfg(test)]
mod tests {
    /// The fullscreen modules must build pipelines through `create_fullscreen_pipeline`.
    #[test]
    fn the_fullscreen_passes_do_not_hand_roll_a_pipeline_descriptor() {
        for (name, src) in [
            ("bloom.rs", include_str!("bloom.rs")),
            ("ssao.rs", include_str!("ssao.rs")),
            ("tonemap.rs", include_str!("tonemap.rs")),
            ("ibl.rs", include_str!("ibl.rs")),
        ] {
            assert!(
                !src.contains("RenderPipelineDescriptor {"),
                "{name} hand-rolls a RenderPipelineDescriptor - use create_fullscreen_pipeline instead"
            );
        }
    }
}
