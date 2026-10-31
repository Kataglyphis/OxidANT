use anyhow::{anyhow, Result};
use log::info;
use ort::session::builder::SessionBuilder;
use ort::session::Session;

use super::model_utils::validate_model_path;
use crate::ort_ext::OrtResultExt;
use kataglyphis_core::config;

const DEFAULT_INPUT_DIMS: (u32, u32) = (640, 640);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Device {
    Cpu,
    Auto,
    Cuda,
    Migraphx,
    Directml,
}

impl Device {
    pub(super) fn parse(value: &str) -> Result<Self> {
        match value {
            "cpu" => Ok(Self::Cpu),
            "auto" => Ok(Self::Auto),
            "cuda" => Ok(Self::Cuda),
            "migraphx" if cfg!(target_os = "linux") => Ok(Self::Migraphx),
            "directml" if cfg!(windows) => Ok(Self::Directml),
            _ => Err(anyhow!(
                "Unsupported KATAGLYPHIS_ORT_DEVICE='{value}' on this platform"
            )),
        }
    }

    pub(super) fn requires_gpu(self) -> bool {
        !matches!(self, Self::Cpu | Self::Auto)
    }
}

fn register_provider(
    builder: &mut SessionBuilder,
    provider: &dyn ort::execution_providers::ExecutionProvider,
) -> Result<()> {
    if !provider.is_available().map_err(|err| anyhow!("{err}"))? {
        return Err(anyhow!(
            "{} is absent from the loaded ONNX Runtime",
            provider.name()
        ));
    }
    provider.register(builder).map_err(|err| anyhow!("{err}"))?;
    info!("ORT {} registered", provider.name());
    Ok(())
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
    use ort::execution_providers::{MIGraphX, CUDA};

    if device == Device::Directml && !cfg!(windows) {
        return Err(anyhow!("DirectML is available only on Windows"));
    }
    if device == Device::Migraphx && !cfg!(target_os = "linux") {
        return Err(anyhow!("MIGraphX is available only on Linux"));
    }
    let mut providers: Vec<Box<dyn ort::execution_providers::ExecutionProvider>> = Vec::new();
    if matches!(device, Device::Auto | Device::Cuda) {
        providers.push(Box::new(CUDA::default()));
    }
    if cfg!(target_os = "linux") && matches!(device, Device::Auto | Device::Migraphx) {
        providers.push(Box::new(MIGraphX::default()));
    }
    #[cfg(windows)]
    if matches!(device, Device::Auto | Device::Directml) {
        let value = match std::env::var("KATAGLYPHIS_DIRECTML_DEVICE_ID") {
            Ok(value) => Some(value),
            Err(std::env::VarError::NotPresent) => None,
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(anyhow!("KATAGLYPHIS_DIRECTML_DEVICE_ID must be Unicode"))
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
            Err(err) if device.requires_gpu() => return Err(err),
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

pub(crate) fn load_ort_session(
    model_path: &str,
    profile: Option<&std::path::Path>,
) -> Result<(Session, (u32, u32))> {
    let canonical_path = validate_model_path(model_path)?;
    let device = Device::parse(config::ort_device())?;
    crate::ort_runtime::ensure_ort_loaded()?;

    info!("ORT device request: {device:?}");
    let session = try_device(device, |provider| {
        create_session(&canonical_path, provider, profile)
    })?;
    info!(
        "ORT session created from model file: {}",
        canonical_path.display()
    );

    let (input_w, input_h) = extract_ort_input_dims(&session);

    Ok((session, (input_w, input_h)))
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
    use super::{parse_directml_device_id, providers_for_device, try_device, Device};

    #[test]
    fn selects_cpu_or_explicit_gpu_without_silent_fallback() {
        assert_eq!(Device::parse("cpu").unwrap(), Device::Cpu);
        assert_eq!(Device::parse("auto").unwrap(), Device::Auto);
        assert!(!Device::Cpu.requires_gpu());
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
        assert!(!Device::Auto.requires_gpu());
        assert!(Device::parse("cuda").unwrap().requires_gpu());
        assert!(Device::parse("unknown").is_err());
        assert_eq!(Device::parse("migraphx").is_ok(), cfg!(target_os = "linux"));
        assert_eq!(Device::parse("directml").is_ok(), cfg!(windows));
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
                assert!(device.requires_gpu());
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

        let mut attempts = 0;
        let result: anyhow::Result<()> = try_device(Device::Cuda, |_| {
            attempts += 1;
            Err(anyhow::anyhow!("session initialization failed"))
        });
        assert!(result.is_err());
        assert_eq!(attempts, 1);
    }
}
