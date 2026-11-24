//! The bridge contract for AccelerANTgine's bridge, read from the source with `kataglyphis_bridge_support::contract`:
//! each file in `kataglyphis::oxidant::accel::<file stem>`, every bridged function one `guarded` call with errors as
//! values, text from C++ as bytes, no call into C++, and the profile `guarded` needs to catch. Owner D3: the crate
//! denies unsafe code and the panic lints, cxx's generated glue its one allowance, while the crates it links may hold
//! unsafe FFI.

use kataglyphis_bridge_support::contract::{
    bridge_files, bridged, linked_workspace_crates, lint_policy_violations, manifest_section,
    source_dir, stray_unsafe_lines, violations,
};

/// This crate's directory, or `None` on a device without the sources.
fn crate_dir() -> Option<&'static std::path::Path> {
    source_dir(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn every_bridge_file_keeps_the_contract() {
    let Some(dir) = crate_dir() else { return };
    let files = bridge_files(dir).unwrap();
    let stems: Vec<&str> = files.iter().map(|(stem, _)| stem.as_str()).collect();
    assert_eq!(
        stems,
        ["diagnostics", "text"],
        "AccelerANTgine lists each in corrosion_add_cxxbridge"
    );
    let mut all = Vec::new();
    for (stem, text) in &files {
        assert_eq!(
            violations("kataglyphis::oxidant::accel", stem, text),
            Vec::<String>::new()
        );
        assert_eq!(text.matches("#[allow(unsafe_code)]").count(), 1, "{stem}");
        all.extend(bridged(text));
    }
    all.sort();
    assert_eq!(
        all,
        [
            "cancel",
            "cancel_handle",
            "genai_available",
            "initialize",
            "is_initialized",
            "new_text_generator",
            "next_piece",
            "oxidant_version",
            "start",
            "take_log_records",
            "text_intra_op_threads"
        ]
    );
}

/// Every Rust file under `src` but this one, which names the attributes it looks for, as (path, text).
fn sources(dir: &std::path::Path) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(sources(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") && !path.ends_with("contract.rs")
        {
            let text = std::fs::read_to_string(&path).unwrap();
            found.push((path.display().to_string(), text));
        }
    }
    found
}

#[test]
fn the_crate_holds_no_unsafe_code_of_its_own_and_denies_panics() {
    let Some(dir) = crate_dir() else { return };
    let lib_rs = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
    assert_eq!(
        lint_policy_violations("accel_bridge", &lib_rs, "#![deny(unsafe_code)]"),
        Vec::<String>::new()
    );
    let files = sources(&dir.join("src"));
    assert!(files.len() >= 4, "{}", files.len());
    for (path, text) in &files {
        assert_eq!(stray_unsafe_lines(text), Vec::<usize>::new(), "{path}");
    }
}

#[test]
fn the_bridge_profile_unwinds_so_guarded_catches() {
    let Some(dir) = crate_dir() else { return };
    let manifest = std::fs::read_to_string(dir.join("../../Cargo.toml")).unwrap();
    let section = manifest_section(&manifest, "profile.bridge");
    assert!(section.contains(&r#"inherits = "release""#), "{section:?}");
    assert!(section.contains(&r#"panic = "unwind""#), "{section:?}");
}

#[test]
fn the_support_crate_keeps_its_policy_and_the_text_crate_may_hold_ffi() {
    let Some(dir) = crate_dir() else { return };
    let crates = linked_workspace_crates(&std::fs::read_to_string(dir.join("Cargo.toml")).unwrap());
    assert_eq!(crates, ["bridge_support", "text"]);
    let support = std::fs::read_to_string(dir.join("../bridge_support/src/lib.rs")).unwrap();
    assert_eq!(
        lint_policy_violations("bridge_support", &support, "#![forbid(unsafe_code)]"),
        Vec::<String>::new()
    );
}
