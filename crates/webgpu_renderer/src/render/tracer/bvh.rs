//! Binned-SAH bounding volume hierarchies, flattened into the 32-byte nodes `bvh_traverse.slang` reads.

use glam::{UVec3, Vec3A};

/// Deepest leaf of a bottom-level hierarchy: the traversal stack holds one entry per level.
pub const BLAS_MAX_DEPTH: u32 = 64;
/// Deepest leaf of the top level over instances.
pub const TLAS_MAX_DEPTH: u32 = 32;
const BINS: usize = 16;
/// A node with more primitives bins a strided sample of about this many: the top levels set the shape, not every box.
const BIN_SAMPLE: usize = 16 * 1024;
/// SAH cost of visiting a node, relative to one primitive test.
const TRAVERSAL_COST: f32 = 1.0;

/// An axis-aligned box; [`Aabb::EMPTY`] is the identity of [`Aabb::grow`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3A,
    pub max: Vec3A,
}

impl Aabb {
    pub const EMPTY: Self = Self {
        min: Vec3A::splat(f32::INFINITY),
        max: Vec3A::splat(f32::NEG_INFINITY),
    };

    pub fn from_points(points: &[Vec3A]) -> Self {
        let mut bounds = Self::EMPTY;
        for &point in points {
            bounds.grow_point(point);
        }
        bounds
    }

    pub fn grow(&mut self, other: &Self) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
    }

    pub fn grow_point(&mut self, point: Vec3A) {
        self.min = self.min.min(point);
        self.max = self.max.max(point);
    }

    /// Half the surface area, the SAH's measure; zero for an empty box.
    pub fn half_area(&self) -> f32 {
        let e = (self.max - self.min).max(Vec3A::ZERO);
        e.x * e.y + e.y * e.z + e.z * e.x
    }

    pub fn is_empty(&self) -> bool {
        self.min.cmpgt(self.max).any()
    }

    pub fn contains(&self, other: &Self) -> bool {
        self.min.cmple(other.min).all() && self.max.cmpge(other.max).all()
    }

    /// Twice the centre, which binning compares without the multiply.
    fn centroid2(&self) -> Vec3A {
        self.min + self.max
    }
}

/// One node: a leaf when `count > 0` (`first` indexes the primitives), else `first` is the left child and `first + 1` the right.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BvhNode {
    pub min: [f32; 3],
    pub first: u32,
    pub max: [f32; 3],
    pub count: u32,
}

impl BvhNode {
    fn with_bounds(bounds: &Aabb) -> Self {
        Self {
            min: bounds.min.to_array(),
            first: 0,
            max: bounds.max.to_array(),
            count: 0,
        }
    }

    pub fn bounds(&self) -> Aabb {
        Aabb {
            min: Vec3A::from_array(self.min),
            max: Vec3A::from_array(self.max),
        }
    }

    pub fn is_leaf(&self) -> bool {
        self.count > 0
    }
}

/// Refits `nodes[range]` to its leaves' `leaf(first, count)` boxes from the back, since every child follows its parent.
pub fn refit(
    nodes: &mut [BvhNode],
    range: std::ops::Range<usize>,
    leaf: impl Fn(u32, u32) -> Aabb,
) {
    for index in range.rev() {
        let node = nodes[index];
        let bounds = if node.is_leaf() {
            leaf(node.first, node.count)
        } else {
            let mut bounds = nodes[node.first as usize].bounds();
            bounds.grow(&nodes[node.first as usize + 1].bounds());
            bounds
        };
        nodes[index].min = bounds.min.to_array();
        nodes[index].max = bounds.max.to_array();
    }
}

/// Leaf size and depth limits of one hierarchy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BvhParams {
    pub max_leaf: u32,
    pub max_depth: u32,
}

impl BvhParams {
    pub const BLAS: Self = Self {
        max_leaf: 4,
        max_depth: BLAS_MAX_DEPTH,
    };
    /// One instance per leaf: the traversal enters a leaf's instance without a loop.
    pub const TLAS: Self = Self {
        max_leaf: 1,
        max_depth: TLAS_MAX_DEPTH,
    };
}

/// A finished hierarchy: `nodes[0]` is the root, and leaf slots map to input primitives through `order`.
#[derive(Clone, Debug, Default)]
pub struct Bvh {
    pub nodes: Vec<BvhNode>,
    pub order: Vec<u32>,
}

impl Bvh {
    /// The depth of the deepest leaf, the root being depth 0.
    pub fn depth(&self) -> u32 {
        let mut deepest = 0;
        let mut pending = vec![(0usize, 0u32)];
        while let Some((index, depth)) = pending.pop() {
            let node = self.nodes[index];
            if node.is_leaf() {
                deepest = deepest.max(depth);
            } else {
                pending.push((node.first as usize, depth + 1));
                pending.push((node.first as usize + 1, depth + 1));
            }
        }
        deepest
    }
}

/// A node still to split: its primitive range and the bounds of their centroids.
#[derive(Clone, Copy, Debug)]
struct Task {
    node: u32,
    start: u32,
    end: u32,
    depth: u32,
    centroids: Aabb,
}

/// The primitives one builder partitions in place; `offset` is their first slot in the whole order.
struct Prims<'a> {
    boxes: &'a mut [Aabb],
    order: &'a mut [u32],
    offset: u32,
}

/// Where a node splits: its children's ranges relative to the node and their boxes.
struct Split {
    mid: usize,
    boxes: [Aabb; 2],
    centroids: [Aabb; 2],
}

/// Splits or finishes one task; returns how many primitives it put into a leaf.
fn process(
    prims: &mut Prims<'_>,
    nodes: &mut Vec<BvhNode>,
    tasks: &mut Vec<Task>,
    task: Task,
    params: BvhParams,
) -> usize {
    let range = task.start as usize..task.end as usize;
    let bounds = nodes[task.node as usize].bounds();
    let split = plan(
        &mut prims.boxes[range.clone()],
        &mut prims.order[range.clone()],
        &task,
        &bounds,
        params,
    );
    let Some(split) = split else {
        let node = &mut nodes[task.node as usize];
        node.first = prims.offset + task.start;
        node.count = task.end - task.start;
        return range.len();
    };
    let left = nodes.len() as u32;
    nodes.push(BvhNode::with_bounds(&split.boxes[0]));
    nodes.push(BvhNode::with_bounds(&split.boxes[1]));
    let parent = &mut nodes[task.node as usize];
    parent.first = left;
    parent.count = 0;
    let mid = task.start + split.mid as u32;
    let child = |node, start, end, centroids| Task {
        node,
        start,
        end,
        depth: task.depth + 1,
        centroids,
    };
    // Right first, so the left subtree is built next and its nodes stay close to their parent.
    tasks.push(child(left + 1, mid, task.end, split.centroids[1]));
    tasks.push(child(left, task.start, mid, split.centroids[0]));
    0
}

/// Levels a median split needs to get `count` primitives into leaves of `max_leaf`.
fn levels_needed(count: u32, max_leaf: u32) -> u32 {
    let leaves = count.div_ceil(max_leaf.max(1));
    u32::BITS - (leaves.max(1) - 1).leading_zeros()
}

/// The split of one node, or `None` for a leaf; SAH unless only median splits still fit under `max_depth`.
fn plan(
    boxes: &mut [Aabb],
    order: &mut [u32],
    task: &Task,
    bounds: &Aabb,
    params: BvhParams,
) -> Option<Split> {
    let count = boxes.len() as u32;
    if count <= 1 || task.depth >= params.max_depth {
        return None;
    }
    let fits_leaf = count <= params.max_leaf;
    if task.depth + levels_needed(count, params.max_leaf) < params.max_depth {
        match sah_split(boxes, &task.centroids, bounds) {
            Some(best) if !fits_leaf || best.cost < count as f32 => {
                if let Some(split) = partition_at(boxes, order, &task.centroids, &best) {
                    return Some(split);
                }
            }
            _ if fits_leaf => return None,
            _ => {}
        }
    } else if fits_leaf {
        return None;
    }
    Some(median_split(boxes, order, &task.centroids))
}

/// The cheapest bin boundary over the three axes.
struct SahChoice {
    axis: usize,
    bin: usize,
    cost: f32,
    boxes: [Aabb; 2],
}

fn sample_stride(count: usize) -> usize {
    (count / BIN_SAMPLE).max(1)
}

/// Bin index of a centroid: `partition_at` recomputes it exactly as binning did.
fn bin_of(centroid2: f32, min: f32, scale: f32) -> usize {
    (((centroid2 - min) * scale) as usize).min(BINS - 1)
}

fn bin_scale(centroids: &Aabb) -> Vec3A {
    let extent = centroids.max - centroids.min;
    Vec3A::select(
        extent.cmpgt(Vec3A::ZERO),
        Vec3A::splat(BINS as f32) / extent,
        Vec3A::ZERO,
    )
}

/// Bins the centroids along all three axes and sweeps each for the lowest SAH cost.
fn sah_split(boxes: &[Aabb], centroids: &Aabb, bounds: &Aabb) -> Option<SahChoice> {
    let scale = bin_scale(centroids);
    let mut bins = [[(Aabb::EMPTY, 0u32); BINS]; 3];
    let last = UVec3::splat(BINS as u32 - 1);
    for prim in boxes.iter().step_by(sample_stride(boxes.len())) {
        let index = ((prim.centroid2() - centroids.min) * scale)
            .as_uvec3()
            .min(last);
        for (axis, bins) in bins.iter_mut().enumerate() {
            let bin = &mut bins[index[axis] as usize];
            bin.0.grow(prim);
            bin.1 += 1;
        }
    }
    let area = bounds.half_area();
    let inverse_area = if area > 0.0 { 1.0 / area } else { 0.0 };
    let mut best: Option<SahChoice> = None;
    for (axis, bins) in bins.iter().enumerate() {
        if scale[axis] == 0.0 {
            continue;
        }
        if let Some(choice) = sweep(axis, bins, inverse_area) {
            if best.as_ref().is_none_or(|b| choice.cost < b.cost) {
                best = Some(choice);
            }
        }
    }
    best
}

/// The best boundary between one axis's bins: prefix boxes from the left against suffix boxes from the right.
fn sweep(axis: usize, bins: &[(Aabb, u32); BINS], inverse_area: f32) -> Option<SahChoice> {
    let mut right_area = [0.0f32; BINS];
    let mut right_count = [0u32; BINS];
    let mut right_boxes = [Aabb::EMPTY; BINS];
    let (mut bounds, mut count) = (Aabb::EMPTY, 0u32);
    for bin in (1..BINS).rev() {
        bounds.grow(&bins[bin].0);
        count += bins[bin].1;
        (right_area[bin], right_count[bin], right_boxes[bin]) = (bounds.half_area(), count, bounds);
    }
    let (mut bounds, mut count) = (Aabb::EMPTY, 0u32);
    let mut best: Option<SahChoice> = None;
    for bin in 1..BINS {
        bounds.grow(&bins[bin - 1].0);
        count += bins[bin - 1].1;
        if count == 0 || right_count[bin] == 0 {
            continue;
        }
        let cost = TRAVERSAL_COST
            + (bounds.half_area() * count as f32 + right_area[bin] * right_count[bin] as f32)
                * inverse_area;
        if best.as_ref().is_none_or(|b| cost < b.cost) {
            best = Some(SahChoice {
                axis,
                bin,
                cost,
                boxes: [bounds, right_boxes[bin]],
            });
        }
    }
    best
}

/// Moves the primitives left of `choice` to the front; `None` when one side would be empty.
fn partition_at(
    boxes: &mut [Aabb],
    order: &mut [u32],
    centroids: &Aabb,
    choice: &SahChoice,
) -> Option<Split> {
    let axis = choice.axis;
    let (min, scale) = (centroids.min[axis], bin_scale(centroids)[axis]);
    // Sampled bins saw only some boxes, so the children's bounds are grown here from all of them.
    let sampled = sample_stride(boxes.len()) > 1;
    let mut halves = [Aabb::EMPTY; 2];
    let mut grown = [Aabb::EMPTY; 2];
    let (mut i, mut j) = (0usize, boxes.len());
    while i < j {
        let centroid = boxes[i].centroid2();
        let side = usize::from(bin_of(centroid[axis], min, scale) >= choice.bin);
        halves[side].grow_point(centroid);
        if sampled {
            grown[side].grow(&boxes[i]);
        }
        if side == 0 {
            i += 1;
        } else {
            j -= 1;
            boxes.swap(i, j);
            order.swap(i, j);
        }
    }
    (i > 0 && i < boxes.len()).then_some(Split {
        mid: i,
        boxes: if sampled { grown } else { choice.boxes },
        centroids: halves,
    })
}

/// Halves the range at the median centroid of its longest axis: always progresses, whatever the input.
fn median_split(boxes: &mut [Aabb], order: &mut [u32], centroids: &Aabb) -> Split {
    let mid = boxes.len() / 2;
    let extent = centroids.max - centroids.min;
    if extent.max_element() > 0.0 {
        let axis = extent.max_position();
        let mut pairs: Vec<(Aabb, u32)> =
            boxes.iter().copied().zip(order.iter().copied()).collect();
        pairs.select_nth_unstable_by(mid, |a, b| {
            a.0.centroid2()[axis].total_cmp(&b.0.centroid2()[axis])
        });
        for (slot, (bounds, index)) in pairs.into_iter().enumerate() {
            boxes[slot] = bounds;
            order[slot] = index;
        }
    }
    let half = |part: &[Aabb]| {
        let (mut bounds, mut centres) = (Aabb::EMPTY, Aabb::EMPTY);
        for prim in part {
            bounds.grow(prim);
            centres.grow_point(prim.centroid2());
        }
        (bounds, centres)
    };
    let (left, right) = (half(&boxes[..mid]), half(&boxes[mid..]));
    Split {
        mid,
        boxes: [left.0, right.0],
        centroids: [left.1, right.1],
    }
}

/// A resumable build: [`BvhBuilder::step`] splits nodes until a work budget is spent, so a frame loop can spread it.
pub struct BvhBuilder {
    boxes: Vec<Aabb>,
    order: Vec<u32>,
    nodes: Vec<BvhNode>,
    tasks: Vec<Task>,
    params: BvhParams,
    finished: usize,
}

impl BvhBuilder {
    /// Starts a hierarchy over `boxes`, which must not be empty.
    pub fn new(boxes: Vec<Aabb>, params: BvhParams) -> Self {
        assert!(!boxes.is_empty(), "a BVH needs at least one primitive");
        let (mut bounds, mut centroids) = (Aabb::EMPTY, Aabb::EMPTY);
        for prim in &boxes {
            bounds.grow(prim);
            centroids.grow_point(prim.centroid2());
        }
        let count = boxes.len() as u32;
        let mut nodes = Vec::with_capacity(boxes.len() * 2);
        nodes.push(BvhNode::with_bounds(&bounds));
        Self {
            order: (0..count).collect(),
            boxes,
            nodes,
            tasks: vec![Task {
                node: 0,
                start: 0,
                end: count,
                depth: 0,
                centroids,
            }],
            params,
            finished: 0,
        }
    }

    pub fn primitive_count(&self) -> usize {
        self.boxes.len()
    }

    /// Splits nodes until about `budget` primitives were binned; true once the hierarchy is complete.
    pub fn step(&mut self, budget: usize) -> bool {
        let mut spent = 0usize;
        while let Some(task) = self.tasks.pop() {
            spent += (task.end - task.start) as usize;
            let mut prims = Prims {
                boxes: &mut self.boxes,
                order: &mut self.order,
                offset: 0,
            };
            self.finished += process(
                &mut prims,
                &mut self.nodes,
                &mut self.tasks,
                task,
                self.params,
            );
            if spent >= budget {
                break;
            }
        }
        self.tasks.is_empty()
    }

    /// The share of primitives already in leaves.
    pub fn progress(&self) -> f32 {
        self.finished as f32 / self.boxes.len() as f32
    }

    pub fn finish(mut self) -> Bvh {
        while !self.step(usize::MAX) {}
        Bvh {
            nodes: self.nodes,
            order: self.order,
        }
    }
}

/// Builds on the calling thread.
pub fn build(boxes: Vec<Aabb>, params: BvhParams) -> Bvh {
    BvhBuilder::new(boxes, params).finish()
}

#[cfg(not(target_arch = "wasm32"))]
pub use parallel::build_parallel;

#[cfg(not(target_arch = "wasm32"))]
mod parallel {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use super::{process, Aabb, Bvh, BvhBuilder, BvhNode, BvhParams, Prims, Task};

    /// Below this many primitives one thread is faster than the hand-off.
    const PARALLEL_MIN: usize = 16 * 1024;

    /// One subtree to build on a worker: its task and the disjoint slices it partitions.
    type Job<'a> = (Task, &'a mut [Aabb], &'a mut [u32]);

    /// Builds the top levels on this thread, then the subtrees below them on `threads` workers.
    pub fn build_parallel(
        boxes: Vec<Aabb>,
        params: BvhParams,
        threads: usize,
        progress: &AtomicUsize,
    ) -> Bvh {
        let mut builder = BvhBuilder::new(boxes, params);
        let total = builder.primitive_count();
        if threads <= 1 || total < PARALLEL_MIN {
            let bvh = builder.finish();
            progress.fetch_add(total, Ordering::Relaxed);
            return bvh;
        }
        split_top_levels(&mut builder, total / (threads * 4));
        progress.fetch_add(builder.finished, Ordering::Relaxed);
        let mut tasks = std::mem::take(&mut builder.tasks);
        tasks.sort_by_key(|task| task.start);
        let jobs = slice_jobs(&tasks, &mut builder.boxes, &mut builder.order);
        let built = run_jobs(jobs, &builder.nodes, params, threads, progress);
        for (task, local) in built {
            stitch(&mut builder.nodes, task.node, local);
        }
        Bvh {
            nodes: builder.nodes,
            order: builder.order,
        }
    }

    /// Splits the largest pending task until none holds more than `grain` primitives.
    fn split_top_levels(builder: &mut BvhBuilder, grain: usize) {
        loop {
            let largest = builder
                .tasks
                .iter()
                .enumerate()
                .max_by_key(|(_, task)| task.end - task.start)
                .filter(|(_, task)| (task.end - task.start) as usize > grain);
            let Some((index, _)) = largest else {
                return;
            };
            let task = builder.tasks.swap_remove(index);
            let mut prims = Prims {
                boxes: &mut builder.boxes,
                order: &mut builder.order,
                offset: 0,
            };
            builder.finished += process(
                &mut prims,
                &mut builder.nodes,
                &mut builder.tasks,
                task,
                builder.params,
            );
        }
    }

    /// One job per task, each owning its own range of the arrays; `tasks` are sorted and never overlap.
    fn slice_jobs<'a>(
        tasks: &[Task],
        mut boxes: &'a mut [Aabb],
        mut order: &'a mut [u32],
    ) -> Vec<Job<'a>> {
        let mut jobs = Vec::with_capacity(tasks.len());
        let mut cursor = 0u32;
        for task in tasks {
            let skip = (task.start - cursor) as usize;
            let len = (task.end - task.start) as usize;
            let (_, rest_boxes) = std::mem::take(&mut boxes).split_at_mut(skip);
            let (job_boxes, rest_boxes) = rest_boxes.split_at_mut(len);
            let (_, rest_order) = std::mem::take(&mut order).split_at_mut(skip);
            let (job_order, rest_order) = rest_order.split_at_mut(len);
            (boxes, order, cursor) = (rest_boxes, rest_order, task.end);
            jobs.push((*task, job_boxes, job_order));
        }
        jobs.sort_by_key(|(task, _, _)| std::cmp::Reverse(task.end - task.start));
        jobs
    }

    /// Workers take the largest job left; each returns its task and the subtree it built.
    fn run_jobs(
        jobs: Vec<Job<'_>>,
        nodes: &[BvhNode],
        params: BvhParams,
        threads: usize,
        progress: &AtomicUsize,
    ) -> Vec<(Task, Vec<BvhNode>)> {
        let queue = Mutex::new(jobs);
        let results = Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            for _ in 0..threads {
                scope.spawn(|| loop {
                    let Some(job) = queue.lock().ok().and_then(|mut jobs| jobs.pop()) else {
                        return;
                    };
                    let task = job.0;
                    let local = build_subtree(job, nodes[task.node as usize], params, progress);
                    if let Ok(mut done) = results.lock() {
                        done.push((task, local));
                    }
                });
            }
        });
        results.into_inner().unwrap_or_default()
    }

    /// Builds one subtree with local node indices; `local[0]` is its root.
    fn build_subtree(
        (task, boxes, order): Job<'_>,
        root: BvhNode,
        params: BvhParams,
        progress: &AtomicUsize,
    ) -> Vec<BvhNode> {
        let mut prims = Prims {
            offset: task.start,
            boxes,
            order,
        };
        let mut nodes = vec![root];
        let mut tasks = vec![Task {
            node: 0,
            start: 0,
            end: task.end - task.start,
            ..task
        }];
        while let Some(next) = tasks.pop() {
            let done = process(&mut prims, &mut nodes, &mut tasks, next, params);
            progress.fetch_add(done, Ordering::Relaxed);
        }
        nodes
    }

    /// Moves a subtree into the global nodes: its root into `slot`, the rest appended with their links shifted.
    fn stitch(nodes: &mut Vec<BvhNode>, slot: u32, local: Vec<BvhNode>) {
        let base = nodes.len() as u32;
        nodes[slot as usize] = relink(local[0], base);
        nodes.extend(local.into_iter().skip(1).map(|node| relink(node, base)));
    }

    /// Local child indices start at 1, after the root that goes back to its slot.
    fn relink(mut node: BvhNode, base: u32) -> BvhNode {
        if !node.is_leaf() {
            node.first = base + node.first - 1;
        }
        node
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::tracer::tests::Rng;

    fn random_boxes(count: usize, seed: u64) -> Vec<Aabb> {
        let mut rng = Rng::new(seed);
        (0..count)
            .map(|_| {
                let c = Vec3A::new(rng.next_f32(), rng.next_f32(), rng.next_f32()) * 100.0;
                let half = Vec3A::new(rng.next_f32(), rng.next_f32(), rng.next_f32());
                Aabb {
                    min: c - half,
                    max: c + half,
                }
            })
            .collect()
    }

    /// Every node bounds its children, every primitive lands in exactly one leaf, and no leaf is too deep.
    fn check(bvh: &Bvh, boxes: &[Aabb], params: BvhParams) {
        let mut seen = vec![false; boxes.len()];
        let mut pending = vec![(0usize, 0u32)];
        while let Some((index, depth)) = pending.pop() {
            let node = bvh.nodes[index];
            assert!(depth <= params.max_depth, "leaf at depth {depth}");
            if node.is_leaf() {
                for slot in node.first..node.first + node.count {
                    let prim = bvh.order[slot as usize] as usize;
                    assert!(!std::mem::replace(&mut seen[prim], true), "{prim} twice");
                    assert!(node.bounds().contains(&boxes[prim]), "leaf misses {prim}");
                }
                continue;
            }
            for child in [node.first, node.first + 1] {
                assert!(node.bounds().contains(&bvh.nodes[child as usize].bounds()));
                pending.push((child as usize, depth + 1));
            }
        }
        assert!(seen.iter().all(|s| *s), "a primitive is in no leaf");
    }

    #[test]
    fn every_primitive_lands_in_one_leaf_under_bounding_nodes() {
        for (count, seed) in [(1, 1), (2, 2), (7, 3), (1000, 4), (20_000, 5)] {
            let boxes = random_boxes(count, seed);
            let bvh = build(boxes.clone(), BvhParams::BLAS);
            check(&bvh, &boxes, BvhParams::BLAS);
            assert!(
                bvh.nodes.len() < 2 * count.max(1),
                "{} nodes",
                bvh.nodes.len()
            );
        }
    }

    #[test]
    fn identical_primitives_still_split_into_small_leaves_within_the_depth_limit() {
        let one = Aabb {
            min: Vec3A::ZERO,
            max: Vec3A::ONE,
        };
        let boxes = vec![one; 5000];
        let bvh = build(boxes.clone(), BvhParams::BLAS);
        check(&bvh, &boxes, BvhParams::BLAS);
        assert!(bvh.nodes.iter().all(|n| !n.is_leaf() || n.count <= 4));
    }

    #[test]
    fn a_degenerate_chain_is_forced_under_the_depth_limit() {
        // Exponentially spaced boxes make SAH peel one primitive per level, past any stack.
        let boxes: Vec<Aabb> = (0..200)
            .map(|i| {
                let x = 1.15f32.powi(i);
                Aabb {
                    min: Vec3A::new(x, 0.0, 0.0),
                    max: Vec3A::new(x * 1.01, 1.0, 1.0),
                }
            })
            .collect();
        let params = BvhParams {
            max_leaf: 1,
            max_depth: 12,
        };
        let bvh = build(boxes.clone(), params);
        check(&bvh, &boxes, params);
        assert!(bvh.depth() <= 12, "depth {}", bvh.depth());
    }

    #[test]
    fn the_top_level_holds_one_instance_per_leaf() {
        let boxes = random_boxes(300, 9);
        let bvh = build(boxes.clone(), BvhParams::TLAS);
        check(&bvh, &boxes, BvhParams::TLAS);
        assert!(bvh.nodes.iter().all(|n| !n.is_leaf() || n.count == 1));
        assert_eq!(bvh.nodes.len(), 2 * 300 - 1, "a full binary tree");
    }

    #[test]
    fn stepping_with_a_small_budget_builds_the_same_hierarchy() {
        let boxes = random_boxes(5000, 11);
        let whole = build(boxes.clone(), BvhParams::BLAS);
        let mut builder = BvhBuilder::new(boxes, BvhParams::BLAS);
        let mut steps = 0;
        while !builder.step(500) {
            steps += 1;
            assert!(builder.progress() < 1.0);
        }
        assert!(steps > 10, "{steps} steps");
        let stepped = builder.finish();
        assert_eq!(stepped.nodes, whole.nodes);
        assert_eq!(stepped.order, whole.order);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_parallel_build_is_a_valid_hierarchy_and_reports_every_primitive() {
        let count = 100_000;
        let boxes = random_boxes(count, 13);
        let progress = std::sync::atomic::AtomicUsize::new(0);
        let bvh = build_parallel(boxes.clone(), BvhParams::BLAS, 4, &progress);
        check(&bvh, &boxes, BvhParams::BLAS);
        assert_eq!(progress.into_inner(), count);
    }

    #[test]
    fn levels_needed_rounds_up() {
        assert_eq!(levels_needed(1, 4), 0);
        assert_eq!(levels_needed(4, 4), 0);
        assert_eq!(levels_needed(5, 4), 1);
        assert_eq!(levels_needed(1 << 20, 1), 20);
        assert_eq!(levels_needed((1 << 20) + 1, 1), 21);
    }
}

#[cfg(test)]
mod wgsl_layout {
    use super::*;
    use crate::render::pod_layout::pod_layout;

    /// The upload writes words 0..4 to `tracerRecords` and 4..8 to `tracerNodeMax`, which `load_node` reads as
    /// (lo, first) and (hi, count); naga has no buffer struct to compare, so both halves are pinned here.
    #[test]
    fn bvh_node_halves_are_the_ones_load_node_reads() {
        let layout = pod_layout!(BvhNode {
            min,
            first,
            max,
            count
        });
        let offsets: Vec<(usize, usize)> = layout.fields.iter().map(|f| (f.1, f.2)).collect();
        assert_eq!(offsets, [(0, 12), (12, 4), (16, 12), (28, 4)]);
        assert_eq!(layout.size, 32);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../shaders/tracer/bvh_traverse.slang");
        let source = std::fs::read_to_string(path).expect("bvh_traverse.slang");
        let body = &source[source
            .find("BvhNode load_node(uint index)")
            .expect("load_node")..];
        let body = &body[..body.find("return node;").expect("load_node returns")];
        for line in [
            "float4 a = tracerRecords[index];",
            "float4 b = tracerNodeMax[index];",
            "node.lo = a.xyz;",
            "node.first = asuint(a.w);",
            "node.hi = b.xyz;",
            "node.count = asuint(b.w);",
        ] {
            assert!(
                body.lines().any(|l| l.trim() == line),
                "load_node lacks `{line}`"
            );
        }
        let upload = include_str!("upload.rs");
        assert!(
            upload.contains("halves(half, &b.records, &b.node_max)"),
            "half 0 must go to tracerRecords"
        );
    }
}
