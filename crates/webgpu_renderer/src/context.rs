//! Instance/adapter/device/queue management plus surface configuration and recovery.
//! A failed acquire repairs the surface ([`GpuContext::acquire`]); a suboptimal one reconfigures after presenting.

use std::sync::Arc;

use anyhow::Context as _;
use winit::window::Window;

/// The backends this renderer can run; GL is not one: naga cannot translate the depth resolve's and SSAO's
/// depth `textureLoad` to GLSL, and a GL adapter could outrank lavapipe in adapter selection.
const BACKENDS: wgpu::Backends = wgpu::Backends::PRIMARY;

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
    /// The instance and window the surface came from, to make it again when the platform loses it.
    origin: Option<(wgpu::Instance, Arc<Window>)>,
    /// The size last asked of the surface, before the device's texture limit scaled it down.
    requested_size: Option<(u32, u32)>,
    /// The frame in flight came from a suboptimal acquire: reconfigure once it is presented.
    suboptimal: bool,
    /// Failed acquires in a row, so a surface that stays unusable logs once, not every frame.
    failed_acquires: u32,
}

/// When a frame whose acquire failed should try again; the surface is already repaired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcquireRetry {
    /// Next frame: the surface was reconfigured or made anew, or the acquire timed out.
    NextFrame,
    /// Once the window is visible again: an occluded surface hands out nothing, so retrying only spins.
    WhenVisible,
}

/// What a failed acquire asks of the surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Repair {
    Nothing,
    Reconfigure,
    Recreate,
}

impl GpuContext {
    /// Creates a context that presents to `window` (native: blocks).
    pub fn new_windowed(window: Arc<Window>) -> anyhow::Result<Self> {
        pollster::block_on(Self::new_windowed_async(window))
    }

    /// Async variant for platforms without block_on (wasm32/browsers).
    pub async fn new_windowed_async(window: Arc<Window>) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = new_instance();
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
        let adapter_info = adapter.get_info();
        check_adapter(&adapter_info)?;

        let (device, queue, supports_bc, supports_timestamps) = request_device(&adapter).await?;
        let surface_config = surface_config(&surface, &adapter, &device, size)?;
        surface.configure(&device, &surface_config);

        Ok(Self {
            device,
            queue,
            supports_bc,
            supports_timestamps,
            adapter_info,
            surface: Some(surface),
            surface_config: Some(surface_config),
            origin: Some((instance, window)),
            requested_size: Some((size.width.max(1), size.height.max(1))),
            suboptimal: false,
            failed_acquires: 0,
        })
    }

    /// Creates a surfaceless context for render-to-texture (tests, CI).
    pub fn new_headless() -> anyhow::Result<Self> {
        pollster::block_on(Self::new_headless_async())
    }

    async fn new_headless_async() -> anyhow::Result<Self> {
        let adapter = new_instance()
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
                // Trusted application: real limits, as in the windowed path.
                apply_limit_buckets: false,
            })
            .await
            .context("No GPU adapter found (headless)")?;
        let adapter_info = adapter.get_info();
        check_adapter(&adapter_info)?;

        let (device, queue, supports_bc, supports_timestamps) = request_device(&adapter).await?;
        Ok(Self {
            device,
            queue,
            supports_bc,
            supports_timestamps,
            adapter_info,
            surface: None,
            surface_config: None,
            origin: None,
            requested_size: None,
            suboptimal: false,
            failed_acquires: 0,
        })
    }

    /// [`Self::new_headless`] for tests: `None` after a SKIP line, or a panic if `KATAGLYPHIS_REQUIRE_GPU` is set.
    /// See <https://github.com/Kataglyphis/BeschleunigerBallett/blob/develop/docs/source/gpu-golden-testing.md>.
    #[doc(hidden)]
    pub fn headless_or_skip() -> Option<Self> {
        static REPORTED: std::sync::Once = std::sync::Once::new();
        match Self::new_headless() {
            Ok(gpu) => {
                REPORTED.call_once(|| gpu.report_adapter());
                Some(gpu)
            }
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

    /// One `ADAPTER:` line on stderr naming the adapter and the optional features the tests use.
    fn report_adapter(&self) {
        let info = &self.adapter_info;
        let line = format!(
            "ADAPTER: {} ({:?}, {:?}, driver '{}' '{}') bc={} timestamps={}\n",
            info.name,
            info.backend,
            info.device_type,
            info.driver,
            info.driver_info,
            self.supports_bc,
            self.supports_timestamps
        );
        // A direct write: libtest captures eprintln!, and a crash in the first render must not hide the adapter.
        let _ = std::io::Write::write_all(&mut std::io::stderr(), line.as_bytes());
    }

    pub fn surface_format(&self) -> Option<wgpu::TextureFormat> {
        self.surface_config.as_ref().map(|c| c.format)
    }

    /// Checks an offscreen target's size against this device, so a tool reports it instead of wgpu panicking.
    pub fn check_target_size(&self, width: u32, height: u32) -> anyhow::Result<()> {
        check_target_size(width, height, self.device.limits().max_texture_dimension_2d)
    }

    /// Reconfigures for a new size; zero (a minimized window) is ignored, and over the device's limit the surface shrinks.
    pub fn resize(&mut self, width: u32, height: u32) {
        let max_side = self.device.limits().max_texture_dimension_2d;
        let Some((fit_width, fit_height)) = surface_extent(width, height, max_side) else {
            return;
        };
        if let (Some(surface), Some(config)) = (self.surface.as_ref(), self.surface_config.as_mut())
        {
            self.requested_size = Some((width, height));
            config.width = fit_width;
            config.height = fit_height;
            surface.configure(&self.device, config);
        }
    }

    /// The size [`Self::resize`] was last asked for, which a clamped surface falls short of.
    pub fn requested_size(&self) -> Option<(u32, u32)> {
        self.requested_size
    }

    /// Surface pixels per requested pixel: 1, or less where the window outgrew the device's texture limit.
    pub fn surface_scale(&self) -> f32 {
        match (self.surface_config.as_ref(), self.requested_size) {
            (Some(config), Some((width, _))) if width > 0 => config.width as f32 / width as f32,
            _ => 1.0,
        }
    }

    /// Reconfigures the surface with its current size (Outdated recovery).
    pub fn reconfigure(&self) {
        if let (Some(surface), Some(config)) = (self.surface.as_ref(), self.surface_config.as_ref())
        {
            surface.configure(&self.device, config);
        }
    }

    /// The next surface texture; on `Err` the surface is repaired (or absent) and the error says when to retry.
    pub fn acquire(&mut self) -> Result<wgpu::SurfaceTexture, AcquireRetry> {
        let status = self
            .surface
            .as_ref()
            .ok_or(AcquireRetry::WhenVisible)?
            .get_current_texture();
        let (frame, suboptimal) = match status {
            wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            failed => return Err(self.repair(&failed)),
        };
        if self.failed_acquires > 0 {
            log::info!(
                "Surface usable again after {} failed acquires",
                self.failed_acquires
            );
            self.failed_acquires = 0;
        }
        self.suboptimal = suboptimal;
        Ok(frame)
    }

    /// Presents a frame from [`Self::acquire`], then reconfigures if that acquire was suboptimal.
    pub fn present(&mut self, frame: wgpu::SurfaceTexture) {
        self.queue.present(frame);
        if std::mem::take(&mut self.suboptimal) {
            self.reconfigure();
        }
    }

    fn repair(&mut self, status: &wgpu::CurrentSurfaceTexture) -> AcquireRetry {
        if self.failed_acquires == 0 {
            log::warn!("Surface acquire failed: {status:?}; skipping frames until it recovers");
        }
        self.failed_acquires = self.failed_acquires.saturating_add(1);
        let (repair, retry) = recovery(status, self.adapter_info.backend);
        match repair {
            Repair::Nothing => {}
            Repair::Reconfigure => self.reconfigure(),
            Repair::Recreate => self.recreate_surface(),
        }
        retry
    }

    /// A new surface from the same window, configured as the lost one was; the old one goes first.
    fn recreate_surface(&mut self) {
        let Some((instance, window)) = self.origin.as_ref() else {
            return self.reconfigure();
        };
        self.surface = None;
        match instance.create_surface(Arc::clone(window)) {
            Ok(surface) => {
                self.surface = Some(surface);
                self.reconfigure();
            }
            Err(err) => log::error!("Recreating the lost surface failed: {err}"),
        }
    }
}

/// Refuses an adapter on a backend this renderer cannot run, with that reason instead of a shader-translation panic.
pub fn check_adapter(info: &wgpu::AdapterInfo) -> anyhow::Result<()> {
    if BACKENDS.contains(info.backend.into()) {
        return Ok(());
    }
    anyhow::bail!(
        "adapter '{}' is on the {:?} backend, which cannot run this renderer's WGSL \
         (depth textureLoad has no GLSL translation); treating it as no usable adapter",
        info.name,
        info.backend
    )
}

/// The largest surface within `max_side` that keeps the window's aspect; `None` for a zero side.
pub fn surface_extent(width: u32, height: u32, max_side: u32) -> Option<(u32, u32)> {
    if width == 0 || height == 0 {
        return None;
    }
    let longest = width.max(height);
    if longest <= max_side {
        return Some((width, height));
    }
    log::warn!(
        "surface {width}x{height} exceeds the {max_side} px texture limit; rendering scaled"
    );
    let scale = f64::from(max_side) / f64::from(longest);
    let fit = |side: u32| ((f64::from(side) * scale).round() as u32).clamp(1, max_side);
    Some((fit(width), fit(height)))
}

/// An offscreen target needs both sides from 1 to the device's `max_texture_dimension_2d`.
pub fn check_target_size(width: u32, height: u32, max_side: u32) -> anyhow::Result<()> {
    if (1..=max_side).contains(&width) && (1..=max_side).contains(&height) {
        return Ok(());
    }
    anyhow::bail!("{width}x{height} is not a render size this device takes: each side must be 1 to {max_side}")
}

/// What repairs the surface after a failed acquire, and when the next try is worth making.
fn recovery(
    status: &wgpu::CurrentSurfaceTexture,
    backend: wgpu::Backend,
) -> (Repair, AcquireRetry) {
    use wgpu::CurrentSurfaceTexture as Status;
    match status {
        Status::Outdated => (Repair::Reconfigure, AcquireRetry::NextFrame),
        // A browser's Lost means only that the last configure failed; a native one needs a new surface.
        Status::Lost if backend == wgpu::Backend::BrowserWebGpu => {
            (Repair::Reconfigure, AcquireRetry::NextFrame)
        }
        Status::Lost => (Repair::Recreate, AcquireRetry::NextFrame),
        Status::Occluded => (Repair::Nothing, AcquireRetry::WhenVisible),
        Status::Timeout | Status::Validation | Status::Success(_) | Status::Suboptimal(_) => {
            (Repair::Nothing, AcquireRetry::NextFrame)
        }
    }
}

fn new_instance() -> wgpu::Instance {
    wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: BACKENDS,
        flags: wgpu::InstanceFlags::default(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: wgpu::BackendOptions::default(),
        display: None,
    })
}

/// An sRGB format where the surface has one, at the window's size within the device's limit.
fn surface_config(
    surface: &wgpu::Surface<'_>,
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    size: winit::dpi::PhysicalSize<u32>,
) -> anyhow::Result<wgpu::SurfaceConfiguration> {
    let caps = surface.get_capabilities(adapter);
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
    let max_side = device.limits().max_texture_dimension_2d;
    let (width, height) = surface_extent(size.width.max(1), size.height.max(1), max_side)
        .context("a surface needs a non-zero size")?;
    Ok(wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        // `Auto` is the only colour space guaranteed for every format; an HDR space needs a capability check.
        color_space: wgpu::SurfaceColorSpace::Auto,
        width,
        height,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode,
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    })
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

    // WebGPU's defaults on every target: a browser grants no more, so native tests fail where the web would.
    let required_limits = wgpu::Limits::default();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gl_adapter_is_refused_with_its_reason_and_the_primary_backends_pass() {
        let gl = wgpu::AdapterInfo::new(wgpu::DeviceType::IntegratedGpu, wgpu::Backend::Gl);
        let err = check_adapter(&gl).expect_err("GL cannot run the renderer's WGSL");
        assert!(err.to_string().contains("Gl backend"), "{err}");
        for backend in [
            wgpu::Backend::Vulkan,
            wgpu::Backend::Metal,
            wgpu::Backend::Dx12,
            wgpu::Backend::BrowserWebGpu,
        ] {
            let info = wgpu::AdapterInfo::new(wgpu::DeviceType::Cpu, backend);
            assert!(check_adapter(&info).is_ok(), "{backend:?}");
        }
        assert!(
            !BACKENDS.contains(wgpu::Backends::GL),
            "no instance offers GL"
        );
    }

    #[test]
    fn native_contexts_never_request_the_gl_backend() {
        assert!(!BACKENDS.contains(wgpu::Backends::GL), "{BACKENDS:?}");
        assert!(BACKENDS.contains(wgpu::Backends::VULKAN), "{BACKENDS:?}");
    }

    #[test]
    fn a_surface_past_the_texture_limit_scales_down_with_its_aspect() {
        assert_eq!(surface_extent(10240, 1440, 8192), Some((8192, 1152)));
        assert_eq!(surface_extent(1440, 10240, 8192), Some((1152, 8192)));
        assert_eq!(surface_extent(20000, 1, 8192), Some((8192, 1)));
        assert_eq!(surface_extent(8192, 4096, 8192), Some((8192, 4096)));
        assert_eq!(surface_extent(1920, 1080, 8192), Some((1920, 1080)));
    }

    #[test]
    fn no_failed_acquire_ends_the_loop_and_each_repairs_the_surface() {
        use wgpu::CurrentSurfaceTexture as Status;
        let native = wgpu::Backend::Vulkan;
        let retry_only = (Repair::Nothing, AcquireRetry::NextFrame);
        assert_eq!(recovery(&Status::Timeout, native), retry_only);
        assert_eq!(recovery(&Status::Validation, native), retry_only);
        assert_eq!(
            recovery(&Status::Occluded, native),
            (Repair::Nothing, AcquireRetry::WhenVisible)
        );
        let reconfigure = (Repair::Reconfigure, AcquireRetry::NextFrame);
        assert_eq!(recovery(&Status::Outdated, native), reconfigure);
        assert_eq!(
            recovery(&Status::Lost, native),
            (Repair::Recreate, AcquireRetry::NextFrame),
            "reconfiguring a lost native surface is a validation error"
        );
        assert_eq!(
            recovery(&Status::Lost, wgpu::Backend::BrowserWebGpu),
            reconfigure
        );
    }

    #[test]
    fn a_surface_over_the_texture_limit_keeps_its_aspect() {
        assert_eq!(surface_extent(0, 600, 8192), None);
        assert_eq!(surface_extent(800, 0, 8192), None);
        assert_eq!(surface_extent(1600, 900, 8192), Some((1600, 900)));
        assert_eq!(surface_extent(8192, 8192, 8192), Some((8192, 8192)));
        assert_eq!(surface_extent(10240, 1440, 8192), Some((8192, 1152)));
        assert_eq!(surface_extent(100, 20000, 8192), Some((41, 8192)));
        assert_eq!(surface_extent(u32::MAX, 1, 8192), Some((8192, 1)));
    }

    #[test]
    fn an_offscreen_size_outside_the_device_limit_is_an_error() {
        assert!(check_target_size(1280, 720, 8192).is_ok());
        assert!(check_target_size(8192, 1, 8192).is_ok());
        for (width, height) in [(0, 720), (1280, 0), (10_000, 5_000), (8193, 8)] {
            let err = check_target_size(width, height, 8192).expect_err("out of range");
            assert!(err.to_string().contains("1 to 8192"), "{err}");
        }
    }

    #[test]
    fn a_headless_context_has_nothing_to_acquire_and_ignores_resizes() {
        let Some(mut gpu) = GpuContext::headless_or_skip() else {
            return;
        };
        assert!(matches!(gpu.acquire(), Err(AcquireRetry::WhenVisible)));
        gpu.resize(20_000, 100);
        assert_eq!(gpu.requested_size(), None);
        assert_eq!(gpu.surface_scale(), 1.0);
        assert!(gpu.check_target_size(64, 64).is_ok());
        assert!(gpu.check_target_size(0, 64).is_err());
    }
}
