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
    /// Appends `data` as its own buffer view (an embedded image); returns the view index.
    fn view(&mut self, data: &[u8]) -> usize {
        while !self.bytes.len().is_multiple_of(4) {
            self.bytes.push(0);
        }
        self.views.push(format!(
            r#"{{"buffer":0,"byteOffset":{},"byteLength":{}}}"#,
            self.bytes.len(),
            data.len()
        ));
        self.bytes.extend_from_slice(data);
        self.views.len() - 1
    }

    /// Appends one accessor over `data` in its own buffer view; returns the accessor index.
    fn accessor(
        &mut self,
        data: &[u8],
        component: u32,
        count: usize,
        kind: &str,
        extra: &str,
    ) -> usize {
        let view = self.view(data);
        self.accessors.push(format!(
            r#"{{"bufferView":{view},"componentType":{component},"count":{count},"type":"{kind}"{extra}}}"#
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

    fn vec2(&mut self, values: &[[f32; 2]]) -> usize {
        self.accessor(
            &f32_bytes(values.as_flattened()),
            5126,
            values.len(),
            "VEC2",
            "",
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

/// The 8 corners of a unit cube, index = x + 2y + 4z, and its 12 triangles.
fn indexed_cube() -> (Vec<[f32; 3]>, Vec<u32>) {
    let corners = (0..8)
        .map(|i| {
            let bit = |b: u32| if i & b != 0 { 0.5 } else { -0.5 };
            [bit(1), bit(2), bit(4)]
        })
        .collect();
    let quads = [
        [0, 4, 6, 2],
        [1, 3, 7, 5],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 2, 3, 1],
        [4, 5, 7, 6],
    ];
    let indices = quads
        .iter()
        .flat_map(|[a, b, c, d]| [*a, *b, *c, *a, *c, *d])
        .collect();
    (corners, indices)
}

fn face_normal(prim: &kataglyphis_webgpu_renderer::CpuPrimitive, tri: &[u32; 3]) -> glam::Vec3 {
    let p = |i: u32| glam::Vec3::from_array(prim.vertices[i as usize].position);
    (p(tri[1]) - p(tri[0]))
        .cross(p(tri[2]) - p(tri[0]))
        .normalize()
}

/// glTF: without NORMAL a client MUST use flat normals, which shared corners cannot carry.
#[test]
fn an_indexed_mesh_without_normals_is_flat_shaded() {
    let (corners, indices) = indexed_cube();
    let deltas: Vec<[f32; 3]> = corners.iter().map(|p| p.map(|c| 2.0 * c)).collect();
    let mut bin = Bin::default();
    let pos = bin.vec3(&corners);
    let idx = bin.indices(&indices);
    let target = bin.vec3(&deltas);
    let glb = bin.glb(&one_mesh(&[format!(
        r#"{{"attributes":{{"POSITION":{pos}}},"indices":{idx},"targets":[{{"POSITION":{target}}}]}}"#
    )]));
    let scene = load_gltf_slice(&glb).expect("cube must load");
    let prim = &scene.primitives[0];
    for tri in prim.indices.as_chunks::<3>().0 {
        let face = face_normal(prim, tri);
        for &corner in tri {
            let n = glam::Vec3::from_array(prim.vertices[corner as usize].normal);
            assert!(
                n.dot(face) > 0.999,
                "corner normal {n:?} is not its face's {face:?}"
            );
        }
    }
    // Rebuilding the vertices must carry every morph delta with its vertex.
    let target = &prim.morph_targets[0].position_deltas;
    assert_eq!(target.len(), prim.vertices.len());
    for (vertex, delta) in prim.vertices.iter().zip(target) {
        assert_eq!(delta.to_array(), vertex.position.map(|c| 2.0 * c));
    }
}

/// `normalize(0)` is NaN in the shader, and it spreads over every triangle touching the vertex.
#[test]
fn degenerate_faces_and_zero_normals_leave_no_vertex_without_a_unit_frame() {
    let strip = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
        [2.0, 0.0, 0.0],
        [2.0, 1.0, 0.0],
        [3.0, 0.0, 0.0],
    ];
    let mut bin = Bin::default();
    let strip_pos = bin.vec3(&strip);
    let stitched = bin.indices(&[0, 1, 2, 3, 3, 4, 4, 5, 6]);
    let tri_pos = bin.vec3(&TRIANGLE);
    let zero_normal = bin.vec3(&[[0.0, 0.0, 1.0], [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]]);
    let glb = bin.glb(&one_mesh(&[
        format!(r#"{{"attributes":{{"POSITION":{strip_pos}}},"indices":{stitched},"mode":5}}"#),
        format!(r#"{{"attributes":{{"POSITION":{tri_pos},"NORMAL":{zero_normal}}}}}"#),
    ]));
    let scene = load_gltf_slice(&glb).expect("both primitives must load");
    assert_eq!(scene.primitives.len(), 2);
    for (p, prim) in scene.primitives.iter().enumerate() {
        for (i, v) in prim.vertices.iter().enumerate() {
            let n = glam::Vec3::from_array(v.normal);
            let t = glam::Vec3::new(v.tangent[0], v.tangent[1], v.tangent[2]);
            assert!(
                (n.length() - 1.0).abs() < 1e-3,
                "primitive {p} vertex {i}: normal {n:?} is not unit"
            );
            assert!(
                t.is_finite() && (t.length() - 1.0).abs() < 1e-3,
                "primitive {p} vertex {i}: tangent {t:?} is not unit"
            );
            assert!(v.tangent[3] == 1.0 || v.tangent[3] == -1.0);
        }
    }
}

/// A 1x1 flat-normal PNG, so a material can name a normal texture.
fn flat_normal_png() -> Vec<u8> {
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(1, 1, image::Rgba([128, 128, 255, 255]))
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("a 1x1 PNG encodes");
    bytes
}

/// glTF generates tangents from the normal texture's UV set, here TEXCOORD_1.
#[test]
fn generated_tangents_follow_the_normal_textures_uv_set() {
    let quad = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    // UV1's u runs along +X; UV0 is it turned a quarter, so its u runs along +Y.
    let uv1: Vec<[f32; 2]> = quad.iter().map(|p| [p[0], p[1]]).collect();
    let uv0: Vec<[f32; 2]> = quad.iter().map(|p| [p[1], 1.0 - p[0]]).collect();
    let mut bin = Bin::default();
    let pos = bin.vec3(&quad);
    let normal = bin.vec3(&[[0.0, 0.0, 1.0]; 4]);
    let (t0, t1) = (bin.vec2(&uv0), bin.vec2(&uv1));
    let idx = bin.indices(&[0, 1, 2, 0, 2, 3]);
    let png = bin.view(&flat_normal_png());
    let glb = bin.glb(&format!(
        r#"{},"materials":[{{"normalTexture":{{"index":0,"texCoord":1}}}}],"textures":[{{"source":0}}],"images":[{{"bufferView":{png},"mimeType":"image/png"}}]"#,
        one_mesh(&[format!(
            r#"{{"attributes":{{"POSITION":{pos},"NORMAL":{normal},"TEXCOORD_0":{t0},"TEXCOORD_1":{t1}}},"indices":{idx},"material":0}}"#
        )])
    ));
    let scene = load_gltf_slice(&glb).expect("quad must load");
    for v in &scene.primitives[0].vertices {
        assert!(
            v.tangent[0] > 0.999,
            "the tangent must follow UV1's u (+X), got {:?}",
            v.tangent
        );
    }
}
