mod model_utils;
pub(crate) mod postprocess;
mod preprocess;

mod tract_backend;

#[cfg(any(target_os = "linux", windows))]
mod ort_backend;

use anyhow::{bail, Context, Result};
use log::info;

#[cfg(any(target_os = "linux", windows))]
use crate::ort_ext::extract_first_f32_output;

use kataglyphis_core::config::{self, PreprocessMode};
use kataglyphis_core::detection::Detection;
use preprocess::{rgba_to_nchw_f32_letterboxed, rgba_to_nchw_f32_stretched};

/// The model's file name, in the checkout's `resources/models/` and in every package's.
pub const MODEL_FILE: &str = "deim_dfine_n_coco.onnx";

/// The model to load: `explicit` when it is not blank, else `KATAGLYPHIS_ONNX_MODEL`,
/// else the model beside the running exe, else the checkout's.
pub fn resolve_model_path(explicit: Option<&str>) -> String {
    if let Some(p) = explicit {
        if !p.trim().is_empty() {
            return p.to_string();
        }
    }
    kataglyphis_core::config::onnx_model_override()
        .clone()
        .or_else(bundled_model_path)
        .unwrap_or_else(default_model_path)
}

/// `<exe dir>/resources/models/deim_dfine_n_coco.onnx`, when that file exists.
/// Every Windows package puts `resources\` beside the exe, so a packaged app needs nothing set.
fn bundled_model_path() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    existing_model_under(exe.parent()?)
}

/// [`model_under`] `root`, when it is a file.
fn existing_model_under(root: &std::path::Path) -> Option<String> {
    let path = model_under(root);
    path.is_file().then(|| path.to_string_lossy().into_owned())
}

/// `<root>/resources/models/deim_dfine_n_coco.onnx`: the checkout's layout, and every package's.
fn model_under(root: &std::path::Path) -> std::path::PathBuf {
    root.join("resources").join("models").join(MODEL_FILE)
}

/// The dev-checkout fallback `<workspace>/resources/models/deim_dfine_n_coco.onnx` (`scripts/linux/fetch-models.sh`).
fn default_model_path() -> String {
    // `ancestors().nth(2)` rather than `parent()` unwraps, so a moved crate cannot panic.
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.ancestors().nth(2).unwrap_or(manifest);
    model_under(root).to_string_lossy().to_string()
}

enum Backend {
    // tract's `run` takes `self: &Arc<Self>`, so the plan must live in an Arc.
    Tract {
        model: std::sync::Arc<TractPlan>,
    },

    #[cfg(any(target_os = "linux", windows))]
    Ort {
        session: ort::session::Session,
    },
}

// See tract_backend.rs: tract 0.23 renamed `SimplePlan` to `RunnableModel`.
type TractPlan = tract_onnx::prelude::TypedRunnableModel;

struct BackendLoad {
    backend: Backend,
    input_dims: (u32, u32),
}

pub struct PersonDetector {
    backend: Backend,
    input_w: u32,
    input_h: u32,
    preprocess: PreprocessMode,
    swap_xy: bool,
    preprocess_buf: Vec<f32>,
}

impl PersonDetector {
    fn from_parts(
        backend: Backend,
        input_w: u32,
        input_h: u32,
        preprocess: PreprocessMode,
        swap_xy: bool,
    ) -> Self {
        let buf_len = 3 * (input_w as usize) * (input_h as usize);
        Self {
            backend,
            input_w,
            input_h,
            preprocess,
            swap_xy,
            preprocess_buf: vec![0.0; buf_len],
        }
    }

    /// [`Self::with_backend`] with `KATAGLYPHIS_ONNX_BACKEND`.
    pub fn new(model_path: &str) -> Result<Self> {
        Self::with_backend(model_path, config::onnx_backend().as_deref())
    }

    /// Loads `model_path` with `backend`: `"ort"`, `"tract"`, or `None` for ORT with tract as its fallback.
    pub fn with_backend(model_path: &str, backend: Option<&str>) -> Result<Self> {
        info!("Loading ONNX model: {model_path}");
        // Checked first: a backend that cannot open the file would report itself missing instead.
        if !std::path::Path::new(model_path).is_file() {
            bail!("no ONNX model at {model_path}");
        }

        let preprocess = config::preprocess_mode();
        let swap_xy = config::swap_xy_enabled();
        log::debug!("Preprocess mode: {:?}, swap_xy: {}", preprocess, swap_xy);

        let BackendLoad {
            backend,
            input_dims,
        } = Self::load_backend(model_path, backend)?;
        let (input_w, input_h) = input_dims;

        Ok(Self::from_parts(
            backend, input_w, input_h, preprocess, swap_xy,
        ))
    }

    /// The backend that loaded the model: `"ort"` or `"tract"`.
    pub fn backend_name(&self) -> &'static str {
        match self.backend {
            Backend::Tract { .. } => "tract",
            #[cfg(any(target_os = "linux", windows))]
            Backend::Ort { .. } => "ort",
        }
    }

    #[cfg(any(target_os = "linux", windows))]
    fn load_ort(model_path: &str) -> Result<BackendLoad> {
        let (session, dims) = ort_backend::load_ort_session(model_path)
            .with_context(|| format!("ORT could not load {model_path}"))?;
        info!("ONNX backend: ort ({}x{})", dims.0, dims.1);
        Ok(BackendLoad {
            backend: Backend::Ort { session },
            input_dims: dims,
        })
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    fn load_ort(model_path: &str) -> Result<BackendLoad> {
        bail!("ORT could not load {model_path}: the family builds no ONNX Runtime for this target")
    }

    fn load_tract(model_path: &str) -> Result<BackendLoad> {
        let (model, dims) = tract_backend::load_tract_model(model_path)
            .with_context(|| format!("tract could not load {model_path}"))?;
        info!("ONNX backend: tract ({}x{})", dims.0, dims.1);
        Ok(BackendLoad {
            backend: Backend::Tract { model },
            input_dims: dims,
        })
    }

    /// ORT first; when it fails, a warning with ORT's reason, then tract.
    #[cfg(any(target_os = "linux", windows))]
    fn load_ort_or_tract(model_path: &str) -> Result<BackendLoad> {
        let ort_err = match Self::load_ort(model_path) {
            Ok(load) => return Ok(load),
            Err(err) => err,
        };
        log::warn!("Falling back to tract: {ort_err:#}");
        Self::load_tract(model_path)
            .map_err(|tract_err| anyhow::anyhow!("{ort_err:#}; {tract_err:#}"))
    }

    fn load_backend(model_path: &str, requested: Option<&str>) -> Result<BackendLoad> {
        match requested {
            Some("ort") | Some("onnxruntime") => Self::load_ort(model_path),
            Some("tract") => Self::load_tract(model_path),
            Some(other) => {
                bail!("Unsupported KATAGLYPHIS_ONNX_BACKEND='{other}'. Use 'tract' or 'ort'.")
            }
            #[cfg(any(target_os = "linux", windows))]
            None => Self::load_ort_or_tract(model_path),
            // No chain ORT exists here, so tract is the backend, not a fallback.
            #[cfg(not(any(target_os = "linux", windows)))]
            None => Self::load_tract(model_path),
        }
    }

    /// Person-only convenience wrapper (COCO class 0) over [`Self::infer_rgba`].
    pub fn infer_persons_rgba(
        &mut self,
        rgba: &[u8],
        width: u32,
        height: u32,
        score_threshold: f32,
    ) -> Result<Vec<Detection>> {
        self.infer_rgba(rgba, width, height, score_threshold, Some(&[0]))
    }

    /// Run detection and keep only the given COCO class ids.
    /// `None` keeps every class the model emits (e.g. 15 = cat, 16 = dog).
    pub fn infer_rgba(
        &mut self,
        rgba: &[u8],
        width: u32,
        height: u32,
        score_threshold: f32,
        class_filter: Option<&[i64]>,
    ) -> Result<Vec<Detection>> {
        let mapping = match self.preprocess {
            PreprocessMode::Letterbox => rgba_to_nchw_f32_letterboxed(
                rgba,
                width,
                height,
                self.input_w,
                self.input_h,
                &mut self.preprocess_buf,
            )?,
            PreprocessMode::Stretch => rgba_to_nchw_f32_stretched(
                rgba,
                width,
                height,
                self.input_w,
                self.input_h,
                &mut self.preprocess_buf,
            )?,
        };

        let (shape, data) = Self::infer_raw_nchw_f32(
            &mut self.backend,
            &self.preprocess_buf,
            self.input_h,
            self.input_w,
        )
        .context("Failed to run ONNX model")?;

        let mut detections = postprocess::parse_yolo_like_detections(
            &shape,
            &data,
            score_threshold,
            mapping,
            self.swap_xy,
        )?;

        if let Some(ids) = class_filter {
            detections.retain(|d| ids.contains(&d.class_id));
        }
        Ok(detections)
    }

    fn infer_raw_nchw_f32(
        backend: &mut Backend,
        input: &[f32],
        input_h: u32,
        input_w: u32,
    ) -> Result<(Vec<usize>, Vec<f32>)> {
        match backend {
            Backend::Tract { model } => {
                use tract_onnx::prelude::*;

                let shape = [1usize, 3usize, input_h as usize, input_w as usize];
                let tensor = Tensor::from_shape(&shape, input)?;
                let outputs = model
                    .run(tvec!(tensor.into()))
                    .context("Failed to run ONNX model (tract)")?;

                let out = outputs.first().context("Model returned no outputs")?;

                let shape = out.shape().to_vec();
                // `to_plain_array_view` errors unless the storage is plain and really f32.
                let view = out
                    .to_plain_array_view::<f32>()
                    .context("Model output is not plain f32")?;
                let data = view
                    .as_slice()
                    .context("Model output view is not contiguous")?;

                Ok((shape, data.to_vec()))
            }

            #[cfg(any(target_os = "linux", windows))]
            Backend::Ort { session } => {
                // `Tensor::from_array` needs owned data; this is a single allocation.
                let input_tensor = ort::value::Tensor::from_array((
                    [1usize, 3usize, input_h as usize, input_w as usize],
                    input.to_vec().into_boxed_slice(),
                ))
                .context("Failed to create ORT input tensor")?;

                let outputs = session
                    .run(ort::inputs![input_tensor])
                    .context("Failed to run ONNX model (ort)")?;

                let (shape, data) = extract_first_f32_output(&outputs)?;
                Ok((shape, data))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_explicit_path_wins_and_a_blank_one_does_not() {
        assert_eq!(resolve_model_path(Some("models/x.onnx")), "models/x.onnx");
        assert_ne!(resolve_model_path(Some("  ")), "  ");
    }

    #[test]
    fn the_model_beside_an_exe_counts_only_when_it_exists() {
        let root = std::env::temp_dir().join(format!("oxidant-model-{}", std::process::id()));
        assert_eq!(existing_model_under(&root), None);
        let model = model_under(&root);
        std::fs::create_dir_all(model.parent().expect("models dir")).expect("create models dir");
        std::fs::write(&model, b"onnx").expect("write model");
        let found = existing_model_under(&root);
        std::fs::remove_dir_all(&root).expect("remove temp root");
        assert_eq!(found, Some(model.to_string_lossy().into_owned()));
    }

    #[test]
    fn the_checkout_fallback_is_the_workspace_model() {
        let path = default_model_path();
        assert!(std::path::Path::new(&path).ends_with("resources/models/deim_dfine_n_coco.onnx"));
    }
}
