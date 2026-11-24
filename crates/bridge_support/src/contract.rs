//! The bridge contract as checks on source text, which each bridge's `contract.rs` runs over its own files: one bridge
//! per file in its area's namespace, every bridged function one `guarded` call with errors as values, text from C++
//! as bytes, no call into C++, `unsafe_code` allowed only on a bridge's generated glue, and a crate root's lint policy.

use std::path::Path;

/// The lints a crate a bridge links denies outside its tests, beside its `unsafe_code` attribute.
pub const PANIC_LINTS: [&str; 5] = [
    "clippy::unwrap_used",
    "clippy::expect_used",
    "clippy::panic",
    "clippy::todo",
    "clippy::unimplemented",
];

/// A bridge crate's directory, or `None` after a SKIP line: the Windows arm64 cross lane runs test binaries on a
/// device without the sources, which every lane that builds them has.
pub fn source_dir(manifest_dir: &'static str) -> Option<&'static Path> {
    let dir = Path::new(manifest_dir);
    if dir.join("src").is_dir() {
        return Some(dir);
    }
    eprintln!("SKIP: no source tree at {}", dir.display());
    None
}

/// Every bridge file in a crate's `src` but its contract test, as (file stem, text), sorted by stem.
pub fn bridge_files(dir: &Path) -> std::io::Result<Vec<(String, String)>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir.join("src"))? {
        let path = entry?.path();
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if stem == "contract" || path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path)?;
        if text.contains("#[cxx::bridge") {
            files.push((stem, text));
        }
    }
    files.sort();
    Ok(files)
}

/// The trimmed lines of a manifest's `[<section>]`.
pub fn manifest_section<'a>(manifest: &'a str, section: &str) -> Vec<&'a str> {
    let header = format!("[{section}]");
    manifest
        .lines()
        .skip_while(|line| line.trim() != header)
        .skip(1)
        .take_while(|line| !line.starts_with('['))
        .map(str::trim)
        .collect()
}

/// The workspace crates in a manifest's `[dependencies]`, as their directory names under `crates/`.
pub fn linked_workspace_crates(manifest: &str) -> Vec<String> {
    manifest_section(manifest, "dependencies")
        .into_iter()
        .filter_map(|line| line.split_once("path = \"../")?.1.split_once('"'))
        .map(|(dir, _)| dir.to_owned())
        .collect()
}

/// The text between the `{` at `open` and its matching `}`; `None` when it never closes.
pub fn block(text: &str, open: usize) -> Option<&str> {
    let mut depth = 0_i32;
    for (at, c) in text.get(open..)?.char_indices() {
        match c {
            '{' => depth += 1,
            '}' if depth == 1 => return text.get(open + 1..open + at),
            '}' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// The bodies of every `extern "<lang>"` block, `unsafe` or not, without their comment lines.
pub fn extern_blocks(text: &str, lang: &str) -> Vec<String> {
    let opening = format!("extern \"{lang}\" {{");
    text.match_indices(&opening)
        .filter_map(|(at, _)| block(text, at + opening.len() - 1))
        .map(|body| {
            body.lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect()
}

/// Each `fn` declaration in an extern block, as (name, parameters, return type).
pub fn declarations(body: &str) -> Vec<(String, String, String)> {
    body.split(';')
        .filter_map(|item| {
            let item = item.trim();
            let after = item.get(item.find("fn ")? + 3..)?;
            let open = after.find('(')?;
            let close = after.rfind(')')?;
            let name = after.get(..open)?.trim().to_owned();
            let parameters = after.get(open + 1..close)?.to_owned();
            let returns = after
                .get(close + 1..)?
                .trim()
                .trim_start_matches("->")
                .trim();
            Some((name, parameters, returns.to_owned()))
        })
        .collect()
}

/// Every function a file bridges, in the order of its `extern "Rust"` blocks.
pub fn bridged(text: &str) -> Vec<String> {
    extern_blocks(text, "Rust")
        .iter()
        .flat_map(|body| declarations(body))
        .map(|(name, _, _)| name)
        .collect()
}

/// Whether a parameter type is text, which cxx checks for UTF-8 by throwing into an engine built without exceptions.
pub fn is_text(parameter_type: &str) -> bool {
    parameter_type
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|token| matches!(token, "str" | "String" | "CxxString"))
}

/// The 1-based lines that name `unsafe_code` other than a crate root's lint or an allowance directly on a bridge.
pub fn stray_unsafe_lines(text: &str) -> Vec<usize> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let on_a_bridge = |at: usize| {
        lines
            .get(at + 1)
            .is_some_and(|next| next.starts_with("#[cxx::bridge"))
    };
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.contains("unsafe_code") && !line.starts_with("//"))
        .filter(|(at, line)| {
            let root = matches!(**line, "#![forbid(unsafe_code)]" | "#![deny(unsafe_code)]");
            let glue = **line == "#[allow(unsafe_code)]" && on_a_bridge(*at);
            !root && !glue
        })
        .map(|(at, _)| at + 1)
        .collect()
}

/// Every way a bridge file breaks the contract in the C++ namespace `<namespace>::<stem>`; empty when it keeps it.
pub fn violations(namespace: &str, stem: &str, text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let bridge = format!("#[cxx::bridge(namespace = \"{namespace}::{stem}\")]");
    if text.matches("#[cxx::bridge").count() != 1 || !text.contains(&bridge) {
        found.push(format!("{stem}: one bridge, in {bridge}"));
    }
    for line in stray_unsafe_lines(text) {
        found.push(format!(
            "{stem}:{line}: unsafe_code off a bridge's generated glue"
        ));
    }
    for body in extern_blocks(text, "C++") {
        if !declarations(&body).is_empty() {
            found.push(format!(
                "{stem}: a C++ function leaves cargo test an undefined symbol"
            ));
        }
    }
    for body in extern_blocks(text, "Rust") {
        for (name, parameters, returns) in declarations(&body) {
            found.extend(signature_violations(stem, &name, &parameters, &returns));
            if !is_one_guarded_call(text, &name) {
                found.push(format!("{stem}::{name}'s body is not one guarded() call"));
            }
        }
    }
    found
}

/// A cxx `Result` is a C++ exception and text in is a UTF-8 check that throws; the engine builds without exceptions.
fn signature_violations(stem: &str, name: &str, parameters: &str, returns: &str) -> Vec<String> {
    let mut found = Vec::new();
    if returns.contains("Result") {
        found.push(format!(
            "{stem}::{name} returns a cxx Result, a C++ exception"
        ));
    }
    for parameter in parameters
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let parameter_type = parameter.split_once(':').map_or("", |(_, ty)| ty);
        if is_text(parameter_type) {
            found.push(format!(
                "{stem}::{name} takes text, not UTF-8 bytes: {parameter}"
            ));
        }
    }
    found
}

/// Whether the first `pub fn <name>(` in `text` has a body that starts with a `guarded(` call.
fn is_one_guarded_call(text: &str, name: &str) -> bool {
    let definition = format!("pub fn {name}(");
    text.find(&definition)
        .and_then(|at| text.get(at..)?.find('{').map(|open| at + open))
        .and_then(|open| block(text, open))
        .is_some_and(|body| body.trim_start().starts_with("guarded("))
}

/// Every way a crate root misses the policy: `unsafe_attribute` and the denied [`PANIC_LINTS`]; empty when it keeps it.
pub fn lint_policy_violations(name: &str, lib_rs: &str, unsafe_attribute: &str) -> Vec<String> {
    let code: String = lib_rs
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut found = Vec::new();
    if !code.contains(unsafe_attribute) {
        found.push(format!("{name}: no {unsafe_attribute}"));
    }
    let denied: String = code
        .match_indices("deny(")
        .filter_map(|(at, _)| {
            let rest = code.get(at..)?;
            rest.get(..rest.find(')')?)
        })
        .collect();
    for lint in PANIC_LINTS {
        if !denied.contains(lint) {
            found.push(format!("{name}: does not deny {lint}"));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAMESPACE: &str = "kataglyphis::oxidant";

    #[test]
    fn the_checker_finds_each_breach() {
        let marker = "#[cxx::bridge";
        let breaches = format!(
            r#"{marker}(namespace = "kataglyphis::oxidant::wrong")]
pub mod ffi {{
    extern "Rust" {{
        /// A doc comment with a (parenthesis) and a ; in it.
        fn text(path: &str, name: String) -> Result<u8>;
        fn unguarded(path: &[u8]) -> u8;
    }}
    unsafe extern "C++" {{
        fn callback(value: u8);
    }}
}}

#[allow(unsafe_code)]
pub fn text(path: &str, name: String) -> Result<u8, String> {{
    Ok(guarded(|| 1, |_| 0))
}}

pub fn unguarded(path: &[u8]) -> u8 {{
    7
}}
"#
        );
        let found = violations(NAMESPACE, "area", &breaches);
        let expected = [
            "area: one bridge, in #[cxx::bridge(namespace = \"kataglyphis::oxidant::area\")]",
            "area:13: unsafe_code off a bridge's generated glue",
            "area: a C++ function leaves cargo test an undefined symbol",
            "area::text returns a cxx Result, a C++ exception",
            "area::text takes text, not UTF-8 bytes: path: &str",
            "area::text takes text, not UTF-8 bytes: name: String",
            "area::text's body is not one guarded() call",
            "area::unguarded's body is not one guarded() call",
        ];
        assert_eq!(found, expected);
        assert!(!is_text("&[u8]") && !is_text("&'static [u8]") && is_text("&'static str"));
        assert_eq!(bridged(&breaches), ["text", "unguarded"]);
    }

    #[test]
    fn a_file_that_keeps_the_contract_has_no_violation() {
        let kept = r#"#[allow(unsafe_code)]
#[cxx::bridge(namespace = "kataglyphis::oxidant::accel::area")]
pub mod ffi {
    extern "Rust" {
        type Engine;
        fn make(path: &[u8]) -> Box<Engine>;
        fn step(self: &mut Engine, bytes: &[u8]) -> u8;
    }
}

pub fn make(path: &[u8]) -> Box<Engine> {
    guarded(|| Box::new(Engine(path.len())), |_| Box::new(Engine(0)))
}

impl Engine {
    pub fn step(&mut self, bytes: &[u8]) -> u8 {
        guarded(|| 1, nothing)
    }
}
"#;
        assert_eq!(
            violations("kataglyphis::oxidant::accel", "area", kept),
            Vec::<String>::new()
        );
        assert_eq!(bridged(kept), ["make", "step"]);
    }

    #[test]
    fn unsafe_code_stays_on_the_crate_root_and_the_bridges() {
        let text = "#![deny(unsafe_code)]\n#[allow(unsafe_code)]\n#[cxx::bridge]\n\
                    // #[allow(unsafe_code)] in a comment counts for nothing.\n\
                    #[allow(unsafe_code)]\nfn raw() {}\n#![allow(unsafe_code)]\n#[expect(unsafe_code)]\n";
        assert_eq!(stray_unsafe_lines(text), [5, 7, 8]);
        assert_eq!(stray_unsafe_lines("#[allow(unsafe_code)]"), [1]);
    }

    #[test]
    fn a_block_ends_at_its_own_brace_and_an_unclosed_one_is_none() {
        let text = "f { a { b } c } d";
        assert_eq!(block(text, 2), Some(" a { b } c "));
        assert_eq!(block("f { a", 2), None);
        assert_eq!(block("f", 9), None);
        assert!(extern_blocks("extern \"Rust\" { fn a();", "Rust").is_empty());
        assert!(declarations("type Engine; fn ) broken (").is_empty());
    }

    #[test]
    fn a_manifest_names_its_sections_and_its_workspace_crates() {
        let manifest = "[package]\nname = \"b\"\n\n[dependencies]\ncxx = \"1.0\"\n# a note\n\
                        a = { path = \"../assets\" }\n  text = { path = \"../text\", x = 1 }\n\n\
                        [dev-dependencies]\nt = { path = \"../tests\" }\n";
        assert_eq!(linked_workspace_crates(manifest), ["assets", "text"]);
        assert_eq!(manifest_section(manifest, "package"), ["name = \"b\"", ""]);
        assert!(manifest_section(manifest, "profile.bridge").is_empty());
    }

    #[test]
    fn a_crate_without_bridges_lists_none_and_a_missing_tree_is_skipped() {
        assert_eq!(source_dir("/no/such/crate"), None);
        if let Some(dir) = source_dir(env!("CARGO_MANIFEST_DIR")) {
            assert_eq!(bridge_files(dir).unwrap(), Vec::<(String, String)>::new());
        }
        assert!(bridge_files(Path::new("/no/such/crate")).is_err());
    }

    #[test]
    fn the_lint_policy_checker_finds_each_gap() {
        let gaps = "// #![forbid(unsafe_code)] in a comment counts for nothing.\n\
                    #![deny(clippy::unwrap_used, clippy::panic)]\n\
                    // clippy::expect_used\n";
        assert_eq!(
            lint_policy_violations("area", gaps, "#![forbid(unsafe_code)]"),
            [
                "area: no #![forbid(unsafe_code)]",
                "area: does not deny clippy::expect_used",
                "area: does not deny clippy::todo",
                "area: does not deny clippy::unimplemented",
            ]
        );
        let kept = format!(
            "#![deny(unsafe_code)]\n#![cfg_attr(not(test), deny({}))]\n",
            PANIC_LINTS.join(", ")
        );
        assert!(lint_policy_violations("area", &kept, "#![deny(unsafe_code)]").is_empty());
        assert_eq!(
            lint_policy_violations("area", &kept, "#![forbid(unsafe_code)]"),
            ["area: no #![forbid(unsafe_code)]"]
        );
    }
}
