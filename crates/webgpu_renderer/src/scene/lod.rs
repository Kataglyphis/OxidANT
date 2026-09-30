//! Vertex-clustering mesh simplification (O(n), order-independent) and runtime LOD selection.
//! A merged vertex averages position and normal; other attributes come whole from the medoid.

use std::collections::HashMap;

use glam::{DVec3, Vec3};

use crate::scene::{CpuPrimitive, Vertex};

/// One level of detail: a simplified primitive and the distance beyond which it is used.
#[derive(Clone, Debug)]
pub struct Lod {
    pub primitive: CpuPrimitive,
    /// Switch to this level when the camera is farther than this (world units).
    pub min_distance: f32,
}

/// Clusters vertices onto a grid whose cell is `cell_ratio` of the bbox diagonal.
pub fn simplify_primitive(prim: &CpuPrimitive, cell_ratio: f32) -> CpuPrimitive {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for v in &prim.vertices {
        let p = Vec3::from_array(v.position);
        min = min.min(p);
        max = max.max(p);
    }
    let diagonal = (max - min).length();
    if !diagonal.is_finite() || diagonal <= 0.0 {
        return prim.clone();
    }
    let cell = (diagonal * cell_ratio).max(1e-6);

    // Centroid, not first-wins, which made the result depend on vertex order.
    let mut cell_to_index: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut normal_sums: Vec<Vec3> = Vec::new();
    // f64: order-dependent f32 rounding could flip the medoid argmin below.
    let mut position_sums: Vec<DVec3> = Vec::new();
    let mut merge_counts: Vec<f32> = Vec::new();
    let mut remap: Vec<u32> = Vec::with_capacity(prim.vertices.len());

    for v in &prim.vertices {
        let p = Vec3::from_array(v.position);
        let key = (
            (p.x / cell).floor() as i64,
            (p.y / cell).floor() as i64,
            (p.z / cell).floor() as i64,
        );
        let index = *cell_to_index.entry(key).or_insert_with(|| {
            vertices.push(*v);
            normal_sums.push(Vec3::ZERO);
            position_sums.push(DVec3::ZERO);
            merge_counts.push(0.0);
            (vertices.len() - 1) as u32
        });
        normal_sums[index as usize] += Vec3::from_array(v.normal);
        position_sums[index as usize] += p.as_dvec3();
        merge_counts[index as usize] += 1.0;
        remap.push(index);
    }

    for (index, vertex) in vertices.iter_mut().enumerate() {
        let n = normal_sums[index].normalize_or_zero();
        if n != Vec3::ZERO {
            vertex.normal = n.to_array();
        }
        let count = merge_counts[index];
        if count > 0.0 {
            vertex.position = (position_sums[index] / count as f64).as_vec3().to_array();
        }
    }

    // Medoid, in f64 against the centroid sum; ties break on position, as index order is not stable.
    let mut medoid_index: Vec<u32> = vec![0; vertices.len()];
    let mut medoid_dist_sq: Vec<f64> = vec![f64::INFINITY; vertices.len()];
    let mut medoid_position: Vec<DVec3> = vec![DVec3::ZERO; vertices.len()];
    for (i, v) in prim.vertices.iter().enumerate() {
        let cell = remap[i] as usize;
        let p = Vec3::from_array(v.position).as_dvec3();
        let centroid = position_sums[cell] / merge_counts[cell] as f64;
        let dist_sq = (p - centroid).length_squared();
        let is_better = dist_sq < medoid_dist_sq[cell]
            || (dist_sq == medoid_dist_sq[cell]
                && p.x
                    .total_cmp(&medoid_position[cell].x)
                    .then_with(|| p.y.total_cmp(&medoid_position[cell].y))
                    .then_with(|| p.z.total_cmp(&medoid_position[cell].z))
                    == std::cmp::Ordering::Less);
        if is_better {
            medoid_dist_sq[cell] = dist_sq;
            medoid_index[cell] = i as u32;
            medoid_position[cell] = p;
        }
    }
    for (cell, vertex) in vertices.iter_mut().enumerate() {
        let source = &prim.vertices[medoid_index[cell] as usize];
        vertex.uv = source.uv;
        vertex.uv1 = source.uv1;
        vertex.tangent = source.tangent;
        vertex.color = source.color;
        vertex.joints = source.joints;
        vertex.weights = source.weights;
    }

    // Rebuild indices, dropping triangles that collapsed to a line/point.
    let mut indices = Vec::with_capacity(prim.indices.len());
    for tri in prim.indices.chunks_exact(3) {
        let (a, b, c) = (
            remap[tri[0] as usize],
            remap[tri[1] as usize],
            remap[tri[2] as usize],
        );
        if a != b && b != c && a != c {
            indices.extend_from_slice(&[a, b, c]);
        }
    }

    CpuPrimitive {
        vertices,
        indices,
        transform: prim.transform,
        node_index: prim.node_index,
        skin_index: prim.skin_index,
        material: prim.material.clone(),
        // Per-vertex morph deltas cannot survive a new vertex count; LODs render unmorphed.
        morph_targets: Vec::new(),
        morph_weights: Vec::new(),
    }
}

/// Builds a clustering LOD chain, one level per switch distance, each twice as aggressive.
pub fn build_lod_chain(prim: &CpuPrimitive, switch_distances: &[f32]) -> Vec<Lod> {
    build_lod_chain_with(prim, switch_distances, Simplifier::VertexClustering)
}

/// Which simplifier a chain uses; their ratios run opposite (cell size vs triangle budget).
/// Attributes differ too: clustering copies from the medoid, QEM blends `uv`/`color` along the edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Simplifier {
    /// Grid quantization: O(n), but loses any feature smaller than its cell.
    VertexClustering,
    /// Quadric error metrics: keeps silhouettes and creases at the same triangle budget.
    Quadric,
}

/// Builds an LOD chain with an explicit simplifier, each level twice as aggressive as the last.
pub fn build_lod_chain_with(
    prim: &CpuPrimitive,
    switch_distances: &[f32],
    simplifier: Simplifier,
) -> Vec<Lod> {
    let mut chain = Vec::with_capacity(switch_distances.len());
    // Clustering grows its cell; QEM shrinks its budget.
    let mut cluster_ratio = 0.02;
    let mut keep_fraction = 0.5;
    for &distance in switch_distances {
        let primitive = match simplifier {
            Simplifier::VertexClustering => simplify_primitive(prim, cluster_ratio),
            Simplifier::Quadric => crate::scene::qem::simplify_primitive_qem(prim, keep_fraction),
        };
        chain.push(Lod {
            primitive,
            min_distance: distance,
        });
        cluster_ratio *= 2.0;
        keep_fraction *= 0.5;
    }
    chain
}

/// LOD index for a camera distance: the last level passed, or `None` for full detail.
pub fn select_lod(chain: &[Lod], distance: f32) -> Option<usize> {
    select_lod_by_distance_iter(chain.iter().map(|lod| lod.min_distance), distance)
}

/// `select_lod` over bare switch distances, for the renderer, which drops the CPU chain.
pub fn select_lod_by_distance(min_distances: &[f32], distance: f32) -> Option<usize> {
    select_lod_by_distance_iter(min_distances.iter().copied(), distance)
}

/// Last level whose switch distance was passed; a full scan so unsorted input picks the last match.
fn select_lod_by_distance_iter(
    min_distances: impl Iterator<Item = f32>,
    distance: f32,
) -> Option<usize> {
    let mut chosen = None;
    for (i, min_distance) in min_distances.enumerate() {
        if distance >= min_distance {
            chosen = Some(i);
        }
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::CpuMaterial;
    use glam::Mat4;

    /// A dense grid mesh in the XY plane.
    fn grid_primitive(n: usize) -> CpuPrimitive {
        let mut vertices = Vec::new();
        for y in 0..n {
            for x in 0..n {
                vertices.push(Vertex {
                    position: [x as f32 / n as f32, y as f32 / n as f32, 0.0],
                    normal: [0.0, 0.0, 1.0],
                    uv: [0.0, 0.0],
                    tangent: [1.0, 0.0, 0.0, 1.0],
                    joints: [0.0; 4],
                    weights: [0.0; 4],
                    color: [1.0, 1.0, 1.0, 1.0],
                    uv1: [0.0, 0.0],
                });
            }
        }
        let mut indices = Vec::new();
        for y in 0..n - 1 {
            for x in 0..n - 1 {
                let i = (y * n + x) as u32;
                let right = i + 1;
                let down = i + n as u32;
                let diag = down + 1;
                indices.extend_from_slice(&[i, right, diag, i, diag, down]);
            }
        }
        CpuPrimitive {
            vertices,
            indices,
            transform: Mat4::IDENTITY,
            node_index: None,
            skin_index: None,
            material: CpuMaterial::default(),
            morph_targets: Vec::new(),
            morph_weights: Vec::new(),
        }
    }

    #[test]
    fn simplification_reduces_geometry() {
        let full = grid_primitive(32);
        let simplified = simplify_primitive(&full, 0.08);

        assert!(
            simplified.vertices.len() < full.vertices.len() / 2,
            "expected a big vertex reduction: {} -> {}",
            full.vertices.len(),
            simplified.vertices.len()
        );
        assert!(!simplified.indices.is_empty(), "mesh collapsed entirely");
        assert_eq!(
            simplified.indices.len() % 3,
            0,
            "indices must stay triangles"
        );
        // Every index must address a surviving vertex.
        let max_index = *simplified.indices.iter().max().unwrap() as usize;
        assert!(max_index < simplified.vertices.len());
    }

    #[test]
    fn harder_ratio_simplifies_more() {
        let full = grid_primitive(32);
        let light = simplify_primitive(&full, 0.04);
        let heavy = simplify_primitive(&full, 0.16);
        assert!(heavy.vertices.len() < light.vertices.len());
    }

    #[test]
    fn lod_selection_follows_distance() {
        let full = grid_primitive(16);
        let chain = build_lod_chain(&full, &[10.0, 50.0]);
        assert_eq!(chain.len(), 2);
        assert!(chain[1].primitive.vertices.len() <= chain[0].primitive.vertices.len());

        assert_eq!(select_lod(&chain, 1.0), None); // near: full detail
        assert_eq!(select_lod(&chain, 20.0), Some(0));
        assert_eq!(select_lod(&chain, 100.0), Some(1));
    }

    #[test]
    fn degenerate_input_is_returned_unchanged() {
        let mut prim = grid_primitive(4);
        // Collapse every vertex to one point: zero diagonal.
        for v in &mut prim.vertices {
            v.position = [0.0, 0.0, 0.0];
        }
        let simplified = simplify_primitive(&prim, 0.05);
        assert_eq!(simplified.vertices.len(), prim.vertices.len());
    }

    /// Mean distance from each original vertex to its merged vertex.
    fn mean_displacement(original: &CpuPrimitive, cell_ratio: f32) -> f32 {
        let simplified = simplify_primitive(original, cell_ratio);

        // Nearest merged vertex, so this measures the result, not the remap table.
        let mut total = 0.0f32;
        for v in &original.vertices {
            let p = Vec3::from_array(v.position);
            let nearest = simplified
                .vertices
                .iter()
                .map(|s| (Vec3::from_array(s.position) - p).length())
                .fold(f32::INFINITY, f32::min);
            total += nearest;
        }
        total / original.vertices.len() as f32
    }

    #[test]
    fn merged_vertices_sit_at_the_cell_centroid() {
        // Four vertices in one cell: the merge must be their average, not the first visited.
        let mut prim = grid_primitive(2);
        prim.vertices[0].position = [0.0, 0.0, 0.0];
        prim.vertices[1].position = [1.0, 0.0, 0.0];
        prim.vertices[2].position = [0.0, 1.0, 0.0];
        prim.vertices[3].position = [1.0, 1.0, 0.0];

        // A cell ratio large enough to swallow the whole mesh.
        let simplified = simplify_primitive(&prim, 10.0);
        assert_eq!(
            simplified.vertices.len(),
            1,
            "the whole mesh should collapse to one vertex"
        );

        let merged = Vec3::from_array(simplified.vertices[0].position);
        let expected = Vec3::new(0.5, 0.5, 0.0);
        assert!(
            (merged - expected).length() < 1e-5,
            "merged vertex at {merged:?}, expected the centroid {expected:?}"
        );
    }

    #[test]
    fn simplification_is_independent_of_vertex_order() {
        // The jitter is load-bearing: a regular grid is symmetric under reversal and hides first-wins.
        let mut original = grid_primitive(16);
        for (i, v) in original.vertices.iter_mut().enumerate() {
            let n = i as f32;
            v.position[0] += (n * 0.37).fract() * 0.01;
            v.position[1] += (n * 0.71).fract() * 0.01;
            v.position[2] += (n * 0.13).fract() * 0.01;
            // Distinct uv/joints per vertex, or a wrong medoid pick passes by coincidence.
            v.uv = [n, n * 2.0];
            v.joints = [n, 0.0, 0.0, 0.0];
            v.weights = [1.0, 0.0, 0.0, 0.0];
        }

        let mut reordered = original.clone();
        reordered.vertices.reverse();
        let last = (original.vertices.len() - 1) as u32;
        for index in &mut reordered.indices {
            *index = last - *index;
        }

        let a = simplify_primitive(&original, 0.1);
        let b = simplify_primitive(&reordered, 0.1);

        assert_eq!(
            a.vertices.len(),
            b.vertices.len(),
            "vertex order changed the simplified vertex count"
        );

        // Tolerance, not bit equality: f32 centroid sums round differently per order.
        let key = |v: &Vertex| {
            let p = v.position;
            (p[0].to_bits(), p[1].to_bits(), p[2].to_bits())
        };
        let mut sorted_a: Vec<&Vertex> = a.vertices.iter().collect();
        let mut sorted_b: Vec<&Vertex> = b.vertices.iter().collect();
        sorted_a.sort_by_key(|v| key(v));
        sorted_b.sort_by_key(|v| key(v));

        for (va, vb) in sorted_a.iter().zip(&sorted_b) {
            let pa = Vec3::from_array(va.position);
            let pb = Vec3::from_array(vb.position);
            assert!(
                (pa - pb).length() < 1e-4,
                "vertex order changed a merged position: {pa:?} vs {pb:?}"
            );

            // Medoid-copied attributes are not summed, so they must match exactly.
            assert_eq!(
                va.uv, vb.uv,
                "vertex order changed which vertex's uv was kept"
            );
            assert_eq!(
                va.joints, vb.joints,
                "vertex order changed which vertex's joints was kept"
            );
            assert_eq!(
                va.weights, vb.weights,
                "vertex order changed which vertex's weights was kept"
            );
        }
    }

    #[test]
    fn merged_vertices_take_their_skin_binding_from_the_nearest_real_vertex() {
        // One cell of vertices bound to different joints: the merge keeps the medoid's binding.
        let mut prim = grid_primitive(2);
        prim.vertices[0].position = [0.0, 0.0, 0.0];
        prim.vertices[0].joints = [1.0, 0.0, 0.0, 0.0];
        prim.vertices[0].weights = [1.0, 0.0, 0.0, 0.0];
        prim.vertices[0].uv = [0.1, 0.1];

        prim.vertices[1].position = [0.01, 0.0, 0.0];
        prim.vertices[1].joints = [2.0, 0.0, 0.0, 0.0];
        prim.vertices[1].weights = [0.0, 1.0, 0.0, 0.0];
        prim.vertices[1].uv = [0.2, 0.2];

        prim.vertices[2].position = [0.0, 0.01, 0.0];
        prim.vertices[2].joints = [3.0, 0.0, 0.0, 0.0];
        prim.vertices[2].weights = [0.0, 0.0, 1.0, 0.0];
        prim.vertices[2].uv = [0.3, 0.3];

        prim.vertices[3].position = [10.0, 10.0, 10.0];
        prim.vertices[3].joints = [4.0, 0.0, 0.0, 0.0];
        prim.vertices[3].weights = [0.0, 0.0, 0.0, 1.0];
        prim.vertices[3].uv = [0.4, 0.4];

        // The cell swallows vertices 0-2 but not the far one; vertex 0 is the medoid.
        let cell_ratio = 0.5;
        let simplified = simplify_primitive(&prim, cell_ratio);
        assert_eq!(
            simplified.vertices.len(),
            2,
            "expected the three close vertices to merge and the far one to stay separate"
        );

        let merged = simplified
            .vertices
            .iter()
            .find(|v| Vec3::from_array(v.position).length() < 1.0)
            .expect("merged cluster vertex not found");

        assert_eq!(
            merged.joints, prim.vertices[0].joints,
            "expected the medoid's (vertex 0) joints, not an average or a different vertex's"
        );
        assert_eq!(
            merged.weights, prim.vertices[0].weights,
            "expected the medoid's (vertex 0) weights"
        );

        // Reversing the input must not change the medoid.
        let mut reversed = prim.clone();
        reversed.vertices.reverse();
        let last = (prim.vertices.len() - 1) as u32;
        for index in &mut reversed.indices {
            *index = last - *index;
        }
        let simplified_reversed = simplify_primitive(&reversed, cell_ratio);
        let merged_reversed = simplified_reversed
            .vertices
            .iter()
            .find(|v| Vec3::from_array(v.position).length() < 1.0)
            .expect("merged cluster vertex not found");

        assert_eq!(
            merged_reversed.joints, merged.joints,
            "vertex order changed which vertex's joints the merge kept"
        );
        assert_eq!(
            merged_reversed.weights, merged.weights,
            "vertex order changed which vertex's weights the merge kept"
        );
        assert_eq!(
            merged_reversed.uv, merged.uv,
            "vertex order changed which vertex's uv the merge kept"
        );
    }

    #[test]
    fn displacement_stays_within_the_cell_size() {
        // Clustering's actual promise: no vertex moves further than its cell.
        let original = grid_primitive(24);
        let cell_ratio = 0.1f32;

        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for v in &original.vertices {
            let p = Vec3::from_array(v.position);
            min = min.min(p);
            max = max.max(p);
        }
        let cell = (max - min).length() * cell_ratio;

        let mean = mean_displacement(&original, cell_ratio);
        assert!(
            mean < cell,
            "mean displacement {mean} exceeds one cell ({cell}); merged vertices are leaving their cells"
        );
        assert!(
            mean > 0.0,
            "nothing moved at all - the mesh was not simplified"
        );
    }
}
