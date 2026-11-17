//! The cxx bridge BeschleunigerBallett's C++ engine calls; Corrosion's `corrosion_add_cxxbridge` generates its C++ half.

// The owner's hard gate on function size; clippy.toml holds the limit.
#![warn(clippy::too_many_lines)]

use kataglyphis_assets::flat::{
    load_flat, FlatLight, FlatMaterial, FlatModel, FlatRange, FlatTexture, FlatVertex,
};
use kataglyphis_assets::scene::{CpuSampler, CpuWrap};

#[cxx::bridge(namespace = "kataglyphis::oxidant")]
mod ffi {
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

    /// One primitive's slice of the flat arrays.
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

    /// A punctual light in world space; `kind` 1 point, 2 spot, 3 directional.
    #[derive(Debug, PartialEq)]
    struct GltfLight {
        kind: u32,
        color: [f32; 3],
        intensity: f32,
        range: f32,
        position: [f32; 3],
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
        lights: Vec<GltfLight>,
    }

    extern "Rust" {
        fn rusty_cxxbridge_integer() -> i32;
        /// Parses the .gltf or .glb at the UTF-8 `path` with OxidANT's loader, flattened for the engine.
        fn load_gltf_flat(path: &[u8]) -> GltfModel;
    }
}

/// The value the engine logs at start-up, which proves the Rust link works.
pub fn rusty_cxxbridge_integer() -> i32 {
    42
}

/// Never unwinds into C++: an error or a panic becomes `ok == false` with the reason.
pub fn load_gltf_flat(path: &[u8]) -> ffi::GltfModel {
    guarded(|| {
        let path = std::str::from_utf8(path).map_err(|_| "the path is not UTF-8".to_owned())?;
        load_flat(path).map_err(|err| format!("{err:#}"))
    })
}

/// A panic under `panic = "unwind"` is caught here; the release profile's `abort` ends the process instead.
fn guarded(load: impl FnOnce() -> Result<FlatModel, String>) -> ffi::GltfModel {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(load)) {
        Ok(Ok(model)) => to_ffi(model, true, String::new()),
        Ok(Err(error)) => to_ffi(FlatModel::default(), false, error),
        Err(panic) => to_ffi(FlatModel::default(), false, panic_message(panic.as_ref())),
    }
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    let reason = panic
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    format!("the glTF loader panicked: {reason}")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../webgpu_renderer/tests/assets")
            .join(name);
        path.to_str().expect("a UTF-8 checkout").as_bytes().to_vec()
    }

    #[test]
    fn the_bridge_function_returns_42() {
        assert_eq!(super::rusty_cxxbridge_integer(), 42);
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
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        let model = load_gltf_flat(b"/no/such/model.glb");
        assert!(!model.ok);
        assert!(model.error.contains("model.glb"), "{}", model.error);
        assert!(model.vertices.is_empty() && model.materials.is_empty());
    }

    #[test]
    fn a_path_that_is_not_utf8_is_an_error() {
        let model = load_gltf_flat(b"\xff\xfe.glb");
        assert!(!model.ok);
        assert_eq!(model.error, "the path is not UTF-8");
    }

    #[test]
    fn a_panic_names_its_reason() {
        let text = std::panic::catch_unwind(|| panic!("static reason")).unwrap_err();
        assert!(panic_message(text.as_ref()).ends_with("static reason"));
        let owned = std::panic::catch_unwind(|| panic!("{}", String::from("owned"))).unwrap_err();
        assert!(panic_message(owned.as_ref()).ends_with("owned"));
        let other = std::panic::catch_unwind(|| std::panic::panic_any(7)).unwrap_err();
        assert!(panic_message(other.as_ref()).ends_with("panicked: "));
    }

    #[test]
    fn a_panicking_load_comes_back_as_an_error() {
        let model = guarded(|| panic!("broken accessor"));
        assert!(!model.ok);
        assert_eq!(model.error, "the glTF loader panicked: broken accessor");
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
