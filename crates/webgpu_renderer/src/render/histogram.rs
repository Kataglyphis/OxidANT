//! GPU luminance histogram over the HDR target, for auto-exposure.
//! Verified against the CPU binning in [`crate::render::auto_exposure`].

use crate::context::GpuContext;
use crate::render::auto_exposure::{BUILD_WORKGROUP, CLEAR_WORKGROUP, HISTOGRAM_BINS};
use crate::render::bind_layout;
use crate::render::buffer_desc;
use crate::render::gpu_timing::PassScope;
use crate::render::pipeline_desc;

/// Per-frame inputs to the reduction pass.
#[derive(Copy, Clone, Debug)]
pub struct ExposureSettings {
    pub delta_time_seconds: f32,
    /// Adaptation rate constant; 0 snaps straight to the target.
    pub speed: f32,
    pub auto_enabled: bool,
    /// Used when `auto_enabled` is false.
    pub manual_ev: f32,
}

impl Default for ExposureSettings {
    fn default() -> Self {
        Self {
            delta_time_seconds: 1.0 / 60.0,
            speed: 3.0,
            auto_enabled: true,
            manual_ev: 0.0,
        }
    }
}

pub struct HistogramPass {
    build_pipeline: wgpu::ComputePipeline,
    clear_pipeline: wgpu::ComputePipeline,
    reduce_pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: Option<wgpu::BindGroup>,
    histogram_buffer: wgpu::Buffer,
    /// [adapted EV, target EV], kept on the GPU: a per-frame readback would serialise the pipeline.
    exposure_buffer: wgpu::Buffer,
    exposure_params_buffer: wgpu::Buffer,
    exposure_readback_buffer: wgpu::Buffer,
    /// MAP_READ staging target, kept alive so diagnostic readbacks do not churn allocations.
    readback_buffer: wgpu::Buffer,
}

impl HistogramPass {
    pub fn new(gpu: &GpuContext) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("histogram_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/histogram.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("histogram_bind_group_layout"),
            entries: &[
                // textureLoad: a filtering sampler on an HDR target is not guaranteed everywhere.
                bind_layout::texture_2d(0, wgpu::ShaderStages::COMPUTE, false),
                bind_layout::storage_buffer(1, wgpu::ShaderStages::COMPUTE, false),
                bind_layout::storage_buffer(2, wgpu::ShaderStages::COMPUTE, false),
                bind_layout::uniform(3, wgpu::ShaderStages::COMPUTE),
            ],
        });

        let pipeline_layout =
            pipeline_desc::single_layout(device, "histogram_pipeline_layout", &bind_group_layout);

        let build_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("histogram_build_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("cs_build_histogram"),
            compilation_options: Default::default(),
            cache: None,
        });

        let reduce_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("histogram_reduce_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("cs_reduce_exposure"),
            compilation_options: Default::default(),
            cache: None,
        });

        let clear_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("histogram_clear_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("cs_clear_histogram"),
            compilation_options: Default::default(),
            cache: None,
        });

        let byte_size = (HISTOGRAM_BINS * std::mem::size_of::<u32>()) as wgpu::BufferAddress;
        let histogram_buffer = buffer_desc::storage_src(device, "histogram_buffer", byte_size);

        let readback_buffer = buffer_desc::readback(device, "histogram_readback", byte_size);

        let exposure_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("exposure_state"),
            size: 8,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let exposure_params_buffer = buffer_desc::uniform(device, "exposure_params", 16);

        let exposure_readback_buffer = buffer_desc::readback(device, "exposure_readback", 8);

        Self {
            build_pipeline,
            clear_pipeline,
            reduce_pipeline,
            exposure_buffer,
            exposure_params_buffer,
            exposure_readback_buffer,
            bind_group_layout,
            bind_group: None,
            histogram_buffer,
            readback_buffer,
        }
    }

    /// (Re)binds the HDR source; call on every HDR target recreation, or it reads a dead texture.
    pub fn set_input(&mut self, gpu: &GpuContext, hdr_view: &wgpu::TextureView) {
        self.bind_group = Some(gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("histogram_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(hdr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.histogram_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.exposure_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.exposure_params_buffer.as_entire_binding(),
                },
            ],
        }));
    }

    /// Clears and rebuilds the histogram in one encoder, so wgpu's barrier orders clear and build.
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        width: u32,
        height: u32,
        scope: PassScope<'_>,
    ) {
        let Some(bind_group) = self.bind_group.as_ref() else {
            return;
        };

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("histogram_clear"),
                timestamp_writes: scope.compute_writes(0, 2),
            });
            pass.set_pipeline(&self.clear_pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.dispatch_workgroups(
                HISTOGRAM_BINS.div_ceil(CLEAR_WORKGROUP as usize) as u32,
                1,
                1,
            );
        }

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("histogram_build"),
                timestamp_writes: scope.compute_writes(1, 2),
            });
            pass.set_pipeline(&self.build_pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            // Round up: truncating leaves the right and bottom edges unsampled and biases exposure.
            pass.dispatch_workgroups(
                width.div_ceil(BUILD_WORKGROUP),
                height.div_ceil(BUILD_WORKGROUP),
                1,
            );
        }
    }

    /// Uploads this frame's adaptation inputs. Call before [`Self::encode_reduce`].
    pub fn set_exposure_settings(&self, queue: &wgpu::Queue, settings: ExposureSettings) {
        queue.write_buffer(
            &self.exposure_params_buffer,
            0,
            bytemuck::bytes_of(&[
                settings.delta_time_seconds,
                settings.speed,
                if settings.auto_enabled {
                    1.0f32
                } else {
                    0.0f32
                },
                settings.manual_ev,
            ]),
        );
    }

    /// Reduces the histogram to an adapted exposure; encode after [`Self::encode`], same encoder.
    pub fn encode_reduce(&self, encoder: &mut wgpu::CommandEncoder, scope: PassScope<'_>) {
        let Some(bind_group) = self.bind_group.as_ref() else {
            return;
        };
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("exposure_reduce"),
            timestamp_writes: scope.compute_writes(0, 1),
        });
        pass.set_pipeline(&self.reduce_pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }

    /// The buffer the tonemap should read the adapted exposure from.
    pub fn exposure_buffer(&self) -> &wgpu::Buffer {
        &self.exposure_buffer
    }

    /// Resets the adaptation state, so a test or scene change does not inherit the last exposure.
    pub fn reset_exposure(&self, queue: &wgpu::Queue, ev: f32) {
        queue.write_buffer(&self.exposure_buffer, 0, bytemuck::bytes_of(&[ev, ev]));
    }

    /// Copies the exposure state out for tests and diagnostics; stalls, so never on the frame path.
    pub fn encode_exposure_readback(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.copy_buffer_to_buffer(
            &self.exposure_buffer,
            0,
            &self.exposure_readback_buffer,
            0,
            8,
        );
    }

    /// Returns (adapted EV, target EV) from the last copied exposure state.
    pub fn read_back_exposure(&self, gpu: &GpuContext) -> (f32, f32) {
        let slice = self.exposure_readback_buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
        let _ = receiver.recv();

        let values = {
            let data = slice
                .get_mapped_range()
                .expect("map completed above, so viewing it cannot fail");
            let floats: Vec<f32> = data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_ne_bytes(*b))
                .collect();
            (floats[0], floats[1])
        };
        self.exposure_readback_buffer.unmap();
        values
    }

    /// Copies the histogram to staging; after [`Self::encode`], before [`Self::read_back`].
    pub fn encode_readback(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.copy_buffer_to_buffer(
            &self.histogram_buffer,
            0,
            &self.readback_buffer,
            0,
            self.histogram_buffer.size(),
        );
    }

    /// Blocking readback of the last copied histogram; it stalls, so tests and diagnostics only.
    pub fn read_back(&self, gpu: &GpuContext) -> Vec<u32> {
        let slice = self.readback_buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
        let _ = receiver.recv();

        let counts = {
            let data = slice
                .get_mapped_range()
                .expect("map completed above, so viewing it cannot fail");
            data.as_chunks::<4>()
                .0
                .iter()
                .map(|bytes| u32::from_ne_bytes(*bytes))
                .collect::<Vec<u32>>()
        };
        self.readback_buffer.unmap();
        counts
    }
}
