//! The limits `shaders/common/limits.slang` and the alpha modes `alpha_mode.slang` declare, read at compile time, so Rust cannot drift from the shaders.

/// The shaders' file, which C++ includes too.
pub const LIMITS_SLANG: &str = include_str!("../../../shaders/common/limits.slang");

/// Shadow cascades, one light_space matrix each.
#[allow(
    clippy::panic,
    reason = "a const-evaluation panic is a compile error, never a runtime one"
)]
pub const CASCADE_COUNT: usize = match slang_int(LIMITS_SLANG, "CASCADE_COUNT") {
    Some(value) => value as usize,
    None => panic!("limits.slang declares no `static const int CASCADE_COUNT = <n>;`"),
};

/// The widest PCF kernel radius.
#[allow(
    clippy::panic,
    reason = "a const-evaluation panic is a compile error, never a runtime one"
)]
pub const MAX_PCF_RADIUS: u32 = match slang_int(LIMITS_SLANG, "MAX_PCF_RADIUS") {
    Some(value) => value,
    None => panic!("limits.slang declares no `static const int MAX_PCF_RADIUS = <n>;`"),
};

/// glTF's alpha modes as both renderers' materials code them, which C++ includes too.
pub const ALPHA_MODE_SLANG: &str = include_str!("../../../shaders/common/alpha_mode.slang");

/// OPAQUE: alpha is ignored.
#[allow(
    clippy::panic,
    reason = "a const-evaluation panic is a compile error, never a runtime one"
)]
pub const ALPHA_OPAQUE: u32 = match slang_int(ALPHA_MODE_SLANG, "ALPHA_OPAQUE") {
    Some(value) => value,
    None => panic!("alpha_mode.slang declares no `static const int ALPHA_OPAQUE = <n>;`"),
};

/// MASK: cut out below the cutoff.
#[allow(
    clippy::panic,
    reason = "a const-evaluation panic is a compile error, never a runtime one"
)]
pub const ALPHA_MASK: u32 = match slang_int(ALPHA_MODE_SLANG, "ALPHA_MASK") {
    Some(value) => value,
    None => panic!("alpha_mode.slang declares no `static const int ALPHA_MASK = <n>;`"),
};

/// BLEND: composited back to front.
#[allow(
    clippy::panic,
    reason = "a const-evaluation panic is a compile error, never a runtime one"
)]
pub const ALPHA_BLEND: u32 = match slang_int(ALPHA_MODE_SLANG, "ALPHA_BLEND") {
    Some(value) => value,
    None => panic!("alpha_mode.slang declares no `static const int ALPHA_BLEND = <n>;`"),
};

/// The value of the first `static const int <name> = <digits>;` line of `source`, or `None`.
#[must_use]
pub const fn slang_int(source: &str, name: &str) -> Option<u32> {
    const PREFIX: &[u8] = b"static const int ";
    let text = source.as_bytes();
    let name = name.as_bytes();
    let mut line = 0;
    while line < text.len() {
        let mut end = line;
        while end < text.len() && text[end] != b'\n' {
            end += 1;
        }
        let mut at = line;
        while at < end && (text[at] == b' ' || text[at] == b'\t') {
            at += 1;
        }
        if starts_with(text, at, end, PREFIX) && starts_with(text, at + PREFIX.len(), end, name) {
            if let Some(value) = assigned_digits(text, at + PREFIX.len() + name.len(), end) {
                return Some(value);
            }
        }
        line = end + 1;
    }
    None
}

const fn starts_with(text: &[u8], at: usize, end: usize, word: &[u8]) -> bool {
    if at + word.len() > end {
        return false;
    }
    let mut i = 0;
    while i < word.len() {
        if text[at + i] != word[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// ` = <digits>;` from `at`; a longer identifier or a non-literal value is no match.
const fn assigned_digits(text: &[u8], mut at: usize, end: usize) -> Option<u32> {
    while at < end && text[at] == b' ' {
        at += 1;
    }
    if at >= end || text[at] != b'=' {
        return None;
    }
    at += 1;
    while at < end && text[at] == b' ' {
        at += 1;
    }
    let mut value: u32 = 0;
    let mut digits = 0;
    while at < end && text[at].is_ascii_digit() {
        value = match value.checked_mul(10) {
            Some(scaled) => match scaled.checked_add((text[at] - b'0') as u32) {
                Some(sum) => sum,
                None => return None,
            },
            None => return None,
        };
        digits += 1;
        at += 1;
    }
    while at < end && text[at] == b' ' {
        at += 1;
    }
    if digits == 0 || at >= end || text[at] != b';' {
        return None;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_declared_constant_and_ignores_longer_names_and_comments() {
        let source =
            "// static const int A = 9;\nstatic const int AB = 2;\n  static const int A = 7;\n";
        assert_eq!(slang_int(source, "A"), Some(7));
        assert_eq!(slang_int(source, "AB"), Some(2));
        assert_eq!(slang_int(source, "B"), None);
    }

    #[test]
    fn rejects_a_value_that_is_no_literal() {
        assert_eq!(slang_int("static const int A = B;\n", "A"), None);
        assert_eq!(slang_int("static const int A = 3\n", "A"), None);
        assert_eq!(slang_int("static const int A = 99999999999;\n", "A"), None);
        assert_eq!(slang_int("static const int A = 4294967296;\n", "A"), None);
        assert_eq!(
            slang_int("static\nstatic const int A = 7 ;\n", "A"),
            Some(7)
        );
        assert_eq!(
            slang_int("static const int A = 4294967295;\n", "A"),
            Some(u32::MAX)
        );
    }

    #[test]
    fn the_limits_file_holds_only_lines_cpp_can_include() {
        for line in LIMITS_SLANG.lines().map(str::trim) {
            assert!(
                line.is_empty() || line.starts_with("//") || line.starts_with("static const int "),
                "limits.slang line C++ cannot include: {line}"
            );
        }
        assert_eq!(CASCADE_COUNT, 3);
        assert_eq!(MAX_PCF_RADIUS, 5);
    }

    #[test]
    fn the_alpha_modes_are_three_distinct_codes_cpp_can_include() {
        for line in ALPHA_MODE_SLANG.lines().map(str::trim) {
            assert!(
                line.is_empty() || line.starts_with("//") || line.starts_with("static const int "),
                "alpha_mode.slang line C++ cannot include: {line}"
            );
        }
        assert_eq!((ALPHA_OPAQUE, ALPHA_MASK, ALPHA_BLEND), (0, 1, 2));
    }
}

#[cfg(test)]
mod one_home {
    use std::path::{Path, PathBuf};

    const NAMES: [&str; 2] = ["CASCADE_COUNT", "MAX_PCF_RADIUS"];

    fn files(root: &Path, extension: &str, out: &mut Vec<PathBuf>) {
        let entries = std::fs::read_dir(root).expect("a workspace directory");
        for entry in entries.flatten() {
            let path = entry.path();
            let skipped = path
                .file_name()
                .is_some_and(|name| name == "target" || name == "third_party" || name == ".git");
            if path.is_dir() && !skipped {
                files(&path, extension, out);
            } else if path.extension().is_some_and(|ext| ext == extension) {
                out.push(path);
            }
        }
    }

    /// Lines of `extension` files under `root` that give one of the limits a literal value of their own.
    fn redeclarations(
        root: &Path,
        extension: &str,
        declares: impl Fn(&str, &str) -> bool,
    ) -> Vec<String> {
        let mut paths = Vec::new();
        files(root, extension, &mut paths);
        let mut found = Vec::new();
        for path in paths
            .iter()
            .filter(|path| !path.ends_with("common/limits.slang"))
        {
            let text = std::fs::read_to_string(path).expect("a readable source");
            for line in text
                .lines()
                .map(str::trim)
                .filter(|line| !line.starts_with("//"))
            {
                if NAMES.iter().any(|name| declares(line, name)) {
                    found.push(format!("{}: {line}", path.display()));
                }
            }
        }
        found
    }

    fn workspace() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn no_shader_redeclares_a_limit() {
        let found = redeclarations(&workspace().join("shaders"), "slang", |line, name| {
            line.starts_with("static const") && line.contains(&format!(" {name} ="))
        });
        assert!(found.is_empty(), "import limits instead: {found:#?}");
    }

    #[test]
    fn no_rust_constant_repeats_a_limit() {
        let found = redeclarations(&workspace().join("crates"), "rs", |line, name| {
            let Some(rest) = line
                .split_once(&format!("const {name}:"))
                .map(|(_, rest)| rest)
            else {
                return false;
            };
            rest.split_once('=')
                .is_some_and(|(_, value)| value.trim().starts_with(|c: char| c.is_ascii_digit()))
        });
        assert!(
            found.is_empty(),
            "use kataglyphis_render_core::shader_limits: {found:#?}"
        );
    }
}
