//! Pins `render::clouds`' constants against the cloud shaders' literals, which no compiler links.
//! Pure CPU, so it runs where no adapter exists.

use kataglyphis_webgpu_renderer::render::clouds::{
    BOX_EXTENT_PER_SCALE, LIGHT_STEPS, MARCH_STEPS, NOISE_PERIOD, NOISE_VOLUME_EXTENT,
    NOISE_WORKGROUP_SIZE,
};

const SHARED: &str = include_str!("../../../shaders/common/volumetric_clouds.slang");
const NOISE_ENTRY: &str = include_str!("../../../shaders/clouds/cloud_noise.slang");
const NOISE_WGSL: &str = include_str!("../src/shaders/cloud_noise.wgsl");

/// The numbers in the first `(...)` after `needle`, split on commas.
fn numbers_after(source: &str, needle: &str) -> Vec<f32> {
    let start = source
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` is gone"))
        + needle.len();
    let open = start + source[start..].find('(').expect("an argument list follows");
    let close = open + source[open..].find(')').expect("the argument list closes");
    source[open + 1..close]
        .split(',')
        .filter_map(|part| part.trim().trim_end_matches('f').parse().ok())
        .collect()
}

#[test]
fn the_step_clamps_match_the_shaders() {
    let march = numbers_after(SHARED, "cloud.num_march_steps = int(clamp");
    let light = numbers_after(SHARED, "cloud.num_march_steps_to_light = int(clamp");
    assert_eq!(
        march,
        [*MARCH_STEPS.start() as f32, *MARCH_STEPS.end() as f32]
    );
    assert_eq!(
        light,
        [*LIGHT_STEPS.start() as f32, *LIGHT_STEPS.end() as f32]
    );
}

#[test]
fn the_noise_volume_extent_and_workgroup_match_the_shaders() {
    let extent = SHARED
        .split("NOISE_VOLUME_EXTENT = ")
        .nth(1)
        .and_then(|rest| rest.split(';').next())
        .and_then(|value| value.trim().parse::<f32>().ok())
        .expect("volumetric_clouds.slang declares NOISE_VOLUME_EXTENT");
    assert_eq!(extent, NOISE_VOLUME_EXTENT as f32);
    let workgroup = NOISE_WORKGROUP_SIZE.map(|n| n as f32).to_vec();
    assert_eq!(numbers_after(NOISE_ENTRY, "[numthreads"), workgroup);
    assert_eq!(numbers_after(NOISE_WGSL, "@workgroup_size"), workgroup);
    assert!(
        NOISE_WGSL.contains("texture_storage_3d<rgba16float, write>"),
        "the volume must be a write-only rgba16float storage texture, the one 3D format WebGPU writes it in"
    );
}

#[test]
fn the_box_extent_factor_matches_the_shader() {
    let line = SHARED
        .lines()
        .find(|line| line.contains("cloud.radius = "))
        .expect("cloud_unpack derives cloud.radius");
    let factor = line
        .rsplit('*')
        .next()
        .and_then(|last| last.trim().trim_end_matches(';').parse::<f32>().ok())
        .expect("cloud.radius ends in its factor");
    assert_eq!(factor, BOX_EXTENT_PER_SCALE);
}

/// The host wraps the wind's offset to `NOISE_PERIOD`, which is exact only while both noise lookups repeat within it.
#[test]
fn the_wind_wraps_at_a_period_both_noise_lookups_share() {
    let divisors = ["uvwCoarse = frac(local / ", "uvwFine = frac(local / "].map(|needle| {
        let at = SHARED
            .find(needle)
            .unwrap_or_else(|| panic!("sample_density no longer has `{needle}`"))
            + needle.len();
        SHARED[at..]
            .split(|c: char| c != '.' && !c.is_ascii_digit())
            .next()
            .and_then(|n| n.parse::<f64>().ok())
            .expect("each lookup divides by a literal")
    });
    assert_eq!(divisors[0], NOISE_PERIOD, "the coarse lookup's period");
    assert_eq!(
        NOISE_PERIOD % divisors[1],
        0.0,
        "the fine period {} must divide it",
        divisors[1]
    );
    assert!(
        SHARED.contains("position - cloud.offset - cloud.wind_offset"),
        "the wind must shift the noise lookup, not the box"
    );
}
