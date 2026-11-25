// SPDX-License-Identifier: MIT

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct PluginLibrary {
    pub name: &'static str,
    pub root_variable: &'static str,
    pub filename: &'static str,
    pub root_subdirectory: &'static str,
}

pub const MIGRAPHX: PluginLibrary = PluginLibrary {
    name: "MIGraphX",
    root_variable: "ORT_AMDGPU_EP_ROOT",
    filename: if cfg!(windows) {
        "migraphx-ep.dll"
    } else {
        "libmigraphx-ep.so"
    },
    root_subdirectory: if cfg!(windows) { "" } else { "lib" },
};

#[derive(Clone)]
pub struct PluginDevice<D> {
    pub provider: String,
    pub device: D,
}

/// The adapter must retain its owning environment throughout each call and validate every enumerated device.
pub trait RuntimeBinding {
    type Options;
    type Device;

    fn register_library(&self, name: &str, path: &Path) -> Result<(), String>;
    fn devices(&self) -> Result<Vec<PluginDevice<Self::Device>>, String>;
    fn append_provider(
        &self,
        options: &mut Self::Options,
        device: &Self::Device,
        values: &[(&str, &str)],
    ) -> Result<(), String>;
}

fn canonical_module(candidate: &Path, advertised: bool) -> Result<Option<PathBuf>, String> {
    let path = match fs::canonicalize(candidate) {
        Ok(path) => path,
        Err(error) => {
            let absent = matches!(fs::symlink_metadata(candidate), Err(ref entry) if entry.kind() == ErrorKind::NotFound);
            if !advertised && error.kind() == ErrorKind::NotFound && absent {
                return Ok(None);
            }
            return Err(format!(
                "Cannot resolve plugin module {}: {error}",
                candidate.display()
            ));
        }
    };
    if !path.is_file() {
        return Err(format!(
            "Plugin module is not a regular file: {}",
            path.display()
        ));
    }
    Ok(Some(path))
}

/// An advertised image root wins; only an absent advertisement and bundled module permit no plugin.
pub fn plugin_library_path(
    plugin: &PluginLibrary,
    bundled: &Path,
    advertised: Option<&OsStr>,
) -> Result<Option<PathBuf>, String> {
    if let Some(root) = advertised.filter(|root| !root.is_empty()) {
        let root = Path::new(root);
        if !root.is_absolute() {
            return Err(format!(
                "{} must be an absolute directory: {}",
                plugin.root_variable,
                root.display()
            ));
        }
        if !root.is_dir() {
            return Err(format!(
                "{} is not a directory: {}",
                plugin.root_variable,
                root.display()
            ));
        }
        return canonical_module(
            &root.join(plugin.root_subdirectory).join(plugin.filename),
            true,
        );
    }
    if bundled.as_os_str().is_empty() {
        return Ok(None);
    }
    if !bundled.is_absolute() {
        return Err(format!(
            "Bundled ORT directory must be absolute: {}",
            bundled.display()
        ));
    }
    canonical_module(&bundled.join(plugin.filename), false)
}

/// Share one registry for one environment; this cache does not coordinate external unregistration.
pub struct PluginRegistry<B> {
    binding: B,
    registered: Mutex<BTreeMap<&'static str, PathBuf>>,
}

impl<B: RuntimeBinding> PluginRegistry<B> {
    pub fn new(binding: B) -> Self {
        Self {
            binding,
            registered: Mutex::new(BTreeMap::new()),
        }
    }

    fn ensure_registered(
        &self,
        registered: &mut BTreeMap<&'static str, PathBuf>,
        plugin: &PluginLibrary,
        bundled: &Path,
        advertised: Option<&OsStr>,
        reuse_existing: bool,
    ) -> Result<(), String> {
        let path = plugin_library_path(plugin, bundled, advertised)?
            .ok_or_else(|| format!("{} plugin is unavailable", plugin.name))?;
        if let Some(previous) = registered.get(plugin.name) {
            if previous != &path {
                return Err(format!(
                    "{} is already registered from {}, refusing {}",
                    plugin.name,
                    previous.display(),
                    path.display()
                ));
            }
            return Ok(());
        }
        // Another library's registration is used, never recorded as this registry's own.
        if reuse_existing
            && self
                .binding
                .devices()?
                .iter()
                .any(|device| device.provider == plugin.name)
        {
            return Ok(());
        }
        self.binding.register_library(plugin.name, &path)?;
        registered.insert(plugin.name, path);
        Ok(())
    }

    pub fn append_provider(
        &self,
        options: &mut B::Options,
        plugin: &PluginLibrary,
        index: usize,
        bundled: &Path,
        advertised: Option<&OsStr>,
        values: &[(&str, &str)],
    ) -> Result<(), String> {
        self.append(options, plugin, index, bundled, advertised, values, false)
    }

    /// Uses a device another library of the process already registered instead of registering the module again.
    pub fn append_provider_reusing_external(
        &self,
        options: &mut B::Options,
        plugin: &PluginLibrary,
        index: usize,
        bundled: &Path,
        advertised: Option<&OsStr>,
        values: &[(&str, &str)],
    ) -> Result<(), String> {
        self.append(options, plugin, index, bundled, advertised, values, true)
    }

    #[allow(clippy::too_many_arguments)]
    fn append(
        &self,
        options: &mut B::Options,
        plugin: &PluginLibrary,
        index: usize,
        bundled: &Path,
        advertised: Option<&OsStr>,
        values: &[(&str, &str)],
        reuse_existing: bool,
    ) -> Result<(), String> {
        let mut registered = self
            .registered
            .lock()
            .map_err(|_| "Plugin registry lock is poisoned".to_string())?;
        self.ensure_registered(&mut registered, plugin, bundled, advertised, reuse_existing)?;
        let devices: Vec<_> = self
            .binding
            .devices()?
            .into_iter()
            .filter(|device| device.provider == plugin.name)
            .collect();
        let selected = devices.get(index).ok_or_else(|| {
            format!(
                "{} device index {index} is unavailable; found {}",
                plugin.name,
                devices.len()
            )
        })?;
        self.binding
            .append_provider(options, &selected.device, values)
    }
}
