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

/// The matrix [`skin_vertex`] moves `v` by: its joints blended by weight, or `node` when unweighted.
pub fn skin_matrix(v: &Vertex, joints: &[Mat4], node: Mat4) -> Mat4 {
    let total: f32 = v.weights.iter().sum();
    if total <= 1e-4 {
        return node;
    }
    let mut m = Mat4::ZERO;
    for k in 0..4 {
        let joint = joints
            .get(v.joints[k] as usize)
            .copied()
            .unwrap_or(Mat4::IDENTITY);
        m += joint * v.weights[k];
    }
    m * (1.0 / total)
}

/// One vertex through its weighted joints, or through `node` when unweighted, as `forward.slang` does.
/// `node` is the primitive's own transform: identity for a skinned one, whose node's transform glTF ignores.
pub fn skin_vertex(v: &Vertex, joints: &[Mat4], node: Mat4) -> Vertex {
    let matrix = skin_matrix(v, joints, node);
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

/// Reverses each triangle whose skin mirrors space at two or three of its corners, so its front stays front once
/// posed by `joints`; the corners' tangent signs flip on their own in [`skin_vertex`]. Returns how many it reversed.
pub fn rewind_mirrored_triangles(
    vertices: &[Vertex],
    indices: &mut [u32],
    joints: &[Mat4],
    node: Mat4,
) -> usize {
    let mirrored: Vec<bool> = vertices
        .iter()
        .map(|v| Pose::new(skin_matrix(v, joints, node)).mirrors())
        .collect();
    let mut reversed = 0;
    for triangle in indices.as_chunks_mut::<3>().0 {
        let corners = triangle
            .iter()
            .filter(|&&i| mirrored.get(i as usize).copied().unwrap_or(false))
            .count();
        if corners >= 2 {
            triangle.swap(1, 2);
            reversed += 1;
        }
    }
    reversed
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

#[cfg(test)]
mod tests {
    use bytemuck::Zeroable;

    use super::*;

    /// A vertex fully weighted to `joint`, or unweighted for `None`.
    fn on(joint: Option<f32>) -> Vertex {
        Vertex {
            joints: [joint.unwrap_or(0.0), 0.0, 0.0, 0.0],
            weights: [if joint.is_some() { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
            ..Vertex::zeroed()
        }
    }

    /// Two or three mirrored corners reverse a triangle, one or none keep it; an out-of-range index counts as upright.
    #[test]
    fn a_triangle_follows_the_mirror_of_most_of_its_corners() {
        let joints = [Mat4::IDENTITY, Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0))];
        // 0 and 1 upright, 2 and 3 mirrored, 4 unweighted on the node.
        let vertices = [
            on(Some(0.0)),
            on(Some(0.0)),
            on(Some(1.0)),
            on(Some(1.0)),
            on(None),
        ];
        let mut indices = [0, 1, 4, 2, 3, 2, 2, 3, 0, 2, 0, 1, 2, 3, 99, 0, 2, 4];
        let reversed = rewind_mirrored_triangles(&vertices, &mut indices, &joints, Mat4::IDENTITY);
        assert_eq!(reversed, 3);
        assert_eq!(
            indices,
            [0, 1, 4, 2, 2, 3, 2, 0, 3, 2, 0, 1, 2, 99, 3, 0, 2, 4]
        );
    }

    /// An unweighted vertex follows the node, so a mirroring node reverses its triangles too.
    #[test]
    fn unweighted_corners_follow_the_node() {
        let vertices = [on(None), on(None), on(None)];
        let mut indices = [0, 1, 2];
        let mirror = Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0));
        assert_eq!(
            rewind_mirrored_triangles(&vertices, &mut indices, &[], mirror),
            1
        );
        assert_eq!(indices, [0, 2, 1]);
    }
}
