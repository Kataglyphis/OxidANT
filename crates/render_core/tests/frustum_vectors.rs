//! The frustum verdicts BeschleunigerBallett's `frustumSuite.cpp` reads too, against this crate's `Frustum`.

use std::collections::HashMap;
use std::path::Path;

use glam::camera::rh::proj::directx as clip;
use glam::camera::rh::view::look_at_mat4;
use glam::{Mat4, Vec3};
use kataglyphis_render_core::frustum::Frustum;
use kataglyphis_render_core::lens;
use kataglyphis_render_core::viewport::SceneViewport;
use serde_json::Value;

fn vectors() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors/frustum_verdicts.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn float(value: &Value, key: &str) -> f32 {
    value[key]
        .as_f64()
        .unwrap_or_else(|| panic!("{key}: {value}")) as f32
}

fn vec3(value: &Value, key: &str) -> Vec3 {
    let axes: Vec<f32> = value[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key}: {value}"))
        .iter()
        .map(|axis| axis.as_f64().unwrap() as f32)
        .collect();
    Vec3::from_slice(&axes)
}

/// A frustum's view-projection, built the way each renderer builds its own.
fn view_projection(frustum: &Value) -> Mat4 {
    let view = look_at_mat4(
        vec3(frustum, "eye"),
        vec3(frustum, "target"),
        vec3(frustum, "up"),
    );
    let projection = match frustum["kind"].as_str() {
        Some("perspective") => lens::perspective(
            float(frustum, "fov_y_deg"),
            float(frustum, "aspect"),
            float(frustum, "near"),
            float(frustum, "far"),
            SceneViewport::FULL,
        ),
        Some("orthographic") => clip::orthographic(
            float(frustum, "left"),
            float(frustum, "right"),
            float(frustum, "bottom"),
            float(frustum, "top"),
            float(frustum, "near"),
            float(frustum, "far"),
        ),
        other => panic!("an unknown frustum kind {other:?}"),
    };
    projection * view
}

#[test]
fn every_shared_verdict_holds() {
    let vectors = vectors();
    let frusta: HashMap<&str, Frustum> = vectors["frusta"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, frustum)| {
            let planes = Frustum::from_view_proj(&view_projection(frustum));
            (name.as_str(), planes)
        })
        .collect();
    let cases = vectors["cases"].as_array().unwrap();
    let mut verdicts = [0usize; 4];
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let frustum = &frusta[case["frustum"].as_str().unwrap()];
        let (min, max) = (vec3(case, "min"), vec3(case, "max"));
        let visible = case["visible"].as_bool().unwrap();
        let caster = case["caster"].as_bool().unwrap();
        assert_eq!(
            frustum.intersects_aabb(min, max),
            visible,
            "{name}: visible"
        );
        assert_eq!(
            frustum.intersects_aabb_as_caster(min, max),
            caster,
            "{name}: caster"
        );
        verdicts[usize::from(visible) * 2 + usize::from(caster)] += 1;
    }
    // Culled, caster only, both: a file that lost a kind of verdict would pass without them.
    assert!(
        verdicts[0] > 0 && verdicts[1] > 0 && verdicts[3] > 0,
        "{verdicts:?}"
    );
    assert_eq!(verdicts[2], 0, "a visible box always casts");
    assert_eq!(frusta.len(), 2);
}
