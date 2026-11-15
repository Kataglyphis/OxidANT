//! The model library the scene dock lists: entries in scan order, shown as the C++ editor groups and sorts them.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

/// The browser's showcase scenes, small enough to embed; the first is the native fallback too.
pub const EMBEDDED_SCENES: &[(&str, &[u8])] = &[
    (
        "cube_on_plane",
        include_bytes!("../../tests/assets/cube_on_plane.gltf"),
    ),
    (
        "cube_textured",
        include_bytes!("../../tests/assets/cube_textured.gltf"),
    ),
    (
        "cube_animated",
        include_bytes!("../../tests/assets/cube_animated.gltf"),
    ),
    (
        "cube_morph",
        include_bytes!("../../tests/assets/cube_morph.gltf"),
    ),
    (
        "cube_unlit",
        include_bytes!("../../tests/assets/cube_unlit.gltf"),
    ),
];

/// The lighter Colosseum the browser fetches beside its page, from AssetANT's `Colosseum-web` release.
pub const WEB_COLOSSEUM: (&str, &str) = ("Colosseum", "Colosseum-web.glb");

/// Where an entry's bytes come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SceneSource {
    /// A .gltf, .glb or .obj on disk.
    File(PathBuf),
    /// Compiled into the binary.
    Embedded(&'static [u8]),
    /// Fetched by the browser, relative to the page unless absolute.
    Url(String),
}

/// One loadable scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryEntry {
    /// `folder/stem` with `/` separators, the C++ display name: the library groups by its folder.
    pub display: String,
    /// The tooltip's path or URL.
    pub location: String,
    pub source: SceneSource,
    /// The glTF `asset.copyright`, once a scan or a load has read it.
    pub credit: Option<String>,
}

impl LibraryEntry {
    /// The part before the last `/`, empty for a loose file.
    pub fn folder(&self) -> &str {
        split_display(&self.display).0
    }

    /// The part after the last `/`.
    pub fn leaf(&self) -> &str {
        split_display(&self.display).1
    }

    /// A file named by its parent folder and stem, as the C++ scan names `Models/<folder>/<file>`.
    pub fn file(path: &Path, location: String) -> Self {
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
        let folder = path.parent().and_then(Path::file_name);
        let display = match (folder, stem) {
            (Some(folder), Some(stem)) => format!("{}/{stem}", folder.to_string_lossy()),
            (None, Some(stem)) => stem,
            _ => path.display().to_string(),
        };
        Self {
            display,
            location,
            source: SceneSource::File(path.to_path_buf()),
            credit: None,
        }
    }
}

/// A run of entries in one folder; a loose file's folder is empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryGroup {
    pub folder: String,
    pub rows: Vec<usize>,
}

/// The entries in scan order, which load requests index, and the C++ display order over them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelLibrary {
    entries: Vec<LibraryEntry>,
    order: Vec<usize>,
}

impl ModelLibrary {
    pub const fn empty() -> Self {
        Self {
            entries: Vec::new(),
            order: Vec::new(),
        }
    }

    pub fn new(entries: Vec<LibraryEntry>) -> Self {
        let mut library = Self {
            entries,
            order: Vec::new(),
        };
        library.reorder();
        library
    }

    /// The browser's list: the embedded showcase scenes and the Colosseum fetched from `scene_base`.
    pub fn web(scene_base: &str) -> Self {
        let mut entries: Vec<LibraryEntry> = EMBEDDED_SCENES
            .iter()
            .map(|&(name, bytes)| LibraryEntry {
                display: name.to_owned(),
                location: format!("{name}.gltf (embedded)"),
                source: SceneSource::Embedded(bytes),
                credit: None,
            })
            .collect();
        let (name, file) = WEB_COLOSSEUM;
        let url = format!("{scene_base}{file}");
        entries.push(LibraryEntry {
            display: name.to_owned(),
            location: url.clone(),
            source: SceneSource::Url(url),
            credit: None,
        });
        Self::new(entries)
    }

    pub fn entries(&self) -> &[LibraryEntry] {
        &self.entries
    }

    pub fn entry(&self, index: usize) -> Option<&LibraryEntry> {
        self.entries.get(index)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The display order's runs of one folder each, folders first.
    pub fn groups(&self) -> Vec<LibraryGroup> {
        let mut groups: Vec<LibraryGroup> = Vec::new();
        for &index in &self.order {
            let folder = self.entries[index].folder();
            match groups.last_mut() {
                Some(group) if compare_ignoring_case(&group.folder, folder).is_eq() => {
                    group.rows.push(index);
                }
                _ => groups.push(LibraryGroup {
                    folder: folder.to_owned(),
                    rows: vec![index],
                }),
            }
        }
        groups
    }

    /// The entry reading `source`; files compare by what they resolve to, so `./a` finds `a`.
    pub fn position(&self, source: &SceneSource) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| same_source(&entry.source, source))
    }

    /// The entry reading the same source, else `entry` appended; returns its index.
    pub fn add(&mut self, entry: LibraryEntry) -> usize {
        if let Some(index) = self.position(&entry.source) {
            return index;
        }
        self.entries.push(entry);
        self.reorder();
        self.entries.len() - 1
    }

    /// Records the credit a load read, so the row's tooltip names it from then on.
    pub fn set_credit(&mut self, index: usize, credit: Option<String>) {
        if let Some(entry) = self.entries.get_mut(index) {
            entry.credit = credit;
        }
    }

    /// Folders first, then loose files; folder, then leaf, ignoring ASCII case, as the C++ `libraryOrder`.
    fn reorder(&mut self) {
        let entries = &self.entries;
        let mut order: Vec<usize> = (0..entries.len()).collect();
        order.sort_by(|&a, &b| {
            let (left, right) = (&entries[a], &entries[b]);
            left.folder()
                .is_empty()
                .cmp(&right.folder().is_empty())
                .then_with(|| compare_ignoring_case(left.folder(), right.folder()))
                .then_with(|| compare_ignoring_case(left.leaf(), right.leaf()))
        });
        self.order = order;
    }
}

/// `display` at its last `/`: the folder (empty for a loose file) and the leaf.
pub fn split_display(display: &str) -> (&str, &str) {
    display.rsplit_once('/').unwrap_or(("", display))
}

/// The C++ `compareIgnoringCase`: ASCII upper case, byte by byte, a prefix first.
pub fn compare_ignoring_case(left: &str, right: &str) -> Ordering {
    let upper = |text: &str| {
        text.bytes()
            .map(|b| b.to_ascii_uppercase())
            .collect::<Vec<u8>>()
    };
    upper(left).cmp(&upper(right))
}

fn same_source(left: &SceneSource, right: &SceneSource) -> bool {
    match (left, right) {
        (SceneSource::File(a), SceneSource::File(b)) => {
            a == b || resolved(a).is_some_and(|a| resolved(b).is_some_and(|b| a == b))
        }
        (SceneSource::Embedded(a), SceneSource::Embedded(b)) => std::ptr::eq(*a, *b),
        (a, b) => a == b,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn resolved(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

#[cfg(target_arch = "wasm32")]
fn resolved(_path: &Path) -> Option<PathBuf> {
    None
}

/// The C++ `ImGuiTextFilter`: comma-separated terms in order, the first match decides; a `-` term excludes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextFilter {
    /// Lower-cased terms, each with whether it excludes.
    terms: Vec<(bool, String)>,
}

impl TextFilter {
    pub fn parse(text: &str) -> Self {
        let terms = text
            .split(',')
            .map(str::trim)
            .filter_map(|term| match term.strip_prefix('-') {
                Some(excluded) => {
                    let excluded = excluded.trim();
                    (!excluded.is_empty()).then(|| (true, excluded.to_lowercase()))
                }
                None => (!term.is_empty()).then(|| (false, term.to_lowercase())),
            })
            .collect();
        Self { terms }
    }

    pub fn is_active(&self) -> bool {
        !self.terms.is_empty()
    }

    /// Case-insensitive substring terms; with no including term, whatever no `-` term caught passes.
    pub fn passes(&self, text: &str) -> bool {
        let text = text.to_lowercase();
        for (excludes, term) in &self.terms {
            if text.contains(term.as_str()) {
                return !excludes;
            }
        }
        self.terms.iter().all(|(excludes, _)| *excludes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(display: &str) -> LibraryEntry {
        LibraryEntry {
            display: display.to_owned(),
            location: format!("Models/{display}.obj"),
            source: SceneSource::Url(display.to_owned()),
            credit: None,
        }
    }

    #[test]
    fn folders_come_first_then_loose_files_each_ignoring_case() {
        let library = ModelLibrary::new(
            [
                "suzanne",
                "crytek-sponza/sponza_triag",
                "GltfTest/cube",
                "Plane",
                "Dinosaurs/dinosaurs",
                "GltfTest/Alpha",
                "gltftest/zeta",
            ]
            .map(entry)
            .to_vec(),
        );
        let groups: Vec<(String, Vec<&str>)> = library
            .groups()
            .into_iter()
            .map(|group| {
                let leaves = group
                    .rows
                    .iter()
                    .map(|&row| library.entries()[row].leaf())
                    .collect();
                (group.folder, leaves)
            })
            .collect();
        assert_eq!(
            groups,
            [
                ("crytek-sponza".to_owned(), vec!["sponza_triag"]),
                ("Dinosaurs".to_owned(), vec!["dinosaurs"]),
                ("GltfTest".to_owned(), vec!["Alpha", "cube", "zeta"]),
                (String::new(), vec!["Plane", "suzanne"]),
            ]
        );
    }

    #[test]
    fn the_filter_follows_imgui_text_filter() {
        let filter = TextFilter::parse(" dino , -saur");
        assert!(filter.is_active());
        assert!(
            filter.passes("Dinosaurs/dinosaurs"),
            "the first term decides"
        );
        assert!(!TextFilter::parse("-saur, dino").passes("Dinosaurs/dinosaurs"));
        assert!(
            !filter.passes("GltfTest/cube"),
            "an including term must match"
        );
        let only_excluding = TextFilter::parse("-cube,");
        assert!(only_excluding.passes("Plane"));
        assert!(!only_excluding.passes("GltfTest/Cube"));
        assert!(!TextFilter::parse("").is_active());
        assert!(TextFilter::parse(" , -").passes("anything"));
    }

    #[test]
    fn a_file_is_named_by_its_folder_and_stem_and_joins_once() {
        let file = LibraryEntry::file(Path::new("Models/Dinosaurs/dinosaurs.obj"), "x".into());
        assert_eq!(
            (file.display.as_str(), file.folder(), file.leaf()),
            ("Dinosaurs/dinosaurs", "Dinosaurs", "dinosaurs")
        );
        assert_eq!(
            LibraryEntry::file(Path::new("cube.glb"), "x".into()).display,
            "cube"
        );
        let mut library = ModelLibrary::default();
        assert_eq!(library.add(file.clone()), 0);
        assert_eq!(library.add(entry("Plane")), 1);
        assert_eq!(library.add(file), 0, "the same file never joins twice");
        assert_eq!(library.groups().len(), 2);
    }

    #[test]
    fn the_browser_lists_the_showcase_and_fetches_the_colosseum_beside_the_page() {
        let library = ModelLibrary::web("");
        let colosseum = library
            .entries()
            .iter()
            .find(|entry| entry.display == "Colosseum")
            .expect("the browser lists the Colosseum");
        assert_eq!(
            colosseum.source,
            SceneSource::Url("Colosseum-web.glb".into())
        );
        let elsewhere = ModelLibrary::web("https://example.org/scenes/");
        assert!(elsewhere.entries().iter().any(|entry| entry.source
            == SceneSource::Url("https://example.org/scenes/Colosseum-web.glb".into())));
        assert_eq!(library.entries().len(), EMBEDDED_SCENES.len() + 1);
    }
}
