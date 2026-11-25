//! Every GPU-bound `Pod` struct against the struct naga lays out from its generated WGSL, so a Rust or shader edit
//! that moves a field fails a test instead of shading with the wrong bytes.

use std::path::Path;

/// Offsets and sizes of a Rust struct's fields, padding fields (`_`-prefixed) left out.
pub(crate) struct RustLayout {
    pub(crate) name: &'static str,
    pub(crate) size: usize,
    pub(crate) fields: Vec<(&'static str, usize, usize)>,
}

/// The layout of `$ty`, whose fields are listed in declaration order.
macro_rules! pod_layout {
    ($ty:ident { $($field:ident),+ $(,)? }) => {{
        let value = <$ty as bytemuck::Zeroable>::zeroed();
        $crate::render::pod_layout::RustLayout {
            name: stringify!($ty),
            size: std::mem::size_of::<$ty>(),
            fields: vec![$((
                stringify!($field),
                std::mem::offset_of!($ty, $field),
                std::mem::size_of_val(&value.$field),
            )),+],
        }
    }};
}
pub(crate) use pod_layout;

/// The generated WGSL `file` under `src/shaders`.
pub(crate) fn wgsl(file: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/shaders")
        .join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

/// Panics unless `rust` has `wgsl_struct`'s span and, padding aside, its members' offsets and sizes in order.
pub(crate) fn assert_matches_wgsl(rust: &RustLayout, wgsl: &str, wgsl_struct: &str) {
    let module = naga::front::wgsl::parse_str(wgsl).expect("generated WGSL parses");
    let mut layouter = naga::proc::Layouter::default();
    layouter
        .update(module.to_ctx())
        .expect("naga lays the module out");
    let (_, ty) = module
        .types
        .iter()
        .find(|(_, ty)| ty.name.as_deref() == Some(wgsl_struct))
        .unwrap_or_else(|| panic!("no struct {wgsl_struct} for {}", rust.name));
    let naga::TypeInner::Struct { members, span } = &ty.inner else {
        panic!("{wgsl_struct} is no struct");
    };
    let shader: Vec<(String, usize, usize)> = members
        .iter()
        .map(|member| {
            let name = member.name.clone().unwrap_or_default();
            let size = layouter[member.ty].size as usize;
            (name, member.offset as usize, size)
        })
        .filter(|(name, _, _)| !name.starts_with("pad"))
        .collect();
    let host: Vec<(&str, usize, usize)> = rust
        .fields
        .iter()
        .copied()
        .filter(|(name, _, _)| !name.starts_with('_'))
        .collect();
    let pairs: Vec<String> = host
        .iter()
        .zip(&shader)
        .filter(|(h, s)| h.1 != s.1 || h.2 != s.2)
        .map(|(h, s)| {
            format!(
                "{} at {} ({} B) vs {} at {} ({} B)",
                h.0, h.1, h.2, s.0, s.1, s.2
            )
        })
        .collect();
    assert_eq!(
        host.len(),
        shader.len(),
        "{} has {} fields, {wgsl_struct} {} members",
        rust.name,
        host.len(),
        shader.len()
    );
    assert!(
        pairs.is_empty(),
        "{} drifted from {wgsl_struct}: {pairs:#?}",
        rust.name
    );
    assert_eq!(
        rust.size, *span as usize,
        "{} is {} B, {wgsl_struct} spans {span} B",
        rust.name, rust.size
    );
}

/// Pod structs no WGSL struct describes, each with the gate that holds its layout instead.
const NOT_A_WGSL_STRUCT: [(&str, &str); 6] = [
    (
        "InstanceRaw",
        "vertex attributes, read through its VertexBufferLayout",
    ),
    (
        "BboxInstance",
        "vertex attributes, read through its VertexBufferLayout",
    ),
    (
        "Vertex",
        "vertex attributes, read through its VertexBufferLayout (assets and gui)",
    ),
    (
        "DrawStats",
        "atomic counters the shader addresses as a u32 array",
    ),
    (
        "GpuMaterial",
        "u32 records, MATERIAL_STRIDE in tests/tracer.rs",
    ),
    (
        "GpuInstance",
        "u32 records, INSTANCE_STRIDE in tests/tracer.rs",
    ),
];

fn rust_sources(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("source dir").flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// `source` with every `//` comment blanked, so a commented-out call or derive counts for nothing.
fn without_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_pod_word(text: &str, at: usize) -> bool {
    let before = text[..at].chars().next_back();
    let after = text[at + 3..].chars().next();
    let ident = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    !ident(before) && !ident(after)
}

/// The first identifier after `from` in `text`.
fn identifier_after(text: &str, from: usize) -> Option<&str> {
    let rest = text[from..].trim_start();
    let end = rest.find(|c: char| !c.is_alphanumeric() && c != '_')?;
    (end > 0).then(|| &rest[..end])
}

/// Every struct `code` derives or implements `Pod` for, an attribute spanning lines or behind `cfg_attr` included.
fn pod_structs(code: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(start) = code[at..].find("#[").map(|offset| at + offset) {
        let mut depth = 0;
        let mut end = start;
        for (offset, c) in code[start..].char_indices().skip(1) {
            match c {
                '[' => depth += 1,
                ']' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                end = start + offset;
                break;
            }
        }
        let attribute = &code[start..=end];
        let derives_pod = attribute.contains("derive(")
            && attribute
                .match_indices("Pod")
                .any(|(pod, _)| is_pod_word(attribute, pod));
        if derives_pod {
            if let Some(name) = code[end..]
                .find("struct ")
                .and_then(|s| identifier_after(code, end + s + 7))
            {
                found.push(name.to_owned());
            }
        }
        at = end + 1;
    }
    for (impl_at, _) in code.match_indices("unsafe impl") {
        let line_end = code[impl_at..]
            .find('{')
            .map_or(code.len(), |e| impl_at + e);
        if let Some(pod_for) = code[impl_at..line_end].find("Pod for ") {
            if let Some(name) = identifier_after(code, impl_at + pod_for + 8) {
                found.push(name.to_owned());
            }
        }
    }
    found
}

/// Every `Pod` struct of the workspace is in the table: a `pod_layout!` call or a reason it has no WGSL twin.
#[test]
fn every_pod_struct_is_checked_against_its_wgsl() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&crates).expect("crates dir").flatten() {
        let src = entry.path().join("src");
        if src.is_dir() {
            rust_sources(&src, &mut paths);
        }
    }
    // This module's fixtures spell Pod structs on purpose.
    paths.retain(|path| !path.ends_with("render/pod_layout.rs"));
    let sources: Vec<String> = paths
        .iter()
        .map(|path| without_comments(&std::fs::read_to_string(path).expect("reads")))
        .collect();
    let mut unchecked = Vec::new();
    for name in sources.iter().flat_map(|code| pod_structs(code)) {
        let call = format!("pod_layout!({name} {{");
        let exempt = NOT_A_WGSL_STRUCT.iter().any(|(exempt, _)| *exempt == name);
        if !exempt && !sources.iter().any(|code| code.contains(&call)) {
            unchecked.push(name);
        }
    }
    assert!(
        unchecked.is_empty(),
        "Pod structs without a WGSL layout check: {unchecked:?}"
    );
}

#[test]
fn the_scan_finds_every_spelling_of_a_pod_struct() {
    let code = without_comments(
        "#[derive(\n    Copy,\n    Clone,\n    bytemuck::Pod,\n)]\nstruct Wrapped {}\n\
         #[cfg_attr(feature = \"gpu\", derive(Pod, Zeroable))]\npub struct Gated {}\n\
         unsafe impl bytemuck::Pod for Manual {}\n\
         #[derive(Clone, Podium)]\nstruct NotPod {}\n\
         // #[derive(Pod)]\n// struct Commented {}\n",
    );
    assert_eq!(pod_structs(&code), ["Wrapped", "Gated", "Manual"]);
    assert!(!without_comments("// pod_layout!(Sky {").contains("pod_layout!("));
}

#[test]
fn a_moved_field_fails_the_check() {
    #[repr(C)]
    #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
    struct Swapped {
        mode: [f32; 4],
        inv_view_proj: [[f32; 4]; 4],
        light_dir_intensity: [f32; 4],
    }
    let swapped = pod_layout!(Swapped {
        mode,
        inv_view_proj,
        light_dir_intensity
    });
    let result = std::panic::catch_unwind(|| {
        assert_matches_wgsl(&swapped, &wgsl("sky.wgsl"), "SkyUniforms_std140_0");
    });
    assert!(result.is_err(), "a reordered struct must not match");
}
