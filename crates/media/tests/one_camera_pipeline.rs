//! Only `crates/media` and ObservANT build a camera appsink: the wgpu GUI's own pipeline never read its bus.

use std::path::{Path, PathBuf};

/// Every `.rs` file under `dir` that builds an appsink, `target` left out.
fn appsink_builders(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "target") {
                appsink_builders(&path, found);
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let source = std::fs::read_to_string(&path).unwrap();
            if ["frame_sink(", "\"appsink\"", "AppSink"]
                .iter()
                .any(|needle| source.contains(needle))
            {
                found.push(path);
            }
        }
    }
}

#[test]
fn only_the_media_crate_and_observant_build_an_appsink() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut found = Vec::new();
    appsink_builders(&root.join("src"), &mut found);
    appsink_builders(&root.join("crates"), &mut found);
    let allowed = [root.join("crates/media"), root.join("crates/observant")];
    let outside: Vec<&PathBuf> = found
        .iter()
        .filter(|path| !allowed.iter().any(|dir| path.starts_with(dir)))
        .collect();
    assert!(outside.is_empty(), "a third camera pipeline: {outside:?}");
    assert!(
        found
            .iter()
            .any(|path| path.ends_with("crates/media/src/capture.rs")),
        "the scan found not even the media crate's own pipeline: {found:?}"
    );
}
