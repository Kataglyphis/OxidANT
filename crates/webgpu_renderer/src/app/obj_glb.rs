//! A Wavefront OBJ as an in-memory GLB: the crate's converter, with its textures packed into the binary chunk.

use std::borrow::Cow;
use std::path::Path;

use anyhow::Context as _;
use gltf::json::Value;

use crate::asset::obj_to_gltf::{read_obj, texture_source, to_gltf};

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

/// Moves every image into `bin` behind a buffer view; returns the images no PNG or JPEG file backs.
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
        let view_index = views.len() - 1;
        bin.extend_from_slice(&bytes);
        let image = &mut root["images"][index];
        if let Some(image) = image.as_object_mut() {
            image.remove("uri");
        }
        image["bufferView"] = Value::from(view_index);
        image["mimeType"] = Value::from(mime);
    }
    Ok(missing)
}

/// The bytes and MIME type of the texture `uri` names, read as the converter finds it.
fn texture(uri: &str, obj_path: &Path) -> Option<(Vec<u8>, &'static str)> {
    let path = texture_source(uri, obj_path);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            log::warn!("{}: {error}; the material draws without it", path.display());
            return None;
        }
    };
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        "image/jpeg"
    } else {
        log::warn!(
            "{}: not a PNG or JPEG; the material draws without it",
            path.display()
        );
        return None;
    };
    Some((bytes, mime))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::gltf_loader::load_gltf_slice;

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
}
