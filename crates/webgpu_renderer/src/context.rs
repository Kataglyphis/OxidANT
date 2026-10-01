//! Instance/adapter/device/queue management plus surface configuration.
//! `Outdated`/`Lost` reconfigure and retry next frame; a suboptimal acquire reconfigures only after presenting.

use std::sync::Arc;

use anyhow::Context as _;
use winit::window::Window;

pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    /// TEXTURE_COMPRESSION_BC was available and enabled on this device.
    pub supports_bc: bool,
    /// TIMESTAMP_QUERY was available and enabled; browsers rarely expose it, so GPU timings are desktop-only.
    pub supports_timestamps: bool,
    /// The adapter the device was created on.
    pub adapter_info: wgpu::AdapterInfo,
    /// Present target; `None` for headless (render-to-texture) contexts.
    pub surface: Option<wgpu::Surface<'static>>,
    pub surface_config: Option<wgpu::SurfaceConfiguration>,
}

impl GpuContext {
    /// Creates a context that presents to `window` (native: blocks).
    pub fn new_windowed(window: Arc<Window>) -> anyhow::Result<Self> {
        pollster::block_on(Self::new_windowed_async(window))
    }

    /// Async variant for platforms without block_on (wasm32/browsers).
    pub async fn new_windowed_async(window: Arc<Window>) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });
        let surface = instance
            .create_surface(Arc::clone(&window))
            .context("Failed to create surface")?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                // Bucketing is anti-fingerprinting for untrusted content; a trusted app wants the real limits.
                apply_limit_buckets: false,
            })
            .await
            .context("No suitable GPU adapter found")?;

        let (device, queue, supports_bc, supports_timestamps) = request_device(&adapter).await?;
        let adapter_info = adapter.get_info();

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .or_else(|| caps.formats.first().copied())
            .context("Surface reports no supported texture formats")?;
        let alpha_mode = caps
            .alpha_modes
            .first()
            .copied()
            .context("Surface reports no supported alpha modes")?;

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            // `Auto` is the only colour space guaranteed for every format; an HDR space needs a capability check.
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        Ok(Self {
            device,
            queue,
            supports_bc,
            supports_timestamps,
            adapter_info,
            surface: Some(surface),
            surface_config: Some(surface_config),
        })
    }

    /// Creates a surfaceless context for render-to-texture (tests, CI).
    pub fn new_headless() -> anyhow::Result<Self> {
        pollster::block_on(Self::new_headless_async())
    }

    async fn new_headless_async() -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
                // Trusted application: real limits, as in the windowed path.
                apply_limit_buckets: false,
            })
            .await
            .context("No GPU adapter found (headless)")?;

        // GL counts as no adapter: naga cannot translate SSAO's depth `textureLoad` to GLSL, so pipelines abort.
        let adapter_info = adapter.get_info();
        if adapter_info.backend == wgpu::Backend::Gl {
            anyhow::bail!(
                "headless adapter '{}' is the OpenGL backend, which cannot run this \
                 renderer's WGSL (depth textureLoad has no GLSL translation); \
                 treating as no usable adapter",
                adapter_info.name
            );
        }

        let (device, queue, supports_bc, supports_timestamps) = request_device(&adapter).await?;
        Ok(Self {
            device,
            queue,
            supports_bc,
            supports_timestamps,
            adapter_info,
            surface: None,
            surface_config: None,
        })
    }

    /// [`Self::new_headless`] for tests: `None` after a SKIP line, or a panic if `KATAGLYPHIS_REQUIRE_GPU` is set.
    /// See <https://github.com/Kataglyphis/BeschleunigerBallett/blob/develop/docs/gpu-golden-testing.md>.
    #[doc(hidden)]
    pub fn headless_or_skip() -> Option<Self> {
        match Self::new_headless() {
            Ok(gpu) => Some(gpu),
            Err(err) => {
                if Self::gpu_required() {
                    panic!("KATAGLYPHIS_REQUIRE_GPU is set but no GPU adapter is usable: {err}");
                }
                eprintln!("SKIP: no GPU adapter available in this environment");
                None
            }
        }
    }

    /// `KATAGLYPHIS_REQUIRE_GPU` is set and non-empty: a GPU test must fail rather than skip itself.
    #[doc(hidden)]
    pub fn gpu_required() -> bool {
        std::env::var("KATAGLYPHIS_REQUIRE_GPU").is_ok_and(|v| !v.is_empty())
    }

    pub fn surface_format(&self) -> Option<wgpu::TextureFormat> {
        self.surface_config.as_ref().map(|c| c.format)
    }

    /// Reconfigures the surface for a new size; zero sizes (minimized windows) are ignored.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if let (Some(surface), Some(config)) = (self.surface.as_ref(), self.surface_config.as_mut())
        {
            config.width = width;
            config.height = height;
            surface.configure(&self.device, config);
        }
    }

    /// Reconfigures the surface with its current size (Outdated/Lost recovery).
    pub fn reconfigure(&self) {
        if let (Some(surface), Some(config)) = (self.surface.as_ref(), self.surface_config.as_ref())
        {
            surface.configure(&self.device, config);
        }
    }
}

async fn request_device(
    adapter: &wgpu::Adapter,
) -> anyhow::Result<(wgpu::Device, wgpu::Queue, bool, bool)> {
    // Request only features the adapter reports: naming a missing one fails the whole device request.
    let supports_bc = adapter
        .features()
        .contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
    let supports_timestamps = adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
    let mut required_features = wgpu::Features::empty();
    if supports_bc {
        required_features |= wgpu::Features::TEXTURE_COMPRESSION_BC;
    }
    if supports_timestamps {
        required_features |= wgpu::Features::TIMESTAMP_QUERY;
    }

    // forward.wgsl binds 5 groups, past the default max_bind_groups of 4; browsers cap it at 4 regardless.
    let mut required_limits = wgpu::Limits::default();
    #[cfg(not(target_arch = "wasm32"))]
    {
        // The adapter's own maximum, since anything above it fails request_device.
        required_limits.max_bind_groups = adapter.limits().max_bind_groups;
    }

    adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("webgpu_renderer_device"),
            required_features,
            required_limits,
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::default(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
        })
        .await
        .map(|(device, queue)| (device, queue, supports_bc, supports_timestamps))
        .context("Failed to create device")
}
