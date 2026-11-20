//! Rust's half of the cross-renderer parity tests, in the C++ namespace `kataglyphis::oxidant::parity`: the viewport
//! remap, the lens and the Model tab's transform for given inputs, which the engine asserts its own functions against.
//! Matrices are column-major in WebGPU's clip space (Y up, depth 0..1); the C++ Vulkan ones negate row 1.

use kataglyphis_render_core::lens;
use kataglyphis_render_core::model_transform::ModelTransform;
use kataglyphis_render_core::viewport::SceneViewport;

use crate::{guarded, nothing};

#[cxx::bridge(namespace = "kataglyphis::oxidant::parity")]
pub mod ffi {
    /// A 4x4 matrix, column after column as glm and glam store it.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct Matrix4 {
        columns: [f32; 16],
    }

    /// The scene's share of the frame, in fractions from its top-left corner: the C++ `ViewportFraction`.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct ViewportRect {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
    }

    /// A rect as the renderer reads it, and the remap that centres the lens on it.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct ViewportRemap {
        sanitized: ViewportRect,
        /// The C++ `makeViewportRemap` negates its Y translation, for Vulkan's clip Y.
        remap: Matrix4,
    }

    /// A perspective lens on part of the frame; `frame_aspect` is the whole frame's width over its height.
    /// The planes are not `near` and `far`, which windows.h defines as macros.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct Lens {
        fov_y_deg: f32,
        frame_aspect: f32,
        near_plane: f32,
        far_plane: f32,
        viewport: ViewportRect,
    }

    /// The Model tab's pose: a translation, and rotations about X, Y and Z in degrees.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct ModelPose {
        position: [f32; 3],
        rotation_deg: [f32; 3],
    }

    extern "Rust" {
        /// `rect` as the renderer reads it and its remap: the C++ `sanitizeViewportFraction` and `makeViewportRemap`.
        fn parity_viewport_remap(rect: ViewportRect) -> ViewportRemap;
        /// The projection of `lens`, the remap included: the C++ remap times `makeVulkanProjection`, row 1 negated.
        fn parity_lens(lens: Lens) -> Matrix4;
        /// T * Rz * Ry * Rx, a non-finite field counting as zero: the C++ `makeGuiModelTransform`.
        fn parity_model_transform(pose: ModelPose) -> Matrix4;
    }
}

pub fn parity_viewport_remap(rect: ffi::ViewportRect) -> ffi::ViewportRemap {
    guarded(
        || {
            let sanitized = viewport(rect).sanitized();
            ffi::ViewportRemap {
                sanitized: ffi::ViewportRect {
                    x0: sanitized.x0,
                    y0: sanitized.y0,
                    x1: sanitized.x1,
                    y1: sanitized.y1,
                },
                remap: matrix(sanitized.remap()),
            }
        },
        nothing,
    )
}

pub fn parity_lens(lens: ffi::Lens) -> ffi::Matrix4 {
    guarded(
        || {
            matrix(lens::perspective(
                lens.fov_y_deg,
                lens.frame_aspect,
                lens.near_plane,
                lens.far_plane,
                viewport(lens.viewport),
            ))
        },
        nothing,
    )
}

pub fn parity_model_transform(pose: ffi::ModelPose) -> ffi::Matrix4 {
    guarded(
        || {
            let transform = ModelTransform {
                position: pose.position.into(),
                rotation_deg: pose.rotation_deg.into(),
            };
            matrix(transform.matrix())
        },
        nothing,
    )
}

fn viewport(rect: ffi::ViewportRect) -> SceneViewport {
    SceneViewport {
        x0: rect.x0,
        y0: rect.y0,
        x1: rect.x1,
        y1: rect.y1,
    }
}

fn matrix(m: glam::Mat4) -> ffi::Matrix4 {
    ffi::Matrix4 {
        columns: m.to_cols_array(),
    }
}

#[cfg(test)]
mod tests {
    use glam::{Mat4, Vec3, Vec4};

    use super::*;

    const DOCKED: ffi::ViewportRect = ffi::ViewportRect {
        x0: 0.155,
        y0: 0.04,
        x1: 0.79,
        y1: 0.77,
    };

    fn mat4(m: ffi::Matrix4) -> Mat4 {
        Mat4::from_cols_array(&m.columns)
    }

    #[test]
    fn the_remap_is_render_cores_for_the_sanitized_rect() {
        let docked = parity_viewport_remap(DOCKED);
        assert_eq!(docked.sanitized, DOCKED);
        assert_eq!(mat4(docked.remap), viewport(DOCKED).remap());
        // Column 3 is the lens shift: x0 + x1 - 1, and Y negated for WebGPU's clip Y.
        assert_eq!(
            docked.remap.columns[12..14],
            [0.155f32 + 0.79 - 1.0, 1.0 - (0.04f32 + 0.77)]
        );
        let wild = parity_viewport_remap(ffi::ViewportRect {
            x0: f32::NAN,
            y0: -0.5,
            x1: 1.5,
            y1: 0.6,
        });
        let expected = ffi::ViewportRect {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 0.6,
        };
        assert_eq!(wild.sanitized, expected, "NaN to 0, edges into the frame");
        let sliver = parity_viewport_remap(ffi::ViewportRect {
            x1: 0.04,
            ..expected
        });
        assert_eq!(mat4(sliver.remap), Mat4::IDENTITY, "a sliver is the frame");
    }

    #[test]
    fn the_lens_is_render_cores_and_lands_on_the_rect() {
        let docked = ffi::Lens {
            fov_y_deg: 45.0,
            frame_aspect: 1600.0 / 900.0,
            near_plane: 0.1,
            far_plane: 1000.0,
            viewport: DOCKED,
        };
        let projection = mat4(parity_lens(docked));
        let expected = lens::perspective(45.0, 1600.0 / 900.0, 0.1, 1000.0, viewport(DOCKED));
        assert_eq!(projection, expected);
        let clip = projection * Vec4::new(0.0, 0.0, -5.0, 1.0);
        let centre: [f32; 2] = [(0.155 + 0.79) * 0.5, (0.04 + 0.77) * 0.5];
        let fraction = [(clip.x / clip.w + 1.0) * 0.5, (1.0 - clip.y / clip.w) * 0.5];
        assert!((fraction[0] - centre[0]).abs() < 1e-5 && (fraction[1] - centre[1]).abs() < 1e-5);
        assert!(
            (0.0..=1.0).contains(&(clip.z / clip.w)),
            "depth 0..1 between the planes"
        );
        let unit =
            glam::camera::rh::proj::directx::perspective(45f32.to_radians(), 1.0, 0.1, 1000.0);
        let square = viewport(DOCKED).remap() * unit;
        for (width, height) in [(1600.0f32, 0.0f32), (0.0, 768.0)] {
            let empty = ffi::Lens {
                frame_aspect: width / height,
                ..docked
            };
            let what = format!("{width}x{height}: aspect 1, as the C++ engine");
            assert_eq!(mat4(parity_lens(empty)), square, "{what}");
        }
    }

    #[test]
    fn the_model_transform_rotates_x_first_and_zeroes_garbage() {
        let pose = ffi::ModelPose {
            position: [1.0, 2.0, 3.0],
            rotation_deg: [90.0, 90.0, 0.0],
        };
        let moved = mat4(parity_model_transform(pose)).transform_point3(Vec3::Y);
        assert!(moved.abs_diff_eq(Vec3::new(2.0, 2.0, 3.0), 1e-5), "{moved}");
        let garbage = ffi::ModelPose {
            position: [f32::NAN, 0.0, f32::INFINITY],
            rotation_deg: [f32::NAN; 3],
        };
        assert_eq!(mat4(parity_model_transform(garbage)), Mat4::IDENTITY);
    }
}
