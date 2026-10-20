//! Each demo does what its name says: the numbers come out right, the nets learn, the model survives a save.

use std::sync::{Mutex, MutexGuard};

use kataglyphis_burn_demos::{lcg::Lcg, losses, plot, simple, two_moons, yolo};

/// Burn's CPU backend draws every random weight from one process-wide generator, so a seeded test runs alone.
fn seeded(seed: u64) -> MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    let turn = TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    kataglyphis_burn_demos::training_device().seed(seed);
    turn
}

#[test]
fn the_tensor_demo_sums_the_broadcast_product() {
    let _turn = seeded(1);
    // [[1,2],[3,4]] @ [[5,6],[7,8]] = [[19,22],[43,50]]; + [1,2] per row sums to 140.
    assert_eq!(simple::tensor_demo().unwrap(), 140.0);
}

#[test]
fn linear_regression_fits_the_line() {
    let _turn = seeded(2);
    let losses = simple::linear_regression_demo(20, 20, 0.05, 128, None).unwrap();
    assert_eq!(losses.len(), 20);
    let (first, last) = (losses[0], *losses.last().unwrap());
    // The noise is under 0.05, so a fitted line leaves a mean squared error far below that.
    assert!(last < 0.01, "{losses:?}");
    assert!(last < first / 10.0, "{losses:?}");
}

#[test]
fn the_xor_net_learns_xor() {
    let _turn = seeded(3);
    let outputs = simple::xor_demo(1500, 0.05, None).unwrap();
    assert_eq!(outputs.len(), 4);
    let learned = outputs
        .iter()
        .zip([0.0, 1.0, 1.0, 0.0])
        .all(|(out, want)| (out - want).abs() < 0.2);
    assert!(learned, "00, 01, 10, 11 -> {outputs:?}");
}

#[test]
fn two_moons_are_told_apart() {
    let _turn = seeded(4);
    let accuracy = two_moons::two_moons_demo(20, 25, 0.01, 128, 0.12, 42, None).unwrap();
    assert!(accuracy > 0.97, "accuracy {accuracy}");
}

#[test]
fn the_tiny_yolo_head_has_one_cell_per_32_pixels() {
    let _turn = seeded(5);
    let dims = simple::yolo_tiny_demo(64, 96, 20, 3, 1, 0.001).unwrap();
    assert_eq!(dims, [1, 3 * (5 + 20), 2, 3]);
}

#[test]
fn a_saved_model_loads_with_the_same_weights() {
    let _turn = seeded(6);
    let (before, after) = simple::save_load_demo().unwrap();
    assert_eq!(before, after);
}

#[test]
fn binary_cross_entropy_matches_its_formula() {
    let _turn = seeded(7);
    use burn::tensor::{Tensor, TensorData};
    let device = kataglyphis_burn_demos::inference_device();
    let pred = Tensor::<2>::from_data(TensorData::new(vec![0.9f32, 0.2], [2, 1]), &device);
    let target = Tensor::<2>::from_data(TensorData::new(vec![1.0f32, 0.0], [2, 1]), &device);
    let loss = losses::binary_cross_entropy(pred, target).into_scalar::<f32>();
    let want = -((0.9f32).ln() + (0.8f32).ln()) / 2.0;
    assert!((loss - want).abs() < 1e-5, "{loss} vs {want}");
}

#[test]
fn the_lcg_repeats_its_sequence_and_stays_in_range() {
    let (mut a, mut b) = (Lcg::new(7), Lcg::new(7));
    for _ in 0..1000 {
        let x = a.next_f32();
        assert_eq!(x, b.next_f32());
        assert!((0.0..=1.0).contains(&x), "{x}");
    }
    assert_ne!(Lcg::new(7).next_u32(), Lcg::new(8).next_u32());
}

#[test]
fn a_loss_curve_is_written_as_png() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("loss.png");
    plot::plot_loss_curve(&path, &[1.0, 0.5, 0.25, 0.2], "test loss").unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "not a PNG");
}

#[test]
fn an_empty_loss_series_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let err = plot::plot_loss_curve(dir.path().join("loss.png"), &[], "empty").unwrap_err();
    assert_eq!(err.to_string(), "cannot plot empty loss series");
}

#[test]
fn the_demo_input_is_seeded() {
    let _turn = seeded(8);
    let device = kataglyphis_burn_demos::inference_device();
    let values = || {
        yolo::YoloTiny::demo_input(&device, 1, 8, 8)
            .to_data()
            .try_to_vec::<f32>()
            .unwrap()
    };
    let first = values();
    assert_eq!(first.len(), 3 * 8 * 8);
    assert_eq!(first, values());
}
