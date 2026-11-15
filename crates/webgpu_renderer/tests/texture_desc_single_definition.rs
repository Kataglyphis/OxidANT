//! Single-layer 2D textures go through `create_2d_texture`; other shapes carry a marker comment.
//! The marker sits on the literal's line or its body's first line, where rustfmt may move it.

const SOURCES: &[(&str, &str, bool)] = &[
    (
        "render/bloom.rs",
        include_str!("../src/render/bloom.rs"),
        false,
    ),
    (
        "render/ssao.rs",
        include_str!("../src/render/ssao.rs"),
        false,
    ),
    (
        "render/forward.rs",
        include_str!("../src/render/forward.rs"),
        false,
    ),
    (
        "render/forward/animate.rs",
        include_str!("../src/render/forward/animate.rs"),
        false,
    ),
    (
        "render/forward/culling.rs",
        include_str!("../src/render/forward/culling.rs"),
        false,
    ),
    (
        "render/forward/deferred.rs",
        include_str!("../src/render/forward/deferred.rs"),
        false,
    ),
    (
        "render/forward/environment.rs",
        include_str!("../src/render/forward/environment.rs"),
        false,
    ),
    (
        "render/forward/frame.rs",
        include_str!("../src/render/forward/frame.rs"),
        false,
    ),
    (
        "render/forward/light_grid.rs",
        include_str!("../src/render/forward/light_grid.rs"),
        false,
    ),
    (
        "render/forward/main_pass.rs",
        include_str!("../src/render/forward/main_pass.rs"),
        false,
    ),
    (
        "render/forward/placement.rs",
        include_str!("../src/render/forward/placement.rs"),
        false,
    ),
    (
        "render/forward/post.rs",
        include_str!("../src/render/forward/post.rs"),
        false,
    ),
    (
        "render/forward/primitive.rs",
        include_str!("../src/render/forward/primitive.rs"),
        false,
    ),
    (
        "render/forward/readback.rs",
        include_str!("../src/render/forward/readback.rs"),
        false,
    ),
    (
        "render/forward/shadow.rs",
        include_str!("../src/render/forward/shadow.rs"),
        false,
    ),
    (
        "render/forward/shadow_casters.rs",
        include_str!("../src/render/forward/shadow_casters.rs"),
        false,
    ),
    (
        "render/forward/sky.rs",
        include_str!("../src/render/forward/sky.rs"),
        false,
    ),
    (
        "render/forward/targets.rs",
        include_str!("../src/render/forward/targets.rs"),
        false,
    ),
    (
        "render/forward/upload.rs",
        include_str!("../src/render/forward/upload.rs"),
        false,
    ),
    ("render/ibl.rs", include_str!("../src/render/ibl.rs"), false),
    (
        "render/texture.rs",
        include_str!("../src/render/texture.rs"),
        true,
    ),
];

const NEEDLE: &str = "wgpu::TextureDescriptor {";
const MARKER: &str = "// TEXTURE_2D_SHAPE_OK:";

#[test]
fn texture_descriptor_literals_are_the_single_definition_or_marked_non_goals() {
    let mut unmarked = Vec::new();
    let mut marked_count = 0usize;
    let mut definition_count = 0usize;

    for (path, contents, is_texture_module) in SOURCES.iter().copied() {
        let lines: Vec<&str> = contents.lines().collect();
        for (i, line) in lines.iter().copied().enumerate() {
            if !line.contains(NEEDLE) {
                continue;
            }
            let marked_here = line.contains(MARKER);
            let marked_below = lines
                .get(i + 1)
                .is_some_and(|next| next.trim_start().starts_with(MARKER));
            if marked_here || marked_below {
                marked_count += 1;
                continue;
            }
            if is_texture_module {
                definition_count += 1;
                continue;
            }
            unmarked.push(format!("{path}:{}", i + 1));
        }
    }

    assert!(
        unmarked.is_empty(),
        "found longhand wgpu::TextureDescriptor literals that should go through \
         render::texture::create_2d_texture/create_2d_view, or be marked \
         `{MARKER}` if they are a genuinely different shape:\n{}",
        unmarked.join("\n")
    );
    assert_eq!(
        marked_count, 4,
        "expected exactly 4 marked non-goal TextureDescriptor literals (three cube \
         textures, one depth array); found {marked_count} - update this count if a \
         new non-goal shape is added, or investigate if one disappeared"
    );
    assert_eq!(
        definition_count, 1,
        "expected exactly 1 TextureDescriptor literal in render/texture.rs, \
         inside create_2d_texture; found {definition_count}"
    );
}
