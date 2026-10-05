//! glTF 2.0 -> `CpuScene`: geometry, node transforms, animation and metallic-roughness materials.

use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;
use glam::{Mat4, Vec2, Vec3};

use crate::scene::{
    AlphaMode, ChannelValues, CpuAnimation, CpuAnimationChannel, CpuCamera, CpuCameraProjection,
    CpuLight, CpuLightKind, CpuMaterial, CpuNode, CpuPrimitive, CpuSampler, CpuScene, CpuSkin,
    CpuTexture, CpuTextureRef, CpuWrap, Interpolation, Vertex,
};

pub fn load_gltf(path: impl AsRef<Path>) -> anyhow::Result<CpuScene> {
    let path = path.as_ref();
    let (document, buffers, images) = gltf::import(path)
        .with_context(|| format!("Failed to import glTF file: {}", path.display()))?;
    build_scene(document, buffers, images)
        .with_context(|| format!("Failed to build scene from {}", path.display()))
}

/// In-memory variant for wasm32 and embedded assets; buffers must be embedded (data URIs or GLB).
pub fn load_gltf_slice(bytes: &[u8]) -> anyhow::Result<CpuScene> {
    let (document, buffers, images) =
        gltf::import_slice(bytes).context("Failed to import glTF from memory")?;
    build_scene(document, buffers, images)
}

fn build_scene(
    document: gltf::Document,
    buffers: Vec<gltf::buffer::Data>,
    images: Vec<gltf::image::Data>,
) -> anyhow::Result<CpuScene> {
    let textures: Vec<Arc<CpuTexture>> = images
        .into_iter()
        .map(|img| to_rgba8(img).map(Arc::new))
        .collect::<anyhow::Result<_>>()?;

    // Full node table (index-aligned with glTF node indices).
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

    let mut scene = CpuScene {
        nodes,
        ..CpuScene::default()
    };

    for animation in document.animations() {
        let mut channels = Vec::new();
        let mut duration = 0.0f32;
        for channel in animation.channels() {
            let reader = channel.reader(|buffer| buffers.get(buffer.index()).map(|b| &b.0[..]));
            let Some(times) = reader.read_inputs() else {
                continue;
            };
            let times: Vec<f32> = times.collect();
            if let Some(&last) = times.last() {
                duration = duration.max(last);
            }
            let Some(outputs) = reader.read_outputs() else {
                continue;
            };
            use gltf::animation::util::ReadOutputs;
            let values = match outputs {
                ReadOutputs::Translations(iter) => {
                    ChannelValues::Translation(iter.map(glam::Vec3::from_array).collect())
                }
                ReadOutputs::Rotations(rotations) => ChannelValues::Rotation(
                    rotations.into_f32().map(glam::Quat::from_array).collect(),
                ),
                ReadOutputs::Scales(iter) => {
                    ChannelValues::Scale(iter.map(glam::Vec3::from_array).collect())
                }
                ReadOutputs::MorphTargetWeights(w) => {
                    ChannelValues::MorphWeights(w.into_f32().collect())
                }
            };
            let interpolation = match channel.sampler().interpolation() {
                gltf::animation::Interpolation::Linear => Interpolation::Linear,
                gltf::animation::Interpolation::Step => Interpolation::Step,
                gltf::animation::Interpolation::CubicSpline => Interpolation::CubicSpline,
            };
            channels.push(CpuAnimationChannel {
                node: channel.target().node().index(),
                times,
                values,
                interpolation,
            });
        }
        if !channels.is_empty() {
            scene.animations.push(CpuAnimation {
                name: animation.name().unwrap_or("animation").to_string(),
                duration,
                channels,
            });
        }
    }

    // Skins: joint node indices + inverse bind matrices.
    for skin in document.skins() {
        let reader = skin.reader(|buffer| buffers.get(buffer.index()).map(|b| &b.0[..]));
        let inverse_bind_matrices = reader
            .read_inverse_bind_matrices()
            .map(|iter| iter.map(|m| Mat4::from_cols_array_2d(&m)).collect())
            .unwrap_or_default();
        scene.skins.push(CpuSkin {
            joints: skin.joints().map(|j| j.index()).collect(),
            inverse_bind_matrices,
        });
    }

    // Cameras authored in the file (pose = their node's world transform).
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
        scene.cameras.push(CpuCamera {
            name: camera.name().map(str::to_string),
            node: node.index(),
            projection,
        });
    }

    let gltf_scene = document
        .default_scene()
        .or_else(|| document.scenes().next())
        .context("glTF file contains no scenes")?;

    for node in gltf_scene.nodes() {
        visit_node(&node, Mat4::IDENTITY, &buffers, &textures, &mut scene)?;
    }

    anyhow::ensure!(
        !scene.primitives.is_empty(),
        "glTF file contains no triangle primitives"
    );

    Ok(scene)
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
fn to_rgba8(img: gltf::image::Data) -> anyhow::Result<CpuTexture> {
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

fn to_cpu_sampler(sampler: &gltf::texture::Sampler) -> CpuSampler {
    use gltf::texture::{MagFilter, MinFilter, WrappingMode};

    let wrap = |mode: WrappingMode| match mode {
        WrappingMode::ClampToEdge => CpuWrap::ClampToEdge,
        WrappingMode::MirroredRepeat => CpuWrap::MirroredRepeat,
        WrappingMode::Repeat => CpuWrap::Repeat,
    };

    let (min_nearest, mip_nearest) = match sampler.min_filter() {
        Some(MinFilter::Nearest | MinFilter::NearestMipmapNearest) => (true, true),
        Some(MinFilter::NearestMipmapLinear) => (true, false),
        Some(MinFilter::LinearMipmapNearest) => (false, true),
        Some(MinFilter::Linear | MinFilter::LinearMipmapLinear) | None => (false, false),
    };

    CpuSampler {
        mag_nearest: matches!(sampler.mag_filter(), Some(MagFilter::Nearest)),
        min_nearest,
        mip_nearest,
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
        .get(info.source().index())
        .cloned()
        .map(|texture| CpuTextureRef {
            texture,
            sampler: to_cpu_sampler(&info.sampler()),
            srgb,
        })
}

fn visit_node(
    node: &gltf::Node,
    parent_transform: Mat4,
    buffers: &[gltf::buffer::Data],
    textures: &[Arc<CpuTexture>],
    scene: &mut CpuScene,
) -> anyhow::Result<()> {
    let local = Mat4::from_cols_array_2d(&node.transform().matrix());
    let world = parent_transform * local;

    if let Some(light) = node.light() {
        use gltf::khr_lights_punctual::Kind;
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
        // glTF lights point down their node's -Z axis.
        let direction = world.transform_vector3(Vec3::NEG_Z).normalize_or_zero();
        scene.lights.push(CpuLight {
            kind,
            color: light.color(),
            intensity: light.intensity(),
            range: light.range().unwrap_or(0.0),
            position: world.transform_point3(Vec3::ZERO).to_array(),
            direction: direction.to_array(),
        });
    }

    if let Some(mesh) = node.mesh() {
        for primitive in mesh.primitives() {
            // Strips and fans are triangulated on load; points and lines have nothing to draw.
            use gltf::mesh::Mode;
            if !matches!(
                primitive.mode(),
                Mode::Triangles | Mode::TriangleStrip | Mode::TriangleFan
            ) {
                log::warn!(
                    "Skipping non-triangle primitive (mode {:?}) in mesh {:?}",
                    primitive.mode(),
                    mesh.name().unwrap_or("<unnamed>")
                );
                continue;
            }
            if let Some(mut cpu) = load_primitive(&primitive, world, buffers, textures)? {
                cpu.node_index = Some(node.index());
                cpu.skin_index = node.skin().map(|s| s.index());
                // Node weights override mesh weights (glTF spec); kept only when the count matches.
                if !cpu.morph_targets.is_empty() {
                    if let Some(weights) = node.weights().or_else(|| mesh.weights()) {
                        if weights.len() == cpu.morph_weights.len() {
                            cpu.morph_weights = weights.to_vec();
                        }
                    }
                }
                scene.primitives.push(cpu);
            }
        }
    }

    for child in node.children() {
        visit_node(&child, world, buffers, textures, scene)?;
    }
    Ok(())
}

fn load_primitive(
    primitive: &gltf::Primitive,
    transform: Mat4,
    buffers: &[gltf::buffer::Data],
    textures: &[Arc<CpuTexture>],
) -> anyhow::Result<Option<CpuPrimitive>> {
    let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(|b| &b.0[..]));

    let Some(positions) = reader.read_positions() else {
        return Ok(None);
    };
    let positions: Vec<[f32; 3]> = positions.collect();

    let normals: Vec<[f32; 3]> = match reader.read_normals() {
        Some(iter) => iter.collect(),
        None => vec![[0.0, 0.0, 0.0]; positions.len()],
    };
    let uvs: Vec<[f32; 2]> = match reader.read_tex_coords(0) {
        Some(iter) => iter.into_f32().collect(),
        None => vec![[0.0, 0.0]; positions.len()],
    };
    // TEXCOORD_1 falls back to UV0, so a slot wrongly flagged UV1 still samples something sane.
    let uvs1: Vec<[f32; 2]> = match reader.read_tex_coords(1) {
        Some(iter) => iter.into_f32().collect(),
        None => uvs.clone(),
    };
    let joints: Vec<[f32; 4]> = match reader.read_joints(0) {
        Some(iter) => iter
            .into_u16()
            .map(|j| [j[0] as f32, j[1] as f32, j[2] as f32, j[3] as f32])
            .collect(),
        None => vec![[0.0; 4]; positions.len()],
    };
    let weights: Vec<[f32; 4]> = match reader.read_weights(0) {
        Some(iter) => iter.into_f32().collect(),
        None => vec![[0.0; 4]; positions.len()],
    };
    // COLOR_0 defaults to white, so multiplying it into albedo is a no-op.
    let colors: Vec<[f32; 4]> = match reader.read_colors(0) {
        Some(iter) => iter.into_rgba_f32().collect(),
        None => vec![[1.0, 1.0, 1.0, 1.0]; positions.len()],
    };

    let tangents: Vec<[f32; 4]> = match reader.read_tangents() {
        Some(iter) => iter.collect(),
        None => vec![[0.0, 0.0, 0.0, 0.0]; positions.len()],
    };
    let had_tangents = reader.read_tangents().is_some();

    anyhow::ensure!(
        normals.len() == positions.len()
            && uvs.len() == positions.len()
            && tangents.len() == positions.len(),
        "Attribute count mismatch: {} positions, {} normals, {} uvs, {} tangents",
        positions.len(),
        normals.len(),
        uvs.len(),
        tangents.len()
    );

    let mut vertices: Vec<Vertex> = positions
        .iter()
        .zip(normals.iter())
        .zip(uvs.iter())
        .zip(tangents.iter())
        .enumerate()
        .map(|(i, (((p, n), t), tan))| Vertex {
            position: *p,
            normal: *n,
            uv: *t,
            tangent: *tan,
            joints: joints.get(i).copied().unwrap_or([0.0; 4]),
            weights: weights.get(i).copied().unwrap_or([0.0; 4]),
            color: colors.get(i).copied().unwrap_or([1.0, 1.0, 1.0, 1.0]),
            uv1: uvs1.get(i).copied().unwrap_or(*t),
        })
        .collect();

    let raw_indices: Vec<u32> = match reader.read_indices() {
        Some(iter) => iter.into_u32().collect(),
        None => (0..vertices.len() as u32).collect(),
    };
    // Triangulating on load keeps culling, LOD, QEM and drawing on one triangle-list representation.
    let indices = triangulate(&raw_indices, primitive.mode());
    // Sanitize before flat normals, tangents and MikkTSpace index `vertices` with these values.
    let (indices, dropped_triangles) = drop_out_of_range_triangles(indices, vertices.len());
    if dropped_triangles > 0 {
        log::warn!(
            "glTF primitive {}: dropped {} triangle(s) whose indices do not address any of its {} vertices",
            primitive.index(),
            dropped_triangles,
            vertices.len()
        );
    }

    // Missing normals: derive flat face normals so lighting stays sane.
    if reader.read_normals().is_none() {
        compute_flat_normals(&mut vertices, &indices);
    }
    if !had_tangents {
        compute_tangents(&mut vertices, &indices);
    }
    // Opt-in MikkTSpace replaces only tangents we generated; a file's own tangents are kept.
    let (vertices, indices) = if !had_tangents && mikktspace_tangents_enabled() {
        match generate_tangents_mikktspace(&vertices, &indices) {
            Some(rebuilt) => rebuilt,
            None => (vertices, indices),
        }
    } else {
        (vertices, indices)
    };

    // Weights start at zero: the caller applies mesh/node defaults, and animation drives them.
    let morph_targets: Vec<crate::scene::MorphTarget> = reader
        .read_morph_targets()
        .map(|(pos, norm, tan)| crate::scene::MorphTarget {
            position_deltas: pos
                .map(|it| it.map(Vec3::from_array).collect())
                .unwrap_or_default(),
            normal_deltas: norm
                .map(|it| it.map(Vec3::from_array).collect())
                .unwrap_or_default(),
            // glTF morph TANGENT deltas are vec3: the base tangent's w handedness is never morphed.
            tangent_deltas: tan
                .map(|it| it.map(Vec3::from_array).collect())
                .unwrap_or_default(),
        })
        .collect();
    let morph_weights = vec![0.0f32; morph_targets.len()];

    let material = primitive.material();
    let pbr = material.pbr_metallic_roughness();

    let alpha_mode = match material.alpha_mode() {
        gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
        gltf::material::AlphaMode::Mask => AlphaMode::Mask(material.alpha_cutoff().unwrap_or(0.5)),
        gltf::material::AlphaMode::Blend => AlphaMode::Blend,
    };

    // KHR_texture_transform is scoped to `textureInfo`, so each slot carries its own transform.
    let base_uv_transform = uv_transform_from_info(
        pbr.base_color_texture()
            .and_then(|info| info.texture_transform()),
    );
    let mr_uv_transform = uv_transform_from_info(
        pbr.metallic_roughness_texture()
            .and_then(|info| info.texture_transform()),
    );
    let emissive_uv_transform = uv_transform_from_info(
        material
            .emissive_texture()
            .and_then(|info| info.texture_transform()),
    );
    // `.extensions()` borrows from the texture-info value itself, so it must outlive the chain.
    let normal_texture_info = material.normal_texture();
    let normal_uv_transform = uv_transform_from_extension_json(
        normal_texture_info
            .as_ref()
            .and_then(|info| info.extensions())
            .and_then(|ext| ext.get("KHR_texture_transform")),
    );
    let occlusion_texture_info = material.occlusion_texture();
    let occlusion_uv_transform = uv_transform_from_extension_json(
        occlusion_texture_info
            .as_ref()
            .and_then(|info| info.extensions())
            .and_then(|ext| ext.get("KHR_texture_transform")),
    );

    let cpu_material = CpuMaterial {
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
        // Which slots sample UV1 (bit per slot: 0 base .. 4 occlusion).
        uv_set_mask: pbr
            .base_color_texture()
            .map_or(0, |i| uv_set_bit("base color", i.tex_coord(), 0))
            | pbr
                .metallic_roughness_texture()
                .map_or(0, |i| uv_set_bit("metallic-roughness", i.tex_coord(), 1))
            | material
                .normal_texture()
                .map_or(0, |i| uv_set_bit("normal", i.tex_coord(), 2))
            | material
                .emissive_texture()
                .map_or(0, |i| uv_set_bit("emissive", i.tex_coord(), 3))
            | material
                .occlusion_texture()
                .map_or(0, |i| uv_set_bit("occlusion", i.tex_coord(), 4)),
    };

    Ok(Some(CpuPrimitive {
        vertices,
        indices,
        transform,
        node_index: None,
        skin_index: None,
        material: cpu_material,
        morph_targets,
        morph_weights,
    }))
}

fn compute_flat_normals(vertices: &mut [Vertex], indices: &[u32]) {
    for tri in indices.as_chunks::<3>().0 {
        let [i0, i1, i2] = tri.map(|i| i as usize);
        let p0 = Vec3::from_array(vertices[i0].position);
        let p1 = Vec3::from_array(vertices[i1].position);
        let p2 = Vec3::from_array(vertices[i2].position);
        let n = (p1 - p0).cross(p2 - p0).normalize_or_zero().to_array();
        for i in [i0, i1, i2] {
            vertices[i].normal = n;
        }
    }
}

/// Per-vertex tangent frame from triangle UV gradients (Lengyel), with glTF handedness in `.w`.
/// Never splits vertices at UV seams; [`generate_tangents_mikktspace`] does.
pub(crate) fn compute_tangents(vertices: &mut [Vertex], indices: &[u32]) {
    let mut tan_accum = vec![Vec3::ZERO; vertices.len()];
    let mut bitan_accum = vec![Vec3::ZERO; vertices.len()];

    for tri in indices.as_chunks::<3>().0 {
        let [i0, i1, i2] = tri.map(|i| i as usize);
        let p0 = Vec3::from_array(vertices[i0].position);
        let p1 = Vec3::from_array(vertices[i1].position);
        let p2 = Vec3::from_array(vertices[i2].position);
        let u0 = Vec2::from_array(vertices[i0].uv);
        let u1 = Vec2::from_array(vertices[i1].uv);
        let u2 = Vec2::from_array(vertices[i2].uv);

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
        let mut t = (tangent - n * n.dot(tangent)).normalize_or_zero();
        if t == Vec3::ZERO {
            t = n.cross(Vec3::Y).normalize_or_zero();
            if t == Vec3::ZERO {
                t = n.cross(Vec3::X).normalize_or_zero();
            }
        }
        // Degenerate accumulation gets +1, since 0 would zero the shader's bitangent.
        let w = if n.cross(t).dot(bitangent) < 0.0 {
            -1.0
        } else {
            1.0
        };
        vertex.tangent = [t.x, t.y, t.z, w];
    }
}

/// Opt-in MikkTSpace via `KATAGLYPHIS_MIKKTSPACE_TANGENTS`; always off on wasm32, which has no environment.
fn mikktspace_tangents_enabled() -> bool {
    std::env::var_os("KATAGLYPHIS_MIKKTSPACE_TANGENTS").is_some()
}

/// MikkTSpace tangents, the basis DCC tools bake normal maps against (opt-in; [`compute_tangents`] is the default).
/// Splits vertices at seams, so it returns fresh buffers; `None` on failure keeps the caller's.
pub(crate) fn generate_tangents_mikktspace(
    vertices: &[Vertex],
    indices: &[u32],
) -> Option<(Vec<Vertex>, Vec<u32>)> {
    if indices.len() < 3 || !indices.len().is_multiple_of(3) {
        return None;
    }

    struct MikkMesh<'a> {
        vertices: &'a [Vertex],
        indices: &'a [u32],
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
            self.vertices[self.corner(face, vert)].uv
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

    let num_corners = indices.len();
    let mut mesh = MikkMesh {
        vertices,
        indices,
        tangents: vec![[0.0, 0.0, 0.0, 1.0]; num_corners],
    };
    if bevy_mikktspace::generate_tangents(&mut mesh).is_err() {
        return None;
    }

    // MikkTSpace emits bit-identical tangents for shared corners, so keying on the bits splits only seams.
    let mut remap: std::collections::HashMap<(u32, [u32; 4]), u32> =
        std::collections::HashMap::new();
    let mut out_vertices: Vec<Vertex> = Vec::with_capacity(vertices.len());
    let mut out_indices: Vec<u32> = Vec::with_capacity(indices.len());
    for (corner, &orig) in indices.iter().enumerate() {
        let tangent = mesh.tangents[corner];
        let key = (
            orig,
            [
                tangent[0].to_bits(),
                tangent[1].to_bits(),
                tangent[2].to_bits(),
                tangent[3].to_bits(),
            ],
        );
        let new_index = *remap.entry(key).or_insert_with(|| {
            let mut v = vertices[orig as usize];
            v.tangent = tangent;
            out_vertices.push(v);
            (out_vertices.len() - 1) as u32
        });
        out_indices.push(new_index);
    }
    Some((out_vertices, out_indices))
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
        compute_tangents(&mut vertices, &filtered);

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

        compute_tangents(&mut vertices, &indices);

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
        compute_tangents(&mut normal, &idx);

        let (mut mirrored, idx) = quad_with_uvs([[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
        compute_tangents(&mut mirrored, &idx);

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
        let (out_v, out_i) =
            generate_tangents_mikktspace(&vertices, &indices).expect("mikktspace should succeed");
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
        let (out_v, _) =
            generate_tangents_mikktspace(&vertices, &indices).expect("mikktspace should succeed");
        for v in &out_v {
            assert_eq!(
                v.tangent[3], -1.0,
                "mirrored chart must be left-handed, got {:?}",
                v.tangent
            );
        }
    }

    #[test]
    fn mikktspace_rejects_degenerate_input() {
        // None, so the caller keeps its buffers instead of getting empty geometry.
        let (vertices, _) = quad_with_uvs([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        assert!(generate_tangents_mikktspace(&vertices, &[0, 1]).is_none());
        assert!(generate_tangents_mikktspace(&vertices, &[]).is_none());
        assert!(generate_tangents_mikktspace(&vertices, &[0, 1, 2, 3]).is_none());
    }
}
