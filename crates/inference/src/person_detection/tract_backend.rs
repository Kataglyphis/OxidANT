use anyhow::Context;

use super::model_utils::validate_model_path;

/// Width and height of a concrete NCHW shape (a zero side counts as one), or `None` for another rank.
fn dims_from_nchw(shape: &[usize]) -> Option<(u32, u32)> {
    let [_, _, h, w] = shape else {
        return None;
    };
    Some((
        u32::try_from((*w).max(1)).ok()?,
        u32::try_from((*h).max(1)).ok()?,
    ))
}

// tract 0.23's fully applied name for `SimplePlan<TypedFact, Box<dyn TypedOp>, TypedModel>`.
type TractPlan = tract_onnx::prelude::TypedRunnableModel;

// `into_runnable()` already returns an Arc, and `run` takes `self: &Arc<Self>`.
pub(crate) fn load_tract_model(
    model_path: &str,
) -> anyhow::Result<(std::sync::Arc<TractPlan>, (u32, u32))> {
    use tract_onnx::prelude::*;

    let canonical_path = validate_model_path(model_path)?;

    let mut model = tract_onnx::onnx()
        .model_for_path(&canonical_path)
        .with_context(|| {
            format!(
                "Failed to load ONNX model from '{}'",
                canonical_path.display()
            )
        })?;

    let fact = model
        .input_fact(0)
        .context("Model has no input[0]")?
        .clone();
    let concrete = fact.shape.as_concrete_finite()?.map(|shape| shape.to_vec());
    let (input_w, input_h) = concrete
        .as_deref()
        .and_then(dims_from_nchw)
        .unwrap_or_else(|| {
            log::warn!("the model names no fixed input size; 640x640 assumed");
            (640, 640)
        });

    model.set_input_fact(
        0,
        InferenceFact::dt_shape(
            f32::datum_type(),
            tvec!(1, 3, input_h as usize, input_w as usize),
        ),
    )?;

    let runnable = model.into_optimized()?.into_runnable()?;
    Ok((runnable, (input_w, input_h)))
}

#[cfg(test)]
mod tests {
    use super::dims_from_nchw;

    #[test]
    fn a_concrete_nchw_shape_gives_its_size_and_another_rank_none() {
        assert_eq!(dims_from_nchw(&[1, 3, 480, 640]), Some((640, 480)));
        assert_eq!(dims_from_nchw(&[1, 3, 0, 640]), Some((640, 1)));
        // None makes the load log say that 640x640 was assumed.
        assert_eq!(dims_from_nchw(&[3, 640, 640]), None);
    }
}
