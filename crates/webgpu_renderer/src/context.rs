//! Instance/adapter/device/queue management plus surface configuration and recovery.
//! A failed acquire repairs the surface ([`GpuContext::acquire`]); a suboptimal one, or a VSync switch, reconfigures
//! after presenting.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Context as _;
use winit::window::Window;

use crate::adapter::{AdapterIdentity, AdapterRequest};
pub use crate::surface_recovery::AcquireRetry;
use crate::surface_recovery::{recovery, Repair};

/// The backends this renderer can run; GL is not one: naga cannot translate the depth resolve's and SSAO's
/// depth `textureLoad` to GLSL, and a GL adapter could outrank lavapipe in adapter selection.
const BACKENDS: wgpu::Backends = wgpu::Backends::PRIMARY;

pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    /// TEXTURE_COMPRESSION_BC was available and enabled on this device.
    pub supports_bc: bool,
    /// TEXTURE_COMPRESSION_ETC2 was available and enabled: the Basis target where BC is missing (mobile).
    pub supports_etc2: bool,
    /// TEXTURE_COMPRESSION_ASTC was available and enabled: the Basis target where BC and ETC2 are missing.
    pub supports_astc: bool,
    /// TIMESTAMP_QUERY was available and enabled; Chrome exposes it too, at 100 µs resolution.
    pub supports_timestamps: bool,
    /// The adapter the device was created on.
    pub adapter_info: wgpu::AdapterInfo,
    /// Its label and class; in a browser built from `GPUAdapterInfo`, since wgpu's name is empty there.
    pub identity: AdapterIdentity,
    /// Natively, every adapter this renderer could run on, in the order wgpu listed them; empty in a browser.
    pub adapters: Vec<wgpu::AdapterInfo>,
    /// Which of [`Self::adapters`] the device runs on.
    pub adapter_index: Option<usize>,
    /// An sRGB texture, and on a windowed context the surface, can be viewed as its linear twin.
    pub srgb_twin_views: bool,
    /// Present target; `None` for headless (render-to-texture) contexts.
    pub surface: Option<wgpu::Surface<'static>>,
    pub surface_config: Option<wgpu::SurfaceConfiguration>,
    /// The present modes the surface offers; a browser canvas offers Fifo alone.
    present_modes: Vec<wgpu::PresentMode>,
    /// VSync as last asked for, which a surface without an uncapped mode presents as Fifo all the same.
    vsync: bool,
    /// The instance and window the surface came from, to make it again when the platform loses it.
    origin: Option<(wgpu::Instance, Arc<Window>)>,
    /// The size last asked of the surface, before the device's texture limit scaled it down.
    requested_size: Option<(u32, u32)>,
    /// Reconfigure once the frame in flight is presented: its acquire was suboptimal, or VSync changed.
    reconfigure_after_present: bool,
    /// Failed acquires in a row, so a surface that stays unusable logs once, not every frame.
    failed_acquires: u32,
    /// Set by wgpu's device-lost callback; the app stops submitting once it is.
    lost: Arc<AtomicBool>,
}

impl GpuContext {
    /// Creates a context that presents to `window` on the adapter the environment asks for (native: blocks).
    pub fn new_windowed(window: Arc<Window>) -> anyhow::Result<Self> {
        Self::new_windowed_with(window, &AdapterRequest::from_env())
    }

    /// As [`Self::new_windowed`], on the adapter `request` names, else the best one.
    pub fn new_windowed_with(
        window: Arc<Window>,
        request: &AdapterRequest,
    ) -> anyhow::Result<Self> {
        pollster::block_on(Self::open_windowed(window, request))
    }

    /// Async variant for platforms without block_on (wasm32/browsers).
    pub async fn new_windowed_async(window: Arc<Window>) -> anyhow::Result<Self> {
        Self::open_windowed(window, &AdapterRequest::from_env()).await
    }

    async fn open_windowed(window: Arc<Window>, request: &AdapterRequest) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = new_instance(request.backends);
        let surface = instance
            .create_surface(Arc::clone(&window))
            .context("Failed to create surface")?;

        let picked = pick_adapter(&instance, Some(&surface), request).await?;
        let adapter = &picked.adapter;
        let adapter_info = adapter.get_info();
        check_adapter(&adapter_info)?;
        let identity = identify(&adapter_info).await;

        let (device, queue, features) = request_device(adapter).await?;
        let downlevel = adapter.get_downlevel_capabilities().flags;
        let srgb_twin_views = downlevel.contains(
            wgpu::DownlevelFlags::VIEW_FORMATS | wgpu::DownlevelFlags::SURFACE_VIEW_FORMATS,
        );
        let caps = surface.get_capabilities(adapter);
        let surface_config = surface_config(&caps, &device, size, srgb_twin_views)?;
        surface.configure(&device, &surface_config);

        let lost = watch_device(&device);
        Ok(Self {
            device,
            queue,
            supports_bc: features.contains(wgpu::Features::TEXTURE_COMPRESSION_BC),
            supports_etc2: features.contains(wgpu::Features::TEXTURE_COMPRESSION_ETC2),
            supports_astc: features.contains(wgpu::Features::TEXTURE_COMPRESSION_ASTC),
            supports_timestamps: features.contains(wgpu::Features::TIMESTAMP_QUERY),
            adapter_info,
            identity,
            adapters: picked.listed,
            adapter_index: picked.index,
            srgb_twin_views,
            surface: Some(surface),
            surface_config: Some(surface_config),
            present_modes: caps.present_modes,
            vsync: true,
            origin: Some((instance, window)),
            requested_size: Some((size.width.max(1), size.height.max(1))),
            reconfigure_after_present: false,
            failed_acquires: 0,
            lost,
        })
    }

    /// Creates a surfaceless context for render-to-texture (tests, CI).
    pub fn new_headless() -> anyhow::Result<Self> {
        pollster::block_on(Self::new_headless_async(false))
    }

    /// `for_tests` also refuses an adapter the tests cannot trust ([`check_test_adapter`]) before it makes a device.
    async fn new_headless_async(for_tests: bool) -> anyhow::Result<Self> {
        let request = AdapterRequest::from_env();
        let picked = pick_adapter(&new_instance(request.backends), None, &request)
            .await
            .context("No GPU adapter found (headless)")?;
        let adapter = picked.adapter;
        let adapter_info = adapter.get_info();
        check_adapter(&adapter_info)?;
        if for_tests {
            check_test_adapter(&adapter_info, std::env::consts::ARCH)?;
        }

        let srgb_twin_views = adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::VIEW_FORMATS);
        let (device, queue, features) = request_device(&adapter).await?;
        let lost = watch_device(&device);
        Ok(Self {
            device,
            queue,
            supports_bc: features.contains(wgpu::Features::TEXTURE_COMPRESSION_BC),
            supports_etc2: features.contains(wgpu::Features::TEXTURE_COMPRESSION_ETC2),
            supports_astc: features.contains(wgpu::Features::TEXTURE_COMPRESSION_ASTC),
            supports_timestamps: features.contains(wgpu::Features::TIMESTAMP_QUERY),
            identity: AdapterIdentity::of(&adapter_info, None),
            adapter_info,
            adapters: picked.listed,
            adapter_index: picked.index,
            srgb_twin_views,
            surface: None,
            surface_config: None,
            present_modes: Vec::new(),
            vsync: true,
            origin: None,
            requested_size: None,
            reconfigure_after_present: false,
            failed_acquires: 0,
            lost,
        })
    }

    /// [`Self::new_headless`] for tests: `None` after a SKIP line, or a panic if `KATAGLYPHIS_REQUIRE_GPU` is set.
    /// See <https://github.com/Kataglyphis/BeschleunigerBallett/blob/develop/docs/source/gpu-golden-testing.md>.
    #[doc(hidden)]
    pub fn headless_or_skip() -> Option<Self> {
        static REPORTED: std::sync::Once = std::sync::Once::new();
        match pollster::block_on(Self::new_headless_async(true)) {
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
            "ADAPTER: {} ({:?}, {:?}, driver '{}' '{}') bc={} etc2={} astc={} timestamps={}\n",
            info.name,
            info.backend,
            info.device_type,
            info.driver,
            info.driver_info,
            self.supports_bc,
            self.supports_etc2,
            self.supports_astc,
            self.supports_timestamps
        );
        // A direct write: libtest captures eprintln!, and a crash in the first render must not hide the adapter.
        let _ = std::io::Write::write_all(&mut std::io::stderr(), line.as_bytes());
    }

    pub fn surface_format(&self) -> Option<wgpu::TextureFormat> {
        self.surface_config.as_ref().map(|c| c.format)
    }

    /// The format egui paints `output` through: its linear twin where one can be viewed, as the browser's canvas is.
    pub fn ui_format(&self, output: wgpu::TextureFormat) -> wgpu::TextureFormat {
        if self.srgb_twin_views {
            output.remove_srgb_suffix()
        } else {
            output
        }
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

    /// VSync as last asked for: on unless [`Self::set_vsync`] turned it off.
    pub fn vsync(&self) -> bool {
        self.vsync
    }

    /// Picks the surface's present mode by [`present_mode`]; the surface takes it once the frame in flight is presented.
    pub fn set_vsync(&mut self, on: bool) {
        if on == self.vsync {
            return;
        }
        self.vsync = on;
        let Some(config) = self.surface_config.as_mut() else {
            return;
        };
        config.present_mode = present_mode(on, &self.present_modes);
        log::info!(
            "VSync {}: presenting {:?}",
            if on { "on" } else { "off" },
            config.present_mode
        );
        // Configuring a surface whose texture is still held fails, so the switch waits for the present.
        self.reconfigure_after_present = true;
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
        self.reconfigure_after_present |= suboptimal;
        Ok(frame)
    }

    /// Presents a frame from [`Self::acquire`], then reconfigures if that acquire was suboptimal or VSync changed.
    pub fn present(&mut self, frame: wgpu::SurfaceTexture) {
        self.queue.present(frame);
        if std::mem::take(&mut self.reconfigure_after_present) {
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

    /// A new surface from the same window, configured as the lost one was.
    fn recreate_surface(&mut self) {
        let (Some((instance, window)), Some(config)) =
            (self.origin.as_ref(), self.surface_config.as_ref())
        else {
            return self.reconfigure();
        };
        crate::surface_recovery::recreate_surface(
            &mut self.surface,
            instance,
            window,
            &self.device,
            config,
        );
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

/// Refuses llvmpipe wider than 128 bits on aarch64, where LLVM's AArch64 backend corrupts the heap (docs/testing.md).
#[doc(hidden)]
pub fn check_test_adapter(info: &wgpu::AdapterInfo, arch: &str) -> anyhow::Result<()> {
    let bits = info
        .name
        .strip_prefix("llvmpipe (")
        .and_then(|rest| rest.split_once(" bits)"))
        .and_then(|(head, _)| head.rsplit(' ').next())
        .and_then(|bits| bits.parse::<u32>().ok());
    match bits {
        Some(bits) if arch == "aarch64" && bits > 128 => anyhow::bail!(
            "'{}' runs {bits}-bit vectors on aarch64, where LLVM's AArch64 backend corrupts the heap \
             and crashes the test binary now and then; set LP_NATIVE_VECTOR_WIDTH=128",
            info.name
        ),
        _ => Ok(()),
    }
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

/// Release builds skip wgpu's indirect-call validation: the draw cull writes in-range
/// arguments, and the per-draw check cost up to 5 ms a frame. `WGPU_VALIDATION_INDIRECT_CALL=1` restores it.
pub fn instance_flags() -> wgpu::InstanceFlags {
    let mut flags = wgpu::InstanceFlags::default();
    if !cfg!(debug_assertions) {
        flags.remove(wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL);
    }
    flags.with_env()
}

/// An instance over `backends` within [`BACKENDS`], which never include GL.
fn new_instance(backends: wgpu::Backends) -> wgpu::Instance {
    let narrowed = backends & BACKENDS;
    wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: if narrowed.is_empty() {
            BACKENDS
        } else {
            narrowed
        },
        flags: instance_flags(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: wgpu::BackendOptions::default(),
        display: None,
    })
}

/// VSync on is Fifo, which every surface offers; off prefers Mailbox, which does not tear, then Immediate, else Fifo.
pub fn present_mode(vsync: bool, offered: &[wgpu::PresentMode]) -> wgpu::PresentMode {
    use wgpu::PresentMode::{Fifo, Immediate, Mailbox};
    if vsync {
        return Fifo;
    }
    [Mailbox, Immediate]
        .into_iter()
        .find(|mode| offered.contains(mode))
        .unwrap_or(Fifo)
}

/// An sRGB format where the surface has one, at the window's size within the device's limit; `twin_view` adds its linear view.
fn surface_config(
    caps: &wgpu::SurfaceCapabilities,
    device: &wgpu::Device,
    size: winit::dpi::PhysicalSize<u32>,
    twin_view: bool,
) -> anyhow::Result<wgpu::SurfaceConfiguration> {
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
        present_mode: present_mode(true, &caps.present_modes),
        alpha_mode,
        // egui blends in sRGB through this view, as on the browser's non-sRGB canvas.
        view_formats: if twin_view && format.is_srgb() {
            vec![format.remove_srgb_suffix()]
        } else {
            vec![]
        },
        desired_maximum_frame_latency: 2,
    })
}

/// The adapter a context opens, and natively the list it was chosen from.
struct PickedAdapter {
    adapter: wgpu::Adapter,
    listed: Vec<wgpu::AdapterInfo>,
    index: Option<usize>,
}

/// Every adapter logged, then `request`'s or the best usable one ([`crate::adapter::choose`]).
#[cfg(not(target_arch = "wasm32"))]
async fn pick_adapter(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
    request: &AdapterRequest,
) -> anyhow::Result<PickedAdapter> {
    let mut adapters = instance.enumerate_adapters(request.backends).await;
    let infos: Vec<wgpu::AdapterInfo> = adapters.iter().map(wgpu::Adapter::get_info).collect();
    let usable: Vec<bool> = adapters
        .iter()
        .zip(&infos)
        .map(|(adapter, info)| {
            check_adapter(info).is_ok() && surface.is_none_or(|s| adapter.is_surface_supported(s))
        })
        .collect();
    for (index, (info, usable)) in infos.iter().zip(&usable).enumerate() {
        let note = if *usable { "" } else { ", unusable" };
        log::info!(
            "Adapter {index}: {} [{:?}]{note}",
            crate::adapter::entry_label(info),
            info.device_type
        );
    }
    let choice = crate::adapter::choose(&infos, &usable, request)
        .context("No suitable GPU adapter found")?;
    if choice.unmatched_request {
        log::warn!(
            "No usable adapter matches '{}'; choosing automatically",
            request.name.as_deref().unwrap_or_default()
        );
    }
    log::info!(
        "Using adapter {}: {}",
        choice.index,
        crate::adapter::entry_label(&infos[choice.index])
    );
    let listed = (0..infos.len()).filter(|i| usable[*i]).collect::<Vec<_>>();
    let index = listed.iter().position(|i| *i == choice.index);
    Ok(PickedAdapter {
        adapter: adapters.swap_remove(choice.index),
        listed: listed.into_iter().map(|i| infos[i].clone()).collect(),
        index,
    })
}

/// The browser offers one adapter; its power preference is a request Chrome ignores on Windows.
#[cfg(target_arch = "wasm32")]
async fn pick_adapter(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
    _request: &AdapterRequest,
) -> anyhow::Result<PickedAdapter> {
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: surface,
            force_fallback_adapter: false,
            // Bucketing is anti-fingerprinting for untrusted content; a trusted app wants the real limits.
            apply_limit_buckets: false,
        })
        .await
        .context("No suitable GPU adapter found")?;
    Ok(PickedAdapter {
        adapter,
        listed: Vec::new(),
        index: None,
    })
}

/// The adapter's label and class; in a browser from `GPUAdapterInfo`'s vendor and architecture.
async fn identify(info: &wgpu::AdapterInfo) -> AdapterIdentity {
    #[cfg(target_arch = "wasm32")]
    let web = crate::app::web::adapter_info().await;
    #[cfg(not(target_arch = "wasm32"))]
    let web = None;
    AdapterIdentity::of(info, web.as_ref())
}

/// Whether wgpu reported this device lost (a reset, a removed card, `destroy`).
impl GpuContext {
    pub fn is_lost(&self) -> bool {
        self.lost.load(Ordering::SeqCst)
    }
}

/// A flag the device-lost callback sets; natively, errors after a loss are logged instead of wgpu's panic, an abort here.
fn watch_device(device: &wgpu::Device) -> Arc<AtomicBool> {
    let lost = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&lost);
    device.set_device_lost_callback(move |reason, message| {
        log::error!("The GPU device was lost ({reason:?}): {message}");
        flag.store(true, Ordering::SeqCst);
    });
    #[cfg(not(target_arch = "wasm32"))]
    {
        let flag = Arc::clone(&lost);
        device.on_uncaptured_error(Arc::new(move |error| {
            if flag.load(Ordering::SeqCst) {
                log::warn!("wgpu error after the device loss: {error}");
            } else {
                // wgpu's own default: any other uncaptured error stays fatal, so a validation bug still fails a test.
                panic!("wgpu error: {error}\n");
            }
        }));
    }
    lost
}

async fn request_device(
    adapter: &wgpu::Adapter,
) -> anyhow::Result<(wgpu::Device, wgpu::Queue, wgpu::Features)> {
    // Request only features the adapter reports: naming a missing one fails the whole device request.
    let required_features = adapter.features()
        & (wgpu::Features::TEXTURE_COMPRESSION_BC
            | wgpu::Features::TEXTURE_COMPRESSION_ETC2
            | wgpu::Features::TEXTURE_COMPRESSION_ASTC
            | wgpu::Features::TIMESTAMP_QUERY);

    // WebGPU's defaults, so native tests fail where the web would; natively the tracer's buffer sizes come from the adapter.
    let required_limits = crate::render::tracer::limits::device_limits(
        &adapter.limits(),
        cfg!(target_arch = "wasm32"),
    );

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
        .map(|(device, queue)| (device, queue, required_features))
        .context("Failed to create device")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indirect_call_validation_follows_the_build_type() {
        if std::env::var_os("WGPU_VALIDATION_INDIRECT_CALL").is_some() {
            return;
        }
        let validated = instance_flags().contains(wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL);
        assert_eq!(validated, cfg!(debug_assertions));
    }

    fn llvmpipe(bits: u32) -> wgpu::AdapterInfo {
        let mut info = wgpu::AdapterInfo::new(wgpu::DeviceType::Cpu, wgpu::Backend::Vulkan);
        info.name = format!("llvmpipe (LLVM 21.1.8, {bits} bits)");
        info
    }

    #[test]
    fn llvmpipe_wider_than_neon_is_refused_on_aarch64_only() {
        let err =
            check_test_adapter(&llvmpipe(256), "aarch64").expect_err("256 bits crash on aarch64");
        assert!(
            err.to_string().contains("LP_NATIVE_VECTOR_WIDTH=128"),
            "{err}"
        );
        check_test_adapter(&llvmpipe(128), "aarch64").expect("128 bits are NEON's own width");
        check_test_adapter(&llvmpipe(256), "x86_64").expect("256 bits are AVX2's own width");
        check_test_adapter(&llvmpipe(512), "x86_64").expect("only aarch64 is refused");
        let mut gpu = wgpu::AdapterInfo::new(wgpu::DeviceType::DiscreteGpu, wgpu::Backend::Vulkan);
        gpu.name = "AMD Radeon RX 9070 XT (RADV GFX1201)".to_owned();
        check_test_adapter(&gpu, "aarch64").expect("a hardware adapter has no vector width");
    }

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
    fn vsync_presents_fifo_and_off_prefers_mailbox_then_immediate() {
        use wgpu::PresentMode::{AutoNoVsync, Fifo, FifoRelaxed, Immediate, Mailbox};
        let every = [Fifo, FifoRelaxed, Immediate, Mailbox];
        assert_eq!(present_mode(true, &every), Fifo);
        assert_eq!(present_mode(false, &every), Mailbox);
        assert_eq!(present_mode(false, &[Fifo, Immediate]), Immediate);
        // A browser canvas offers Fifo alone, so it presents in step with the display either way.
        assert_eq!(present_mode(false, &[Fifo]), Fifo);
        assert_eq!(present_mode(false, &[]), Fifo);
        assert_eq!(present_mode(false, &[FifoRelaxed, AutoNoVsync]), Fifo);
    }

    #[test]
    fn a_headless_context_keeps_the_vsync_it_was_asked_for() {
        let Some(mut gpu) = GpuContext::headless_or_skip() else {
            return;
        };
        assert!(gpu.vsync(), "VSync starts on");
        gpu.set_vsync(false);
        assert!(!gpu.vsync());
        assert!(!gpu.reconfigure_after_present, "no surface to reconfigure");
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
