//! The bridge contract, read from the source: each file in its area's namespace, every bridged function one `guarded`
//! call with errors as values, text from C++ as bytes, no call into C++, the profile `guarded` needs to catch, and
//! no unsafe code or explicit panic in the workspace crates the bridge links. The checks are
//! `kataglyphis_bridge_support::contract`'s, which AccelerANTgine's bridge runs too.

use kataglyphis_bridge_support::contract::{
    bridge_files, bridged, linked_workspace_crates, lint_policy_violations, manifest_section,
    source_dir, violations,
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
        ["diagnostics", "gltf", "parity", "settings"],
        "BeschleunigerBallett lists each in corrosion_add_cxxbridge"
    );
    let mut all = Vec::new();
    for (stem, text) in &files {
        assert_eq!(
            violations("kataglyphis::oxidant", stem, text),
            Vec::<String>::new()
        );
        all.extend(bridged(text));
    }
    all.sort();
    assert_eq!(
        all,
        [
            "default_scene_settings",
            "frame_scene",
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
fn the_bridge_profile_unwinds_so_guarded_catches() {
    let Some(dir) = crate_dir() else { return };
    let manifest = std::fs::read_to_string(dir.join("../../Cargo.toml")).unwrap();
    let section = manifest_section(&manifest, "profile.bridge");
    assert!(section.contains(&r#"inherits = "release""#), "{section:?}");
    assert!(section.contains(&r#"panic = "unwind""#), "{section:?}");
}

#[test]
fn the_linked_crates_forbid_unsafe_code_and_panics() {
    let Some(dir) = crate_dir() else { return };
    let crates = linked_workspace_crates(&std::fs::read_to_string(dir.join("Cargo.toml")).unwrap());
    assert!(crates.contains(&"assets".to_owned()), "{crates:?}");
    assert!(crates.contains(&"bridge_support".to_owned()), "{crates:?}");
    let root = dir.join("..");
    for dir in crates {
        let lib_rs = std::fs::read_to_string(root.join(&dir).join("src/lib.rs")).unwrap();
        assert_eq!(
            lint_policy_violations(&dir, &lib_rs, "#![forbid(unsafe_code)]"),
            Vec::<String>::new()
        );
    }
}
