//! Where the native viewer finds scenes: the command line, `KATAGLYPHIS_MODEL_OVERRIDE`, the fetched Colosseum, the cube.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::library::{LibraryEntry, ModelLibrary, SceneSource};
use super::scene_meta::SceneMeta;

/// The C++ engine's startup override: a path under `Resources/`, or absolute.
pub const MODEL_OVERRIDE: &str = "KATAGLYPHIS_MODEL_OVERRIDE";
/// The folder the scene fetch puts the native default in, AssetANT's `Colosseum-gltf`.
pub const DEFAULT_SCENE: &str = "Colosseum-gltf";
/// What the status bar says when the chain found no scene and the cube stands in.
pub const FETCH_HINT: &str = "No scene fetched: run scripts/linux/fetch-models.sh --scenes \
                              (Windows: scripts\\windows\\Get-Models.ps1 -Scenes)";
/// The C++ `kResourceSearchDepth`.
const RESOURCE_SEARCH_DEPTH: usize = 8;
const MODEL_EXTENSIONS: [&str; 3] = ["gltf", "glb", "obj"];

/// The folders the library lists, in the order a file found twice keeps its first entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SceneRoots {
    /// The command-line file's folder, listed without its subfolders.
    pub cli_folder: Option<PathBuf>,
    /// OxidANT's `resources/scenes`: under the working directory, then in this crate's checkout.
    pub scenes: Vec<PathBuf>,
    /// BeschleunigerBallett's `Resources/Models`, in the working directory or an ancestor, as the C++ looks.
    pub models: Option<PathBuf>,
}

impl SceneRoots {
    pub fn discover(cwd: &Path, checkout: &Path, cli: Option<&Path>) -> Self {
        let mut scenes: Vec<PathBuf> = Vec::new();
        for dir in [
            cwd.join("resources/scenes"),
            checkout.join("resources/scenes"),
        ] {
            if dir.is_dir() && !scenes.iter().any(|known| same_path(known, &dir)) {
                scenes.push(dir);
            }
        }
        let cli_folder = cli.and_then(Path::parent).map(|parent| {
            if parent.as_os_str().is_empty() {
                cwd.to_path_buf()
            } else {
                parent.to_path_buf()
            }
        });
        Self {
            cli_folder: cli_folder.filter(|dir| dir.is_dir()),
            scenes,
            models: resources_models(cwd),
        }
    }
}

/// The first `Resources/Models` in `cwd` or its ancestors, the C++ `searchAncestorsForRelative`.
pub fn resources_models(cwd: &Path) -> Option<PathBuf> {
    cwd.ancestors()
        .take(RESOURCE_SEARCH_DEPTH)
        .map(|dir| dir.join("Resources").join("Models"))
        .find(|dir| dir.is_dir())
}

/// BeschleunigerBallett's DOOM2016 skybox, searched like `Resources/Models`; elsewhere the procedural sky stays.
pub fn skybox_dir(cwd: &Path) -> Option<PathBuf> {
    cwd.ancestors()
        .take(RESOURCE_SEARCH_DEPTH)
        .map(|dir| dir.join(crate::asset::skybox::DOOM2016_DIR))
        .find(|dir| dir.is_dir())
}

/// The startup scene, and a note for every candidate the chain could not use.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Startup {
    pub path: Option<PathBuf>,
    pub notes: Vec<String>,
}

/// The command line, then the override, then the fetched Colosseum in OxidANT's and BeschleunigerBallett's trees.
pub fn startup_scene(
    cli: Option<&Path>,
    model_override: Option<&str>,
    roots: &SceneRoots,
    cwd: &Path,
) -> Startup {
    let mut notes = Vec::new();
    if let Some(cli) = cli {
        if cli.is_file() {
            return Startup {
                path: Some(cli.to_path_buf()),
                notes,
            };
        }
        notes.push(format!("{} does not exist", cli.display()));
    }
    if let Some(value) = model_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some(path) = resolve_override(value, roots, cwd) {
            return Startup {
                path: Some(path),
                notes,
            };
        }
        notes.push(format!("{MODEL_OVERRIDE}={value} names no file"));
    }
    let fetched = roots
        .scenes
        .iter()
        .chain(&roots.models)
        .find_map(|root| scene_file(&root.join(DEFAULT_SCENE)));
    if fetched.is_none() {
        notes.push(FETCH_HINT.to_owned());
    }
    Startup {
        path: fetched,
        notes,
    }
}

/// Under `Resources/` as the C++ reads it; absolute, or from the working directory, too.
fn resolve_override(value: &str, roots: &SceneRoots, cwd: &Path) -> Option<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute() {
        return path.is_file().then(|| path.to_path_buf());
    }
    let resources = roots.models.as_deref().and_then(Path::parent);
    resources
        .map(|resources| resources.join(path))
        .into_iter()
        .chain([cwd.join(path)])
        .find(|candidate| candidate.is_file())
}

/// `<dir>/<dir name>.gltf` or `.glb`, else the folder's first model by name.
fn scene_file(dir: &Path) -> Option<PathBuf> {
    let name = dir.file_name()?.to_str()?;
    let named = ["gltf", "glb"].map(|extension| dir.join(format!("{name}.{extension}")));
    named
        .into_iter()
        .find(|path| path.is_file())
        .or_else(|| scan(dir, false).into_iter().next())
}

/// Every root's models; a file found twice keeps the entry of the first root that lists it.
pub fn scan_library(roots: &SceneRoots, cwd: &Path) -> ModelLibrary {
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    let flat = roots.cli_folder.iter().map(|root| (root, false));
    let deep = roots
        .scenes
        .iter()
        .chain(&roots.models)
        .map(|root| (root, true));
    for (root, recursive) in flat.chain(deep) {
        for path in scan(root, recursive) {
            if !seen.insert(std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone())) {
                continue;
            }
            let mut entry = LibraryEntry::file(&path, shown(&path, cwd));
            if recursive {
                entry.display = display_under(root, &path).unwrap_or(entry.display);
            }
            entry.credit = credit(&path);
            entries.push(entry);
        }
    }
    ModelLibrary::new(entries)
}

/// The library row for `path`, added unless a row reads it already: a command-line file or a drop.
pub fn file_entry(path: &Path, cwd: &Path) -> LibraryEntry {
    let mut entry = LibraryEntry::file(path, shown(path, cwd));
    entry.credit = credit(path);
    entry
}

/// The model files in `root`, by name; hidden entries skipped, subfolders only when `recursive`.
pub fn scan(root: &Path, recursive: bool) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(listing) = std::fs::read_dir(&dir) else {
            continue;
        };
        for item in listing.flatten() {
            let Ok(kind) = item.file_type() else {
                continue;
            };
            let path = item.path();
            if item.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if kind.is_dir() {
                if recursive {
                    pending.push(path);
                }
            } else if is_model(&path) {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// A .gltf, .glb or .obj, by extension in any case, as the C++ `isSupportedModelPath`.
pub fn is_model(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            MODEL_EXTENSIONS
                .iter()
                .any(|model| extension.eq_ignore_ascii_case(model))
        })
}

/// The C++ display name under a root: the folders below it joined by `/`, then the stem.
fn display_under(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let stem = relative.file_stem()?.to_string_lossy();
    let folders: Vec<String> = relative
        .parent()?
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    Some(if folders.is_empty() {
        stem.into_owned()
    } else {
        format!("{}/{stem}", folders.join("/"))
    })
}

/// A glTF's credit, read from its JSON alone.
fn credit(path: &Path) -> Option<String> {
    let gltf = path
        .extension()
        .is_some_and(|extension| !extension.eq_ignore_ascii_case("obj"));
    gltf.then(|| SceneMeta::read(path).copyright).flatten()
}

/// The path from the working directory when it lies below it.
fn shown(path: &Path, cwd: &Path) -> String {
    path.strip_prefix(cwd).unwrap_or(path).display().to_string()
}

fn same_path(left: &Path, right: &Path) -> bool {
    let resolve = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    resolve(left) == resolve(right)
}

/// The viewer's library and startup scene: its command line, the environment, the working directory and this checkout.
pub fn native_startup(cli: Option<&Path>) -> (ModelLibrary, Startup) {
    let cwd = std::env::current_dir().unwrap_or_default();
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let checkout = manifest.parent().and_then(Path::parent).unwrap_or(manifest);
    let roots = SceneRoots::discover(&cwd, checkout, cli);
    let library = scan_library(&roots, &cwd);
    let model_override = std::env::var(MODEL_OVERRIDE).ok();
    let startup = startup_scene(cli, model_override.as_deref(), &roots, &cwd);
    (library, startup)
}

impl ModelLibrary {
    /// The row reading `path`, added if none does.
    pub fn file_row(&mut self, path: &Path, cwd: &Path) -> usize {
        match self.position(&SceneSource::File(path.to_path_buf())) {
            Some(row) => row,
            None => self.add(file_entry(path, cwd)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A BeschleunigerBallett tree, an OxidANT tree and a loose folder, each with a Colosseum where it belongs.
    struct Trees {
        root: PathBuf,
    }

    impl Trees {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("discovery-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            let credited =
                r#"{"asset": {"version": "2.0", "copyright": "Colosseum by Sulo456, CC BY 4.0"}}"#;
            for (file, text) in [
                ("bb/Resources/Models/Dinosaurs/dinosaurs.obj", ""),
                ("bb/Resources/Models/GltfTest/cube.glb", ""),
                ("bb/Resources/Models/GltfTest/sub/card.gltf", ""),
                ("bb/Resources/Models/plane.OBJ", ""),
                ("bb/Resources/Models/notes.txt", ""),
                ("bb/Resources/Models/.hidden/secret.gltf", ""),
                (
                    "bb/Resources/Models/Colosseum-gltf/Colosseum-gltf.gltf",
                    credited,
                ),
                (
                    "ox/resources/scenes/Colosseum-gltf/Colosseum-gltf.gltf",
                    credited,
                ),
                ("ox/resources/scenes/Colosseum-gltf/textures/a.png", ""),
                ("loose/thing.glb", ""),
            ] {
                let path = root.join(file);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, text).unwrap();
            }
            Self { root }
        }

        fn path(&self, relative: &str) -> PathBuf {
            self.root.join(relative)
        }
    }

    impl Drop for Trees {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn the_chain_takes_the_command_line_then_the_override_then_the_fetched_colosseum() {
        let trees = Trees::new("chain");
        let cwd = trees.path("bb");
        let roots = SceneRoots::discover(&cwd, &trees.path("ox"), None);
        assert_eq!(roots.models, Some(trees.path("bb/Resources/Models")));
        let cli = trees.path("loose/thing.glb");
        let chosen = startup_scene(Some(&cli), Some("Models/plane.OBJ"), &roots, &cwd);
        assert_eq!((chosen.path, chosen.notes.len()), (Some(cli), 0));
        let missing = trees.path("loose/gone.glb");
        let overridden = startup_scene(Some(&missing), Some("Models/plane.OBJ"), &roots, &cwd);
        assert_eq!(
            overridden.path,
            Some(trees.path("bb/Resources/Models/plane.OBJ"))
        );
        assert!(
            overridden.notes[0].contains("gone.glb"),
            "{:?}",
            overridden.notes
        );
        let fetched = startup_scene(None, Some(" "), &roots, &cwd);
        let colosseum = "ox/resources/scenes/Colosseum-gltf/Colosseum-gltf.gltf";
        assert_eq!(
            fetched.path,
            Some(trees.path(colosseum)),
            "OxidANT's fetch first"
        );
        let bb_only = SceneRoots {
            scenes: Vec::new(),
            ..roots.clone()
        };
        let from_bb = startup_scene(None, Some("Models/nowhere.obj"), &bb_only, &cwd);
        let bb_colosseum = "bb/Resources/Models/Colosseum-gltf/Colosseum-gltf.gltf";
        assert_eq!(from_bb.path, Some(trees.path(bb_colosseum)));
        assert!(
            from_bb.notes[0].starts_with(MODEL_OVERRIDE),
            "{:?}",
            from_bb.notes
        );
    }

    #[test]
    fn with_nothing_fetched_the_cube_stands_in_and_the_hint_names_the_fetch() {
        let trees = Trees::new("empty");
        let roots = SceneRoots::discover(&trees.path("loose"), &trees.path("loose"), None);
        assert_eq!(roots, SceneRoots::default());
        let startup = startup_scene(None, None, &roots, &trees.path("loose"));
        assert_eq!(startup.path, None);
        assert_eq!(startup.notes, [FETCH_HINT]);
        assert!(FETCH_HINT.contains("fetch-models.sh --scenes"));
    }

    #[test]
    fn the_scan_lists_models_under_every_root_by_folder_once_each() {
        let trees = Trees::new("scan");
        let cwd = trees.path("bb");
        let roots = SceneRoots::discover(
            &cwd,
            &trees.path("ox"),
            Some(&trees.path("loose/thing.glb")),
        );
        let library = scan_library(&roots, &cwd);
        let mut names: Vec<&str> = library
            .entries()
            .iter()
            .map(|entry| entry.display.as_str())
            .collect();
        names.sort_unstable();
        assert_eq!(
            names,
            [
                "Colosseum-gltf/Colosseum-gltf",
                "Colosseum-gltf/Colosseum-gltf",
                "Dinosaurs/dinosaurs",
                "GltfTest/cube",
                "GltfTest/sub/card",
                "loose/thing",
                "plane",
            ],
            "OBJ listed, hidden folders and other files skipped; both Colosseums are distinct files"
        );
        let dinosaurs = library
            .entries()
            .iter()
            .find(|e| e.display == "Dinosaurs/dinosaurs")
            .unwrap();
        assert_eq!(
            dinosaurs.location,
            "Resources/Models/Dinosaurs/dinosaurs.obj"
        );
        let colosseum = library
            .entries()
            .iter()
            .find(|e| e.display.starts_with("Colosseum"))
            .unwrap();
        assert_eq!(
            colosseum.credit.as_deref(),
            Some("Colosseum by Sulo456, CC BY 4.0")
        );
        let mut library = library;
        let rows = library.entries().len();
        let again = library.file_row(
            &trees.path("bb/Resources/Models/GltfTest/../GltfTest/cube.glb"),
            &cwd,
        );
        assert_eq!(
            (
                library.entries()[again].display.as_str(),
                library.entries().len()
            ),
            ("GltfTest/cube", rows)
        );
        let elsewhere = library.file_row(&trees.path("loose/thing.glb"), &cwd);
        assert_eq!(library.entries()[elsewhere].display, "loose/thing");
    }
}
