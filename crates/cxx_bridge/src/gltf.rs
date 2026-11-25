//! The model loader for the engine: `kataglyphis_assets::flat`'s arrays for a glTF or an OBJ, in the C++ namespace `kataglyphis::oxidant::gltf`.

use glam::Vec3;
use kataglyphis_assets::flat::{
    load_flat, FlatCamera, FlatLight, FlatMaterial, FlatModel, FlatRange, FlatTexture, FlatVertex,
};
use kataglyphis_assets::obj::{is_obj_path, load_obj_flat};
use kataglyphis_assets::scene::{CpuSampler, CpuWrap};
use kataglyphis_assets::LoadOptions;
use kataglyphis_render_core::framing::{frame_for_cpp, CppPose, FramingSource, SceneCamera};

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
        /// An OBJ `map_d` cut-out mask, -1 for none and for every glTF material.
        alpha_texture: i32,
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

    /// One texture slot: its RGBA8 mip chain, level 0 first and packed back to back, each level halving down to 1x1;
    /// for a format only the engine's stb reads (PSD, PIC), `level_count` 0 and the file's bytes in `encoded`.
    #[derive(Debug, PartialEq)]
    struct GltfTexture {
        width: u32,
        height: u32,
        level_count: u32,
        rgba8: Vec<u8>,
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

    /// A camera the file authors, at its node's world pose; `yfov_rad` 0 for an orthographic one.
    #[derive(Debug, PartialEq)]
    struct GltfCamera {
        name: String,
        eye: [f32; 3],
        /// The way it looks, a unit vector.
        forward: [f32; 3],
        yfov_rad: f32,
    }

    /// The vertices' world-space box; `valid` false for a model without vertices.
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct GltfBounds {
        valid: bool,
        min: [f32; 3],
        max: [f32; 3],
    }

    /// The C++ fly camera: front = (cos pitch cos yaw, sin pitch, cos pitch sin yaw), angles and the field of view in
    /// degrees, the fly speed in world units per second.
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct CameraPose {
        eye: [f32; 3],
        yaw_deg: f32,
        pitch_deg: f32,
        fov_y_deg: f32,
        // Not `near` and `far`, which windows.h defines as macros.
        near_plane: f32,
        far_plane: f32,
        fly_speed: f32,
        shadow_distance: f32,
    }

    /// `frame_scene`'s answer; `source` 0 keeps the fixed pose, 1 the active camera, 2 the first camera, 3 the bounds.
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct SceneFraming {
        framed: bool,
        source: u8,
        /// The fixed pose itself when `framed` is false.
        pose: CameraPose,
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
        cameras: Vec<GltfCamera>,
        /// The camera `scenes[scene].extras.active_camera` names, an index into `cameras`, or -1.
        active_camera: i32,
        /// `asset.copyright`, else the `ATTRIBUTION.txt` beside the model; empty for none.
        credit: String,
        bounds: GltfBounds,
    }

    /// How the loader builds geometry; each field's zero is its default, so `GltfLoadOptions{}` is `load_gltf_flat`'s.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct GltfLoadOptions {
        /// MikkTSpace tangents where the file has none, the basis DCC tools bake against; false keeps Lengyel's.
        mikktspace_tangents: bool,
    }

    extern "Rust" {
        /// Parses the .gltf, .glb or .obj at the UTF-8 `path` with OxidANT's loaders, flattened for the engine.
        fn load_gltf_flat(path: &[u8]) -> GltfModel;
        /// `load_gltf_flat` with `options`, which the engine reads once at its start; the loader reads no environment.
        fn load_gltf_flat_with(path: &[u8], options: &GltfLoadOptions) -> GltfModel;
        /// Where the camera starts on a loaded scene: framed only at its own camera, or at its bounds when `fixed`
        /// would need a far plane past 50 000 near planes; otherwise `fixed`, unframed.
        fn frame_scene(
            cameras: &[GltfCamera],
            active_camera: i32,
            bounds: &GltfBounds,
            fixed: &CameraPose,
        ) -> SceneFraming;
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
    let loaded = if is_obj_path(std::path::Path::new(path)) {
        load_obj_flat(path, options)
    } else {
        load_flat(path, options)
    };
    loaded.map_err(|err| format!("{err:#}"))
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
    let bounds = bounds(&model.vertices);
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
        bounds,
        cameras: model.cameras.into_iter().map(camera).collect(),
        active_camera: model
            .active_camera
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(-1),
        credit: model.credit.unwrap_or_default(),
    }
}

fn camera(c: FlatCamera) -> ffi::GltfCamera {
    ffi::GltfCamera {
        name: c.name,
        eye: c.eye,
        forward: c.forward,
        yfov_rad: c.yfov_rad.unwrap_or(0.0),
    }
}

fn bounds(vertices: &[FlatVertex]) -> ffi::GltfBounds {
    let (min, max) = vertices.iter().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(min, max), v| {
            let p = Vec3::from(v.position);
            (min.min(p), max.max(p))
        },
    );
    ffi::GltfBounds {
        valid: !vertices.is_empty() && min.is_finite() && max.is_finite(),
        min: min.to_array(),
        max: max.to_array(),
    }
}

/// Never unwinds into C++: a panic keeps the fixed pose.
pub fn frame_scene(
    cameras: &[ffi::GltfCamera],
    active_camera: i32,
    bounds: &ffi::GltfBounds,
    fixed: &ffi::CameraPose,
) -> ffi::SceneFraming {
    guarded(
        || framing(cameras, active_camera, bounds, fixed),
        kept_on_panic(fixed),
    )
}

/// `frame_scene`'s panic fallback, named so a test can make it run.
fn kept_on_panic(fixed: &ffi::CameraPose) -> impl FnOnce(String) -> ffi::SceneFraming + '_ {
    |_| unframed(fixed)
}

fn framing(
    cameras: &[ffi::GltfCamera],
    active_camera: i32,
    bounds: &ffi::GltfBounds,
    fixed: &ffi::CameraPose,
) -> ffi::SceneFraming {
    let scene: Vec<SceneCamera> = cameras
        .iter()
        .map(|c| SceneCamera {
            eye: Vec3::from(c.eye),
            forward: Vec3::from(c.forward),
            yfov_rad: (c.yfov_rad > 0.0).then_some(c.yfov_rad),
        })
        .collect();
    let active = usize::try_from(active_camera)
        .ok()
        .and_then(|index| Some((index, cameras.get(index)?.name.clone())));
    let box_ = bounds
        .valid
        .then(|| (Vec3::from(bounds.min), Vec3::from(bounds.max)));
    match frame_for_cpp(&scene, active, box_, &cpp_pose(fixed)) {
        Some((pose, source)) => ffi::SceneFraming {
            framed: true,
            source: match source {
                FramingSource::ActiveCamera(_) => 1,
                FramingSource::FirstCamera => 2,
                FramingSource::Bounds => 3,
                FramingSource::Unchanged => 0,
            },
            pose: ffi_pose(&pose),
        },
        None => unframed(fixed),
    }
}

fn unframed(fixed: &ffi::CameraPose) -> ffi::SceneFraming {
    ffi::SceneFraming {
        framed: false,
        source: 0,
        pose: *fixed,
    }
}

fn cpp_pose(p: &ffi::CameraPose) -> CppPose {
    CppPose {
        eye: Vec3::from(p.eye),
        yaw_deg: p.yaw_deg,
        pitch_deg: p.pitch_deg,
        fov_y_deg: p.fov_y_deg,
        near: p.near_plane,
        far: p.far_plane,
        fly_speed: p.fly_speed,
        shadow_distance: p.shadow_distance,
    }
}

fn ffi_pose(p: &CppPose) -> ffi::CameraPose {
    ffi::CameraPose {
        eye: p.eye.to_array(),
        yaw_deg: p.yaw_deg,
        pitch_deg: p.pitch_deg,
        fov_y_deg: p.fov_y_deg,
        near_plane: p.near,
        far_plane: p.far,
        fly_speed: p.fly_speed,
        shadow_distance: p.shadow_distance,
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
        alpha_texture: m.alpha_texture,
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
        width: t.chain.width,
        height: t.chain.height,
        level_count: t.chain.level_count,
        rgba8: t.chain.texels,
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
            .join("../assets/tests/assets")
            .join(name);
        bytes(&path)
    }

    fn bytes(path: &Path) -> Vec<u8> {
        path.to_str().expect("a UTF-8 path").as_bytes().to_vec()
    }

    /// One triangle under node 0, then `nodes`, `scenes` and a top-level `extra`, written to `dir`.
    /// The bytes a texture's packed chain holds: each level halves down to 1x1.
    fn chain_bytes(texture: &ffi::GltfTexture) -> usize {
        (0..texture.level_count)
            .map(|level| {
                let side = |s: u32| (s >> level).max(1) as usize;
                side(texture.width) * side(texture.height) * 4
            })
            .sum()
    }

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
        let texture = &model.textures[0];
        assert!(texture.encoded.is_empty(), "decoded here, not by stb");
        assert_eq!(
            texture.level_count,
            32 - texture.width.max(texture.height).leading_zeros()
        );
        assert_eq!(texture.rgba8.len(), chain_bytes(texture));
        assert!(texture.srgb);
        assert_eq!(model.materials[0].base_texture, 0);
        assert_eq!(model.materials.last().unwrap().base_texture, -1);
        assert!(
            model.materials.iter().all(|m| m.alpha_texture == -1),
            "glTF has no map_d"
        );
    }

    #[test]
    fn a_basisu_only_texture_crosses_as_its_transcoded_chain() {
        let model = load_gltf_flat(&asset("cube_basisu.gltf"));
        assert!(model.ok, "{}", model.error);
        assert_eq!(model.textures.len(), 1, "the KTX2 is the slot");
        assert_eq!(model.materials[0].base_texture, 0);
        let texture = &model.textures[0];
        assert!(texture.encoded.is_empty());
        assert!(texture.level_count > 1, "the KTX2's own levels");
        assert_eq!(texture.rgba8.len(), chain_bytes(texture));
    }

    #[test]
    fn an_obj_crosses_the_same_bridge_by_its_extension_in_any_case() {
        let dir = tempfile::tempdir().unwrap();
        let card = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/tests/assets/obj_mask");
        for name in ["card.mtl", "red.bmp", "half_mask.tga"] {
            std::fs::copy(card.join(name), dir.path().join(name)).unwrap();
        }
        std::fs::copy(card.join("card.obj"), dir.path().join("CARD.OBJ")).unwrap();
        let model = load_gltf_flat(&bytes(&dir.path().join("CARD.OBJ")));
        assert!(model.ok, "{}", model.error);
        let material = &model.materials[0];
        assert_eq!(material.alpha_cutoff, 0.5, "map_d cuts out");
        let mask = &model.textures[material.alpha_texture as usize];
        assert!(!mask.srgb);
        let base = &model.textures[material.base_texture as usize];
        assert!(base.encoded.is_empty(), "the BMP decodes here");
        assert_eq!((base.width, base.height, base.level_count), (8, 8, 4));
        assert_eq!(base.rgba8.len(), chain_bytes(base));
        assert_eq!(base.rgba8[..4], [255, 0, 0, 255], "red.bmp is red");
        let missing = load_gltf_flat(b"/no/such/model.obj");
        assert!(
            !missing.ok && missing.error.contains("model.obj"),
            "{}",
            missing.error
        );
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

    /// The C++ Release start pose, Sponza's.
    fn release_pose() -> ffi::CameraPose {
        ffi::CameraPose {
            eye: [0.0, 2.0, 0.0],
            yaw_deg: -90.0,
            pitch_deg: 0.0,
            fov_y_deg: 45.0,
            near_plane: 0.1,
            far_plane: 4000.0,
            fly_speed: 10.0,
            shadow_distance: 60.0,
        }
    }

    #[test]
    fn the_cameras_the_credit_and_the_bounds_cross_the_bridge() {
        let dir = tempfile::tempdir().unwrap();
        let scene = triangle(
            dir.path(),
            r#",{"name":"Arena","camera":0,"translation":[0,1,5]}"#,
            r#""scene":0,"scenes":[{"nodes":[0,1],"extras":{"active_camera":"Arena"}}],"#,
            r#","cameras":[{"type":"perspective","perspective":{"yfov":0.6,"znear":0.1}}]"#,
        );
        let model = load_gltf_flat(&scene);
        assert!(model.ok, "{}", model.error);
        assert_eq!(model.cameras.len(), 1);
        let arena = &model.cameras[0];
        assert_eq!(
            (
                arena.name.as_str(),
                arena.eye,
                arena.forward,
                arena.yfov_rad
            ),
            ("Arena", [0.0, 1.0, 5.0], [0.0, 0.0, -1.0], 0.6)
        );
        assert_eq!(model.active_camera, 0);
        assert_eq!(
            model.bounds,
            ffi::GltfBounds {
                valid: true,
                min: [0.0; 3],
                max: [1.0, 1.0, 0.0]
            }
        );
        assert!(model.credit.is_empty());
        std::fs::write(
            dir.path().join("ATTRIBUTION.txt"),
            "License:     CC BY 4.0\nAttribution: Somebody\n",
        )
        .unwrap();
        assert_eq!(load_gltf_flat(&scene).credit, "Somebody, CC BY 4.0");
        let failed = load_gltf_flat(b"/no/such/model.glb");
        assert_eq!((failed.active_camera, failed.bounds.valid), (-1, false));
    }

    #[test]
    fn a_scene_is_framed_at_its_camera_and_otherwise_keeps_the_fixed_pose() {
        let arena = arena_camera();
        let sponza = ffi::GltfBounds {
            valid: true,
            min: [-19.2, -1.3, -11.9],
            max: [18.0, 15.4, 11.2],
        };
        let framed = frame_scene(&[arena], 0, &sponza, &release_pose());
        assert!(framed.framed);
        assert_eq!(framed.source, 1);
        // The orbit camera places the eye from its target, a float round trip away from the authored one.
        assert!(Vec3::from(framed.pose.eye).abs_diff_eq(Vec3::new(-58.0, 34.0, 36.0), 1e-3));
        // The C++ convention: yaw + 180 and -pitch of the orbit's, which looks along the camera's -Z.
        let (yaw, pitch) = (
            framed.pose.yaw_deg.to_radians(),
            framed.pose.pitch_deg.to_radians(),
        );
        let front = Vec3::new(
            pitch.cos() * yaw.cos(),
            pitch.sin(),
            pitch.cos() * yaw.sin(),
        );
        assert!(
            front.abs_diff_eq(Vec3::from(arena_forward()), 1e-4),
            "{front}"
        );
        assert!((framed.pose.fov_y_deg - 0.7427_f32.to_degrees()).abs() < 1e-3);
        let kept = frame_scene(&[], -1, &sponza, &release_pose());
        assert_eq!(
            kept,
            ffi::SceneFraming {
                framed: false,
                source: 0,
                pose: release_pose()
            }
        );
        let city = ffi::GltfBounds {
            valid: true,
            min: [-4000.0, -11.25, -4000.0],
            max: [4000.0, 58.6, 4000.0],
        };
        let fitted = frame_scene(&[], -1, &city, &release_pose());
        assert_eq!((fitted.framed, fitted.source), (true, 3));
        assert!(fitted.pose.far_plane / fitted.pose.near_plane <= 50_001.0);
        // An index past the cameras falls back to the first.
        assert_eq!(
            frame_scene(&[arena_camera()], 7, &sponza, &release_pose()).source,
            2
        );
    }

    #[test]
    fn a_panic_while_framing_keeps_the_fixed_pose() {
        let kept = guarded(|| panic!("broken camera"), kept_on_panic(&release_pose()));
        assert_eq!(
            kept,
            ffi::SceneFraming {
                framed: false,
                source: 0,
                pose: release_pose()
            }
        );
    }

    fn arena_forward() -> [f32; 3] {
        Vec3::new(0.783, -0.403, -0.474).normalize().to_array()
    }

    fn arena_camera() -> ffi::GltfCamera {
        ffi::GltfCamera {
            name: "Kamera_Arena".into(),
            eye: [-58.0, 34.0, 36.0],
            forward: arena_forward(),
            yfov_rad: 0.7427,
        }
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
