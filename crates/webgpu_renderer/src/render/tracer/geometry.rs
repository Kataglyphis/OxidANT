//! The scene's triangles as tracer meshes: a repeated or moving mesh is instanced in object space, the rest merges in world space.

use std::borrow::Cow;
use std::collections::HashMap;

use glam::{Mat3, Mat4, Vec2, Vec3, Vec4};

use super::deform::{push_deformed, DeformedSource};
use super::material::MaterialTable;
use crate::render::bounds::normal_matrix_of;
use crate::scene::{
    blend_morph_targets, ChannelValues, CpuPrimitive, CpuScene, CpuSkin, MorphTarget, Vertex,
};

/// Triangle records keep their material in the low bits and whether it alpha-tests in the top bit.
pub const TRIANGLE_MASKED: u32 = 1 << 31;
pub const TRIANGLE_MATERIAL: u32 = 0x00FF_FFFF;

/// One bottom-level mesh before its BVH: positions, packed attributes and triangles over local vertex indices.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshBuild {
    pub positions: Vec<[f32; 4]>,
    pub vertices: Vec<[u32; 8]>,
    pub triangles: Vec<[u32; 4]>,
}

/// Where an instance comes from: its mesh, its scene primitive (`u32::MAX` for a world-space mesh) and its transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InstanceSource {
    pub mesh: u32,
    pub primitive: u32,
    /// The explicit instance's transform, applied after the primitive's; identity without one.
    pub extra: Mat4,
    /// The primitive's own transform, which `extra` follows.
    pub base: Mat4,
    pub local: Mat4,
}

/// Every mesh and instance of a scene, and the meshes an animation deforms.
#[derive(Clone, Debug, Default)]
pub struct Gathered {
    pub meshes: Vec<MeshBuild>,
    pub instances: Vec<InstanceSource>,
    pub deformed: Vec<DeformedSource>,
}

/// Splits `scene` into meshes; `explicit[i]`, when not empty, instances primitive `i` like `ForwardRenderer::set_instances`.
pub fn gather(scene: &CpuScene, table: &MaterialTable, explicit: &[Vec<Mat4>]) -> Gathered {
    let animated = animated_nodes(scene);
    let moving = |i: usize| {
        let prim = &scene.primitives[i];
        explicit.get(i).is_some_and(|e| !e.is_empty())
            || prim
                .node_index
                .is_some_and(|n| animated.get(n).copied().unwrap_or(false))
    };
    let (classes, deformed) = classify(scene, table);
    let (posed, mut merged): (Vec<usize>, Vec<usize>) = deformed
        .into_iter()
        .partition(|&i| deforms_over_time(scene, &animated, i));
    let world = CpuScene::compute_world_transforms(&scene.nodes);
    let mut gathered = Gathered::default();
    for primitive in posed {
        push_deformed(&mut gathered, scene, table, primitive, &world);
    }
    for class in classes {
        if class.len() < 2 && !moving(class[0]) {
            merged.push(class[0]);
            continue;
        }
        let rep = &scene.primitives[class[0]];
        let mut build = MeshBuild::default();
        let material = table.primitive_material[class[0]];
        append_primitive(
            &mut build,
            (&rep.vertices, &rep.indices),
            None,
            material,
            table,
        );
        if build.triangles.is_empty() {
            continue;
        }
        let mesh = gathered.meshes.len() as u32;
        gathered.meshes.push(build);
        for &member in &class {
            push_instances(&mut gathered, scene, explicit, mesh, member);
        }
    }
    merged.sort_unstable();
    let build = merge(scene, table, &merged, &world);
    if !build.triangles.is_empty() {
        // First, so assembling the arrays can take over the largest mesh's vectors without a copy.
        gathered.meshes.insert(0, build);
        for instance in &mut gathered.instances {
            instance.mesh += 1;
        }
        for source in &mut gathered.deformed {
            source.mesh += 1;
        }
        let whole = InstanceSource {
            mesh: 0,
            primitive: u32::MAX,
            extra: Mat4::IDENTITY,
            base: Mat4::IDENTITY,
            local: Mat4::IDENTITY,
        };
        gathered.instances.insert(0, whole);
    }
    gathered
}

/// The primitives `merged` names, posed and moved to world space, as one mesh.
fn merge(scene: &CpuScene, table: &MaterialTable, merged: &[usize], world: &[Mat4]) -> MeshBuild {
    let mut build = MeshBuild::default();
    for &i in merged {
        let prim = &scene.primitives[i];
        let (vertices, posed) = posed_vertices(scene, prim, world);
        let transform = if posed {
            Mat4::IDENTITY
        } else {
            prim.transform
        };
        let material = table.primitive_material[i];
        append_primitive(
            &mut build,
            (&vertices, &prim.indices),
            Some(transform),
            material,
            table,
        );
    }
    build
}

fn push_instances(
    gathered: &mut Gathered,
    scene: &CpuScene,
    explicit: &[Vec<Mat4>],
    mesh: u32,
    member: usize,
) {
    let transform = scene.primitives[member].transform;
    let extras = match explicit.get(member).filter(|e| !e.is_empty()) {
        Some(instances) => instances.clone(),
        None => vec![Mat4::IDENTITY],
    };
    for extra in extras {
        let local = extra * transform;
        // A zero scale hides a node; its singular matrix has no inverse to carry rays into it.
        if local.is_finite() && local.determinant().abs() > f32::MIN_POSITIVE {
            gathered.instances.push(InstanceSource {
                mesh,
                primitive: member as u32,
                extra,
                base: transform,
                local,
            });
        }
    }
}

/// Primitives with equal geometry and material in classes, in first-seen order; skinned and morphed ones apart.
fn classify(scene: &CpuScene, table: &MaterialTable) -> (Vec<Vec<usize>>, Vec<usize>) {
    let mut classes: Vec<Vec<usize>> = Vec::new();
    let mut by_key: HashMap<(usize, usize, u32), Vec<usize>> = HashMap::new();
    let mut deformed = Vec::new();
    for (i, prim) in scene.primitives.iter().enumerate() {
        if prim.skin_index.is_some() || !prim.morph_targets.is_empty() {
            deformed.push(i);
            continue;
        }
        let material = table.primitive_material[i];
        let candidates = by_key
            .entry((prim.vertices.len(), prim.indices.len(), material))
            .or_default();
        let found = candidates
            .iter()
            .copied()
            .find(|&c| same_geometry(&scene.primitives[classes[c][0]], prim));
        match found {
            Some(class) => classes[class].push(i),
            None => {
                candidates.push(classes.len());
                classes.push(vec![i]);
            }
        }
    }
    (classes, deformed)
}

fn same_geometry(a: &CpuPrimitive, b: &CpuPrimitive) -> bool {
    a.indices == b.indices
        && bytemuck::cast_slice::<Vertex, u8>(&a.vertices)
            == bytemuck::cast_slice::<Vertex, u8>(&b.vertices)
}

/// Whether an animation moves a skinned primitive's joints or a morphed one's weights, so its mesh is re-posed.
fn deforms_over_time(scene: &CpuScene, animated: &[bool], i: usize) -> bool {
    let prim = &scene.primitives[i];
    let skin = prim.skin_index.and_then(|s| scene.skins.get(s));
    let joints_move = skin.is_some_and(|skin| {
        skin.joints
            .iter()
            .any(|&joint| animated.get(joint).copied().unwrap_or(false))
    });
    let weights_move = !prim.morph_targets.is_empty()
        && scene
            .animations
            .iter()
            .flat_map(|a| &a.channels)
            .any(|channel| {
                Some(channel.node) == prim.node_index
                    && matches!(channel.values, ChannelValues::MorphWeights(_))
            });
    joints_move || weights_move
}

/// Nodes an animation channel moves, directly or through an ancestor.
fn animated_nodes(scene: &CpuScene) -> Vec<bool> {
    let mut animated = vec![false; scene.nodes.len()];
    for channel in scene.animations.iter().flat_map(|a| &a.channels) {
        if let Some(flag) = animated.get_mut(channel.node) {
            *flag = true;
        }
    }
    // Parents can follow their children in the table, so repeat until nothing changes.
    let mut changed = animated.iter().any(|a| *a);
    while changed {
        changed = false;
        for (i, node) in scene.nodes.iter().enumerate() {
            let parent_moves = node
                .parent
                .is_some_and(|p| animated.get(p).copied().unwrap_or(false));
            if parent_moves && !animated[i] {
                animated[i] = true;
                changed = true;
            }
        }
    }
    animated
}

/// The pose the raster pass draws first: default morph weights blended, skins posed; true when already in world space.
fn posed_vertices<'a>(
    scene: &CpuScene,
    prim: &'a CpuPrimitive,
    world: &[Mat4],
) -> (Cow<'a, [Vertex]>, bool) {
    let joints = prim
        .skin_index
        .and_then(|s| scene.skins.get(s))
        .map(|skin| joint_matrices(skin, world));
    let pose = Deformation {
        targets: &prim.morph_targets,
        weights: &prim.morph_weights,
        joints: joints.as_deref(),
        node: prim.transform,
    };
    pose.apply(&prim.vertices)
}

/// What poses a primitive's vertices: morph targets and their weights, and a skin's joint matrices with the node's own.
pub(super) struct Deformation<'a> {
    pub targets: &'a [MorphTarget],
    pub weights: &'a [f32],
    pub joints: Option<&'a [Mat4]>,
    /// The transform an unweighted vertex of a skinned primitive follows.
    pub node: Mat4,
}

impl Deformation<'_> {
    /// The posed vertices; true when skinned, which leaves them in world space.
    pub fn apply<'v>(&self, vertices: &'v [Vertex]) -> (Cow<'v, [Vertex]>, bool) {
        let mut posed = Cow::Borrowed(vertices);
        if self.weights.iter().any(|w| *w != 0.0) {
            posed = Cow::Owned(blend_morph_targets(vertices, self.targets, self.weights));
        }
        let Some(joints) = self.joints else {
            return (posed, false);
        };
        let skinned = posed
            .iter()
            .map(|v| skin_vertex(v, joints, self.node))
            .collect();
        (Cow::Owned(skinned), true)
    }
}

/// Each joint's world matrix times its inverse bind matrix, as the raster's joint buffer holds them.
pub(super) fn joint_matrices(skin: &CpuSkin, world: &[Mat4]) -> Vec<Mat4> {
    skin.joints
        .iter()
        .enumerate()
        .map(|(j, &node)| {
            let bind = skin
                .inverse_bind_matrices
                .get(j)
                .copied()
                .unwrap_or(Mat4::IDENTITY);
            world.get(node).copied().unwrap_or(Mat4::IDENTITY) * bind
        })
        .collect()
}

/// One vertex through its weighted joints, or through the node's transform when unweighted, as `forward.slang` does.
fn skin_vertex(v: &Vertex, joints: &[Mat4], node: Mat4) -> Vertex {
    let total: f32 = v.weights.iter().sum();
    let matrix = if total > 1e-4 {
        let mut m = Mat4::ZERO;
        for k in 0..4 {
            let joint = joints
                .get(v.joints[k] as usize)
                .copied()
                .unwrap_or(Mat4::IDENTITY);
            m += joint * v.weights[k];
        }
        m * (1.0 / total)
    } else {
        node
    };
    let (normal, tangent, handedness) = Pose::new(matrix).frame(v);
    Vertex {
        position: matrix
            .transform_point3(Vec3::from_array(v.position))
            .to_array(),
        normal: normal.to_array(),
        tangent: [tangent.x, tangent.y, tangent.z, handedness],
        ..*v
    }
}

/// A transform with its normal matrix and mirror sign, worked out once for all of a primitive's vertices.
pub(super) struct Pose {
    model: Mat4,
    normal: Mat4,
    mirror: f32,
}

impl Pose {
    fn new(model: Mat4) -> Self {
        let mirror = if Mat3::from_mat4(model).determinant() < 0.0 {
            -1.0
        } else {
            1.0
        };
        Self {
            model,
            normal: normal_matrix_of(model),
            mirror,
        }
    }

    /// The normal, tangent and bitangent sign of `v`; a mirroring pose flips the sign, as the raster does.
    fn frame(&self, v: &Vertex) -> (Vec3, Vec3, f32) {
        let normal = self.normal.transform_vector3(Vec3::from_array(v.normal));
        let tangent =
            self.model
                .transform_vector3(Vec3::new(v.tangent[0], v.tangent[1], v.tangent[2]));
        (
            normal.normalize_or_zero(),
            tangent.normalize_or_zero(),
            v.tangent[3] * self.mirror,
        )
    }
}

/// Appends vertices (moved to world space by `transform` when given) and the non-degenerate triangles over them.
pub(super) fn append_primitive(
    build: &mut MeshBuild,
    (vertices, indices): (&[Vertex], &[u32]),
    transform: Option<Mat4>,
    material: u32,
    table: &MaterialTable,
) {
    let base = build.positions.len() as u32;
    let pose = transform.map(Pose::new);
    append_vertices(build, vertices, pose.as_ref());
    let masked = if table.masked(material) {
        TRIANGLE_MASKED
    } else {
        0
    };
    // A mirrored primitive winds clockwise in world space; swapping two corners keeps its front where glTF puts it.
    let mirrored = pose.is_some_and(|p| p.mirror < 0.0);
    for tri in indices.as_chunks::<3>().0 {
        let [a, b, c] = tri.map(|i| base + i);
        let corner = |i: u32| Vec4::from_array(build.positions[i as usize]).truncate();
        let area2 = (corner(b) - corner(a))
            .cross(corner(c) - corner(a))
            .length_squared();
        if area2 > 0.0 && area2.is_finite() {
            let (b, c) = if mirrored { (c, b) } else { (b, c) };
            build.triangles.push([a, b, c, material | masked]);
        }
    }
}

/// Appends each vertex's position and packed attributes, moved by `pose` when given.
pub(super) fn append_vertices(build: &mut MeshBuild, vertices: &[Vertex], pose: Option<&Pose>) {
    for v in vertices {
        let (position, (normal, tangent, handedness)) = match pose {
            Some(pose) => (
                pose.model.transform_point3(Vec3::from_array(v.position)),
                pose.frame(v),
            ),
            None => (
                Vec3::from_array(v.position),
                (
                    Vec3::from_array(v.normal),
                    Vec3::new(v.tangent[0], v.tangent[1], v.tangent[2]),
                    v.tangent[3],
                ),
            ),
        };
        build.positions.push(position.extend(0.0).to_array());
        build
            .vertices
            .push(pack_vertex(v, normal, tangent, handedness));
    }
}

/// A unit vector on the octahedron, folded into the square [-1, 1]^2.
pub fn oct_encode(n: Vec3) -> Vec2 {
    let l1 = n.x.abs() + n.y.abs() + n.z.abs();
    if l1 <= 0.0 || !l1.is_finite() {
        return Vec2::ZERO;
    }
    let p = Vec2::new(n.x, n.y) / l1;
    if n.z >= 0.0 {
        return p;
    }
    let sign = Vec2::new(
        if p.x >= 0.0 { 1.0 } else { -1.0 },
        if p.y >= 0.0 { 1.0 } else { -1.0 },
    );
    (Vec2::ONE - Vec2::new(p.y.abs(), p.x.abs())) * sign
}

pub fn oct_decode(e: Vec2) -> Vec3 {
    let mut v = Vec3::new(e.x, e.y, 1.0 - e.x.abs() - e.y.abs());
    let t = (-v.z).max(0.0);
    v.x += if v.x >= 0.0 { -t } else { t };
    v.y += if v.y >= 0.0 { -t } else { t };
    v.normalize_or_zero()
}

fn snorm16(v: f32) -> u32 {
    u32::from((v.clamp(-1.0, 1.0) * 32767.0).round() as i16 as u16)
}

fn unorm16(v: f32) -> u32 {
    (v.clamp(0.0, 1.0) * 65535.0).round() as u32
}

fn unpack_snorm16(bits: u32) -> f32 {
    (f32::from(bits as u16 as i16) / 32767.0).max(-1.0)
}

/// The bitangent sign rides in the lowest bit of the tangent's second component.
const HANDEDNESS_BIT: u32 = 1 << 16;

/// One vertex's shading attributes in 32 bytes: oct normal, oct tangent and sign, UV0, UV1, RGBA16 colour.
pub fn pack_vertex(v: &Vertex, normal: Vec3, tangent: Vec3, handedness: f32) -> [u32; 8] {
    let n = oct_encode(normal);
    let t = oct_encode(tangent);
    let sign = if handedness < 0.0 { HANDEDNESS_BIT } else { 0 };
    let c = v.color;
    [
        snorm16(n.x) | snorm16(n.y) << 16,
        snorm16(t.x) | (snorm16(t.y) << 16 & !HANDEDNESS_BIT) | sign,
        v.uv[0].to_bits(),
        v.uv[1].to_bits(),
        v.uv1[0].to_bits(),
        v.uv1[1].to_bits(),
        unorm16(c[0]) | unorm16(c[1]) << 16,
        unorm16(c[2]) | unorm16(c[3]) << 16,
    ]
}

/// The attributes [`pack_vertex`] stored, as `bvh_traverse.slang` decodes them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VertexAttributes {
    pub normal: Vec3,
    /// xyz the tangent, w the bitangent sign.
    pub tangent: Vec4,
    pub uv0: Vec2,
    pub uv1: Vec2,
    pub color: Vec4,
}

pub fn unpack_vertex(p: &[u32; 8]) -> VertexAttributes {
    let pair = |w: u32| Vec2::new(unpack_snorm16(w & 0xFFFF), unpack_snorm16(w >> 16));
    let unorm = |w: u32| Vec2::new((w & 0xFFFF) as f32, (w >> 16) as f32) / 65535.0;
    let sign = if p[1] & HANDEDNESS_BIT != 0 {
        -1.0
    } else {
        1.0
    };
    let (rg, ba) = (unorm(p[6]), unorm(p[7]));
    VertexAttributes {
        normal: oct_decode(pair(p[0])),
        tangent: oct_decode(pair(p[1] & !HANDEDNESS_BIT)).extend(sign),
        uv0: Vec2::new(f32::from_bits(p[2]), f32::from_bits(p[3])),
        uv1: Vec2::new(f32::from_bits(p[4]), f32::from_bits(p[5])),
        color: Vec4::new(rg.x, rg.y, ba.x, ba.y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{CpuMaterial, CpuNode};
    use glam::Quat;

    pub(crate) fn vertex(position: [f32; 3]) -> Vertex {
        Vertex {
            position,
            normal: [0.0, 0.0, 1.0],
            uv: [0.25, 0.75],
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0.0; 4],
            weights: [0.0; 4],
            color: [1.0, 0.5, 0.25, 1.0],
            uv1: [0.5, 0.5],
        }
    }

    pub(crate) fn quad(transform: Mat4, node: Option<usize>) -> CpuPrimitive {
        CpuPrimitive {
            vertices: [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ]
            .map(vertex)
            .to_vec(),
            indices: vec![0, 1, 2, 0, 2, 3],
            transform,
            node_index: node,
            skin_index: None,
            material: CpuMaterial::default(),
            morph_targets: Vec::new(),
            morph_weights: Vec::new(),
        }
    }

    fn gathered(scene: &CpuScene) -> Gathered {
        gather(scene, &MaterialTable::build(scene, 256), &[])
    }

    #[test]
    fn repeated_meshes_become_instances_of_one_mesh_and_the_rest_merge() {
        let moved = |x: f32| Mat4::from_translation(Vec3::new(x, 0.0, 0.0));
        let mut unique = quad(moved(9.0), None);
        unique.vertices[2].position = [2.0, 2.0, 0.0];
        let scene = CpuScene {
            primitives: vec![
                quad(moved(0.0), None),
                unique,
                quad(moved(3.0), None),
                quad(moved(6.0), None),
            ],
            ..CpuScene::default()
        };
        let g = gathered(&scene);
        assert_eq!(g.meshes.len(), 2, "the merged mesh and one instanced quad");
        let instanced: Vec<_> = g.instances.iter().filter(|i| i.mesh == 1).collect();
        assert_eq!(instanced.len(), 3);
        assert_eq!(instanced[2].local, moved(6.0));
        assert_eq!(
            g.meshes[1].positions[1],
            [1.0, 0.0, 0.0, 0.0],
            "object space"
        );
        let merged = g.instances[0];
        assert_eq!(
            (merged.mesh, merged.primitive, merged.local),
            (0, u32::MAX, Mat4::IDENTITY)
        );
        assert_eq!(
            g.meshes[0].positions[0],
            [9.0, 0.0, 0.0, 0.0],
            "world space"
        );
    }

    #[test]
    fn an_animated_node_keeps_its_own_instance_and_a_hidden_one_none() {
        let mut scene = CpuScene {
            primitives: vec![
                quad(Mat4::IDENTITY, Some(1)),
                quad(Mat4::from_scale(Vec3::ZERO), None),
            ],
            nodes: vec![
                CpuNode {
                    parent: None,
                    translation: Vec3::ZERO,
                    rotation: Quat::IDENTITY,
                    scale: Vec3::ONE,
                },
                CpuNode {
                    parent: Some(0),
                    translation: Vec3::ZERO,
                    rotation: Quat::IDENTITY,
                    scale: Vec3::ONE,
                },
            ],
            ..CpuScene::default()
        };
        scene.animations.push(crate::scene::CpuAnimation {
            name: String::new(),
            duration: 1.0,
            channels: vec![crate::scene::CpuAnimationChannel {
                node: 0,
                times: vec![0.0],
                values: crate::scene::ChannelValues::Translation(vec![Vec3::ZERO]),
                interpolation: crate::scene::Interpolation::Linear,
            }],
        });
        let g = gathered(&scene);
        assert_eq!(
            g.instances.len(),
            1,
            "the child of an animated node, not the zero-scale quad"
        );
        assert_eq!(g.instances[0].primitive, 0);
    }

    #[test]
    fn degenerate_triangles_are_dropped() {
        let mut prim = quad(Mat4::IDENTITY, None);
        prim.indices.extend([0, 1, 1]);
        let scene = CpuScene {
            primitives: vec![prim],
            ..CpuScene::default()
        };
        assert_eq!(gathered(&scene).meshes[0].triangles.len(), 2);
    }

    #[test]
    fn a_mirroring_transform_flips_the_merged_bitangent_sign() {
        let scene = CpuScene {
            primitives: vec![quad(Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0)), None)],
            ..CpuScene::default()
        };
        let g = gathered(&scene);
        let attributes = unpack_vertex(&g.meshes[0].vertices[0]);
        assert_eq!(attributes.tangent.w, -1.0);
        assert!((attributes.tangent.truncate() - Vec3::NEG_X).length() < 1e-3);
    }

    #[test]
    fn packed_attributes_round_trip_within_quantisation() {
        let mut v = vertex([0.0; 3]);
        v.uv1 = [-3.5, 12.25];
        for (i, n) in [
            Vec3::X,
            Vec3::NEG_Z,
            Vec3::new(0.3, -0.5, -0.81),
            Vec3::new(-0.6, 0.6, 0.53),
        ]
        .iter()
        .enumerate()
        {
            let n = n.normalize();
            let t = n.any_orthonormal_vector();
            let handedness = if i % 2 == 0 { 1.0 } else { -1.0 };
            let out = unpack_vertex(&pack_vertex(&v, n, t, handedness));
            assert!((out.normal - n).length() < 1e-3, "{n} -> {}", out.normal);
            assert!(
                (out.tangent.truncate() - t).length() < 1e-3,
                "{t} -> {}",
                out.tangent
            );
            assert_eq!(out.tangent.w, handedness);
            assert_eq!(
                (out.uv0, out.uv1),
                (Vec2::new(0.25, 0.75), Vec2::new(-3.5, 12.25))
            );
            assert!(
                (out.color - Vec4::new(1.0, 0.5, 0.25, 1.0))
                    .abs()
                    .max_element()
                    < 1e-4
            );
        }
    }
}
