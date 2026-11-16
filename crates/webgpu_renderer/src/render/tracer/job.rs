//! The traced scene, built the first time a traced mode wants it: on a worker thread natively, a budget per frame in the browser.
//! The finished scene then uploads a byte budget per frame, and drops the CPU arrays only the upload read.

use std::collections::BTreeSet;
use std::sync::Arc;

use glam::Mat4;

use super::build::{
    BuildProgress, TraceBuildOptions, TraceProgress, TraceSceneBuilder, TraceStage,
};
use super::gpu::TraceGpuScene;
use super::limits::{TraceBufferSizes, TraceCounts};
use super::scene::{InstanceChange, TraceScene, TraceStats};
use super::upload::TraceUpload;
use crate::context::GpuContext;
use crate::scene::CpuScene;

/// Work units (triangles binned, texels resampled) one browser frame spends on a build.
pub const WEB_STEP_BUDGET: usize = 200_000;

/// The C++ editor's three render modes; until a traced mode's scene is ready, the frame rasters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RenderMode {
    #[default]
    Raster,
    RayTraced,
    PathTraced,
}

impl RenderMode {
    pub const ALL: [Self; 3] = [Self::Raster, Self::RayTraced, Self::PathTraced];

    pub fn is_traced(self) -> bool {
        self != Self::Raster
    }

    /// The position in [`RenderMode::ALL`], the editor's segmented control index.
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
    }
}

/// Where the traced scene is, for the editor and the host.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum TraceStatus {
    /// Not asked for yet, or no scene to trace.
    #[default]
    Idle,
    Building(TraceProgress),
    Ready(TraceStats),
    Failed(String),
}

impl TraceStatus {
    /// One line: `Building BVHs · 42 %`, `Ready · 5.9 M triangles in 2 instances, built in 1.8 s`.
    pub fn text(&self) -> String {
        match self {
            Self::Idle => "Built the first time a traced mode is chosen".to_owned(),
            Self::Building(progress) => {
                format!(
                    "Tracer: {}  ·  {:.0} %",
                    progress.stage.label(),
                    progress.fraction * 100.0
                )
            }
            Self::Ready(stats) => {
                let built = stats
                    .build_seconds
                    .map_or(String::new(), |s| format!(", built in {s:.1} s"));
                format!(
                    "Tracer ready  ·  {:.2} M triangles in {} instances{built}",
                    stats.triangles as f64 / 1e6,
                    stats.instances
                )
            }
            Self::Failed(why) => format!("Tracer unavailable: {why}"),
        }
    }
}

/// A build in flight.
struct Job {
    progress: Arc<BuildProgress>,
    #[cfg(not(target_arch = "wasm32"))]
    handle: Option<std::thread::JoinHandle<Option<TraceScene>>>,
    #[cfg(target_arch = "wasm32")]
    builder: TraceSceneBuilder,
}

impl Job {
    #[cfg(not(target_arch = "wasm32"))]
    fn start(
        source: Arc<CpuScene>,
        explicit: Vec<Vec<Mat4>>,
        options: TraceBuildOptions,
    ) -> Result<Self, String> {
        let builder = TraceSceneBuilder::new(source, explicit, options);
        let progress = builder.progress();
        let handle = std::thread::Builder::new()
            .name("tracer-build".to_owned())
            .spawn(move || {
                let start = std::time::Instant::now();
                let mut scene = builder.run()?;
                scene.stats.build_seconds = Some(start.elapsed().as_secs_f64());
                Some(scene)
            })
            .map_err(|error| format!("could not start the build thread: {error}"))?;
        Ok(Self {
            progress,
            handle: Some(handle),
        })
    }

    #[cfg(target_arch = "wasm32")]
    fn start(
        source: Arc<CpuScene>,
        explicit: Vec<Vec<Mat4>>,
        options: TraceBuildOptions,
    ) -> Result<Self, String> {
        let builder = TraceSceneBuilder::new(source, explicit, options);
        Ok(Self {
            progress: builder.progress(),
            builder,
        })
    }

    /// The finished scene, or `None` while it builds.
    #[cfg(not(target_arch = "wasm32"))]
    fn poll(&mut self) -> Option<Result<TraceScene, String>> {
        if self.handle.as_ref()?.is_finished() {
            self.join()
        } else {
            None
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn poll(&mut self) -> Option<Result<TraceScene, String>> {
        self.builder.step(WEB_STEP_BUDGET).map(Ok)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn join(&mut self) -> Option<Result<TraceScene, String>> {
        Some(match self.handle.take()?.join() {
            Ok(Some(scene)) => Ok(scene),
            Ok(None) => Err("the build was cancelled".to_owned()),
            Err(_) => Err("the build thread panicked".to_owned()),
        })
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.progress.cancel();
    }
}

enum State {
    Idle,
    // Boxed: the browser's job holds the whole scene builder inline.
    Building(Box<Job>),
    /// Built, and going to the GPU a budget a frame.
    Uploading(Box<(TraceScene, TraceUpload)>),
    Ready(Box<(TraceScene, TraceGpuScene)>),
    Failed(String),
}

/// The lazily built traced scene of whatever [`Tracer::set_source`] last handed it.
pub struct Tracer {
    source: Option<Arc<CpuScene>>,
    state: State,
    pub options: TraceBuildOptions,
    /// Counts every change of the uploaded scene, so a path tracer's history knows when it went stale.
    generation: u64,
    /// The model transform every instance sits under, kept across builds.
    root: Mat4,
    /// `set_instances`' transforms by scene primitive, which the next build instances.
    explicit: Vec<Vec<Mat4>>,
    /// Primitives whose `set_instances` the built scene could not follow, each warned once.
    stale_instances: BTreeSet<usize>,
}

impl Default for Tracer {
    fn default() -> Self {
        Self {
            source: None,
            state: State::Idle,
            options: TraceBuildOptions::default(),
            generation: 0,
            root: Mat4::IDENTITY,
            explicit: Vec::new(),
            stale_instances: BTreeSet::new(),
        }
    }
}

impl Tracer {
    /// The scene to trace from now on; a build of the previous one is cancelled and its result dropped.
    pub fn set_source(&mut self, scene: Arc<CpuScene>) {
        self.source = Some(scene);
        self.state = State::Idle;
    }

    /// Forgets the scene, everything built from it and where it was placed.
    pub fn reset(&mut self) {
        self.source = None;
        self.state = State::Idle;
        self.root = Mat4::IDENTITY;
        self.explicit.clear();
        self.stale_instances.clear();
    }

    /// Starts the build unless one ran or runs; without a source there is nothing to build.
    pub fn request(&mut self) {
        let (State::Idle, Some(source)) = (&self.state, &self.source) else {
            return;
        };
        let explicit = self.explicit.clone();
        self.state = match Job::start(Arc::clone(source), explicit, self.options) {
            Ok(job) => State::Building(Box::new(job)),
            Err(why) => State::Failed(why),
        };
    }

    /// Once per frame: starts the build when `wanted`, starts the upload of a finished one and continues it.
    pub fn advance(&mut self, gpu: &GpuContext, wanted: bool) {
        if wanted {
            self.request();
        }
        if let State::Building(job) = &mut self.state {
            if let Some(result) = job.poll() {
                self.start_upload(gpu, result);
            }
        }
        self.continue_upload(gpu, self.options.upload_bytes_per_frame);
    }

    fn start_upload(&mut self, gpu: &GpuContext, result: Result<TraceScene, String>) {
        self.stale_instances.clear();
        self.generation += 1;
        let started = result.and_then(|scene| {
            let upload = TraceUpload::new(gpu, &scene).map_err(|error| error.to_string())?;
            Ok((scene, upload))
        });
        self.state = match started {
            Ok(uploading) => State::Uploading(Box::new(uploading)),
            Err(why) => self.fail(why),
        };
    }

    /// Writes up to `budget` bytes of the upload in flight; the last write makes the scene ready.
    fn continue_upload(&mut self, gpu: &GpuContext, budget: u64) {
        let State::Uploading(uploading) = &mut self.state else {
            return;
        };
        let (scene, upload) = &mut **uploading;
        if !upload.step(gpu, scene, budget) {
            return;
        }
        let State::Uploading(uploading) = std::mem::replace(&mut self.state, State::Idle) else {
            unreachable!("the state was uploading a moment ago");
        };
        let (mut scene, upload) = *uploading;
        // The root may have moved while the upload ran.
        if self.root != Mat4::IDENTITY {
            scene.set_root_transform(self.root);
        }
        let uploaded = upload.finish(gpu, &scene);
        self.state = State::Ready(Box::new(self.settle(scene, uploaded)));
    }

    /// The uploaded scene with what only the upload read dropped, and the source it was built from.
    fn settle(
        &mut self,
        mut scene: TraceScene,
        uploaded: TraceGpuScene,
    ) -> (TraceScene, TraceGpuScene) {
        // The CPU scene is not read again; its memory goes back now.
        self.source = None;
        if self.options.keep_cpu_geometry {
            // The CPU alpha test reads level 0 of the sRGB layers; the GPU holds the rest.
            scene.srgb.keep_base_level();
            scene.linear.layers.clear();
        } else {
            scene.release_geometry();
        }
        log_upload(&scene.stats);
        (scene, uploaded)
    }

    fn fail(&mut self, why: String) -> State {
        self.source = None;
        log::warn!("tracer: {why}");
        State::Failed(why)
    }

    /// Blocks until the requested build is done and uploaded, at most `timeout`; true when it is ready.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn finish_build(&mut self, gpu: &GpuContext, timeout: std::time::Duration) -> bool {
        self.request();
        let deadline = std::time::Instant::now() + timeout;
        while let State::Building(job) = &mut self.state {
            if let Some(result) = job.poll() {
                self.start_upload(gpu, result);
            } else if std::time::Instant::now() >= deadline {
                return false;
            } else {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        self.continue_upload(gpu, u64::MAX);
        matches!(self.state, State::Ready(_))
    }

    pub fn status(&self) -> TraceStatus {
        match &self.state {
            State::Idle => TraceStatus::Idle,
            State::Building(job) => TraceStatus::Building(job.progress.snapshot()),
            State::Uploading(uploading) => TraceStatus::Building(TraceProgress {
                stage: TraceStage::Uploading,
                fraction: uploading.1.fraction(),
            }),
            State::Ready(ready) => TraceStatus::Ready(ready.0.stats.clone()),
            State::Failed(why) => TraceStatus::Failed(why.clone()),
        }
    }

    /// The uploaded scene a traced kernel binds as its group [`super::TRACE_SCENE_GROUP`].
    pub fn gpu_scene(&self) -> Option<&TraceGpuScene> {
        match &self.state {
            State::Ready(ready) => Some(&ready.1),
            _ => None,
        }
    }

    /// The same scene on the CPU, for picking and tests.
    pub fn cpu_scene(&self) -> Option<&TraceScene> {
        match &self.state {
            State::Ready(ready) => Some(&ready.0),
            _ => None,
        }
    }

    /// Moves every instance under `root` (the model transform): the top level rebuilds, the meshes stay; a later build keeps it.
    pub fn set_root_transform(&mut self, gpu: &GpuContext, root: Mat4) {
        if root == self.root {
            return;
        }
        self.root = root;
        self.move_instances(gpu, |scene| scene.set_root_transform(root));
    }

    /// The transform every instance sits under now.
    pub fn root_transform(&self) -> Mat4 {
        self.root
    }

    /// Instances `primitive` by `transforms` as `ForwardRenderer::set_instances`: a build restarts, a built scene re-instances.
    pub fn set_instances(&mut self, gpu: &GpuContext, primitive: usize, transforms: &[Mat4]) {
        if self.explicit.len() <= primitive {
            self.explicit.resize(primitive + 1, Vec::new());
        }
        self.explicit[primitive] = transforms.to_vec();
        let ready = match &mut self.state {
            // The source is kept until the upload ends, so the build restarts with these instances.
            State::Building(_) | State::Uploading(_) => {
                self.state = State::Idle;
                return;
            }
            State::Ready(ready) => ready,
            State::Idle | State::Failed(_) => return,
        };
        let (scene, uploaded) = &mut **ready;
        let written = match scene.set_instance_extras(primitive as u32, transforms) {
            InstanceChange::Moved => {
                uploaded.write_instances(gpu, scene);
                Ok(())
            }
            InstanceChange::Recounted => {
                remeasure(scene);
                uploaded
                    .write_records(gpu, scene)
                    .map_err(|why| why.to_string())
            }
            InstanceChange::Refused => Err("it is merged into the world-space mesh".to_owned()),
        };
        match written {
            Ok(()) => {
                self.generation += 1;
                self.stale_instances.remove(&primitive);
            }
            Err(why) if scene.layout() != uploaded.layout() => {
                log::warn!(
                    "tracer: {} instances of primitive {primitive} do not fit: {why}",
                    transforms.len()
                );
                self.state = State::Failed(why);
            }
            Err(why) => {
                if self.stale_instances.insert(primitive) {
                    log::warn!(
                        "tracer: primitive {primitive} keeps its traced place until the next upload: {why}"
                    );
                }
            }
        }
    }

    /// Whether the built scene traces every instance `set_instances` asked for.
    pub fn instances_current(&self) -> bool {
        self.stale_instances.is_empty()
    }

    /// Moves instanced primitives to their new world transforms, by scene primitive; see [`TraceScene::set_primitive_transforms`].
    pub fn set_primitive_transforms(&mut self, gpu: &GpuContext, transforms: &[Mat4]) {
        self.move_instances(gpu, |scene| scene.set_primitive_transforms(transforms));
    }

    /// Re-poses the skinned and morphed meshes an animation deforms; see [`TraceScene::pose_deformed`].
    pub fn pose_deformed(&mut self, gpu: &GpuContext, world: &[Mat4], weights: &[&[f32]]) {
        if let State::Ready(ready) = &mut self.state {
            let (scene, uploaded) = &mut **ready;
            let changed = scene.pose_deformed(world, weights);
            if !changed.is_empty() {
                uploaded.write_meshes(gpu, scene, &changed);
                self.generation += 1;
            }
        }
    }

    fn move_instances(&mut self, gpu: &GpuContext, change: impl FnOnce(&mut TraceScene)) {
        if let State::Ready(ready) = &mut self.state {
            let (scene, uploaded) = &mut **ready;
            change(scene);
            uploaded.write_instances(gpu, scene);
            self.generation += 1;
        }
    }

    /// Changes with every upload and every instance move; equal values mean the same traced scene.
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

/// The scene's counts after its instances changed, keeping the build's times and the uploaded layers.
fn remeasure(scene: &mut TraceScene) {
    let before = scene.stats.clone();
    // The layer arrays were trimmed or dropped after the upload; their bytes are the upload's.
    let layer_bytes = before.gpu_bytes.saturating_sub(buffer_bytes(&before));
    scene.measure(before.build_seconds);
    let stats = &mut scene.stats;
    stats.gpu_bytes = buffer_bytes(stats) + layer_bytes;
    stats.layers = before.layers;
    stats.layer_sizes = before.layer_sizes;
    stats.stage_seconds = before.stage_seconds;
}

/// The storage buffers' bytes at `stats`' counts.
fn buffer_bytes(stats: &TraceStats) -> u64 {
    TraceBufferSizes::of_counts(TraceCounts {
        nodes: u64::from(stats.nodes),
        instances: u64::from(stats.instances),
        materials: u64::from(stats.materials),
        triangles: stats.unique_triangles,
        vertices: stats.vertices,
    })
    .total()
}

fn log_upload(stats: &TraceStats) {
    log::info!(
        "tracer: {} triangles ({} stored) in {} instances of {} meshes, {} nodes, {:.1} MiB on the GPU, built in {:?} s",
        stats.triangles,
        stats.unique_triangles,
        stats.instances,
        stats.meshes,
        stats.nodes,
        stats.gpu_bytes as f64 / f64::from(1u32 << 20),
        stats.build_seconds
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_line_names_the_stage_and_the_result() {
        let building = TraceStatus::Building(TraceProgress {
            stage: super::super::build::TraceStage::Bvh,
            fraction: 0.424,
        });
        assert_eq!(building.text(), "Tracer: building BVHs  ·  42 %");
        let ready = TraceStatus::Ready(TraceStats {
            triangles: 5_900_000,
            instances: 1087,
            build_seconds: Some(1.84),
            ..TraceStats::default()
        });
        assert_eq!(
            ready.text(),
            "Tracer ready  ·  5.90 M triangles in 1087 instances, built in 1.8 s"
        );
    }

    #[test]
    fn render_modes_map_to_the_editors_switch_and_back() {
        for (index, mode) in RenderMode::ALL.iter().enumerate() {
            assert_eq!(mode.index(), index);
            assert_eq!(RenderMode::from_index(index), *mode);
        }
        assert_eq!(RenderMode::from_index(9), RenderMode::Raster);
        assert!(!RenderMode::Raster.is_traced() && RenderMode::PathTraced.is_traced());
    }
}
