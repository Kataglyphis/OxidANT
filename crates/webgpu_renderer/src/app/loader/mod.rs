//! Scene loads off the frame: parsed on a worker natively and fetched with progress in the browser, uploaded by the app.

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

use std::path::PathBuf;

use super::scene_meta::SceneMeta;
use crate::asset::gltf_loader::{load_gltf_slice_with, LoadOptions};
use crate::scene::CpuScene;

/// Set to any non-empty value, it turns on MikkTSpace tangents ([`LoadOptions::mikktspace_tangents`]).
pub const MIKKTSPACE_TANGENTS_ENV: &str = "KATAGLYPHIS_MIKKTSPACE_TANGENTS";

/// The loader options the environment asks for, which an application reads once at its start; wasm32 has none set.
pub fn load_options_from_env() -> LoadOptions {
    load_options_from(std::env::var_os(MIKKTSPACE_TANGENTS_ENV))
}

/// An empty value counts as unset, as for the C++ engine's knobs.
fn load_options_from(mikktspace_tangents: Option<std::ffi::OsString>) -> LoadOptions {
    LoadOptions {
        mikktspace_tangents: mikktspace_tangents.is_some_and(|value| !value.is_empty()),
        ..LoadOptions::default()
    }
}

/// What to load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SceneRequest {
    /// A .gltf, .glb or .obj on disk.
    File(PathBuf),
    /// A whole .gltf or .glb in memory: an embedded scene, a browser drop.
    Bytes(std::sync::Arc<[u8]>),
    /// A .glb the browser fetches.
    Url(String),
}

/// Where a load is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadStage {
    Fetching,
    Parsing,
    Uploading,
}

/// A load's progress, for the status bar and the viewport chip.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadProgress {
    /// The scene's library name.
    pub name: String,
    pub stage: LoadStage,
    /// Done over all, when known: a fetch with a length.
    pub fraction: Option<f32>,
}

impl LoadProgress {
    /// `Loading Colosseum  ·  42 %`, or the stage when nothing is counted.
    pub fn text(&self) -> String {
        let detail = match (self.stage, self.fraction) {
            (LoadStage::Fetching, Some(fraction)) => {
                format!("{:.0} %", (fraction * 100.0).clamp(0.0, 100.0))
            }
            (LoadStage::Fetching, None) => "downloading".to_owned(),
            (LoadStage::Parsing, _) => "parsing".to_owned(),
            (LoadStage::Uploading, _) => "uploading".to_owned(),
        };
        format!("Loading {}  ·  {detail}", self.name)
    }
}

/// A finished parse: the scene and what its glTF says about itself.
pub struct LoadedScene {
    pub scene: CpuScene,
    pub meta: SceneMeta,
}

/// A finished load, handed out once: the request's library row and name, and the scene or why there is none.
pub struct LoadOutcome {
    pub entry: Option<usize>,
    pub name: String,
    pub result: Result<LoadedScene, String>,
}

/// Parses a whole .gltf or .glb in memory.
pub(super) fn parse_bytes(bytes: &[u8], options: &LoadOptions) -> Result<LoadedScene, String> {
    let scene = load_gltf_slice_with(bytes, options).map_err(|error| format!("{error:#}"))?;
    Ok(LoadedScene {
        scene,
        meta: SceneMeta::parse(bytes),
    })
}

/// One load at a time; a new request replaces the running one, whose result is then dropped.
#[derive(Default)]
pub struct SceneLoader {
    job: Option<Job>,
    /// What every load from now on parses with.
    options: LoadOptions,
}

/// The running load: what it is for, and the platform's state of it.
struct Job {
    entry: Option<usize>,
    progress: LoadProgress,
    #[cfg(not(target_arch = "wasm32"))]
    state: native::State,
    #[cfg(target_arch = "wasm32")]
    state: web::State,
}

impl SceneLoader {
    /// Starts loading `request` for library row `entry`; the scene on screen stays until [`Self::poll`] hands this one out.
    pub fn start(&mut self, entry: Option<usize>, name: &str, request: SceneRequest) {
        let (state, stage) = Self::begin(request, self.options);
        self.job = Some(Job {
            entry,
            progress: LoadProgress {
                name: name.to_owned(),
                stage,
                fraction: None,
            },
            state,
        });
    }

    /// The options the next [`Self::start`] parses with; a load already running keeps its own.
    pub fn set_options(&mut self, options: LoadOptions) {
        self.options = options;
    }

    pub fn options(&self) -> LoadOptions {
        self.options
    }

    pub fn progress(&self) -> Option<&LoadProgress> {
        self.job.as_ref().map(|job| &job.progress)
    }

    pub fn is_busy(&self) -> bool {
        self.job.is_some()
    }

    /// Advances the load; a parsed scene shows `uploading` for one poll before it is handed out for upload.
    pub fn poll(&mut self) -> Option<LoadOutcome> {
        let job = self.job.as_mut()?;
        let finished = Self::advance(&mut job.state, &mut job.progress)?;
        let job = self.job.take()?;
        Some(LoadOutcome {
            entry: job.entry,
            name: job.progress.name,
            result: finished,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_names_the_scene_and_its_stage() {
        let mut progress = LoadProgress {
            name: "Colosseum".to_owned(),
            stage: LoadStage::Fetching,
            fraction: Some(0.424),
        };
        assert_eq!(progress.text(), "Loading Colosseum  ·  42 %");
        progress.fraction = Some(1.7);
        assert_eq!(
            progress.text(),
            "Loading Colosseum  ·  100 %",
            "a compressed body overshoots"
        );
        progress.stage = LoadStage::Parsing;
        assert_eq!(progress.text(), "Loading Colosseum  ·  parsing");
    }

    #[test]
    fn the_environment_asks_for_mikktspace_by_a_value() {
        assert_eq!(load_options_from(None), LoadOptions::default());
        assert_eq!(
            load_options_from(Some("".into())),
            LoadOptions::default(),
            "empty is unset"
        );
        let set = load_options_from(Some("0".into()));
        assert!(
            set.mikktspace_tangents,
            "any value, as the C++ engine's knobs"
        );
        assert_eq!(MIKKTSPACE_TANGENTS_ENV, "KATAGLYPHIS_MIKKTSPACE_TANGENTS");
        let mut loader = SceneLoader::default();
        assert_eq!(loader.options(), LoadOptions::default());
        loader.set_options(set);
        assert_eq!(loader.options(), set);
    }
}
