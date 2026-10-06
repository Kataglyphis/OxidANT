//! A Windows install is one folder, so the exe points GStreamer, ONNX Runtime and the model at it (Linux: the `catcam` launcher).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The variables an install folder `dir` implies, minus any `is_set` already holds; empty when `dir` is no install.
pub fn plan(
    dir: &Path,
    local_app_data: Option<&Path>,
    is_set: impl Fn(&str) -> bool,
) -> Vec<(&'static str, OsString)> {
    let plugins = dir.join("lib").join("gstreamer-1.0");
    if !plugins.is_dir() {
        return Vec::new();
    }
    let mut wanted: Vec<(&'static str, OsString)> = vec![
        ("GST_PLUGIN_PATH", plugins.into_os_string()),
        // Set but empty: GStreamer then loads no plugins from the host, which carries another build.
        ("GST_PLUGIN_SYSTEM_PATH_1_0", OsString::new()),
        ("GST_PLUGIN_SYSTEM_PATH", OsString::new()),
    ];
    let scanner = dir
        .join("libexec")
        .join("gstreamer-1.0")
        .join("gst-plugin-scanner.exe");
    if scanner.is_file() {
        wanted.push(("GST_PLUGIN_SCANNER", scanner.into_os_string()));
    }
    if let Some(base) = local_app_data {
        let registry: PathBuf = base.join("omni-accelerant-catcam").join("gst-registry.bin");
        wanted.push(("GST_REGISTRY", registry.into_os_string()));
    }
    let ort = dir.join("onnxruntime.dll");
    if ort.is_file() {
        wanted.push(("ORT_DYLIB_PATH", ort.into_os_string()));
    }
    let model = dir.join("models").join("yolo26n.onnx");
    if model.is_file() {
        wanted.push(("KATAGLYPHIS_ONNX_MODEL", model.into_os_string()));
    }
    wanted.retain(|(name, _)| !is_set(name));
    wanted
}

/// Applies [`plan`] for the running exe's folder (Windows only); call it first in `main`, before any thread or GStreamer.
pub fn adopt() {
    if !cfg!(windows) {
        return;
    }
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    else {
        return;
    };
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    for (name, value) in plan(&dir, local.as_deref(), |n| std::env::var_os(n).is_some()) {
        if name == "GST_REGISTRY" {
            if let Some(parent) = Path::new(&value).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
        std::env::set_var(name, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install(dir: &Path, with_ort_and_model: bool) {
        std::fs::create_dir_all(dir.join("lib").join("gstreamer-1.0")).unwrap();
        std::fs::create_dir_all(dir.join("libexec").join("gstreamer-1.0")).unwrap();
        std::fs::write(
            dir.join("libexec")
                .join("gstreamer-1.0")
                .join("gst-plugin-scanner.exe"),
            b"MZ",
        )
        .unwrap();
        if with_ort_and_model {
            std::fs::write(dir.join("onnxruntime.dll"), b"MZ").unwrap();
            std::fs::create_dir_all(dir.join("models")).unwrap();
            std::fs::write(dir.join("models").join("yolo26n.onnx"), b"onnx").unwrap();
        }
    }

    fn names(plan: &[(&'static str, OsString)]) -> Vec<&'static str> {
        plan.iter().map(|(n, _)| *n).collect()
    }

    #[test]
    fn a_folder_without_plugins_is_no_install() {
        let dir = tempfile::tempdir().unwrap();
        assert!(plan(dir.path(), None, |_| false).is_empty());
    }

    #[test]
    fn an_install_points_everything_at_itself() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), true);
        let local = dir.path().join("local");
        let p = plan(dir.path(), Some(&local), |_| false);
        assert_eq!(
            names(&p),
            [
                "GST_PLUGIN_PATH",
                "GST_PLUGIN_SYSTEM_PATH_1_0",
                "GST_PLUGIN_SYSTEM_PATH",
                "GST_PLUGIN_SCANNER",
                "GST_REGISTRY",
                "ORT_DYLIB_PATH",
                "KATAGLYPHIS_ONNX_MODEL"
            ]
        );
        let value = |name: &str| p.iter().find(|(n, _)| *n == name).unwrap().1.clone();
        assert_eq!(value("GST_PLUGIN_SYSTEM_PATH_1_0"), OsString::new());
        assert_eq!(
            PathBuf::from(value("ORT_DYLIB_PATH")),
            dir.path().join("onnxruntime.dll")
        );
        assert!(PathBuf::from(value("GST_REGISTRY")).starts_with(&local));
    }

    #[test]
    fn what_the_caller_set_wins_and_absent_files_are_not_named() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), false);
        let p = plan(dir.path(), None, |n| n == "GST_PLUGIN_PATH");
        assert_eq!(
            names(&p),
            [
                "GST_PLUGIN_SYSTEM_PATH_1_0",
                "GST_PLUGIN_SYSTEM_PATH",
                "GST_PLUGIN_SCANNER"
            ]
        );
    }
}
