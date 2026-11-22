//! Meshes an animation skins or morphs: a mesh each, re-posed on the CPU and refitted when the pose moves.

use glam::Mat4;
use kataglyphis_assets::skin::joint_matrices;

use super::geometry::{
    append_primitive, append_vertices, Deformation, Gathered, InstanceSource, MeshBuild,
};
use super::material::MaterialTable;
use crate::scene::{CpuScene, CpuSkin, MorphTarget, Vertex};

/// What re-poses one deformed mesh: the primitive's rest vertices, morph targets, skin and the pose last applied.
#[derive(Clone, Debug)]
pub struct DeformedSource {
    pub mesh: u32,
    pub primitive: u32,
    vertices: Vec<Vertex>,
    targets: Vec<MorphTarget>,
    skin: Option<CpuSkin>,
    /// The primitive's own transform, which no animation moves: glTF ignores a skinned mesh's node.
    transform: Mat4,
    /// The weights the build saw, for a caller that passes none.
    rest_weights: Vec<f32>,
    /// The joint matrices and weights the arrays hold now.
    posed: (Vec<Mat4>, Vec<f32>),
}

impl DeformedSource {
    /// The inputs of a pose at `world` with `weights`: joint matrices and the weights.
    fn inputs(&self, world: &[Mat4], weights: &[f32]) -> (Vec<Mat4>, Vec<f32>) {
        let joints = self
            .skin
            .as_ref()
            .map_or_else(Vec::new, |skin| joint_matrices(skin, world));
        let weights = if weights.is_empty() {
            &self.rest_weights
        } else {
            weights
        };
        (joints, weights.to_vec())
    }

    fn deformed(&self, (joints, weights): &(Vec<Mat4>, Vec<f32>)) -> Vec<Vertex> {
        let pose = Deformation {
            targets: &self.targets,
            weights,
            joints: self.skin.is_some().then_some(joints.as_slice()),
            node: self.transform,
        };
        pose.apply(&self.vertices).0.into_owned()
    }

    /// The positions and packed vertices at `world` with `weights` (by the primitive's targets); `None` when unmoved.
    pub fn pose(&mut self, world: &[Mat4], weights: &[f32]) -> Option<MeshBuild> {
        let inputs = self.inputs(world, weights);
        if inputs == self.posed {
            return None;
        }
        let mut build = MeshBuild::default();
        append_vertices(&mut build, &self.deformed(&inputs), None);
        self.posed = inputs;
        Some(build)
    }
}

/// Gives primitive `primitive` a mesh of its own at its first pose; a skinned one lies in world space, uninstanced.
pub(super) fn push_deformed(
    gathered: &mut Gathered,
    scene: &CpuScene,
    table: &MaterialTable,
    primitive: usize,
    world: &[Mat4],
) {
    let prim = &scene.primitives[primitive];
    let mut source = DeformedSource {
        mesh: gathered.meshes.len() as u32,
        primitive: primitive as u32,
        vertices: prim.vertices.clone(),
        targets: prim.morph_targets.clone(),
        skin: prim.skin_index.and_then(|s| scene.skins.get(s)).cloned(),
        transform: prim.transform,
        rest_weights: prim.morph_weights.clone(),
        posed: (Vec::new(), Vec::new()),
    };
    source.posed = source.inputs(world, &prim.morph_weights);
    let mut build = MeshBuild::default();
    let material = table.primitive_material[primitive];
    let vertices = source.deformed(&source.posed);
    append_primitive(
        &mut build,
        (&vertices, &prim.indices),
        None,
        material,
        table,
    );
    if build.triangles.is_empty() {
        return;
    }
    let in_world = source.skin.is_some();
    let local = if in_world {
        Mat4::IDENTITY
    } else {
        prim.transform
    };
    gathered.meshes.push(build);
    gathered.instances.push(InstanceSource {
        mesh: source.mesh,
        // A skinned mesh is posed in world space, so no node transform may move it.
        primitive: if in_world { u32::MAX } else { source.primitive },
        extra: Mat4::IDENTITY,
        base: local,
        local,
    });
    gathered.deformed.push(source);
}
