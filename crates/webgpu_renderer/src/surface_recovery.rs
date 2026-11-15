//! What a failed surface acquire asks of the surface; `crates/gui` includes this file by path, so it uses wgpu and winit only.

use std::sync::Arc;

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
pub enum Repair {
    Nothing,
    Reconfigure,
    Recreate,
}

/// What repairs the surface after a failed acquire, and when the next try is worth making.
pub fn recovery(
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

/// A new surface from `window` in `slot`, configured as the lost one was; the old one goes first.
pub fn recreate_surface(
    slot: &mut Option<wgpu::Surface<'static>>,
    instance: &wgpu::Instance,
    window: &Arc<winit::window::Window>,
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) {
    *slot = None;
    match instance.create_surface(Arc::clone(window)) {
        Ok(surface) => {
            surface.configure(device, config);
            *slot = Some(surface);
        }
        Err(err) => log::error!("Recreating the lost surface failed: {err}"),
    }
}
