//! XNNPACK and oneDNN with the real model: each runs, names itself and finds both cats, or fails as ProviderUnavailable.
#![cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]

mod common;

use common::{assert_both_cats, model, photo, unavailable};
use kataglyphis_inference::person_detection::PersonDetector;

#[test]
fn xnnpack_runs_on_every_chain_architecture() {
    let Some(path) = model() else { return };
    let (rgba, w, h) = photo();
    let mut detector = PersonDetector::with_ort_device(&path, "xnnpack").unwrap();
    assert_eq!(
        (detector.backend_name(), detector.execution_provider()),
        ("ort", "xnnpack")
    );
    assert_both_cats(&detector.infer_rgba(&rgba, w, h, 0.4, None).unwrap(), h, w);
}

/// Whether oneDNN's runtime loads here; the chain ships the DNNL provider without it (hub BACKLOG CON191).
fn onednn_loads() -> bool {
    // SAFETY: loading oneDNN runs only its own initialisers, which take no arguments.
    unsafe { libloading::Library::new("libdnnl.so.3") }.is_ok()
}

#[test]
fn dnnl_runs_where_onednn_loads_and_is_unavailable_elsewhere() {
    let Some(path) = model() else { return };
    let result = PersonDetector::with_ort_device(&path, "dnnl");
    if cfg!(target_arch = "x86_64") && onednn_loads() {
        let (rgba, w, h) = photo();
        let mut detector = result.unwrap();
        assert_eq!(detector.execution_provider(), "dnnl");
        assert_both_cats(&detector.infer_rgba(&rgba, w, h, 0.4, None).unwrap(), h, w);
    } else {
        let error = unavailable(result);
        assert_eq!(error.provider, "DnnlExecutionProvider");
        if cfg!(target_arch = "x86_64") {
            assert!(error.reason.contains("libdnnl.so.3"), "{error}");
        }
    }
}

#[test]
fn the_default_session_runs_on_the_cpu_provider() {
    let Some(path) = model() else { return };
    let detector = PersonDetector::with_ort_device(&path, "cpu").unwrap();
    assert_eq!(detector.execution_provider(), "cpu");
}

#[test]
fn an_absent_gpu_provider_is_unavailable_not_a_cpu_session() {
    use kataglyphis_inference::ort_plugin::{PluginEp, MIGRAPHX};
    use ort::execution_providers::{ExecutionProvider, CUDA};
    let Some(path) = model() else { return };
    kataglyphis_inference::ort_runtime::ensure_ort_loaded().unwrap();
    let mut candidates = vec![(
        "cuda",
        CUDA::default().name(),
        CUDA::default().is_available(),
    )];
    // A plugin is available when its module is on disk; GetAvailableProviders never lists it.
    let migraphx = PluginEp(&MIGRAPHX);
    candidates.push(("migraphx", migraphx.name(), migraphx.is_available()));
    let absent: Vec<_> = candidates
        .into_iter()
        .filter(|(_, _, available)| !available.as_ref().unwrap())
        .collect();
    assert!(
        !absent.is_empty(),
        "every GPU provider is present: nothing absent to test"
    );
    for (device, provider, _) in absent {
        let error = unavailable(PersonDetector::with_ort_device(&path, device));
        assert_eq!(error.provider, provider, "{device}");
    }
}
