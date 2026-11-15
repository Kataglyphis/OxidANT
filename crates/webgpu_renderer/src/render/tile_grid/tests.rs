use super::*;
// See render/bounds.rs's tests for why `directx` matches the old `Mat4::*_rh` constructors.
use glam::camera::rh::proj::directx as clip;
use glam::camera::rh::view::look_at_mat4;

use crate::scene::camera::OrbitCamera;
use crate::scene::viewport::SceneViewport;

#[test]
fn build_tile_light_grid_bins_lights_into_correct_tiles() {
    // Single light at origin, perspective camera at (0,0,5) looking at origin.
    let view = look_at_mat4(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
    let proj = clip::perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);

    let mut packed = [[0.0f32; 4]; MAX_PUNCTUAL_LIGHTS * 4];
    // The range spans the frustum, so only "the centre tile holds it" is checkable here.
    packed[0] = [0.0, 0.0, 0.0, 1.0]; // position + kind=point
    packed[1] = [1.0, 1.0, 1.0, 10.0]; // color*intensity + range

    let mut scratch = TileLightGridScratch::default();
    build_tile_light_grid(&mut scratch, &packed, 1, (256, 256), &view, &proj);
    // The light at origin should cover the center tile.
    let tx = 128 / TILE_SIZE; // ~8 for 16px tiles
    let ty = 128 / TILE_SIZE;
    let tile_w = 256 / TILE_SIZE; // 16
    let center_tile = (ty * tile_w + tx) as usize;
    assert!(
        scratch.grid[center_tile * 2] >= 1,
        "center tile should contain the light"
    );
    let offset = scratch.grid[center_tile * 2 + 1] as usize;
    let count = scratch.grid[center_tile * 2] as usize;
    assert!(
        scratch.indices[offset..offset + count].contains(&0),
        "center tile's light list should contain light index 0"
    );
}

#[test]
fn build_tile_light_grid_handles_zero_lights() {
    let packed = [[0.0f32; 4]; MAX_PUNCTUAL_LIGHTS * 4];
    let mut scratch = TileLightGridScratch::default();
    let id = Mat4::IDENTITY;
    build_tile_light_grid(&mut scratch, &packed, 0, (256, 256), &id, &id);
    assert!(
        scratch.grid.iter().all(|&c| c == 0),
        "all tiles should have zero lights"
    );
    assert!(!scratch.indices.is_empty(), "indices buffer should exist");
}

#[test]
fn build_tile_light_grid_skips_lights_behind_camera() {
    // Light behind the camera, with a range too short to reach in front of it.
    let view = look_at_mat4(Vec3::new(5.0, 0.0, 0.0), Vec3::ZERO, Vec3::Y);
    let proj = clip::perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);

    let mut packed = [[0.0f32; 4]; MAX_PUNCTUAL_LIGHTS * 4];
    packed[0] = [10.0, 0.0, 0.0, 1.0];
    packed[1] = [1.0, 1.0, 1.0, 2.0];

    let mut scratch = TileLightGridScratch::default();
    build_tile_light_grid(&mut scratch, &packed, 1, (256, 256), &view, &proj);
    for chunk in scratch.grid.chunks(2) {
        assert_eq!(chunk[0], 0, "no light should reach any tile");
    }
}

#[test]
fn build_tile_light_grid_covers_tiles_within_range() {
    // Range spills past the centre tile but stays a proper subset, so the rectangle is checkable.
    let view = look_at_mat4(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
    let proj = clip::perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);

    let mut packed = [[0.0f32; 4]; MAX_PUNCTUAL_LIGHTS * 4];
    packed[0] = [0.0, 0.0, 0.0, 1.0];
    packed[1] = [1.0, 1.0, 1.0, 1.0]; // range = 1.0

    let mut scratch = TileLightGridScratch::default();
    build_tile_light_grid(&mut scratch, &packed, 1, (256, 256), &view, &proj);

    let tile_w = 256 / TILE_SIZE;
    let lit_tiles: Vec<(u32, u32)> = (0..tile_w)
        .flat_map(|ty| (0..tile_w).map(move |tx| (tx, ty)))
        .filter(|&(tx, ty)| scratch.grid[((ty * tile_w + tx) * 2) as usize] > 0)
        .collect();

    assert!(
        lit_tiles.len() > 1,
        "a light with nonzero range should cover more than one tile, got {}",
        lit_tiles.len()
    );

    let min_tx = lit_tiles.iter().map(|&(tx, _)| tx).min().unwrap();
    let max_tx = lit_tiles.iter().map(|&(tx, _)| tx).max().unwrap();
    let min_ty = lit_tiles.iter().map(|&(_, ty)| ty).min().unwrap();
    let max_ty = lit_tiles.iter().map(|&(_, ty)| ty).max().unwrap();
    let expected_count = ((max_tx - min_tx + 1) * (max_ty - min_ty + 1)) as usize;
    assert_eq!(
        lit_tiles.len(),
        expected_count,
        "lit tiles should form a contiguous rectangle, got {:?}",
        lit_tiles
    );

    let center_tx = 128 / TILE_SIZE;
    let center_ty = 128 / TILE_SIZE;
    assert!(
        min_tx <= center_tx && center_tx <= max_tx,
        "rectangle should surround the centre tile"
    );
    assert!(
        min_ty <= center_ty && center_ty <= max_ty,
        "rectangle should surround the centre tile"
    );
}

#[test]
fn build_tile_light_grid_includes_directional_lights_in_every_tile() {
    let mut packed = [[0.0f32; 4]; MAX_PUNCTUAL_LIGHTS * 4];
    packed[0] = [0.0, -1.0, 0.0, 3.0]; // kind = directional; position unused
    packed[1] = [1.0, 1.0, 1.0, 0.0]; // color*intensity + range (unused)

    let mut scratch = TileLightGridScratch::default();
    let id = Mat4::IDENTITY;
    build_tile_light_grid(&mut scratch, &packed, 1, (256, 256), &id, &id);

    let total_tiles = scratch.grid.len() / 2;
    for t in 0..total_tiles {
        assert!(
            scratch.grid[t * 2] >= 1,
            "tile {} missing the directional light",
            t
        );
    }
}

#[test]
fn build_tile_light_grid_bins_a_light_into_the_row_the_shader_reads() {
    // Small-range light above centre, so its rect stays in the upper half.
    let view = look_at_mat4(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
    let proj = clip::perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
    let vp = proj * view;

    let mut packed = [[0.0f32; 4]; MAX_PUNCTUAL_LIGHTS * 4];
    packed[0] = [0.0, 1.5, 0.0, 1.0]; // position above centre + kind=point
    packed[1] = [1.0, 1.0, 1.0, 0.3]; // color*intensity + small range

    let mut scratch = TileLightGridScratch::default();
    build_tile_light_grid(&mut scratch, &packed, 1, (256, 256), &view, &proj);

    // Expected tile as the shader computes it: fragCoord is y down, 0.0 = top.
    let clip = vp * glam::Vec4::new(0.0, 1.5, 0.0, 1.0);
    let ndc = clip.xy() / clip.w;
    let uv_y = 0.5 - (ndc.y) * 0.5;
    let ty = ((uv_y * 256.0) as u32).min(255) / TILE_SIZE;
    let tile_w = 256 / TILE_SIZE;

    let mut found = false;
    for tx in 0..tile_w {
        let idx = (ty * tile_w + tx) as usize;
        let count = scratch.grid[idx * 2] as usize;
        if count == 0 {
            continue;
        }
        let offset = scratch.grid[idx * 2 + 1] as usize;
        if scratch.indices[offset..offset + count].contains(&0) {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "the row the shader reads for this light should contain it"
    );
    assert!(
        ty < 8,
        "a light above centre should land in the upper half, got ty={ty}"
    );
}

#[test]
fn build_tile_light_grid_keeps_two_lights_in_their_own_halves() {
    let view = look_at_mat4(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
    let proj = clip::perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);

    let mut packed = [[0.0f32; 4]; MAX_PUNCTUAL_LIGHTS * 4];
    packed[0] = [0.0, 1.5, 0.0, 1.0]; // above centre
    packed[1] = [1.0, 1.0, 1.0, 0.3];
    packed[4] = [0.0, -1.5, 0.0, 1.0]; // below centre
    packed[5] = [1.0, 1.0, 1.0, 0.3];

    let mut scratch = TileLightGridScratch::default();
    build_tile_light_grid(&mut scratch, &packed, 2, (256, 256), &view, &proj);

    let tile_w = 256 / TILE_SIZE;
    let tile_h = 256 / TILE_SIZE;

    let tile_contains = |ty: u32, tx: u32, light: u32| -> bool {
        let idx = (ty * tile_w + tx) as usize;
        let count = scratch.grid[idx * 2] as usize;
        if count == 0 {
            return false;
        }
        let offset = scratch.grid[idx * 2 + 1] as usize;
        scratch.indices[offset..offset + count].contains(&light)
    };

    // Each light must appear in its own half and never leak into the other.
    let light0_in_upper = (0..8).any(|ty| (0..tile_w).any(|tx| tile_contains(ty, tx, 0)));
    let light0_in_lower = (8..tile_h).any(|ty| (0..tile_w).any(|tx| tile_contains(ty, tx, 0)));
    let light1_in_lower = (8..tile_h).any(|ty| (0..tile_w).any(|tx| tile_contains(ty, tx, 1)));
    let light1_in_upper = (0..8).any(|ty| (0..tile_w).any(|tx| tile_contains(ty, tx, 1)));

    assert!(
        light0_in_upper,
        "light above centre should be in the upper half"
    );
    assert!(
        !light0_in_lower,
        "light above centre should not leak into the lower half"
    );
    assert!(
        light1_in_lower,
        "light below centre should be in the lower half"
    );
    assert!(
        !light1_in_upper,
        "light below centre should not leak into the upper half"
    );
}

#[test]
fn build_tile_light_grid_keeps_offscreen_but_overlapping_lights() {
    // Centre outside the frustum, but the range reaches back onto the screen.
    let view = look_at_mat4(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
    let proj = clip::perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);

    let mut packed = [[0.0f32; 4]; MAX_PUNCTUAL_LIGHTS * 4];
    packed[0] = [3.0, 0.0, 0.0, 1.0]; // centre projects outside [0,1]
    packed[1] = [1.0, 1.0, 1.0, 5.0]; // range reaches back onto the screen

    let mut scratch = TileLightGridScratch::default();
    build_tile_light_grid(&mut scratch, &packed, 1, (256, 256), &view, &proj);

    let any_lit = scratch.grid.chunks(2).any(|c| c[0] > 0);
    assert!(
        any_lit,
        "a light whose range reaches the screen must light at least one tile even though its centre is off-screen"
    );
}

/// The light indices `scratch` lists for tile (`tx`, `ty`) of a `tile_w`-wide grid; none when it overflows.
fn tile_lights(scratch: &TileLightGridScratch, tile_w: u32, tx: u32, ty: u32) -> &[u32] {
    let idx = (ty * tile_w + tx) as usize;
    let count = listed(scratch.grid[idx * 2]) as usize;
    let offset = scratch.grid[idx * 2 + 1] as usize;
    &scratch.indices[offset..offset + count]
}

/// Whether the shader shades `light` in tile (`tx`, `ty`): listed there, or the tile overflows.
fn tile_shades(
    scratch: &TileLightGridScratch,
    tile_w: u32,
    (tx, ty): (u32, u32),
    light: u32,
) -> bool {
    scratch.grid[((ty * tile_w + tx) * 2) as usize] > MAX_LIGHTS_PER_TILE
        || tile_lights(scratch, tile_w, tx, ty).contains(&light)
}

#[test]
fn a_range_less_point_light_reaches_every_tile_beside_a_directional_one() {
    let view = look_at_mat4(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
    let proj = clip::perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);

    let mut packed = [[0.0f32; 4]; MAX_PUNCTUAL_LIGHTS * 4];
    packed[0] = [0.0, -1.0, 0.0, 3.0]; // directional
    packed[1] = [1.0, 1.0, 1.0, 0.0];
    packed[4] = [0.0, 0.0, 0.0, 1.0]; // point in front of the camera
    packed[5] = [1.0, 1.0, 1.0, 0.0]; // range 0: unbounded
    packed[8] = [0.0, 0.0, 9.0, 1.0]; // point behind the camera
    packed[9] = [1.0, 1.0, 1.0, 0.0];

    let mut scratch = TileLightGridScratch::default();
    build_tile_light_grid(&mut scratch, &packed, 3, (256, 256), &view, &proj);

    let tile_w = 256 / TILE_SIZE;
    for ty in 0..tile_w {
        for tx in 0..tile_w {
            let lights = tile_lights(&scratch, tile_w, tx, ty);
            assert!(
                lights.contains(&1) && lights.contains(&2),
                "tile ({tx}, {ty}) lists {lights:?}: both unbounded point lights reach it"
            );
        }
    }
}

#[test]
fn an_overflowing_tile_is_marked_to_shade_every_light_instead_of_dropping_some() {
    let view = look_at_mat4(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
    let proj = clip::perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);

    // 40 small lights around the centre plus one far off to the side, which few tiles see.
    let light_count = MAX_LIGHTS_PER_TILE as usize + 9;
    let mut packed = [[0.0f32; 4]; MAX_PUNCTUAL_LIGHTS * 4];
    for i in 0..light_count - 1 {
        packed[i * 4] = [(i % 5) as f32 * 0.01, (i / 5) as f32 * 0.01, 0.0, 1.0];
        packed[i * 4 + 1] = [1.0, 1.0, 1.0, 0.2];
    }
    packed[(light_count - 1) * 4] = [1.4, 1.4, 0.0, 1.0];
    packed[(light_count - 1) * 4 + 1] = [1.0, 1.0, 1.0, 0.1];

    let mut scratch = TileLightGridScratch::default();
    build_tile_light_grid(
        &mut scratch,
        &packed,
        light_count as u32,
        (256, 256),
        &view,
        &proj,
    );

    let tile_w = 256 / TILE_SIZE;
    let centre = (8 * tile_w + 8) as usize;
    assert_eq!(
        scratch.grid[centre * 2],
        light_count as u32 - 1,
        "the centre tile must count every light that overlaps it, past MAX_LIGHTS_PER_TILE"
    );
    let next_offset = scratch.grid[centre * 2 + 3];
    assert_eq!(
        next_offset,
        scratch.grid[centre * 2 + 1],
        "an overflowing tile must list nothing, not a truncated list"
    );
    let corner_light = (light_count - 1) as u32;
    let lit_by_corner_light = (0..tile_w)
        .flat_map(|ty| (0..tile_w).map(move |tx| (tx, ty)))
        .any(|(tx, ty)| tile_lights(&scratch, tile_w, tx, ty) == [corner_light]);
    assert!(
        lit_by_corner_light,
        "a tile only the side light overlaps must still list it alone"
    );
}

/// SplitMix64, so the random scenes below are the same on every run.
struct Rng(u64);

impl Rng {
    fn unit(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    fn direction(&mut self) -> Vec3 {
        loop {
            let v = Vec3::new(
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
            );
            if (1e-3..=1.0).contains(&v.length_squared()) {
                return v.normalize();
            }
        }
    }
}

/// A random orbit camera, a docked viewport every other trial, and a random frame size.
fn random_camera(rng: &mut Rng, trial: u32) -> (OrbitCamera, u32, u32) {
    let mut camera = OrbitCamera {
        target: rng.direction() * rng.range(0.0, 3.0),
        radius: rng.range(1.0, 25.0),
        yaw_deg: rng.range(0.0, 360.0),
        pitch_deg: rng.range(-80.0, 80.0),
        fov_y_deg: rng.range(20.0, 100.0),
        ..OrbitCamera::default()
    };
    if trial % 2 == 1 {
        let (x0, y0) = (rng.range(0.0, 0.3), rng.range(0.0, 0.3));
        camera.viewport = SceneViewport {
            x0,
            y0,
            x1: rng.range(x0 + 0.3, 1.0),
            y1: rng.range(y0 + 0.3, 1.0),
        };
    }
    let width = rng.range(17.0, 400.0) as u32;
    let height = rng.range(17.0, 400.0) as u32;
    (camera, width, height)
}

/// The tile `p` shades in, as the shader picks it from `@builtin(position)`, or `None` off screen.
fn shading_tile(camera: &OrbitCamera, width: u32, height: u32, p: Vec3) -> Option<(u32, u32)> {
    let aspect = width as f32 / height as f32;
    let depth = -(camera.view().transform_point3(p)).z;
    let clip = camera.view_projection(aspect) * p.extend(1.0);
    let ndc = clip.xyz() / clip.w;
    if depth <= camera.near || ndc.z > 1.0 || ndc.x.abs() >= 1.0 || ndc.y.abs() >= 1.0 {
        return None;
    }
    let px = ((ndc.x * 0.5 + 0.5) * width as f32) as u32;
    let py = ((0.5 - ndc.y * 0.5) * height as f32) as u32;
    Some((
        px.min(width - 1) / TILE_SIZE,
        py.min(height - 1) / TILE_SIZE,
    ))
}

fn bin_for_camera(
    scratch: &mut TileLightGridScratch,
    packed: &[[f32; 4]; MAX_PUNCTUAL_LIGHTS * 4],
    light_count: u32,
    (camera, width, height): (&OrbitCamera, u32, u32),
) {
    let aspect = width as f32 / height as f32;
    build_tile_light_grid(
        scratch,
        packed,
        light_count,
        (width, height),
        &camera.view(),
        &camera.projection(aspect),
    );
}

#[test]
fn every_point_a_light_reaches_shades_in_a_tile_that_lists_it() {
    // At most MAX_LIGHTS_PER_TILE lights, so no tile overflows and every reached tile must list the light.
    const LIGHTS: usize = MAX_LIGHTS_PER_TILE as usize;
    const SAMPLES_PER_LIGHT: usize = 64;
    let mut rng = Rng(0x5EED_1165);
    let (mut checked, mut missed) = (0usize, 0usize);
    let mut first_miss = None;
    for trial in 0..300 {
        let (camera, width, height) = random_camera(&mut rng, trial);
        let mut packed = [[0.0f32; 4]; MAX_PUNCTUAL_LIGHTS * 4];
        for i in 0..LIGHTS {
            let pos = camera.target + rng.direction() * rng.range(0.0, 20.0);
            let range = 10f32.powf(rng.range(-2.0, 1.7));
            let kind = if rng.unit() < 0.5 { 1.0 } else { 2.0 };
            packed[i * 4] = [pos.x, pos.y, pos.z, kind];
            packed[i * 4 + 1] = [1.0, 1.0, 1.0, range];
        }
        let mut scratch = TileLightGridScratch::default();
        bin_for_camera(
            &mut scratch,
            &packed,
            LIGHTS as u32,
            (&camera, width, height),
        );
        let tile_w = width.div_ceil(TILE_SIZE);
        for i in 0..LIGHTS {
            let pos = Vec3::from_slice(&packed[i * 4][..3]);
            let range = packed[i * 4 + 1][3];
            for _ in 0..SAMPLES_PER_LIGHT {
                // Mostly near the surface, where the silhouette's tangent points are.
                let reach = range * 0.999 * (1.0 - 0.3 * rng.unit().powi(3));
                let p = pos + rng.direction() * reach;
                let Some((tx, ty)) = shading_tile(&camera, width, height, p) else {
                    continue;
                };
                checked += 1;
                if !tile_shades(&scratch, tile_w, (tx, ty), i as u32) {
                    missed += 1;
                    first_miss.get_or_insert((trial, i, range, tx, ty));
                }
            }
        }
    }
    assert_eq!(
        missed, 0,
        "{missed} of {checked} on-screen points inside a light's range shade in a tile that does not list it; \
         first (trial, light, range, tile x, tile y): {first_miss:?}"
    );
    assert!(checked > 100_000, "only {checked} samples landed on screen");
}

#[test]
fn a_sphere_is_bound_by_its_tangents_not_its_axis_points() {
    // Tangents reach 0.4928 NDC (pixels 63.9..192.1 padded); the ±range axis points reached 0.4828 (66.2..189.8).
    let view = look_at_mat4(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
    let proj = clip::perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);

    let mut packed = [[0.0f32; 4]; MAX_PUNCTUAL_LIGHTS * 4];
    packed[0] = [0.0, 0.0, 0.0, 1.0];
    packed[1] = [1.0, 1.0, 1.0, 1.0];

    let mut scratch = TileLightGridScratch::default();
    build_tile_light_grid(&mut scratch, &packed, 1, (256, 256), &view, &proj);

    assert_eq!(scratch.rects[0], Some((3, 3, 12, 12)));
}

#[test]
fn a_light_straddling_the_near_plane_covers_the_whole_grid() {
    // The +Z candidate (z = 6.9) is behind the eye at z = 5 while the centre is in front.
    let view = look_at_mat4(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
    let proj = clip::perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);

    let mut packed = [[0.0f32; 4]; MAX_PUNCTUAL_LIGHTS * 4];
    packed[0] = [0.0, 0.0, 4.9, 1.0]; // position + kind=point
    packed[1] = [1.0, 1.0, 1.0, 2.0]; // color*intensity + range

    let mut scratch = TileLightGridScratch::default();
    build_tile_light_grid(&mut scratch, &packed, 1, (256, 256), &view, &proj);

    for chunk in scratch.grid.chunks(2) {
        assert!(
            chunk[0] >= 1,
            "every tile should list the near-plane-straddling light"
        );
    }
}
