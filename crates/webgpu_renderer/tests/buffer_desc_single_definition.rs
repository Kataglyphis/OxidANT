//! Every `wgpu::BufferDescriptor` literal lives in `render::buffer_desc`, bar two named outliers.
//! Source text only, so it needs no GPU adapter.

const NEEDLE: &str = "wgpu::BufferDescriptor {";

/// The part of `line` before the first `//`, so a comment naming the needle is not counted.
fn code_of(line: &str) -> &str {
    match line.find("//") {
        Some(comment_start) => &line[..comment_start],
        None => line,
    }
}

/// The modules scanned for `BufferDescriptor` literals outside `render::buffer_desc`.
const SOURCES: &[(&str, &str)] = &[
    (
        "render/forward.rs",
        include_str!("../src/render/forward.rs"),
    ),
    (
        "render/forward/animate.rs",
        include_str!("../src/render/forward/animate.rs"),
    ),
    (
        "render/forward/cloud_pass.rs",
        include_str!("../src/render/forward/cloud_pass.rs"),
    ),
    (
        "render/forward/culling.rs",
        include_str!("../src/render/forward/culling.rs"),
    ),
    (
        "render/forward/deferred.rs",
        include_str!("../src/render/forward/deferred.rs"),
    ),
    (
        "render/forward/environment.rs",
        include_str!("../src/render/forward/environment.rs"),
    ),
    (
        "render/forward/frame.rs",
        include_str!("../src/render/forward/frame.rs"),
    ),
    (
        "render/forward/light_grid.rs",
        include_str!("../src/render/forward/light_grid.rs"),
    ),
    (
        "render/forward/main_pass.rs",
        include_str!("../src/render/forward/main_pass.rs"),
    ),
    (
        "render/forward/placement.rs",
        include_str!("../src/render/forward/placement.rs"),
    ),
    (
        "render/forward/post.rs",
        include_str!("../src/render/forward/post.rs"),
    ),
    (
        "render/forward/primitive.rs",
        include_str!("../src/render/forward/primitive.rs"),
    ),
    (
        "render/forward/readback.rs",
        include_str!("../src/render/forward/readback.rs"),
    ),
    (
        "render/forward/shadow.rs",
        include_str!("../src/render/forward/shadow.rs"),
    ),
    (
        "render/forward/shadow_casters.rs",
        include_str!("../src/render/forward/shadow_casters.rs"),
    ),
    (
        "render/forward/sky.rs",
        include_str!("../src/render/forward/sky.rs"),
    ),
    (
        "render/forward/targets.rs",
        include_str!("../src/render/forward/targets.rs"),
    ),
    (
        "render/forward/upload.rs",
        include_str!("../src/render/forward/upload.rs"),
    ),
    (
        "render/occlusion.rs",
        include_str!("../src/render/occlusion.rs"),
    ),
    (
        "render/gpu_timing.rs",
        include_str!("../src/render/gpu_timing.rs"),
    ),
    (
        "render/tonemap.rs",
        include_str!("../src/render/tonemap.rs"),
    ),
    ("render/ibl.rs", include_str!("../src/render/ibl.rs")),
    (
        "render/histogram.rs",
        include_str!("../src/render/histogram.rs"),
    ),
    (
        "render/gpu_occlusion.rs",
        include_str!("../src/render/gpu_occlusion.rs"),
    ),
    ("render/ssao.rs", include_str!("../src/render/ssao.rs")),
    (
        "render/tracer/gpu.rs",
        include_str!("../src/render/tracer/gpu.rs"),
    ),
    (
        "render/tracer/debug.rs",
        include_str!("../src/render/tracer/debug.rs"),
    ),
    (
        "render/tracer/modes/passes.rs",
        include_str!("../src/render/tracer/modes/passes.rs"),
    ),
    (
        "render/forward/traced.rs",
        include_str!("../src/render/forward/traced.rs"),
    ),
];

#[test]
fn buffer_descriptor_literals_are_the_single_definition_or_the_two_named_outliers() {
    let buffer_desc_src = include_str!("../src/render/buffer_desc.rs");
    let definition_count: usize = buffer_desc_src
        .lines()
        .map(|line| code_of(line).matches(NEEDLE).count())
        .sum();
    assert_eq!(
        definition_count, 5,
        "expected exactly 5 wgpu::BufferDescriptor literals in render/buffer_desc.rs \
         (uniform, storage_dst, storage_src, readback, query_resolve); found {definition_count}"
    );

    let mut unmarked = Vec::new();
    for (path, contents) in SOURCES {
        for (i, line) in contents.lines().enumerate() {
            if code_of(line).contains(NEEDLE) {
                unmarked.push(format!("{path}:{}", i + 1));
            }
        }
    }

    assert_eq!(
        unmarked,
        vec![
            "render/occlusion.rs:396".to_string(),
            "render/histogram.rs:102".to_string(),
        ],
        "found unexpected wgpu::BufferDescriptor literals outside render/buffer_desc.rs - \
         route new buffer sites through the helpers, or add a genuinely new outlier here \
         if the shape does not fit"
    );

    let occlusion_block = descriptor_block(include_str!("../src/render/occlusion.rs"), NEEDLE);
    assert!(
        occlusion_block.contains("BufferUsages::VERTEX")
            && occlusion_block.contains("BufferUsages::COPY_DST"),
        "occlusion.rs's instance buffer outlier should use VERTEX | COPY_DST:\n{occlusion_block}"
    );

    let histogram_block = descriptor_block(include_str!("../src/render/histogram.rs"), NEEDLE);
    assert!(
        histogram_block.contains("BufferUsages::STORAGE")
            && histogram_block.contains("BufferUsages::COPY_SRC")
            && histogram_block.contains("BufferUsages::COPY_DST"),
        "histogram.rs's exposure state buffer outlier should use \
         STORAGE | COPY_SRC | COPY_DST:\n{histogram_block}"
    );
}

/// The first `needle { ... }` block in `src`, through its first closing brace.
fn descriptor_block<'a>(src: &'a str, needle: &str) -> &'a str {
    let start = src.find(needle).expect("needle present in source");
    let rest = &src[start..];
    let end = rest
        .find('}')
        .expect("descriptor block has a closing brace");
    &rest[..=end]
}
