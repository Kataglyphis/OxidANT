//! Pins the cull shaders' workgroup sizes and the draw cull's layouts against their Rust copies.
//! Pure CPU, so it runs where no adapter exists.

use kataglyphis_webgpu_renderer::render::draw_cull::{
    DrawInput, ARGS_BYTES, DRAW_CULL_WORKGROUP, MAX_CULL_CASCADES,
};
use kataglyphis_webgpu_renderer::render::gpu_occlusion::CULL_WORKGROUP;

const SHADER_SOURCE: &str = include_str!("../src/shaders/gpu_cull.wgsl");
const DRAW_CULL_SOURCE: &str = include_str!("../src/shaders/draw_cull.wgsl");

/// The x of the `@workgroup_size` nearest before `fn <entry_point>(` in [`SHADER_SOURCE`].
fn extract_workgroup_x(entry_point: &str) -> u32 {
    workgroup_x_in(SHADER_SOURCE, entry_point)
}

/// The x of the `@workgroup_size` nearest before `fn <entry_point>(` in `source`.
fn workgroup_x_in(source: &str, entry_point: &str) -> u32 {
    let fn_needle = format!("fn {entry_point}(");
    let fn_pos = source
        .find(&fn_needle)
        .unwrap_or_else(|| panic!("the shader no longer declares `fn {entry_point}`"));
    let preceding = &source[..fn_pos];

    let attr_needle = "@workgroup_size(";
    let attr_pos = preceding
        .rfind(attr_needle)
        .unwrap_or_else(|| panic!("no @workgroup_size(...) precedes `fn {entry_point}`"));
    let args_start = &preceding[attr_pos + attr_needle.len()..];
    let close = args_start
        .find(')')
        .unwrap_or_else(|| panic!("unterminated @workgroup_size(...) before `fn {entry_point}`"));

    args_start[..close]
        .split(',')
        .next()
        .unwrap_or_else(|| panic!("empty @workgroup_size(...) before `fn {entry_point}`"))
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("non-numeric @workgroup_size x for `fn {entry_point}`"))
}

#[test]
fn cull_workgroup_size_matches_the_shader() {
    let shader_value = extract_workgroup_x("cs_main");
    assert_eq!(
        shader_value, CULL_WORKGROUP,
        "gpu_cull.wgsl's cs_main @workgroup_size ({shader_value}) disagrees with \
         render::gpu_occlusion::CULL_WORKGROUP ({CULL_WORKGROUP}) - edit both together"
    );
}

#[test]
fn draw_cull_workgroup_size_matches_the_shader() {
    assert_eq!(
        workgroup_x_in(DRAW_CULL_SOURCE, "cs_main"),
        DRAW_CULL_WORKGROUP,
        "draw_cull.wgsl's cs_main @workgroup_size disagrees with render::draw_cull::DRAW_CULL_WORKGROUP"
    );
}

#[test]
fn draw_cull_layouts_match_the_shader() {
    // Five u32 words per draw, the stride `write_args` uses.
    assert_eq!(ARGS_BYTES, 20);
    assert!(
        DRAW_CULL_SOURCE.contains("slot_0 * u32(5)"),
        "write_args' stride moved"
    );
    // Two vec4 bounds, then eight u32: the std430 struct the shader reads.
    assert_eq!(std::mem::size_of::<DrawInput>(), 64);
    let planes = format!(
        "cascade_planes_0 : array<vec4<f32>, i32({})>",
        6 * MAX_CULL_CASCADES
    );
    assert!(
        DRAW_CULL_SOURCE.contains(&planes),
        "the views uniform holds another cascade count"
    );
}
