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

    let sources: &[(&str, &str)] = &[
        (
            "render/forward.rs",
            include_str!("../src/render/forward.rs"),
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
    ];

    let mut unmarked = Vec::new();
    for (path, contents) in sources {
        for (i, line) in contents.lines().enumerate() {
            if code_of(line).contains(NEEDLE) {
                unmarked.push(format!("{path}:{}", i + 1));
            }
        }
    }

    assert_eq!(
        unmarked,
        vec![
            "render/occlusion.rs:358".to_string(),
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
