//! Builds the tracer's scene from a glTF and reports it: `cargo run --release -p kataglyphis_webgpu_renderer --example trace_build -- model.gltf`.

use std::sync::Arc;
use std::time::Instant;

use kataglyphis_webgpu_renderer::render::tracer::limits::{check, TraceBufferSizes};
use kataglyphis_webgpu_renderer::render::tracer::{
    TraceBuildOptions, TraceGpuScene, TraceSceneBuilder,
};
use kataglyphis_webgpu_renderer::{load_gltf, GpuContext};

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let Some(path) = std::env::args().nth(1) else {
        anyhow::bail!("usage: trace_build <model.gltf|model.glb>");
    };
    let start = Instant::now();
    let scene = load_gltf(&path)?;
    println!(
        "loaded {path} in {:.2} s: {} primitives, {} triangles",
        start.elapsed().as_secs_f64(),
        scene.primitives.len(),
        scene.triangle_count()
    );
    let options = TraceBuildOptions::default();
    let start = Instant::now();
    let Some(traced) = TraceSceneBuilder::new(Arc::new(scene), Vec::new(), options).run() else {
        anyhow::bail!("the build was cancelled");
    };
    let seconds = start.elapsed().as_secs_f64();
    let stats = &traced.stats;
    println!(
        "built in {seconds:.2} s on {} threads: {} triangles ({} stored) in {} instances of {} meshes, {} nodes, {} materials",
        options.threads, stats.triangles, stats.unique_triangles, stats.instances, stats.meshes, stats.nodes, stats.materials
    );
    if let Some([gather, bvh, textures, assemble]) = stats.stage_seconds {
        println!("  gathering {gather:.2} s, BVHs {bvh:.2} s, textures {textures:.2} s, assembling {assemble:.2} s");
    }
    println!(
        "texture layers: {} sRGB at {}, {} linear at {}; {:.1} MiB for the GPU",
        stats.layers[0],
        stats.layer_sizes[0],
        stats.layers[1],
        stats.layer_sizes[1],
        stats.gpu_bytes as f64 / f64::from(1u32 << 20)
    );
    for (name, bytes) in TraceBufferSizes::of(&traced).named() {
        println!("  {name}: {:.1} MiB", bytes as f64 / f64::from(1u32 << 20));
    }
    match check(&traced, &wgpu::Limits::default()) {
        Ok(()) => println!("fits WebGPU's default limits (the browser)"),
        Err(error) => println!("past WebGPU's default limits (the browser): {error}"),
    }
    match GpuContext::new_headless() {
        Ok(gpu) => match TraceGpuScene::upload(&gpu, &traced) {
            Ok(_) => println!("uploaded to {}", gpu.adapter_info.name),
            Err(error) => println!("not uploaded to {}: {error}", gpu.adapter_info.name),
        },
        Err(error) => println!("no GPU to upload to: {error:#}"),
    }
    Ok(())
}
