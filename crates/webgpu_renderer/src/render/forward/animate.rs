//! Animation playback: node and morph channels, the posed bounds they move, joints and morph uploads.

use glam::Mat4;

use super::{ForwardRenderer, MAX_JOINTS};
use crate::context::GpuContext;
use crate::render::animation::{
    keyframe_lerp_indices, sample_morph_weights, sample_quat, sample_vec3,
};
use crate::render::bounds::{
    instanced_bounds, normal_matrix_of, transform_aabb, widen_bounds_for_skin,
};
use crate::scene::{ChannelValues, CpuAnimation, CpuNode, CpuSkin};

/// The uploaded scene's node graph, clips and skins, plus the pose the next frame uploads.
#[derive(Default)]
pub(super) struct SceneAnimation {
    pub(super) nodes: Vec<CpuNode>,
    pub(super) animations: Vec<CpuAnimation>,
    pub(super) skins: Vec<CpuSkin>,
    pub(super) pending_joint_world: Option<Vec<Mat4>>,
}

impl SceneAnimation {
    /// Samples every translation, rotation and scale channel at `time` into its node.
    pub(super) fn sample_node_channels(&mut self, time: f32) {
        for animation in &self.animations {
            for channel in &animation.channels {
                let Some(node) = self.nodes.get_mut(channel.node) else {
                    continue;
                };
                let t = clip_time(animation, time);
                let (i0, i1, frac, dt) = keyframe_lerp_indices(&channel.times, t);
                match &channel.values {
                    ChannelValues::Translation(values) => {
                        if let Some(v) =
                            sample_vec3(values, channel.interpolation, i0, i1, frac, dt)
                        {
                            node.translation = v;
                        }
                    }
                    ChannelValues::Rotation(values) => {
                        if let Some(q) =
                            sample_quat(values, channel.interpolation, i0, i1, frac, dt)
                        {
                            node.rotation = q;
                        }
                    }
                    ChannelValues::Scale(values) => {
                        if let Some(v) =
                            sample_vec3(values, channel.interpolation, i0, i1, frac, dt)
                        {
                            node.scale = v;
                        }
                    }
                    // Sampled by `sample_morph_channels`, which avoids fighting the `nodes` borrow.
                    ChannelValues::MorphWeights(_) => {}
                }
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
    /// Morph weights; the target count comes from each primitive to stride the flat channel.
    pub(super) fn sample_morph_channels(&mut self, time: f32) {
        for animation in &self.animation.animations {
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
            let matrices: Vec<[[f32; 4]; 4]> = skin
                .joints
                .iter()
                .take(MAX_JOINTS)
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
