//! Skinning on the CPU as `forward.slang` skins: each joint's matrix, and a vertex through its weighted joints.

use glam::{Mat3, Mat4, Vec3};

use crate::scene::{CpuSkin, Vertex};

/// Each joint's world matrix times its inverse bind matrix, as the raster's joint buffer holds them.
pub fn joint_matrices(skin: &CpuSkin, world: &[Mat4]) -> Vec<Mat4> {
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

/// One vertex through its weighted joints, or through `node` when unweighted, as `forward.slang` does.
/// `node` is the primitive's own transform: identity for a skinned one, whose node's transform glTF ignores.
pub fn skin_vertex(v: &Vertex, joints: &[Mat4], node: Mat4) -> Vertex {
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

/// Inverse-transpose of a model matrix, identity when singular.
/// Zero-scale nodes (how exporters hide objects) are common; a NaN normal matrix shades garbage.
pub fn normal_matrix_of(model: Mat4) -> Mat4 {
    let inv = model.inverse();
    if inv.is_finite() {
        inv.transpose()
    } else {
        Mat4::IDENTITY
    }
}

/// A transform with its normal matrix and mirror sign, worked out once for all of a primitive's vertices.
#[derive(Clone, Copy, Debug)]
pub struct Pose {
    model: Mat4,
    normal: Mat4,
    mirror: f32,
}

impl Pose {
    pub fn new(model: Mat4) -> Self {
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

    pub fn model(&self) -> Mat4 {
        self.model
    }

    /// Whether the transform mirrors space, which reverses a triangle's winding.
    pub fn mirrors(&self) -> bool {
        self.mirror < 0.0
    }

    /// The normal, tangent and bitangent sign of `v`; a mirroring pose flips the sign, as the raster does.
    pub fn frame(&self, v: &Vertex) -> (Vec3, Vec3, f32) {
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
