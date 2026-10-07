use burn::tensor::Tensor;

/// Mean binary cross-entropy of 2-D `pred` and `target` in `[0, 1]`, as a 1-D scalar tensor.
pub fn binary_cross_entropy(pred: Tensor<2>, target: Tensor<2>) -> Tensor<1> {
    let eps = 1e-6;
    let pred = pred.clamp(eps, 1.0 - eps);
    let one_minus_pred = pred.clone().mul_scalar(-1.0f32).add_scalar(1.0f32);
    let one_minus_y = one_minus_pred.clone();
    -(target * pred.log() + one_minus_y * one_minus_pred.log()).mean()
}
