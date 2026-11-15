//! Scene loads off the frame: parsed on a worker natively and fetched with progress in the browser, uploaded by the app.

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

use std::path::PathBuf;

use super::scene_meta::SceneMeta;
use crate::asset::gltf_loader::load_gltf_slice;
use crate::scene::CpuScene;

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
pub(super) fn parse_bytes(bytes: &[u8]) -> Result<LoadedScene, String> {
    let scene = load_gltf_slice(bytes).map_err(|error| format!("{error:#}"))?;
    Ok(LoadedScene {
        scene,
        meta: SceneMeta::parse(bytes),
    })
}

/// One load at a time; a new request replaces the running one, whose result is then dropped.
#[derive(Default)]
pub struct SceneLoader {
    job: Option<Job>,
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
        let (state, stage) = Self::begin(request);
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
}
