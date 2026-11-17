//! glTF import with `KHR_texture_basisu`, which `gltf::import` refuses: KTX2 through `load_ktx2`.

use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;

use crate::ktx2_loader::load_ktx2;
use crate::scene::CpuTexture;

const BASISU: &str = "KHR_texture_basisu";

/// Where one glTF texture's image comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextureSource {
    /// The `KHR_texture_basisu` KTX2 image, preferred when it loads.
    pub basisu: Option<usize>,
    /// The core `source`: the whole source without the extension, its fallback with it.
    pub fallback: Option<usize>,
}

/// A parsed, validated glTF or GLB, its binary chunk and each texture's sources.
pub(crate) struct Parsed {
    pub document: gltf::Document,
    pub blob: Option<Vec<u8>>,
    pub sources: Vec<TextureSource>,
}

/// `gltf::Gltf::from_slice`, except that it accepts `KHR_texture_basisu` as required and as a texture's only source.
pub(crate) fn parse(bytes: &[u8]) -> anyhow::Result<Parsed> {
    let gltf::Gltf { document, blob } = gltf::Gltf::from_slice_without_validation(bytes)?;
    let mut root = document.into_json();
    let image_count = root.images.len();
    let mut sources = Vec::with_capacity(root.textures.len());
    for (index, texture) in root.textures.iter_mut().enumerate() {
        let basisu = basisu_source(texture, index, image_count)?;
        let core = texture.source.value();
        let fallback = (core != u32::MAX as usize).then_some(core);
        if fallback.is_none() {
            if let Some(image) = basisu {
                // Validation wants a core source, and `Texture::source` unwraps it.
                texture.source = gltf::json::Index::new(image as u32);
            }
        }
        sources.push(TextureSource { basisu, fallback });
    }
    root.extensions_required.retain(|name| name != BASISU);
    let document = gltf::Document::from_json(root)?;
    Ok(Parsed {
        document,
        blob,
        sources,
    })
}

fn basisu_source(
    texture: &gltf::json::Texture,
    index: usize,
    image_count: usize,
) -> anyhow::Result<Option<usize>> {
    let Some(extension) = texture
        .extensions
        .as_ref()
        .and_then(|extensions| extensions.others.get(BASISU))
    else {
        return Ok(None);
    };
    let image = extension
        .get("source")
        .and_then(gltf::json::Value::as_u64)
        .with_context(|| format!("texture {index}: {BASISU} has no image source"))?;
    anyhow::ensure!(
        image < image_count as u64,
        "texture {index}: {BASISU} source {image} is past the {image_count} images"
    );
    Ok(Some(image as usize))
}

/// Every image, index-aligned with the document's; a failed KTX2 stays an error for a fallback to replace.
pub(crate) fn import_images(
    document: &gltf::Document,
    base: Option<&Path>,
    buffers: &[gltf::buffer::Data],
) -> anyhow::Result<Vec<anyhow::Result<Arc<CpuTexture>>>> {
    document
        .images()
        .map(|image| {
            if let Some(bytes) = ktx2_bytes(&image, base, buffers) {
                let texture = bytes
                    .and_then(|bytes| load_ktx2(&bytes))
                    .with_context(|| format!("glTF image {} (KTX2)", image.index()));
                return Ok(texture.map(Arc::new));
            }
            let data = gltf::image::Data::from_source(image.source(), base, buffers)?;
            Ok(Ok(Arc::new(crate::gltf_loader::to_rgba8(data)?)))
        })
        .collect()
}

/// Each texture's image: its `KHR_texture_basisu` KTX2, else (with a warning on failure) its fallback.
pub(crate) fn resolve_textures(
    sources: &[TextureSource],
    images: Vec<anyhow::Result<Arc<CpuTexture>>>,
) -> anyhow::Result<Vec<Arc<CpuTexture>>> {
    let mut used = vec![false; images.len()];
    let mut textures = Vec::with_capacity(sources.len());
    for (index, source) in sources.iter().enumerate() {
        let chosen = match (source.basisu, source.fallback) {
            (Some(basisu), Some(fallback)) => match &images[basisu] {
                Ok(_) => basisu,
                Err(err) => {
                    log::warn!(
                        "glTF texture {index}: {err:#}; using its fallback image {fallback}"
                    );
                    fallback
                }
            },
            (Some(basisu), None) => basisu,
            (None, Some(fallback)) => fallback,
            (None, None) => anyhow::bail!("glTF texture {index} has no image"),
        };
        used[chosen] = true;
        match &images[chosen] {
            Ok(texture) => textures.push(Arc::clone(texture)),
            Err(err) => anyhow::bail!("glTF texture {index}: {err:#}"),
        }
    }
    for (image, unused_error) in images.iter().enumerate().filter(|(i, _)| !used[*i]) {
        if let Err(err) = unused_error {
            log::warn!("glTF image {image} is unused and does not load: {err:#}");
        }
    }
    Ok(textures)
}

/// A KTX2 image's bytes, or `None` for any other image; KTX2 is told by MIME type or `.ktx2` URI.
fn ktx2_bytes(
    image: &gltf::Image,
    base: Option<&Path>,
    buffers: &[gltf::buffer::Data],
) -> Option<anyhow::Result<Vec<u8>>> {
    match image.source() {
        gltf::image::Source::View { view, mime_type } => (mime_type == "image/ktx2").then(|| {
            let buffer = &buffers[view.buffer().index()].0;
            buffer
                .get(view.offset()..view.offset() + view.length())
                .map(<[u8]>::to_vec)
                .context("the buffer view runs past its buffer")
        }),
        gltf::image::Source::Uri { uri, mime_type } => {
            if let Some(data) = uri.strip_prefix("data:") {
                let (media, payload) = data.split_once(";base64,")?;
                (media == "image/ktx2").then(|| decode_base64(payload))
            } else {
                let is_ktx2 = mime_type == Some("image/ktx2")
                    || uri
                        .rsplit('.')
                        .next()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("ktx2"));
                is_ktx2.then(|| read_relative(base, uri))
            }
        }
    }
}

fn read_relative(base: Option<&Path>, uri: &str) -> anyhow::Result<Vec<u8>> {
    let base = base.with_context(|| {
        format!("glTF image '{uri}' is an external file, which an in-memory import cannot read")
    })?;
    let path = match uri
        .strip_prefix("file://")
        .or_else(|| uri.strip_prefix("file:"))
    {
        Some(path) => Path::new(path).to_path_buf(),
        None => base.join(percent_decode(uri)?),
    };
    std::fs::read(&path).with_context(|| format!("reading {}", path.display()))
}

/// RFC 3986 percent-decoding of a relative URI, as gltf applies to its own.
fn percent_decode(uri: &str) -> anyhow::Result<String> {
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
fn decode_base64(text: &str) -> anyhow::Result<Vec<u8>> {
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
    }

    fn texture(basisu: Option<usize>, fallback: Option<usize>) -> TextureSource {
        TextureSource { basisu, fallback }
    }

    fn solid(value: u8) -> anyhow::Result<Arc<CpuTexture>> {
        Ok(Arc::new(CpuTexture {
            width: 1,
            height: 1,
            rgba8: vec![value; 4],
            compressed: None,
        }))
    }

    #[test]
    fn a_loaded_basisu_image_wins_and_a_failed_one_falls_back() {
        let images = vec![
            solid(1),
            solid(2),
            Err(anyhow::anyhow!("broken KTX2")),
            solid(4),
        ];
        let sources = [
            texture(Some(1), Some(0)),
            texture(Some(2), Some(3)),
            texture(None, Some(0)),
            texture(Some(1), None),
        ];
        let resolved = resolve_textures(&sources, images).unwrap();
        let firsts: Vec<u8> = resolved.iter().map(|t| t.rgba8[0]).collect();
        assert_eq!(firsts, [2, 4, 1, 2]);
        assert!(
            Arc::ptr_eq(&resolved[0], &resolved[3]),
            "one image, one texture"
        );
    }

    #[test]
    fn a_failed_basisu_image_without_a_fallback_fails_the_load() {
        let images = vec![Err(anyhow::anyhow!("broken KTX2"))];
        let err = resolve_textures(&[texture(Some(0), None)], images).unwrap_err();
        assert!(format!("{err:#}").contains("broken KTX2"), "{err:#}");
    }
}
