//! Punctual-light shadows: a depth cube for each of the first four point and spot lights, as the C++ engine's
//! `PunctualShadowMap`, drawn through the cascades' caster pipelines into one 2D array of six layers per cube.

use glam::{Mat4, Vec3};

use super::readback::{read_texture, TextureRegion};
use super::shadow::{ShadowViewRaw, PUNCTUAL_FACE_VIEW};
use super::{ForwardRenderer, DEPTH_FORMAT};
use crate::context::GpuContext;
use crate::render::bounds::Frustum;
use crate::render::buffer_desc;
use crate::render::gpu_timing::TimedPass;
use crate::render::lights::{
    assign_punctual_shadow_slots, punctual_shadow_face_matrices, PunctualShadowSlot,
    MAX_SHADOWED_PUNCTUAL_LIGHTS,
};

/// Texels per side of each cube face, the C++ engine's.
pub(super) const PUNCTUAL_SHADOW_MAP_SIZE: u32 = 512;
/// Six faces per cube; a 2D array, not a cube array, so a WebGPU compatibility device samples it too.
const FACE_LAYERS: usize = 6 * MAX_SHADOWED_PUNCTUAL_LIGHTS;

pub(super) struct PunctualShadowResources {
    /// Read back only by tests.
    texture: wgpu::Texture,
    /// The whole array, group 3 binding 3 of the forward and deferred lighting.
    pub(super) view: wgpu::TextureView,
    layer_views: Vec<wgpu::TextureView>,
    /// One group 1 per layer: written before the encoder exists, so faces cannot share a buffer.
    face_buffers: Vec<wgpu::Buffer>,
    face_bind_groups: Vec<wgpu::BindGroup>,
    /// This frame's cubes and each layer's view-projection.
    slots: Vec<PunctualShadowSlot>,
    face_matrices: [Mat4; FACE_LAYERS],
}

impl PunctualShadowResources {
    pub(super) fn new(device: &wgpu::Device, view_layout: &wgpu::BindGroupLayout) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            // TEXTURE_2D_SHAPE_OK: depth array, one layer per cube face
            label: Some("punctual_shadow_faces"),
            size: wgpu::Extent3d {
                width: PUNCTUAL_SHADOW_MAP_SIZE,
                height: PUNCTUAL_SHADOW_MAP_SIZE,
                depth_or_array_layers: FACE_LAYERS as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            // COPY_SRC only for `read_punctual_shadow_map`.
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("punctual_shadow_faces_view"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let layer_views = (0..FACE_LAYERS as u32)
            .map(|layer| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("punctual_shadow_face"),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: layer,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let size = std::mem::size_of::<ShadowViewRaw>() as wgpu::BufferAddress;
        let face_buffers: Vec<wgpu::Buffer> = (0..FACE_LAYERS)
            .map(|_| buffer_desc::uniform(device, "punctual_shadow_face", size))
            .collect();
        let face_bind_groups = face_buffers
            .iter()
            .map(|buffer| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("punctual_shadow_face_bg"),
                    layout: view_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    }],
                })
            })
            .collect();
        Self {
            texture,
            view,
            layer_views,
            face_buffers,
            face_bind_groups,
            slots: Vec::new(),
            face_matrices: [Mat4::IDENTITY; FACE_LAYERS],
        }
    }
}

impl ForwardRenderer {
    /// Gives the lights their cubes and writes each drawn face's matrix; before the lights upload.
    pub(super) fn prepare_punctual_shadows(&mut self, gpu: &GpuContext) {
        let lights = &mut self.lights;
        let punctual = &mut self.punctual_shadow;
        punctual.slots = assign_punctual_shadow_slots(
            &mut lights.punctual,
            lights.punctual_count,
            self.scene_bounds,
        );
        if !self.shadows.punctual {
            return;
        }
        for (cube, slot) in punctual.slots.iter().enumerate() {
            for (face, matrix) in punctual_shadow_face_matrices(slot).into_iter().enumerate() {
                let layer = cube * 6 + face;
                punctual.face_matrices[layer] = matrix;
                let raw = ShadowViewRaw {
                    index: [PUNCTUAL_FACE_VIEW, 0, 0, 0],
                    face: matrix.to_cols_array_2d(),
                };
                gpu.queue
                    .write_buffer(&punctual.face_buffers[layer], 0, bytemuck::bytes_of(&raw));
            }
        }
    }

    /// Cubes this frame draws and the lighting samples: none with the light shadows off.
    pub fn punctual_shadow_cubes(&self) -> usize {
        if self.shadows.punctual {
            self.punctual_shadow.slots.len()
        } else {
            0
        }
    }

    pub(super) fn punctual_shadow_faces(&self) -> usize {
        self.punctual_shadow_cubes() * 6
    }

    /// The cubes the lights hold now, in slot order, whether or not the light shadows are on.
    #[doc(hidden)]
    pub fn punctual_shadow_slots(&self) -> &[PunctualShadowSlot] {
        &self.punctual_shadow.slots
    }

    /// One depth pass per face of every drawn cube, each a sub-pass of the `PunctualShadows` scope.
    pub(super) fn encode_punctual_shadow_faces(&self, encoder: &mut wgpu::CommandEncoder) {
        let scope = self.gpu_timing.scope(TimedPass::PunctualShadows);
        let total = self.punctual_shadow_faces();
        let punctual = &self.punctual_shadow;
        for (cube, slot) in punctual
            .slots
            .iter()
            .take(self.punctual_shadow_cubes())
            .enumerate()
        {
            let casters = self.casters_in_reach(slot);
            for face in 0..6 {
                let layer = cube * 6 + face;
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("punctual_shadow_pass"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &punctual.layer_views[layer],
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: scope.render_writes(layer, total),
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_bind_group(1, &punctual.face_bind_groups[layer], &[]);
                pass.set_bind_group(2, &self.frame.bind_group, &[]);
                let frustum = Frustum::from_view_proj(&punctual.face_matrices[layer]);
                for &i in &casters {
                    let prim = &self.primitives[i];
                    if self.frustum_culling
                        && !frustum.intersects_aabb_as_caster(prim.aabb_min, prim.aabb_max)
                    {
                        continue;
                    }
                    self.shadow.draw_caster(&mut pass, prim);
                }
            }
        }
    }

    /// The casters whose box reaches into `slot`'s sphere, or every caster while frustum culling is off.
    fn casters_in_reach(&self, slot: &PunctualShadowSlot) -> Vec<usize> {
        self.primitives
            .iter()
            .enumerate()
            .filter(|(_, prim)| prim.casts_shadow)
            .filter(|(_, prim)| {
                !self.frustum_culling
                    || casts_into_cube(prim.aabb_min, prim.aabb_max, slot.position, slot.far)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Every cube face's depth, layer-major then row-major, as the last frame left it.
    /// Stalls on a readback, so tests and diagnostics only.
    #[doc(hidden)]
    pub fn read_punctual_shadow_map(&self, gpu: &GpuContext) -> anyhow::Result<Vec<f32>> {
        let bytes = read_texture(
            gpu,
            &self.punctual_shadow.texture,
            &TextureRegion {
                aspect: wgpu::TextureAspect::DepthOnly,
                width: PUNCTUAL_SHADOW_MAP_SIZE,
                height: PUNCTUAL_SHADOW_MAP_SIZE,
                layers: FACE_LAYERS as u32,
                texel_bytes: 4,
            },
        )?;
        Ok(bytemuck::pod_collect_to_vec(&bytes))
    }
}

/// Whether a box reaches within `far` of the light; one of unknown extent (min past max) always does.
fn casts_into_cube(min: Vec3, max: Vec3, light: Vec3, far: f32) -> bool {
    if !min.cmple(max).all() {
        return true;
    }
    light.clamp(min, max).distance_squared(light) <= far * far
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casters_outside_the_lights_reach_are_culled() {
        let light = Vec3::new(0.0, 2.0, 0.0);
        assert!(casts_into_cube(
            Vec3::new(-1.0, 0.0, -1.0),
            Vec3::ONE,
            light,
            5.0
        ));
        assert!(
            casts_into_cube(
                Vec3::new(-1.0, 1.0, -1.0),
                Vec3::new(1.0, 3.0, 1.0),
                light,
                0.5
            ),
            "a box around the light casts"
        );
        assert!(!casts_into_cube(
            Vec3::new(10.0, 0.0, 0.0),
            Vec3::new(11.0, 1.0, 1.0),
            light,
            5.0
        ));
        assert!(
            casts_into_cube(Vec3::ONE, Vec3::NEG_ONE, light, 5.0),
            "a box of unknown extent must not be culled"
        );
    }
}
