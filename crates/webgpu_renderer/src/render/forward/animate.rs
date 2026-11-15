//! Animation playback: node and morph channels, the posed bounds they move, joints and morph uploads.

use glam::Mat4;

use super::ForwardRenderer;
use crate::context::GpuContext;
use crate::render::animation::{
    keyframe_lerp_indices, sample_morph_weights, sample_quat, sample_vec3,
};
use crate::render::bounds::{
    instanced_bounds, normal_matrix_of, transform_aabb, widen_bounds_for_skin,
};
use crate::scene::{ChannelValues, CpuAnimation, CpuNode, CpuScene, CpuSkin};

/// Bytes per joint in a joint buffer: one column-major mat4.
pub(super) const JOINT_MATRIX_BYTES: u64 = std::mem::size_of::<[[f32; 4]; 4]>() as u64;

/// The uploaded scene's node graph, clips and skins, plus the pose the next frame uploads.
#[derive(Default)]
pub(super) struct SceneAnimation {
    pub(super) nodes: Vec<CpuNode>,
    pub(super) animations: Vec<CpuAnimation>,
    pub(super) skins: Vec<CpuSkin>,
    pub(super) pending_joint_world: Option<Vec<Mat4>>,
    /// The one clip `set_animation_time` samples: clips on the same nodes would overwrite each other.
    active: usize,
    /// The file's pose, which a clip switch restores for the nodes the new clip does not drive.
    rest_nodes: Vec<CpuNode>,
    /// Each primitive's default morph weights, restored on a clip switch like `rest_nodes`.
    rest_morph_weights: Vec<Vec<f32>>,
}

impl SceneAnimation {
    /// The uploaded scene at its rest pose, with the first clip active.
    pub(super) fn for_scene(scene: &CpuScene) -> Self {
        Self {
            nodes: scene.nodes.clone(),
            animations: scene.animations.clone(),
            skins: scene.skins.clone(),
            pending_joint_world: None,
            active: 0,
            rest_nodes: scene.nodes.clone(),
            rest_morph_weights: scene
                .primitives
                .iter()
                .map(|prim| prim.morph_weights.clone())
                .collect(),
        }
    }

    /// Samples the active clip's translation, rotation and scale channels at `time` into their nodes.
    pub(super) fn sample_node_channels(&mut self, time: f32) {
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

/// `time` wrapped into the clip; a zero-length clip holds its first key.
fn clip_time(animation: &CpuAnimation, time: f32) -> f32 {
    if animation.duration > 0.0 {
        time % animation.duration
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
    pub(super) fn sample_morph_channels(&mut self, time: f32) {
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

    /// Moves every node-driven primitive to its node's `world` matrix and re-derives its bounds.
    pub(super) fn retarget_primitives(&mut self, world: &[Mat4]) {
        for prim in &mut self.primitives {
            if let Some(node) = prim.node_index {
                if let Some(m) = world.get(node) {
                    prim.model = *m;
                    prim.normal_matrix = normal_matrix_of(*m);
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
                    let (min, max) = instanced_bounds(bounds, &prim.instance_transforms);
                    prim.aabb_min = min;
                    prim.aabb_max = max;
                    prim.world_center = (min + max) * 0.5;
                }
            }
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
