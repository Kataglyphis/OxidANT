//! Shared helpers for working with `ort` (ONNX Runtime) results and outputs.
//! `ort::Error<SessionBuilder>` is not `Send + Sync`, so anyhow's `.context()` cannot take it.

use anyhow::{bail, Context, Result};

/// Maps `ort::Error<R>` for any `R` into `anyhow::Error` through its unconditional `Display`.
pub trait OrtResultExt<T> {
    fn with_ort_context(self, msg: &'static str) -> Result<T>;
}

impl<T, R> OrtResultExt<T> for std::result::Result<T, ort::Error<R>> {
    #[inline]
    fn with_ort_context(self, msg: &'static str) -> Result<T> {
        self.map_err(|e| anyhow::anyhow!("{msg}: {e}"))
    }
}

/// Extract the first output of an ORT run as owned `(shape, flat_f32_data)`; bails on dynamic dims.
pub fn extract_first_f32_output(
    outputs: &ort::session::SessionOutputs<'_>,
) -> Result<(Vec<usize>, Vec<f32>)> {
    let (_, out) = outputs.iter().next().context("Model returned no outputs")?;

    let (shape, data) = out
        .try_extract_tensor::<f32>()
        .context("Failed to extract f32 tensor from ORT output")?;

    let mut dims = Vec::with_capacity(shape.len());
    for &d in shape.iter() {
        if d < 0 {
            bail!("ORT output had dynamic/negative dimension: {d}");
        }
        dims.push(d as usize);
    }

    Ok((dims, data.to_vec()))
}

/// An execution provider behind a pointer; `ort` 2.0.0-rc.15's own trait has a const name and a static check, so no `dyn`.
pub trait Provider: Send + Sync {
    /// ONNX Runtime's name, e.g. `CUDAExecutionProvider`.
    fn name(&self) -> &'static str;
    /// Whether the loaded ONNX Runtime was built with it; a plugin answers whether its module is on disk.
    fn is_available(&self) -> ort::Result<bool>;
    /// Appends it to `builder`'s session options.
    fn register(&self, builder: &mut ort::session::builder::SessionBuilder) -> ort::Result<()>;
}

/// One of `ort`'s built-in providers as a [`Provider`].
pub struct Builtin<E>(pub E);

impl<E: ort::ep::ExecutionProvider> Provider for Builtin<E> {
    fn name(&self) -> &'static str {
        E::NAME
    }

    fn is_available(&self) -> ort::Result<bool> {
        E::is_available()
    }

    fn register(&self, builder: &mut ort::session::builder::SessionBuilder) -> ort::Result<()> {
        self.0.register(builder)
    }
}
