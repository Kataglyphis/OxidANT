//! The traversal on the CPU, step for step as `bvh_traverse.slang` runs it: the oracle the GPU kernels are tested against.

use glam::{Vec2, Vec3, Vec4};

use super::bvh::{BvhNode, BLAS_MAX_DEPTH, TLAS_MAX_DEPTH};
use super::geometry::{unpack_vertex, TRIANGLE_MASKED, TRIANGLE_MATERIAL};
use super::layers::LayerArray;
use super::material::{flags, GpuMaterial, NO_LAYER};
use super::scene::TraceScene;

/// `TraceHit::instance` of a ray that hit nothing.
pub const NO_HIT: u32 = u32::MAX;
/// A box the ray misses enters at this distance.
pub const FAR: f32 = 3.0e38;
/// PBRT's `1 + 2 gamma(3)`: rounding must not miss a box a triangle lies flat in.
const EXIT_SCALE: f32 = 1.000_000_4;
/// The traversal stack: a top-level path and a bottom-level path below it.
pub const STACK_SIZE: usize = (TLAS_MAX_DEPTH + BLAS_MAX_DEPTH) as usize;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceRay {
    pub origin: Vec3,
    pub tmin: f32,
    pub direction: Vec3,
    pub tmax: f32,
}

/// The closest accepted triangle: its distance in units of the ray's direction, instance, global triangle and barycentrics of vertices 1 and 2.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceHit {
    pub t: f32,
    pub instance: u32,
    pub triangle: u32,
    pub barycentrics: Vec2,
}

/// Componentwise `1 / d`, with zero components nudged so the slab test never computes `0 * inf`.
pub fn safe_inverse(d: Vec3) -> Vec3 {
    let nudge = |c: f32| {
        if c.abs() > 1e-30 {
            c
        } else if c < 0.0 {
            -1e-30
        } else {
            1e-30
        }
    };
    Vec3::new(1.0 / nudge(d.x), 1.0 / nudge(d.y), 1.0 / nudge(d.z))
}

/// Where the ray enters `node`'s box within `[tmin, tmax]`, or [`FAR`].
pub fn box_entry(node: &BvhNode, origin: Vec3, inv: Vec3, tmin: f32, tmax: f32) -> f32 {
    let t0 = (Vec3::from_array(node.min) - origin) * inv;
    let t1 = (Vec3::from_array(node.max) - origin) * inv;
    let (near, far) = (t0.min(t1), t0.max(t1));
    let enter = near.x.max(near.y).max(near.z.max(tmin));
    let exit = far.x.min(far.y).min(far.z.min(tmax)) * EXIT_SCALE;
    if enter <= exit {
        enter
    } else {
        FAR
    }
}

/// Möller-Trumbore, two-sided: the distance and the barycentrics of vertices 1 and 2, or `None`.
pub fn intersect_triangle(o: Vec3, d: Vec3, p0: Vec3, p1: Vec3, p2: Vec3) -> Option<(f32, Vec2)> {
    let (e1, e2) = (p1 - p0, p2 - p0);
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det == 0.0 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - p0;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    Some((e2.dot(q) * inv, Vec2::new(u, v)))
}

/// Strictly closer by distance, ties broken by instance and then triangle, so any visiting order picks the same hit.
fn closer(t: f32, instance: u32, triangle: u32, best: &TraceHit) -> bool {
    t < best.t
        || (t == best.t
            && (instance < best.instance
                || (instance == best.instance && triangle < best.triangle)))
}

/// The ray as one level of the hierarchy sees it.
#[derive(Clone, Copy)]
struct Frame {
    origin: Vec3,
    direction: Vec3,
    inv: Vec3,
}

impl Frame {
    fn new(origin: Vec3, direction: Vec3) -> Self {
        Self {
            origin,
            direction,
            inv: safe_inverse(direction),
        }
    }
}

/// One traversal's state: the stack over both levels, and the instance whose mesh it is in.
struct Walk<'a> {
    ray: &'a TraceRay,
    world: Frame,
    frame: Frame,
    stack: [u32; STACK_SIZE],
    sp: usize,
    /// The stack height where the current mesh began; `None` in the top level.
    floor: Option<usize>,
    instance: u32,
}

impl<'a> Walk<'a> {
    fn new(ray: &'a TraceRay) -> Self {
        let world = Frame::new(ray.origin, ray.direction);
        Self {
            ray,
            world,
            frame: world,
            stack: [0; STACK_SIZE],
            sp: 0,
            floor: None,
            instance: 0,
        }
    }

    fn hits(&self, node: &BvhNode, tmax: f32) -> bool {
        box_entry(node, self.frame.origin, self.frame.inv, self.ray.tmin, tmax) < FAR
    }

    /// The nearer child the ray enters, the farther one pushed; `None` when it misses both.
    fn descend(&mut self, scene: &TraceScene, node: &BvhNode, tmax: f32) -> Option<BvhNode> {
        let (index_a, index_b) = (node.first as usize, node.first as usize + 1);
        let (a, b) = (scene.nodes[index_a], scene.nodes[index_b]);
        let (origin, inv, tmin) = (self.frame.origin, self.frame.inv, self.ray.tmin);
        let (ta, tb) = (
            box_entry(&a, origin, inv, tmin, tmax),
            box_entry(&b, origin, inv, tmin, tmax),
        );
        let (near, t_near, far_index, t_far) = if tb < ta {
            (b, tb, index_a, ta)
        } else {
            (a, ta, index_b, tb)
        };
        if t_near >= FAR {
            return None;
        }
        if t_far < FAR && self.sp < STACK_SIZE {
            self.stack[self.sp] = far_index as u32;
            self.sp += 1;
        }
        Some(near)
    }

    /// Carries the ray into a top-level leaf's instance; its mesh's root when the ray enters it.
    fn enter(&mut self, scene: &TraceScene, leaf: &BvhNode, tmax: f32) -> Option<BvhNode> {
        self.instance = leaf.first;
        let inst = &scene.instances[self.instance as usize];
        let to_object = inst.world_to_object();
        self.frame = Frame::new(
            to_object.transform_point3(self.ray.origin),
            to_object.transform_vector3(self.ray.direction),
        );
        self.floor = Some(self.sp);
        let root = scene.nodes[inst.blas_root as usize];
        self.hits(&root, tmax).then_some(root)
    }

    /// The next pushed node the ray still enters, leaving each finished mesh for the top level.
    fn pop(&mut self, scene: &TraceScene, tmax: f32) -> Option<BvhNode> {
        loop {
            if self.floor == Some(self.sp) {
                self.floor = None;
                self.frame = self.world;
            }
            if self.sp == 0 {
                return None;
            }
            self.sp -= 1;
            let node = scene.nodes[self.stack[self.sp] as usize];
            if self.hits(&node, tmax) {
                return Some(node);
            }
        }
    }
}

impl TraceScene {
    /// The closest hit in `[tmin, tmax]`, MASK materials alpha-tested.
    pub fn trace_closest(&self, ray: &TraceRay) -> Option<TraceHit> {
        Some(self.trace(ray, false)).filter(|hit| hit.instance != NO_HIT)
    }

    /// Whether anything blocks `[tmin, tmax]`; ends at the first accepted hit.
    pub fn trace_any(&self, ray: &TraceRay) -> bool {
        self.trace(ray, true).instance != NO_HIT
    }

    fn trace(&self, ray: &TraceRay, any_hit: bool) -> TraceHit {
        let mut best = TraceHit {
            t: ray.tmax,
            instance: NO_HIT,
            triangle: 0,
            barycentrics: Vec2::ZERO,
        };
        if self.instances.is_empty() {
            return best;
        }
        let mut walk = Walk::new(ray);
        let root = self.nodes[self.tlas_root() as usize];
        let mut node = walk.hits(&root, best.t).then_some(root);
        while let Some(current) = node {
            node = if !current.is_leaf() {
                walk.descend(self, &current, best.t)
            } else if walk.floor.is_none() {
                walk.enter(self, &current, best.t)
            } else if self.test_leaf(&current, &walk.frame, ray.tmin, walk.instance, &mut best)
                && any_hit
            {
                return best;
            } else {
                None
            };
            if node.is_none() {
                node = walk.pop(self, best.t);
            }
        }
        best
    }

    /// Tests one bottom-level leaf's triangles; true when one became the best hit.
    fn test_leaf(
        &self,
        node: &BvhNode,
        frame: &Frame,
        tmin: f32,
        instance: u32,
        best: &mut TraceHit,
    ) -> bool {
        let mut found = false;
        for triangle in node.first..node.first + node.count {
            if self.test_triangle(triangle, frame, tmin, instance, best) {
                found = true;
            }
        }
        found
    }

    fn test_triangle(
        &self,
        triangle: u32,
        frame: &Frame,
        tmin: f32,
        instance: u32,
        best: &mut TraceHit,
    ) -> bool {
        let tri = self.triangles[triangle as usize];
        let p = |v: u32| Vec4::from_array(self.positions[v as usize]).truncate();
        let Some((t, barycentrics)) = intersect_triangle(
            frame.origin,
            frame.direction,
            p(tri[0]),
            p(tri[1]),
            p(tri[2]),
        ) else {
            return false;
        };
        if t < tmin || !closer(t, instance, triangle, best) {
            return false;
        }
        if tri[3] & TRIANGLE_MASKED != 0 && self.alpha_masked_out(tri, barycentrics) {
            return false;
        }
        *best = TraceHit {
            t,
            instance,
            triangle,
            barycentrics,
        };
        true
    }

    /// The MASK test at a hit: base colour alpha times texture alpha times vertex alpha below the cutoff.
    pub fn alpha_masked_out(&self, tri: [u32; 4], barycentrics: Vec2) -> bool {
        let material = &self.materials[(tri[3] & TRIANGLE_MATERIAL) as usize];
        if material.alpha_mode() != flags::ALPHA_MASK {
            return false;
        }
        let w = [
            1.0 - barycentrics.x - barycentrics.y,
            barycentrics.x,
            barycentrics.y,
        ];
        let corners = [0, 1, 2].map(|k| unpack_vertex(&self.vertices[tri[k] as usize]));
        let mix = |f: &dyn Fn(usize) -> Vec4| f(0) * w[0] + f(1) * w[1] + f(2) * w[2];
        let uv0 = mix(&|k| corners[k].uv0.extend(0.0).extend(0.0))
            .truncate()
            .truncate();
        let uv1 = mix(&|k| corners[k].uv1.extend(0.0).extend(0.0))
            .truncate()
            .truncate();
        let alpha = mix(&|k| corners[k].color).w * material.base_color[3];
        let texel = sample_nearest(
            &self.srgb,
            material,
            0,
            material.slot_uv(0, uv0.to_array(), uv1.to_array()),
        );
        alpha * texel.w < material.emissive_cutoff[3]
    }

    /// Every triangle of every instance, tested in the same frames as the traversal: the reference it must match.
    pub fn brute_force_closest(&self, ray: &TraceRay) -> Option<TraceHit> {
        let mut best = TraceHit {
            t: ray.tmax,
            instance: NO_HIT,
            triangle: 0,
            barycentrics: Vec2::ZERO,
        };
        for (instance, inst) in self.instances.iter().enumerate() {
            let to_object = inst.world_to_object();
            let frame = Frame::new(
                to_object.transform_point3(ray.origin),
                to_object.transform_vector3(ray.direction),
            );
            let mesh = &self.meshes[inst.mesh as usize];
            for triangle in mesh.first_triangle..mesh.first_triangle + mesh.triangle_count {
                self.test_triangle(triangle, &frame, ray.tmin, instance as u32, &mut best);
            }
        }
        Some(best).filter(|hit| hit.instance != NO_HIT)
    }
}

/// A slot's texel at level 0, nearest, with its wrap modes; white without a layer.
pub fn sample_nearest(
    array: &LayerArray,
    material: &GpuMaterial,
    slot: usize,
    uv: [f32; 2],
) -> Vec4 {
    let layer = material.layer(slot);
    if layer == NO_LAYER {
        return Vec4::ONE;
    }
    let size = array.size as f32;
    let wrap = material.wrap(slot);
    let coord = |c: f32, mode: u32| {
        let c = match mode {
            flags::WRAP_CLAMP => c.clamp(0.0, 1.0),
            flags::WRAP_MIRROR => 1.0 - ((c * 0.5).fract().abs() * 2.0 - 1.0).abs(),
            _ => c - c.floor(),
        };
        ((c * size) as u32).min(array.size - 1)
    };
    let (x, y) = (coord(uv[0], wrap[0]), coord(uv[1], wrap[1]));
    let at = ((y * array.size + x) * 4) as usize;
    let level = &array.layers[layer as usize][0];
    Vec4::from_array([0, 1, 2, 3].map(|c| f32::from(level[at + c]) / 255.0))
}
