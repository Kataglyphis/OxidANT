//! glTF import with `KHR_texture_basisu`, which `gltf::import` refuses: KTX2 through `load_ktx2`.

use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;

use crate::files::{self, decode_base64, percent_decode, read_confined};
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
    reject_dangling_positions(&root)?;
    let document = gltf::Document::from_json(root)?;
    crate::accessor_check::check_primitive_accessors(&document)?;
    Ok(Parsed {
        document,
        blob,
        sources,
    })
}

/// gltf-json 1.4.1's primitive hook indexes `accessors` by POSITION before validation reports it out of range: a panic.
fn reject_dangling_positions(root: &gltf::json::Root) -> anyhow::Result<()> {
    let positions = gltf::json::validation::Checked::Valid(gltf::json::mesh::Semantic::Positions);
    for (m, mesh) in root.meshes.iter().enumerate() {
        for (p, primitive) in mesh.primitives.iter().enumerate() {
            if let Some(index) = primitive.attributes.get(&positions) {
                anyhow::ensure!(
                    index.value() < root.accessors.len(),
                    "meshes[{m}].primitives[{p}].attributes.POSITION names accessor {}, which does not exist",
                    index.value()
                );
            }
        }
    }
    Ok(())
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
            if let (gltf::image::Source::Uri { uri, .. }, Some(base)) = (image.source(), base) {
                check_relative_image(base, uri)
                    .with_context(|| format!("glTF image {}", image.index()))?;
            }
            let data = gltf::image::Data::from_source(image.source(), base, buffers)?;
            Ok(Ok(Arc::new(crate::gltf_loader::to_rgba8(data)?)))
        })
        .collect()
}

/// gltf's own reader unwraps a relative URI's percent-decoding and opens what it names: a panic, or a FIFO's endless wait.
fn check_relative_image(base: &Path, uri: &str) -> anyhow::Result<()> {
    if uri.contains(':') {
        return Ok(());
    }
    files::require_regular_file(&base.join(percent_decode(uri)?))
        .with_context(|| format!("'{uri}'"))
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
        gltf::image::Source::View { view, mime_type } => {
            (mime_type == "image/ktx2").then(|| view_bytes(&view, buffers))
        }
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

fn view_bytes(
    view: &gltf::buffer::View,
    buffers: &[gltf::buffer::Data],
) -> anyhow::Result<Vec<u8>> {
    buffers
        .get(view.buffer().index())
        .and_then(|buffer| buffer.0.get(view.offset()..view.offset() + view.length()))
        .map(<[u8]>::to_vec)
        .context("the buffer view runs past its buffer")
}

/// An image's bytes as stored, for a consumer that decodes them itself; a file must lie inside `base`.
pub(crate) fn encoded_bytes(
    image: &gltf::Image,
    base: &Path,
    buffers: &[gltf::buffer::Data],
) -> anyhow::Result<Vec<u8>> {
    match image.source() {
        gltf::image::Source::View { view, .. } => view_bytes(&view, buffers),
        gltf::image::Source::Uri { uri, .. } => {
            if let Some(data) = uri.strip_prefix("data:") {
                let (_, payload) = data
                    .split_once(";base64,")
                    .context("a data URI image that is not base64")?;
                // Padded base64 only: a truncated payload is a broken image, not a shorter one.
                anyhow::ensure!(
                    !payload.is_empty() && payload.len().is_multiple_of(4),
                    "a data URI image whose base64 is truncated"
                );
                return decode_base64(payload);
            }
            anyhow::ensure!(
                !uri.starts_with("http:") && !uri.starts_with("https:"),
                "'{uri}' is remote"
            );
            read_confined(base, uri)
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
    files::read_regular_file(&path).with_context(|| format!("reading {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
