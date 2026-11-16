//! Building a [`TraceScene`] from a [`CpuScene`]: in one call on a worker thread, or in budgeted steps a frame loop spreads.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

use glam::{Mat4, Vec3A};

use super::bvh::{Aabb, Bvh, BvhBuilder, BvhParams};
use super::geometry::{self, Gathered, MeshBuild};
use super::layers::{self, LayerArray};
use super::material::MaterialTable;
use super::scene::TraceScene;
use crate::scene::CpuScene;

/// Where a build is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TraceStage {
    #[default]
    Gathering,
    Bvh,
    Textures,
    Assembling,
    /// Not a build stage: the finished scene going to the GPU over a few frames.
    Uploading,
}

impl TraceStage {
    const ALL: [Self; 4] = [Self::Gathering, Self::Bvh, Self::Textures, Self::Assembling];

    pub fn label(self) -> &'static str {
        match self {
            Self::Gathering => "gathering meshes",
            Self::Bvh => "building BVHs",
            Self::Textures => "resampling textures",
            Self::Assembling => "assembling",
            Self::Uploading => "uploading to the GPU",
        }
    }
}

/// A build's stage and how far it is, as the host shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TraceProgress {
    pub stage: TraceStage,
    /// Done over all of the stage's work.
    pub fraction: f32,
}

/// The progress a worker thread publishes and the frame reads, and the frame's request to stop.
#[derive(Debug, Default)]
pub struct BuildProgress {
    stage: AtomicU32,
    done: AtomicUsize,
    total: AtomicUsize,
    cancelled: AtomicBool,
}

impl BuildProgress {
    /// Asks the build to stop at its next stage boundary.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    fn begin(&self, stage: TraceStage, total: usize) {
        self.done.store(0, Ordering::Relaxed);
        self.total.store(total.max(1), Ordering::Relaxed);
        self.stage.store(stage as u32, Ordering::Relaxed);
    }

    fn add(&self, work: usize) {
        self.done.fetch_add(work, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> TraceProgress {
        let stage = TraceStage::ALL[(self.stage.load(Ordering::Relaxed) as usize).min(3)];
        let done = self.done.load(Ordering::Relaxed) as f32;
        let total = self.total.load(Ordering::Relaxed).max(1) as f32;
        TraceProgress {
            stage,
            fraction: (done / total).min(1.0),
        }
    }
}

/// What a build may spend: texture layer side and memory, layers per array, worker threads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceBuildOptions {
    pub max_layer_size: u32,
    /// Bytes each texture array may take, mips included.
    pub layer_budget: u64,
    pub max_layers: usize,
    pub threads: usize,
    /// Bytes the first upload writes a frame, at least one piece.
    pub upload_bytes_per_frame: u64,
    /// Keeps the triangles, vertices and texels on the CPU after the upload, for CPU queries of the uploaded scene.
    pub keep_cpu_geometry: bool,
}

impl Default for TraceBuildOptions {
    /// Natively 1024-texel layers in 512 MiB per array on every core; in the browser 512 in 128 MiB on one thread.
    fn default() -> Self {
        let web = cfg!(target_arch = "wasm32");
        Self {
            max_layer_size: if web { 512 } else { 1024 },
            layer_budget: if web { 128 << 20 } else { 512 << 20 },
            max_layers: wgpu::Limits::default().max_texture_array_layers as usize,
            threads: if web {
                1
            } else {
                std::thread::available_parallelism().map_or(1, usize::from)
            },
            upload_bytes_per_frame: super::upload::UPLOAD_BYTES_PER_FRAME,
            keep_cpu_geometry: false,
        }
    }
}

/// Seconds per stage where the platform has a clock (not the browser's wasm32).
#[derive(Default)]
struct StageClock {
    #[cfg(not(target_arch = "wasm32"))]
    current: Option<(TraceStage, std::time::Instant)>,
    seconds: [f32; 4],
}

impl StageClock {
    fn enter(&mut self, stage: TraceStage) {
        self.stop();
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.current = Some((stage, std::time::Instant::now()));
        }
        let _ = stage;
    }

    fn stop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((stage, start)) = self.current.take() {
            self.seconds[stage as usize] += start.elapsed().as_secs_f32();
        }
    }

    fn finish(&mut self) -> Option<[f32; 4]> {
        self.stop();
        (!cfg!(target_arch = "wasm32")).then_some(self.seconds)
    }
}

/// Per-triangle boxes of one mesh, the BVH builder's input.
fn triangle_boxes(mesh: &MeshBuild) -> Vec<Aabb> {
    mesh.triangles
        .iter()
        .map(|tri| {
            let corner = |k: usize| Vec3A::from_slice(&mesh.positions[tri[k] as usize][..3]);
            Aabb::from_points(&[corner(0), corner(1), corner(2)])
        })
        .collect()
}

/// Which work comes next.
enum Stage {
    Gather,
    Bvh {
        mesh: usize,
        builder: Option<BvhBuilder>,
    },
    Textures {
        layer: usize,
    },
    Assemble,
    Done,
}

/// A build as a state machine, so the browser can run it a budget per frame; `run` finishes it in one call.
pub struct TraceSceneBuilder {
    source: Arc<CpuScene>,
    explicit: Vec<Vec<Mat4>>,
    options: TraceBuildOptions,
    progress: Arc<BuildProgress>,
    stage: Stage,
    clock: StageClock,
    table: MaterialTable,
    gathered: Gathered,
    bvhs: Vec<Bvh>,
    layers: [LayerArray; 2],
}

impl TraceSceneBuilder {
    /// A build of `source`; `explicit[i]`, when not empty, instances primitive `i` as `set_instances` does.
    pub fn new(
        source: Arc<CpuScene>,
        explicit: Vec<Vec<Mat4>>,
        options: TraceBuildOptions,
    ) -> Self {
        Self {
            source,
            explicit,
            options,
            progress: Arc::default(),
            stage: Stage::Gather,
            clock: StageClock::default(),
            table: MaterialTable::default(),
            gathered: Gathered::default(),
            bvhs: Vec::new(),
            layers: [LayerArray::default(), LayerArray::default()],
        }
    }

    /// Shared with whoever shows the progress.
    pub fn progress(&self) -> Arc<BuildProgress> {
        Arc::clone(&self.progress)
    }

    /// Works until about `budget` units (triangles binned, texels resampled) are spent; the scene once it is done.
    pub fn step(&mut self, budget: usize) -> Option<TraceScene> {
        let mut spent = 0usize;
        while spent < budget.max(1) {
            // An unbounded step stays unbounded, so `run` takes the threaded paths.
            let left = if budget == usize::MAX {
                budget
            } else {
                budget - spent
            };
            match std::mem::replace(&mut self.stage, Stage::Done) {
                Stage::Gather => spent += self.gather(),
                Stage::Bvh { mesh, builder } => spent += self.bvh_step(mesh, builder, left),
                Stage::Textures { layer } => spent += self.texture_step(layer),
                Stage::Assemble => return Some(self.assemble()),
                Stage::Done => return None,
            }
        }
        None
    }

    /// Finishes the build on this thread, natively with each big BVH's subtrees over the workers; `None` once cancelled.
    pub fn run(mut self) -> Option<TraceScene> {
        while !self.progress.cancelled() {
            if let Some(scene) = self.step(usize::MAX) {
                return Some(scene);
            }
        }
        None
    }

    /// Starts `stage` with `total` units of work, and its clock.
    fn enter(&mut self, stage: TraceStage, total: usize) {
        self.progress.begin(stage, total);
        self.clock.enter(stage);
    }

    fn gather(&mut self) -> usize {
        self.enter(TraceStage::Gathering, 1);
        self.table = MaterialTable::build(&self.source, self.options.max_layers);
        self.gathered = geometry::gather(&self.source, &self.table, &self.explicit);
        let triangles: usize = self.gathered.meshes.iter().map(|m| m.triangles.len()).sum();
        self.enter(TraceStage::Bvh, triangles);
        self.stage = Stage::Bvh {
            mesh: 0,
            builder: None,
        };
        self.source.triangle_count().max(1)
    }

    fn bvh_step(&mut self, mesh: usize, builder: Option<BvhBuilder>, budget: usize) -> usize {
        let Some(build) = self.gathered.meshes.get(mesh) else {
            self.begin_textures();
            return 1;
        };
        let count = build.triangles.len();
        let next = Stage::Bvh {
            mesh: mesh + 1,
            builder: None,
        };
        if budget == usize::MAX && builder.is_none() {
            self.bvhs.push(self.build_whole(triangle_boxes(build)));
            self.stage = next;
            return count;
        }
        let mut builder =
            builder.unwrap_or_else(|| BvhBuilder::new(triangle_boxes(build), BvhParams::BLAS));
        let before = builder.progress();
        let done = builder.step(budget);
        let advanced = ((builder.progress() - before) * count as f32) as usize;
        self.progress.add(advanced);
        if done {
            self.bvhs.push(builder.finish());
            self.stage = next;
        } else {
            self.stage = Stage::Bvh {
                mesh,
                builder: Some(builder),
            };
        }
        budget.min(count).max(1)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn build_whole(&self, boxes: Vec<Aabb>) -> Bvh {
        super::bvh::build_parallel(
            boxes,
            BvhParams::BLAS,
            self.options.threads,
            &self.progress.done,
        )
    }

    #[cfg(target_arch = "wasm32")]
    fn build_whole(&self, boxes: Vec<Aabb>) -> Bvh {
        let count = boxes.len();
        let bvh = super::bvh::build(boxes, BvhParams::BLAS);
        self.progress.add(count);
        bvh
    }

    fn begin_textures(&mut self) {
        let sizes = [&self.table.srgb_layers, &self.table.linear_layers].map(|sources| {
            layers::layer_size(
                sources,
                self.options.max_layer_size,
                self.options.layer_budget,
            )
        });
        for (array, size) in self.layers.iter_mut().zip(sizes) {
            array.size = size;
        }
        let total = self.table.srgb_layers.len() + self.table.linear_layers.len();
        self.enter(TraceStage::Textures, total);
        self.stage = Stage::Textures { layer: 0 };
    }

    /// Resamples one layer, sRGB ones first; natively every layer at once over the worker threads.
    fn texture_step(&mut self, layer: usize) -> usize {
        let srgb_count = self.table.srgb_layers.len();
        let total = srgb_count + self.table.linear_layers.len();
        if layer >= total {
            for array in &mut self.layers {
                if array.layers.is_empty() {
                    *array = layers::placeholder();
                }
            }
            self.enter(TraceStage::Assembling, 1);
            self.stage = Stage::Assemble;
            return 1;
        }
        if layer == 0 && self.options.threads > 1 && !cfg!(target_arch = "wasm32") {
            self.resample_all();
            self.stage = Stage::Textures { layer: total };
            return total;
        }
        let (srgb, index) = if layer < srgb_count {
            (true, layer)
        } else {
            (false, layer - srgb_count)
        };
        let sources = if srgb {
            &self.table.srgb_layers
        } else {
            &self.table.linear_layers
        };
        let array = &mut self.layers[usize::from(!srgb)];
        let texture = &sources[index];
        array
            .layers
            .push(layers::resample_layer(texture, srgb, array.size));
        self.progress.add(1);
        self.stage = Stage::Textures { layer: layer + 1 };
        (texture.width * texture.height) as usize
    }

    /// Every layer of both arrays, each on whichever worker is free.
    fn resample_all(&mut self) {
        let jobs: Vec<(bool, &Arc<crate::scene::CpuTexture>)> = self
            .table
            .srgb_layers
            .iter()
            .map(|t| (true, t))
            .chain(self.table.linear_layers.iter().map(|t| (false, t)))
            .collect();
        let sizes = [self.layers[0].size, self.layers[1].size];
        let next = AtomicUsize::new(0);
        let results: Vec<std::sync::Mutex<Vec<Vec<u8>>>> =
            jobs.iter().map(|_| Default::default()).collect();
        std::thread::scope(|scope| {
            for _ in 0..self.options.threads.min(jobs.len()) {
                scope.spawn(|| loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&(srgb, texture)) = jobs.get(index) else {
                        return;
                    };
                    let levels = layers::resample_layer(texture, srgb, sizes[usize::from(!srgb)]);
                    if let Ok(mut slot) = results[index].lock() {
                        *slot = levels;
                    }
                    self.progress.add(1);
                });
            }
        });
        for ((srgb, _), levels) in jobs.iter().zip(results) {
            let levels = levels.into_inner().unwrap_or_default();
            self.layers[usize::from(!srgb)].layers.push(levels);
        }
    }

    fn assemble(&mut self) -> TraceScene {
        let table = std::mem::take(&mut self.table);
        let gathered = std::mem::take(&mut self.gathered);
        let bvhs = std::mem::take(&mut self.bvhs);
        let layers = std::mem::take(&mut self.layers);
        let mut scene = TraceScene::assemble(table, gathered, bvhs, layers);
        scene.measure(None);
        scene.stats.stage_seconds = self.clock.finish();
        self.progress.add(1);
        scene
    }
}
