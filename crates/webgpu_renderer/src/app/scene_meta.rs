//! What a glTF says about itself beyond its geometry: the credit its licence asks for and the camera its author left active.

use gltf::json::Value;

/// The longest credit kept; a longer one is cut with an ellipsis.
const MAX_CREDIT_CHARS: usize = 300;
/// The largest JSON read for metadata, so a scan never swallows a huge embedded-buffer .gltf.
#[cfg(not(target_arch = "wasm32"))]
const MAX_JSON_BYTES: u64 = 64 << 20;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SceneMeta {
    /// `asset.copyright` on one line.
    pub copyright: Option<String>,
    /// The node `scenes[scene].extras.active_camera` names, by node name first, then by camera name.
    pub active_camera_node: Option<usize>,
    /// That camera's name, as the file writes it.
    pub active_camera: Option<String>,
}

impl SceneMeta {
    /// From a .gltf's JSON or a whole .glb; unreadable metadata is none, never an error.
    pub fn parse(bytes: &[u8]) -> Self {
        let json = glb_json(bytes).unwrap_or(bytes);
        gltf::json::deserialize::from_slice::<Value>(json)
            .map(|root| Self::of(&root))
            .unwrap_or_default()
    }

    /// From a file, reading only its JSON: a .glb's first chunk, a .gltf whole.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn read(path: &std::path::Path) -> Self {
        read_json(path)
            .map(|json| Self::parse(&json))
            .unwrap_or_default()
    }

    fn of(root: &Value) -> Self {
        let scene = root["scene"].as_u64().unwrap_or(0) as usize;
        let active = root["scenes"][scene]["extras"]["active_camera"].as_str();
        Self {
            copyright: root["asset"]["copyright"].as_str().and_then(one_line),
            active_camera_node: active.and_then(|name| camera_node(root, name)),
            active_camera: active.map(str::to_owned),
        }
    }
}

/// The camera node called `name`, else the node whose camera is called `name`.
fn camera_node(root: &Value, name: &str) -> Option<usize> {
    let nodes = root["nodes"].as_array()?;
    let by_node = nodes
        .iter()
        .position(|node| !node["camera"].is_null() && node["name"].as_str() == Some(name));
    by_node.or_else(|| {
        nodes.iter().position(|node| {
            let camera = node["camera"].as_u64().map(|index| index as usize);
            camera.is_some_and(|index| root["cameras"][index]["name"].as_str() == Some(name))
        })
    })
}

/// Whitespace collapsed to single spaces and the length capped; empty is none.
fn one_line(text: &str) -> Option<String> {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.is_empty() {
        return None;
    }
    if joined.chars().count() <= MAX_CREDIT_CHARS {
        return Some(joined);
    }
    let cut: String = joined.chars().take(MAX_CREDIT_CHARS - 1).collect();
    Some(format!("{}…", cut.trim_end()))
}

/// A GLB's JSON chunk; `None` when `bytes` is no GLB.
fn glb_json(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.get(0..4)? != b"glTF" || bytes.get(16..20)? != b"JSON" {
        return None;
    }
    let length = u32::from_le_bytes(bytes.get(12..16)?.try_into().ok()?) as usize;
    bytes.get(20..20 + length)
}

#[cfg(not(target_arch = "wasm32"))]
fn read_json(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read as _;
    let mut file = std::fs::File::open(path)?;
    let mut head = [0u8; 20];
    let read = file.read(&mut head)?;
    if read == head.len() && &head[0..4] == b"glTF" && &head[16..20] == b"JSON" {
        let length = u64::from(u32::from_le_bytes([head[12], head[13], head[14], head[15]]));
        let mut json = Vec::new();
        file.take(length.min(MAX_JSON_BYTES))
            .read_to_end(&mut json)?;
        return Ok(json);
    }
    let mut json = head[..read].to_vec();
    file.take(MAX_JSON_BYTES).read_to_end(&mut json)?;
    Ok(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLOSSEUM_LIKE: &str = r#"{
        "asset": {"version": "2.0", "copyright": "Colosseum by Sulo456 (https://github.com/Sulo456),\n  CC BY 4.0"},
        "scene": 0,
        "scenes": [{"nodes": [0, 1, 2], "extras": {"active_camera": "Kamera_Arena"}}],
        "cameras": [{"name": "Kamera", "type": "perspective"}, {"name": "Arena data", "type": "perspective"}],
        "nodes": [{"name": "Kamera", "camera": 0}, {"name": "Kamera_Arena"}, {"name": "Kamera_Arena", "camera": 1}]
    }"#;

    /// `json` as the only chunk of a GLB.
    fn glb(json: &str) -> Vec<u8> {
        let mut chunk = json.as_bytes().to_vec();
        while !chunk.len().is_multiple_of(4) {
            chunk.push(b' ');
        }
        let mut bytes = b"glTF".to_vec();
        bytes.extend(2u32.to_le_bytes());
        bytes.extend((20 + chunk.len() as u32).to_le_bytes());
        bytes.extend((chunk.len() as u32).to_le_bytes());
        bytes.extend(b"JSON");
        bytes.extend(chunk);
        bytes
    }

    #[test]
    fn the_credit_and_the_active_camera_come_from_gltf_and_glb_alike() {
        for bytes in [COLOSSEUM_LIKE.as_bytes().to_vec(), glb(COLOSSEUM_LIKE)] {
            let meta = SceneMeta::parse(&bytes);
            assert_eq!(
                meta.copyright.as_deref(),
                Some("Colosseum by Sulo456 (https://github.com/Sulo456), CC BY 4.0")
            );
            assert_eq!(meta.active_camera.as_deref(), Some("Kamera_Arena"));
            assert_eq!(
                meta.active_camera_node,
                Some(2),
                "the camera node of that name, not the plain node before it"
            );
        }
    }

    #[test]
    fn an_active_camera_may_name_the_camera_instead_of_its_node() {
        let json = COLOSSEUM_LIKE.replace(
            "\"active_camera\": \"Kamera_Arena\"",
            "\"active_camera\": \"Arena data\"",
        );
        assert_eq!(
            SceneMeta::parse(json.as_bytes()).active_camera_node,
            Some(2)
        );
        let unknown = COLOSSEUM_LIKE.replace("Kamera_Arena\"}", "Nowhere\"}");
        assert_eq!(
            SceneMeta::parse(unknown.as_bytes()).active_camera_node,
            None
        );
    }

    #[test]
    fn metadata_that_is_missing_or_broken_is_none() {
        assert_eq!(SceneMeta::parse(b"not json"), SceneMeta::default());
        assert_eq!(
            SceneMeta::parse(br#"{"asset": {"copyright": "  "}}"#),
            SceneMeta::default()
        );
        let long = format!(r#"{{"asset": {{"copyright": "{}"}}}}"#, "a".repeat(400));
        let credit = SceneMeta::parse(long.as_bytes()).copyright.unwrap();
        assert_eq!(credit.chars().count(), MAX_CREDIT_CHARS);
        assert!(credit.ends_with('…'));
    }

    #[test]
    fn a_file_is_read_up_to_its_json() {
        let dir = std::env::temp_dir().join(format!("scene-meta-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scene.glb");
        let mut bytes = glb(COLOSSEUM_LIKE);
        bytes.extend([0u8; 64]);
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(SceneMeta::read(&path).active_camera_node, Some(2));
        assert_eq!(
            SceneMeta::read(&dir.join("missing.gltf")),
            SceneMeta::default()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
