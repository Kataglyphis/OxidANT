//! The textures behind a compiled graph's slots, kept across frames while the frame's shape holds.

use super::{CompiledGraph, Resource, TextureDesc};
use crate::render::texture::create_2d_texture;

/// One slot's texture and the id that tells a reused texture from a new one.
struct Pooled {
    desc: TextureDesc,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    id: u64,
}

/// Transient textures by description: a frame takes the ones it needs and the rest are dropped.
#[derive(Default)]
pub struct TransientPool {
    slots: Vec<Pooled>,
    /// Each transient of the last acquired frame, with the id of the texture backing it.
    bound: Vec<(Resource, usize, u64)>,
    next_id: u64,
}

impl TransientPool {
    /// Backs every slot of `graph`, reusing textures of equal description; true when any transient's texture changed.
    pub fn acquire(&mut self, device: &wgpu::Device, graph: &CompiledGraph) -> bool {
        let mut spare: Vec<Option<Pooled>> = std::mem::take(&mut self.slots)
            .into_iter()
            .map(Some)
            .collect();
        for &desc in &graph.slots {
            let reused = spare
                .iter_mut()
                .find(|pooled| pooled.as_ref().is_some_and(|p| p.desc == desc))
                .and_then(Option::take);
            let pooled = reused.unwrap_or_else(|| self.create(device, desc));
            self.slots.push(pooled);
        }
        let bound: Vec<(Resource, usize, u64)> = graph
            .transients
            .iter()
            .map(|t| (t.resource, t.slot, self.slots[t.slot].id))
            .collect();
        let changed = bound != self.bound;
        self.bound = bound;
        changed
    }

    fn create(&mut self, device: &wgpu::Device, desc: TextureDesc) -> Pooled {
        let (width, height) = desc.size;
        let texture = create_2d_texture(
            device,
            Some("graph_transient"),
            width,
            height,
            desc.format,
            desc.usage,
            1,
            desc.sample_count,
        );
        self.next_id += 1;
        Pooled {
            desc,
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            texture,
            id: self.next_id,
        }
    }

    fn pooled(&self, resource: Resource) -> Option<&Pooled> {
        let &(_, slot, _) = self.bound.iter().find(|(r, _, _)| *r == resource)?;
        self.slots.get(slot)
    }

    /// The texture backing `resource` in the last acquired frame.
    pub fn texture(&self, resource: Resource) -> Option<&wgpu::Texture> {
        self.pooled(resource).map(|pooled| &pooled.texture)
    }

    /// The view of the texture backing `resource` in the last acquired frame.
    pub fn view(&self, resource: Resource) -> Option<&wgpu::TextureView> {
        self.pooled(resource).map(|pooled| &pooled.view)
    }

    /// Bytes of every texture the pool holds.
    pub fn bytes(&self) -> u64 {
        self.slots.iter().map(|pooled| pooled.desc.bytes()).sum()
    }
}
