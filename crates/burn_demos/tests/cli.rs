//! The `burn-demos` binary: each subcommand runs and prints its result.

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::str::contains;

fn demo(args: &[&str]) -> assert_cmd::assert::Assert {
    cargo_bin_cmd!("burn-demos").args(args).assert()
}

#[test]
fn the_tensor_demo_prints_its_sum() {
    demo(&["tensor-demo"])
        .success()
        .stdout(contains("sum=140.0"));
}

#[test]
fn the_xor_demo_prints_its_outputs() {
    demo(&["xor", "--epochs", "200"])
        .success()
        .stdout(contains("xor outputs (00, 01, 10, 11):"));
}

#[test]
fn the_regression_demo_plots_its_loss() {
    let dir = tempfile::tempdir().unwrap();
    let plot = dir.path().join("loss.png");
    demo(&[
        "linear-regression",
        "--epochs",
        "2",
        "--steps-per-epoch",
        "2",
        "--plot-path",
        plot.to_str().unwrap(),
    ])
    .success()
    .stdout(contains("epoch 0 loss="));
    assert!(plot.is_file());
}

#[test]
fn the_two_moons_demo_reports_its_accuracy() {
    demo(&["two-moons", "--epochs", "1", "--steps-per-epoch", "5"])
        .success()
        .stdout(contains("final acc="));
}

#[test]
fn the_yolo_demo_prints_its_output_shape() {
    demo(&["yolo-tiny", "--height", "64", "--width", "96"])
        .success()
        .stdout(contains("yolo_tiny output shape: [1, 75, 2, 3]"));
}

#[test]
fn the_save_load_demo_round_trips() {
    demo(&["save-load"])
        .success()
        .stdout(contains("save/load ok"));
}

#[cfg(any(target_os = "linux", windows))]
#[test]
fn the_detector_demo_names_a_missing_model() {
    demo(&["onnx-detector", "--model-path", "/nonexistent/deim.onnx"])
        .failure()
        .stderr(contains(
            "onnx-detector demo failed (model=/nonexistent/deim.onnx)",
        ));
}
