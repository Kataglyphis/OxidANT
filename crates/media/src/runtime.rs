//! One-time GStreamer runtime initialization.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static GST_INIT: OnceLock<Result<(), String>> = OnceLock::new();

/// Initializes GStreamer exactly once, resolving the plugin directory first.
/// The only supported entry point: the plugin variables must be set before `gst::init()`.
pub fn ensure_gst_initialized() -> anyhow::Result<()> {
    let result = GST_INIT.get_or_init(|| {
        if std::env::var_os("GST_PLUGIN_PATH").is_none() {
            // Runs before any GStreamer thread exists; frb init calls this before others race.
            if let Some(dir) = bundled_plugin_dir() {
                log::info!(
                    "GST_PLUGIN_PATH not set, using the bundled {}",
                    dir.display()
                );
                for (name, value) in bundled_env(&dir, cache_root().as_deref()) {
                    if std::env::var_os(name).is_none() {
                        std::env::set_var(name, value);
                    }
                }
            } else if let Some(dir) = image_plugin_dir() {
                log::info!("GST_PLUGIN_PATH not set, using {}", dir.display());
                std::env::set_var("GST_PLUGIN_PATH", &dir);
            }
        }
        gstreamer::init().map_err(|e| e.to_string())
    });
    result
        .clone()
        .map_err(|e| anyhow::anyhow!("GStreamer init failed: {e}"))
}

/// The container/dev-image prefix, the fallback when nothing is bundled.
fn image_plugin_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let image = PathBuf::from(r"C:\runtime\lib\gstreamer-1.0");
    #[cfg(not(windows))]
    let image = PathBuf::from("/usr/lib/gstreamer-1.0");

    image.is_dir().then_some(image)
}

/// The plugin dir a packaged app carries beside its exe: `gstreamer-1.0` or `lib/gstreamer-1.0`.
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

/// What makes GStreamer load the package's plugins and nothing else (ANTfrastructure's linux-build-basics.md).
fn bundled_env(plugin_dir: &Path, cache_root: Option<&Path>) -> Vec<(&'static str, OsString)> {
    let mut env = vec![
        ("GST_PLUGIN_PATH", plugin_dir.as_os_str().to_owned()),
        // Unset, GStreamer adds the host's plugin directories, which are built against another GStreamer.
        ("GST_PLUGIN_SYSTEM_PATH", OsString::new()),
        ("GST_PLUGIN_SYSTEM_PATH_1_0", OsString::new()),
        // No gst-plugin-scanner travels, so the scan runs in-process.
        ("GST_REGISTRY_FORK", OsString::from("no")),
    ];
    // One registry per installed copy: a copy never reads what another GStreamer wrote.
    if let Some(root) = cache_root {
        let name = format!("gstreamer-{:016x}.bin", fnv1a(plugin_dir));
        env.push((
            "GST_REGISTRY",
            root.join("kataglyphis").join(name).into_os_string(),
        ));
    }
    env
}

/// The user's cache directory: `%LOCALAPPDATA%`, else `$XDG_CACHE_HOME`, else `~/.cache`.
fn cache_root() -> Option<PathBuf> {
    let non_empty = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty());
    if cfg!(windows) {
        return non_empty("LOCALAPPDATA").map(PathBuf::from);
    }
    non_empty("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| non_empty("HOME").map(|home| PathBuf::from(home).join(".cache")))
}

/// FNV-1a over the path's bytes: stable across runs and builds, unlike `DefaultHasher`.
fn fnv1a(path: &Path) -> u64 {
    path.as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value<'a>(env: &'a [(&str, OsString)], name: &str) -> Option<&'a OsString> {
        env.iter().find(|(n, _)| *n == name).map(|(_, v)| v)
    }

    #[test]
    fn a_bundle_loads_its_own_plugins_and_no_host_ones() {
        let dir = Path::new("/opt/oxidant/lib/gstreamer-1.0");
        let env = bundled_env(dir, Some(Path::new("/home/u/.cache")));
        assert_eq!(
            value(&env, "GST_PLUGIN_PATH"),
            Some(&dir.as_os_str().to_owned())
        );
        assert_eq!(
            value(&env, "GST_PLUGIN_SYSTEM_PATH"),
            Some(&OsString::new())
        );
        assert_eq!(
            value(&env, "GST_PLUGIN_SYSTEM_PATH_1_0"),
            Some(&OsString::new())
        );
        assert_eq!(
            value(&env, "GST_REGISTRY_FORK"),
            Some(&OsString::from("no"))
        );
    }

    #[test]
    fn each_installed_copy_keeps_its_own_registry() {
        let cache = Path::new("/home/u/.cache");
        let registry = |dir: &str| {
            value(&bundled_env(Path::new(dir), Some(cache)), "GST_REGISTRY")
                .cloned()
                .map(PathBuf::from)
                .unwrap()
        };
        let a = registry("/opt/oxidant/lib/gstreamer-1.0");
        assert!(a.starts_with(cache.join("kataglyphis")), "{a:?}");
        assert_eq!(a, registry("/opt/oxidant/lib/gstreamer-1.0"));
        assert_ne!(a, registry("/tmp/oxidant/lib/gstreamer-1.0"));
    }

    #[test]
    fn without_a_cache_dir_gstreamer_keeps_its_default_registry() {
        let env = bundled_env(Path::new("/b/lib/gstreamer-1.0"), None);
        assert_eq!(value(&env, "GST_REGISTRY"), None);
        assert_eq!(env.len(), 4);
    }

    #[test]
    fn the_registry_hash_is_fnv1a() {
        // The published FNV-1a 64 vectors: the empty input and "a".
        assert_eq!(fnv1a(Path::new("")), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(Path::new("a")), 0xaf63_dc4c_8601_ec8c);
    }
}
