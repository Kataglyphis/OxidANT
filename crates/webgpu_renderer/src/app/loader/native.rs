//! The native loader: a worker thread per load parses, the main thread polls its channel.

use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use super::{
    parse_bytes, LoadOutcome, LoadProgress, LoadStage, LoadedScene, SceneLoader, SceneRequest,
};
use crate::app::obj_glb::obj_to_glb;
use crate::app::scene_meta::SceneMeta;
use crate::asset::gltf_loader::{load_gltf_with, LoadOptions};

pub(super) enum State {
    Parsing(Receiver<Result<LoadedScene, String>>),
    /// Parsed; handed out on the next poll, so `uploading` shows for a frame first.
    Ready(Option<Result<LoadedScene, String>>),
}

impl SceneLoader {
    pub(super) fn begin(request: SceneRequest, options: LoadOptions) -> (State, LoadStage) {
        let (sender, receiver) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("scene-loader".to_owned())
            .spawn(move || {
                let started = Instant::now();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    load(request, &options)
                }))
                .unwrap_or_else(|_| Err("the loader panicked".to_owned()));
                if let Ok(loaded) = &result {
                    let seconds = started.elapsed().as_secs_f32();
                    let triangles = loaded.scene.triangle_count();
                    log::info!("Parsed {triangles} triangles in {seconds:.1} s");
                }
                // A replaced job dropped its receiver: its result goes nowhere, as intended.
                let _ = sender.send(result);
            });
        let state = match spawned {
            Ok(_) => State::Parsing(receiver),
            Err(error) => State::Ready(Some(Err(format!("no loader thread: {error}")))),
        };
        (state, LoadStage::Parsing)
    }

    pub(super) fn advance(
        state: &mut State,
        progress: &mut LoadProgress,
    ) -> Option<Result<LoadedScene, String>> {
        match state {
            State::Parsing(receiver) => {
                let result = match receiver.try_recv() {
                    Ok(result) => result,
                    Err(TryRecvError::Empty) => return None,
                    Err(TryRecvError::Disconnected) => {
                        Err("the loader stopped without a scene".to_owned())
                    }
                };
                if result.is_err() {
                    return Some(result);
                }
                progress.stage = LoadStage::Uploading;
                *state = State::Ready(Some(result));
                None
            }
            State::Ready(result) => result.take(),
        }
    }

    /// Polls until the load finishes or `timeout` passes, for snapshots and tests.
    pub fn wait(&mut self, timeout: Duration) -> Option<LoadOutcome> {
        let deadline = Instant::now() + timeout;
        while self.is_busy() && Instant::now() < deadline {
            if let Some(outcome) = self.poll() {
                return Some(outcome);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }
}

fn load(request: SceneRequest, options: &LoadOptions) -> Result<LoadedScene, String> {
    match request {
        SceneRequest::File(path) => load_file(&path, options),
        SceneRequest::Bytes(bytes) => parse_bytes(&bytes, options),
        SceneRequest::Url(url) => Err(format!("{url}: only the browser fetches scenes")),
    }
}

/// An OBJ through the converter in memory; a glTF through the importer, which reads its files beside it.
fn load_file(path: &Path, options: &LoadOptions) -> Result<LoadedScene, String> {
    let is_obj = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("obj"));
    if is_obj {
        let glb = obj_to_glb(path).map_err(|error| format!("{error:#}"))?;
        return parse_bytes(&glb, options);
    }
    let scene = load_gltf_with(path, options).map_err(|error| format!("{error:#}"))?;
    Ok(LoadedScene {
        scene,
        meta: SceneMeta::read(path),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/assets")
            .join(name)
    }

    #[test]
    fn a_load_parses_off_the_frame_and_shows_uploading_before_it_is_handed_out() {
        let mut loader = SceneLoader::default();
        loader.start(
            Some(3),
            "cube_on_plane",
            SceneRequest::File(asset("cube_on_plane.gltf")),
        );
        assert_eq!(loader.progress().map(|p| p.stage), Some(LoadStage::Parsing));
        let deadline = Instant::now() + Duration::from_secs(30);
        while loader.progress().map(|p| p.stage) == Some(LoadStage::Parsing)
            && Instant::now() < deadline
        {
            assert!(
                loader.poll().is_none(),
                "a parsed scene is not handed out before it shows uploading"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            loader.progress().map(|p| p.stage),
            Some(LoadStage::Uploading)
        );
        let outcome = loader.poll().expect("handed out on the next poll");
        assert_eq!(
            (outcome.entry, outcome.name.as_str()),
            (Some(3), "cube_on_plane")
        );
        assert_eq!(outcome.result.expect("parses").scene.primitives.len(), 2);
        assert!(!loader.is_busy());
    }

    #[test]
    fn a_broken_or_missing_file_is_an_error_not_a_panic() {
        let dir = std::env::temp_dir().join(format!("loader-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("broken.glb"), b"glTF\x02\x00\x00\x00garbage").unwrap();
        for path in [dir.join("broken.glb"), dir.join("missing.gltf")] {
            let mut loader = SceneLoader::default();
            loader.start(None, "broken", SceneRequest::File(path.clone()));
            let outcome = loader.wait(Duration::from_secs(30)).expect("finishes");
            let error = outcome.result.err().expect("fails");
            assert!(!error.is_empty(), "{path:?}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_newer_request_replaces_the_running_one() {
        let mut loader = SceneLoader::default();
        loader.start(
            Some(0),
            "first",
            SceneRequest::File(asset("cube_on_plane.gltf")),
        );
        loader.start(
            Some(1),
            "second",
            SceneRequest::File(asset("cube_textured.gltf")),
        );
        let outcome = loader.wait(Duration::from_secs(30)).expect("finishes");
        assert_eq!((outcome.entry, outcome.name.as_str()), (Some(1), "second"));
        assert!(
            loader.wait(Duration::from_millis(50)).is_none(),
            "the first never arrives"
        );
    }

    #[test]
    fn a_load_parses_with_the_options_it_started_with() {
        // A quad whose second triangle mirrors its UVs: only MikkTSpace splits the two corners both share.
        let quad = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/tests/assets/mirrored_uv_quad.gltf");
        let vertices = |loader: &mut SceneLoader, request: SceneRequest| {
            loader.start(None, "quad", request);
            let outcome = loader.wait(Duration::from_secs(30)).expect("finishes");
            outcome.result.expect("loads").scene.primitives[0]
                .vertices
                .len()
        };
        let mut loader = SceneLoader::default();
        assert_eq!(vertices(&mut loader, SceneRequest::File(quad.clone())), 4);
        loader.set_options(LoadOptions {
            mikktspace_tangents: true,
        });
        assert_eq!(vertices(&mut loader, SceneRequest::File(quad.clone())), 6);
        let bytes = std::fs::read(&quad).unwrap();
        assert_eq!(vertices(&mut loader, SceneRequest::Bytes(bytes.into())), 6);
    }
}
