//! Every shader must parse, validate and emit SPIR-V, the path shared with the C++ Vulkan engine.

use naga::back::spv;
use naga::valid::{Capabilities, ValidationFlags, Validator};

const SHADERS: &[(&str, &str)] = &[
    ("forward", include_str!("../src/shaders/forward.wgsl")),
    ("sky", include_str!("../src/shaders/sky.wgsl")),
    ("tonemap", include_str!("../src/shaders/tonemap.wgsl")),
    ("bloom", include_str!("../src/shaders/bloom.wgsl")),
    ("ssao", include_str!("../src/shaders/ssao.wgsl")),
    ("ibl", include_str!("../src/shaders/ibl.wgsl")),
    (
        "occlusion_bbox",
        include_str!("../src/shaders/occlusion_bbox.wgsl"),
    ),
    (
        "depth_resolve",
        include_str!("../src/shaders/depth_resolve.wgsl"),
    ),
    ("gpu_cull", include_str!("../src/shaders/gpu_cull.wgsl")),
    ("histogram", include_str!("../src/shaders/histogram.wgsl")),
];

#[test]
fn all_shaders_export_to_spirv() {
    for (name, source) in SHADERS {
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|e| panic!("{name}.wgsl must parse: {e:?}"));
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        let info = validator
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}.wgsl must validate: {e:?}"));
        let words = spv::write_vec(&module, &info, &spv::Options::default(), None)
            .unwrap_or_else(|e| panic!("{name}.wgsl must emit SPIR-V: {e:?}"));

        assert!(!words.is_empty(), "{name}: empty SPIR-V");
        // SPIR-V magic number.
        assert_eq!(words[0], 0x0723_0203, "{name}: bad SPIR-V magic");
        assert!(
            !module.entry_points.is_empty(),
            "{name}: no entry points exported"
        );
    }
}

/// Every `.wgsl` in `src/shaders/` must appear in the hand-maintained `SHADERS` list.
#[test]
fn every_shader_file_is_covered() {
    let shaders_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/shaders");
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(shaders_dir)
        .unwrap_or_else(|e| panic!("failed to read {shaders_dir}: {e:?}"))
    {
        let path = entry
            .unwrap_or_else(|e| panic!("failed to read entry in {shaders_dir}: {e:?}"))
            .path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("wgsl") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_else(|| panic!("non-UTF8 shader filename: {path:?}"))
            .to_string();
        if !SHADERS.iter().any(|(name, _)| *name == stem) {
            missing.push(stem);
        }
    }
    assert!(
        missing.is_empty(),
        "src/shaders/ has .wgsl file(s) not covered by SHADERS in shader_export.rs: {missing:?}"
    );
}

/// Slang emits no comments, so a `//` in generated WGSL is a hand-edit the next regenerate drops.
/// `histogram.wgsl` is hand-written, so it is exempt.
#[test]
fn generated_wgsl_has_no_hand_edits() {
    let shaders_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/shaders");
    let mut hand_edits = Vec::new();
    for entry in std::fs::read_dir(shaders_dir)
        .unwrap_or_else(|e| panic!("failed to read {shaders_dir}: {e:?}"))
    {
        let path = entry
            .unwrap_or_else(|e| panic!("failed to read entry in {shaders_dir}: {e:?}"))
            .path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("wgsl") {
            continue;
        }
        if path.file_name().and_then(|n| n.to_str()) == Some("histogram.wgsl") {
            continue;
        }
        let contents = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {path:?}: {e:?}"));
        for (line_number, line) in contents.lines().enumerate() {
            if line.contains("//") {
                hand_edits.push(format!("{path:?}:{}: {line}", line_number + 1));
            }
        }
    }
    assert!(
        hand_edits.is_empty(),
        "generated WGSL must not be hand-edited - put the change in the .slang source, or in \
         the post-emit patch table in Build-SlangShaders.ps1 / compile-slang-shaders.sh: {hand_edits:#?}"
    );
}

/// Entry points `src/render/*.rs` names by string, which otherwise fail only at pipeline creation.
const REQUIRED_ENTRY_POINTS: &[(&str, &[&str])] = &[
    (
        "forward",
        &[
            "vs_main",
            "fs_main",
            "vs_shadow_masked",
            "fs_shadow_masked",
            "vs_shadow",
        ],
    ),
    ("sky", &["vs_main", "fs_main"]),
    ("tonemap", &["vs_main", "fs_main"]),
    (
        "bloom",
        &["vs_main", "fs_brightpass", "fs_blur_h", "fs_blur_v"],
    ),
    ("ssao", &["vs_main", "fs_ssao", "fs_blur"]),
    (
        "ibl",
        &[
            "vs_fullscreen",
            "fs_equirect_to_cube",
            "fs_downsample_cube",
            "fs_irradiance",
            "fs_prefilter",
            "fs_brdf_lut",
        ],
    ),
    ("occlusion_bbox", &["vs_main", "fs_main"]),
    ("depth_resolve", &["vs_main", "fs_main"]),
    ("gpu_cull", &["cs_main"]),
    (
        "histogram",
        &[
            "cs_build_histogram",
            "cs_reduce_exposure",
            "cs_clear_histogram",
        ],
    ),
];

#[test]
fn every_pipeline_entry_point_is_exported() {
    for (name, source) in SHADERS {
        let Some((_, required)) = REQUIRED_ENTRY_POINTS.iter().find(|(n, _)| n == name) else {
            continue;
        };
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|e| panic!("{name}.wgsl must parse: {e:?}"));
        for entry_point in *required {
            assert!(
                module.entry_points.iter().any(|ep| ep.name == *entry_point),
                "{name}.wgsl must export `{entry_point}` - a src/render/*.rs pipeline names it"
            );
        }
    }
}

/// Pins the post-emit patch to `@builtin(frag_depth)`: Slang emits a colour output wgpu drops.
#[test]
fn depth_resolve_fragment_writes_frag_depth() {
    let source = include_str!("../src/shaders/depth_resolve.wgsl");
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|e| panic!("depth_resolve.wgsl must parse: {e:?}"));
    let fs_main = module
        .entry_points
        .iter()
        .find(|ep| ep.name == "fs_main")
        .expect("depth_resolve.wgsl must export fs_main");
    let result = fs_main
        .function
        .result
        .as_ref()
        .expect("fs_main must return a value");
    let members = match &module.types[result.ty].inner {
        naga::TypeInner::Struct { members, .. } => members,
        other => panic!("fs_main result type must be a struct, got {other:?}"),
    };
    assert!(
        members.iter().any(|m| matches!(
            m.binding,
            Some(naga::Binding::BuiltIn(naga::BuiltIn::FragDepth))
        )),
        "fs_main must write @builtin(frag_depth); result members are {members:?}"
    );
    assert!(
        !members
            .iter()
            .any(|m| matches!(m.binding, Some(naga::Binding::Location { .. }))),
        "fs_main must not emit a colour @location output; result members are {members:?}"
    );
}

/// Text from `start_idx`'s first `open` to its match; naga's AST loses the identifier names.
fn extract_balanced(source: &str, start_idx: usize, open: char, close: char) -> &str {
    let mut depth = 0i32;
    let mut body_start = None;
    for (i, c) in source[start_idx..].char_indices() {
        if c == open {
            if depth == 0 {
                body_start = Some(start_idx + i);
            }
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                let start = body_start.expect("close before open");
                return &source[start..=start_idx + i];
            }
        }
    }
    panic!("unbalanced '{open}'/'{close}' starting at byte {start_idx}");
}

/// Prefix of the shared base-colour UV selector, before the compiler's mangling suffix.
const UV_SELECTOR_PREFIX: &str = "base_color_uv_select";

/// The mangled name of the UV selector `body` calls, when it is not inlined.
fn uv_selector_called_in(body: &str) -> Option<String> {
    let start = body.find(UV_SELECTOR_PREFIX)?;
    let rest = &body[start..];
    let end = rest.find('(')?;
    Some(rest[..end].trim().to_string())
}

fn fn_body<'a>(source: &'a str, fn_name: &str) -> &'a str {
    let needle = format!("fn {fn_name}(");
    let idx = source
        .find(&needle)
        .unwrap_or_else(|| panic!("`{needle}` not found in forward.wgsl"));
    extract_balanced(source, idx, '{', '}')
}

/// The masked-shadow pass alpha-tests with the forward pass's UV set and vertex alpha.
#[test]
fn shadow_masked_uses_the_forward_pass_uv_set() {
    let source = include_str!("../src/shaders/forward.wgsl");

    let vs_body = fn_body(source, "vs_shadow_masked");
    assert!(
        vs_body.contains("uv1"),
        "vs_shadow_masked must reference the uv1 input member to pick the base-colour UV set the same way fs_main does; body:\n{vs_body}"
    );

    // The selection may be inlined or a shared helper; either way it must branch on material_flags.
    if !vs_body.contains("material_flags") {
        let selector = uv_selector_called_in(vs_body).unwrap_or_else(|| {
            panic!(
                "vs_shadow_masked neither branches on material_flags itself nor calls a \
                 `{UV_SELECTOR_PREFIX}*` helper - the masked-shadow pass is no longer picking the \
                 base-colour UV set; body:\n{vs_body}"
            )
        });

        let selector_body = fn_body(source, &selector);
        assert!(
            selector_body.contains("material_flags"),
            "`{selector}`, the UV selector vs_shadow_masked calls, must branch on material_flags; body:\n{selector_body}"
        );
        assert!(
            selector_body.contains("uv1"),
            "`{selector}` must be able to return the uv1 set; body:\n{selector_body}"
        );

        // Definition plus two call sites; one would mean the passes drifted apart.
        let mentions = source.matches(&format!("{selector}(")).count();
        assert!(
            mentions >= 3,
            "`{selector}` is referenced {mentions} time(s) (definition + call sites); the masked-shadow \
             pass and the forward pass must share ONE selector, so at least two call sites are expected"
        );
    }

    let fs_body = fn_body(source, "fs_shadow_masked");
    let if_idx = fs_body.find("if(").unwrap_or_else(|| {
        panic!("fs_shadow_masked must contain the discard test; body:\n{fs_body}")
    });
    let condition = extract_balanced(fs_body, if_idx + 2, '(', ')');
    assert!(
        condition.contains("base_color"),
        "fs_shadow_masked's discard test must include the base-colour alpha factor; condition:\n{condition}"
    );
    assert!(
        condition.contains("textureSample"),
        "fs_shadow_masked's discard test must include the base-colour texture sample; condition:\n{condition}"
    );
    assert!(
        condition.contains("alpha"),
        "fs_shadow_masked's discard test must multiply in the vertex-colour alpha, matching fs_main's three-factor product; condition:\n{condition}"
    );
}
