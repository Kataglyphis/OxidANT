//! Each cascade's shadow casters, culled against its box and recorded as indirect draws into a bundle replayed while that set holds.

use std::ops::Range;

use super::culling::DrawSlot;
use super::{ForwardRenderer, CASCADE_COUNT, DEPTH_FORMAT};
use crate::render::bounds::Frustum;

/// One cascade's recorded draws and the casters they draw.
struct CasterBundle {
    casters: Vec<u32>,
    bundle: wgpu::RenderBundle,
}

/// Every cascade's bundle; one whose caster set changed is recorded again.
#[derive(Default)]
pub(super) struct CasterBundles {
    entries: [Option<CasterBundle>; CASCADE_COUNT],
    /// The draw cull's args buffer the bundles read, by its generation, and the slots they draw.
    args_generation: u64,
    slots: Vec<DrawSlot>,
    primitive_slots: Vec<Range<usize>>,
}

impl CasterBundles {
    pub(super) fn bundle(&self, cascade: usize) -> Option<&wgpu::RenderBundle> {
        self.entries
            .get(cascade)?
            .as_ref()
            .map(|entry| &entry.bundle)
    }
}

impl ForwardRenderer {
    /// Records the bundle of every active cascade whose caster set, slots or args buffer moved.
    pub(super) fn ensure_shadow_caster_bundles(&mut self, device: &wgpu::Device, count: usize) {
        let casters: Vec<u32> = self
            .primitives
            .iter()
            .enumerate()
            .filter(|(_, prim)| prim.casts_shadow)
            .map(|(i, _)| i as u32)
            .collect();
        let mut bundles = self.current_caster_bundles();
        for (cascade, entry) in bundles.entries.iter_mut().enumerate().take(count) {
            let inside = self.casters_in_cascade(&casters, cascade);
            if entry
                .as_ref()
                .is_some_and(|cached| cached.casters == inside)
            {
                continue;
            }
            let bundle = self.record_caster_bundle(device, cascade, &inside);
            *entry = Some(CasterBundle {
                casters: inside,
                bundle,
            });
        }
        self.shadow.caster_bundles = Some(bundles);
    }

    /// The cached bundles while they read the current args buffer at the current slots, else none.
    fn current_caster_bundles(&mut self) -> CasterBundles {
        let culling = &self.culling;
        let generation = culling.draw_cull.generation();
        let current = |cached: &CasterBundles| {
            cached.args_generation == generation
                && cached.slots == culling.slots
                && cached.primitive_slots == culling.primitive_slots
        };
        self.shadow
            .caster_bundles
            .take()
            .filter(current)
            .unwrap_or_else(|| CasterBundles {
                args_generation: generation,
                slots: culling.slots.clone(),
                primitive_slots: culling.primitive_slots.clone(),
                ..CasterBundles::default()
            })
    }

    /// The casters in `cascade`'s box but for its near plane, or all of them while frustum culling is off.
    fn casters_in_cascade(&self, casters: &[u32], cascade: usize) -> Vec<u32> {
        if !self.frustum_culling {
            return casters.to_vec();
        }
        let frustum = Frustum::from_view_proj(&self.shadow.cascade_matrices[cascade]);
        casters
            .iter()
            .copied()
            .filter(|&i| {
                let prim = &self.primitives[i as usize];
                frustum.intersects_aabb_as_caster(prim.aabb_min, prim.aabb_max)
            })
            .collect()
    }

    /// Cascade `cascade`'s draws of `casters`; a bundle never inherits the pass's bind groups.
    fn record_caster_bundle(
        &self,
        device: &wgpu::Device,
        cascade: usize,
        casters: &[u32],
    ) -> wgpu::RenderBundle {
        let mut bundle_encoder =
            device.create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
                label: Some("shadow_casters"),
                color_formats: &[],
                depth_stencil: Some(wgpu::RenderBundleDepthStencil {
                    format: DEPTH_FORMAT,
                    depth_read_only: false,
                    stencil_read_only: true,
                }),
                sample_count: 1,
                multiview: None,
            });
        bundle_encoder.set_bind_group(1, &self.shadow.cascade_index_bind_groups[cascade], &[]);
        bundle_encoder.set_bind_group(2, &self.frame.bind_group, &[]);
        let draw_cull = &self.culling.draw_cull;
        for &i in casters {
            let slots = self
                .culling
                .slots_of(i as usize)
                .map(|(slot, draw)| (draw_cull.args_offset(cascade + 1, slot), draw));
            self.shadow.record_caster(
                &mut bundle_encoder,
                &self.primitives[i as usize],
                slots,
                draw_cull.args(),
            );
        }
        bundle_encoder.finish(&wgpu::RenderBundleDescriptor {
            label: Some("shadow_caster_bundle"),
        })
    }
}
