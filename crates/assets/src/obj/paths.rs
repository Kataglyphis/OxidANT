//! Where an OBJ's `.mtl` files and textures live, confined to the OBJ's folder like a glTF's buffers and images.

use std::path::{Path, PathBuf};

/// `path` with `\` as `/` and `.` and `dir/..` folded away; `None` for a drive, a root, a UNC name or a climb out.
pub(super) fn inside(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    if path.contains(':') || path.starts_with('/') {
        return None;
    }
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            _ => segments.push(segment),
        }
    }
    (!segments.is_empty()).then(|| segments.join("/"))
}

/// `name` from a file at `relative_dir` (relative to the OBJ's folder) as a path relative to that folder.
pub(super) fn placed(relative_dir: &str, name: &str) -> Option<String> {
    if relative_dir.is_empty() {
        inside(name)
    } else {
        // Checked alone first, so `../x` in a subfolder's .mtl cannot hide a root or a drive behind the prefix.
        let name = name.replace('\\', "/");
        if name.contains(':') || name.starts_with('/') {
            return None;
        }
        inside(&format!("{relative_dir}/{name}"))
    }
}

/// The folder of a path relative to the OBJ's folder, "" for the folder itself.
pub(super) fn folder(relative: &str) -> &str {
    relative.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// The file `relative` names inside the OBJ's folder, or the reason it is refused.
pub(super) fn confined(obj_path: &Path, relative: &str) -> anyhow::Result<PathBuf> {
    let base = obj_path.parent().unwrap_or_else(|| Path::new(""));
    crate::files::confined_path(base, relative)
}

/// A texture beside its `.mtl`, else in that folder's `textures/` (every textured shipped scene's layout); `None` with a
/// warning naming both when neither is a regular file inside the OBJ's folder.
pub fn texture_source(uri: &str, obj_path: &Path) -> Option<PathBuf> {
    // See https://github.com/Kataglyphis/BeschleunigerBallett/blob/develop/docs/source/model-loading.md
    let file = uri.rsplit_once('/').map_or(uri, |(_, file)| file);
    let under_textures = placed(folder(uri), &format!("textures/{file}"))?;
    match confined(obj_path, uri) {
        Ok(found) => Some(found),
        Err(beside) => {
            match confined(obj_path, &under_textures) {
                Ok(found) => Some(found),
                Err(error) => {
                    log::warn!("texture '{uri}': {beside:#}, and under textures/: {error:#}; drawn without it");
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_that_leaves_the_folder_is_refused() {
        assert_eq!(inside("tex/./a.png").as_deref(), Some("tex/a.png"));
        assert_eq!(inside("tex/../a.png").as_deref(), Some("a.png"));
        assert_eq!(inside("textures\\a.png").as_deref(), Some("textures/a.png"));
        for refused in [
            "../a.png",
            "/etc/passwd",
            "C:/a.png",
            "c:a.png",
            "\\\\server\\share\\a.png",
            "//server/a.png",
        ] {
            assert_eq!(inside(refused), None, "{refused}");
        }
        assert_eq!(placed("sub", "../a.png").as_deref(), Some("a.png"));
        assert_eq!(placed("sub", "../../a.png"), None);
        assert_eq!(placed("sub", "/a.png"), None);
        assert_eq!(folder("sub/a.mtl"), "sub");
        assert_eq!(folder("a.mtl"), "");
    }
}
