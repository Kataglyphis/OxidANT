//! What a frame pays before the model, at 640x480: conversion, privacy blocks, a box, the snapshot JPEG, the motion look.
use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use kataglyphis_observant::{frame, motion, status};

fn frame_stages(c: &mut Criterion) {
    let (w, h) = (640u32, 480u32);
    let i420: Vec<u8> = (0..frame::size(w, h)).map(|i| (i % 253) as u8).collect();
    let rgba = frame::to_rgba(&i420, w, h);
    let boxes = [[200.0, 150.0, 420.0, 330.0]];
    let luma = &i420[..(w * h) as usize];
    let mut group = c.benchmark_group("frame_640x480");
    group.bench_function("i420_to_rgba", |b| {
        b.iter(|| frame::to_rgba(black_box(&i420), w, h))
    });
    group.bench_function("pixelate_outside_one_box", |b| {
        let mut work = i420.clone();
        b.iter(|| frame::pixelate_outside(black_box(&mut work), w, h, &boxes));
    });
    group.bench_function("draw_box", |b| {
        let mut work = i420.clone();
        b.iter(|| frame::draw_rect(black_box(&mut work), w, h, boxes[0], frame::GREEN, 4));
    });
    group.bench_function("snapshot_jpeg", |b| {
        b.iter(|| status::encode_jpeg(black_box(&rgba), w, h).unwrap())
    });
    group.bench_function("motion_thumbnail", |b| {
        b.iter(|| motion::thumbnail(black_box(luma), w, h))
    });
    group.finish();
}

criterion_group!(benches, frame_stages);
criterion_main!(benches);
