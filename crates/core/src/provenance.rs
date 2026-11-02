//! The chain-build marker search ORT and GenAI share to refuse a library built outside the family's tree.

/// True when `haystack` contains `needle`, ignoring ASCII case and the path separator.
pub fn embeds_marker(haystack: &[u8], needle: &str) -> bool {
    let needle = needle.as_bytes();
    needle.is_empty()
        || haystack
            .windows(needle.len())
            .any(|w| w.iter().zip(needle).all(|(a, b)| same_path_byte(*a, *b)))
}

/// clang-cl writes `__FILE__` with whatever separators CMake handed it, so `/` and `\` match each other.
fn same_path_byte(a: u8, b: u8) -> bool {
    let separator = |c: u8| c == b'/' || c == b'\\';
    a.eq_ignore_ascii_case(&b) || (separator(a) && separator(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_needle_is_always_embedded() {
        assert!(embeds_marker(b"", ""));
    }

    #[test]
    fn a_needle_longer_than_the_haystack_is_not() {
        assert!(!embeds_marker(b"/opt", "/opt/onnxruntime/"));
    }
}
