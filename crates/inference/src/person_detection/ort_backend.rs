use anyhow::{anyhow, Result};
use log::info;
use ort::session::builder::SessionBuilder;
use ort::session::Session;

use super::model_utils::validate_model_path;
use crate::ort_ext::OrtResultExt;
use crate::provider::ProviderUnavailable;
use kataglyphis_core::config;

const DEFAULT_INPUT_DIMS: (u32, u32) = (640, 640);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Device {
    Cpu,
    Auto,
    Cuda,
    Migraphx,
    Directml,
    Xnnpack,
    Dnnl,
}

impl Device {
    pub(super) fn parse(value: &str) -> Result<Self> {
        match value {
            "cpu" => Ok(Self::Cpu),
            "auto" => Ok(Self::Auto),
            "cuda" => Ok(Self::Cuda),
            "migraphx" if cfg!(target_os = "linux") => Ok(Self::Migraphx),
            "directml" if cfg!(windows) => Ok(Self::Directml),
            "xnnpack" => Ok(Self::Xnnpack),
            "dnnl" => Ok(Self::Dnnl),
            _ => Err(anyhow!(
                "Unsupported KATAGLYPHIS_ORT_DEVICE='{value}' on this platform"
            )),
        }
    }

    /// A named provider: it runs or the load fails, never a silent CPU session.
    pub(super) fn is_explicit(self) -> bool {
        !matches!(self, Self::Cpu | Self::Auto)
    }
}

/// The short label a session reports: `cpu` without a registered provider.
pub(super) fn provider_label(provider: Option<&str>) -> &'static str {
    match provider {
        None => "cpu",
        Some("CUDAExecutionProvider") => "cuda",
        Some("MIGraphXExecutionProvider") => "migraphx",
        Some("DmlExecutionProvider") => "directml",
        Some("XnnpackExecutionProvider") => "xnnpack",
        Some("DnnlExecutionProvider") => "dnnl",
        Some(_) => "other",
    }
}

fn register_provider(
    builder: &mut SessionBuilder,
    provider: &dyn ort::execution_providers::ExecutionProvider,
) -> Result<()> {
    require_provider_available(
        provider.name(),
        provider.is_available().map_err(|err| anyhow!("{err}")),
    )?;
    #[cfg(target_os = "linux")]
    if provider.name() == "MIGraphXExecutionProvider" {
        super::hip_probe::require_answer(provider.name())?;
    }
    // A listed provider whose library cannot load (the image's DNNL without libdnnl.so.3) is unavailable too.
    provider
        .register(builder)
        .map_err(|err| ProviderUnavailable {
            provider: provider.name(),
            reason: format!("failed to register: {err}"),
        })?;
    info!("ORT {} registered", provider.name());
    Ok(())
}

fn require_provider_available(name: &'static str, available: Result<bool>) -> Result<()> {
    if !available? {
        return Err(ProviderUnavailable {
            provider: name,
            reason: "is absent from the loaded ONNX Runtime".into(),
        }
        .into());
    }
    Ok(())
}

// The legacy MIGraphX options struct can turn an empty cache path into literal quotes.
#[cfg(target_os = "linux")]
struct GenericMigraphx;

#[cfg(target_os = "linux")]
fn with_migraphx_options<T>(
    append: impl FnOnce(&std::ffi::CStr, [&std::ffi::CStr; 2], [&std::ffi::CStr; 2]) -> T,
) -> T {
    let limit = std::ffi::CString::new(usize::MAX.to_string())
        .expect("a decimal memory limit cannot contain NUL");
    append(
        c"MIGraphX",
        [c"device_id", c"migraphx_mem_limit"],
        [c"0", limit.as_c_str()],
    )
}

#[cfg(target_os = "linux")]
impl ort::execution_providers::ExecutionProvider for GenericMigraphx {
    fn name(&self) -> &'static str {
        "MIGraphXExecutionProvider"
    }

    fn register(&self, builder: &mut SessionBuilder) -> ort::Result<()> {
        use ort::AsPointer;

        with_migraphx_options(|name, keys, values| {
            let keys = keys.map(std::ffi::CStr::as_ptr);
            let values = values.map(std::ffi::CStr::as_ptr);
            // SAFETY: the builder and all NUL-terminated strings stay valid for the call.
            let status = unsafe {
                (ort::api().SessionOptionsAppendExecutionProvider)(
                    builder.ptr_mut(),
                    name.as_ptr(),
                    keys.as_ptr(),
                    values.as_ptr(),
                    keys.len(),
                )
            };
            // SAFETY: ORT returns either null or an owned status from this API call.
            unsafe { ort::error::Error::result_from_status(status) }
        })
    }
}

#[cfg(any(windows, test))]
fn parse_directml_device_id(value: Option<&str>) -> Result<Option<i32>> {
    let Some(value) = value else { return Ok(None) };
    let id = value
        .trim()
        .parse::<i32>()
        .map_err(|_| anyhow!("KATAGLYPHIS_DIRECTML_DEVICE_ID must be a non-negative integer"))?;
    if id < 0 {
        return Err(anyhow!(
            "KATAGLYPHIS_DIRECTML_DEVICE_ID must be a non-negative integer"
        ));
    }
    Ok(Some(id))
}

fn providers_for_device(
    device: Device,
) -> Result<Vec<Box<dyn ort::execution_providers::ExecutionProvider>>> {
    use ort::execution_providers::CUDA;

    if device == Device::Directml && !cfg!(windows) {
        return Err(anyhow!("DirectML is available only on Windows"));
    }
    if device == Device::Migraphx && !cfg!(target_os = "linux") {
        return Err(anyhow!("MIGraphX is available only on Linux"));
    }
    // The chain ONNX Runtime builds oneDNN on x86_64 only.
    if device == Device::Dnnl && !cfg!(target_arch = "x86_64") {
        return Err(ProviderUnavailable {
            provider: "DnnlExecutionProvider",
            reason: "is built only into the x86_64 chain ONNX Runtime".into(),
        }
        .into());
    }
    let mut providers: Vec<Box<dyn ort::execution_providers::ExecutionProvider>> = Vec::new();
    if matches!(device, Device::Auto | Device::Cuda) {
        providers.push(Box::new(CUDA::default()));
    }
    #[cfg(target_os = "linux")]
    if matches!(device, Device::Auto | Device::Migraphx) {
        providers.push(Box::new(GenericMigraphx));
    }
    if device == Device::Xnnpack {
        providers.push(Box::new(ort::execution_providers::XNNPACK::default()));
    }
    if device == Device::Dnnl {
        providers.push(Box::new(ort::execution_providers::OneDNN::default()));
    }
    #[cfg(windows)]
    if matches!(device, Device::Auto | Device::Directml) {
        let value = match std::env::var("KATAGLYPHIS_DIRECTML_DEVICE_ID") {
            Ok(value) => Some(value),
            Err(std::env::VarError::NotPresent) => None,
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(anyhow!("KATAGLYPHIS_DIRECTML_DEVICE_ID must be Unicode"));
            }
        };
        let mut provider = ort::execution_providers::DirectML::default();
        if let Some(id) = parse_directml_device_id(value.as_deref())? {
            provider = provider.with_device_id(id);
        }
        providers.push(Box::new(provider));
    }

    Ok(providers)
}

fn try_device<T>(
    device: Device,
    mut attempt: impl FnMut(Option<&dyn ort::execution_providers::ExecutionProvider>) -> Result<T>,
) -> Result<T> {
    for provider in providers_for_device(device)? {
        match attempt(Some(provider.as_ref())) {
            Ok(value) => return Ok(value),
            Err(err) if device.is_explicit() => return Err(err),
            Err(err) => log::warn!("ORT {} failed: {err:#}", provider.name()),
        }
    }
    attempt(None)
}

fn create_session(
    path: &std::path::Path,
    provider: Option<&dyn ort::execution_providers::ExecutionProvider>,
    profile: Option<&std::path::Path>,
) -> Result<Session> {
    let mut builder = Session::builder().with_ort_context("Failed to create ORT SessionBuilder")?;
    if let Some(prefix) = profile {
        builder = builder
            .with_profiling(prefix)
            .with_ort_context("Failed to enable ORT profiling")?;
    }
    if let Some(provider) = provider {
        if provider.name() == "DmlExecutionProvider" {
            builder = builder
                .with_memory_pattern(false)
                .with_ort_context("Failed to disable ORT memory patterns for DirectML")?;
        }
        register_provider(&mut builder, provider)?;
    }
    // Spinning suits back-to-back inference runs.
    builder = builder
        .with_intra_op_spinning(config::ort_spin_enabled())
        .with_ort_context("Failed to set ORT intra-op spinning")?;
    let threads = config::ort_intra_threads();
    if threads > 0 {
        builder = builder
            .with_intra_threads(threads)
            .with_ort_context("Failed to set ORT intra-op threads")?;
    }
    builder
        .commit_from_file(path)
        .with_ort_context("Failed to load ONNX model (ort)")
}

pub(super) struct OrtLoad {
    pub(super) session: Session,
    pub(super) input_dims: (u32, u32),
    /// [`provider_label`] of the provider the session runs on.
    pub(super) provider: &'static str,
}

pub(super) fn load_ort_session(
    model_path: &str,
    profile: Option<&std::path::Path>,
    device: Device,
) -> Result<OrtLoad> {
    let canonical_path = validate_model_path(model_path)?;
    crate::ort_runtime::ensure_ort_loaded()?;

    info!("ORT device request: {device:?}");
    let (session, provider) = try_device(device, |provider| {
        let session = create_session(&canonical_path, provider, profile)?;
        Ok((session, provider_label(provider.map(|p| p.name()))))
    })?;
    info!(
        "ORT session created on {provider} from model file: {}",
        canonical_path.display()
    );

    let input_dims = extract_ort_input_dims(&session);

    Ok(OrtLoad {
        session,
        input_dims,
        provider,
    })
}

fn extract_ort_input_dims(session: &Session) -> (u32, u32) {
    let Some(first) = session.inputs().first() else {
        return DEFAULT_INPUT_DIMS;
    };
    let Some(shape) = first.dtype().tensor_shape() else {
        return DEFAULT_INPUT_DIMS;
    };
    if shape.len() != 4 {
        return DEFAULT_INPUT_DIMS;
    }
    let h = shape[2];
    let w = shape[3];
    if h <= 0 || w <= 0 {
        return DEFAULT_INPUT_DIMS;
    }
    (w as u32, h as u32)
}

#[cfg(test)]
mod device_tests {
    use super::{
        parse_directml_device_id, provider_label, providers_for_device, require_provider_available,
        try_device, Device,
    };
    use crate::provider::ProviderUnavailable;

    #[test]
    fn unavailable_provider_or_availability_error_is_rejected() {
        assert!(require_provider_available("test", Ok(true)).is_ok());
        let absent = require_provider_available("test", Ok(false)).unwrap_err();
        assert!(absent.to_string().contains("absent"));
        assert_eq!(
            absent
                .downcast_ref::<ProviderUnavailable>()
                .unwrap()
                .provider,
            "test"
        );
        assert!(
            require_provider_available("test", Err(anyhow::anyhow!("availability failed")))
                .unwrap_err()
                .to_string()
                .contains("availability failed")
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn generic_migraphx_uses_only_explicit_non_cache_options_and_propagates_errors() {
        let result: Result<(), &str> = super::with_migraphx_options(|name, keys, values| {
            assert_eq!(name, c"MIGraphX");
            assert_eq!(keys, [c"device_id", c"migraphx_mem_limit"]);
            assert_eq!(values[0], c"0");
            assert_eq!(values[1].to_str().unwrap(), usize::MAX.to_string());
            Err("injected provider registration failure")
        });
        assert_eq!(result, Err("injected provider registration failure"));
    }

    #[test]
    fn selects_cpu_or_explicit_gpu_without_silent_fallback() {
        assert_eq!(Device::parse("cpu").unwrap(), Device::Cpu);
        assert_eq!(Device::parse("auto").unwrap(), Device::Auto);
        assert!(!Device::Cpu.is_explicit());
        assert!(providers_for_device(Device::Cpu).unwrap().is_empty());
        assert_eq!(
            providers_for_device(Device::Cuda).unwrap()[0].name(),
            "CUDAExecutionProvider"
        );
        if cfg!(target_os = "linux") {
            assert_eq!(
                providers_for_device(Device::Migraphx).unwrap()[0].name(),
                "MIGraphXExecutionProvider"
            );
        }
        if cfg!(windows) {
            assert_eq!(
                providers_for_device(Device::Directml).unwrap()[0].name(),
                "DmlExecutionProvider"
            );
        } else {
            assert!(providers_for_device(Device::Directml).is_err());
        }
        assert!(!Device::Auto.is_explicit());
        assert!(Device::parse("cuda").unwrap().is_explicit());
        assert!(Device::parse("unknown").is_err());
        assert_eq!(Device::parse("migraphx").is_ok(), cfg!(target_os = "linux"));
        assert_eq!(Device::parse("directml").is_ok(), cfg!(windows));
    }

    #[test]
    fn cpu_accelerators_are_explicit_and_never_fall_back() {
        for (name, device, provider) in [
            ("xnnpack", Device::Xnnpack, "XnnpackExecutionProvider"),
            ("dnnl", Device::Dnnl, "DnnlExecutionProvider"),
        ] {
            assert_eq!(Device::parse(name).unwrap(), device);
            assert!(device.is_explicit());
            let mut attempts = Vec::new();
            let result: anyhow::Result<()> = try_device(device, |p| {
                attempts.push(p.map(|p| p.name()));
                Err(anyhow::anyhow!("session initialization failed"))
            });
            assert!(result.is_err());
            match providers_for_device(device) {
                Ok(providers) => {
                    assert_eq!(providers.len(), 1);
                    assert_eq!(providers[0].name(), provider);
                    assert_eq!(attempts, [Some(provider)], "no CPU retry for {name}");
                }
                Err(error) => {
                    assert_eq!(name, "dnnl");
                    assert_ne!(std::env::consts::ARCH, "x86_64", "{error:#}");
                    assert_eq!(
                        error
                            .downcast_ref::<ProviderUnavailable>()
                            .unwrap()
                            .provider,
                        provider
                    );
                    assert!(attempts.is_empty());
                }
            }
        }
        assert!(Device::Cuda.is_explicit() && !Device::Auto.is_explicit());
    }

    #[test]
    fn sessions_name_their_provider_and_plain_sessions_are_cpu() {
        assert_eq!(provider_label(None), "cpu");
        assert_eq!(provider_label(Some("XnnpackExecutionProvider")), "xnnpack");
        assert_eq!(provider_label(Some("DnnlExecutionProvider")), "dnnl");
        assert_eq!(provider_label(Some("CUDAExecutionProvider")), "cuda");
        assert_eq!(
            provider_label(Some("MIGraphXExecutionProvider")),
            "migraphx"
        );
        assert_eq!(provider_label(Some("DmlExecutionProvider")), "directml");
        assert_eq!(provider_label(Some("TensorrtExecutionProvider")), "other");
    }

    #[test]
    fn directml_device_id_is_optional_and_strict() {
        assert_eq!(parse_directml_device_id(None).unwrap(), None);
        assert_eq!(parse_directml_device_id(Some("0")).unwrap(), Some(0));
        assert_eq!(parse_directml_device_id(Some(" 2 ")).unwrap(), Some(2));
        for invalid in ["", "-1", "x", "2147483648"] {
            assert!(
                parse_directml_device_id(Some(invalid)).is_err(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn advertised_gpu_devices_match_the_platform_provider_implementation() {
        let manifest = include_str!("../../Cargo.toml");
        let entry = manifest
            .lines()
            .skip_while(|line| line.trim() != "[package.metadata.kataglyphis]")
            .skip(1)
            .take_while(|line| !line.trim().starts_with('['))
            .find_map(|line| line.trim().strip_prefix("ort-devices = "))
            .expect("missing GPU device capability marker");
        let devices: Vec<&str> = entry
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .expect("GPU device marker must be a TOML string array")
            .split(',')
            .map(|value| {
                value
                    .trim()
                    .strip_prefix('"')
                    .and_then(|value| value.strip_suffix('"'))
                    .expect("GPU device marker must contain quoted strings")
            })
            .collect();
        assert_eq!(devices, ["cuda", "migraphx", "directml"]);
        for (name, supported, provider) in [
            ("cuda", true, "CUDAExecutionProvider"),
            (
                "migraphx",
                cfg!(target_os = "linux"),
                "MIGraphXExecutionProvider",
            ),
            ("directml", cfg!(windows), "DmlExecutionProvider"),
        ] {
            let parsed = Device::parse(name);
            assert_eq!(parsed.is_ok(), supported, "incorrect support for {name}");
            if let Ok(device) = parsed {
                assert!(device.is_explicit());
                assert_eq!(providers_for_device(device).unwrap()[0].name(), provider);
            }
        }
    }

    #[test]
    fn auto_retries_after_session_creation_fails_but_explicit_gpu_does_not() {
        let mut attempts = Vec::new();
        let result = try_device(Device::Auto, |provider| {
            attempts.push(provider.map(|p| p.name()));
            if provider.is_some() {
                Err(anyhow::anyhow!("session initialization failed"))
            } else {
                Ok(())
            }
        });
        assert!(result.is_ok());
        assert_eq!(attempts.last(), Some(&None));
        assert!(attempts.len() > 1);

        for device in [Device::Cuda, Device::Migraphx] {
            if device == Device::Migraphx && !cfg!(target_os = "linux") {
                continue;
            }
            let mut attempts = 0;
            let result: anyhow::Result<()> = try_device(device, |_| {
                attempts += 1;
                Err(anyhow::anyhow!("session initialization failed"))
            });
            assert!(result.is_err());
            assert_eq!(attempts, 1);
        }
    }
}
