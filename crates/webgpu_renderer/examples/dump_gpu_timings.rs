//! Renders headlessly and writes per-pass GPU timings as JSON in the C++ engine's schema.
//! Usage: `cargo run --example dump_gpu_timings -- <out.json> [scene.gltf [width height]] [--deferred] [--clouds|--half-res-clouds]`
//! `frames_measured` counts the frames whose timestamps arrived, 0 without timestamps; a pass is the renderer's mean.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use glam::Vec3;
use kataglyphis_webgpu_renderer::render::clouds::CloudSettings;
use kataglyphis_webgpu_renderer::render::forward::RasterPath;
use kataglyphis_webgpu_renderer::render::gpu_timing::TimedPass;
use kataglyphis_webgpu_renderer::{
    load_gltf, ForwardRenderer, GpuContext, OrbitCamera, TonemapPass,
};

/// Matches the C++ export; tests/gpu_timing.rs says why 64 is the minimum that yields averages.
const FRAMES: u32 = 96;

/// Each pass's mean GPU milliseconds, by name.
type Passes = Vec<(&'static str, f32)>;

fn main() -> anyhow::Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // The C++ export times its forward path, so the deferred one is asked for by name.
    let flag = |name: &str| args.iter().any(|arg| arg == name);
    let path = if flag("--deferred") {
        RasterPath::Deferred
    } else {
        RasterPath::Forward
    };
    let clouds = (flag("--clouds") || flag("--half-res-clouds")).then(|| flag("--half-res-clouds"));
    args.retain(|arg| !arg.starts_with("--"));
    let mut args = args.into_iter();
    let out_path = args
        .next()
        .unwrap_or_else(|| "gpu-timings-rust.json".to_string());
    let scene_path = args.next().map(PathBuf::from).unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/tests/assets/cube.gltf")
    });
    let width: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1280);
    let height: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(720);

    let Ok(gpu) = GpuContext::new_headless() else {
        eprintln!("no GPU adapter; writing an unsupported dump");
        return write(&out_path, &timings_json(false, 0, &[]));
    };
    gpu.check_target_size(width, height)?;
    let (supported, measured, passes) =
        measure(&gpu, &scene_path, [width, height], (path, clouds))?;
    write(&out_path, &timings_json(supported, measured, &passes))?;
    eprintln!("wrote {out_path} ({} passes)", passes.len());
    Ok(())
}

/// Renders [`FRAMES`] frames: whether timestamps work, how many frames delivered them, and each pass's mean.
/// `clouds` puts the camera inside a thin cloud box, so every pixel marches; `Some(true)` at half resolution.
fn measure(
    gpu: &GpuContext,
    scene: &Path,
    [width, height]: [u32; 2],
    (path, clouds): (RasterPath, Option<bool>),
) -> anyhow::Result<(bool, u32, Passes)> {
    let mut renderer = ForwardRenderer::new(gpu, width, height);
    renderer.raster_path = path;
    if let Some(half_resolution) = clouds {
        renderer.clouds = CloudSettings {
            enabled: true,
            coverage: 0.3,
            half_resolution,
            ..CloudSettings::default()
        };
        renderer
            .clouds
            .place_box(Vec3::ZERO, Vec3::splat(20.0), 0.05);
    }
    let supported = renderer.enable_gpu_timing(gpu);
    renderer.upload_scene(gpu, &load_gltf(scene).context("scene must load")?);
    let camera = OrbitCamera::default();
    let mut tonemap = TonemapPass::new(gpu, wgpu::TextureFormat::Rgba8UnormSrgb);
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("timing_dump_target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut measured = 0;
    for _ in 0..FRAMES {
        renderer.render_tonemapped(gpu, &mut tonemap, &view, width, height, &camera);
        // Headless: nothing presents, so the map callbacks need a poll to run.
        let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
        // With that wait each frame's stamps land one frame later, so this counts the frames that delivered some.
        if !renderer.gpu_timings_ms().is_empty() {
            measured += 1;
        }
    }
    Ok((supported, measured, renderer.gpu_timings_ms()))
}

/// The C++ export's JSON: without timestamps no frame was measured, however many ran; `nested_in` names each nested
/// pass's parent, so a frame sum adds the top level only.
fn timings_json(supported: bool, frames_with_samples: u32, passes: &[(&str, f32)]) -> String {
    let frames = if supported { frames_with_samples } else { 0 };
    let body: Vec<String> = passes
        .iter()
        .map(|(name, ms)| format!("    \"{name}\": {ms}"))
        .collect();
    let nested: Vec<String> = passes
        .iter()
        .filter_map(|(name, _)| parent(name).map(|parent| format!("    \"{name}\": \"{parent}\"")))
        .collect();
    format!(
        "{{\n  \"frames_measured\": {frames},\n  \"timestamps_supported\": {supported},\n  \"passes\": {{\n{}\n  }},\n  \"nested_in\": {{\n{}\n  }}\n}}\n",
        body.join(",\n"),
        nested.join(",\n")
    )
}

fn parent(name: &str) -> Option<&'static str> {
    let pass = TimedPass::ALL
        .into_iter()
        .find(|pass| pass.name() == name)?;
    pass.nested_in().map(TimedPass::name)
}

fn write(path: &str, json: &str) -> anyhow::Result<()> {
    std::fs::write(path, json).with_context(|| format!("could not write {path}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_timestamps_no_frame_counts_as_measured() {
        let json = timings_json(false, FRAMES, &[]);
        assert!(json.contains("\"frames_measured\": 0,"), "{json}");
        assert!(json.contains("\"timestamps_supported\": false,"), "{json}");
        assert!(json.contains("\"passes\": {\n\n  }"), "{json}");
    }

    #[test]
    fn a_nested_pass_names_its_parent_as_the_cpp_export_does() {
        let json = timings_json(
            true,
            95,
            &[("Main", 2.0), ("Forward", 1.5), ("Tonemap", 0.25)],
        );
        let doc: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(doc["passes"]["Main"], 2.0, "{json}");
        assert_eq!(doc["nested_in"]["Forward"], "Main", "{json}");
        let nested = doc["nested_in"].as_object().expect("an object");
        assert_eq!(nested.len(), 1, "a top-level pass names no parent: {json}");
    }

    #[test]
    fn with_timestamps_the_count_and_every_pass_are_written() {
        let json = timings_json(true, 95, &[("shadow", 0.5), ("forward", 1.25)]);
        assert!(json.contains("\"frames_measured\": 95,"), "{json}");
        assert!(
            json.contains("\"shadow\": 0.5,\n    \"forward\": 1.25\n"),
            "{json}"
        );
    }
}
