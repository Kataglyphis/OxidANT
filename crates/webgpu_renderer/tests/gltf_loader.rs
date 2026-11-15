//! The glTF loader against malformed and unusual files, each built in memory as a GLB.

use kataglyphis_webgpu_renderer::asset::gltf_loader::load_gltf_slice;
use kataglyphis_webgpu_renderer::{ForwardRenderer, GpuContext, OrbitCamera};

const TRIANGLE: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];

fn f32_bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn u32_bytes(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// A GLB's binary chunk plus the buffer views and accessors that describe it.
#[derive(Default)]
struct Bin {
    bytes: Vec<u8>,
    views: Vec<String>,
    accessors: Vec<String>,
}

impl Bin {
    /// Appends one accessor over `data` in its own buffer view; returns the accessor index.
    fn accessor(
        &mut self,
        data: &[u8],
        component: u32,
        count: usize,
        kind: &str,
        extra: &str,
    ) -> usize {
        while !self.bytes.len().is_multiple_of(4) {
            self.bytes.push(0);
        }
        self.views.push(format!(
            r#"{{"buffer":0,"byteOffset":{},"byteLength":{}}}"#,
            self.bytes.len(),
            data.len()
        ));
        self.bytes.extend_from_slice(data);
        self.accessors.push(format!(
            r#"{{"bufferView":{},"componentType":{component},"count":{count},"type":"{kind}"{extra}}}"#,
            self.views.len() - 1
        ));
        self.accessors.len() - 1
    }

    /// A VEC3 float accessor with the min/max POSITION requires.
    fn vec3(&mut self, values: &[[f32; 3]]) -> usize {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in values {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        let bounds = format!(
            r#","min":[{},{},{}],"max":[{},{},{}]"#,
            lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]
        );
        self.accessor(
            &f32_bytes(values.as_flattened()),
            5126,
            values.len(),
            "VEC3",
            &bounds,
        )
    }

    fn scalars(&mut self, values: &[f32]) -> usize {
        self.accessor(&f32_bytes(values), 5126, values.len(), "SCALAR", "")
    }

    fn indices(&mut self, values: &[u32]) -> usize {
        self.accessor(&u32_bytes(values), 5125, values.len(), "SCALAR", "")
    }

    /// The GLB; `rest` holds every top-level member after asset, buffers, bufferViews and accessors.
    fn glb(&self, rest: &str) -> Vec<u8> {
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{}}}],"bufferViews":[{}],"accessors":[{}],{rest}}}"#,
            self.bytes.len(),
            self.views.join(","),
            self.accessors.join(",")
        );
        let mut json = json.into_bytes();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let mut bin = self.bytes.clone();
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let total = 12 + 8 + json.len() + 8 + bin.len();
        let mut out = b"glTF".to_vec();
        for word in [2, total, json.len()] {
            out.extend_from_slice(&(word as u32).to_le_bytes());
        }
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&json);
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&bin);
        out
    }
}

/// One node holding one mesh of `primitives` (JSON objects), as the scene's only root.
fn one_mesh(primitives: &[String]) -> String {
    format!(
        r#""meshes":[{{"primitives":[{}]}}],"nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0"#,
        primitives.join(",")
    )
}

#[test]
fn a_primitive_without_a_surviving_triangle_is_skipped() {
    let mut bin = Bin::default();
    let pos = bin.vec3(&TRIANGLE);
    let good = bin.indices(&[0, 1, 2]);
    let out_of_range = bin.indices(&[7, 8, 9]);
    let short_strip = bin.indices(&[0, 1]);
    let glb = bin.glb(&one_mesh(&[
        format!(r#"{{"attributes":{{"POSITION":{pos}}},"indices":{good}}}"#),
        format!(r#"{{"attributes":{{"POSITION":{pos}}},"indices":{out_of_range}}}"#),
        format!(r#"{{"attributes":{{"POSITION":{pos}}},"indices":{short_strip},"mode":5}}"#),
    ]));
    let scene = load_gltf_slice(&glb).expect("one good primitive is enough to load");
    assert_eq!(
        scene.primitives.len(),
        1,
        "a primitive left without a triangle must not load"
    );
    assert_eq!(scene.primitives[0].indices, vec![0, 1, 2]);
}

/// A scene built in code can still hold empty lists; the first frame must not panic on them.
#[test]
fn empty_primitives_upload_and_render_without_a_panic() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut bin = Bin::default();
    let pos = bin.vec3(&TRIANGLE);
    let glb = bin.glb(&one_mesh(&[format!(
        r#"{{"attributes":{{"POSITION":{pos}}}}}"#
    )]));
    let mut scene = load_gltf_slice(&glb).expect("triangle must load");
    let mut no_indices = scene.primitives[0].clone();
    no_indices.indices.clear();
    let mut nothing = no_indices.clone();
    nothing.vertices.clear();
    scene.primitives.extend([no_indices, nothing]);

    let mut renderer = ForwardRenderer::new(&gpu, 64, 64);
    renderer.upload_scene(&gpu, &scene);
    renderer
        .render_to_pixels(&gpu, 64, 64, &OrbitCamera::default())
        .expect("a frame with empty primitives must render");
}

/// One triangle mesh hung from `nodes` (JSON objects), the scene's roots being `roots`.
fn triangle_under(nodes: &str, roots: &str) -> Vec<u8> {
    let mut bin = Bin::default();
    let pos = bin.vec3(&TRIANGLE);
    bin.glb(&format!(
        r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos}}}}}]}}],"nodes":[{nodes}],"scenes":[{{"nodes":[{roots}]}}],"scene":0"#
    ))
}

/// gltf validates index ranges only; a cycle recursed until the stack overflowed (an abort, not an error).
#[test]
fn node_cycles_load_each_node_once() {
    let two_cycle = triangle_under(r#"{"children":[1]},{"children":[0],"mesh":0}"#, "0");
    let scene = load_gltf_slice(&two_cycle).expect("a cycle is skipped, not fatal");
    assert_eq!(scene.primitives.len(), 1, "the mesh node is visited once");

    let self_parent = triangle_under(r#"{"children":[0],"mesh":0}"#, "0");
    let scene = load_gltf_slice(&self_parent).expect("a self-parented node is skipped, not fatal");
    assert_eq!(scene.primitives.len(), 1);
}

/// Valid but deep: a recursive walk needs one stack frame per level.
#[test]
fn a_deep_node_chain_loads_without_overflowing_the_stack() {
    const DEPTH: usize = 50_000;
    let mut nodes: Vec<String> = (1..DEPTH)
        .map(|child| format!(r#"{{"children":[{child}],"translation":[0,1,0]}}"#))
        .collect();
    nodes.push(r#"{"mesh":0}"#.to_string());
    let scene = load_gltf_slice(&triangle_under(&nodes.join(","), "0")).expect("deep chain");
    assert_eq!(scene.primitives.len(), 1);
    let lifted = scene.primitives[0].transform.w_axis.y;
    assert_eq!(
        lifted,
        (DEPTH - 1) as f32,
        "every level adds its translation"
    );
}

/// A NaN time defeats both end tests of the keyframe search, which then underflowed on every frame.
#[test]
fn animation_channels_with_bad_keyframes_are_dropped() {
    let mut bin = Bin::default();
    let pos = bin.vec3(&TRIANGLE);
    let nan_times = bin.scalars(&[f32::NAN, 1.0]);
    let backwards = bin.scalars(&[1.0, 0.0]);
    let good_times = bin.scalars(&[0.0, 1.0]);
    let two_keys = bin.vec3(&[[0.0; 3], [1.0; 3]]);
    let one_key = bin.vec3(&[[0.0; 3]]);
    let samplers = format!(
        r#"{{"input":{nan_times},"output":{two_keys}}},{{"input":{backwards},"output":{two_keys}}},{{"input":{good_times},"output":{one_key}}},{{"input":{good_times},"output":{two_keys}}}"#
    );
    let channels = r#"{"sampler":0,"target":{"node":0,"path":"translation"}},{"sampler":1,"target":{"node":0,"path":"scale"}},{"sampler":2,"target":{"node":1,"path":"translation"}},{"sampler":3,"target":{"node":1,"path":"scale"}}"#;
    let glb = bin.glb(&format!(
        r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":{pos}}}}}]}}],"nodes":[{{"mesh":0}},{{}}],"scenes":[{{"nodes":[0,1]}}],"scene":0,"animations":[{{"samplers":[{samplers}],"channels":[{channels}]}}]"#
    ));
    let scene = load_gltf_slice(&glb).expect("bad channels are dropped, not fatal");
    assert_eq!(scene.animations.len(), 1);
    let kept = &scene.animations[0].channels;
    assert_eq!(
        kept.len(),
        1,
        "only the channel with sorted, finite times and enough values stays"
    );
    assert_eq!(kept[0].times, vec![0.0, 1.0]);
}
