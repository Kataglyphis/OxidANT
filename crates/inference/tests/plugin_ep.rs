//! AMD's plugin where it cannot run: a named `migraphx` fails and says what was looked for, `auto` goes on to the CPU.
//! One test: the plugin's root variable and ONNX Runtime's registrations belong to the process.
#![cfg(target_os = "linux")]

mod common;

use std::path::{Path, PathBuf};

use common::{model, unavailable};
use kataglyphis_inference::ort_plugin::{PluginEp, MIGRAPHX};
use kataglyphis_inference::ort_runtime::ensure_ort_loaded;
use kataglyphis_inference::person_detection::PersonDetector;
use kataglyphis_inference::provider::ProviderUnavailable;
use ort::execution_providers::ExecutionProvider;

/// A plugin root under the temp directory; `lib/` exists and is empty, as in every image without the plugin.
fn root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("oxidant-plugin-{}-{name}", std::process::id()));
    std::fs::create_dir_all(root.join("lib")).unwrap();
    root
}

fn named_request_fails(model: &str) -> ProviderUnavailable {
    let error = unavailable(PersonDetector::with_ort_device(model, "migraphx"));
    assert_eq!(error.provider, "MIGraphX");
    error
}

fn auto_runs_on_the_cpu(model: &str) {
    let detector = PersonDetector::with_ort_device(model, "auto").unwrap();
    assert_eq!(detector.execution_provider(), "cpu");
}

/// Qualcomm's module, the one real plugin the arm64 image carries: it registers and lists no device off a Snapdragon.
fn a_real_plugin_without_a_device() -> Option<PathBuf> {
    let root = std::env::var_os("ORT_QNN_EP_ROOT").filter(|root| !root.is_empty())?;
    let module = Path::new(&root).join("lib/libonnxruntime_providers_qnn.so");
    module.is_file().then_some(module)
}

#[test]
fn a_named_migraphx_fails_with_what_was_looked_for_and_auto_runs_on_the_cpu() {
    let Some(model) = model() else { return };
    let beside = ensure_ort_loaded()
        .unwrap()
        .with_file_name(MIGRAPHX.module)
        .display()
        .to_string();

    // Every image without the plugin sets the variable to the empty string.
    std::env::set_var(MIGRAPHX.root_var, "");
    assert!(!PluginEp(&MIGRAPHX).is_available().unwrap());
    let missing = named_request_fails(&model);
    assert!(
        missing.reason.contains("has no module libmigraphx-ep.so")
            && missing.reason.contains(&beside),
        "{missing}"
    );
    auto_runs_on_the_cpu(&model);

    let empty = root("empty");
    std::env::set_var(MIGRAPHX.root_var, &empty);
    let in_root = empty.join("lib").join(MIGRAPHX.module);
    let missing = named_request_fails(&model);
    assert!(
        missing.reason.contains(&in_root.display().to_string()) && missing.reason.contains(&beside),
        "{missing}"
    );

    // A file ONNX Runtime cannot load stands for a module that cannot register, as AMD's without a card.
    std::fs::write(&in_root, b"not a module").unwrap();
    assert!(PluginEp(&MIGRAPHX).is_available().unwrap());
    let refused = named_request_fails(&model);
    assert!(
        refused.reason.contains("failed to register")
            && refused.reason.contains(&in_root.display().to_string()),
        "{refused}"
    );
    assert_eq!(named_request_fails(&model), refused);
    auto_runs_on_the_cpu(&model);
    std::fs::remove_dir_all(&empty).unwrap();

    let Some(plugin) = a_real_plugin_without_a_device() else {
        eprintln!("SKIP: this image carries no plugin module to register");
        return;
    };
    let registers = root("registers");
    std::os::unix::fs::symlink(&plugin, registers.join("lib").join(MIGRAPHX.module)).unwrap();
    std::env::set_var(MIGRAPHX.root_var, &registers);
    let no_device = named_request_fails(&model);
    assert!(
        no_device.reason.contains("lists 0 MIGraphX device(s)"),
        "{no_device}"
    );
    // Registered by the first request: a second registration of the name would be ONNX Runtime's error instead.
    assert_eq!(named_request_fails(&model), no_device);
    auto_runs_on_the_cpu(&model);
    std::fs::remove_dir_all(&registers).unwrap();
}
