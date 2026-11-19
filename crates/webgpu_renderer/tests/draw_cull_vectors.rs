//! `draw_cull.slang` against the frustum verdicts render_core's `Frustum` and BeschleunigerBallett's engine reach.

use glam::camera::rh::proj::directx as clip;
use glam::camera::rh::view::look_at_mat4;
use glam::{Mat4, Vec3};
use kataglyphis_render_core::lens;
use kataglyphis_render_core::viewport::SceneViewport;
use kataglyphis_webgpu_renderer::render::draw_cull::{
    CullFrame, DrawCull, DrawInput, OcclusionSource, DRAW_CASTS_SHADOW,
};
use kataglyphis_webgpu_renderer::render::gpu_timing::PassScope;
use kataglyphis_webgpu_renderer::GpuContext;
use serde_json::Value;

/// Embedded, as `frustum_vectors.rs` embeds them, for a device without the source tree.
fn vectors() -> Value {
    serde_json::from_str(include_str!(
        "../../render_core/tests/vectors/frustum_verdicts.json"
    ))
    .unwrap()
}

fn float(value: &Value, key: &str) -> f32 {
    value[key].as_f64().unwrap() as f32
}

fn vec3(value: &Value, key: &str) -> Vec3 {
    let axes: Vec<f32> = serde_json::from_value(value[key].clone()).unwrap();
    Vec3::from_slice(&axes)
}

/// The view-projection `frustum_vectors.rs` builds for the same entry.
fn view_projection(frustum: &Value) -> Mat4 {
    if frustum["kind"] == "matrix" {
        let rows: [[f32; 4]; 4] = serde_json::from_value(frustum["rows"].clone()).unwrap();
        return Mat4::from_cols_array_2d(&rows).transpose();
    }
    let view = look_at_mat4(
        vec3(frustum, "eye"),
        vec3(frustum, "target"),
        vec3(frustum, "up"),
    );
    let projection = if frustum["kind"] == "perspective" {
        lens::perspective(
            float(frustum, "fov_y_deg"),
            float(frustum, "aspect"),
            float(frustum, "near"),
            float(frustum, "far"),
            SceneViewport::FULL,
        )
    } else {
        clip::orthographic(
            float(frustum, "left"),
            float(frustum, "right"),
            float(frustum, "bottom"),
            float(frustum, "top"),
            float(frustum, "near"),
            float(frustum, "far"),
        )
    };
    projection * view
}

/// Instances each slot of `view` (0 the camera, 1 the one cascade) keeps after one dispatch over `boxes`.
fn instances(gpu: &GpuContext, view_proj: Mat4, boxes: &[(Vec3, Vec3)]) -> [Vec<u32>; 2] {
    let inputs: Vec<DrawInput> = boxes
        .iter()
        .enumerate()
        .map(|(i, (min, max))| DrawInput {
            aabb_min: min.extend(0.0).to_array(),
            aabb_max: max.extend(0.0).to_array(),
            primitive: i as u32,
            flags: DRAW_CASTS_SHADOW,
            camera_index_count: 3,
            shadow_index_count: 3,
            instance_count: 1,
            _pad: [0; 3],
        })
        .collect();
    let mut cull = DrawCull::new(&gpu.device);
    cull.upload(gpu, &inputs);
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    let frame = CullFrame {
        camera: view_proj,
        cascades: &[view_proj],
        eye: Vec3::ZERO,
        occlusion_eye: Vec3::ZERO,
        frustum_culling: true,
        occlusion: OcclusionSource::None,
    };
    cull.encode(gpu, &mut encoder, &frame, PassScope::disabled());
    gpu.queue.submit(Some(encoder.finish()));
    [0, 1].map(|view| {
        let args = cull.read_args(gpu, view).expect("args readback");
        args.iter().map(|a| a.instance_count).collect()
    })
}

#[test]
fn the_gpu_cull_reaches_every_shared_verdict() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let vectors = vectors();
    let cases = vectors["cases"].as_array().unwrap();
    let mut checked = 0;
    for (name, frustum) in vectors["frusta"].as_object().unwrap() {
        let mine: Vec<&Value> = cases.iter().filter(|c| c["frustum"] == *name).collect();
        let boxes: Vec<(Vec3, Vec3)> = mine
            .iter()
            .map(|c| (vec3(c, "min"), vec3(c, "max")))
            .collect();
        let [camera, cascade] = instances(&gpu, view_projection(frustum), &boxes);
        for (i, case) in mine.iter().enumerate() {
            let what = case["name"].as_str().unwrap();
            let visible = u32::from(case["visible"].as_bool().unwrap());
            let caster = u32::from(case["caster"].as_bool().unwrap());
            assert_eq!(camera[i], visible, "{what}: in_camera");
            assert_eq!(cascade[i], caster, "{what}: in_cascade");
            checked += 1;
        }
    }
    assert_eq!(checked, cases.len(), "a case names an unknown frustum");
}
