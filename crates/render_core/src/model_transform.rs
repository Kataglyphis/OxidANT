//! The Model tab's transform of the loaded scene: the C++ `makeGuiModelTransform`.

use glam::{Mat4, Vec3};

/// A translation and XYZ rotations in degrees; no scale, so it never mirrors and keeps lengths.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ModelTransform {
    pub position: Vec3,
    /// Degrees about X, Y and Z.
    pub rotation_deg: Vec3,
}

impl ModelTransform {
    /// T * Rz * Ry * Rx, the C++ order; a non-finite field counts as zero.
    pub fn matrix(&self) -> Mat4 {
        let finite =
            |v: Vec3| Vec3::from_array(v.to_array().map(|c| if c.is_finite() { c } else { 0.0 }));
        let position = finite(self.position);
        let radians = finite(self.rotation_deg) * (std::f32::consts::PI / 180.0);
        Mat4::from_translation(position)
            * Mat4::from_rotation_z(radians.z)
            * Mat4::from_rotation_y(radians.y)
            * Mat4::from_rotation_x(radians.x)
    }

    pub fn is_identity(&self) -> bool {
        self.matrix() == Mat4::IDENTITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rotations_apply_x_first_then_y_then_z_then_the_translation() {
        let transform = ModelTransform {
            position: Vec3::new(1.0, 2.0, 3.0),
            rotation_deg: Vec3::new(90.0, 90.0, 0.0),
        };
        // X turns +Y into +Z, then Y turns +Z into +X; the reverse order would leave +Z.
        let moved = transform.matrix().transform_point3(Vec3::Y);
        assert!(moved.abs_diff_eq(Vec3::new(2.0, 2.0, 3.0), 1e-5), "{moved}");
        let z_last = ModelTransform {
            rotation_deg: Vec3::new(0.0, 90.0, 90.0),
            ..ModelTransform::default()
        };
        // Y turns +X into -Z, which Z leaves alone.
        let turned = z_last.matrix().transform_vector3(Vec3::X);
        assert!(turned.abs_diff_eq(Vec3::NEG_Z, 1e-5), "{turned}");
    }

    #[test]
    fn the_default_is_the_identity_and_garbage_counts_as_zero() {
        assert!(ModelTransform::default().is_identity());
        let garbage = ModelTransform {
            position: Vec3::new(f32::NAN, 0.0, f32::INFINITY),
            rotation_deg: Vec3::splat(f32::NAN),
        };
        assert!(garbage.is_identity());
        let turned = ModelTransform {
            rotation_deg: Vec3::new(0.0, 30.0, 0.0),
            ..ModelTransform::default()
        };
        let m = turned.matrix();
        assert!((glam::Mat3::from_mat4(m).determinant() - 1.0).abs() < 1e-5);
        assert!(!turned.is_identity());
    }
}
