use anyhow::{anyhow, Result};
use log::info;
use ort::session::builder::SessionBuilder;
use ort::session::Session;

use super::model_utils::validate_model_path;
use crate::ort_ext::OrtResultExt;
use kataglyphis_core::config;

const DEFAULT_INPUT_DIMS: (u32, u32) = (640, 640);

/// `KATAGLYPHIS_ORT_DEVICE=cuda` needs the CUDA EP; `auto` takes it when the loaded ORT has it, else the CPU.
fn with_cuda(builder: SessionBuilder, device: &str) -> Result<SessionBuilder> {
    use ort::execution_providers::{ExecutionProvider, CUDA};

    let cuda = CUDA::default();
    let enabled = match cuda.is_available() {
        Ok(true) => builder
            .with_execution_providers([cuda.build().error_on_failure()])
            .with_ort_context("Failed to configure ORT CUDA execution provider"),
        Ok(false) => Err(anyhow!(
            "the loaded ONNX Runtime has no CUDA execution provider"
        )),
        Err(err) => Err(anyhow!("Failed to query CUDA EP availability: {err}")),
    };
    match enabled {
        Ok(builder) => {
            info!("ORT CUDA execution provider enabled");
            Ok(builder)
        }
        Err(err) if device == "cuda" => Err(err),
        Err(err) => {
            log::warn!("ORT CUDA provider unavailable, running on the CPU: {err:#}");
            Session::builder().with_ort_context("Failed to recreate ORT SessionBuilder")
        }
    }
}

pub(crate) fn load_ort_session(model_path: &str) -> Result<(Session, (u32, u32))> {
    let canonical_path = validate_model_path(model_path)?;
    crate::ort_runtime::ensure_ort_loaded()?;

    let mut builder = Session::builder().with_ort_context("Failed to create ORT SessionBuilder")?;

    let device = config::ort_device();
    info!("ORT device request: {device}");
    if device == "cuda" || device == "auto" {
        builder = with_cuda(builder, device)?;
    }

    #[cfg(windows)]
    {
        use ort::execution_providers::DirectML;
        builder = builder
            .with_execution_providers([DirectML::default().build()])
            .with_ort_context("Failed to configure ORT DirectML execution provider")?;
        info!("ORT DirectML execution provider enabled");
    }

    // After the EP block, whose CUDA fallback starts a new builder; spinning suits back-to-back runs only.
    builder = builder
        .with_intra_op_spinning(kataglyphis_core::config::ort_spin_enabled())
        .with_ort_context("Failed to set ORT intra-op spinning")?;
    let threads = kataglyphis_core::config::ort_intra_threads();
    if threads > 0 {
        builder = builder
            .with_intra_threads(threads)
            .with_ort_context("Failed to set ORT intra-op threads")?;
    }

    let session = builder
        .commit_from_file(&canonical_path)
        .with_ort_context("Failed to load ONNX model (ort)")?;
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
