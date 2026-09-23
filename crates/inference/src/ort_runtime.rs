//! Which ONNX Runtime dylib `ort` loads - chosen here, once, before the first
//! `ort` call.
//!
//! Every `onnxruntime*` feature is `load-dynamic` (see `Cargo.toml`), and the
//! family's owner rule (2026-09-23) allows only the chain-built ORT of the
//! ANTfrastructure images. So the search is closed: an explicit
//! `ORT_DYLIB_PATH`, the executable's own directory (where packaging stages the
//! chain copy), then the images' chain prefix. There is deliberately no
//! bare-name fallback: `LoadLibraryExW` searches System32 before `PATH` and
//! finds Windows ML's in-box `onnxruntime.dll` there. The path handed to `ort`
//! is always absolute: `ort` passes a relative one straight to the OS loader.
//!
//! The file found is then refused unless it embeds the chain's ORT source path
//! (ORT compiles it in through `__FILE__`), whoever named it. What this does NOT
//! prove: the version - a stale chain build passes.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{anyhow, bail, Context, Result};

#[cfg(windows)]
const DYLIB_NAMES: &[&str] = &["onnxruntime.dll"];
#[cfg(not(windows))]
const DYLIB_NAMES: &[&str] = &["libonnxruntime.so", "libonnxruntime.so.1"];

/// The chain prefix baked into the family images when no variable names it.
#[cfg(windows)]
const IMAGE_CHAIN_DIR: &str = r"C:\runtime\lib\onnxruntime-source\bin";
#[cfg(not(windows))]
const IMAGE_CHAIN_DIR: &str = "/usr/local/lib/onnxruntime-cpu/lib";

/// The chain's ORT checkout as ORT embeds it: the hub's Build-OnnxFromSource.ps1
/// `SourceDir` (Windows) and onnxruntime/build/lib/common.sh `ORT_SRC_DIR` (Linux).
#[cfg(windows)]
const CHAIN_SOURCE_MARKER: Option<&str> = Some(r"C:\temp\onnx-src\onnxruntime\core\");
#[cfg(target_os = "linux")]
const CHAIN_SOURCE_MARKER: Option<&str> = Some("/opt/onnxruntime/onnxruntime/core/");
/// The family builds ORT for no other target, so nothing may be loaded there.
#[cfg(not(any(windows, target_os = "linux")))]
const CHAIN_SOURCE_MARKER: Option<&str> = None;

/// True when `haystack` contains `needle`, ignoring ASCII case (Windows paths).
pub fn embeds_marker(haystack: &[u8], needle: &str) -> bool {
    let needle = needle.as_bytes();
    let Some(first) = needle.first() else {
        return true;
    };
    haystack
        .windows(needle.len())
        .any(|w| w[0].eq_ignore_ascii_case(first) && w.eq_ignore_ascii_case(needle))
}

/// Refuses `path` unless it was compiled from the chain's ORT checkout.
pub fn verify_chain_build(path: &Path) -> Result<()> {
    let Some(marker) = CHAIN_SOURCE_MARKER else {
        bail!(
            "no chain-built ONNX Runtime exists for this target; refusing {}",
            path.display()
        );
    };
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    if !embeds_marker(&bytes, marker) {
        bail!(
            "{} is not the family's chain-built ONNX Runtime: it does not embed the chain source path '{marker}'. A downloaded, pip, distro or OS copy is refused.",
            path.display()
        );
    }
    Ok(())
}

/// Everything the search reads from the process, gathered so the ordering can
/// be tested without touching the real environment.
#[derive(Debug, Default, Clone)]
pub struct DylibSearch {
    /// `ORT_DYLIB_PATH`, when set and non-empty.
    pub explicit: Option<OsString>,
    /// Directory of the running executable.
    pub exe_dir: Option<PathBuf>,
    /// Chain directories named by the environment, highest priority first.
    pub chain_dirs: Vec<PathBuf>,
}

impl DylibSearch {
    /// Reads `ORT_DYLIB_PATH`, the executable directory and the chain
    /// variables (`ONNX_ROOT` on Windows, `ORT_LIB_LOCATION` elsewhere).
    pub fn from_process() -> Self {
        let explicit = std::env::var_os("ORT_DYLIB_PATH").filter(|v| !v.is_empty());
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        let mut chain_dirs = Vec::new();
        #[cfg(windows)]
        if let Some(root) = std::env::var_os("ONNX_ROOT").filter(|v| !v.is_empty()) {
            chain_dirs.push(PathBuf::from(root).join("bin"));
        }
        #[cfg(not(windows))]
        if let Some(dir) = std::env::var_os("ORT_LIB_LOCATION").filter(|v| !v.is_empty()) {
            chain_dirs.push(PathBuf::from(dir));
        }
        chain_dirs.push(PathBuf::from(IMAGE_CHAIN_DIR));
        Self {
            explicit,
            exe_dir,
            chain_dirs,
        }
    }

    /// Candidate files in priority order. An explicit path is the ONLY
    /// candidate when set, so a typo fails instead of silently loading
    /// something else.
    pub fn candidates(&self) -> Vec<PathBuf> {
        if let Some(explicit) = &self.explicit {
            let path = PathBuf::from(explicit);
            // `has_root` too: on Windows "/x" is not absolute, yet not exe-relative.
            if !path.is_absolute() && !path.has_root() {
                if let Some(dir) = &self.exe_dir {
                    return vec![dir.join(&path), path];
                }
            }
            return vec![path];
        }
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(dir) = &self.exe_dir {
            dirs.push(dir.clone());
            // The Flutter Linux bundle keeps its libraries in <bundle>/lib.
            #[cfg(not(windows))]
            dirs.push(dir.join("lib"));
        }
        dirs.extend(self.chain_dirs.iter().cloned());
        dirs.iter()
            .flat_map(|dir| DYLIB_NAMES.iter().map(move |name| dir.join(name)))
            .collect()
    }

    /// The first candidate that exists, made absolute, or an error naming
    /// every path tried.
    pub fn resolve(&self, exists: impl Fn(&Path) -> bool) -> Result<PathBuf> {
        let candidates = self.candidates();
        if let Some(found) = candidates.iter().find(|p| exists(p)) {
            // A relative path reaches LoadLibraryExW/dlopen verbatim, whose search
            // is not the working-directory file `exists` just checked.
            return std::path::absolute(found)
                .with_context(|| format!("cannot make {} absolute", found.display()));
        }
        let tried: Vec<String> = candidates.iter().map(|p| p.display().to_string()).collect();
        if self.explicit.is_some() {
            bail!(
                "ORT_DYLIB_PATH names no file (tried: {}). Point it at the family's chain-built ONNX Runtime.",
                tried.join(", ")
            );
        }
        bail!(
            "No chain-built ONNX Runtime found (tried: {}). Stage it next to the executable or set ORT_DYLIB_PATH; a bare-name load is refused because it would reach the OS copy in System32.",
            tried.join(", ")
        )
    }
}

static LOADED: OnceLock<std::result::Result<PathBuf, String>> = OnceLock::new();

/// Loads the chain ONNX Runtime exactly once and returns its path. Call it
/// before any other `ort` API; every session constructor in this workspace
/// does.
pub fn ensure_ort_loaded() -> Result<PathBuf> {
    LOADED
        .get_or_init(|| {
            let path = DylibSearch::from_process()
                .resolve(Path::is_file)
                .and_then(|path| verify_chain_build(&path).map(|()| path))
                .map_err(|e| format!("{e:#}"))?;
            let builder = ort::init_from(&path)
                .map_err(|e| format!("failed to load {}: {e}", path.display()))?;
            if !builder.commit() {
                log::debug!(
                    "an ORT environment already existed; the dylib is still {}",
                    path.display()
                );
            }
            log::info!("ONNX Runtime loaded from {}", path.display());
            Ok(path)
        })
        .clone()
        .map_err(|e| anyhow!(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search(explicit: Option<&str>, exe_dir: Option<&str>, chain: &[&str]) -> DylibSearch {
        DylibSearch {
            explicit: explicit.map(OsString::from),
            exe_dir: exe_dir.map(PathBuf::from),
            chain_dirs: chain.iter().map(PathBuf::from).collect(),
        }
    }

    #[test]
    fn exe_dir_wins_over_the_chain_prefix() {
        let s = search(None, Some("/app"), &["/chain"]);
        let first = s.candidates().into_iter().next().unwrap();
        assert!(first.starts_with("/app"), "{first:?}");
    }

    #[test]
    fn never_offers_a_bare_name() {
        let s = search(None, Some("/app"), &["/chain"]);
        for candidate in s.candidates() {
            assert!(
                candidate
                    .parent()
                    .is_some_and(|p| !p.as_os_str().is_empty()),
                "{candidate:?}"
            );
        }
    }

    #[test]
    fn an_explicit_path_is_the_only_candidate() {
        let s = search(
            Some("/opt/ort/libonnxruntime.so"),
            Some("/app"),
            &["/chain"],
        );
        assert_eq!(
            s.candidates(),
            vec![PathBuf::from("/opt/ort/libonnxruntime.so")]
        );
    }

    #[test]
    fn a_missing_explicit_path_fails_instead_of_falling_through() {
        let s = search(Some("/nowhere/onnxruntime"), Some("/app"), &["/chain"]);
        let err = s.resolve(|_| false).unwrap_err().to_string();
        assert!(err.contains("ORT_DYLIB_PATH"), "{err}");
    }

    #[test]
    fn nothing_found_is_an_error_not_a_bare_load() {
        let s = search(None, Some("/app"), &["/chain"]);
        let err = s.resolve(|_| false).unwrap_err().to_string();
        assert!(err.contains("bare-name load is refused"), "{err}");
    }

    #[test]
    fn falls_back_to_the_chain_prefix() {
        let s = search(None, Some("/app"), &["/chain"]);
        let found = s.resolve(|p| p.starts_with("/chain")).unwrap();
        let chain = std::path::absolute("/chain").unwrap();
        assert!(found.starts_with(&chain), "{found:?}");
    }

    #[test]
    fn an_explicit_bare_name_resolves_to_an_absolute_path() {
        // Only the working directory holds it: ort would hand the bare name to the OS search.
        let s = search(Some(DYLIB_NAMES[0]), Some("/app"), &["/chain"]);
        let found = s.resolve(|p| p == Path::new(DYLIB_NAMES[0])).unwrap();
        assert!(found.is_absolute(), "{found:?}");
        assert!(
            found.parent().is_some_and(|p| !p.as_os_str().is_empty()),
            "{found:?}"
        );
    }

    #[test]
    fn the_chain_marker_matches_regardless_of_case() {
        let bin = b"\0ORT C:\\TEMP\\onnx-src\\onnxruntime\\core\\session\\x.cc\0";
        assert!(embeds_marker(bin, r"C:\temp\onnx-src\onnxruntime\core\"));
    }

    #[test]
    fn foreign_build_paths_are_not_the_chain() {
        // Windows ML's System32 copy and a manylinux (PyPI/GitHub release) build.
        let win = b"C:\\__w\\1\\s\\onnxruntime\\onnxruntime\\core\\session\\x.cc";
        assert!(!embeds_marker(win, r"C:\temp\onnx-src\onnxruntime\core\"));
        let linux = b"/onnxruntime_src/onnxruntime/core/session/x.cc";
        assert!(!embeds_marker(linux, "/opt/onnxruntime/onnxruntime/core/"));
    }

    fn scratch_file(name: &str, contents: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("ort-runtime-{}-{name}", std::process::id()));
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn verify_refuses_a_dylib_without_the_chain_path() {
        let path = scratch_file(
            "foreign",
            b"C:\\__w\\1\\s\\onnxruntime\\onnxruntime\\core\\",
        );
        let result = verify_chain_build(&path);
        std::fs::remove_file(&path).unwrap();
        assert!(result.is_err());
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn verify_accepts_a_dylib_with_the_chain_path() {
        let marker = CHAIN_SOURCE_MARKER.unwrap();
        let path = scratch_file("chain", format!("\0{marker}session\0").as_bytes());
        let result = verify_chain_build(&path);
        std::fs::remove_file(&path).unwrap();
        result.unwrap();
    }
}
