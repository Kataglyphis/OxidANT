//! Reading what a glTF document names: regular files only, and relative URIs inside the document's directory.

use std::path::Path;

use anyhow::Context as _;

/// A regular file's bytes; a FIFO, device or directory is refused unopened, since reading one can block forever.
pub(crate) fn read_regular_file(path: &Path) -> anyhow::Result<Vec<u8>> {
    require_regular_file(path)?;
    Ok(std::fs::read(path)?)
}

pub(crate) fn require_regular_file(path: &Path) -> anyhow::Result<()> {
    refuse_device(path)?;
    anyhow::ensure!(std::fs::metadata(path)?.is_file(), "not a regular file");
    Ok(())
}

/// Windows opens a DOS device for these names in any directory, and reading "con" waits for console input forever.
fn refuse_device(path: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(
        !(cfg!(windows) && is_windows_device_name(path)),
        "a Windows device name, not a file"
    );
    Ok(())
}

/// A DOS device name as Windows matches it: the last component up to its first dot, without trailing spaces, in any case.
/// "con", "CON.txt", "models/con" and "con . " are all the console; BeschleunigerBallett's `FileReader` has the same rule.
pub(crate) fn is_windows_device_name(path: &Path) -> bool {
    const DEVICES: [&str; 6] = ["con", "prn", "aux", "nul", "conin$", "conout$"];
    let text = path.to_string_lossy();
    let name = text.rsplit(['/', '\\']).next().unwrap_or_default();
    let stem = name.split('.').next().unwrap_or_default();
    let stem = stem.trim_end_matches(' ').to_ascii_lowercase();
    let port = stem
        .strip_prefix("com")
        .or_else(|| stem.strip_prefix("lpt"));
    DEVICES.contains(&stem.as_str())
        || port.is_some_and(|digit| {
            (digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit())
                || matches!(digit, "¹" | "²" | "³")
        })
}

/// Reads a relative URI that must resolve inside `base`, so an untrusted file cannot name `../../secret`.
pub(crate) fn read_confined(base: &Path, uri: &str) -> anyhow::Result<Vec<u8>> {
    let base = if base.as_os_str().is_empty() {
        Path::new(".")
    } else {
        base
    };
    let root = base
        .canonicalize()
        .with_context(|| format!("resolving {}", base.display()))?;
    let candidate = base.join(percent_decode(uri)?.replace('\\', "/"));
    // Before canonicalize, which opens the path on Windows.
    refuse_device(&candidate).with_context(|| format!("'{uri}'"))?;
    let file = candidate
        .canonicalize()
        .with_context(|| format!("'{uri}' does not resolve to a file"))?;
    anyhow::ensure!(
        file.starts_with(&root),
        "'{uri}' resolves outside the document's directory"
    );
    read_regular_file(&file).with_context(|| format!("reading {}", file.display()))
}

/// `gltf::import_buffers` with its GLB chunk, data URIs, padding and byteLength check, but a file must be a regular
/// one inside `base`: gltf's reader opens any path, and panics on a percent escape that is not UTF-8.
pub(crate) fn import_buffers(
    document: &gltf::Document,
    base: Option<&Path>,
    mut blob: Option<Vec<u8>>,
) -> anyhow::Result<Vec<gltf::buffer::Data>> {
    let mut buffers = Vec::with_capacity(document.buffers().len());
    for buffer in document.buffers() {
        let index = buffer.index();
        let mut data = match buffer.source() {
            gltf::buffer::Source::Bin => blob.take().context("no GLB binary chunk is left for it"),
            gltf::buffer::Source::Uri(uri) => uri_bytes(base, uri),
        }
        .with_context(|| format!("glTF buffer {index}"))?;
        while !data.len().is_multiple_of(4) {
            data.push(0);
        }
        anyhow::ensure!(
            data.len() >= buffer.length(),
            "glTF buffer {index} holds {} bytes, fewer than its byteLength {}",
            data.len(),
            buffer.length()
        );
        buffers.push(gltf::buffer::Data(data));
    }
    Ok(buffers)
}

/// A buffer URI's bytes: base64 data, else a relative file confined to `base`; an in-memory import has no `base`.
fn uri_bytes(base: Option<&Path>, uri: &str) -> anyhow::Result<Vec<u8>> {
    if let Some(data) = uri.strip_prefix("data:") {
        let (_, payload) = data
            .split_once(";base64,")
            .context("a data URI that is not base64")?;
        return decode_base64(payload);
    }
    // gltf reads `file:` paths and refuses every other scheme; neither names a file beside the document.
    anyhow::ensure!(!uri.contains(':'), "'{uri}' is not a relative URI");
    let base = base.with_context(|| {
        format!("'{uri}' is an external file, which an in-memory import cannot read")
    })?;
    read_confined(base, uri)
}

/// RFC 3986 percent-decoding of a relative URI, as gltf applies to its own.
pub(crate) fn percent_decode(uri: &str) -> anyhow::Result<String> {
    let bytes = uri.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = uri
                .get(at + 1..at + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                .with_context(|| format!("bad percent escape in '{uri}'"))?;
            out.push(hex);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).with_context(|| format!("'{uri}' is not UTF-8 once decoded"))
}

/// Standard base64 with optional padding.
pub(crate) fn decode_base64(text: &str) -> anyhow::Result<Vec<u8>> {
    let value = |byte: u8| match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let digits = text.trim_end_matches('=').as_bytes();
    let mut out = Vec::with_capacity(digits.len() * 3 / 4);
    let (mut bits, mut count) = (0u32, 0);
    for &byte in digits {
        let six = value(byte).context("a base64 data URI holds a character outside base64")?;
        bits = (bits << 6) | u32::from(six);
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decodes_every_padding_length() {
        assert_eq!(decode_base64("").unwrap(), b"");
        assert_eq!(decode_base64("TQ==").unwrap(), b"M");
        assert_eq!(decode_base64("TWE=").unwrap(), b"Ma");
        assert_eq!(decode_base64("TWFu").unwrap(), b"Man");
        assert_eq!(
            decode_base64("q83v/+A").unwrap(),
            [0xAB, 0xCD, 0xEF, 0xFF, 0xE0]
        );
        assert!(decode_base64("TW F").is_err());
    }

    #[test]
    fn percent_escapes_decode_and_bad_ones_error() {
        assert_eq!(percent_decode("a%20b.ktx2").unwrap(), "a b.ktx2");
        assert_eq!(percent_decode("plain.ktx2").unwrap(), "plain.ktx2");
        assert!(percent_decode("bad%2").is_err());
        assert!(percent_decode("bad%zz").is_err());
        assert!(percent_decode("%FF.bin").is_err(), "not UTF-8 once decoded");
    }

    #[test]
    fn every_dos_device_spelling_is_a_device_name_and_ordinary_names_are_not() {
        for name in [
            "con",
            "CON",
            "con.txt",
            "Con.Tar.gz",
            "con . ",
            "models/con",
            r"C:\models\nul.png",
            "aux",
            "prn.gltf",
            "conin$",
            "CONOUT$",
            "com1",
            "LPT9.bin",
            "com0",
            "com¹",
            "lpt³.png",
        ] {
            assert!(is_windows_device_name(Path::new(name)), "{name}");
        }
        for name in [
            "console.gltf",
            "icon.png",
            "com",
            "com10",
            "lpt",
            "comx",
            "con/model.gltf",
            " con",
            "nul_texture.png",
            "",
        ] {
            assert!(!is_windows_device_name(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn a_directory_is_not_a_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let err = read_regular_file(dir.path()).unwrap_err();
        assert_eq!(format!("{err:#}"), "not a regular file");
        assert!(read_regular_file(&dir.path().join("missing.bin")).is_err());
        std::fs::write(dir.path().join("present.bin"), b"abc").unwrap();
        assert_eq!(
            read_regular_file(&dir.path().join("present.bin")).unwrap(),
            b"abc"
        );
    }
}
