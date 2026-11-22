//! Pure-CPU shadow-cascade fitting: the C++ `computeCascadeDataInto`, one texel-stabilized box per view-frustum slice.
//! Splits are view depths along the camera's forward axis, which `forward_lighting.slang`'s `shadow_visibility` selects by.

// glam's `directx` clip convention is NDC Z in [0,1] with Y up, the same as wgpu's.
use glam::camera::rh::proj::directx as clip;
use glam::camera::rh::view::look_at_mat4;
use glam::{Mat4, Vec3};

use crate::render::forward::CASCADE_COUNT;
use crate::render::shadow_settings::ShadowSettings;
use crate::scene::camera::OrbitCamera;

/// The C++ `snapZPadding`: room along the light in front of and behind every slice.
pub(crate) const Z_PADDING: f32 = 10.0;

/// The camera as the fit reads it: the lens of the free rect, never the docks' remap.
#[derive(Clone, Copy, Debug)]
pub struct CascadeLens {
    /// World to view.
    pub view: Mat4,
    pub fov_y_deg: f32,
    /// The free rect's width over height.
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
}

impl CascadeLens {
    /// `camera` drawn into a frame of `frame_aspect`, as the C++ fits its scene rect.
    pub(crate) fn of(camera: &OrbitCamera, frame_aspect: f32) -> Self {
        Self {
            view: camera.view(),
            fov_y_deg: camera.fov_y_deg,
            aspect: camera.viewport.sanitized().aspect(frame_aspect),
            near: camera.near,
            far: camera.far,
        }
    }
}

/// One entry per cascade; past the settings' count the last fitted one repeats.
pub struct CascadeFit {
    /// Each cascade's far end as a view depth.
    pub splits: [f32; CASCADE_COUNT],
    pub matrices: [Mat4; CASCADE_COUNT],
    /// The C++ box's depth range over this one's, so a bias in its NDC keeps its world size here.
    pub bias_scales: [f32; CASCADE_COUNT],
    /// The camera's forward axis, along which `splits` are measured.
    pub view_dir: Vec3,
}

/// One light matrix per slice of the camera frustum up to the shadow distance.
pub fn fit_cascades(
    lens: &CascadeLens,
    light_dir: Vec3,
    settings: &ShadowSettings,
    scene_bounds: Option<(Vec3, Vec3)>,
) -> CascadeFit {
    let settings = settings.sanitized();
    let count = settings.cascade_count as usize;
    let edges = split_distances(
        count,
        (lens.near, lens.far),
        settings.distance,
        settings.split_lambda,
    );
    let camera_to_world = lens.view.inverse();
    let basis = light_basis(light_dir);
    // Every caster lies in the scene box, so its corner nearest the light bounds them all.
    let nearest_caster = scene_bounds.map_or(f32::INFINITY, |(min, max)| {
        nearest_light_depth(basis, min, max)
    });
    let tan_half = (lens.fov_y_deg.clamp(1.0, 179.0).to_radians() * 0.5).tan();
    let mut fit = CascadeFit {
        splits: [edges[count]; CASCADE_COUNT],
        matrices: [Mat4::IDENTITY; CASCADE_COUNT],
        bias_scales: [1.0; CASCADE_COUNT],
        view_dir: camera_to_world
            .transform_vector3(Vec3::NEG_Z)
            .normalize_or(Vec3::NEG_Z),
    };
    for cascade in 0..CASCADE_COUNT {
        let slice = cascade.min(count - 1);
        let corners = slice_corners(
            camera_to_world,
            (tan_half, lens.aspect.max(1e-6)),
            edges[slice],
            edges[slice + 1],
        );
        let (matrix, bias_scale) =
            stabilized_light_matrix(basis, &corners, settings.resolution, nearest_caster);
        fit.splits[cascade] = edges[slice + 1];
        fit.matrices[cascade] = matrix;
        fit.bias_scales[cascade] = bias_scale;
    }
    fit
}

/// The C++ `cascadeSplit(i)` for i in 0..=count: both ends exact, lambda blending log and uniform between.
pub(crate) fn split_distances(
    count: usize,
    (near, far): (f32, f32),
    distance: f32,
    lambda: f32,
) -> [f32; CASCADE_COUNT + 1] {
    let far = if far.is_finite() && far > 0.0 {
        far
    } else {
        1.0
    };
    let shadow_far = if distance > 0.0 {
        distance.min(far)
    } else {
        far
    };
    let shadow_near = near.min(shadow_far * 0.5).max(shadow_far * 1e-6);
    let lambda = lambda.clamp(0.0, 1.0);
    let mut edges = [shadow_far; CASCADE_COUNT + 1];
    edges[0] = shadow_near;
    for (i, edge) in edges.iter_mut().enumerate().take(count).skip(1) {
        let p = i as f32 / count as f32;
        let log_split = shadow_near * (shadow_far / shadow_near).powf(p);
        let uniform_split = shadow_near + (shadow_far - shadow_near) * p;
        *edge = lambda * log_split + (1.0 - lambda) * uniform_split;
    }
    edges
}

/// The eight world-space corners of the camera frustum between view depths `near` and `far`.
fn slice_corners(
    camera_to_world: Mat4,
    (tan_half, aspect): (f32, f32),
    near: f32,
    far: f32,
) -> [Vec3; 8] {
    std::array::from_fn(|corner| {
        let depth = if corner & 4 == 0 { near } else { far };
        let x = if corner & 1 == 0 { -1.0 } else { 1.0 };
        let y = if corner & 2 == 0 { -1.0 } else { 1.0 };
        let half_height = depth * tan_half;
        let in_view = Vec3::new(x * half_height * aspect, y * half_height, -depth);
        camera_to_world.transform_point3(in_view)
    })
}

/// A world-fixed light view: anchored at the origin, so camera motion moves boxes by whole texels.
fn light_basis(light_dir: Vec3) -> Mat4 {
    let toward_light = light_dir.normalize_or(Vec3::Y);
    let up = if toward_light.dot(Vec3::Y).abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    // Eye at +light_dir, not -: ours points toward the light, the C++ `lightDir` along the ray.
    look_at_mat4(toward_light, Vec3::ZERO, up)
}

/// The smallest light-view depth over the scene box's corners: the scene's point nearest the light.
fn nearest_light_depth(light_basis: Mat4, scene_min: Vec3, scene_max: Vec3) -> f32 {
    (0..8)
        .map(|corner| {
            Vec3::select(
                glam::BVec3::new(corner & 1 != 0, corner & 2 != 0, corner & 4 != 0),
                scene_max,
                scene_min,
            )
        })
        .map(|p| -light_basis.transform_point3(p).z)
        .fold(f32::INFINITY, f32::min)
}

/// The slice's bounding sphere as an ortho box snapped to whole texels, and its bias scale.
fn stabilized_light_matrix(
    light_basis: Mat4,
    corners: &[Vec3; 8],
    resolution: u32,
    nearest_caster: f32,
) -> (Mat4, f32) {
    let center = corners.iter().copied().sum::<Vec3>() / corners.len() as f32;
    let radius = corners
        .iter()
        .map(|corner| corner.distance(center))
        .fold(1e-4, f32::max);
    // One texel of padding, so the snap and the projection share a grid; size <= 2 cannot be padded.
    let (half_extent, texel_world) = if resolution > 2 {
        let half_extent = radius * resolution as f32 / (resolution as f32 - 2.0);
        (half_extent, 2.0 * half_extent / resolution as f32)
    } else {
        (radius, 0.0)
    };
    let center_ls = light_basis.transform_point3(center);
    let snap = |value: f32| {
        if texel_world > 0.0 {
            (value / texel_world).floor() * texel_world
        } else {
            value
        }
    };
    let (snapped_x, snapped_y) = (snap(center_ls.x), snap(center_ls.y));
    let (min_z, max_z) = corners
        .iter()
        .map(|corner| light_basis.transform_point3(*corner).z)
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), z| {
            (lo.min(z), hi.max(z))
        });
    let near = -max_z - Z_PADDING;
    let mut far = -min_z + Z_PADDING;
    if far <= near {
        far = near + 1.0;
    }
    // WebGPU core has no depth clamp to pancake casters in front of the box, so the near plane reaches them.
    let reach = if nearest_caster.is_finite() {
        near.min(nearest_caster - Z_PADDING)
    } else {
        near
    };
    let projection = clip::orthographic(
        snapped_x - half_extent,
        snapped_x + half_extent,
        snapped_y - half_extent,
        snapped_y + half_extent,
        reach,
        far,
    );
    (projection * light_basis, (far - near) / (far - reach))
}

#[cfg(test)]
mod tests;
