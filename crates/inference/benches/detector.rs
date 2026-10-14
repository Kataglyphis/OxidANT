//! A whole detection on the real model (preprocessing, the model, postprocessing), as ObservANT runs it.
use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use kataglyphis_inference::person_detection::{resolve_model_path, PersonDetector};

fn detector(c: &mut Criterion) {
    let path = resolve_model_path(None);
    if !std::path::Path::new(&path).is_file() {
        eprintln!("SKIP: no model at {path} (scripts/linux/fetch-models.sh fetches it)");
        return;
    }
    // ObservANT's own setting (inference_threads = 2), unless the caller asks for another.
    if std::env::var_os("KATAGLYPHIS_ORT_THREADS").is_none() {
        std::env::set_var("KATAGLYPHIS_ORT_THREADS", "2");
    }
    let (w, h) = (640u32, 480u32);
    let rgba: Vec<u8> = (0..w * h * 4).map(|i| (i % 251) as u8).collect();
    let mut detector = PersonDetector::new(&path).unwrap();
    let mut group = c.benchmark_group("detector");
    group.sample_size(10);
    group.bench_function("deim_dfine_n_640x480", |b| {
        b.iter(|| {
            detector
                .infer_rgba(black_box(&rgba), w, h, 0.25, None)
                .unwrap()
        });
    });
    group.finish();
}

criterion_group!(benches, detector);
criterion_main!(benches);
