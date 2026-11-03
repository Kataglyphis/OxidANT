//! The default model's file name, read from `resources/models/models.lock` when this crate compiles.

/// Splits at the first `sep`: the text before it and the text after; no `sep` leaves the tail empty.
const fn split_at_byte(text: &str, sep: u8) -> (&str, &str) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == sep {
            let (head, tail) = text.split_at(i);
            return (head, tail.split_at(1).1);
        }
        i += 1;
    }
    (text, "")
}

/// `text` without its leading blanks, as the lock aligns its columns with several.
const fn skip_blanks(mut text: &str) -> &str {
    while let [b' ', ..] = text.as_bytes() {
        text = text.split_at(1).1;
    }
    text
}

const fn ends_with_onnx(name: &str) -> bool {
    matches!(name.as_bytes(), [.., b'.', b'o', b'n', b'n', b'x'])
}

/// The file column of the lock's one `.onnx` row; none or several fail the build rather than pick a model.
pub(super) const fn onnx_row(lock: &str) -> &str {
    let mut rest = lock;
    let mut found = None;
    while !rest.is_empty() {
        let (line, tail) = split_at_byte(rest, b'\n');
        rest = tail;
        if !matches!(line.as_bytes(), [b'#', ..] | []) {
            let (name, _) = split_at_byte(skip_blanks(split_at_byte(line, b' ').1), b' ');
            if ends_with_onnx(name) {
                assert!(
                    found.is_none(),
                    "models.lock names more than one .onnx model"
                );
                found = Some(name);
            }
        }
    }
    match found {
        Some(name) => name,
        None => panic!("models.lock names no .onnx model"),
    }
}

#[cfg(test)]
mod tests {
    use super::onnx_row;

    const SHA: &str = "0bf16e851fd6471653b5f8243bac2d65db05e353cdf2a7b9181b2a58e94d09bd";

    #[test]
    fn the_onnx_row_names_the_model_whatever_it_is_called() {
        let lock = format!(
            "# sha256 file url\n{SHA}  other.LICENSE  https://x/other.LICENSE\n{SHA}  other.onnx      https://x/other.onnx\n"
        );
        assert_eq!(onnx_row(&lock), "other.onnx");
    }

    #[test]
    fn a_comment_naming_a_model_is_no_row() {
        let lock =
            format!("# {SHA}  old.onnx  https://x/old.onnx\n{SHA}  new.onnx  https://x/new.onnx\n");
        assert_eq!(onnx_row(&lock), "new.onnx");
    }

    #[test]
    #[should_panic(expected = "no .onnx model")]
    fn a_lock_without_a_model_is_refused() {
        onnx_row(&format!("{SHA}  other.LICENSE  https://x/other.LICENSE\n"));
    }

    #[test]
    #[should_panic(expected = "more than one .onnx model")]
    fn a_lock_with_two_models_is_refused() {
        onnx_row(&format!(
            "{SHA}  a.onnx  https://x/a\n{SHA}  b.onnx  https://x/b\n"
        ));
    }
}
