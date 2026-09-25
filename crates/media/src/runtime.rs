//! One-time GStreamer runtime initialization.

use std::path::PathBuf;
use std::sync::OnceLock;

static GST_INIT: OnceLock<Result<(), String>> = OnceLock::new();

/// Initializes GStreamer exactly once, resolving the plugin directory first.
///
/// `GST_PLUGIN_PATH` must be set before `gst::init()` reads the registry, so
/// this is the only supported entry point; all capture/device APIs call it.
pub fn ensure_gst_initialized() -> anyhow::Result<()> {
    let result = GST_INIT.get_or_init(|| {
        if std::env::var_os("GST_PLUGIN_PATH").is_none() {
            if let Some(dir) = find_plugin_dir() {
                log::info!("GST_PLUGIN_PATH not set, using {}", dir.display());
                // Runs before any GStreamer threads exist; the frb init path
                // invokes this before other API calls can race it.
                std::env::set_var("GST_PLUGIN_PATH", &dir);
            }
        }
        gstreamer::init().map_err(|e| e.to_string())
    });
    result
        .clone()
        .map_err(|e| anyhow::anyhow!("GStreamer init failed: {e}"))
}

/// Plugin dir search order: next to the executable (packaged app layouts),
/// then the container/dev-image install prefix.
fn find_plugin_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let image = PathBuf::from(r"C:\runtime\lib\gstreamer-1.0");
    #[cfg(not(windows))]
    let image = PathBuf::from("/usr/lib/gstreamer-1.0");

    bundled_plugin_dir().or_else(|| image.is_dir().then_some(image))
}

/// The plugin directory a packaged app carries beside its executable, when there is
/// one: `gstreamer-1.0` (OmniAccelerANT's runner) or `lib/gstreamer-1.0` (OxidANT's
/// Windows packages, where GStreamer also looks unasked while its DLL sits beside the exe).
pub fn bundled_plugin_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [
        dir.join("gstreamer-1.0"),
        dir.join("lib").join("gstreamer-1.0"),
    ]
    .into_iter()
    .find(|p| p.is_dir())
}
