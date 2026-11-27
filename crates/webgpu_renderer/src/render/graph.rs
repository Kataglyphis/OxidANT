//! Render graph v2: passes declare what they read and write, and [`FrameGraph::compile`] derives the rest.
//! wgpu places the barriers itself, so the graph derives the order, culls passes nothing exported needs,
//! and packs the transient textures into as few as their lifetimes allow; [`TransientPool`] backs them.

mod pool;
#[cfg(test)]
mod tests;

pub use pool::TransientPool;

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

/// Named GPU resources the passes exchange within a frame.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Resource {
    /// Cascaded shadow depth array.
    ShadowMap,
    /// MSAA HDR colour of the forward pass, resolved into `HdrColor` by the same pass.
    HdrMsaa,
    /// MSAA depth buffer written by the forward pass.
    DepthMsaa,
    /// Single-sample depth resolved from `DepthMsaa`, written by the G-buffer pass, or the traced primary hit's; SSAO, the cull pass and the clouds read it.
    Depth,
    /// The forward path's farthest MSAA sample per pixel and its share, which the full-resolution cloud march reads.
    DepthSpread,
    /// The deferred path's normal and metallic target.
    GBufferNormal,
    /// The deferred path's albedo and occlusion target.
    GBufferAlbedo,
    /// The deferred path's roughness and emissive target.
    GBufferMaterial,
    /// HDR scene color.
    HdrColor,
    /// Cull-pass visibility; the *next* frame's draw cull reads it, so nothing here does.
    Visibility,
    /// Every draw's indirect arguments, which the draw cull writes and the scene passes draw from.
    DrawArgs,
    /// Blurred bloom contribution, at half resolution.
    Bloom,
    /// Bloom's horizontal blur, which the vertical one reads back into `Bloom`.
    BloomScratch,
    /// Blurred ambient occlusion factor, at half resolution, which the scene passes' ambient light takes.
    Ambient,
    /// The forward path's single-sample depth prepass, which SSAO reads before the forward pass shades.
    PrepassDepth,
    /// SSAO's occlusion before its blur into `Ambient`.
    AmbientRaw,
    /// Luminance histogram bins.
    Histogram,
    /// Adapted exposure value reduced from `Histogram`.
    Exposure,
    /// The tonemapped frame FXAA filters; with FXAA off the tonemap writes `Output` itself.
    Tonemapped,
    /// Final display target (swapchain frame or readback texture).
    Output,
    /// The clouds' 3D noise, generated on the first frame with clouds and kept.
    NoiseVolume,
    /// Premultiplied cloud colour and coverage, marched to `Depth`.
    CloudColor,
    /// The half-resolution march the upsample reads into `CloudColor`.
    CloudHalf,
    /// The traced modes' per-pixel running mean.
    Accumulation,
}

/// A declared pass: its name and the resources it reads and writes.
#[derive(Clone, Debug)]
pub struct PassDesc<'a> {
    pub name: &'static str,
    pub reads: &'a [Resource],
    pub writes: &'a [Resource],
}

/// What a transient texture is made as; two of equal description are interchangeable.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextureDesc {
    pub size: (u32, u32),
    pub format: wgpu::TextureFormat,
    pub sample_count: u32,
    pub usage: wgpu::TextureUsages,
}

impl TextureDesc {
    /// The texture's size in memory, as its texels add up; drivers may pad it.
    pub fn bytes(&self) -> u64 {
        let texel = self
            .format
            .block_copy_size(None)
            .or_else(|| {
                self.format
                    .block_copy_size(Some(wgpu::TextureAspect::DepthOnly))
            })
            .unwrap_or(0);
        u64::from(self.size.0)
            * u64::from(self.size.1)
            * u64::from(self.sample_count)
            * u64::from(texel)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum GraphError {
    /// A pass reads a resource that no pass writes and the frame does not import.
    UndefinedRead {
        pass: &'static str,
        resource: Resource,
    },
    /// Two passes write the same resource in one frame.
    DuplicateWrite {
        pass: &'static str,
        resource: Resource,
    },
    /// The passes' reads and writes order them in a circle; these are the passes on or behind it.
    Cycle { passes: Vec<&'static str> },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::UndefinedRead { pass, resource } => write!(
                f,
                "pass '{pass}' reads {resource:?}, which no pass writes and the frame does not import"
            ),
            GraphError::DuplicateWrite { pass, resource } => {
                write!(f, "pass '{pass}' writes {resource:?} twice in one frame")
            }
            GraphError::Cycle { passes } => write!(f, "passes {passes:?} depend on each other"),
        }
    }
}

impl std::error::Error for GraphError {}

/// One frame's passes and resources; the order passes were added in breaks ties in the derived order.
#[derive(Default)]
pub struct FrameGraph<'a> {
    passes: Vec<PassDesc<'a>>,
    transients: Vec<(Resource, TextureDesc)>,
    imported: HashSet<Resource>,
    exported: HashSet<Resource>,
}

/// Positions in [`CompiledGraph::order`] of a transient's first and last use, both inclusive.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Lifetime {
    pub first: usize,
    pub last: usize,
}

impl Lifetime {
    pub fn overlaps(self, other: Lifetime) -> bool {
        self.first <= other.last && other.first <= self.last
    }
}

/// A transient the compiled frame uses: its description, its lifetime and the slot that backs it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Transient {
    pub resource: Resource,
    pub desc: TextureDesc,
    pub lifetime: Lifetime,
    pub slot: usize,
}

/// What [`FrameGraph::compile`] derives: the passes to record, in order, and the textures behind the transients.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledGraph {
    /// Indices of the passes that run, in the order to record them; a culled pass is absent.
    pub order: Vec<usize>,
    /// One texture per slot; the transients sharing a slot never live at once.
    pub slots: Vec<TextureDesc>,
    /// Every transient a running pass uses, by first use.
    pub transients: Vec<Transient>,
}

impl CompiledGraph {
    /// The slot backing `resource`, if it is a transient this frame uses.
    pub fn slot(&self, resource: Resource) -> Option<usize> {
        self.transients
            .iter()
            .find(|transient| transient.resource == resource)
            .map(|transient| transient.slot)
    }

    /// Bytes of every slot's texture: what the pool holds for this frame.
    pub fn transient_bytes(&self) -> u64 {
        self.slots.iter().map(TextureDesc::bytes).sum()
    }

    /// Bytes the same transients would take with a texture each.
    pub fn unaliased_bytes(&self) -> u64 {
        self.transients.iter().map(|t| t.desc.bytes()).sum()
    }
}

impl<'a> FrameGraph<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a pass and returns its index, which [`CompiledGraph::order`] refers to.
    pub fn add_pass(&mut self, pass: PassDesc<'a>) -> usize {
        self.passes.push(pass);
        self.passes.len() - 1
    }

    /// A texture the pool backs for this frame only, from its first use to its last.
    pub fn transient(&mut self, resource: Resource, desc: TextureDesc) {
        self.transients.push((resource, desc));
    }

    /// A resource made outside the frame, so a pass may read it although no pass writes it.
    pub fn import(&mut self, resource: Resource) {
        self.imported.insert(resource);
    }

    /// A resource read after the frame: its writers are never culled, and a transient one lives to the frame's end.
    pub fn export(&mut self, resource: Resource) {
        self.exported.insert(resource);
    }

    pub fn passes(&self) -> &[PassDesc<'a>] {
        &self.passes
    }

    /// Orders the passes by their reads and writes, culls those nothing exported needs, and assigns the transients slots.
    pub fn compile(&self) -> Result<CompiledGraph, GraphError> {
        let writers = self.writers()?;
        let successors = self.successors(&writers)?;
        let live = self.live(&writers);
        let order: Vec<usize> = self
            .topological(&successors)?
            .into_iter()
            .filter(|&pass| live[pass])
            .collect();
        let (slots, transients) = assign_slots(self.lifetimes(&order));
        Ok(CompiledGraph {
            order,
            slots,
            transients,
        })
    }

    /// The one pass that writes each resource.
    fn writers(&self) -> Result<HashMap<Resource, usize>, GraphError> {
        let mut writers = HashMap::new();
        for (index, pass) in self.passes.iter().enumerate() {
            for &resource in pass.writes {
                if writers.insert(resource, index).is_some() {
                    return Err(GraphError::DuplicateWrite {
                        pass: pass.name,
                        resource,
                    });
                }
            }
        }
        Ok(writers)
    }

    /// Each pass's readers: an edge from a resource's writer to every other pass that reads it.
    fn successors(
        &self,
        writers: &HashMap<Resource, usize>,
    ) -> Result<Vec<Vec<usize>>, GraphError> {
        let mut successors = vec![Vec::new(); self.passes.len()];
        for (reader, pass) in self.passes.iter().enumerate() {
            for resource in pass.reads {
                match writers.get(resource) {
                    Some(&writer) if writer != reader => successors[writer].push(reader),
                    Some(_) => {}
                    None if self.imported.contains(resource) => {}
                    None => {
                        return Err(GraphError::UndefinedRead {
                            pass: pass.name,
                            resource: *resource,
                        })
                    }
                }
            }
        }
        Ok(successors)
    }

    /// Kahn's algorithm, always taking the earliest-added ready pass, so an already ordered frame keeps its order.
    fn topological(&self, successors: &[Vec<usize>]) -> Result<Vec<usize>, GraphError> {
        let mut incoming = vec![0usize; self.passes.len()];
        for &reader in successors.iter().flatten() {
            incoming[reader] += 1;
        }
        let mut ready: BinaryHeap<Reverse<usize>> = (0..self.passes.len())
            .filter(|&pass| incoming[pass] == 0)
            .map(Reverse)
            .collect();
        let mut order = Vec::with_capacity(self.passes.len());
        while let Some(Reverse(pass)) = ready.pop() {
            order.push(pass);
            for &reader in &successors[pass] {
                incoming[reader] -= 1;
                if incoming[reader] == 0 {
                    ready.push(Reverse(reader));
                }
            }
        }
        if order.len() < self.passes.len() {
            let passes = (0..self.passes.len())
                .filter(|&pass| incoming[pass] > 0)
                .map(|pass| self.passes[pass].name)
                .collect();
            return Err(GraphError::Cycle { passes });
        }
        Ok(order)
    }

    /// The passes an exported resource depends on, through the writers of everything they read.
    fn live(&self, writers: &HashMap<Resource, usize>) -> Vec<bool> {
        let mut live = vec![false; self.passes.len()];
        let mut pending: Vec<usize> = self
            .exported
            .iter()
            .filter_map(|resource| writers.get(resource).copied())
            .collect();
        while let Some(pass) = pending.pop() {
            if std::mem::replace(&mut live[pass], true) {
                continue;
            }
            let reads = self.passes[pass].reads.iter();
            pending.extend(reads.filter_map(|resource| writers.get(resource).copied()));
        }
        live
    }

    /// Each transient a running pass uses, with the positions of its first and last use, by first use.
    fn lifetimes(&self, order: &[usize]) -> Vec<(Resource, TextureDesc, Lifetime)> {
        let frame_end = order.len().saturating_sub(1);
        let mut used = Vec::new();
        for &(resource, desc) in &self.transients {
            let uses = order.iter().enumerate().filter(|&(_, &pass)| {
                let pass = &self.passes[pass];
                pass.reads.contains(&resource) || pass.writes.contains(&resource)
            });
            let positions: Vec<usize> = uses.map(|(position, _)| position).collect();
            let (Some(&first), Some(&last)) = (positions.first(), positions.last()) else {
                continue;
            };
            let last = if self.exported.contains(&resource) {
                frame_end
            } else {
                last
            };
            used.push((resource, desc, Lifetime { first, last }));
        }
        // Stable, so transients first used by the same pass keep their declaration order.
        used.sort_by_key(|&(_, _, lifetime)| lifetime.first);
        used
    }
}

/// Greedy interval packing per description: each transient takes the first slot free before its first use.
fn assign_slots(
    lifetimes: Vec<(Resource, TextureDesc, Lifetime)>,
) -> (Vec<TextureDesc>, Vec<Transient>) {
    let mut slots: Vec<(TextureDesc, usize)> = Vec::new();
    let mut transients = Vec::with_capacity(lifetimes.len());
    for (resource, desc, lifetime) in lifetimes {
        let free = slots
            .iter()
            .position(|&(slot_desc, busy_until)| slot_desc == desc && busy_until < lifetime.first);
        let slot = match free {
            Some(slot) => {
                slots[slot].1 = lifetime.last;
                slot
            }
            None => {
                slots.push((desc, lifetime.last));
                slots.len() - 1
            }
        };
        transients.push(Transient {
            resource,
            desc,
            lifetime,
            slot,
        });
    }
    (
        slots.into_iter().map(|(desc, _)| desc).collect(),
        transients,
    )
}
