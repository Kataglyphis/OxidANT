//! Mip-tail streaming's GPU half: each material texture, the levels it holds, and the finer steps a frame uploads.
//! The policy (tail, wanted level, budget) is `render/texture_stream.rs`.

use std::collections::HashMap;
use std::sync::Arc;

use super::frame::FrameView;
use super::primitive::GpuPrimitive;
use super::streamed_texture::{StepCounters, StreamUser, StreamedTexture};
use super::upload::{MaterialDefaults, MaterialSlots};
use super::ForwardRenderer;
use crate::context::GpuContext;
use crate::render::texture::{create_sampler, material_chain, material_view};
use crate::render::texture_stream::{
    distance_to_aabb, max_axis_scale, next_base, plan_steps, uv_density, uv_transform_scale,
    wanted_level, StreamRequest, TextureResidency, TextureStreaming,
};
use crate::scene::{CpuPrimitive, CpuSampler, CpuTextureRef};

/// The scene's material textures: one upload per (image, colour space), not per referencing slot.
#[derive(Default)]
pub(super) struct MaterialTextures {
    /// Index into `entries`; `None` for a payload that does not decode, whose slots keep their defaults.
    keys: HashMap<(usize, bool), Option<usize>>,
    entries: Vec<StreamedTexture>,
    samplers: HashMap<CpuSampler, wgpu::Sampler>,
    counters: StepCounters,
}

impl MaterialTextures {
    /// Distinct GPU textures created; shared images count once.
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(super) fn residency(&self) -> TextureResidency {
        let entries = &self.entries;
        TextureResidency {
            textures: entries.len(),
            streaming: entries.iter().filter(|e| e.pending()).count(),
            resident_bytes: entries.iter().map(|e| e.bytes_from(e.base)).sum(),
            full_bytes: entries.iter().map(|e| e.bytes_from(e.finest())).sum(),
            step_written_bytes: self.counters.written,
            step_copied_bytes: self.counters.copied,
        }
    }

    /// Base color, metallic-roughness, normal, emissive and occlusion of primitive `i`, each falling back alone.
    pub(super) fn material_slots(
        &mut self,
        gpu: &GpuContext,
        defaults: &MaterialDefaults,
        (prim, i): (&CpuPrimitive, usize),
        streaming: &TextureStreaming,
    ) -> MaterialSlots {
        let material = &prim.material;
        let slots = [
            (
                &material.base_color_texture,
                &defaults.white,
                material.base_uv_transform,
            ),
            (
                &material.metallic_roughness_texture,
                &defaults.white,
                material.mr_uv_transform,
            ),
            (
                &material.normal_texture,
                &defaults.flat_normal,
                material.normal_uv_transform,
            ),
            (
                &material.emissive_texture,
                &defaults.white,
                material.emissive_uv_transform,
            ),
            (
                &material.occlusion_texture,
                &defaults.white,
                material.occlusion_uv_transform,
            ),
        ];
        let mut densities = [None; 2];
        let mut out = MaterialSlots::with_capacity(slots.len());
        for (slot, (texture_ref, fallback, uv_transform)) in slots.into_iter().enumerate() {
            let Some(tex_ref) = texture_ref else {
                out.push(
                    (*fallback).clone(),
                    self.sampler(&gpu.device, CpuSampler::default()),
                );
                continue;
            };
            let label = format!("material_{i}_slot_{slot}");
            let view = self.entry_for(gpu, tex_ref, label, streaming).map(|id| {
                let uv1 = material.uv_set_mask & (1 << slot) != 0;
                let density = *densities[usize::from(uv1)]
                    .get_or_insert_with(|| uv_density(&prim.vertices, &prim.indices, uv1));
                let entry = &mut self.entries[id];
                entry.users.push(StreamUser {
                    primitive: i,
                    slot,
                    sampler: tex_ref.sampler,
                    uv_per_unit: density * uv_transform_scale(&uv_transform),
                });
                // The view, not the texture, follows the sampler: one upload serves both filter kinds.
                material_view(&entry.texture, &tex_ref.sampler)
            });
            out.push(
                view.unwrap_or_else(|| (*fallback).clone()),
                self.sampler(&gpu.device, tex_ref.sampler),
            );
        }
        out
    }

    fn entry_for(
        &mut self,
        gpu: &GpuContext,
        tex_ref: &CpuTextureRef,
        label: String,
        streaming: &TextureStreaming,
    ) -> Option<usize> {
        let key = (Arc::as_ptr(&tex_ref.texture) as usize, tex_ref.srgb);
        if let Some(&id) = self.keys.get(&key) {
            return id;
        }
        let id = material_chain(gpu, &tex_ref.texture, tex_ref.srgb, Some(&label))
            .and_then(|chain| StreamedTexture::upload(gpu, chain, label, streaming))
            .map(|entry| {
                self.entries.push(entry);
                self.entries.len() - 1
            });
        self.keys.insert(key, id);
        id
    }

    fn sampler(&mut self, device: &wgpu::Device, desc: CpuSampler) -> wgpu::Sampler {
        self.samplers
            .entry(desc)
            .or_insert_with(|| create_sampler(device, &desc))
            .clone()
    }
}

impl ForwardRenderer {
    /// Uploads this frame's finer levels: within the budget while streaming, every pending one once it is off.
    pub(super) fn stream_textures(&mut self, gpu: &GpuContext, view: &FrameView) {
        let steps = if self.texture_streaming.enabled {
            self.plan_texture_steps(view)
        } else {
            self.finishing_steps()
        };
        self.apply_texture_steps(gpu, &steps);
    }

    /// Uploads every texture's finest level now, ignoring the budget; for a capture that must not wait.
    pub fn finish_texture_streaming(&mut self, gpu: &GpuContext) {
        let steps = self.finishing_steps();
        self.apply_texture_steps(gpu, &steps);
    }

    /// What the material textures hold on the GPU, against their full chains.
    pub fn texture_residency(&self) -> TextureResidency {
        self.textures.residency()
    }

    fn finishing_steps(&self) -> Vec<(usize, usize)> {
        let entries = self.textures.entries.iter().enumerate();
        entries
            .filter(|(_, entry)| entry.pending())
            .map(|(id, entry)| (id, entry.finest()))
            .collect()
    }

    /// One step finer for each texture the camera needs sharper, as many as the budget holds.
    fn plan_texture_steps(&self, view: &FrameView) -> Vec<(usize, usize)> {
        // Pixels one world unit covers at distance 1; `projection` carries the dock's viewport remap.
        let pixels_per_unit = view.projection.y_axis.y.abs() * view.height as f32 * 0.5;
        let streaming = self.texture_streaming;
        let mut next = HashMap::new();
        let mut requests = Vec::new();
        for (id, entry) in self.textures.entries.iter().enumerate() {
            let (wanted, visible) = texture_need(&self.primitives, entry, view, pixels_per_unit);
            let Some(base) = next_base(&entry.candidates, entry.base, wanted) else {
                continue;
            };
            next.insert(id, base);
            requests.push(StreamRequest {
                id,
                visible,
                levels_short: entry.base - wanted,
                step_bytes: entry.step_bytes(base),
            });
        }
        plan_steps(requests, streaming.bytes_per_frame)
            .into_iter()
            .map(|id| (id, next[&id]))
            .collect()
    }

    /// Steps each texture to its new base and rebinds the primitives that sample it; one submit for the copies.
    fn apply_texture_steps(&mut self, gpu: &GpuContext, steps: &[(usize, usize)]) {
        let streaming = self.texture_streaming;
        let textures = &mut self.textures;
        if steps.is_empty() {
            return;
        }
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("texture_streaming_copies"),
            });
        let mut touched = Vec::new();
        for &(id, base) in steps {
            let entry = &mut textures.entries[id];
            if !entry.step(
                gpu,
                &mut encoder,
                base,
                (streaming.gpu_copies, &mut textures.counters),
            ) {
                continue;
            }
            for user in &entry.users {
                let view = material_view(&entry.texture, &user.sampler);
                self.primitives[user.primitive]
                    .material_slots
                    .set_view(user.slot, view);
                touched.push(user.primitive);
            }
        }
        gpu.queue.submit([encoder.finish()]);
        touched.sort_unstable();
        touched.dedup();
        self.rebind_primitives(&gpu.device, &touched);
    }

    /// Group 0 and, for a MASK caster, the alpha-testing shadow group, around new material views.
    fn rebind_primitives(&mut self, device: &wgpu::Device, indices: &[usize]) {
        for &i in indices {
            let prim = &self.primitives[i];
            let (uniforms, joints, slots) = (
                &prim.uniform_buffer,
                &prim.joint_buffer,
                &prim.material_slots,
            );
            let main = self.create_material_bind_group(device, i, uniforms, joints, slots);
            let masked = prim
                .alpha_masked
                .then(|| self.create_masked_shadow_bind_group(device, i, uniforms, joints, slots));
            let prim = &mut self.primitives[i];
            prim.bind_group = main;
            if masked.is_some() {
                prim.shadow_masked_bind_group = masked;
                // The caster bundles baked the old alpha-testing group.
                self.shadow.caster_bundles = None;
            }
        }
    }
}

/// The finest level any user of `entry` needs from this view, and whether any user is in the frustum.
fn texture_need(
    primitives: &[GpuPrimitive],
    entry: &StreamedTexture,
    view: &FrameView,
    pixels_per_unit: f32,
) -> (usize, bool) {
    let side = entry.side() as f32;
    let (mut wanted, mut visible) = (usize::MAX, false);
    for user in &entry.users {
        let Some(prim) = primitives.get(user.primitive) else {
            continue;
        };
        let scale = max_axis_scale(&(prim.root * prim.model));
        let texels_per_unit = side * user.uv_per_unit / scale;
        let distance = distance_to_aabb(view.eye, prim.aabb_min, prim.aabb_max);
        wanted = wanted.min(wanted_level(texels_per_unit, distance, pixels_per_unit));
        visible |= view.frustum.intersects_aabb(prim.aabb_min, prim.aabb_max);
    }
    (wanted, visible)
}
