//! The bridge contract, read from the source: each file in its area's namespace, every bridged function one `guarded`
//! call with errors as values, text from C++ as bytes, no call into C++, and the profile `guarded` needs to catch.

use std::path::Path;

/// Every bridge file but this checker, as (file stem, text).
fn bridge_files() -> Vec<(String, String)> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<(String, String)> = std::fs::read_dir(src)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .map(|path| {
            let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
            (stem, std::fs::read_to_string(&path).unwrap())
        })
        .filter(|(stem, text)| stem != "contract" && text.contains("#[cxx::bridge"))
        .collect();
    files.sort();
    files
}

/// The text between the `{` at `open` and its matching `}`.
fn block(text: &str, open: usize) -> &str {
    let mut depth = 0;
    for (at, c) in text[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' if depth == 1 => return &text[open + 1..open + at],
            '}' => depth -= 1,
            _ => {}
        }
    }
    panic!("an unclosed block at byte {open}");
}

/// The bodies of every `extern "<lang>"` block, `unsafe` or not, without their comment lines.
fn extern_blocks(text: &str, lang: &str) -> Vec<String> {
    let opening = format!("extern \"{lang}\" {{");
    text.match_indices(&opening)
        .map(|(at, _)| {
            block(text, at + opening.len() - 1)
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect()
}

/// Each `fn` declaration in an extern block, as (name, parameters, return type).
fn declarations(body: &str) -> Vec<(String, String, String)> {
    body.split(';')
        .filter_map(|item| {
            let item = item.trim();
            let after = &item[item.find("fn ")? + 3..];
            let open = after.find('(')?;
            let close = after.rfind(')')?;
            let name = after[..open].trim().to_owned();
            let returns = after[close + 1..].trim().trim_start_matches("->").trim();
            Some((name, after[open + 1..close].to_owned(), returns.to_owned()))
        })
        .collect()
}

fn is_text(parameter_type: &str) -> bool {
    parameter_type
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|token| matches!(token, "str" | "String" | "CxxString"))
}

/// Every way the file breaks the contract; empty when it keeps it.
fn violations(stem: &str, text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let namespace = format!("#[cxx::bridge(namespace = \"kataglyphis::oxidant::{stem}\")]");
    if text.matches("#[cxx::bridge").count() != 1 || !text.contains(&namespace) {
        found.push(format!("{stem}: one bridge, in {namespace}"));
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
            let definition = format!("pub fn {name}(");
            let body = text
                .find(&definition)
                .and_then(|at| text[at..].find('{').map(|open| block(text, at + open)));
            if !body.is_some_and(|body| body.trim_start().starts_with("guarded(")) {
                found.push(format!("{stem}::{name}'s body is not one guarded() call"));
            }
        }
    }
    found
}

#[test]
fn every_bridge_file_keeps_the_contract() {
    let files = bridge_files();
    let stems: Vec<&str> = files.iter().map(|(stem, _)| stem.as_str()).collect();
    assert_eq!(
        stems,
        ["diagnostics", "gltf", "parity", "settings"],
        "BeschleunigerBallett lists each in corrosion_add_cxxbridge"
    );
    let mut bridged = Vec::new();
    for (stem, text) in &files {
        assert_eq!(violations(stem, text), Vec::<String>::new());
        for body in extern_blocks(text, "Rust") {
            bridged.extend(declarations(&body).into_iter().map(|(name, _, _)| name));
        }
    }
    bridged.sort();
    assert_eq!(
        bridged,
        [
            "default_scene_settings",
            "load_gltf_flat",
            "load_gltf_flat_with",
            "oxidant_version",
            "parity_lens",
            "parity_model_transform",
            "parity_viewport_remap",
            "sanitize",
            "setting_ranges",
            "take_log_records"
        ]
    );
}

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

pub fn text(path: &str, name: String) -> Result<u8, String> {{
    Ok(guarded(|| 1, |_| 0))
}}

pub fn unguarded(path: &[u8]) -> u8 {{
    7
}}
"#
    );
    let found = violations("area", &breaches);
    let expected = [
        "area: one bridge, in #[cxx::bridge(namespace = \"kataglyphis::oxidant::area\")]",
        "area: a C++ function leaves cargo test an undefined symbol",
        "area::text returns a cxx Result, a C++ exception",
        "area::text takes text, not UTF-8 bytes: path: &str",
        "area::text takes text, not UTF-8 bytes: name: String",
        "area::text's body is not one guarded() call",
        "area::unguarded's body is not one guarded() call",
    ];
    assert_eq!(found, expected);
    assert!(!is_text("&[u8]") && !is_text("&'static [u8]") && is_text("&'static str"));
}

#[test]
fn the_bridge_profile_unwinds_so_guarded_catches() {
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml"))
            .unwrap();
    let section: Vec<&str> = manifest
        .lines()
        .skip_while(|line| line.trim() != "[profile.bridge]")
        .skip(1)
        .take_while(|line| !line.starts_with('['))
        .map(str::trim)
        .collect();
    assert!(section.contains(&r#"inherits = "release""#), "{section:?}");
    assert!(section.contains(&r#"panic = "unwind""#), "{section:?}");
}
