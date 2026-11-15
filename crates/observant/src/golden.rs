//! Golden files of ObservANT's JSON, which OmniAccelerANT's tests parse; `OBSERVANT_UPDATE_GOLDEN=1` rewrites them.

use std::path::Path;

/// Compares `text` with `embedded`, the golden built in with `include_str!`, or rewrites the file on request.
pub(crate) fn check(dir: &str, name: &str, text: &str, embedded: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(dir)
        .join(format!("{name}.json"));
    if std::env::var_os("OBSERVANT_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        return;
    }
    // A Windows checkout may hand the file over with CRLF; the bytes ObservANT sends are LF.
    assert_eq!(
        embedded.replace("\r\n", "\n"),
        text,
        "{} differs; OBSERVANT_UPDATE_GOLDEN=1 rewrites it",
        path.display()
    );
}

/// `json` as the goldens keep it: pretty, with a final newline.
pub(crate) fn pretty(json: &serde_json::Value) -> String {
    serde_json::to_string_pretty(json).unwrap() + "\n"
}
