//! A glTF (or, through `obj::load_obj_flat`, an OBJ) flattened into the arrays BeschleunigerBallett's C++ engine draws: world-space vertices in one list,
//! a material id per triangle, a range per primitive, and each texture's mip chain, decoded on every core: RGBA8, or a
//! KTX2's blocks in a format the engine samples.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Context as _;
use glam::{Mat3, Mat4, Vec3};

use crate::gltf_images::{self, LocatedImage, StoredImage, TextureSource};
use crate::gltf_loader::{self, Geometry, Imported, LoadOptions};
use crate::image_decode;
use crate::levels::{self, BlockSupport, ChainFormat, TextureChain};
use crate::parallel::{self, Budget};
use crate::scene::{
    self, AlphaMode, CpuCameraProjection, CpuLight, CpuLightKind, CpuMaterial, CpuSampler, CpuScene,
};
use crate::scene_meta::SceneMeta;
use crate::skin;

/// One vertex in the C++ `Vertex`'s field order, in world space.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct FlatVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
    pub uv0: [f32; 2],
    /// xyz: tangent, w: bitangent handedness, flipped with a mirroring node.
    pub tangent: [f32; 4],
    pub uv1: [f32; 2],
}

/// The top two rows of a KHR_texture_transform T*R*S matrix.
pub type UvRows = [[f32; 3]; 2];

/// A glTF material as the C++ `ObjMaterial` reads it; a texture id indexes `FlatModel::textures`, -1 for none.
#[derive(Clone, Debug, PartialEq)]
pub struct FlatMaterial {
    pub diffuse: [f32; 3],
    /// baseColorFactor's alpha.
    pub dissolve: f32,
    /// emissiveFactor times KHR_materials_emissive_strength.
    pub emission: [f32; 3],
    pub metallic: f32,
    pub roughness: f32,
    /// MASK's cutoff, -1 for OPAQUE and BLEND.
    pub alpha_cutoff: f32,
    pub normal_scale: f32,
    /// occlusionTexture's strength: how far its texel darkens the ambient term, `lerp(1, texel.r, strength)`.
    pub occlusion_strength: f32,
    pub base_uv: UvRows,
    pub metallic_roughness_uv: UvRows,
    pub normal_uv: UvRows,
    pub emissive_uv: UvRows,
    pub occlusion_uv: UvRows,
    pub base_texture: i32,
    pub metallic_roughness_texture: i32,
    pub normal_texture: i32,
    pub emissive_texture: i32,
    /// Linear; its red channel occludes the ambient term only, never the sun or a punctual light.
    pub occlusion_texture: i32,
    /// An OBJ `map_d` cut-out mask, sampled `.r` at the base colour's UV; glTF has no such slot.
    pub alpha_texture: i32,
    /// Slots sampling TEXCOORD_1, one bit each: 0 base colour, 1 metallic-roughness, 2 normal, 3 emissive, 4 occlusion.
    pub uv1_slot_mask: u32,
    pub unlit: bool,
    pub alpha_mode: AlphaMode,
    pub double_sided: bool,
}

/// One texture slot with its sampler and colour space: the mip chain, or, for a format only the engine's stb reads (PSD,
/// PIC), an empty chain and the file's bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct FlatTexture {
    pub chain: TextureChain,
    pub encoded: Vec<u8>,
    pub sampler: CpuSampler,
    pub srgb: bool,
}

impl FlatTexture {
    /// The chain of `stored`, or for PSD and PIC its bytes for the engine's stb; `None`, with a warning, when it does
    /// not load or is over the pixel cap, which stb would not have refused.
    pub(crate) fn from_stored(
        stored: &StoredImage,
        (sampler, srgb): (CpuSampler, bool),
        what: &dyn std::fmt::Display,
        budget: &Budget,
        blocks: &BlockSupport,
    ) -> Option<FlatTexture> {
        let texture = |chain: TextureChain, encoded: Vec<u8>| FlatTexture {
            chain,
            encoded,
            sampler,
            srgb,
        };
        match decode_chain(stored, srgb, budget, blocks) {
            Ok(Some(chain)) => Some(texture(chain, Vec::new())),
            Ok(None) => Some(texture(TextureChain::default(), stored.bytes.clone())),
            Err(err) => {
                log::warn!("{what}: {err:#}; the material draws without it");
                None
            }
        }
    }
}

/// `stored`'s chain, `None` for a PSD or PIC within the cap; the decode holds its bytes from `budget` meanwhile.
fn decode_chain(
    stored: &StoredImage,
    srgb: bool,
    budget: &Budget,
    blocks: &BlockSupport,
) -> anyhow::Result<Option<TextureChain>> {
    let bytes = &stored.bytes;
    anyhow::ensure!(!bytes.is_empty(), "the image is empty");
    if bytes.starts_with(KTX2_MAGIC) {
        let reader = ktx2::Reader::new(bytes.as_slice())
            .map_err(|err| anyhow::anyhow!("not a valid KTX2 container: {err:?}"))?;
        let header = reader.header();
        let (width, height) = (header.pixel_width.max(1), header.pixel_height.max(1));
        image_decode::check_dimensions(width, height)?;
        let _lease = budget.lease(ktx2_in_flight_bytes(&reader, blocks));
        return levels::ktx2_chain(bytes, srgb, blocks).map(Some);
    }
    if let Some((width, height)) = image_decode::stb_only_dimensions(bytes) {
        image_decode::check_dimensions(width, height)?;
        return Ok(None);
    }
    let format =
        image_decode::image_format(bytes, stored.mime_type.as_deref(), stored.uri.as_deref())
            .context("not an image format either renderer reads")?;
    let (width, height) = image::ImageReader::with_format(std::io::Cursor::new(bytes), format)
        .into_dimensions()
        .with_context(|| format!("reading the {format:?} header"))?;
    image_decode::check_dimensions(width, height)?;
    let _lease = budget.lease(in_flight_bytes(width, height));
    let texture = stored.decode_for_chain()?;
    levels::rgba8_chain(texture, srgb).map(Some)
}

/// What one decode holds at its peak: the decoded image beside its full chain.
fn in_flight_bytes(width: u32, height: u32) -> u64 {
    let chain = levels::chain_bytes(width, height, levels::full_level_count(width, height));
    chain as u64 + u64::from(width) * u64::from(height) * 4
}

/// A KTX2's peak: its blocks where `blocks` will take them as they are, else the RGBA8 decode's.
fn ktx2_in_flight_bytes(reader: &ktx2::Reader<&[u8]>, blocks: &BlockSupport) -> u64 {
    let header = reader.header();
    let (width, height) = (header.pixel_width.max(1), header.pixel_height.max(1));
    let format = if crate::basis::is_basis(reader) {
        blocks.basis_target()
    } else {
        crate::ktx2_loader::stored_format(reader)
    };
    match format {
        Some(format) if blocks.uploads_blocks(format, width, height, header.level_count.max(1)) => {
            ChainFormat::Blocks(format).chain_bytes(width, height, header.level_count.max(1)) as u64
        }
        _ => in_flight_bytes(width, height),
    }
}

/// Decodes in flight at once, in bytes: one load cannot hold eight 16384x16384 chains together.
pub(crate) const DECODE_BUDGET: u64 = 1 << 30;

/// The identifier every KTX2 file starts with.
const KTX2_MAGIC: &[u8] = b"\xabKTX 20\xbb\r\n\x1a\n";

/// One primitive's slice of the flat arrays.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct FlatRange {
    pub vertex_base: usize,
    pub vertex_count: usize,
    pub index_start: usize,
    pub index_count: usize,
    pub tri_start: usize,
    pub tri_count: usize,
    pub double_sided: bool,
    pub alpha_blended: bool,
}

/// A KHR_lights_punctual light in world space; `kind` is 1 point, 2 spot, 3 directional, as the shaders count.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FlatLight {
    pub kind: u32,
    pub color: [f32; 3],
    pub intensity: f32,
    /// 0 for unbounded.
    pub range: f32,
    pub position: [f32; 3],
    pub direction: [f32; 3],
    /// 1 for every kind but spot.
    pub cos_inner: f32,
    pub cos_outer: f32,
}

/// The whole file: the last material is the neutral one a primitive without a material draws with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlatModel {
    pub vertices: Vec<FlatVertex>,
    pub indices: Vec<u32>,
    /// One per triangle.
    pub material_index: Vec<u32>,
    pub materials: Vec<FlatMaterial>,
    pub textures: Vec<FlatTexture>,
    pub ranges: Vec<FlatRange>,
    /// Every punctual light but the Sun.
    pub lights: Vec<FlatLight>,
    /// The first directional light, which the engine adopts as its Sun: [`crate::CpuScene::take_sun`]'s rule.
    pub sun: Option<FlatLight>,
    /// Every camera the file authors, in node order, posed in world space.
    pub cameras: Vec<FlatCamera>,
    /// The camera `scenes[scene].extras.active_camera` names, as an index into `cameras`.
    pub active_camera: Option<usize>,
    /// `asset.copyright`, else the scene folder's attribution: [`crate::scene_meta::credit`].
    pub credit: Option<String>,
}

/// A camera at its node's world pose.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlatCamera {
    /// The camera's name, else its node's; empty when neither has one.
    pub name: String,
    pub eye: [f32; 3],
    /// The node's -Z, a unit vector, or zero for a degenerate node.
    pub forward: [f32; 3],
    /// `None` for an orthographic camera.
    pub yfov_rad: Option<f32>,
}

/// Loads `path` through the shared loader with `options` and flattens it; an error when nothing draws.
pub fn load_flat(path: impl AsRef<Path>, options: &LoadOptions) -> anyhow::Result<FlatModel> {
    let path = path.as_ref();
    let imported = gltf_loader::import_document(path)
        .with_context(|| format!("Failed to import glTF file: {}", path.display()))?;
    let (materials, pending) = read_materials(&imported);
    let mut model = FlatModel {
        materials,
        ..FlatModel::default()
    };
    let source = Source {
        imported: &imported,
        options,
        joints: rest_joint_matrices(&imported),
    };
    let mut lights = Vec::new();
    gltf_loader::walk_nodes(&imported.document, |node, world| {
        visit_node(&mut model, &mut lights, &source, node, world)
    })?;
    anyhow::ensure!(
        !model.vertices.is_empty(),
        "glTF file contains no triangle primitives"
    );
    model.sun = scene::take_sun(&mut lights).map(|sun| flat_light(&sun));
    model.lights = lights.iter().map(flat_light).collect();
    let meta = read_meta(path);
    (model.cameras, model.active_camera) = flat_cameras(&imported.document, &meta);
    model.credit = scene_meta_credit(path, meta.copyright);
    // One decode per image and colour space; a second sampler of it shares the decoded chain.
    let mut decode_of: HashMap<(TextureSource, bool), usize> = HashMap::new();
    let mut located: Vec<LocatedSlot> = Vec::new();
    let jobs: Vec<usize> = pending
        .iter()
        .map(|slot| {
            *decode_of
                .entry((slot.source, slot.srgb))
                .or_insert_with(|| {
                    located.push(slot.locate(&imported));
                    located.len() - 1
                })
        })
        .collect();
    drop(source);
    // Before the chains arrive: Colosseum-gltf's 197 MiB .bin would otherwise peak beside them.
    drop(imported);
    let budget = Budget::new(DECODE_BUDGET);
    let decoded = parallel::map(&located, |slot| slot.decode(&budget, &options.blocks));
    let textures = share_decodes(decoded, &jobs, &pending);
    model.textures = renumber_slots(&mut model.materials, textures);
    Ok(model)
}

/// The file's credit and active camera; a browser build reads no file beside the model.
fn read_meta(path: &Path) -> SceneMeta {
    #[cfg(not(target_arch = "wasm32"))]
    return SceneMeta::read(path);
    #[cfg(target_arch = "wasm32")]
    {
        let _ = path;
        SceneMeta::default()
    }
}

/// [`crate::scene_meta::credit`] where a folder can be read.
pub(crate) fn scene_meta_credit(path: &Path, copyright: Option<String>) -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    return crate::scene_meta::credit(path, copyright);
    #[cfg(target_arch = "wasm32")]
    {
        let _ = path;
        copyright
    }
}

/// Every authored camera at its world pose, and which of them `meta` names active.
fn flat_cameras(document: &gltf::Document, meta: &SceneMeta) -> (Vec<FlatCamera>, Option<usize>) {
    let world = CpuScene::compute_world_transforms(&gltf_loader::read_nodes(document));
    let authored = gltf_loader::read_cameras(document);
    let cameras: Vec<FlatCamera> = authored
        .iter()
        .map(|camera| {
            let pose = world.get(camera.node).copied().unwrap_or(Mat4::NAN);
            let node_name = document
                .nodes()
                .nth(camera.node)
                .and_then(|node| node.name());
            FlatCamera {
                name: camera
                    .name
                    .as_deref()
                    .or(node_name)
                    .unwrap_or_default()
                    .to_owned(),
                eye: pose.transform_point3(Vec3::ZERO).to_array(),
                forward: pose
                    .transform_vector3(Vec3::NEG_Z)
                    .normalize_or_zero()
                    .to_array(),
                yfov_rad: match camera.projection {
                    CpuCameraProjection::Perspective { yfov_rad, .. } => Some(yfov_rad),
                    CpuCameraProjection::Orthographic { .. } => None,
                },
            }
        })
        .collect();
    let active = meta
        .active_camera_node
        .and_then(|node| authored.iter().position(|camera| camera.node == node));
    (cameras, active)
}

/// What every node reads: the file, the load options and each skin's joint matrices at rest.
struct Source<'a> {
    imported: &'a Imported,
    options: &'a LoadOptions,
    joints: Vec<Vec<Mat4>>,
}

/// Each skin's joint matrices at the file's pose: the engine draws no animation, so its skins keep that pose.
fn rest_joint_matrices(imported: &Imported) -> Vec<Vec<Mat4>> {
    let world = CpuScene::compute_world_transforms(&gltf_loader::read_nodes(&imported.document));
    gltf_loader::read_skins(&imported.document, &imported.buffers)
        .iter()
        .map(|skin| skin::joint_matrices(skin, &world))
        .collect()
}

/// Every material in document order, then the neutral fallback, and the texture slots they name, still to decode.
fn read_materials(imported: &Imported) -> (Vec<FlatMaterial>, Vec<PendingSlot>) {
    let mut slots = Slots {
        imported,
        keys: HashMap::new(),
        pending: Vec::new(),
    };
    let mut materials: Vec<FlatMaterial> = imported
        .document
        .materials()
        .map(|material| {
            let ids = slots.material_slots(&material);
            flat_material(&gltf_loader::load_material(&material, &[]), ids)
        })
        .collect();
    materials.push(flat_material(&CpuMaterial::default(), [-1; 5]));
    (materials, slots.pending)
}

/// Each slot's texture with its own sampler, cloned from its decode while another slot still needs it, moved after.
fn share_decodes(
    mut decoded: Vec<Option<FlatTexture>>,
    jobs: &[usize],
    pending: &[PendingSlot],
) -> Vec<Option<FlatTexture>> {
    let mut uses = vec![0usize; decoded.len()];
    for &job in jobs {
        uses[job] += 1;
    }
    jobs.iter()
        .zip(pending)
        .map(|(&job, slot)| {
            uses[job] -= 1;
            let texture = if uses[job] == 0 {
                decoded[job].take()
            } else {
                decoded[job].clone()
            };
            texture.map(|texture| FlatTexture {
                sampler: slot.sampler,
                ..texture
            })
        })
        .collect()
}

/// Drops the slots that did not load and renumbers the materials' ids to the ones that did, -1 for a dropped one.
pub(crate) fn renumber_slots(
    materials: &mut [FlatMaterial],
    decoded: Vec<Option<FlatTexture>>,
) -> Vec<FlatTexture> {
    let mut textures = Vec::with_capacity(decoded.len());
    let ids: Vec<i32> = decoded
        .into_iter()
        .map(|texture| match texture {
            Some(texture) => {
                textures.push(texture);
                textures.len() as i32 - 1
            }
            None => -1,
        })
        .collect();
    for material in materials.iter_mut() {
        for id in [
            &mut material.base_texture,
            &mut material.metallic_roughness_texture,
            &mut material.normal_texture,
            &mut material.emissive_texture,
            &mut material.occlusion_texture,
            &mut material.alpha_texture,
        ] {
            if *id >= 0 {
                *id = ids.get(*id as usize).copied().unwrap_or(-1);
            }
        }
    }
    textures
}

/// Dedups slots on (sources, sampler settings, colour space): an image sampled two ways gets one slot per way.
struct Slots<'a> {
    imported: &'a Imported,
    /// The settings, not the sampler's index: two equal sampler objects, or none and a default one, share a slot.
    keys: HashMap<(TextureSource, CpuSampler, bool), i32>,
    pending: Vec<PendingSlot>,
}

/// A slot to decode: its `KHR_texture_basisu` image, preferred, and its core one.
struct PendingSlot {
    source: TextureSource,
    sampler: CpuSampler,
    srgb: bool,
}

impl Slots<'_> {
    /// Base colour, metallic-roughness, normal, emissive and occlusion, in the C++ material's order.
    fn material_slots(&mut self, material: &gltf::Material) -> [i32; 5] {
        let pbr = material.pbr_metallic_roughness();
        [
            self.slot(pbr.base_color_texture().map(|i| i.texture()), true),
            self.slot(pbr.metallic_roughness_texture().map(|i| i.texture()), false),
            self.slot(material.normal_texture().map(|i| i.texture()), false),
            self.slot(material.emissive_texture().map(|i| i.texture()), true),
            self.slot(material.occlusion_texture().map(|i| i.texture()), false),
        ]
    }

    fn slot(&mut self, texture: Option<gltf::Texture>, srgb: bool) -> i32 {
        let Some(texture) = texture else {
            return -1;
        };
        let Some(&source) = self.imported.sources.get(texture.index()) else {
            return -1;
        };
        let sampler = gltf_loader::to_cpu_sampler(&texture.sampler());
        let key = (source, sampler, srgb);
        if let Some(&slot) = self.keys.get(&key) {
            return slot;
        }
        let slot = self.pending.len() as i32;
        self.keys.insert(key, slot);
        self.pending.push(PendingSlot {
            source,
            sampler,
            srgb,
        });
        slot
    }
}

impl PendingSlot {
    /// Each source's bytes, or where to read them, so the decode needs no buffer of the document.
    fn locate(&self, imported: &Imported) -> LocatedSlot {
        let locate = |image: usize| {
            let located = imported
                .document
                .images()
                .nth(image)
                .context("no such image")
                .and_then(|gltf_image| {
                    gltf_images::locate_image(&gltf_image, Some(&imported.base), &imported.buffers)
                });
            (image, located)
        };
        LocatedSlot {
            basisu: self.source.basisu.map(locate),
            fallback: self.source.fallback.map(locate),
            sampler: self.sampler,
            srgb: self.srgb,
        }
    }
}

/// A slot's `KHR_texture_basisu` image and its core one, each with its index for the log.
struct LocatedSlot {
    basisu: Option<(usize, anyhow::Result<LocatedImage>)>,
    fallback: Option<(usize, anyhow::Result<LocatedImage>)>,
    sampler: CpuSampler,
    srgb: bool,
}

impl LocatedSlot {
    /// The `KHR_texture_basisu` image, else (with a warning) the core one; `None` when neither loads.
    fn decode(&self, budget: &Budget, blocks: &BlockSupport) -> Option<FlatTexture> {
        if let Some((image, located)) = &self.basisu {
            let chain = located
                .as_ref()
                .map_err(|err| anyhow::anyhow!("{err:#}"))
                .and_then(LocatedImage::load)
                .and_then(|stored| decode_chain(&stored, self.srgb, budget, blocks))
                .and_then(|chain| chain.context("a KHR_texture_basisu image that is no KTX2"));
            match chain {
                Ok(chain) => {
                    return Some(FlatTexture {
                        chain,
                        encoded: Vec::new(),
                        sampler: self.sampler,
                        srgb: self.srgb,
                    })
                }
                Err(err) => log::warn!("glTF image {image} (KTX2): {err:#}"),
            }
        }
        let (image, located) = self.fallback.as_ref()?;
        let what = format!("glTF image {image}");
        match located
            .as_ref()
            .map_err(|err| anyhow::anyhow!("{err:#}"))
            .and_then(LocatedImage::load)
        {
            Ok(stored) => {
                FlatTexture::from_stored(&stored, (self.sampler, self.srgb), &what, budget, blocks)
            }
            Err(err) => {
                log::warn!("{what}: {err:#}");
                None
            }
        }
    }
}

/// The C++ `ObjMaterial` fields of a loaded material; factors clamp to [0, 1] as the C++ loader's do.
pub(crate) fn flat_material(material: &CpuMaterial, ids: [i32; 5]) -> FlatMaterial {
    let [r, g, b, a] = material.base_color;
    FlatMaterial {
        diffuse: [r, g, b],
        dissolve: a,
        emission: material.emissive_factor,
        metallic: material.metallic_factor.clamp(0.0, 1.0),
        roughness: material.roughness_factor.clamp(0.0, 1.0),
        alpha_cutoff: match material.alpha_mode {
            AlphaMode::Mask(cutoff) => cutoff,
            AlphaMode::Opaque | AlphaMode::Blend => -1.0,
        },
        normal_scale: material.normal_scale,
        occlusion_strength: material.occlusion_strength,
        base_uv: material.base_uv_transform,
        metallic_roughness_uv: material.mr_uv_transform,
        normal_uv: material.normal_uv_transform,
        emissive_uv: material.emissive_uv_transform,
        occlusion_uv: material.occlusion_uv_transform,
        base_texture: ids[0],
        metallic_roughness_texture: ids[1],
        normal_texture: ids[2],
        emissive_texture: ids[3],
        occlusion_texture: ids[4],
        alpha_texture: -1,
        uv1_slot_mask: material.uv_set_mask,
        unlit: material.unlit,
        alpha_mode: material.alpha_mode,
        double_sided: material.double_sided,
    }
}

/// One node: its light into `lights`, then each drawable primitive in its default morph pose, baked by its world
/// transform, or by its skin at rest.
fn visit_node(
    model: &mut FlatModel,
    lights: &mut Vec<CpuLight>,
    source: &Source,
    node: &gltf::Node,
    world: Mat4,
) -> anyhow::Result<()> {
    lights.extend(gltf_loader::read_light(node, world));
    let Some(mesh) = node.mesh() else {
        return Ok(());
    };
    // glTF ignores a skinned mesh's node transform: its joints place it.
    let joints = node.skin().map(|skin| {
        source
            .joints
            .get(skin.index())
            .map_or(&[][..], Vec::as_slice)
    });
    let world = if joints.is_some() {
        Mat4::IDENTITY
    } else {
        world
    };
    let fallback = (model.materials.len() - 1) as u32;
    for primitive in mesh.primitives().filter(gltf_loader::draws_triangles) {
        let Some(mut geometry) =
            gltf_loader::primitive_geometry(&primitive, &source.imported.buffers, source.options)?
        else {
            continue;
        };
        // The C++ engine draws no morphs, so it gets the default pose the Rust renderer starts from.
        if let Some(weights) =
            gltf_loader::default_morph_weights(node, geometry.morph_targets.len())
        {
            geometry.vertices =
                scene::blend_morph_targets(&geometry.vertices, &geometry.morph_targets, weights);
        }
        // Nor skins: it gets the pose the Rust renderer uploads, the joints at the file's transforms.
        if let Some(joints) = joints {
            let vertices = &mut geometry.vertices;
            gltf_loader::bind_influences(vertices, Some(joints.len()), primitive.index());
            skin::rewind_mirrored_triangles(
                vertices,
                &mut geometry.indices,
                joints,
                Mat4::IDENTITY,
            );
            for vertex in vertices.iter_mut() {
                *vertex = skin::skin_vertex(vertex, joints, Mat4::IDENTITY);
            }
        }
        let material = primitive.material().index().map_or(fallback, |i| i as u32);
        append_primitive(model, &geometry, world, material);
    }
    Ok(())
}

/// Appends one primitive in world space; a mirroring transform reverses the winding so the front face stays front.
pub(crate) fn append_primitive(
    model: &mut FlatModel,
    geometry: &Geometry,
    world: Mat4,
    material: u32,
) {
    let linear = Mat3::from_mat4(world);
    let normal_matrix = linear.inverse().transpose();
    let mirrored = linear.determinant() < 0.0;
    let handedness = if mirrored { -1.0 } else { 1.0 };
    let vertex_base = model.vertices.len();
    model.vertices.extend(geometry.vertices.iter().map(|v| {
        let tangent = (linear * Vec3::from_slice(&v.tangent[..3])).normalize_or_zero();
        FlatVertex {
            position: world
                .transform_point3(Vec3::from_array(v.position))
                .to_array(),
            normal: (normal_matrix * Vec3::from_array(v.normal))
                .normalize_or_zero()
                .to_array(),
            color: v.color,
            uv0: v.uv,
            tangent: [tangent.x, tangent.y, tangent.z, v.tangent[3] * handedness],
            uv1: v.uv1,
        }
    }));
    let base = vertex_base as u32;
    let index_start = model.indices.len();
    for &[a, b, c] in geometry.indices.as_chunks::<3>().0 {
        let triangle = if mirrored { [a, c, b] } else { [a, b, c] };
        model.indices.extend(triangle.map(|i| base + i));
    }
    let tri_start = model.material_index.len();
    let tri_count = (model.indices.len() - index_start) / 3;
    model
        .material_index
        .extend(std::iter::repeat_n(material, tri_count));
    let flat = &model.materials[material as usize];
    model.ranges.push(FlatRange {
        vertex_base,
        vertex_count: geometry.vertices.len(),
        index_start,
        index_count: tri_count * 3,
        tri_start,
        tri_count,
        double_sided: flat.double_sided,
        alpha_blended: flat.alpha_mode == AlphaMode::Blend,
    });
}

fn flat_light(light: &CpuLight) -> FlatLight {
    let (kind, cos_inner, cos_outer) = match light.kind {
        CpuLightKind::Point => (1, 1.0, 1.0),
        CpuLightKind::Spot {
            cos_inner,
            cos_outer,
        } => (2, cos_inner, cos_outer),
        CpuLightKind::Directional => (3, 1.0, 1.0),
    };
    // A node that scales its light to nothing has no -Z; the C++ light then points down -Z.
    let direction = Vec3::from_array(light.direction)
        .try_normalize()
        .unwrap_or(Vec3::NEG_Z);
    FlatLight {
        kind,
        color: light.color,
        intensity: light.intensity,
        range: light.range,
        position: light.position,
        direction: direction.to_array(),
        cos_inner,
        cos_outer,
    }
}
