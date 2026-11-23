//! Polygon triangulation as tinyobjloader 2 does it without earcut, so every OBJ keeps the triangles the C++
//! engine drew from it: a quad splits along its shorter diagonal, a larger polygon is ear-clipped in the plane of
//! its two widest axes, and what the clipper cannot finish is dropped.

/// The triangles of one face as corner numbers into `corners`, whose entries are positions.
pub(super) fn triangulate(corners: &[[f32; 3]]) -> Vec<[usize; 3]> {
    match corners.len() {
        0..=2 => Vec::new(),
        3 => vec![[0, 1, 2]],
        4 => quad(corners).to_vec(),
        _ => clip_ears(corners),
    }
}

fn quad(p: &[[f32; 3]]) -> [[usize; 3]; 2] {
    let squared = |a: [f32; 3], b: [f32; 3]| {
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
    };
    if squared(p[0], p[2]) < squared(p[1], p[3]) {
        [[0, 1, 2], [0, 2, 3]]
    } else {
        [[0, 1, 3], [1, 2, 3]]
    }
}

/// The two axes tinyobjloader projects onto, read off the first corner that is not straight.
fn projection_axes(p: &[[f32; 3]]) -> [usize; 2] {
    let mut axes = [1, 2];
    let n = p.len();
    for k in 0..n {
        let [v0, v1, v2] = [p[k], p[(k + 1) % n], p[(k + 2) % n]];
        let e0 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
        let e1 = [v2[0] - v1[0], v2[1] - v1[1], v2[2] - v1[2]];
        let cx = (e0[1] * e1[2] - e0[2] * e1[1]).abs();
        let cy = (e0[2] * e1[0] - e0[0] * e1[2]).abs();
        let cz = (e0[0] * e1[1] - e0[1] * e1[0]).abs();
        if cx > f32::EPSILON || cy > f32::EPSILON || cz > f32::EPSILON {
            if !(cx > cy && cx > cz) {
                axes[0] = 0;
                if cz > cx && cz > cy {
                    axes[1] = 1;
                }
            }
            break;
        }
    }
    axes
}

/// tinyobjloader's built-in ear clipper, its quirks included (its area test reads only two corners).
fn clip_ears(p: &[[f32; 3]]) -> Vec<[usize; 3]> {
    let axes = projection_axes(p);
    let xy = |corner: usize| (p[corner][axes[0]], p[corner][axes[1]]);
    let mut remaining: Vec<usize> = (0..p.len()).collect();
    let mut triangles = Vec::with_capacity(p.len() - 2);
    let mut guess = 0;
    let mut iterations_left = remaining.len();
    let mut previous_len = remaining.len();
    // tinyobjloader's own limit drops a face only after n fruitless tries per corner; this bounds a hostile one too.
    let budget = 64 * p.len();
    let mut failures = 0;
    while remaining.len() > 3 && iterations_left > 0 {
        let n = remaining.len();
        if guess >= n {
            guess -= n;
        }
        if previous_len == n {
            iterations_left -= 1;
        } else {
            previous_len = n;
            iterations_left = n;
        }
        let ind = [0, 1, 2].map(|k| remaining[(guess + k) % n]);
        let [(x0, y0), (x1, y1), (x2, y2)] = ind.map(xy);
        let cross = (x1 - x0) * (y2 - y1) - (y1 - y0) * (x2 - x1);
        let area = (x0 * y1 - y0 * x1) * 0.5;
        let contains_other = || {
            (3..n).any(|other| {
                let (tx, ty) = xy(remaining[(guess + other) % n]);
                inside([x0, x1, x2], [y0, y1, y2], tx, ty)
            })
        };
        if cross * area < 0.0 || contains_other() {
            guess += 1;
            failures += 1;
            if failures > budget {
                log::warn!(
                    "an OBJ face of {} corners found no ear in {budget} tries; its rest is dropped",
                    p.len()
                );
                break;
            }
            continue;
        }
        triangles.push(ind);
        remaining.remove((guess + 1) % n);
    }
    if let [a, b, c] = remaining[..] {
        triangles.push([a, b, c]);
    }
    triangles
}

/// W. Randolph Franklin's crossing test, as tinyobjloader's `pnpoly`.
fn inside(xs: [f32; 3], ys: [f32; 3], tx: f32, ty: f32) -> bool {
    let mut odd = false;
    let mut j = 2;
    for i in 0..3 {
        if (ys[i] > ty) != (ys[j] > ty)
            && tx < (xs[j] - xs[i]) * (ty - ys[i]) / (ys[j] - ys[i]) + xs[i]
        {
            odd = !odd;
        }
        j = i;
    }
    odd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quad_splits_along_its_shorter_diagonal() {
        let long_02 = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [3.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        assert_eq!(triangulate(&long_02), [[0, 1, 3], [1, 2, 3]]);
        let short_02 = [
            [0.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 3.0, 0.0],
        ];
        assert_eq!(triangulate(&short_02), [[0, 1, 2], [0, 2, 3]]);
    }

    #[test]
    fn a_convex_polygon_becomes_n_minus_two_triangles() {
        let hexagon: Vec<[f32; 3]> = (0..6)
            .map(|k| {
                let angle = k as f32 * std::f32::consts::TAU / 6.0;
                [angle.cos(), 0.0, angle.sin()]
            })
            .collect();
        let triangles = triangulate(&hexagon);
        assert_eq!(triangles.len(), 4);
        let mut used: Vec<usize> = triangles.iter().flatten().copied().collect();
        used.sort_unstable();
        used.dedup();
        assert_eq!(used, [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn a_concave_polygon_keeps_its_notch_empty() {
        // An L in the XY plane: a fan from corner 0 would cover the notch at (1.5, 1.5).
        let l_shape = [
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 2.0, 0.0],
            [0.0, 2.0, 0.0],
        ];
        let triangles = triangulate(&l_shape);
        assert_eq!(triangles.len(), 4);
        let area: f32 = triangles
            .iter()
            .map(|&[a, b, c]| {
                let (p, q, r) = (l_shape[a], l_shape[b], l_shape[c]);
                ((q[0] - p[0]) * (r[1] - p[1]) - (r[0] - p[0]) * (q[1] - p[1])).abs() / 2.0
            })
            .sum();
        assert!((area - 3.0).abs() < 1e-5, "the L covers 3, not {area}");
    }

    #[test]
    fn fewer_than_three_corners_draw_nothing() {
        assert!(triangulate(&[[0.0; 3], [1.0; 3]]).is_empty());
    }
}
