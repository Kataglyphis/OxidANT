//! glTF import with `KHR_texture_basisu` and `EXT_texture_webp`, which `gltf::import` refuses: KTX2 through
//! `load_ktx2`, WebP through `image_decode`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context as _;

use crate::files::{self, decode_base64};
use crate::image_decode::decode_rgba8;
use crate::ktx2_loader::load_ktx2;
use crate::scene::CpuTexture;

/// Read by `attributes::read_f32`; gltf 1.4.1 refuses it as required.
const QUANTIZATION: &str = "KHR_mesh_quantization";

/// A texture extension naming an image in place of the core `source`; gltf 1.4.1 has no type for either.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ImageExtension {
    Basisu,
    Webp,
}

impl ImageExtension {
    /// In the order a texture prefers them: blocks the device samples before a decoded WebP.
    pub(crate) const PREFERRED: [ImageExtension; 2] =
        [ImageExtension::Basisu, ImageExtension::Webp];

    pub(crate) fn name(self) -> &'static str {
        match self {
            ImageExtension::Basisu => "KHR_texture_basisu",
            ImageExtension::Webp => "EXT_texture_webp",
        }
    }

    /// The format its image is in, for the log.
    pub(crate) fn format(self) -> &'static str {
        match self {
            ImageExtension::Basisu => "KTX2",
            ImageExtension::Webp => "WebP",
        }
    }
}

/// Where one glTF texture's image comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TextureSource {
    /// Each extension's image in [`ImageExtension::PREFERRED`]'s order; the first that loads is the texture.
    pub preferred: [Option<usize>; 2],
    /// The core `source`: the whole source without an extension, the fallback with one.
    pub fallback: Option<usize>,
}

impl TextureSource {
    /// The images to try in order, with the extension naming each; the core source last, with none.
    pub(crate) fn candidates(&self) -> impl Iterator<Item = (Option<ImageExtension>, usize)> + '_ {
        ImageExtension::PREFERRED
            .into_iter()
            .zip(self.preferred)
            .filter_map(|(extension, image)| Some((Some(extension), image?)))
            .chain(self.fallback.map(|image| (None, image)))
    }
}

/// A parsed, validated glTF or GLB, its binary chunk and each texture's sources.
pub(crate) struct Parsed {
    pub document: gltf::Document,
    pub blob: Option<Vec<u8>>,
    pub sources: Vec<TextureSource>,
}

/// `gltf::Gltf::from_slice`, except that it accepts `KHR_texture_basisu` and `EXT_texture_webp` as required and as a
/// texture's only source, and `KHR_mesh_quantization` as required.
pub(crate) fn parse(bytes: &[u8]) -> anyhow::Result<Parsed> {
    let gltf::Gltf { document, blob } = gltf::Gltf::from_slice_without_validation(bytes)?;
    let mut root = document.into_json();
    let image_count = root.images.len();
    let mut sources = Vec::with_capacity(root.textures.len());
    for (index, texture) in root.textures.iter_mut().enumerate() {
        let mut preferred = [None; 2];
        for (slot, extension) in preferred.iter_mut().zip(ImageExtension::PREFERRED) {
            *slot = extension_source(texture, index, image_count, extension)?;
        }
        let core = texture.source.value();
        let fallback = (core != u32::MAX as usize).then_some(core);
        if fallback.is_none() {
            if let Some(image) = preferred.into_iter().flatten().next() {
                // Validation wants a core source, and `Texture::source` unwraps it.
                texture.source = gltf::json::Index::new(image as u32);
            }
        }
        sources.push(TextureSource {
            preferred,
            fallback,
        });
    }
    let accepted = ImageExtension::PREFERRED.map(ImageExtension::name);
    root.extensions_required
        .retain(|name| !accepted.contains(&name.as_str()) && name != QUANTIZATION);
    if !root.extensions_used.iter().any(|name| name == QUANTIZATION) {
        normalize_core_tex_coords(&mut root);
    }
    reject_dangling_positions(&root)?;
    reject_sourceless_images(&root)?;
    let document = gltf::Document::from_json(root)?;
    crate::accessor_check::check_primitive_accessors(&document)?;
    crate::accessor_check::check_skin_accessors(&document)?;
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

/// Core glTF allows a u8 or u16 TEXCOORD only normalized, and gltf's reader normalized one without the flag.
fn normalize_core_tex_coords(root: &mut gltf::json::Root) {
    use gltf::json::accessor::{ComponentType, GenericComponentType};
    use gltf::json::validation::Checked;
    let tex_coords: Vec<usize> = root
        .meshes
        .iter()
        .flat_map(|mesh| &mesh.primitives)
        .flat_map(|primitive| &primitive.attributes)
        .filter(|(semantic, _)| {
            matches!(
                semantic,
                Checked::Valid(gltf::json::mesh::Semantic::TexCoords(_))
            )
        })
        .map(|(_, index)| index.value())
        .collect();
    for index in tex_coords {
        if let Some(accessor) = root.accessors.get_mut(index) {
            if matches!(
                accessor.component_type,
                Checked::Valid(GenericComponentType(ComponentType::U8 | ComponentType::U16))
            ) {
                accessor.normalized = true;
            }
        }
    }
}

/// gltf-json 1.4.1 validates neither: `Image::source` unwraps the uri, or a bufferView's mimeType, and panics.
fn reject_sourceless_images(root: &gltf::json::Root) -> anyhow::Result<()> {
    for (i, image) in root.images.iter().enumerate() {
        match (&image.buffer_view, &image.uri) {
            (Some(_), _) => anyhow::ensure!(
                image.mime_type.is_some(),
                "images[{i}] names a bufferView but no mimeType"
            ),
            (None, Some(_)) => {}
            (None, None) => anyhow::bail!("images[{i}] has neither a uri nor a bufferView"),
        }
    }
    Ok(())
}

/// The image `texture` names through `extension`, `None` without that extension.
fn extension_source(
    texture: &gltf::json::Texture,
    index: usize,
    image_count: usize,
    extension: ImageExtension,
) -> anyhow::Result<Option<usize>> {
    let name = extension.name();
    let Some(json) = texture
        .extensions
        .as_ref()
        .and_then(|extensions| extensions.others.get(name))
    else {
        return Ok(None);
    };
    let image = json
        .get("source")
        .and_then(gltf::json::Value::as_u64)
        .with_context(|| format!("texture {index}: {name} has no image source"))?;
    anyhow::ensure!(
        image < image_count as u64,
        "texture {index}: {name} source {image} is past the {image_count} images"
    );
    Ok(Some(image as usize))
}

/// Every image, index-aligned with the document's; a failed KTX2, or an image an extension names, stays an error for
/// a fallback to replace.
pub(crate) fn import_images(
    document: &gltf::Document,
    base: Option<&Path>,
    buffers: &[gltf::buffer::Data],
    sources: &[TextureSource],
) -> anyhow::Result<Vec<anyhow::Result<Arc<CpuTexture>>>> {
    let preferred: std::collections::HashSet<usize> = sources
        .iter()
        .flat_map(|source| source.preferred.into_iter().flatten())
        .collect();
    document
        .images()
        .map(|image| {
            if let Some(bytes) = ktx2_bytes(&image, base, buffers) {
                let texture = bytes
                    .and_then(|bytes| load_ktx2(&bytes))
                    .with_context(|| format!("glTF image {} (KTX2)", image.index()));
                return Ok(texture.map(Arc::new));
            }
            let texture = image_bytes(&image, base, buffers)
                .and_then(|stored| stored.decode())
                .with_context(|| format!("glTF image {}", image.index()));
            if preferred.contains(&image.index()) {
                return Ok(texture.map(Arc::new));
            }
            Ok(Ok(Arc::new(texture?)))
        })
        .collect()
}

/// An image's bytes as stored, with what names its format: the MIME type, and a file's URI.
#[derive(Clone)]
pub(crate) struct StoredImage {
    pub bytes: Vec<u8>,
    pub mime_type: Option<String>,
    pub uri: Option<String>,
}

impl StoredImage {
    pub(crate) fn decode(&self) -> anyhow::Result<CpuTexture> {
        decode_rgba8(&self.bytes, self.mime_type.as_deref(), self.uri.as_deref())
    }

    /// [`StoredImage::decode`] into room for its whole mip chain.
    pub(crate) fn decode_for_chain(&self) -> anyhow::Result<CpuTexture> {
        crate::image_decode::decode_rgba8_into(
            &self.bytes,
            self.mime_type.as_deref(),
            self.uri.as_deref(),
            |width, height| {
                crate::levels::chain_bytes(
                    width,
                    height,
                    crate::levels::full_level_count(width, height),
                )
            },
        )
    }
}

/// The file an image URI names beside the document, by `read_confined`'s rule; an in-memory import has none.
fn image_file(base: Option<&Path>, uri: &str) -> anyhow::Result<PathBuf> {
    let base = base.with_context(|| {
        format!("glTF image '{uri}' is an external file, which an in-memory import cannot read")
    })?;
    files::confined_file(base, uri)
}

/// Each texture's image: the first of its candidates that loads (a warning for each that did not), else its last
/// one's error.
pub(crate) fn resolve_textures(
    sources: &[TextureSource],
    images: Vec<anyhow::Result<Arc<CpuTexture>>>,
) -> anyhow::Result<Vec<Arc<CpuTexture>>> {
    let mut used = vec![false; images.len()];
    let mut textures = Vec::with_capacity(sources.len());
    for (index, source) in sources.iter().enumerate() {
        let candidates: Vec<usize> = source.candidates().map(|(_, image)| image).collect();
        let Some(&last) = candidates.last() else {
            anyhow::bail!("glTF texture {index} has no image");
        };
        let chosen = candidates
            .iter()
            .copied()
            .find(|&image| images[image].is_ok())
            .unwrap_or(last);
        let texture = match &images[chosen] {
            Ok(texture) => texture,
            Err(err) => anyhow::bail!("glTF texture {index}: {err:#}"),
        };
        for err in candidates
            .iter()
            .take_while(|&&image| image != chosen)
            .filter_map(|&image| images[image].as_ref().err())
        {
            log::warn!("glTF texture {index}: {err:#}; using its image {chosen}");
        }
        used[chosen] = true;
        textures.push(Arc::clone(texture));
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
                is_ktx2.then(|| {
                    let file = image_file(base, uri)?;
                    std::fs::read(&file).with_context(|| format!("reading {}", file.display()))
                })
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

/// An image's bytes as stored, so a path need not be UTF-8 and nothing opens a file `confined_file` did not check.
pub(crate) fn image_bytes(
    image: &gltf::Image,
    base: Option<&Path>,
    buffers: &[gltf::buffer::Data],
) -> anyhow::Result<StoredImage> {
    locate_image(image, base, buffers)?.load()
}

/// Where an image's bytes are: copied out of a buffer view or data URI, or a checked file still to read.
pub(crate) enum LocatedImage {
    Stored(StoredImage),
    File {
        path: PathBuf,
        mime_type: Option<String>,
        uri: String,
    },
}

impl LocatedImage {
    pub(crate) fn load(&self) -> anyhow::Result<StoredImage> {
        match self {
            LocatedImage::Stored(stored) => Ok(stored.clone()),
            LocatedImage::File {
                path,
                mime_type,
                uri,
            } => Ok(StoredImage {
                bytes: std::fs::read(path)
                    .with_context(|| format!("reading {}", path.display()))?,
                mime_type: mime_type.clone(),
                uri: Some(uri.clone()),
            }),
        }
    }
}

/// An image located without its document's buffers held any longer: a view or data URI is copied, a file is not read.
pub(crate) fn locate_image(
    image: &gltf::Image,
    base: Option<&Path>,
    buffers: &[gltf::buffer::Data],
) -> anyhow::Result<LocatedImage> {
    match image.source() {
        gltf::image::Source::View { view, mime_type } => Ok(LocatedImage::Stored(StoredImage {
            bytes: view_bytes(&view, buffers)?,
            mime_type: Some(mime_type.to_owned()),
            uri: None,
        })),
        gltf::image::Source::Uri { uri, mime_type } => {
            if let Some(data) = uri.strip_prefix("data:") {
                let (media, payload) = data
                    .split_once(";base64,")
                    .context("a data URI image that is not base64")?;
                // Padded base64 only: a truncated payload is a broken image, not a shorter one.
                anyhow::ensure!(
                    !payload.is_empty() && payload.len().is_multiple_of(4),
                    "a data URI image whose base64 is truncated"
                );
                return Ok(LocatedImage::Stored(StoredImage {
                    bytes: decode_base64(payload)?,
                    mime_type: Some(mime_type.unwrap_or(media).to_owned()),
                    uri: None,
                }));
            }
            Ok(LocatedImage::File {
                path: image_file(base, uri)?,
                mime_type: mime_type.map(str::to_owned),
                uri: uri.to_owned(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texture(basisu: Option<usize>, fallback: Option<usize>) -> TextureSource {
        TextureSource {
            preferred: [basisu, None],
            fallback,
        }
    }

    fn solid(value: u8) -> anyhow::Result<Arc<CpuTexture>> {
        Ok(Arc::new(CpuTexture {
            width: 1,
            height: 1,
            rgba8: vec![value; 4],
            compressed: None,
        }))
    }

    /// Each shape made `Image::source` panic on a fuzzed file (Windows lane, 2026-11-20); now each is an error.
    #[test]
    fn an_image_without_a_source_is_an_error_not_a_panic() {
        let doc = |images: &str| {
            format!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":4,"uri":"data:application/octet-stream;base64,AAAAAA=="}}],"bufferViews":[{{"buffer":0,"byteLength":4}}],"images":[{images}]}}"#
            )
        };
        let no_source = parse(doc("{}").as_bytes())
            .err()
            .expect("an image with no source loaded");
        assert!(
            format!("{no_source:#}").contains("neither a uri nor a bufferView"),
            "{no_source:#}"
        );
        let no_mime = parse(doc(r#"{"bufferView":0}"#).as_bytes())
            .err()
            .expect("a bufferView image without a mimeType loaded");
        assert!(
            format!("{no_mime:#}").contains("no mimeType"),
            "{no_mime:#}"
        );
        assert!(parse(doc(r#"{"bufferView":0,"mimeType":"image/png"}"#).as_bytes()).is_ok());
        assert!(parse(doc(r#"{"uri":"tex.png"}"#).as_bytes()).is_ok());
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
    fn a_textures_images_are_tried_basisu_then_webp_then_core() {
        let broken = || Err(anyhow::anyhow!("broken"));
        let images = vec![solid(1), solid(2), solid(3), broken(), broken()];
        let both = |basisu, webp, fallback| TextureSource {
            preferred: [basisu, webp],
            fallback,
        };
        let sources = [
            both(Some(0), Some(1), Some(2)),
            both(Some(3), Some(1), Some(2)),
            both(Some(3), Some(4), Some(2)),
            both(None, Some(1), Some(2)),
            both(None, Some(4), Some(2)),
            both(None, Some(1), None),
        ];
        let resolved = resolve_textures(&sources, images).unwrap();
        let firsts: Vec<u8> = resolved.iter().map(|t| t.rgba8[0]).collect();
        assert_eq!(firsts, [1, 2, 3, 2, 3, 2]);
        let images = vec![solid(1), broken()];
        let err = resolve_textures(&[both(None, Some(1), None)], images).unwrap_err();
        assert!(format!("{err:#}").contains("broken"), "{err:#}");
    }

    /// Each texture names the same one image through its extensions, or only through `EXT_texture_webp`.
    fn webp_document(extensions: &str, required: &str) -> String {
        format!(
            r#"{{"asset":{{"version":"2.0"}},"images":[{{"uri":"a.webp"}},{{"uri":"b.png"}}],"textures":[{{"extensions":{{{extensions}}}}}],"extensionsUsed":[{required}],"extensionsRequired":[{required}]}}"#
        )
    }

    #[test]
    fn ext_texture_webp_is_accepted_as_required_and_as_the_only_source() {
        let only = r#""EXT_texture_webp":{"source":0}"#;
        let parsed = parse(webp_document(only, r#""EXT_texture_webp""#).as_bytes()).unwrap();
        assert_eq!(parsed.sources, [texture(None, None).with_webp(0)]);
        assert_eq!(
            parsed.document.textures().next().unwrap().source().index(),
            0
        );
        let both = r#""KHR_texture_basisu":{"source":1},"EXT_texture_webp":{"source":0}"#;
        let required = r#""KHR_texture_basisu","EXT_texture_webp""#;
        let parsed = parse(webp_document(both, required).as_bytes()).unwrap();
        assert_eq!(parsed.sources, [texture(Some(1), None).with_webp(0)]);
        let past = r#""EXT_texture_webp":{"source":2}"#;
        let err = parse(webp_document(past, r#""EXT_texture_webp""#).as_bytes())
            .err()
            .expect("source 2 does not exist");
        assert!(
            format!("{err:#}").contains("EXT_texture_webp source 2 is past the 2 images"),
            "{err:#}"
        );
        let draco = parse(
            webp_document(only, r#""EXT_texture_webp","KHR_draco_mesh_compression""#).as_bytes(),
        )
        .err()
        .expect("draco is not supported");
        assert!(
            format!("{draco:#}").contains("KHR_draco_mesh_compression"),
            "{draco:#}"
        );
    }

    impl TextureSource {
        fn with_webp(mut self, image: usize) -> TextureSource {
            self.preferred[1] = Some(image);
            self
        }
    }

    #[test]
    fn a_failed_basisu_image_without_a_fallback_fails_the_load() {
        let images = vec![Err(anyhow::anyhow!("broken KTX2"))];
        let err = resolve_textures(&[texture(Some(0), None)], images).unwrap_err();
        assert!(format!("{err:#}").contains("broken KTX2"), "{err:#}");
    }
}
