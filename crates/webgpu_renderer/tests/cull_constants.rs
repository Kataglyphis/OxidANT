//! Pins `gpu_cull.wgsl`'s workgroup size against the Rust dispatch copy in `render::gpu_occlusion`.
//! Pure CPU, so it runs where no adapter exists.

use kataglyphis_webgpu_renderer::render::gpu_occlusion::CULL_WORKGROUP;

const SHADER_SOURCE: &str = include_str!("../src/shaders/gpu_cull.wgsl");

/// The x of the `@workgroup_size` nearest before `fn <entry_point>(` in [`SHADER_SOURCE`].
fn extract_workgroup_x(entry_point: &str) -> u32 {
    let fn_needle = format!("fn {entry_point}(");
    let fn_pos = SHADER_SOURCE
        .find(&fn_needle)
        .unwrap_or_else(|| panic!("gpu_cull.wgsl no longer declares `fn {entry_point}`"));
    let preceding = &SHADER_SOURCE[..fn_pos];

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
