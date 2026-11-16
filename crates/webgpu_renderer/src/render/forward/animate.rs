//! Animation playback: node and morph channels, the posed bounds they move, joints and morph uploads.

use glam::{Mat4, Vec3};

use super::primitive::is_mirrored;
use super::ForwardRenderer;
use crate::context::GpuContext;
use crate::render::animation::{
    keyframe_lerp_indices, sample_morph_weights, sample_quat, sample_vec3,
};
use crate::render::bounds::{
    instanced_bounds, morph_local_aabb, normal_matrix_of, transform_aabb, widen_bounds_for_skin,
};
use crate::render::lights::pack_punctual_lights;
use crate::scene::{ChannelValues, CpuAnimation, CpuLight, CpuNode, CpuScene, CpuSkin};

/// Bytes per joint in a joint buffer: one column-major mat4.
pub(super) const JOINT_MATRIX_BYTES: u64 = std::mem::size_of::<[[f32; 4]; 4]>() as u64;

/// The uploaded scene's node graph, clips and skins, plus the pose the next frame uploads.
#[derive(Default)]
pub(super) struct SceneAnimation {
    pub(super) nodes: Vec<CpuNode>,
    pub(super) animations: Vec<CpuAnimation>,
    pub(super) skins: Vec<CpuSkin>,
    pub(super) pending_joint_world: Option<Vec<Mat4>>,
    /// Every node's world matrix at the current pose, which the tracer re-poses its skins from.
    pub(super) world: Vec<Mat4>,
    /// The one clip `set_animation_time` samples: clips on the same nodes would overwrite each other.
    active: usize,
    /// The file's pose, which a clip switch restores for the nodes the new clip does not drive.
    rest_nodes: Vec<CpuNode>,
    /// Each primitive's default morph weights, restored on a clip switch like `rest_nodes`.
    rest_morph_weights: Vec<Vec<f32>>,
    /// The scene's punctual lights, re-posed from their nodes on every animation frame.
    lights: Vec<CpuLight>,
}

impl SceneAnimation {
    /// The uploaded scene at its rest pose, with the first clip active.
    pub(super) fn for_scene(scene: &CpuScene) -> Self {
        Self {
            nodes: scene.nodes.clone(),
            animations: scene.animations.clone(),
            skins: scene.skins.clone(),
            pending_joint_world: None,
            world: CpuScene::compute_world_transforms(&scene.nodes),
            active: 0,
            rest_nodes: scene.nodes.clone(),
            rest_morph_weights: scene
                .primitives
                .iter()
                .map(|prim| prim.morph_weights.clone())
                .collect(),
            lights: scene.lights.clone(),
        }
    }

    /// Samples the active clip's translation, rotation and scale channels at `time` into their nodes.
    pub(super) fn sample_node_channels(&mut self, time: f64) {
        let Some(animation) = self.animations.get(self.active) else {
            return;
        };
        let t = clip_time(animation, time);
        for channel in &animation.channels {
            let Some(node) = self.nodes.get_mut(channel.node) else {
                continue;
            };
            let (i0, i1, frac, dt) = keyframe_lerp_indices(&channel.times, t);
            let interpolation = channel.interpolation;
            match &channel.values {
                ChannelValues::Translation(values) => {
                    if let Some(v) = sample_vec3(values, interpolation, i0, i1, frac, dt) {
                        node.translation = v;
                    }
                }
                ChannelValues::Rotation(values) => {
                    if let Some(q) = sample_quat(values, interpolation, i0, i1, frac, dt) {
                        node.rotation = q;
                    }
                }
                ChannelValues::Scale(values) => {
                    if let Some(v) = sample_vec3(values, interpolation, i0, i1, frac, dt) {
                        node.scale = v;
                    }
                }
                // Sampled by `sample_morph_channels`, which avoids fighting the `nodes` borrow.
                ChannelValues::MorphWeights(_) => {}
            }
        }
    }
}

/// `time` wrapped into the clip in f64, then narrowed: in f32 the wrap itself loses the fraction.
/// A zero-length clip holds its first key.
fn clip_time(animation: &CpuAnimation, time: f64) -> f32 {
    if animation.duration > 0.0 {
        (time % f64::from(animation.duration)) as f32
    } else {
        0.0
    }
}

impl ForwardRenderer {
    /// The uploaded pose: joint matrices for the next frame and scene bounds from the posed boxes.
    /// A scene without animations never calls `set_animation_time`, so its skins pose here.
    pub(super) fn settle_uploaded_pose(&mut self, scene: &CpuScene, node_world: Vec<Mat4>) {
        if !scene.skins.is_empty() {
            self.animation.pending_joint_world = Some(node_world);
        }
        self.recompute_scene_bounds();
    }

    /// Names of the uploaded clips in file order, for [`Self::set_active_animation`].
    pub fn animation_names(&self) -> Vec<&str> {
        self.animation
            .animations
            .iter()
            .map(|animation| animation.name.as_str())
            .collect()
    }

    /// The clip `set_animation_time` plays; the first one after an upload.
    pub fn active_animation(&self) -> usize {
        self.animation.active
    }

    /// Plays clip `index` from the next `set_animation_time` on; an index past the last clip is ignored.
    /// Nodes and morph weights return to the file's pose first, since the new clip may drive fewer of them.
    pub fn set_active_animation(&mut self, index: usize) {
        if index >= self.animation.animations.len() || index == self.animation.active {
            return;
        }
        self.animation.active = index;
        self.animation.nodes = self.animation.rest_nodes.clone();
        for (prim, rest) in self
            .primitives
            .iter_mut()
            .zip(&self.animation.rest_morph_weights)
        {
            if prim.morph_weights != *rest {
                prim.morph_weights = rest.clone();
                prim.morph_dirty = true;
            }
        }
    }

    /// The active clip's morph weights; each primitive's target count strides the flat channel.
    pub(super) fn sample_morph_channels(&mut self, time: f64) {
        if let Some(animation) = self.animation.animations.get(self.animation.active) {
            let t = clip_time(animation, time);
            for channel in &animation.channels {
                let ChannelValues::MorphWeights(values) = &channel.values else {
                    continue;
                };
                let (i0, i1, frac, dt) = keyframe_lerp_indices(&channel.times, t);
                for prim in &mut self.primitives {
                    if prim.node_index != Some(channel.node) || prim.morph_targets.is_empty() {
                        continue;
                    }
                    let n = prim.morph_targets.len();
                    let w =
                        sample_morph_weights(values, n, channel.interpolation, i0, i1, frac, dt);
                    if w != prim.morph_weights {
                        prim.morph_weights = w;
                        prim.morph_dirty = true;
                    }
                }
            }
        }
    }

    /// Re-poses every light tied to a node from `world` and repacks them; a light at load stayed put.
    pub(super) fn retarget_lights(&mut self, world: &[Mat4]) {
        let mut moved = false;
        for light in &mut self.animation.lights {
            if let Some(m) = light.node.and_then(|node| world.get(node)) {
                light.pose_at(*m);
                moved = true;
            }
        }
        if moved {
            self.repack_lights();
        }
    }

    /// Packs the scene's lights where the Model tab's root places them.
    pub(super) fn repack_lights(&mut self) {
        let root = self.placed_root;
        let placed: Vec<CpuLight> = self
            .animation
            .lights
            .iter()
            .map(|light| CpuLight {
                position: root
                    .transform_point3(Vec3::from_array(light.position))
                    .to_array(),
                direction: root
                    .transform_vector3(Vec3::from_array(light.direction))
                    .to_array(),
                ..*light
            })
            .collect();
        let (packed, count) = pack_punctual_lights(&placed);
        self.lights.punctual = packed;
        self.lights.punctual_count = count;
    }

    /// Moves every node-driven primitive to its node's `world` matrix and re-derives its bounds.
    pub(super) fn retarget_primitives(&mut self, world: &[Mat4]) {
        let mut winding_flipped = false;
        for prim in &mut self.primitives {
            // New weights move the vertices, so the local box follows them before it is posed.
            if prim.morph_dirty && !prim.base_vertices.is_empty() {
                (prim.local_aabb_min, prim.local_aabb_max) = morph_local_aabb(
                    &prim.base_vertices,
                    &prim.morph_targets,
                    &prim.morph_weights,
                );
            }
            if let Some(node) = prim.node_index {
                if let Some(m) = world.get(node) {
                    prim.model = *m;
                    prim.normal_matrix = normal_matrix_of(*m);
                    winding_flipped |= prim.mirrored != is_mirrored(m);
                    prim.mirrored = is_mirrored(m);
                    prim.uniforms_dirty = true;
                    let mut bounds = transform_aabb(*m, prim.local_aabb_min, prim.local_aabb_max);
                    if let Some(skin) = prim.skin_index.and_then(|s| self.animation.skins.get(s)) {
                        bounds = widen_bounds_for_skin(
                            bounds,
                            prim.local_aabb_min,
                            prim.local_aabb_max,
                            skin,
                            world,
                        );
                    }
                    // The posed box moved, so re-apply the instances on top.
                    prim.pre_instance_aabb = bounds;
                    let (min, max) = instanced_bounds(bounds, &prim.placed_instances());
                    prim.aabb_min = min;
                    prim.aabb_max = max;
                    prim.world_center = (min + max) * 0.5;
                }
            }
        }
        // An animated negative scale reverses the winding the shadow bundles baked in.
        if winding_flipped {
            self.shadow.caster_bundles = None;
        }
    }

    /// Uploads skin joint matrices (joint world * inverse bind) once per frame.
    pub(super) fn update_joint_matrices(&mut self, gpu: &GpuContext) {
        let Some(world) = self.animation.pending_joint_world.take() else {
            return;
        };
        if self.animation.skins.is_empty() {
            return;
        }
        for prim in &self.primitives {
            let Some(skin_index) = prim.skin_index else {
                continue;
            };
            let Some(skin) = self.animation.skins.get(skin_index) else {
                continue;
            };
            // The device's binding limit may have clamped the buffer below the skin's joint count.
            let capacity = (prim.joint_buffer.size() / JOINT_MATRIX_BYTES) as usize;
            let matrices: Vec<[[f32; 4]; 4]> = skin
                .joints
                .iter()
                .take(capacity)
                .enumerate()
                .map(|(i, &node)| {
                    let joint_world = world.get(node).copied().unwrap_or(Mat4::IDENTITY);
                    let inverse_bind = skin
                        .inverse_bind_matrices
                        .get(i)
                        .copied()
                        .unwrap_or(Mat4::IDENTITY);
                    (joint_world * inverse_bind).to_cols_array_2d()
                })
                .collect();
            if !matrices.is_empty() {
                gpu.queue
                    .write_buffer(&prim.joint_buffer, 0, bytemuck::cast_slice(&matrices));
            }
        }
    }

    /// Re-blends and re-uploads the vertex buffer of every morphed primitive whose weights moved.
    pub(super) fn apply_morph_targets(&mut self, gpu: &GpuContext) {
        for prim in &mut self.primitives {
            if !prim.morph_dirty || prim.base_vertices.is_empty() {
                continue;
            }
            let blended = crate::scene::blend_morph_targets(
                &prim.base_vertices,
                &prim.morph_targets,
                &prim.morph_weights,
            );
            gpu.queue
                .write_buffer(&prim.vertex_buffer, 0, bytemuck::cast_slice(&blended));
            prim.morph_dirty = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{CpuAnimationChannel, Interpolation};
    use glam::{Quat, Vec3};

    #[test]
    fn lights_on_animated_nodes_follow_them() {
        let Some(gpu) = GpuContext::headless_or_skip() else {
            return;
        };
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/point_light.gltf");
        let mut scene = crate::load_gltf(&path).expect("point_light.gltf must load");
        let point_node = scene.lights[0].node.expect("a KHR light carries its node");
        let spot_node = scene.lights[1].node.expect("a KHR light carries its node");
        let channel = |node, values| CpuAnimationChannel {
            node,
            times: vec![0.0, 2.0],
            values,
            interpolation: Interpolation::Linear,
        };
        scene.animations = vec![CpuAnimation {
            name: "move".to_string(),
            duration: 2.0,
            channels: vec![
                channel(
                    point_node,
                    ChannelValues::Translation(vec![
                        Vec3::new(-2.0, 1.0, 0.0),
                        Vec3::new(2.0, 1.0, 0.0),
                    ]),
                ),
                channel(spot_node, ChannelValues::Rotation(vec![Quat::IDENTITY; 2])),
            ],
        }];
        let mut renderer = ForwardRenderer::new(&gpu, 4, 4);
        renderer.upload_scene(&gpu, &scene);
        renderer.set_animation_time(1.0);

        let row = |light: usize, row: usize| {
            let r = renderer.lights.punctual[light * 4 + row];
            Vec3::new(r[0], r[1], r[2])
        };
        assert!(
            row(0, 0).abs_diff_eq(Vec3::new(0.0, 1.0, 0.0), 1e-5),
            "the point light must sit at its node's animated position, got {}",
            row(0, 0)
        );
        assert!(
            row(1, 2).abs_diff_eq(Vec3::NEG_Z, 1e-5),
            "an unrotated spot points down -Z, got {}",
            row(1, 2)
        );
    }

    #[test]
    fn clip_time_keeps_its_fraction_after_days_of_uptime() {
        // 2^18 s is three days, where consecutive f32 seconds are 1/32 s apart: a 144 Hz frame is 1/144 s.
        let clip = CpuAnimation {
            name: "clip".to_string(),
            duration: 1.0,
            channels: Vec::new(),
        };
        let base = f64::from(1u32 << 18);
        let frames: Vec<f64> = (0..60).map(|k| base + f64::from(k) / 144.0).collect();
        let wrapped: Vec<f32> = frames.iter().map(|&t| clip_time(&clip, t)).collect();
        assert!(
            wrapped.windows(2).all(|pair| pair[1] > pair[0]),
            "every frame must advance the clip: {wrapped:?}"
        );
        // Negative control: the same frames as f32 seconds repeat.
        let in_f32: Vec<f32> = frames.iter().map(|&t| (t as f32) % 1.0).collect();
        assert!(in_f32.windows(2).any(|pair| pair[1] <= pair[0]));
    }
}
