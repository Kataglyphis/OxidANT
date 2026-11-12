//! Plugin execution providers: modules the family image ships outside ONNX Runtime, registered by path.
//! `GetAvailableProviders` never lists one and the by-name append never reaches one.

use std::ffi::{CStr, CString, OsStr};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use ort::environment::Environment;
use ort::execution_providers::ExecutionProvider;
use ort::session::builder::SessionBuilder;
use ort::AsPointer;

use crate::provider::ProviderUnavailable;

/// One plugin. Another plugin is one more row in `PLUGINS`.
#[derive(Debug)]
pub struct Plugin {
    /// The registration name. AMD's module reports it as the EP name of its devices.
    pub name: &'static str,
    /// Names the plugin's root in the image that ships it; set but empty in every other image.
    pub root_var: &'static str,
    /// The module's file name, in `<root>/lib` or beside the loaded ONNX Runtime.
    pub module: &'static str,
}

/// AMD's MIGraphX plugin, the only MIGraphX provider of the rocm image.
pub const MIGRAPHX: Plugin = Plugin {
    name: "MIGraphX",
    root_var: "ORT_AMDGPU_EP_ROOT",
    module: "libmigraphx-ep.so",
};

const PLUGINS: &[Plugin] = &[MIGRAPHX];

/// The device a session gets: the plugin's first, which is also the vendor runtime's device 0.
const FIRST_DEVICE: usize = 0;

/// Registration names in ort's environment. The lock also covers each read of ONNX Runtime's device array, which a registration grows.
static REGISTERED: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

impl Plugin {
    /// The plugin registered under `name`, if `name` is a plugin's.
    pub fn named(name: &str) -> Option<&'static Plugin> {
        PLUGINS.iter().find(|plugin| plugin.name == name)
    }

    /// Where the module may be, in order: the image's root, then beside the ONNX Runtime library, where a package carries it.
    fn candidates(&self, root: Option<&OsStr>, ort_library: &Path) -> Vec<PathBuf> {
        let in_root = root
            .filter(|root| !root.is_empty())
            .map(|root| Path::new(root).join("lib").join(self.module));
        let beside = ort_library.parent().map(|dir| dir.join(self.module));
        in_root.into_iter().chain(beside).collect()
    }

    /// The first candidate that exists, or `ProviderUnavailable` naming every path tried.
    fn find(
        &self,
        root: Option<&OsStr>,
        ort_library: &Path,
        exists: impl Fn(&Path) -> bool,
    ) -> Result<PathBuf, ProviderUnavailable> {
        let candidates = self.candidates(root, ort_library);
        if let Some(found) = candidates.iter().find(|path| exists(path)) {
            return Ok(found.clone());
        }
        let tried: Vec<String> = candidates
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        Err(ProviderUnavailable {
            provider: self.name,
            reason: format!(
                "has no module {}: tried {} (${}/lib when that variable is not empty, then the directory of the loaded ONNX Runtime)",
                self.module,
                tried.join(", "),
                self.root_var
            ),
        })
    }

    /// The module this process would register.
    pub fn module(&self) -> anyhow::Result<PathBuf> {
        let ort_library = crate::ort_runtime::ensure_ort_loaded()?;
        let root = std::env::var_os(self.root_var);
        Ok(self.find(root.as_deref(), &ort_library, Path::is_file)?)
    }
}

/// A plugin as a session's provider: its module registered once per process, then one of its devices appended.
#[derive(Debug)]
pub struct PluginEp(pub &'static Plugin);

impl ExecutionProvider for PluginEp {
    fn name(&self) -> &'static str {
        self.0.name
    }

    /// The module on disk makes a plugin available; `GetAvailableProviders` would always answer no.
    fn is_available(&self) -> ort::Result<bool> {
        Ok(self.0.module().is_ok())
    }

    fn register(&self, builder: &mut SessionBuilder) -> ort::Result<()> {
        let module = self
            .0
            .module()
            .map_err(|err| ort::Error::new(format!("{err:#}")))?;
        let about_module =
            |err: ort::Error| ort::Error::new(format!("{}: {err}", module.display()));
        let env = Environment::current()?;
        {
            let mut registered = REGISTERED.lock().unwrap_or_else(PoisonError::into_inner);
            register_once(
                &mut registered,
                self.0.name,
                || lists_a_device(&env, self.0.name),
                // The handle has no Drop: the module stays registered until the process ends.
                || env.register_ep_library(self.0.name, &module).map(drop),
            )
            .map_err(about_module)?;
        }
        append_device(builder, &env, self.0.name, FIRST_DEVICE).map_err(about_module)
    }
}

/// Runs `register` unless `name` is recorded or `listed`: ONNX Runtime refuses a second registration of a name.
fn register_once<E>(
    registered: &mut Vec<&'static str>,
    name: &'static str,
    listed: impl FnOnce() -> bool,
    register: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    if !registered.contains(&name) {
        // Another library of the process may have registered the name first: AccelerANTgine beside this crate in one app.
        if !listed() {
            // ONNX Runtime rolls a failed registration back, so the next request may try again.
            register()?;
        }
        registered.push(name);
    }
    Ok(())
}

/// Whether ONNX Runtime already lists a device of the EP `name`.
fn lists_a_device(env: &Environment, name: &str) -> bool {
    env.devices()
        .any(|device| device.ep().is_ok_and(|ep| ep == name))
}

/// The options a plugin gets: an option the plugin does not list fails the creation of its provider.
fn device_options(index: usize) -> [(&'static CStr, CString); 1] {
    let id = CString::new(index.to_string()).expect("a decimal number has no NUL");
    [(c"device_id", id)]
}

/// Appends the `index`-th device ONNX Runtime lists for the EP `name`, with `index` as its `device_id`.
fn append_device(
    builder: &mut SessionBuilder,
    env: &Environment,
    name: &str,
    index: usize,
) -> ort::Result<()> {
    let _reading = REGISTERED.lock().unwrap_or_else(PoisonError::into_inner);
    let listed: Vec<_> = env
        .devices()
        .filter(|device| device.ep().is_ok_and(|ep| ep == name))
        .collect();
    let Some(device) = listed.get(index) else {
        return Err(ort::Error::new(format!(
            "ONNX Runtime lists {} {name} device(s), so there is no device {index}",
            listed.len()
        )));
    };
    // One device only: AMD's factory refuses more, and takes the card from the option, not from the device.
    let devices = [device.ptr()];
    let options = device_options(index);
    let keys = options.each_ref().map(|(key, _)| key.as_ptr());
    let values = options.each_ref().map(|(_, value)| value.as_ptr());
    // SAFETY: the builder, the environment, the device and all NUL-terminated strings stay valid for the call.
    let status = unsafe {
        (ort::api().SessionOptionsAppendExecutionProvider_V2)(
            builder.ptr_mut(),
            env.ptr().cast_mut(),
            devices.as_ptr(),
            devices.len(),
            keys.as_ptr(),
            values.as_ptr(),
            keys.len(),
        )
    };
    // SAFETY: ORT returns either null or an owned status from this API call.
    unsafe { ort::Error::result_from_status(status) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORT_LIBRARY: &str = "/app/lib/libonnxruntime.so";

    fn os(value: &str) -> Option<&OsStr> {
        Some(OsStr::new(value))
    }

    #[test]
    fn the_image_root_comes_first_and_the_directory_of_onnx_runtime_second() {
        assert_eq!(
            MIGRAPHX.candidates(
                os("/usr/local/lib/onnxruntime-ep-amdgpu"),
                Path::new(ORT_LIBRARY)
            ),
            [
                PathBuf::from("/usr/local/lib/onnxruntime-ep-amdgpu/lib/libmigraphx-ep.so"),
                PathBuf::from("/app/lib/libmigraphx-ep.so"),
            ]
        );
    }

    #[test]
    fn an_empty_or_unset_root_leaves_only_the_directory_of_onnx_runtime() {
        let beside = [PathBuf::from("/app/lib/libmigraphx-ep.so")];
        assert_eq!(MIGRAPHX.candidates(os(""), Path::new(ORT_LIBRARY)), beside);
        assert_eq!(MIGRAPHX.candidates(None, Path::new(ORT_LIBRARY)), beside);
    }

    #[test]
    fn a_root_without_the_file_falls_back_to_the_packaged_module() {
        let found = MIGRAPHX
            .find(os("/empty-root"), Path::new(ORT_LIBRARY), |path| {
                path.starts_with("/app")
            })
            .unwrap();
        assert_eq!(found, Path::new("/app/lib/libmigraphx-ep.so"));
        let in_root = MIGRAPHX
            .find(os("/root"), Path::new(ORT_LIBRARY), |_| true)
            .unwrap();
        assert_eq!(in_root, Path::new("/root/lib/libmigraphx-ep.so"));
    }

    #[test]
    fn a_missing_module_names_the_plugin_the_file_and_every_path_tried() {
        let error = MIGRAPHX
            .find(os("/root"), Path::new(ORT_LIBRARY), |_| false)
            .unwrap_err();
        assert_eq!(error.provider, "MIGraphX");
        let message = error.to_string();
        for part in [
            "ProviderUnavailable: MIGraphX has no module libmigraphx-ep.so",
            "/root/lib/libmigraphx-ep.so, /app/lib/libmigraphx-ep.so",
            "$ORT_AMDGPU_EP_ROOT/lib",
        ] {
            assert!(message.contains(part), "{message}");
        }
    }

    #[test]
    fn a_plugin_is_found_by_its_registration_name_only() {
        assert_eq!(Plugin::named("MIGraphX").unwrap().module, MIGRAPHX.module);
        assert!(Plugin::named("MIGraphXExecutionProvider").is_none());
        assert!(Plugin::named("CUDAExecutionProvider").is_none());
        assert_eq!(PluginEp(&MIGRAPHX).name(), "MIGraphX");
        // A name ending in ".virtual" would switch ONNX Runtime's virtual devices on.
        assert!(PLUGINS
            .iter()
            .all(|plugin| !plugin.name.ends_with(".virtual")));
    }

    #[test]
    fn a_name_is_registered_once_and_a_failure_is_not_recorded() {
        let mut registered = Vec::new();
        let mut calls = 0;
        let failed = register_once(
            &mut registered,
            "MIGraphX",
            || false,
            || {
                calls += 1;
                Err("no HIP device")
            },
        );
        assert_eq!(failed, Err("no HIP device"));
        assert!(registered.is_empty());
        for _ in 0..2 {
            let done: Result<(), &str> = register_once(
                &mut registered,
                "MIGraphX",
                || false,
                || {
                    calls += 1;
                    Ok(())
                },
            );
            assert_eq!(done, Ok(()));
        }
        assert_eq!((calls, registered), (2, vec!["MIGraphX"]));
    }

    #[test]
    fn a_name_another_library_registered_is_used_and_not_registered_again() {
        let mut registered = Vec::new();
        let done: Result<(), &str> = register_once(
            &mut registered,
            "MIGraphX",
            || true,
            || Err("EP library is already registered under MIGraphX"),
        );
        assert_eq!(done, Ok(()));
        assert_eq!(registered, vec!["MIGraphX"]);
    }

    #[test]
    fn the_only_option_is_the_device_id() {
        let options = device_options(3);
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].0, c"device_id");
        assert_eq!(options[0].1.as_c_str(), c"3");
    }

    /// The CPU provider's device stands in: a plugin lists a device only on its own hardware.
    #[test]
    fn one_listed_device_is_appended_and_a_device_not_listed_is_an_error() {
        if crate::ort_runtime::ensure_ort_loaded().is_err() {
            return;
        }
        let env = Environment::current().unwrap();
        let builder = || ort::session::Session::builder().unwrap();
        append_device(&mut builder(), &env, "CPUExecutionProvider", 0).unwrap();
        let beyond = append_device(&mut builder(), &env, "CPUExecutionProvider", 1).unwrap_err();
        assert!(
            beyond
                .to_string()
                .contains("lists 1 CPUExecutionProvider device(s), so there is no device 1"),
            "{beyond}"
        );
        let none = append_device(&mut builder(), &env, "NoSuchProvider", 0).unwrap_err();
        assert!(
            none.to_string()
                .contains("lists 0 NoSuchProvider device(s)"),
            "{none}"
        );
    }
}
