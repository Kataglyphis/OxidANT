//! The Model tab's transform: one root matrix left of every primitive's instances and on the scene's lights.

use super::ForwardRenderer;
use crate::context::GpuContext;

impl ForwardRenderer {
    /// Places every primitive and light anew when `model_transform` moved since the last frame.
    pub(super) fn apply_model_transform(&mut self, gpu: &GpuContext) {
        let root = self.model_transform.matrix();
        if root == self.placed_root {
            return;
        }
        self.placed_root = root;
        let mut stale = false;
        for prim in &mut self.primitives {
            prim.root = root;
            let instances = std::mem::take(&mut prim.instance_transforms);
            stale |= prim.write_instances(gpu, &instances);
        }
        // A grown instance buffer or a new winding split is baked into the caster bundles.
        if stale {
            self.shadow.caster_bundles = None;
        }
        self.repack_lights();
        self.recompute_scene_bounds();
        // The traced scene keeps the root across a build, so a transform set before its first traced frame holds too.
        self.tracer.set_root_transform(gpu, root);
    }

    /// The root matrix the instances and lights carry now.
    pub fn placed_root(&self) -> glam::Mat4 {
        self.placed_root
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;
    use crate::render::model_transform::ModelTransform;

    #[test]
    fn the_scene_s_lights_move_and_turn_with_the_model() {
        let Some(gpu) = GpuContext::headless_or_skip() else {
            return;
        };
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/tests/assets/point_light.gltf");
        let scene = crate::load_gltf(&path).expect("point_light.gltf must load");
        let mut renderer = ForwardRenderer::new(&gpu, 4, 4);
        renderer.upload_scene(&gpu, &scene);
        let row = |renderer: &ForwardRenderer, light: usize, row: usize| {
            let r = renderer.lights.punctual[light * 4 + row];
            Vec3::new(r[0], r[1], r[2])
        };
        let (position, direction) = (row(&renderer, 0, 0), row(&renderer, 1, 2));
        renderer.model_transform = ModelTransform {
            position: Vec3::new(0.0, 0.0, 2.0),
            rotation_deg: Vec3::new(0.0, 90.0, 0.0),
        };
        renderer.apply_model_transform(&gpu);
        let turn = glam::Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let expected = turn.transform_point3(position) + Vec3::new(0.0, 0.0, 2.0);
        assert!(
            row(&renderer, 0, 0).abs_diff_eq(expected, 1e-5),
            "{}",
            row(&renderer, 0, 0)
        );
        let turned = turn.transform_vector3(direction);
        assert!(
            row(&renderer, 1, 2).abs_diff_eq(turned, 1e-5),
            "{}",
            row(&renderer, 1, 2)
        );
        renderer.model_transform = ModelTransform::default();
        renderer.apply_model_transform(&gpu);
        assert_eq!(row(&renderer, 0, 0), position, "the identity puts it back");
    }
}
