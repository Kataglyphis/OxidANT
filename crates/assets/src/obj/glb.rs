//! A Wavefront OBJ as an in-memory GLB for the WebGPU renderer: the converter's glTF, its textures in the binary chunk.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

use anyhow::Context as _;
use gltf::json::Value;

use super::gltf_json::to_gltf;
use super::material::MASK_CUTOFF;
use super::parse::{read_obj, ObjMesh};
use super::paths::texture_source;
use crate::image_decode;

/// One white texel, standing in for a texture no file holds: the image stays, nothing samples it.
const WHITE_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xf8, 0x0f, 0x04, 0x00,
    0x09, 0xfb, 0x03, 0xfd, 0x68, 0xfa, 0x1c, 0xcc, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
    0xae, 0x42, 0x60, 0x82,
];
/// The material slots `to_gltf` writes, as JSON paths under a material.
const TEXTURE_SLOTS: [(&str, Option<&str>); 3] = [
    ("pbrMetallicRoughness", Some("baseColorTexture")),
    ("normalTexture", None),
    ("emissiveTexture", None),
];

/// `obj_path` converted and packed as one GLB, which `load_gltf_slice` reads; a missing texture only warns.
pub fn obj_to_glb(obj_path: &Path) -> anyhow::Result<Vec<u8>> {
    let mesh = read_obj(obj_path)?;
    let (json, mut bin) = to_gltf(&mesh, "");
    let mut root: Value =
        gltf::json::deserialize::from_str(&json).context("the OBJ converter's glTF")?;
    let missing = pack_images(&mut root, &mut bin, obj_path)?;
    drop_texture_slots(&mut root, &missing);
    apply_masks(&mut root, &mut bin, &mesh, obj_path)?;
    let buffer = &mut root["buffers"][0];
    if let Some(buffer) = buffer.as_object_mut() {
        buffer.remove("uri");
    }
    buffer["byteLength"] = Value::from(bin.len());
    let glb = gltf::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            length: 0,
        },
        json: Cow::Owned(gltf::json::serialize::to_vec(&root)?),
        bin: Some(Cow::Owned(bin)),
    };
    glb.to_vec().context("packing the OBJ as a GLB")
}

/// Moves every image into `bin` behind a buffer view; returns the images no readable file backs.
fn pack_images(root: &mut Value, bin: &mut Vec<u8>, obj_path: &Path) -> anyhow::Result<Vec<usize>> {
    let count = root["images"].as_array().map_or(0, Vec::len);
    let mut missing = Vec::new();
    for index in 0..count {
        let uri = root["images"][index]["uri"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let (bytes, mime) = match texture(&uri, obj_path) {
            Some(found) => found,
            None => {
                missing.push(index);
                (WHITE_PNG.to_vec(), "image/png")
            }
        };
        let view = push_view(root, bin, &bytes)?;
        let image = &mut root["images"][index];
        if let Some(image) = image.as_object_mut() {
            image.remove("uri");
        }
        image["bufferView"] = Value::from(view);
        image["mimeType"] = Value::from(mime);
    }
    Ok(missing)
}

/// Appends `bytes` to `bin`, 4-byte aligned, behind a new buffer view; returns the view's index.
fn push_view(root: &mut Value, bin: &mut Vec<u8>, bytes: &[u8]) -> anyhow::Result<usize> {
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let view = format!(
        r#"{{"buffer": 0, "byteOffset": {}, "byteLength": {}}}"#,
        bin.len(),
        bytes.len()
    );
    let views = root["bufferViews"]
        .as_array_mut()
        .context("the converter writes buffer views")?;
    views.push(gltf::json::deserialize::from_str(&view)?);
    bin.extend_from_slice(bytes);
    Ok(views.len() - 1)
}

/// The bytes and MIME type of the texture `uri` names, packed as stored: `image_decode` reads BMP, TGA and the rest
/// back from their MIME type, so nothing is re-encoded; a file whose header does not read is left out.
fn texture(uri: &str, obj_path: &Path) -> Option<(Vec<u8>, &'static str)> {
    let path = texture_source(uri, obj_path)?;
    let bytes = read_texture(&path)?;
    let format = image_decode::image_format(&bytes, None, Some(&path.to_string_lossy()));
    let Some(mime_type) = format.and_then(image_decode::mime_type_of) else {
        log::warn!(
            "{}: not an image this renderer decodes, so the material draws without it",
            path.display()
        );
        return None;
    };
    let reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format?);
    if let Err(error) = reader.into_dimensions() {
        log::warn!("{}: {error}; the material draws without it", path.display());
        return None;
    }
    Some((bytes, mime_type))
}

/// A regular file only: a device name or a FIFO would block the loader.
fn read_texture(path: &Path) -> Option<Vec<u8>> {
    crate::files::read_regular_file(path)
        .inspect_err(|error| {
            log::warn!(
                "{}: {error:#}; the material draws without it",
                path.display()
            );
        })
        .ok()
}

/// The file `uri` names decoded to RGBA8, as both renderers decode it.
fn decode(uri: &str, obj_path: &Path) -> Option<image::RgbaImage> {
    let path = texture_source(uri, obj_path)?;
    let bytes = read_texture(&path)?;
    let texture = image_decode::decode_rgba8(&bytes, None, Some(&path.to_string_lossy()))
        .inspect_err(|error| {
            log::warn!(
                "{}: {error:#}; the material draws without it",
                path.display()
            );
        })
        .ok()?;
    image::RgbaImage::from_raw(texture.width, texture.height, texture.rgba8)
}

fn encode_png(image: &image::RgbaImage) -> Option<Vec<u8>> {
    let mut png = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut png, image::ImageFormat::Png)
        .inspect_err(|error| log::warn!("encoding a texture as PNG: {error}"))
        .ok()?;
    Some(png.into_inner())
}

/// Unhooks the material slots that sample a missing image; `to_gltf` gives texture `i` image `i`.
fn drop_texture_slots(root: &mut Value, missing: &[usize]) {
    let Some(materials) = root["materials"].as_array_mut() else {
        return;
    };
    for material in materials {
        for (key, nested) in TEXTURE_SLOTS {
            let (parent, slot) = match nested {
                Some(slot) => (&mut material[key], slot),
                None => (&mut *material, key),
            };
            let index = parent[slot]["index"].as_u64().map(|index| index as usize);
            if index.is_some_and(|index| missing.contains(&index)) {
                if let Some(parent) = parent.as_object_mut() {
                    parent.remove(slot);
                }
            }
        }
    }
}

/// glTF has no opacity slot, so each `map_d` material samples its base colour with the mask's first channel as alpha.
fn apply_masks(
    root: &mut Value,
    bin: &mut Vec<u8>,
    mesh: &ObjMesh,
    obj_path: &Path,
) -> anyhow::Result<()> {
    let mut composites: HashMap<(Option<&str>, &str), usize> = HashMap::new();
    for (index, material) in mesh.materials.iter().enumerate() {
        let Some(mask_uri) = material.alpha_texture.as_deref() else {
            continue;
        };
        let pbr = &root["materials"][index]["pbrMetallicRoughness"];
        // A base colour texture the packer dropped composites over white, as an untextured one does.
        let base_uri = material
            .base_color_texture
            .as_deref()
            .filter(|_| pbr.get("baseColorTexture").is_some());
        let key = (base_uri, mask_uri);
        let texture = match composites.get(&key) {
            Some(&texture) => texture,
            None => {
                let Some(png) = masked_base_color(base_uri, mask_uri, obj_path) else {
                    continue;
                };
                let texture = push_texture(root, bin, &png)?;
                composites.insert(key, texture);
                texture
            }
        };
        let gltf_material = &mut root["materials"][index];
        gltf_material["pbrMetallicRoughness"]["baseColorTexture"] =
            gltf::json::deserialize::from_str(&format!(r#"{{"index": {texture}}}"#))?;
        gltf_material["alphaMode"] = Value::from("MASK");
        gltf_material["alphaCutoff"] = Value::from(f64::from(MASK_CUTOFF));
    }
    Ok(())
}

/// The base colour image with alpha multiplied by the mask's red, as the C++ shaders test it; the mask is resized to fit.
fn masked_base_color(base_uri: Option<&str>, mask_uri: &str, obj_path: &Path) -> Option<Vec<u8>> {
    let mask = decode(mask_uri, obj_path)?;
    let mut base = match base_uri {
        Some(uri) => decode(uri, obj_path)?,
        None => image::RgbaImage::from_pixel(mask.width(), mask.height(), image::Rgba([255; 4])),
    };
    let mask = if mask.dimensions() == base.dimensions() {
        mask
    } else {
        let (width, height) = base.dimensions();
        image::imageops::resize(&mask, width, height, image::imageops::FilterType::Triangle)
    };
    for (texel, cover) in base.pixels_mut().zip(mask.pixels()) {
        texel[3] = ((u16::from(texel[3]) * u16::from(cover[0]) + 127) / 255) as u8;
    }
    encode_png(&base)
}

/// A PNG image and a texture sampling it with OBJ's repeat; returns the texture's index.
fn push_texture(root: &mut Value, bin: &mut Vec<u8>, png: &[u8]) -> anyhow::Result<usize> {
    let view = push_view(root, bin, png)?;
    // A model whose only texture is a mask has none of these arrays yet.
    for key in ["images", "textures", "samplers"] {
        if !root[key].is_array() {
            root[key] = Value::Array(Vec::new());
        }
    }
    let samplers = root["samplers"].as_array_mut().context("samplers")?;
    if samplers.is_empty() {
        samplers.push(gltf::json::deserialize::from_str(
            r#"{"wrapS": 10497, "wrapT": 10497}"#,
        )?);
    }
    let images = root["images"].as_array_mut().context("images")?;
    images.push(gltf::json::deserialize::from_str(&format!(
        r#"{{"bufferView": {view}, "mimeType": "image/png"}}"#
    ))?);
    let image = images.len() - 1;
    let textures = root["textures"].as_array_mut().context("textures")?;
    textures.push(gltf::json::deserialize::from_str(&format!(
        r#"{{"source": {image}, "sampler": 0}}"#
    ))?);
    Ok(textures.len() - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gltf_loader::load_gltf_slice;
    use crate::scene::{AlphaMode, CpuScene};

    /// A quad whose .mtl names one texture that exists and one that does not.
    fn quad_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("obj-glb-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("textures")).unwrap();
        let obj =
            "mtllib quad.mtl\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\n\
                   usemtl lit\nf 1/1 2/2 3/3\nusemtl gone\nf 1/1 3/3 4/4\n";
        std::fs::write(dir.join("quad.obj"), obj).unwrap();
        let mtl = "newmtl lit\nKd 1 1 1\nmap_Kd white.png\nnewmtl gone\nKd 1 0 0\nmap_Kd nowhere.png\nmap_Bump nowhere.png\n";
        std::fs::write(dir.join("quad.mtl"), mtl).unwrap();
        std::fs::write(dir.join("textures/white.png"), WHITE_PNG).unwrap();
        dir
    }

    #[test]
    fn an_obj_loads_from_memory_with_its_textures_and_without_the_missing_ones() {
        let dir = quad_dir();
        let glb = obj_to_glb(&dir.join("quad.obj")).expect("converts");
        let scene = load_gltf_slice(&glb).expect("the GLB loads");
        assert_eq!(scene.triangle_count(), 2);
        let textured: Vec<bool> = scene
            .primitives
            .iter()
            .map(|primitive| primitive.material.base_color_texture.is_some())
            .collect();
        assert_eq!(
            textured,
            [true, false],
            "found in textures/, the other dropped"
        );
        assert!(scene.primitives[1].material.normal_texture.is_none());
        assert_eq!(scene.primitives[1].material.base_color[0], 1.0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_obj_is_an_error() {
        let error = obj_to_glb(Path::new("/nonexistent/model.obj")).unwrap_err();
        assert!(format!("{error:#}").contains("model.obj"), "{error:#}");
    }

    /// `crates/assets/tests/assets/obj_mask`: a 2x2 card, `map_Kd red.bmp`, and `map_d half_mask.tga` clearing its left half.
    fn mask_card() -> CpuScene {
        let obj = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/obj_mask/card.obj");
        load_gltf_slice(&obj_to_glb(&obj).expect("converts")).expect("the GLB loads")
    }

    #[test]
    fn a_bmp_is_packed_as_stored_not_re_encoded() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/obj_mask");
        let glb = obj_to_glb(&dir.join("card.obj")).expect("converts");
        let gltf::Gltf { document, blob } =
            gltf::Gltf::from_slice_without_validation(&glb).unwrap();
        let blob = blob.unwrap();
        let packed: Vec<(String, &[u8])> = document
            .images()
            .filter_map(|image| match image.source() {
                gltf::image::Source::View { view, mime_type } => Some((
                    mime_type.to_owned(),
                    &blob[view.offset()..view.offset() + view.length()],
                )),
                gltf::image::Source::Uri { .. } => None,
            })
            .collect();
        let mimes: Vec<&str> = packed.iter().map(|(m, _)| m.as_str()).collect();
        let bmp = std::fs::read(dir.join("red.bmp")).unwrap();
        assert!(
            packed
                .iter()
                .any(|(m, bytes)| m == "image/bmp" && *bytes == bmp.as_slice()),
            "red.bmp is not packed as its own bytes: {mimes:?}"
        );
    }

    #[test]
    fn a_map_d_mask_becomes_the_base_colour_alpha_of_a_mask_material() {
        let scene = mask_card();
        let material = &scene.primitives[0].material;
        assert_eq!(material.alpha_mode, AlphaMode::Mask(0.5));
        let texture = &material
            .base_color_texture
            .as_ref()
            .expect("the BMP decodes")
            .texture;
        assert_eq!((texture.width, texture.height), (8, 8));
        let texel = |x: usize| &texture.rgba8[x * 4..x * 4 + 4];
        assert_eq!(texel(0), [255, 0, 0, 0], "the TGA's black left half is cut");
        assert_eq!(texel(7), [255, 0, 0, 255], "its white right half stays");
    }
}
