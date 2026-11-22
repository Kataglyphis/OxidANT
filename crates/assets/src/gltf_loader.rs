//! glTF 2.0 -> `CpuScene`: geometry, node transforms, animation and metallic-roughness materials.

use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;
use glam::{Mat4, Vec2, Vec3};

use crate::attributes::read_f32;
use crate::scene::{
    AlphaMode, ChannelValues, CpuAnimation, CpuAnimationChannel, CpuCamera, CpuCameraProjection,
    CpuLight, CpuLightKind, CpuMaterial, CpuNode, CpuPrimitive, CpuSampler, CpuScene, CpuSkin,
    CpuTexture, CpuTextureRef, CpuWrap, Interpolation, MorphTarget, Vertex,
};
use crate::{files, gltf_images};

/// How the loader builds geometry. An application reads its settings once at its start; the loader reads no environment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoadOptions {
    /// MikkTSpace tangents where the file has none, the basis DCC tools bake against; off keeps Lengyel's.
    pub mikktspace_tangents: bool,
}

/// [`load_gltf_with`] the default options.
pub fn load_gltf(path: impl AsRef<Path>) -> anyhow::Result<CpuScene> {
    load_gltf_with(path, &LoadOptions::default())
}

pub fn load_gltf_with(path: impl AsRef<Path>, options: &LoadOptions) -> anyhow::Result<CpuScene> {
    let path = path.as_ref();
    let (document, buffers, textures) = import_file(path)
        .with_context(|| format!("Failed to import glTF file: {}", path.display()))?;
    build_scene(document, buffers, textures, options)
        .with_context(|| format!("Failed to build scene from {}", path.display()))
}

/// `gltf::import` with `KHR_texture_basisu`: the document, its buffers and each texture's image.
fn import_file(
    path: &Path,
) -> anyhow::Result<(
    gltf::Document,
    Vec<gltf::buffer::Data>,
    Vec<Arc<CpuTexture>>,
)> {
    let imported = import_document(path)?;
    let base = Some(imported.base.as_path());
    let images = gltf_images::import_images(&imported.document, base, &imported.buffers)?;
    let textures = gltf_images::resolve_textures(&imported.sources, images)?;
    Ok((imported.document, imported.buffers, textures))
}

/// A glTF file's document, buffers and texture sources, with its images left undecoded.
pub(crate) struct Imported {
    pub document: gltf::Document,
    pub buffers: Vec<gltf::buffer::Data>,
    pub sources: Vec<gltf_images::TextureSource>,
    /// The directory relative URIs resolve against.
    pub base: std::path::PathBuf,
}

pub(crate) fn import_document(path: &Path) -> anyhow::Result<Imported> {
    let bytes = files::read_regular_file(path)?;
    let base = path.parent().unwrap_or_else(|| Path::new("./"));
    let gltf_images::Parsed {
        document,
        blob,
        sources,
    } = gltf_images::parse(&bytes)?;
    let buffers = files::import_buffers(&document, Some(base), blob)?;
    Ok(Imported {
        document,
        buffers,
        sources,
        base: base.to_path_buf(),
    })
}

/// [`load_gltf_slice_with`] the default options.
pub fn load_gltf_slice(bytes: &[u8]) -> anyhow::Result<CpuScene> {
    load_gltf_slice_with(bytes, &LoadOptions::default())
}

/// In-memory variant for wasm32 and embedded assets; buffers and images must be embedded (data URIs or GLB).
pub fn load_gltf_slice_with(bytes: &[u8], options: &LoadOptions) -> anyhow::Result<CpuScene> {
    let gltf_images::Parsed {
        document,
        blob,
        sources,
    } = gltf_images::parse(bytes).context("Failed to import glTF from memory")?;
    let buffers = files::import_buffers(&document, None, blob)
        .context("Failed to import glTF buffers from memory")?;
    if let Some(uri) = document.images().find_map(|image| match image.source() {
        gltf::image::Source::Uri { uri, .. } if !uri.starts_with("data:") => Some(uri.to_owned()),
        _ => None,
    }) {
        anyhow::bail!(
            "glTF image '{uri}' is an external file, which an in-memory import cannot read"
        );
    }
    // gltf 1.4's import_slice refuses a data: image too (it wants a base path); every image left is data: or GLB, so none reads the base.
    let images = gltf_images::import_images(&document, Some(Path::new("")), &buffers)
        .context("Failed to import glTF images from memory")?;
    let textures = gltf_images::resolve_textures(&sources, images)
        .context("Failed to import glTF images from memory")?;
    build_scene(document, buffers, textures, options)
}

/// `textures` is index-aligned with the document's textures.
fn build_scene(
    document: gltf::Document,
    buffers: Vec<gltf::buffer::Data>,
    textures: Vec<Arc<CpuTexture>>,
    options: &LoadOptions,
) -> anyhow::Result<CpuScene> {
    let mut scene = CpuScene {
        nodes: read_nodes(&document),
        animations: read_animations(&document, &buffers),
        skins: read_skins(&document, &buffers),
        cameras: read_cameras(&document),
        ..CpuScene::default()
    };

    walk_nodes(&document, |node, world| {
        visit_node(node, world, &buffers, &textures, options, &mut scene)
    })?;

    anyhow::ensure!(
        !scene.primitives.is_empty(),
        "glTF file contains no triangle primitives"
    );

    Ok(scene)
}

/// The full node table, index-aligned with glTF node indices, with each node's local TRS and parent.
pub(crate) fn read_nodes(document: &gltf::Document) -> Vec<CpuNode> {
    let mut nodes: Vec<CpuNode> = document
        .nodes()
        .map(|n| {
            let (t, r, s) = n.transform().decomposed();
            CpuNode {
                parent: None,
                translation: glam::Vec3::from_array(t),
                rotation: glam::Quat::from_array(r),
                scale: glam::Vec3::from_array(s),
            }
        })
        .collect();
    for node in document.nodes() {
        for child in node.children() {
            nodes[child.index()].parent = Some(node.index());
        }
    }
    nodes
}

/// Every animation with at least one valid channel.
fn read_animations(document: &gltf::Document, buffers: &[gltf::buffer::Data]) -> Vec<CpuAnimation> {
    let mut animations = Vec::new();
    for animation in document.animations() {
        let channels: Vec<CpuAnimationChannel> = animation
            .channels()
            .filter_map(|channel| read_channel(&channel, buffers))
            .collect();
        let duration = channels
            .iter()
            .filter_map(|channel| channel.times.last().copied())
            .fold(0.0f32, f32::max);
        if !channels.is_empty() {
            animations.push(CpuAnimation {
                name: animation.name().unwrap_or("animation").to_string(),
                duration,
                channels,
            });
        }
    }
    animations
}

/// One channel, or `None` when unreadable or invalid; gltf checks no accessor values.
fn read_channel(
    channel: &gltf::animation::Channel,
    buffers: &[gltf::buffer::Data],
) -> Option<CpuAnimationChannel> {
    let reader = channel.reader(|buffer| buffers.get(buffer.index()).map(|b| &b.0[..]));
    let times: Vec<f32> = reader.read_inputs()?.collect();
    let values = channel_values(reader.read_outputs()?);
    let interpolation = interpolation_of(channel.sampler().interpolation());
    if let Err(reason) = check_keyframes(&times, &values, interpolation) {
        log::warn!(
            "glTF animation channel {} skipped: {reason}",
            channel.index()
        );
        return None;
    }
    Some(CpuAnimationChannel {
        node: channel.target().node().index(),
        times,
        values,
        interpolation,
    })
}

/// Sampling bisects `times`, so a NaN or a step back breaks it; a short output reads past its end.
fn check_keyframes(
    times: &[f32],
    values: &ChannelValues,
    interpolation: Interpolation,
) -> Result<(), &'static str> {
    if times.is_empty() || times.iter().any(|t| !t.is_finite()) {
        return Err("its keyframe times are empty or not finite");
    }
    if times.windows(2).any(|pair| pair[1] < pair[0]) {
        return Err("its keyframe times decrease");
    }
    let (count, finite) = match values {
        ChannelValues::Translation(v) | ChannelValues::Scale(v) => {
            (v.len(), v.iter().all(|x| x.is_finite()))
        }
        ChannelValues::Rotation(v) => (v.len(), v.iter().all(|q| q.is_finite())),
        ChannelValues::MorphWeights(v) => (v.len(), v.iter().all(|w| w.is_finite())),
    };
    if !finite {
        return Err("its keyframe values are not finite");
    }
    let per_key = if interpolation == Interpolation::CubicSpline {
        3
    } else {
        1
    };
    let keys = times.len() * per_key;
    let fits = match values {
        // Morph weights hold one value per target per key, so any whole multiple fits.
        ChannelValues::MorphWeights(_) => count > 0 && count.is_multiple_of(keys),
        _ => count == keys,
    };
    if !fits {
        return Err("its value count does not match its keyframes");
    }
    Ok(())
}

fn channel_values(outputs: gltf::animation::util::ReadOutputs<'_>) -> ChannelValues {
    use gltf::animation::util::ReadOutputs;
    match outputs {
        ReadOutputs::Translations(iter) => {
            ChannelValues::Translation(iter.map(glam::Vec3::from_array).collect())
        }
        ReadOutputs::Rotations(rotations) => {
            ChannelValues::Rotation(rotations.into_f32().map(glam::Quat::from_array).collect())
        }
        ReadOutputs::Scales(iter) => {
            ChannelValues::Scale(iter.map(glam::Vec3::from_array).collect())
        }
        ReadOutputs::MorphTargetWeights(w) => ChannelValues::MorphWeights(w.into_f32().collect()),
    }
}

fn interpolation_of(interpolation: gltf::animation::Interpolation) -> Interpolation {
    match interpolation {
        gltf::animation::Interpolation::Linear => Interpolation::Linear,
        gltf::animation::Interpolation::Step => Interpolation::Step,
        gltf::animation::Interpolation::CubicSpline => Interpolation::CubicSpline,
    }
}

/// Skins: joint node indices + inverse bind matrices.
pub(crate) fn read_skins(
    document: &gltf::Document,
    buffers: &[gltf::buffer::Data],
) -> Vec<CpuSkin> {
    document
        .skins()
        .map(|skin| {
            let reader = skin.reader(|buffer| buffers.get(buffer.index()).map(|b| &b.0[..]));
            let inverse_bind_matrices = reader
                .read_inverse_bind_matrices()
                .map(|iter| iter.map(|m| Mat4::from_cols_array_2d(&m)).collect())
                .unwrap_or_default();
            CpuSkin {
                joints: skin.joints().map(|j| j.index()).collect(),
                inverse_bind_matrices,
            }
        })
        .collect()
}

/// Cameras authored in the file (pose = their node's world transform).
fn read_cameras(document: &gltf::Document) -> Vec<CpuCamera> {
    let mut cameras = Vec::new();
    for node in document.nodes() {
        let Some(camera) = node.camera() else {
            continue;
        };
        let projection = match camera.projection() {
            gltf::camera::Projection::Perspective(perspective) => {
                CpuCameraProjection::Perspective {
                    yfov_rad: perspective.yfov(),
                    znear: perspective.znear(),
                    zfar: perspective.zfar(),
                }
            }
            gltf::camera::Projection::Orthographic(orthographic) => {
                CpuCameraProjection::Orthographic {
                    xmag: orthographic.xmag(),
                    ymag: orthographic.ymag(),
                    znear: orthographic.znear(),
                    zfar: orthographic.zfar(),
                }
            }
        };
        cameras.push(CpuCamera {
            name: camera.name().map(str::to_string),
            node: node.index(),
            projection,
        });
    }
    cameras
}

/// UV-set bit for a texture slot: `1 << bit` for TEXCOORD_1, else 0 (TEXCOORD_2+ warns and uses UV0).
fn uv_set_bit(slot: &str, tex_coord: u32, bit: u32) -> u32 {
    match tex_coord {
        0 => 0,
        1 => 1u32 << bit,
        n => {
            log::warn!(
                "Material {slot} texture uses TEXCOORD_{n}, but only TEXCOORD_0/1 are supported; sampling with UV0"
            );
            0
        }
    }
}

const IDENTITY_UV_TRANSFORM: [[f32; 3]; 2] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];

/// Affine T*R*S rows for KHR_texture_transform; rotation is negated because this engine's UV space is Y-down.
fn uv_transform_rows(offset: [f32; 2], rotation: f32, scale: [f32; 2]) -> [[f32; 3]; 2] {
    let m = glam::Mat3::from_translation(glam::Vec2::from_array(offset))
        * glam::Mat3::from_angle(-rotation)
        * glam::Mat3::from_scale(glam::Vec2::from_array(scale));
    [
        [m.x_axis.x, m.y_axis.x, m.z_axis.x],
        [m.x_axis.y, m.y_axis.y, m.z_axis.y],
    ]
}

/// KHR_texture_transform of a typed `Info` slot (base colour, metallic-roughness, emissive), else identity.
fn uv_transform_from_info(transform: Option<gltf::texture::TextureTransform>) -> [[f32; 3]; 2] {
    transform.map_or(IDENTITY_UV_TRANSFORM, |t| {
        uv_transform_rows(t.offset(), t.rotation(), t.scale())
    })
}

/// KHR_texture_transform for normal/occlusion slots, parsed from raw JSON since `gltf` has no typed accessor there.
fn uv_transform_from_extension_json(value: Option<&gltf::json::Value>) -> [[f32; 3]; 2] {
    value
        .and_then(|v| {
            gltf::json::deserialize::from_value::<gltf::json::extensions::texture::TextureTransform>(v.clone()).ok()
        })
        .map_or(IDENTITY_UV_TRANSFORM, |t| {
            uv_transform_rows(t.offset.0, t.rotation.0, t.scale.0)
        })
}

/// Expands a triangle strip or fan into a triangle list; odd strip triangles swap two indices to keep winding.
fn triangulate(indices: &[u32], mode: gltf::mesh::Mode) -> Vec<u32> {
    use gltf::mesh::Mode;
    match mode {
        Mode::TriangleStrip => {
            let mut out = Vec::with_capacity(indices.len().saturating_sub(2) * 3);
            for i in 0..indices.len().saturating_sub(2) {
                let (a, b, c) = (indices[i], indices[i + 1], indices[i + 2]);
                if i % 2 == 0 {
                    out.extend_from_slice(&[a, b, c]);
                } else {
                    out.extend_from_slice(&[b, a, c]);
                }
            }
            out
        }
        Mode::TriangleFan => {
            let mut out = Vec::with_capacity(indices.len().saturating_sub(2) * 3);
            for i in 1..indices.len().saturating_sub(1) {
                out.extend_from_slice(&[indices[0], indices[i], indices[i + 1]]);
            }
            out
        }
        _ => indices.to_vec(),
    }
}

/// Drops whole triangles with an out-of-range corner, returning the kept list and the dropped count.
/// `gltf` never checks index values, and a downstream bounds panic aborts the whole WASM canvas.
fn drop_out_of_range_triangles(indices: Vec<u32>, vertex_count: usize) -> (Vec<u32>, usize) {
    let in_range = |tri: &&[u32; 3]| tri.iter().all(|&i| (i as usize) < vertex_count);
    let triangle_count = indices.len() / 3;
    let kept = indices.as_chunks::<3>().0.iter().filter(in_range).count();
    if kept == triangle_count && indices.len().is_multiple_of(3) {
        return (indices, 0);
    }

    let mut out = Vec::with_capacity(kept * 3);
    for tri in indices.as_chunks::<3>().0.iter().filter(in_range) {
        out.extend_from_slice(tri);
    }
    (out, triangle_count - kept)
}

/// Converts a decoded glTF image into tightly packed RGBA8.
pub(crate) fn to_rgba8(img: gltf::image::Data) -> anyhow::Result<CpuTexture> {
    use gltf::image::Format;

    let pixel_count = (img.width * img.height) as usize;
    let rgba8 = match img.format {
        Format::R8G8B8A8 => img.pixels,
        Format::R8G8B8 => {
            let mut out = Vec::with_capacity(pixel_count * 4);
            for rgb in img.pixels.as_chunks::<3>().0 {
                out.extend_from_slice(rgb);
                out.push(255);
            }
            out
        }
        Format::R8 => {
            let mut out = Vec::with_capacity(pixel_count * 4);
            for &r in &img.pixels {
                out.extend_from_slice(&[r, r, r, 255]);
            }
            out
        }
        Format::R8G8 => {
            let mut out = Vec::with_capacity(pixel_count * 4);
            for rg in img.pixels.as_chunks::<2>().0 {
                out.extend_from_slice(&[rg[0], rg[1], 0, 255]);
            }
            out
        }
        // 16-bit PNGs are common tool output: keep the high byte rather than fail the whole file.
        Format::R16 | Format::R16G16 | Format::R16G16B16 | Format::R16G16B16A16 => {
            let channels = match img.format {
                Format::R16 => 1,
                Format::R16G16 => 2,
                Format::R16G16B16 => 3,
                _ => 4,
            };
            let mut out = Vec::with_capacity(pixel_count * 4);
            for texel in img.pixels.chunks_exact(channels * 2) {
                // Little-endian u16 -> u8 by keeping the high byte.
                let ch = |i: usize| texel[i * 2 + 1];
                match channels {
                    1 => {
                        let v = ch(0);
                        out.extend_from_slice(&[v, v, v, 255]);
                    }
                    2 => out.extend_from_slice(&[ch(0), ch(1), 0, 255]),
                    3 => out.extend_from_slice(&[ch(0), ch(1), ch(2), 255]),
                    _ => out.extend_from_slice(&[ch(0), ch(1), ch(2), ch(3)]),
                }
            }
            out
        }
        other => anyhow::bail!("Unsupported glTF image format: {other:?}"),
    };

    anyhow::ensure!(
        rgba8.len() == pixel_count * 4,
        "Image byte count mismatch after RGBA8 conversion"
    );

    Ok(CpuTexture {
        width: img.width,
        height: img.height,
        rgba8,
        compressed: None,
    })
}

pub(crate) fn to_cpu_sampler(sampler: &gltf::texture::Sampler) -> CpuSampler {
    use gltf::texture::{MagFilter, MinFilter, WrappingMode};

    let wrap = |mode: WrappingMode| match mode {
        WrappingMode::ClampToEdge => CpuWrap::ClampToEdge,
        WrappingMode::MirroredRepeat => CpuWrap::MirroredRepeat,
        WrappingMode::Repeat => CpuWrap::Repeat,
    };

    let (min_nearest, mip_nearest, mipmaps) = match sampler.min_filter() {
        // NEAREST and LINEAR are glTF's mipmap-less filters: level 0 only.
        Some(MinFilter::Nearest) => (true, true, false),
        Some(MinFilter::Linear) => (false, false, false),
        Some(MinFilter::NearestMipmapNearest) => (true, true, true),
        Some(MinFilter::NearestMipmapLinear) => (true, false, true),
        Some(MinFilter::LinearMipmapNearest) => (false, true, true),
        Some(MinFilter::LinearMipmapLinear) | None => (false, false, true),
    };

    CpuSampler {
        mag_nearest: matches!(sampler.mag_filter(), Some(MagFilter::Nearest)),
        min_nearest,
        mip_nearest,
        mipmaps,
        wrap_u: wrap(sampler.wrap_s()),
        wrap_v: wrap(sampler.wrap_t()),
    }
}

fn texture_ref(
    info: &gltf::Texture,
    textures: &[Arc<CpuTexture>],
    srgb: bool,
) -> Option<CpuTextureRef> {
    textures
        .get(info.index())
        .cloned()
        .map(|texture| CpuTextureRef {
            texture,
            sampler: to_cpu_sampler(&info.sampler()),
            srgb,
        })
}

/// Every node a load draws, with its world transform: the default scene's, else the first scene's, else all.
pub(crate) fn walk_nodes(
    document: &gltf::Document,
    mut visit: impl FnMut(&gltf::Node, Mat4) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    if let Some(gltf_scene) = document
        .default_scene()
        .or_else(|| document.scenes().next())
    {
        return walk_scene(&gltf_scene, document.nodes().len(), visit);
    }
    // Without scenes every node is all the geometry there is; each is posed by its parent chain, not walked into.
    log::warn!("glTF document has no scenes; loading every node");
    let world = CpuScene::compute_world_transforms(&read_nodes(document));
    for node in document.nodes() {
        visit(&node, world[node.index()])?;
    }
    Ok(())
}

/// Depth-first over the scene's node trees with an explicit stack, each node visited once with its world transform.
/// A cycle or a second parent is invalid glTF; recursion overflowed the stack on it, and on deep chains.
fn walk_scene(
    gltf_scene: &gltf::Scene,
    node_count: usize,
    mut visit: impl FnMut(&gltf::Node, Mat4) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let mut visited = vec![false; node_count];
    let mut stack: Vec<(gltf::Node, Mat4)> = gltf_scene
        .nodes()
        .map(|node| (node, Mat4::IDENTITY))
        .collect();
    stack.reverse();
    while let Some((node, parent_world)) = stack.pop() {
        if std::mem::replace(&mut visited[node.index()], true) {
            log::warn!(
                "glTF node {} is reached a second time (a cycle or a second parent); skipped",
                node.index()
            );
            continue;
        }
        let world = parent_world * Mat4::from_cols_array_2d(&node.transform().matrix());
        visit(&node, world)?;
        let first_child = stack.len();
        stack.extend(node.children().map(|child| (child, world)));
        // Reversed, so children pop in document order as they did in the recursive walk.
        stack[first_child..].reverse();
    }
    Ok(())
}

/// The node's KHR_lights_punctual light, posed by its `world` transform and tied to the node.
pub(crate) fn read_light(node: &gltf::Node, world: Mat4) -> Option<CpuLight> {
    use gltf::khr_lights_punctual::Kind;
    let light = node.light()?;
    let kind = match light.kind() {
        Kind::Point => CpuLightKind::Point,
        Kind::Spot {
            inner_cone_angle,
            outer_cone_angle,
        } => CpuLightKind::Spot {
            cos_inner: inner_cone_angle.cos(),
            cos_outer: outer_cone_angle.cos(),
        },
        Kind::Directional => CpuLightKind::Directional,
    };
    let mut cpu = CpuLight {
        kind,
        color: light.color(),
        intensity: light.intensity(),
        range: light.range().unwrap_or(0.0),
        position: [0.0; 3],
        direction: [0.0; 3],
        node: Some(node.index()),
    };
    cpu.pose_at(world);
    Some(cpu)
}

/// One node's light and mesh primitives, posed by its `world` transform.
fn visit_node(
    node: &gltf::Node,
    world: Mat4,
    buffers: &[gltf::buffer::Data],
    textures: &[Arc<CpuTexture>],
    options: &LoadOptions,
    scene: &mut CpuScene,
) -> anyhow::Result<()> {
    if let Some(light) = read_light(node, world) {
        scene.lights.push(light);
    }

    if let Some(mesh) = node.mesh() {
        // glTF ignores a skinned mesh's node transform: its joints place it.
        let transform = if node.skin().is_some() {
            Mat4::IDENTITY
        } else {
            world
        };
        for primitive in mesh.primitives().filter(draws_triangles) {
            if let Some(mut cpu) =
                load_primitive(&primitive, transform, buffers, textures, options)?
            {
                cpu.node_index = Some(node.index());
                cpu.skin_index = node.skin().map(|s| s.index());
                let joint_count = node.skin().map(|skin| skin.joints().count());
                bind_influences(&mut cpu.vertices, joint_count, primitive.index());
                if let Some(weights) = default_morph_weights(node, cpu.morph_targets.len()) {
                    cpu.morph_weights = weights.to_vec();
                }
                scene.primitives.push(cpu);
            }
        }
    }
    Ok(())
}

/// The node's default morph weights, else its mesh's (glTF spec); `None` unless there is one per target.
pub(crate) fn default_morph_weights<'a>(
    node: &gltf::Node<'a>,
    targets: usize,
) -> Option<&'a [f32]> {
    let weights = node.weights().or_else(|| node.mesh()?.weights())?;
    (targets > 0 && weights.len() == targets).then_some(weights)
}

/// Strips and fans are triangulated on load; points and lines have nothing to draw, and warn.
pub(crate) fn draws_triangles(primitive: &gltf::Primitive) -> bool {
    use gltf::mesh::Mode;
    let mode = primitive.mode();
    let triangles = matches!(
        mode,
        Mode::Triangles | Mode::TriangleStrip | Mode::TriangleFan
    );
    if !triangles {
        log::warn!(
            "Skipping non-triangle primitive {} (mode {mode:?})",
            primitive.index()
        );
    }
    triangles
}

/// Drops skin influences the shader cannot resolve: every one without a skin, and joints past its end.
/// A weighted vertex skips the node transform, so a skinless node's copy drew at the mesh origin.
pub(crate) fn bind_influences(
    vertices: &mut [Vertex],
    joint_count: Option<usize>,
    primitive: usize,
) {
    let mut dropped = 0usize;
    for vertex in vertices.iter_mut() {
        for k in 0..4 {
            if joint_count.is_some_and(|count| (vertex.joints[k] as usize) < count) {
                continue;
            }
            dropped += usize::from(vertex.weights[k] != 0.0);
            vertex.weights[k] = 0.0;
            vertex.joints[k] = 0.0;
        }
    }
    if dropped > 0 {
        log::warn!(
            "glTF primitive {primitive}: dropped {dropped} skin influence(s) without a joint in its node's skin"
        );
    }
}

fn load_primitive(
    primitive: &gltf::Primitive,
    transform: Mat4,
    buffers: &[gltf::buffer::Data],
    textures: &[Arc<CpuTexture>],
    options: &LoadOptions,
) -> anyhow::Result<Option<CpuPrimitive>> {
    let Some(geometry) = primitive_geometry(primitive, buffers, options)? else {
        return Ok(None);
    };
    // Weights start at zero: the caller applies mesh/node defaults, and animation drives them.
    let morph_weights = vec![0.0f32; geometry.morph_targets.len()];

    Ok(Some(CpuPrimitive {
        vertices: geometry.vertices,
        indices: geometry.indices,
        transform,
        node_index: None,
        skin_index: None,
        material: load_material(&primitive.material(), textures),
        morph_targets: geometry.morph_targets,
        morph_weights,
    }))
}

/// A primitive's triangle list with generated normals and tangents, in its mesh's space; `None` when nothing draws.
pub(crate) fn primitive_geometry(
    primitive: &gltf::Primitive,
    buffers: &[gltf::buffer::Data],
    load: &LoadOptions,
) -> anyhow::Result<Option<Geometry>> {
    let Some(Read {
        vertices,
        has_normals,
        has_tangents,
    }) = read_vertices(primitive, buffers)?
    else {
        return Ok(None);
    };
    let Some(indices) = read_triangles(primitive, buffers, vertices.len()) else {
        return Ok(None);
    };
    let options = GeometryOptions {
        has_normals,
        has_tangents,
        // glTF generates tangents from the UV set the normal texture samples.
        tangent_uv_set: primitive.material().normal_texture().map_or(0, |info| {
            let transform = info
                .extensions()
                .and_then(|ext| ext.get("KHR_texture_transform"));
            raw_tex_coord(info.tex_coord(), transform)
        }),
        mikktspace: load.mikktspace_tangents,
    };
    let geometry = compacted(Geometry {
        vertices,
        indices,
        morph_targets: read_morph_targets(primitive, buffers),
    });
    Ok(Some(finish_geometry(geometry, &options)))
}

/// Only the vertices the indices address, in file order: primitives sharing one accessor each copied all of it.
fn compacted(geometry: Geometry) -> Geometry {
    let mut used = vec![false; geometry.vertices.len()];
    for &index in &geometry.indices {
        used[index as usize] = true;
    }
    if used.iter().all(|&kept| kept) {
        return geometry;
    }
    let sources: Vec<u32> = (0..geometry.vertices.len() as u32)
        .filter(|&old| used[old as usize])
        .collect();
    let mut remap = vec![0u32; geometry.vertices.len()];
    for (new, &old) in sources.iter().enumerate() {
        remap[old as usize] = new as u32;
    }
    Geometry {
        vertices: sources
            .iter()
            .map(|&old| geometry.vertices[old as usize])
            .collect(),
        indices: geometry
            .indices
            .iter()
            .map(|&old| remap[old as usize])
            .collect(),
        morph_targets: gather_morph_targets(&geometry.morph_targets, &sources),
    }
}

/// A primitive's vertices, triangle list and morph targets, kept together while vertices are rebuilt.
pub(crate) struct Geometry {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub morph_targets: Vec<MorphTarget>,
}

/// Which attributes the file supplied, and how missing tangents are generated.
struct GeometryOptions {
    has_normals: bool,
    has_tangents: bool,
    /// The normal texture's `texCoord`; 1 means TEXCOORD_1, anything else UV0, as the shader samples it.
    tangent_uv_set: u32,
    mikktspace: bool,
}

/// Generates what the file left out (flat normals, tangents) and repairs what NaNs the shader.
fn finish_geometry(mut geometry: Geometry, options: &GeometryOptions) -> Geometry {
    if !options.has_normals {
        // Flat shading needs a vertex per corner: a shared vertex kept its last face's normal.
        geometry = unindexed(geometry);
        compute_flat_normals(&mut geometry.vertices, &geometry.indices);
    }
    repair_normals(&mut geometry.vertices, &geometry.indices);
    if !options.has_tangents {
        let uv_set = options.tangent_uv_set;
        compute_tangents(&mut geometry.vertices, &geometry.indices, uv_set);
        // Opt-in MikkTSpace replaces only tangents we generated; a file's own tangents are kept.
        if options.mikktspace {
            if let Some((vertices, indices, sources)) =
                generate_tangents_mikktspace(&geometry.vertices, &geometry.indices, uv_set)
            {
                // MikkTSpace renumbers and splits vertices; the deltas must follow them.
                geometry = Geometry {
                    morph_targets: gather_morph_targets(&geometry.morph_targets, &sources),
                    vertices,
                    indices,
                };
            }
        }
    }
    repair_tangents(&mut geometry.vertices);
    geometry
}

/// One vertex per triangle corner, with each morph target's deltas gathered alongside.
fn unindexed(geometry: Geometry) -> Geometry {
    Geometry {
        vertices: geometry
            .indices
            .iter()
            .map(|&i| geometry.vertices[i as usize])
            .collect(),
        morph_targets: gather_morph_targets(&geometry.morph_targets, &geometry.indices),
        indices: (0..geometry.indices.len() as u32).collect(),
    }
}

/// Re-indexes each target so new vertex `i` carries the deltas of old vertex `source[i]`.
fn gather_morph_targets(targets: &[MorphTarget], source: &[u32]) -> Vec<MorphTarget> {
    let gather = |deltas: &Vec<Vec3>| -> Vec<Vec3> {
        if deltas.is_empty() {
            return Vec::new();
        }
        source
            .iter()
            .map(|&i| deltas.get(i as usize).copied().unwrap_or(Vec3::ZERO))
            .collect()
    };
    targets
        .iter()
        .map(|target| MorphTarget {
            position_deltas: gather(&target.position_deltas),
            normal_deltas: gather(&target.normal_deltas),
            tangent_deltas: gather(&target.tangent_deltas),
        })
        .collect()
}

/// A primitive's vertices, and which of the attributes the loader generates the file supplied.
struct Read {
    vertices: Vec<Vertex>,
    has_normals: bool,
    has_tangents: bool,
}

/// Every vertex attribute with its glTF fallback; `None` without POSITION.
fn read_vertices(
    primitive: &gltf::Primitive,
    buffers: &[gltf::buffer::Data],
) -> anyhow::Result<Option<Read>> {
    use gltf::mesh::Semantic;
    let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(|b| &b.0[..]));
    // KHR_mesh_quantization stores these as integers, which gltf's own readers take for floats.
    let accessor = |semantic: Semantic| primitive.get(&semantic);
    let Some(positions) = accessor(Semantic::Positions).and_then(|a| read_f32::<3>(a, buffers))
    else {
        return Ok(None);
    };
    let count = positions.len();

    let normals = accessor(Semantic::Normals).and_then(|a| read_f32::<3>(a, buffers));
    let has_normals = normals.is_some();
    let normals = normals.unwrap_or_else(|| vec![[0.0, 0.0, 0.0]; count]);
    let uvs = accessor(Semantic::TexCoords(0))
        .and_then(|a| read_f32::<2>(a, buffers))
        .unwrap_or_else(|| vec![[0.0, 0.0]; count]);
    // TEXCOORD_1 falls back to UV0, so a slot wrongly flagged UV1 still samples something sane.
    let uvs1 = accessor(Semantic::TexCoords(1))
        .and_then(|a| read_f32::<2>(a, buffers))
        .unwrap_or_else(|| uvs.clone());
    let joints: Vec<[f32; 4]> = reader.read_joints(0).map_or_else(
        || vec![[0.0; 4]; count],
        |iter| iter.into_u16().map(|j| j.map(f32::from)).collect(),
    );
    let weights: Vec<[f32; 4]> = reader
        .read_weights(0)
        .map_or_else(|| vec![[0.0; 4]; count], |iter| iter.into_f32().collect());
    // COLOR_0 defaults to white, so multiplying it into albedo is a no-op.
    let colors: Vec<[f32; 4]> = reader.read_colors(0).map_or_else(
        || vec![[1.0, 1.0, 1.0, 1.0]; count],
        |iter| iter.into_rgba_f32().collect(),
    );
    let tangents = accessor(Semantic::Tangents).and_then(|a| read_f32::<4>(a, buffers));
    let has_tangents = tangents.is_some();
    let tangents = tangents.unwrap_or_else(|| vec![[0.0, 0.0, 0.0, 0.0]; count]);

    anyhow::ensure!(
        normals.len() == count && uvs.len() == count && tangents.len() == count,
        "Attribute count mismatch: {} positions, {} normals, {} uvs, {} tangents",
        count,
        normals.len(),
        uvs.len(),
        tangents.len()
    );

    let vertices = (0..count)
        .map(|i| Vertex {
            position: positions[i],
            normal: normals[i],
            uv: uvs[i],
            tangent: tangents[i],
            joints: joints.get(i).copied().unwrap_or([0.0; 4]),
            weights: weights.get(i).copied().unwrap_or([0.0; 4]),
            color: colors.get(i).copied().unwrap_or([1.0, 1.0, 1.0, 1.0]),
            uv1: uvs1.get(i).copied().unwrap_or(uvs[i]),
        })
        .collect();
    Ok(Some(Read {
        vertices,
        has_normals,
        has_tangents,
    }))
}

/// The primitive as a triangle list whose every index addresses one of its `vertex_count` vertices.
/// `None` when no triangle survives: an empty index buffer panics wgpu's `slice(..)` on the first frame.
fn read_triangles(
    primitive: &gltf::Primitive,
    buffers: &[gltf::buffer::Data],
    vertex_count: usize,
) -> Option<Vec<u32>> {
    let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(|b| &b.0[..]));
    let raw_indices: Vec<u32> = match reader.read_indices() {
        Some(iter) => iter.into_u32().collect(),
        None => (0..vertex_count as u32).collect(),
    };
    // Triangulating on load keeps culling, LOD, QEM and drawing on one triangle-list representation.
    let indices = triangulate(&raw_indices, primitive.mode());
    // Sanitize before flat normals, tangents and MikkTSpace index `vertices` with these values.
    let (indices, dropped_triangles) = drop_out_of_range_triangles(indices, vertex_count);
    if dropped_triangles > 0 {
        log::warn!(
            "glTF primitive {}: dropped {} triangle(s) whose indices do not address any of its {} vertices",
            primitive.index(),
            dropped_triangles,
            vertex_count
        );
    }
    if indices.is_empty() {
        log::warn!(
            "glTF primitive {}: skipped, no triangle survives triangulation and the index checks",
            primitive.index()
        );
        return None;
    }
    Some(indices)
}

fn read_morph_targets(
    primitive: &gltf::Primitive,
    buffers: &[gltf::buffer::Data],
) -> Vec<MorphTarget> {
    let deltas = |accessor: Option<gltf::Accessor>| -> Vec<Vec3> {
        accessor
            .and_then(|a| read_f32::<3>(a, buffers))
            .map(|values| values.into_iter().map(Vec3::from_array).collect())
            .unwrap_or_default()
    };
    primitive
        .morph_targets()
        .map(|target| MorphTarget {
            position_deltas: deltas(target.positions()),
            normal_deltas: deltas(target.normals()),
            // glTF morph TANGENT deltas are vec3: the base tangent's w handedness is never morphed.
            tangent_deltas: deltas(target.tangents()),
        })
        .collect()
}

/// The metallic-roughness material with each slot's texture, sampler, UV set and transform.
pub(crate) fn load_material(
    material: &gltf::Material,
    textures: &[Arc<CpuTexture>],
) -> CpuMaterial {
    let pbr = material.pbr_metallic_roughness();
    let alpha_mode = match material.alpha_mode() {
        gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
        gltf::material::AlphaMode::Mask => AlphaMode::Mask(material.alpha_cutoff().unwrap_or(0.5)),
        gltf::material::AlphaMode::Blend => AlphaMode::Blend,
    };
    let [base_uv_transform, mr_uv_transform, normal_uv_transform, emissive_uv_transform, occlusion_uv_transform] =
        slot_uv_transforms(material);
    CpuMaterial {
        base_color: pbr.base_color_factor(),
        base_uv_transform,
        mr_uv_transform,
        normal_uv_transform,
        emissive_uv_transform,
        occlusion_uv_transform,
        alpha_mode,
        metallic_factor: pbr.metallic_factor(),
        roughness_factor: pbr.roughness_factor(),
        // KHR_materials_emissive_strength folds into the factor so the shader path stays unchanged.
        emissive_factor: {
            let ef = material.emissive_factor();
            let strength = material.emissive_strength().unwrap_or(1.0);
            [ef[0] * strength, ef[1] * strength, ef[2] * strength]
        },
        occlusion_strength: material
            .occlusion_texture()
            .map_or(1.0, |occ| occ.strength()),
        normal_scale: material.normal_texture().map_or(1.0, |nrm| nrm.scale()),
        double_sided: material.double_sided(),
        unlit: material.unlit(),
        base_color_texture: pbr
            .base_color_texture()
            .and_then(|info| texture_ref(&info.texture(), textures, true)),
        metallic_roughness_texture: pbr
            .metallic_roughness_texture()
            .and_then(|info| texture_ref(&info.texture(), textures, false)),
        normal_texture: material
            .normal_texture()
            .and_then(|info| texture_ref(&info.texture(), textures, false)),
        emissive_texture: material
            .emissive_texture()
            .and_then(|info| texture_ref(&info.texture(), textures, true)),
        // Occlusion is the slot most likely on UV1: baked AO is a standard Blender/Substance export.
        occlusion_texture: material
            .occlusion_texture()
            .and_then(|info| texture_ref(&info.texture(), textures, false)),
        uv_set_mask: uv_set_mask(material),
    }
}

/// KHR_texture_transform per slot (base, MR, normal, emissive, occlusion): glTF scopes it to `textureInfo`.
fn slot_uv_transforms(material: &gltf::Material) -> [[[f32; 3]; 2]; 5] {
    let pbr = material.pbr_metallic_roughness();
    // `.extensions()` borrows from the texture-info value itself, so it must outlive the chain.
    let normal_texture_info = material.normal_texture();
    let occlusion_texture_info = material.occlusion_texture();
    [
        uv_transform_from_info(
            pbr.base_color_texture()
                .and_then(|info| info.texture_transform()),
        ),
        uv_transform_from_info(
            pbr.metallic_roughness_texture()
                .and_then(|info| info.texture_transform()),
        ),
        uv_transform_from_extension_json(
            normal_texture_info
                .as_ref()
                .and_then(|info| info.extensions())
                .and_then(|ext| ext.get("KHR_texture_transform")),
        ),
        uv_transform_from_info(
            material
                .emissive_texture()
                .and_then(|info| info.texture_transform()),
        ),
        uv_transform_from_extension_json(
            occlusion_texture_info
                .as_ref()
                .and_then(|info| info.extensions())
                .and_then(|ext| ext.get("KHR_texture_transform")),
        ),
    ]
}

/// Which slots sample UV1 (bit per slot: 0 base .. 4 occlusion).
fn uv_set_mask(material: &gltf::Material) -> u32 {
    let pbr = material.pbr_metallic_roughness();
    pbr.base_color_texture()
        .map_or(0, |i| uv_set_bit("base color", info_tex_coord(&i), 0))
        | pbr.metallic_roughness_texture().map_or(0, |i| {
            uv_set_bit("metallic-roughness", info_tex_coord(&i), 1)
        })
        | material.normal_texture().map_or(0, |i| {
            let transform = i
                .extensions()
                .and_then(|ext| ext.get("KHR_texture_transform"));
            uv_set_bit("normal", raw_tex_coord(i.tex_coord(), transform), 2)
        })
        | material
            .emissive_texture()
            .map_or(0, |i| uv_set_bit("emissive", info_tex_coord(&i), 3))
        | material.occlusion_texture().map_or(0, |i| {
            let transform = i
                .extensions()
                .and_then(|ext| ext.get("KHR_texture_transform"));
            uv_set_bit("occlusion", raw_tex_coord(i.tex_coord(), transform), 4)
        })
}

/// A typed slot's UV set: KHR_texture_transform's `texCoord` overrides the textureInfo's, as that extension says.
fn info_tex_coord(info: &gltf::texture::Info) -> u32 {
    info.texture_transform()
        .and_then(|t| t.tex_coord())
        .unwrap_or_else(|| info.tex_coord())
}

/// [`info_tex_coord`] for the normal and occlusion slots, whose extension `gltf` leaves as raw JSON.
fn raw_tex_coord(tex_coord: u32, transform: Option<&gltf::json::Value>) -> u32 {
    transform
        .and_then(|t| t.get("texCoord"))
        .and_then(gltf::json::Value::as_u64)
        .map_or(tex_coord, |set| set as u32)
}

fn compute_flat_normals(vertices: &mut [Vertex], indices: &[u32]) {
    for tri in indices.as_chunks::<3>().0 {
        let [i0, i1, i2] = tri.map(|i| i as usize);
        let p0 = Vec3::from_array(vertices[i0].position);
        let p1 = Vec3::from_array(vertices[i1].position);
        let p2 = Vec3::from_array(vertices[i2].position);
        // A zero-area face has no normal; writing its zero would erase a shared corner's real one.
        let Some(n) = (p1 - p0).cross(p2 - p0).try_normalize() else {
            continue;
        };
        for i in [i0, i1, i2] {
            vertices[i].normal = n.to_array();
        }
    }
}

/// Replaces zero or non-finite normals, which the shader's `normalize` turns into NaN shading.
/// The replacement is the corner's area-weighted face normal, else +Y.
fn repair_normals(vertices: &mut [Vertex], indices: &[u32]) {
    let broken = |n: [f32; 3]| Vec3::from_array(n).try_normalize().is_none();
    if !vertices.iter().any(|v| broken(v.normal)) {
        return;
    }
    let mut face_sums = vec![Vec3::ZERO; vertices.len()];
    for tri in indices.as_chunks::<3>().0 {
        let [a, b, c] = tri.map(|i| Vec3::from_array(vertices[i as usize].position));
        let face = (b - a).cross(c - a);
        if face.is_finite() {
            for &i in tri {
                face_sums[i as usize] += face;
            }
        }
    }
    for (vertex, sum) in vertices.iter_mut().zip(face_sums) {
        if broken(vertex.normal) {
            vertex.normal = sum.try_normalize().unwrap_or(Vec3::Y).to_array();
        }
    }
}

/// A zero or non-finite tangent NaNs the shader's TBN even without a normal map; `w` must be a sign.
fn repair_tangents(vertices: &mut [Vertex]) {
    for vertex in vertices {
        let t = Vec3::new(vertex.tangent[0], vertex.tangent[1], vertex.tangent[2]);
        if t.try_normalize().is_none() {
            let p = any_perpendicular(Vec3::from_array(vertex.normal));
            vertex.tangent[..3].copy_from_slice(&p.to_array());
        }
        let w = vertex.tangent[3];
        if w != 1.0 && w != -1.0 {
            vertex.tangent[3] = if w < 0.0 { -1.0 } else { 1.0 };
        }
    }
}

/// Any unit vector perpendicular to the unit `n`, for a tangent with no UV gradient to follow.
fn any_perpendicular(n: Vec3) -> Vec3 {
    n.cross(Vec3::Y)
        .try_normalize()
        .or_else(|| n.cross(Vec3::X).try_normalize())
        .unwrap_or(Vec3::X)
}

/// The UV set `uv_set` names: TEXCOORD_1 for 1, else TEXCOORD_0.
fn tangent_uv(vertex: &Vertex, uv_set: u32) -> [f32; 2] {
    if uv_set == 1 {
        vertex.uv1
    } else {
        vertex.uv
    }
}

/// Per-vertex tangent frame from the UV gradients of `uv_set` (Lengyel), with glTF handedness in `.w`.
/// Never splits vertices at UV seams; [`generate_tangents_mikktspace`] does.
pub(crate) fn compute_tangents(vertices: &mut [Vertex], indices: &[u32], uv_set: u32) {
    let mut tan_accum = vec![Vec3::ZERO; vertices.len()];
    let mut bitan_accum = vec![Vec3::ZERO; vertices.len()];

    for tri in indices.as_chunks::<3>().0 {
        let [i0, i1, i2] = tri.map(|i| i as usize);
        let p0 = Vec3::from_array(vertices[i0].position);
        let p1 = Vec3::from_array(vertices[i1].position);
        let p2 = Vec3::from_array(vertices[i2].position);
        let u0 = Vec2::from_array(tangent_uv(&vertices[i0], uv_set));
        let u1 = Vec2::from_array(tangent_uv(&vertices[i1], uv_set));
        let u2 = Vec2::from_array(tangent_uv(&vertices[i2], uv_set));

        let e1 = p1 - p0;
        let e2 = p2 - p0;
        let d1 = u1 - u0;
        let d2 = u2 - u0;

        let det = d1.x * d2.y - d2.x * d1.y;
        if det.abs() < 1e-8 {
            continue;
        }
        let r = 1.0 / det;
        let tangent = (e1 * d2.y - e2 * d1.y) * r;
        let bitangent = (e2 * d1.x - e1 * d2.x) * r;
        for i in [i0, i1, i2] {
            tan_accum[i] += tangent;
            bitan_accum[i] += bitangent;
        }
    }

    for ((vertex, tangent), bitangent) in vertices.iter_mut().zip(tan_accum).zip(bitan_accum) {
        let n = Vec3::from_array(vertex.normal);
        // Gram-Schmidt against the normal; any perpendicular axis for degenerate UVs.
        let t = (tangent - n * n.dot(tangent))
            .try_normalize()
            .unwrap_or_else(|| any_perpendicular(n));
        // Degenerate accumulation gets +1, since 0 would zero the shader's bitangent.
        let w = if n.cross(t).dot(bitangent) < 0.0 {
            -1.0
        } else {
            1.0
        };
        vertex.tangent = [t.x, t.y, t.z, w];
    }
}

/// The mesh as `bevy_mikktspace` reads it, collecting one tangent per triangle corner.
struct MikkMesh<'a> {
    vertices: &'a [Vertex],
    indices: &'a [u32],
    uv_set: u32,
    /// Per-corner tangent, length `num_faces * 3`, filled by the algorithm.
    tangents: Vec<[f32; 4]>,
}

impl MikkMesh<'_> {
    fn corner(&self, face: usize, vert: usize) -> usize {
        self.indices[face * 3 + vert] as usize
    }
}

impl bevy_mikktspace::Geometry for MikkMesh<'_> {
    fn num_faces(&self) -> usize {
        self.indices.len() / 3
    }
    fn num_vertices_of_face(&self, _face: usize) -> usize {
        3
    }
    fn position(&self, face: usize, vert: usize) -> [f32; 3] {
        self.vertices[self.corner(face, vert)].position
    }
    fn normal(&self, face: usize, vert: usize) -> [f32; 3] {
        self.vertices[self.corner(face, vert)].normal
    }
    fn tex_coord(&self, face: usize, vert: usize) -> [f32; 2] {
        tangent_uv(&self.vertices[self.corner(face, vert)], self.uv_set)
    }
    fn set_tangent(
        &mut self,
        tangent_space: Option<bevy_mikktspace::TangentSpace>,
        face: usize,
        vert: usize,
    ) {
        // A `None` space (degenerate corner) keeps the initialized default.
        if let Some(ts) = tangent_space {
            self.tangents[face * 3 + vert] = ts.tangent_encoded();
        }
    }
}

/// MikkTSpace tangents from `uv_set`, the basis DCC tools bake normal maps against ([`LoadOptions::mikktspace_tangents`]).
/// Splits vertices at seams, so it returns fresh buffers and each new vertex's source; `None` keeps the caller's.
pub(crate) fn generate_tangents_mikktspace(
    vertices: &[Vertex],
    indices: &[u32],
    uv_set: u32,
) -> Option<(Vec<Vertex>, Vec<u32>, Vec<u32>)> {
    if indices.len() < 3 || !indices.len().is_multiple_of(3) {
        return None;
    }
    let mut mesh = MikkMesh {
        vertices,
        indices,
        uv_set,
        tangents: vec![[0.0, 0.0, 0.0, 1.0]; indices.len()],
    };
    if bevy_mikktspace::generate_tangents(&mut mesh).is_err() {
        return None;
    }

    // MikkTSpace emits bit-identical tangents for shared corners, so keying on the bits splits only seams.
    let mut remap: std::collections::HashMap<(u32, [u32; 4]), u32> =
        std::collections::HashMap::new();
    let mut out_vertices: Vec<Vertex> = Vec::with_capacity(vertices.len());
    let mut out_indices: Vec<u32> = Vec::with_capacity(indices.len());
    let mut sources: Vec<u32> = Vec::with_capacity(vertices.len());
    for (corner, &orig) in indices.iter().enumerate() {
        let tangent = mesh.tangents[corner];
        let key = (orig, tangent.map(f32::to_bits));
        let new_index = *remap.entry(key).or_insert_with(|| {
            let mut v = vertices[orig as usize];
            v.tangent = tangent;
            out_vertices.push(v);
            sources.push(orig);
            (out_vertices.len() - 1) as u32
        });
        out_indices.push(new_index);
    }
    Some((out_vertices, out_indices, sources))
}

#[cfg(test)]
mod tests {

    #[test]
    fn strips_and_fans_expand_to_triangle_lists() {
        use gltf::mesh::Mode;
        let idx = [0u32, 1, 2, 3, 4];

        // The odd strip triangle swaps its first two indices, or it comes out back-facing.
        let strip = triangulate(&idx, Mode::TriangleStrip);
        assert_eq!(strip, vec![0, 1, 2, /*swapped*/ 2, 1, 3, 2, 3, 4]);

        // Fan: every triangle shares index 0.
        let fan = triangulate(&idx, Mode::TriangleFan);
        assert_eq!(fan, vec![0, 1, 2, 0, 2, 3, 0, 3, 4]);

        // Triangles pass through untouched.
        assert_eq!(triangulate(&idx, Mode::Triangles), idx.to_vec());

        // Degenerate input must not panic or underflow.
        assert!(triangulate(&[0, 1], Mode::TriangleStrip).is_empty());
        assert!(triangulate(&[], Mode::TriangleFan).is_empty());
    }

    #[test]
    fn out_of_range_indices_drop_their_triangle() {
        // Vertex 7 was never shipped; unguarded, compute_flat_normals panics on it.
        let (kept, dropped) = drop_out_of_range_triangles(vec![0, 1, 2, 7, 1, 2], 3);
        assert_eq!(kept, vec![0, 1, 2], "only the in-range triangle survives");
        assert_eq!(dropped, 1);
        assert!(
            kept.len().is_multiple_of(3),
            "the list must stay a triangle list"
        );
        assert!(
            kept.iter().all(|&i| (i as usize) < 3),
            "every surviving index must address a real vertex"
        );

        // The whole triangle goes: dropping only the corner would re-wind its neighbours.
        let (kept, dropped) = drop_out_of_range_triangles(vec![9, 1, 2, 0, 1, 2], 3);
        assert_eq!(kept, vec![0, 1, 2]);
        assert_eq!(dropped, 1);

        // The oracle: these consumers panic on the unfiltered list.
        let mut vertices: Vec<Vertex> = (0..3)
            .map(|i| Vertex {
                position: [i as f32, 0.0, 0.0],
                normal: [0.0, 0.0, 0.0],
                uv: [i as f32, 0.0],
                tangent: [0.0; 4],
                joints: [0.0; 4],
                weights: [0.0; 4],
                color: [1.0, 1.0, 1.0, 1.0],
                uv1: [0.0, 0.0],
            })
            .collect();
        let (filtered, dropped) =
            drop_out_of_range_triangles(vec![0, 1, 2, 3, 4, 5], vertices.len());
        assert_eq!(dropped, 1);
        compute_flat_normals(&mut vertices, &filtered);
        compute_tangents(&mut vertices, &filtered, 0);

        // A trailing partial triangle is truncated but not counted as dropped.
        assert_eq!(drop_out_of_range_triangles(vec![], 3), (vec![], 0));
        assert_eq!(drop_out_of_range_triangles(vec![5, 6, 7], 3), (vec![], 1));
        assert_eq!(
            drop_out_of_range_triangles(vec![0, 1, 2, 0], 3),
            (vec![0, 1, 2], 0)
        );
        assert_eq!(drop_out_of_range_triangles(vec![0, 1], 3), (vec![], 0));
        assert_eq!(
            drop_out_of_range_triangles(vec![0, 1, 2, 2, 1, 0], 3),
            (vec![0, 1, 2, 2, 1, 0], 0),
            "a fully in-range list must pass through unchanged"
        );
        // vertex_count 0 (no POSITION data) must reject everything.
        assert_eq!(drop_out_of_range_triangles(vec![0, 0, 0], 0), (vec![], 1));
    }

    #[test]
    fn sixteen_bit_images_down_convert_instead_of_failing_the_whole_file() {
        // 2x1 R16G16B16A16, little-endian.
        let texel = |r: u16, g: u16, b: u16, a: u16| {
            let mut v = Vec::new();
            for c in [r, g, b, a] {
                v.extend_from_slice(&c.to_le_bytes());
            }
            v
        };
        let mut pixels = texel(0xFFFF, 0x0000, 0x8000, 0xFFFF);
        pixels.extend(texel(0x0000, 0xFF00, 0x0000, 0x1234));

        let data = gltf::image::Data {
            pixels,
            format: gltf::image::Format::R16G16B16A16,
            width: 2,
            height: 1,
        };
        let tex = to_rgba8(data).expect("16-bit image must convert, not bail");
        assert_eq!(tex.rgba8.len(), 2 * 4, "must be tightly packed RGBA8");
        // High byte survives: 0xFFFF -> 0xFF, 0x8000 -> 0x80, 0x1234 -> 0x12.
        assert_eq!(&tex.rgba8[0..4], &[0xFF, 0x00, 0x80, 0xFF]);
        assert_eq!(&tex.rgba8[4..8], &[0x00, 0xFF, 0x00, 0x12]);
    }
    use super::*;

    #[test]
    fn tangents_follow_uv_gradient() {
        // Unit quad in the XY plane, UVs aligned with X/Y: tangent must be +X.
        let mut vertices: Vec<Vertex> = [
            ([0.0, 0.0, 0.0], [0.0, 0.0]),
            ([1.0, 0.0, 0.0], [1.0, 0.0]),
            ([1.0, 1.0, 0.0], [1.0, 1.0]),
            ([0.0, 1.0, 0.0], [0.0, 1.0]),
        ]
        .iter()
        .map(|(p, uv)| Vertex {
            position: *p,
            normal: [0.0, 0.0, 1.0],
            uv: *uv,
            tangent: [0.0; 4],
            joints: [0.0; 4],
            weights: [0.0; 4],
            color: [1.0, 1.0, 1.0, 1.0],
            uv1: [0.0, 0.0],
        })
        .collect();
        let indices = [0u32, 1, 2, 0, 2, 3];

        compute_tangents(&mut vertices, &indices, 0);

        for vertex in &vertices {
            assert!(
                (vertex.tangent[0] - 1.0).abs() < 1e-4
                    && vertex.tangent[1].abs() < 1e-4
                    && vertex.tangent[2].abs() < 1e-4,
                "tangent should be +X, got {:?}",
                vertex.tangent
            );
            // Right-handed UVs: handedness must be +1.
            assert_eq!(vertex.tangent[3], 1.0, "expected +1 handedness");
        }
    }

    fn quad_with_uvs(uvs: [[f32; 2]; 4]) -> (Vec<Vertex>, [u32; 6]) {
        let pos = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let vertices = pos
            .iter()
            .zip(uvs)
            .map(|(p, uv)| Vertex {
                position: *p,
                normal: [0.0, 0.0, 1.0],
                uv,
                tangent: [0.0; 4],
                joints: [0.0; 4],
                weights: [0.0; 4],
                color: [1.0, 1.0, 1.0, 1.0],
                uv1: [0.0, 0.0],
            })
            .collect();
        (vertices, [0u32, 1, 2, 0, 2, 3])
    }

    #[test]
    fn mirrored_uvs_flip_handedness() {
        // Mirroring V keeps the +X tangent but makes the chart left-handed, so w must flip to -1.
        let (mut normal, idx) = quad_with_uvs([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        compute_tangents(&mut normal, &idx, 0);

        let (mut mirrored, idx) = quad_with_uvs([[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
        compute_tangents(&mut mirrored, &idx, 0);

        for v in &normal {
            assert_eq!(v.tangent[3], 1.0, "unmirrored chart is right-handed");
        }
        for v in &mirrored {
            assert_eq!(
                v.tangent[3], -1.0,
                "mirrored chart must be left-handed, got {:?}",
                v.tangent
            );
        }
    }

    #[test]
    fn mikktspace_quad_tangent_is_x() {
        // Aligned UVs have no seam, so the corners must weld back to the original 4 vertices.
        let (vertices, indices) = quad_with_uvs([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        let (out_v, out_i, _) = generate_tangents_mikktspace(&vertices, &indices, 0)
            .expect("mikktspace should succeed");
        assert_eq!(out_i.len(), indices.len(), "index count is preserved");
        assert_eq!(
            out_v.len(),
            4,
            "aligned UVs have no seam, corners weld to 4 verts"
        );
        for v in &out_v {
            let t = v.tangent;
            let len = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
            assert!(
                (len - 1.0).abs() < 1e-3,
                "tangent must be unit length, got {len}"
            );
            assert!(
                (t[0].abs() - 1.0).abs() < 1e-3 && t[1].abs() < 1e-3 && t[2].abs() < 1e-3,
                "tangent should lie along X, got {:?}",
                t
            );
            assert_eq!(t[3], 1.0, "right-handed chart -> +1 handedness");
        }
    }

    #[test]
    fn mikktspace_mirrored_uvs_flip_handedness() {
        // V mirrored: handedness must flip to -1, as on the Lengyel path.
        let (vertices, indices) = quad_with_uvs([[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
        let (out_v, _, _) = generate_tangents_mikktspace(&vertices, &indices, 0)
            .expect("mikktspace should succeed");
        for v in &out_v {
            assert_eq!(
                v.tangent[3], -1.0,
                "mirrored chart must be left-handed, got {:?}",
                v.tangent
            );
        }
    }

    #[test]
    fn morph_deltas_follow_the_vertices_mikktspace_renumbers() {
        // First use 3, 0, 2, 1: MikkTSpace's output order, which the file's delta order is not.
        let (vertices, _) = quad_with_uvs([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        let indices = vec![3, 0, 2, 2, 0, 1];
        let deltas: Vec<Vec3> = (0..4)
            .map(|i| Vec3::new(0.0, 0.0, i as f32 + 1.0))
            .collect();
        let options = GeometryOptions {
            has_normals: true,
            has_tangents: false,
            tangent_uv_set: 0,
            mikktspace: true,
        };
        let geometry = finish_geometry(
            Geometry {
                vertices: vertices.clone(),
                indices,
                morph_targets: vec![MorphTarget {
                    position_deltas: deltas.clone(),
                    ..MorphTarget::default()
                }],
            },
            &options,
        );
        let moved = &geometry.morph_targets[0].position_deltas;
        assert_eq!(moved.len(), geometry.vertices.len());
        for (vertex, delta) in geometry.vertices.iter().zip(moved) {
            let source = vertices
                .iter()
                .position(|v| v.position == vertex.position)
                .expect("every output vertex comes from an input vertex");
            assert_eq!(*delta, deltas[source], "vertex at {:?}", vertex.position);
        }
    }

    #[test]
    fn compaction_keeps_the_used_vertices_in_file_order_with_their_morph_deltas() {
        let (mut vertices, _) = quad_with_uvs([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        vertices.push(vertices[0]);
        let deltas: Vec<Vec3> = (0..5).map(|i| Vec3::splat(i as f32)).collect();
        let geometry = compacted(Geometry {
            vertices: vertices.clone(),
            indices: vec![3, 1, 4],
            morph_targets: vec![MorphTarget {
                position_deltas: deltas,
                normal_deltas: Vec::new(),
                tangent_deltas: vec![Vec3::X; 2],
            }],
        });
        assert_eq!(geometry.indices, [1, 0, 2], "old 1, 3, 4 become 0, 1, 2");
        let positions: Vec<[f32; 3]> = geometry.vertices.iter().map(|v| v.position).collect();
        assert_eq!(
            positions,
            [
                vertices[1].position,
                vertices[3].position,
                vertices[4].position
            ]
        );
        let target = &geometry.morph_targets[0];
        assert_eq!(
            target.position_deltas,
            [Vec3::splat(1.0), Vec3::splat(3.0), Vec3::splat(4.0)]
        );
        assert!(
            target.normal_deltas.is_empty(),
            "an absent attribute stays absent"
        );
        assert_eq!(
            target.tangent_deltas,
            [Vec3::X, Vec3::ZERO, Vec3::ZERO],
            "a short list pads with zero"
        );

        let all = compacted(Geometry {
            vertices: vertices.clone(),
            indices: vec![0, 1, 2, 2, 3, 4],
            morph_targets: Vec::new(),
        });
        assert_eq!(
            all.indices,
            [0, 1, 2, 2, 3, 4],
            "nothing to drop leaves the geometry alone"
        );
        assert_eq!(all.vertices.len(), 5);
    }

    #[test]
    fn mikktspace_rejects_degenerate_input() {
        // None, so the caller keeps its buffers instead of getting empty geometry.
        let (vertices, _) = quad_with_uvs([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        assert!(generate_tangents_mikktspace(&vertices, &[0, 1], 0).is_none());
        assert!(generate_tangents_mikktspace(&vertices, &[], 0).is_none());
        assert!(generate_tangents_mikktspace(&vertices, &[0, 1, 2, 3], 0).is_none());
    }
}
