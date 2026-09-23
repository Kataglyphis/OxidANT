#[allow(unused_imports)]
use anyhow::bail;
use anyhow::Result;
use log::info;

use super::model_utils::validate_model_path;
use crate::ort_ext::OrtResultExt;
#[cfg(feature = "onnxruntime_cuda")]
use kataglyphis_core::config;

const DEFAULT_INPUT_DIMS: (u32, u32) = (640, 640);

pub(crate) fn load_ort_session(model_path: &str) -> Result<(ort::session::Session, (u32, u32))> {
    use ort::session::Session;

    let canonical_path = validate_model_path(model_path)?;
    crate::ort_runtime::ensure_ort_loaded()?;

    let mut builder = Session::builder().with_ort_context("Failed to create ORT SessionBuilder")?;

    #[cfg(feature = "onnxruntime_cuda")]
    {
        use ort::execution_providers::{ExecutionProvider, CUDA};
        let device = config::ort_device();

        info!("ORT device request: {device}");

        if device == "cuda" || device == "auto" {
            let cuda = CUDA::default();
            match cuda.is_available() {
                Ok(true) => {}
                Ok(false) => {
                    bail!("ORT was built without CUDA support (CUDA EP unavailable).");
                }
                Err(err) => {
                    bail!("Failed to query CUDA EP availability: {err}");
                }
            }

            let cuda_result = builder
                .with_execution_providers([cuda.build().error_on_failure()])
                .with_ort_context("Failed to configure ORT CUDA execution provider");

            match cuda_result {
                Ok(b) => {
                    info!("ORT CUDA execution provider enabled");
                    builder = b;
                }
                Err(err) => {
                    if device == "cuda" {
                        return Err(err);
                    }
                    log::warn!("ORT CUDA provider unavailable, falling back to CPU: {err:#}");
                    builder = Session::builder()
                        .with_ort_context("Failed to recreate ORT SessionBuilder")?;
                }
            }
        }
    }

    #[cfg(all(feature = "onnxruntime_directml", windows))]
    {
        use ort::execution_providers::DirectML;
        builder = builder
            .with_execution_providers([DirectML::default().build()])
            .with_ort_context("Failed to configure ORT DirectML execution provider")?;
        info!("ORT DirectML execution provider enabled");
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

fn extract_ort_input_dims(session: &ort::session::Session) -> (u32, u32) {
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
