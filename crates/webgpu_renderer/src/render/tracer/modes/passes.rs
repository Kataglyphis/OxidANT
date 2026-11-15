//! The traced modes on the GPU: both kernels, the accumulation they write, and the resolve into the HDR target.

use super::dispatch::TraceDispatch;
use crate::context::GpuContext;
use crate::render::forward::{DEPTH_FORMAT, HDR_FORMAT};
use crate::render::gpu_timing::PassScope;
use crate::render::pipeline_desc;
use crate::render::tracer::gpu::{TraceGpuScene, TRACE_SCENE_GROUP};
use crate::render::{bind_layout, buffer_desc};

/// `trace_shading.slang`'s `TRACE_GROUP_SIZE`: a kernel's workgroup is this square of pixels.
pub const KERNEL_GROUP_SIZE: u32 = 8;
/// Dispatch slots the tile buffer starts with; it doubles when a frame needs more.
const INITIAL_TILE_SLOTS: usize = 64;

/// `TraceFrame` in `trace_shading.slang`, whose first field `trace_resolve.slang` reads too.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TraceFrameUniforms {
    /// Width, height, the path tracer's bounce cap, unused.
    pub size: [u32; 4],
    pub inv_view_proj: [[f32; 4]; 4],
    /// Towards the sun, unit length; w is the sky's sun-disk intensity.
    pub light_dir: [f32; 4],
    /// Colour times intensity; w is the sky's light, the raster's ambient strength times its environment's.
    pub light_radiance: [f32; 4],
    /// The white furnace's sky radiance, metallic and roughness; w is 1 when it is on.
    pub furnace: [f32; 4],
    /// The raster's world to clip, for the primary hit's depth.
    pub view_proj: [[f32; 4]; 4],
}

/// `TraceTile` in `trace_shading.slang`: one dispatch's tile and samples.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TraceTileUniforms {
    pub rect: [u32; 4],
    pub samples: [u32; 4],
}

impl From<&TraceDispatch> for TraceTileUniforms {
    fn from(d: &TraceDispatch) -> Self {
        Self {
            rect: [d.x, d.y, d.width, d.height],
            samples: [d.first_sample, d.samples, d.accumulated, 0],
        }
    }
}

/// Which kernel a traced frame runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kernel {
    Ray,
    Path,
}

/// Draws into the resolve's pass after it, against the depth it wrote: the ray-traced mode's sky.
pub type AfterResolve<'a> = &'a dyn Fn(&mut wgpu::RenderPass<'_>);

/// One traced frame: what it traces, how, and the HDR and single-sample depth targets it resolves into.
pub struct TracedFrame<'a> {
    pub kernel: Kernel,
    pub scene: &'a TraceGpuScene,
    pub uniforms: TraceFrameUniforms,
    pub dispatches: &'a [TraceDispatch],
    pub hdr_view: &'a wgpu::TextureView,
    pub depth_view: &'a wgpu::TextureView,
    pub after_resolve: Option<AfterResolve<'a>>,
}

/// The WGSL of the traced kernels and their resolve: the generated files, or a shader reload's.
#[derive(Clone, Copy, Debug)]
pub struct TracedSources<'a> {
    pub traced: &'a str,
    pub resolve: &'a str,
}

impl TracedSources<'static> {
    pub fn shipped() -> Self {
        Self {
            traced: include_str!("../../../shaders/traced.wgsl"),
            resolve: include_str!("../../../shaders/trace_resolve.wgsl"),
        }
    }
}

/// The accumulation of one output size and the groups that bind it.
struct Target {
    size: (u32, u32),
    accumulation: wgpu::Buffer,
    kernel_group: wgpu::BindGroup,
    resolve_group: wgpu::BindGroup,
}

/// The per-dispatch uniforms, one slot per dispatch at the device's offset alignment.
struct TileSlots {
    buffer: wgpu::Buffer,
    stride: u64,
    capacity: usize,
}

/// The kernels' module and layout; a kernel compiles the first time its mode draws, since one costs minutes on a software adapter.
struct Kernels {
    module: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    ray: Option<wgpu::ComputePipeline>,
    path: Option<wgpu::ComputePipeline>,
}

impl Kernels {
    /// The scene's group comes second, as `bvh_traverse.slang` declares it.
    fn new(device: &wgpu::Device, kernel_layout: &wgpu::BindGroupLayout, wgsl: &str) -> Self {
        let scene_layout = TraceGpuScene::bind_group_layout(device);
        debug_assert_eq!(TRACE_SCENE_GROUP, 1, "the layout lists the scene second");
        Self {
            module: device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("traced"),
                source: wgpu::ShaderSource::Wgsl(wgsl.into()),
            }),
            layout: device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("traced_kernel_pipeline_layout"),
                bind_group_layouts: &[Some(kernel_layout), Some(&scene_layout)],
                immediate_size: 0,
            }),
            ray: None,
            path: None,
        }
    }

    fn compile(&mut self, device: &wgpu::Device, kernel: Kernel) {
        let (slot, entry_point) = match kernel {
            Kernel::Ray => (&mut self.ray, "cs_ray_trace"),
            Kernel::Path => (&mut self.path, "cs_path_trace"),
        };
        slot.get_or_insert_with(|| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&self.layout),
                module: &self.module,
                entry_point: Some(entry_point),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            })
        });
    }

    fn compiled(&self, kernel: Kernel) -> &wgpu::ComputePipeline {
        match kernel {
            Kernel::Ray => self.ray.as_ref(),
            Kernel::Path => self.path.as_ref(),
        }
        .expect("compiled before its first dispatch")
    }
}

/// Both kernels, the resolve and the buffers they share; built the first time a traced mode draws.
pub struct TracedPasses {
    kernels: Kernels,
    resolve: wgpu::RenderPipeline,
    kernel_layout: wgpu::BindGroupLayout,
    resolve_layout: wgpu::BindGroupLayout,
    frame: wgpu::Buffer,
    tiles: TileSlots,
    target: Option<Target>,
}

impl TracedPasses {
    pub fn new(device: &wgpu::Device) -> Self {
        Self::with_sources(device, TracedSources::shipped())
    }

    /// Both kernels compiled now rather than on first use, so a shader reload's error scope sees every entry point.
    pub fn compiled(device: &wgpu::Device, sources: TracedSources<'_>) -> Self {
        let mut passes = Self::with_sources(device, sources);
        passes.kernels.compile(device, Kernel::Ray);
        passes.kernels.compile(device, Kernel::Path);
        passes
    }

    /// Whether `kernel`'s pipeline has been built.
    pub fn is_compiled(&self, kernel: Kernel) -> bool {
        match kernel {
            Kernel::Ray => self.kernels.ray.is_some(),
            Kernel::Path => self.kernels.path.is_some(),
        }
    }

    fn with_sources(device: &wgpu::Device, sources: TracedSources<'_>) -> Self {
        let compute = wgpu::ShaderStages::COMPUTE;
        let tile_size = std::mem::size_of::<TraceTileUniforms>() as u64;
        let kernel_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("traced_kernel_layout"),
            entries: &[
                bind_layout::uniform(0, compute),
                bind_layout::storage_buffer(1, compute, false),
                bind_layout::dynamic_uniform(2, compute, tile_size),
            ],
        });
        let resolve_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("traced_resolve_layout"),
            entries: &[
                bind_layout::uniform(0, wgpu::ShaderStages::FRAGMENT),
                bind_layout::storage_buffer(1, wgpu::ShaderStages::FRAGMENT, true),
            ],
        });
        let stride = tile_size.next_multiple_of(u64::from(
            device.limits().min_uniform_buffer_offset_alignment,
        ));
        Self {
            kernels: Kernels::new(device, &kernel_layout, sources.traced),
            resolve: create_resolve(device, &resolve_layout, sources.resolve),
            kernel_layout,
            resolve_layout,
            frame: buffer_desc::uniform(
                device,
                "traced_frame",
                std::mem::size_of::<TraceFrameUniforms>() as u64,
            ),
            tiles: TileSlots::new(device, stride, INITIAL_TILE_SLOTS),
            target: None,
        }
    }

    /// Records the frame's dispatches and the resolve; an output this device cannot hold an accumulation for is refused.
    pub fn encode(
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        frame: &TracedFrame<'_>,
        (trace_scope, resolve_scope): (PassScope<'_>, PassScope<'_>),
    ) -> Result<(), String> {
        let size = (frame.uniforms.size[0], frame.uniforms.size[1]);
        self.prepare(gpu, size, frame.dispatches.len())?;
        self.kernels.compile(&gpu.device, frame.kernel);
        gpu.queue
            .write_buffer(&self.frame, 0, bytemuck::bytes_of(&frame.uniforms));
        self.tiles.write(gpu, frame.dispatches);
        self.encode_kernel(encoder, frame, trace_scope);
        self.encode_resolve(encoder, frame, resolve_scope);
        Ok(())
    }

    /// Every dispatch in one compute pass; each binds its tile slot at its own offset.
    fn encode_kernel(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        frame: &TracedFrame<'_>,
        scope: PassScope<'_>,
    ) {
        let target = self.target.as_ref().expect("prepared before the kernels");
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("traced_kernel"),
            timestamp_writes: scope.compute_writes(0, 1),
        });
        pass.set_pipeline(self.kernels.compiled(frame.kernel));
        pass.set_bind_group(TRACE_SCENE_GROUP, frame.scene.bind_group(), &[]);
        for (slot, dispatch) in frame.dispatches.iter().enumerate() {
            let offset = (slot as u64 * self.tiles.stride) as u32;
            pass.set_bind_group(0, &target.kernel_group, &[offset]);
            pass.dispatch_workgroups(
                dispatch.width.div_ceil(KERNEL_GROUP_SIZE),
                dispatch.height.div_ceil(KERNEL_GROUP_SIZE),
                1,
            );
        }
    }

    /// The accumulation for `size` and room for `dispatches` tile slots, with groups binding both.
    fn prepare(
        &mut self,
        gpu: &GpuContext,
        size: (u32, u32),
        dispatches: usize,
    ) -> Result<(), String> {
        let grown = self.tiles.reserve(&gpu.device, dispatches);
        if grown || self.target.as_ref().is_none_or(|t| t.size != size) {
            let accumulation = match self.target.take() {
                Some(target) if target.size == size => target.accumulation,
                _ => create_accumulation(&gpu.device, size)?,
            };
            self.target = Some(self.bind(&gpu.device, size, accumulation));
        }
        Ok(())
    }

    fn bind(&self, device: &wgpu::Device, size: (u32, u32), accumulation: wgpu::Buffer) -> Target {
        let tile = wgpu::BufferBinding {
            buffer: &self.tiles.buffer,
            offset: 0,
            size: wgpu::BufferSize::new(std::mem::size_of::<TraceTileUniforms>() as u64),
        };
        let kernel_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("traced_kernel_group"),
            layout: &self.kernel_layout,
            entries: &[
                buffer_entry(0, self.frame.as_entire_binding()),
                buffer_entry(1, accumulation.as_entire_binding()),
                buffer_entry(2, wgpu::BindingResource::Buffer(tile)),
            ],
        });
        let resolve_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("traced_resolve_group"),
            layout: &self.resolve_layout,
            entries: &[
                buffer_entry(0, self.frame.as_entire_binding()),
                buffer_entry(1, accumulation.as_entire_binding()),
            ],
        });
        Target {
            size,
            accumulation,
            kernel_group,
            resolve_group,
        }
    }

    /// The accumulation into the HDR and depth targets, every pixel written, then what draws against that depth.
    fn encode_resolve(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        frame: &TracedFrame<'_>,
        scope: PassScope<'_>,
    ) {
        let target = self.target.as_ref().expect("prepared before the kernels");
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("traced_resolve"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: frame.hdr_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: frame.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: scope.render_writes(0, 1),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.resolve);
        pass.set_bind_group(0, &target.resolve_group, &[]);
        pass.draw(0..3, 0..1);
        if let Some(draw) = frame.after_resolve {
            draw(&mut pass);
        }
    }
}

impl TileSlots {
    fn new(device: &wgpu::Device, stride: u64, capacity: usize) -> Self {
        Self {
            buffer: buffer_desc::uniform(device, "traced_tiles", stride * capacity as u64),
            stride,
            capacity,
        }
    }

    /// Grows to at least `count` slots; true when the buffer was replaced, so its groups must be too.
    fn reserve(&mut self, device: &wgpu::Device, count: usize) -> bool {
        if count <= self.capacity {
            return false;
        }
        *self = Self::new(device, self.stride, count.next_power_of_two());
        true
    }

    fn write(&self, gpu: &GpuContext, dispatches: &[TraceDispatch]) {
        let stride = self.stride as usize;
        let mut bytes = vec![0u8; dispatches.len() * stride];
        for (slot, dispatch) in bytes.chunks_exact_mut(stride).zip(dispatches) {
            let uniforms = TraceTileUniforms::from(dispatch);
            slot[..std::mem::size_of::<TraceTileUniforms>()]
                .copy_from_slice(bytemuck::bytes_of(&uniforms));
        }
        gpu.queue.write_buffer(&self.buffer, 0, &bytes);
    }
}

/// One running mean per pixel, or why this device cannot bind one that large.
/// The largest accumulation `limits` let the kernels bind, in bytes.
pub fn accumulation_limit(limits: &wgpu::Limits) -> u64 {
    limits
        .max_storage_buffer_binding_size
        .min(limits.max_buffer_size)
}

/// The bytes a `width` x `height` accumulation takes, or why `limit` cannot hold it.
pub fn accumulation_bytes((width, height): (u32, u32), limit: u64) -> Result<u64, String> {
    let bytes = u64::from(width) * u64::from(height) * 16;
    if bytes > limit {
        return Err(format!(
            "a {width} x {height} accumulation needs {bytes} bytes, past this device's {limit}"
        ));
    }
    Ok(bytes)
}

fn create_accumulation(device: &wgpu::Device, size: (u32, u32)) -> Result<wgpu::Buffer, String> {
    let bytes = accumulation_bytes(size, accumulation_limit(&device.limits()))?;
    Ok(buffer_desc::storage_src(
        device,
        "traced_accumulation",
        bytes.max(16),
    ))
}

fn buffer_entry(binding: u32, resource: wgpu::BindingResource<'_>) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry { binding, resource }
}

/// A full-screen copy that writes every pixel's depth too, so it tests nothing and always writes.
fn create_resolve(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    wgsl: &str,
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("trace_resolve"),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    let pipeline_layout =
        pipeline_desc::single_layout(device, "traced_resolve_pipeline_layout", layout);
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("traced_resolve"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}
