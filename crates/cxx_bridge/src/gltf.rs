//! The glTF loader for the engine: `kataglyphis_assets::flat`'s arrays, in the C++ namespace `kataglyphis::oxidant::gltf`.

use kataglyphis_assets::flat::{
    load_flat, FlatLight, FlatMaterial, FlatModel, FlatRange, FlatTexture, FlatVertex,
};
use kataglyphis_assets::scene::{CpuSampler, CpuWrap};
use kataglyphis_assets::LoadOptions;

use crate::guarded;

#[cxx::bridge(namespace = "kataglyphis::oxidant::gltf")]
pub mod ffi {
    /// The C++ `Vertex`'s fields in its order and sizes, in world space.
    #[derive(Debug, PartialEq)]
    struct GltfVertex {
        position: [f32; 3],
        normal: [f32; 3],
        color: [f32; 4],
        uv0: [f32; 2],
        tangent: [f32; 4],
        uv1: [f32; 2],
    }

    /// The glTF half of `ObjMaterial`; a texture id indexes `GltfModel::textures`, -1 for none.
    #[derive(Debug, PartialEq)]
    struct GltfMaterial {
        diffuse: [f32; 3],
        dissolve: f32,
        emission: [f32; 3],
        metallic: f32,
        roughness: f32,
        alpha_cutoff: f32,
        normal_scale: f32,
        base_uv_row0: [f32; 3],
        base_uv_row1: [f32; 3],
        metallic_roughness_uv_row0: [f32; 3],
        metallic_roughness_uv_row1: [f32; 3],
        normal_uv_row0: [f32; 3],
        normal_uv_row1: [f32; 3],
        emissive_uv_row0: [f32; 3],
        emissive_uv_row1: [f32; 3],
        base_texture: i32,
        metallic_roughness_texture: i32,
        normal_texture: i32,
        emissive_texture: i32,
        uv1_slot_mask: u32,
        unlit: bool,
        alpha_blend: bool,
        double_sided: bool,
    }

    /// A glTF sampler; a wrap is 0 repeat, 1 mirrored repeat, 2 clamp to edge.
    #[derive(Debug, PartialEq)]
    struct GltfSampler {
        mag_nearest: bool,
        min_nearest: bool,
        mip_nearest: bool,
        /// False for glTF's bare NEAREST and LINEAR minification, which read level 0 only.
        mipmaps: bool,
        wrap_u: u8,
        wrap_v: u8,
    }

    /// One texture slot: the image as the file stores it (PNG, JPEG, ...), for the engine's own decoder.
    #[derive(Debug, PartialEq)]
    struct GltfTexture {
        encoded: Vec<u8>,
        sampler: GltfSampler,
        srgb: bool,
    }

    /// One primitive's slice of the flat arrays; its vertices are the ones its triangles use.
    #[derive(Debug, PartialEq)]
    struct GltfMeshRange {
        vertex_base: usize,
        vertex_count: usize,
        index_start: usize,
        index_count: usize,
        tri_start: usize,
        tri_count: usize,
        double_sided: bool,
        alpha_blended: bool,
    }

    /// A punctual light in world space; `kind` 1 point, 2 spot, 3 directional, 0 for `GltfModel::sun`'s none.
    #[derive(Debug, PartialEq)]
    struct GltfLight {
        kind: u32,
        color: [f32; 3],
        intensity: f32,
        range: f32,
        position: [f32; 3],
        /// The way the light travels, a unit vector.
        direction: [f32; 3],
        cos_inner: f32,
        cos_outer: f32,
    }

    /// The parse result; `ok` false carries `error` and empty arrays, since the engine builds without exceptions.
    #[derive(Debug, PartialEq)]
    struct GltfModel {
        ok: bool,
        error: String,
        vertices: Vec<GltfVertex>,
        indices: Vec<u32>,
        /// One per triangle; the last material is the neutral fallback.
        material_index: Vec<u32>,
        materials: Vec<GltfMaterial>,
        textures: Vec<GltfTexture>,
        mesh_ranges: Vec<GltfMeshRange>,
        /// Every punctual light but the Sun.
        lights: Vec<GltfLight>,
        /// The first directional light, which the engine adopts as its Sun: direction and colour, not the glTF lux.
        has_sun: bool,
        /// Meaningful only with `has_sun`; all zero otherwise.
        sun: GltfLight,
    }

    /// How the loader builds geometry; each field's zero is its default, so `GltfLoadOptions{}` is `load_gltf_flat`'s.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct GltfLoadOptions {
        /// MikkTSpace tangents where the file has none, the basis DCC tools bake against; false keeps Lengyel's.
        mikktspace_tangents: bool,
    }

    extern "Rust" {
        /// Parses the .gltf or .glb at the UTF-8 `path` with OxidANT's loader, flattened for the engine.
        fn load_gltf_flat(path: &[u8]) -> GltfModel;
        /// `load_gltf_flat` with `options`, which the engine reads once at its start; the loader reads no environment.
        fn load_gltf_flat_with(path: &[u8], options: &GltfLoadOptions) -> GltfModel;
    }
}

/// Never unwinds into C++: an error or a panic becomes `ok == false` with the reason.
pub fn load_gltf_flat(path: &[u8]) -> ffi::GltfModel {
    guarded(
        || model(load(path, &LoadOptions::default())),
        loader_panicked,
    )
}

pub fn load_gltf_flat_with(path: &[u8], options: &ffi::GltfLoadOptions) -> ffi::GltfModel {
    guarded(
        || model(load(path, &load_options(options))),
        loader_panicked,
    )
}

fn load_options(options: &ffi::GltfLoadOptions) -> LoadOptions {
    LoadOptions {
        mikktspace_tangents: options.mikktspace_tangents,
    }
}

fn load(path: &[u8], options: &LoadOptions) -> Result<FlatModel, String> {
    let path = std::str::from_utf8(path).map_err(|_| "the path is not UTF-8".to_owned())?;
    load_flat(path, options).map_err(|err| format!("{err:#}"))
}

fn model(loaded: Result<FlatModel, String>) -> ffi::GltfModel {
    match loaded {
        Ok(model) => to_ffi(model, true, String::new()),
        Err(error) => failed(error),
    }
}

fn loader_panicked(reason: String) -> ffi::GltfModel {
    failed(format!("the glTF loader panicked: {reason}"))
}

fn failed(error: String) -> ffi::GltfModel {
    to_ffi(FlatModel::default(), false, error)
}

// Each map keeps its element's size, so collect reuses the allocation instead of copying the arrays again.
fn to_ffi(model: FlatModel, ok: bool, error: String) -> ffi::GltfModel {
    ffi::GltfModel {
        ok,
        error,
        vertices: model.vertices.into_iter().map(vertex).collect(),
        indices: model.indices,
        material_index: model.material_index,
        materials: model.materials.into_iter().map(material).collect(),
        textures: model.textures.into_iter().map(texture).collect(),
        mesh_ranges: model.ranges.into_iter().map(range).collect(),
        lights: model.lights.into_iter().map(light).collect(),
        has_sun: model.sun.is_some(),
        sun: model.sun.map_or_else(no_light, light),
    }
}

fn vertex(v: FlatVertex) -> ffi::GltfVertex {
    ffi::GltfVertex {
        position: v.position,
        normal: v.normal,
        color: v.color,
        uv0: v.uv0,
        tangent: v.tangent,
        uv1: v.uv1,
    }
}

fn material(m: FlatMaterial) -> ffi::GltfMaterial {
    ffi::GltfMaterial {
        diffuse: m.diffuse,
        dissolve: m.dissolve,
        emission: m.emission,
        metallic: m.metallic,
        roughness: m.roughness,
        alpha_cutoff: m.alpha_cutoff,
        normal_scale: m.normal_scale,
        base_uv_row0: m.base_uv[0],
        base_uv_row1: m.base_uv[1],
        metallic_roughness_uv_row0: m.metallic_roughness_uv[0],
        metallic_roughness_uv_row1: m.metallic_roughness_uv[1],
        normal_uv_row0: m.normal_uv[0],
        normal_uv_row1: m.normal_uv[1],
        emissive_uv_row0: m.emissive_uv[0],
        emissive_uv_row1: m.emissive_uv[1],
        base_texture: m.base_texture,
        metallic_roughness_texture: m.metallic_roughness_texture,
        normal_texture: m.normal_texture,
        emissive_texture: m.emissive_texture,
        uv1_slot_mask: m.uv1_slot_mask,
        unlit: m.unlit,
        alpha_blend: m.alpha_blend,
        double_sided: m.double_sided,
    }
}

fn wrap(mode: CpuWrap) -> u8 {
    match mode {
        CpuWrap::Repeat => 0,
        CpuWrap::MirroredRepeat => 1,
        CpuWrap::ClampToEdge => 2,
    }
}

fn sampler(s: CpuSampler) -> ffi::GltfSampler {
    ffi::GltfSampler {
        mag_nearest: s.mag_nearest,
        min_nearest: s.min_nearest,
        mip_nearest: s.mip_nearest,
        mipmaps: s.mipmaps,
        wrap_u: wrap(s.wrap_u),
        wrap_v: wrap(s.wrap_v),
    }
}

fn texture(t: FlatTexture) -> ffi::GltfTexture {
    ffi::GltfTexture {
        encoded: t.encoded,
        sampler: sampler(t.sampler),
        srgb: t.srgb,
    }
}

fn range(r: FlatRange) -> ffi::GltfMeshRange {
    ffi::GltfMeshRange {
        vertex_base: r.vertex_base,
        vertex_count: r.vertex_count,
        index_start: r.index_start,
        index_count: r.index_count,
        tri_start: r.tri_start,
        tri_count: r.tri_count,
        double_sided: r.double_sided,
        alpha_blended: r.alpha_blended,
    }
}

fn light(l: FlatLight) -> ffi::GltfLight {
    ffi::GltfLight {
        kind: l.kind,
        color: l.color,
        intensity: l.intensity,
        range: l.range,
        position: l.position,
        direction: l.direction,
        cos_inner: l.cos_inner,
        cos_outer: l.cos_outer,
    }
}

fn no_light() -> ffi::GltfLight {
    ffi::GltfLight {
        kind: 0,
        color: [0.0; 3],
        intensity: 0.0,
        range: 0.0,
        position: [0.0; 3],
        direction: [0.0; 3],
        cos_inner: 0.0,
        cos_outer: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::diagnostics::{ffi::LogLevel, take_log_records};

    fn asset(name: &str) -> Vec<u8> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../webgpu_renderer/tests/assets")
            .join(name);
        bytes(&path)
    }

    fn bytes(path: &Path) -> Vec<u8> {
        path.to_str().expect("a UTF-8 path").as_bytes().to_vec()
    }

    /// One triangle under node 0, then `nodes`, `scenes` and a top-level `extra`, written to `dir`.
    fn triangle(dir: &Path, nodes: &str, scenes: &str, extra: &str) -> Vec<u8> {
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},{scenes}"nodes":[{{"mesh":0}}{nodes}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}}}]}}],
            "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}}],
            "bufferViews":[{{"buffer":0,"byteLength":36}}],
            "buffers":[{{"byteLength":36,"uri":"data:application/octet-stream;base64,AAAAAAAAAAAAAAAAAACAPwAAAAAAAAAAAAAAAAAAgD8AAAAA"}}]{extra}}}"#
        );
        let path = dir.join("triangle.gltf");
        std::fs::write(&path, json).unwrap();
        bytes(&path)
    }

    #[test]
    fn a_textured_cube_crosses_the_bridge_with_every_array_filled() {
        let model = load_gltf_flat(&asset("cube_textured.gltf"));
        assert!(model.ok, "{}", model.error);
        assert!(model.error.is_empty());
        assert_eq!(model.indices.len() % 3, 0);
        assert_eq!(model.material_index.len(), model.indices.len() / 3);
        assert!(!model.vertices.is_empty() && !model.mesh_ranges.is_empty());
        assert_eq!(model.textures.len(), 1, "one base-colour image");
        assert!(model.textures[0].encoded.starts_with(b"\x89PNG"));
        assert!(model.textures[0].srgb);
        assert_eq!(model.materials[0].base_texture, 0);
        assert_eq!(model.materials.last().unwrap().base_texture, -1);
    }

    #[test]
    fn a_light_crosses_the_bridge_posed_by_its_node() {
        let model = load_gltf_flat(&asset("point_light.gltf"));
        assert!(model.ok, "{}", model.error);
        assert!(!model.lights.is_empty());
        assert!(model.lights.iter().all(|l| (1..=3).contains(&l.kind)));
        assert!(
            !model.has_sun,
            "a point and a spot light, no directional one"
        );
        assert_eq!(model.sun, no_light());
    }

    #[test]
    fn the_first_directional_light_crosses_as_the_sun_and_not_as_a_light() {
        let dir = tempfile::tempdir().unwrap();
        // Pitched -90 degrees about X, so the Sun's -Z travels straight down.
        let nodes = r#",{"extensions":{"KHR_lights_punctual":{"light":0}},"rotation":[-0.7071068,0,0,0.7071068]},
            {"extensions":{"KHR_lights_punctual":{"light":1}},"translation":[0,2,0]}"#;
        let lights = r#","extensionsUsed":["KHR_lights_punctual"],"extensions":{"KHR_lights_punctual":{"lights":[
            {"type":"directional","color":[1,0.9,0.76],"intensity":4098},{"type":"point","intensity":5}]}}"#;
        let scenes = r#""scene":0,"scenes":[{"nodes":[0,1,2]}],"#;
        let model = load_gltf_flat(&triangle(dir.path(), nodes, scenes, lights));
        assert!(model.ok, "{}", model.error);
        assert!(model.has_sun);
        assert_eq!(model.sun.kind, 3);
        assert_eq!(model.sun.color, [1.0, 0.9, 0.76]);
        let [x, y, z] = model.sun.direction;
        assert!(x.abs() < 1e-5 && (y + 1.0).abs() < 1e-5 && z.abs() < 1e-5);
        assert_eq!(
            model.lights.len(),
            1,
            "the point light stays, the Sun does not"
        );
        assert_eq!(model.lights[0].kind, 1);
    }

    #[test]
    fn a_loader_warning_waits_on_the_calling_thread_for_the_engine() {
        let dir = tempfile::tempdir().unwrap();
        take_log_records();
        let model = load_gltf_flat(&triangle(dir.path(), "", "", ""));
        assert!(model.ok, "{}", model.error);
        let records = take_log_records();
        let warning = records
            .iter()
            .find(|record| record.message.contains("no scenes"))
            .expect("a document without scenes warns");
        assert_eq!(warning.level, LogLevel::Warn);
        assert_eq!(warning.target, "kataglyphis_assets::gltf_loader");
        assert!(take_log_records().is_empty(), "drained");
    }

    #[test]
    fn the_load_options_reach_the_loader_and_their_zero_is_the_default() {
        let quad = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/tests/assets/mirrored_uv_quad.gltf");
        let quad = bytes(&quad);
        let defaults = ffi::GltfLoadOptions::default();
        assert_eq!(load_options(&defaults), LoadOptions::default());
        let plain = load_gltf_flat(&quad);
        assert!(plain.ok, "{}", plain.error);
        assert_eq!(load_gltf_flat_with(&quad, &defaults), plain);
        let mikktspace = ffi::GltfLoadOptions {
            mikktspace_tangents: true,
        };
        let split = load_gltf_flat_with(&quad, &mikktspace);
        assert!(split.ok, "{}", split.error);
        assert_eq!(
            (plain.vertices.len(), split.vertices.len()),
            (4, 6),
            "MikkTSpace splits the corners the mirrored charts share"
        );
        let missing = load_gltf_flat_with(b"/no/such/model.glb", &mikktspace);
        assert!(!missing.ok && missing.error.contains("model.glb"));
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        let model = load_gltf_flat(b"/no/such/model.glb");
        assert!(!model.ok);
        assert!(model.error.contains("model.glb"), "{}", model.error);
        assert!(model.vertices.is_empty() && model.materials.is_empty());
        assert!(!model.has_sun);
    }

    #[test]
    fn a_path_that_is_not_utf8_is_an_error() {
        let model = load_gltf_flat(b"\xff\xfe.glb");
        assert!(!model.ok);
        assert_eq!(model.error, "the path is not UTF-8");
    }

    #[test]
    fn a_panicking_load_comes_back_as_an_error() {
        let model = guarded(|| panic!("broken accessor"), loader_panicked);
        assert!(!model.ok);
        assert_eq!(model.error, "the glTF loader panicked: broken accessor");
        assert!(model.vertices.is_empty());
    }

    #[test]
    fn every_wrap_mode_has_its_code() {
        let codes = [
            CpuWrap::Repeat,
            CpuWrap::MirroredRepeat,
            CpuWrap::ClampToEdge,
        ]
        .map(wrap);
        assert_eq!(codes, [0, 1, 2]);
    }
}
