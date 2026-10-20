use anyhow::Context;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "burn_demos", about = "Burn demos for common PyTorch tasks")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Demonstrates PyTorch-like tensor basics: matmul, broadcast, reductions.
    TensorDemo,

    /// Fits y = 3x + 2 (with light noise) using a single Linear layer.
    LinearRegression {
        #[arg(long, default_value_t = 50)]
        epochs: usize,
        #[arg(long, default_value_t = 50)]
        steps_per_epoch: usize,
        #[arg(long, default_value_t = 0.02)]
        lr: f64,
        #[arg(long, default_value_t = 256)]
        batch_size: usize,
        /// Save loss curve as PNG.
        #[arg(long)]
        plot_path: Option<std::path::PathBuf>,
    },

    /// Learns XOR using a small MLP.
    Xor {
        #[arg(long, default_value_t = 2000)]
        epochs: usize,
        #[arg(long, default_value_t = 0.05)]
        lr: f64,
        /// Save loss curve as PNG.
        #[arg(long)]
        plot_path: Option<std::path::PathBuf>,
    },

    /// Trains a deeper MLP on a synthetic 2D "two moons" classification dataset.
    TwoMoons {
        #[arg(long, default_value_t = 200)]
        epochs: usize,
        #[arg(long, default_value_t = 50)]
        steps_per_epoch: usize,
        #[arg(long, default_value_t = 0.01)]
        lr: f64,
        #[arg(long, default_value_t = 256)]
        batch_size: usize,
        #[arg(long, default_value_t = 0.12)]
        noise: f32,
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// Save loss curve as PNG.
        #[arg(long)]
        plot_path: Option<std::path::PathBuf>,
    },

    /// Builds a tiny YOLO-like conv net and runs a forward pass (optionally a couple train steps).
    YoloTiny {
        #[arg(long, default_value_t = 128)]
        height: usize,
        #[arg(long, default_value_t = 128)]
        width: usize,
        #[arg(long, default_value_t = 20)]
        num_classes: usize,
        #[arg(long, default_value_t = 3)]
        num_anchors: usize,
        #[arg(long, default_value_t = 0)]
        train_steps: usize,
        #[arg(long, default_value_t = 0.001)]
        lr: f64,
    },

    /// Loads resources/models/deim_dfine_n_coco.onnx (scripts/linux/fetch-models.sh) via ONNX Runtime and runs it;
    /// optionally trains a tiny adapter on its outputs.
    #[cfg(any(target_os = "linux", windows))]
    #[command(alias = "onnx-yolov10")]
    OnnxDetector {
        #[arg(long, default_value_t = 0)]
        warmup: usize,
        #[arg(long, default_value_t = 1)]
        runs: usize,
        #[arg(long, default_value_t = 3)]
        print_topk: usize,
        #[arg(long, default_value_t = 0)]
        train_adapter_steps: usize,
        #[arg(long, default_value_t = 0.001)]
        lr: f64,
        #[arg(long)]
        model_path: Option<std::path::PathBuf>,
    },

    /// Save/Load demonstration using a burnpack file.
    SaveLoad,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::TensorDemo => kataglyphis_burn_demos::simple::tensor_demo().map(drop),

        Command::LinearRegression {
            epochs,
            steps_per_epoch,
            lr,
            batch_size,
            plot_path,
        } => kataglyphis_burn_demos::simple::linear_regression_demo(
            epochs,
            steps_per_epoch,
            lr,
            batch_size,
            plot_path,
        )
        .map(drop),

        Command::Xor {
            epochs,
            lr,
            plot_path,
        } => kataglyphis_burn_demos::simple::xor_demo(epochs, lr, plot_path).map(drop),

        Command::TwoMoons {
            epochs,
            steps_per_epoch,
            lr,
            batch_size,
            noise,
            seed,
            plot_path,
        } => kataglyphis_burn_demos::two_moons::two_moons_demo(
            epochs,
            steps_per_epoch,
            lr,
            batch_size,
            noise,
            seed,
            plot_path,
        )
        .map(drop),

        Command::YoloTiny {
            height,
            width,
            num_classes,
            num_anchors,
            train_steps,
            lr,
        } => kataglyphis_burn_demos::simple::yolo_tiny_demo(
            height,
            width,
            num_classes,
            num_anchors,
            train_steps,
            lr,
        )
        .map(drop),

        #[cfg(any(target_os = "linux", windows))]
        Command::OnnxDetector {
            warmup,
            runs,
            print_topk,
            train_adapter_steps,
            lr,
            model_path,
        } => {
            let model_path = model_path.unwrap_or_else(|| {
                kataglyphis_inference::person_detection::resolve_model_path(None).into()
            });

            let device = kataglyphis_burn_demos::training_device();
            kataglyphis_burn_demos::onnx_detector::onnx_detector_demo(
                &model_path,
                warmup,
                runs,
                print_topk,
                train_adapter_steps,
                lr,
                &device,
            )
            .map(drop)
            .with_context(|| format!("onnx-detector demo failed (model={})", model_path.display()))
        }

        Command::SaveLoad => kataglyphis_burn_demos::simple::save_load_demo().map(drop),
    }
}
