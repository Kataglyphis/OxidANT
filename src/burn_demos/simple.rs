use anyhow::Context;
use burn::module::Module;
use burn::nn;
use burn::optim::{AdamConfig, GradientsParams};
use burn::tensor::activation::{relu, sigmoid};
use burn::tensor::{Device, Tensor, TensorData};

use crate::burn_demos::{inference_device, lcg::Lcg, losses, plot, training_device};

pub fn tensor_demo() -> anyhow::Result<()> {
    let device = inference_device();

    let a = Tensor::<2>::from_data(TensorData::new(vec![1.0, 2.0, 3.0, 4.0], [2, 2]), &device);
    let b = Tensor::<2>::from_data(TensorData::new(vec![5.0, 6.0, 7.0, 8.0], [2, 2]), &device);

    let c = a.clone().matmul(b.clone());
    // TensorData's Debug shows raw bytes since burn 0.22; Display shows the values.
    println!("a=\n{}", a.to_data());
    println!("b=\n{}", b.to_data());
    println!("a@b=\n{}", c.to_data());

    let v = Tensor::<2>::from_data(TensorData::new(vec![1.0, 2.0], [1, 2]), &device);
    let d = c + v;
    println!("broadcast add=\n{}", d.to_data());

    let sum = d.sum();
    println!("sum={:?}", sum.into_scalar::<f32>());

    Ok(())
}

#[derive(Module, Debug)]
struct LinearRegressor {
    linear: nn::Linear,
}

impl LinearRegressor {
    fn new(device: &Device) -> Self {
        let linear = nn::LinearConfig::new(1, 1).init(device);
        Self { linear }
    }

    fn forward(&self, x: Tensor<2>) -> Tensor<2> {
        self.linear.forward(x)
    }
}

fn make_regression_batch(device: &Device, batch: usize, step: usize) -> (Tensor<2>, Tensor<2>) {
    // y = 3x + 2 + noise
    let mut xs = Vec::with_capacity(batch);
    let mut ys = Vec::with_capacity(batch);

    let mut rng = Lcg::new((step as u64) ^ 0xD6E8_FEB8_6659_FD93);
    for i in 0..batch {
        let x = rng.next_f32() * 2.0 - 1.0;

        // small deterministic noise
        let noise =
            (((i as f32) * 12.9898 + (step as f32) * 78.233).sin() * 43_758.547).fract() * 0.05;
        let y = 3.0 * x + 2.0 + noise;

        xs.push(x);
        ys.push(y);
    }

    let x = Tensor::<2>::from_data(TensorData::new(xs, [batch, 1]), device);
    let y = Tensor::<2>::from_data(TensorData::new(ys, [batch, 1]), device);
    (x, y)
}

pub fn linear_regression_demo(
    epochs: usize,
    steps_per_epoch: usize,
    lr: f64,
    batch_size: usize,
    plot_path: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    let device = training_device();
    let mut model = LinearRegressor::new(&device);
    let mut optim = AdamConfig::new().init();

    let mut losses = Vec::with_capacity(epochs);

    for epoch in 0..epochs {
        let mut loss_sum = 0.0f32;
        for step in 0..steps_per_epoch {
            let (x, y) = make_regression_batch(&device, batch_size, epoch * steps_per_epoch + step);
            let pred = model.forward(x);
            let loss = (pred - y).powf_scalar(2.0).mean();

            let grads = GradientsParams::from_grads(loss.backward(), &model);
            model = optim.step(lr, model, grads);

            loss_sum += loss.into_scalar::<f32>();
        }

        let loss_avg = loss_sum / steps_per_epoch.max(1) as f32;
        losses.push(loss_avg);
        if epoch % 10 == 0 {
            println!("epoch {epoch} loss={loss_avg:.6}");
        }
    }

    if let Some(path) = plot_path {
        plot::plot_loss_curve(path, &losses, "Linear regression loss")?;
    }

    Ok(())
}

#[derive(Module, Debug)]
struct XorNet {
    l1: nn::Linear,
    l2: nn::Linear,
}

impl XorNet {
    fn new(device: &Device) -> Self {
        let l1 = nn::LinearConfig::new(2, 16).init(device);
        let l2 = nn::LinearConfig::new(16, 1).init(device);
        Self { l1, l2 }
    }

    fn forward(&self, x: Tensor<2>) -> Tensor<2> {
        let x = relu(self.l1.forward(x));
        sigmoid(self.l2.forward(x))
    }
}

fn xor_dataset(device: &Device) -> (Tensor<2>, Tensor<2>) {
    let x = vec![0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0];
    let y = vec![0.0, 1.0, 1.0, 0.0];

    let x = Tensor::<2>::from_data(TensorData::new(x, [4, 2]), device);
    let y = Tensor::<2>::from_data(TensorData::new(y, [4, 1]), device);
    (x, y)
}

pub fn xor_demo(
    epochs: usize,
    lr: f64,
    plot_path: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    let device = training_device();
    let mut model = XorNet::new(&device);
    let mut optim = AdamConfig::new().init();

    let (x, y) = xor_dataset(&device);
    let mut losses = Vec::with_capacity(epochs);

    for epoch in 0..epochs {
        let pred = model.forward(x.clone());

        let loss = losses::binary_cross_entropy(pred, y.clone());

        let grads = GradientsParams::from_grads(loss.backward(), &model);
        model = optim.step(lr, model, grads);

        let l = loss.into_scalar::<f32>();
        losses.push(l);
        if epoch % 200 == 0 {
            println!("epoch {epoch} loss={l:.6}");
        }
    }

    if let Some(path) = plot_path {
        plot::plot_loss_curve(path, &losses, "XOR loss")?;
    }

    Ok(())
}

pub fn yolo_tiny_demo(
    height: usize,
    width: usize,
    num_classes: usize,
    num_anchors: usize,
    train_steps: usize,
    lr: f64,
) -> anyhow::Result<()> {
    use crate::burn_demos::yolo::YoloTiny;

    let device = training_device();
    let mut model = YoloTiny::new(&device, num_classes, num_anchors);
    let mut optim = AdamConfig::new().init();

    let x = YoloTiny::demo_input(&device, 1, height, width);

    // Forward pass.
    let y = model.forward(x.clone());
    println!("yolo_tiny output shape: {:?}", y.dims());

    // Optional tiny training steps (dummy loss).
    for step in 0..train_steps {
        let pred = model.forward(x.clone());
        let loss = pred.mean();
        let grads = GradientsParams::from_grads(loss.backward(), &model);
        model = optim.step(lr, model, grads);
        println!(
            "train step {step}/{train_steps} loss={:.6}",
            loss.into_scalar::<f32>()
        );
    }

    Ok(())
}

pub fn save_load_demo() -> anyhow::Result<()> {
    let device = inference_device();

    // Create a tiny model, run forward, save it as burnpack (0.22 dropped the Recorder API).
    let model = LinearRegressor::new(&device);
    let x = Tensor::<2>::from_data(TensorData::new(vec![1.0], [1, 1]), &device);
    let y0 = model.forward(x.clone()).to_data();

    let tmp_dir = tempfile::tempdir().context("create temp dir")?;
    let path = tmp_dir.path().join("linear-regressor.bpk");

    model.save_file(&path).context("save model")?;

    // Load into a fresh model.
    let model2 = LinearRegressor::new(&device)
        .try_load_file(&path)
        .context("load model")?;
    let y1 = model2.forward(x).to_data();

    println!("save/load ok: before={y0} after={y1}");
    Ok(())
}
