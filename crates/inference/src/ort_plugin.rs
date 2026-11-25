//! Plugin execution providers: modules the family image ships outside ONNX Runtime, registered by path.
//! `GetAvailableProviders` never lists one and the by-name append never reaches one.
//! The path and registration policy is the hub's `ort_plugin_eps.rs`; this file binds it to the pinned ort.

use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::ort_ext::Provider;
use crate::ort_plugin_eps::{plugin_library_path, PluginDevice, PluginRegistry, RuntimeBinding};
pub use crate::ort_plugin_eps::{PluginLibrary, MIGRAPHX};
use ort::environment::Environment;
use ort::session::builder::SessionBuilder;
use ort::AsPointer;

use crate::provider::ProviderUnavailable;

/// Every plugin a session may ask for by its registration name.
const PLUGINS: &[&PluginLibrary] = &[&MIGRAPHX];

/// The device a session gets: the plugin's first, which is also the vendor runtime's device 0.
const FIRST_DEVICE: usize = 0;

/// The plugin registered under `name`, if `name` is a plugin's.
pub fn named(name: &str) -> Option<&'static PluginLibrary> {
    PLUGINS.iter().copied().find(|plugin| plugin.name == name)
}

/// The module this process would register: the image's root when it names one, else beside the loaded ONNX Runtime.
pub fn module(plugin: &'static PluginLibrary) -> anyhow::Result<PathBuf> {
    let bundled = bundled_directory()?;
    let root = std::env::var_os(plugin.root_variable);
    match plugin_library_path(plugin, &bundled, root.as_deref()) {
        Ok(Some(path)) => Ok(path),
        Ok(None) => Err(ProviderUnavailable {
            provider: plugin.name,
            reason: format!(
                "has no module {}: ${} is empty or unset and {} does not exist",
                plugin.filename,
                plugin.root_variable,
                bundled.join(plugin.filename).display()
            ),
        }
        .into()),
        Err(reason) => Err(ProviderUnavailable {
            provider: plugin.name,
            reason,
        }
        .into()),
    }
}

/// The directory of the loaded ONNX Runtime, where a package carries the module.
fn bundled_directory() -> anyhow::Result<PathBuf> {
    let library = crate::ort_runtime::ensure_ort_loaded()?;
    library.parent().map(Path::to_path_buf).ok_or_else(|| {
        anyhow::anyhow!(
            "the loaded ONNX Runtime {} has no directory",
            library.display()
        )
    })
}

/// A plugin as a session's provider: its module registered once per process, then one of its devices appended.
pub struct PluginEp(pub &'static PluginLibrary);

/// By name: the hub's exact `PluginLibrary` carries no `Debug`.
impl std::fmt::Debug for PluginEp {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("PluginEp")
            .field(&self.0.name)
            .finish()
    }
}

impl Provider for PluginEp {
    fn name(&self) -> &'static str {
        self.0.name
    }

    /// The module on disk makes a plugin available; `GetAvailableProviders` would always answer no.
    fn is_available(&self) -> ort::Result<bool> {
        Ok(module(self.0).is_ok())
    }

    fn register(&self, builder: &mut SessionBuilder) -> ort::Result<()> {
        let module = module(self.0).map_err(|err| ort::Error::new(format!("{err:#}")))?;
        let bundled = bundled_directory().map_err(|err| ort::Error::new(format!("{err:#}")))?;
        let root = std::env::var_os(self.0.root_variable);
        let device = FIRST_DEVICE.to_string();
        // Another library of the process may have registered the name first: AccelerANTgine beside this crate in one app.
        registry()
            .map_err(|err| ort::Error::new(format!("{err:#}")))?
            .append_provider_reusing_external(
                builder,
                self.0,
                FIRST_DEVICE,
                &bundled,
                root.as_deref(),
                &[("device_id", device.as_str())],
            )
            .map_err(|err| ort::Error::new(format!("{}: {err}", module.display())))
    }
}

/// One registry for the process's single ORT environment.
fn registry() -> anyhow::Result<&'static PluginRegistry<OrtBinding>> {
    static REGISTRY: OnceLock<PluginRegistry<OrtBinding>> = OnceLock::new();
    let environment = crate::ort_runtime::environment()?;
    Ok(REGISTRY.get_or_init(|| PluginRegistry::new(OrtBinding { environment })))
}

/// The pinned ort over the process's environment, which lives until the process ends.
struct OrtBinding {
    environment: &'static Environment,
}

/// The C API's status as a result; ORT returns either null or an owned status.
fn status_result(status: ort::sys::OrtStatusPtr) -> Result<(), String> {
    // SAFETY: the status comes from this binding's global C API and is consumed once.
    unsafe { ort::Error::result_from_status(status) }.map_err(|err| err.to_string())
}

impl RuntimeBinding for OrtBinding {
    type Options = SessionBuilder;
    type Device = *const ort::sys::OrtEpDevice;

    fn register_library(&self, name: &str, path: &Path) -> Result<(), String> {
        // The handle has no Drop: the module stays registered until the process ends.
        self.environment
            .register_ep_library(name, path)
            .map(drop)
            .map_err(|err| err.to_string())
    }

    /// The typed C API, because `Environment::devices()` drops a failed `GetEpDevices`.
    fn devices(&self) -> Result<Vec<PluginDevice<Self::Device>>, String> {
        let mut devices = std::ptr::null();
        let mut count = 0;
        // SAFETY: the live environment owns the returned array, whose pointer and length are checked below.
        status_result(unsafe {
            (ort::api().GetEpDevices)(self.environment.ptr(), &mut devices, &mut count)
        })?;
        if count == 0 {
            return Ok(Vec::new());
        }
        if devices.is_null() {
            return Err("ONNX Runtime returned a null plugin device list".into());
        }
        // SAFETY: a successful call returned a non-null array of `count` entries owned by the environment.
        unsafe { std::slice::from_raw_parts(devices, count) }
            .iter()
            .map(|&device| {
                if device.is_null() {
                    return Err("ONNX Runtime returned a null plugin device".into());
                }
                // SAFETY: the entry belongs to this environment, whose API supplies its EP name.
                let name = unsafe { (ort::api().EpDevice_EpName)(device) };
                if name.is_null() {
                    return Err("ONNX Runtime returned a plugin device without a name".into());
                }
                // SAFETY: ONNX Runtime owns this NUL-terminated name while the environment lives.
                let provider = unsafe { CStr::from_ptr(name) }
                    .to_str()
                    .map_err(|err| err.to_string())?
                    .to_owned();
                Ok(PluginDevice { provider, device })
            })
            .collect()
    }

    /// One device only: AMD's factory refuses more, and takes the card from `device_id`, not from the device.
    fn append_provider(
        &self,
        options: &mut SessionBuilder,
        device: &Self::Device,
        values: &[(&str, &str)],
    ) -> Result<(), String> {
        let strings = values
            .iter()
            .map(|(key, value)| {
                Ok((
                    CString::new(*key).map_err(|err| err.to_string())?,
                    CString::new(*value).map_err(|err| err.to_string())?,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let keys: Vec<_> = strings.iter().map(|(key, _)| key.as_ptr()).collect();
        let values: Vec<_> = strings.iter().map(|(_, value)| value.as_ptr()).collect();
        // SAFETY: the builder, the environment, the device and all NUL-terminated strings stay valid for the call.
        status_result(unsafe {
            (ort::api().SessionOptionsAppendExecutionProvider_V2)(
                options.ptr_mut(),
                self.environment.ptr().cast_mut(),
                device,
                1,
                keys.as_ptr(),
                values.as_ptr(),
                keys.len(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plugin_is_found_by_its_registration_name_only() {
        assert_eq!(named("MIGraphX").unwrap().filename, MIGRAPHX.filename);
        assert!(named("MIGraphXExecutionProvider").is_none());
        assert!(named("CUDAExecutionProvider").is_none());
        assert_eq!(PluginEp(&MIGRAPHX).name(), "MIGraphX");
        // A name ending in ".virtual" would switch ONNX Runtime's virtual devices on.
        assert!(PLUGINS
            .iter()
            .all(|plugin| !plugin.name.ends_with(".virtual")));
    }

    /// The CPU provider's device stands in: a plugin lists a device only on its own hardware.
    #[test]
    fn the_binding_lists_and_appends_a_real_device() {
        if crate::ort_runtime::ensure_ort_loaded().is_err() {
            return;
        }
        let binding = OrtBinding {
            environment: crate::ort_runtime::environment().unwrap(),
        };
        let devices = binding.devices().unwrap();
        let cpu = devices
            .iter()
            .find(|device| device.provider == "CPUExecutionProvider")
            .expect("ONNX Runtime always lists its CPU device");
        let mut builder = ort::session::Session::builder(binding.environment).unwrap();
        binding
            .append_provider(&mut builder, &cpu.device, &[])
            .unwrap();
        let refused = binding
            .append_provider(&mut builder, &cpu.device, &[("no\0key", "1")])
            .unwrap_err();
        assert!(refused.contains("nul"), "{refused}");
    }
}
