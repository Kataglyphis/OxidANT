//! The app's scenes: the library, the load in flight, what is loaded, and the start view `FrameScene` returns to.

use std::sync::Arc;

use glam::{Vec3, Vec4};

use super::framing::{self, Framing};
use super::library::{ModelLibrary, SceneSource};
use super::loader::{LoadOutcome, LoadProgress, SceneLoader, SceneRequest};
use super::scene_meta::SceneMeta;
use super::StudioApp;
use crate::scene::camera::OrbitCamera;
use crate::scene::controller::OrbitController;
use crate::scene::{CpuLight, CpuLightKind, CpuScene};
use crate::ui::editor::{ModelSource, SceneStatus};

/// A line the status bar shows until the next load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// Something to do, such as fetching the scenes.
    Hint(String),
    /// A load that failed; the scene before it stays.
    Error(String),
}

/// What the app knows about scenes beyond the one uploaded.
pub(super) struct SceneState {
    library: ModelLibrary,
    /// The library row the renderer holds, which the library marks; `None` for a drop or the stand-in.
    loaded: Option<usize>,
    loader: SceneLoader,
    notice: Option<Notice>,
    meta: SceneMeta,
    /// The loaded scene's start view.
    framing: Option<Framing>,
    /// The shell's camera, whose direction and field of view a bounds fit keeps.
    base: OrbitCamera,
}

impl SceneState {
    pub(super) fn new(base: OrbitCamera) -> Self {
        Self {
            library: ModelLibrary::default(),
            loaded: None,
            loader: SceneLoader::default(),
            notice: None,
            meta: SceneMeta::default(),
            framing: None,
            base,
        }
    }

    /// The editor's view of it for one pass.
    pub(super) fn status(&self) -> SceneStatus<'_> {
        SceneStatus {
            library: &self.library,
            loaded: self.loaded,
            loading: self.loader.progress(),
            notice: self.notice.as_ref(),
            credit: self.meta.copyright.as_deref(),
        }
    }
}

impl StudioApp {
    pub fn library(&self) -> &ModelLibrary {
        &self.scene.library
    }

    /// Replaces the library; `loaded` is the row the scene on screen came from, if any.
    pub fn set_library(&mut self, library: ModelLibrary, loaded: Option<usize>) {
        self.scene.library = library;
        self.scene.loaded = loaded.filter(|&row| row < self.scene.library.entries().len());
    }

    /// The library row the renderer holds.
    pub fn loaded_entry(&self) -> Option<usize> {
        self.scene.loaded
    }

    pub fn notice(&self) -> Option<&Notice> {
        self.scene.notice.as_ref()
    }

    pub fn set_notice(&mut self, notice: Option<Notice>) {
        self.scene.notice = notice;
    }

    pub fn load_progress(&self) -> Option<&LoadProgress> {
        self.scene.loader.progress()
    }

    pub fn is_loading(&self) -> bool {
        self.scene.loader.is_busy()
    }

    /// The loaded scene's `asset.copyright`, which its licence asks to show.
    pub fn scene_credit(&self) -> Option<&str> {
        self.scene.meta.copyright.as_deref()
    }

    /// The loaded scene's start view and fly speed.
    pub fn scene_framing(&self) -> Option<&Framing> {
        self.scene.framing.as_ref()
    }

    /// Starts loading library row `row`; the scene on screen stays until the new one is ready.
    pub fn open_entry(&mut self, row: usize) {
        let Some(entry) = self.scene.library.entry(row) else {
            return;
        };
        let request = match &entry.source {
            SceneSource::File(path) => SceneRequest::File(path.clone()),
            SceneSource::Embedded(bytes) => SceneRequest::Bytes(Arc::from(*bytes)),
            SceneSource::Url(url) => SceneRequest::Url(url.clone()),
        };
        let name = entry.leaf().to_owned();
        self.scene.notice = None;
        self.scene.loader.start(Some(row), &name, request);
    }

    /// Starts loading a file, a command-line argument or a drop, which joins the library unless a row reads it.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_path(&mut self, path: &std::path::Path) {
        let cwd = std::env::current_dir().unwrap_or_default();
        let row = self.scene.library.file_row(path, &cwd);
        self.open_entry(row);
    }

    /// Starts loading a whole .gltf or .glb from memory, a browser drop, which no library row holds.
    pub fn open_bytes(&mut self, name: &str, bytes: Vec<u8>) {
        self.scene.notice = None;
        self.scene
            .loader
            .start(None, name, SceneRequest::Bytes(bytes.into()));
    }

    /// Finishes the load in flight now, for snapshots and tests; false when it is still running at `timeout`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn finish_loading(&mut self, timeout: std::time::Duration) -> bool {
        if let Some(outcome) = self.scene.loader.wait(timeout) {
            self.adopt(outcome);
        }
        !self.is_loading()
    }

    /// Swaps in `scene` at once, framed and lit as a load would be; no library row holds it.
    pub fn load_scene(&mut self, scene: CpuScene) {
        self.upload_loaded(scene, SceneMeta::default());
        self.scene.loaded = None;
    }

    /// Back to the loaded scene's start view, which the user asked for, so the idle spin stays off.
    pub fn frame_scene(&mut self) {
        self.apply_framing();
        self.controller.auto_orbit = false;
    }

    /// Takes a finished load once per frame, before the editor pass reads the state.
    pub(super) fn poll_scene(&mut self) {
        if let Some(outcome) = self.scene.loader.poll() {
            self.adopt(outcome);
        }
    }

    fn adopt(&mut self, outcome: LoadOutcome) {
        let loaded = match outcome.result {
            Ok(loaded) => loaded,
            Err(error) => {
                log::error!("Failed to load {}: {error}", outcome.name);
                self.scene.notice = Some(Notice::Error(format!(
                    "Could not load {}: {error}",
                    outcome.name
                )));
                return;
            }
        };
        if let Some(row) = outcome.entry {
            self.scene
                .library
                .set_credit(row, loaded.meta.copyright.clone());
        }
        let entry = outcome.entry.and_then(|row| self.scene.library.entry(row));
        self.model = Some(match entry {
            Some(entry) => ModelSource {
                name: entry.display.clone(),
                path: entry.location.clone(),
            },
            None => ModelSource {
                name: outcome.name.clone(),
                path: outcome.name.clone(),
            },
        });
        self.upload_loaded(loaded.scene, loaded.meta);
        self.scene.loaded = outcome.entry;
    }

    /// A loaded scene's first directional light becomes the Sun, then the scene uploads without it.
    fn upload_loaded(&mut self, mut scene: CpuScene, meta: SceneMeta) {
        if let Some(sun) = take_sun(&mut scene.lights) {
            self.adopt_sun(&sun);
        }
        self.upload(&scene, meta);
    }

    /// The Sun takes the light's direction and colour and keeps its intensity: glTF gives lux, the renderer no exposure for it.
    fn adopt_sun(&mut self, sun: &CpuLight) {
        let toward = -Vec3::from_array(sun.direction).normalize_or_zero();
        if !toward.is_finite() || toward == Vec3::ZERO {
            return;
        }
        let renderer = &mut self.renderer;
        renderer.light_dir_ambient = toward.extend(renderer.light_dir_ambient.w);
        let [red, green, blue] = sun.color;
        renderer.light_color_intensity =
            Vec4::new(red, green, blue, renderer.light_color_intensity.w);
    }

    /// The renderer takes `scene`; the camera starts at its framing and the animation clock over.
    pub(super) fn upload(&mut self, scene: &CpuScene, meta: SceneMeta) {
        self.renderer.upload_scene(&self.gpu, scene);
        let framing = framing::frame(scene, &meta, self.renderer.scene_bounds(), &self.scene.base);
        log::info!(
            "Uploaded {} primitives, {} triangles; start view from {:?}",
            scene.primitives.len(),
            scene.triangle_count(),
            framing.source
        );
        let auto_orbit = framing.auto_orbit();
        self.scene.meta = meta;
        self.scene.framing = Some(framing);
        self.controller = OrbitController::default();
        self.apply_framing();
        self.controller.auto_orbit = auto_orbit;
        self.epoch_s = None;
    }

    fn apply_framing(&mut self) {
        let Some(framing) = &self.scene.framing else {
            return;
        };
        self.home = framing.camera.clone();
        self.camera = OrbitCamera {
            viewport: self.camera.viewport,
            ..framing.camera.clone()
        };
        self.fly.speed = framing.fly_speed;
        self.controller.radius_range = framing.radius_range;
    }
}

/// The scene's first directional light, taken out of `lights`; the renderer's own sun casts its shadows.
fn take_sun(lights: &mut Vec<CpuLight>) -> Option<CpuLight> {
    let index = lights
        .iter()
        .position(|light| matches!(light.kind, CpuLightKind::Directional))?;
    Some(lights.remove(index))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use super::*;
    use crate::app::library::LibraryEntry;
    use crate::app::HostCaps;
    use crate::ui::editor::EditorAction;
    use crate::{load_gltf, GpuContext};

    fn asset(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/assets")
            .join(name)
    }

    fn studio() -> Option<StudioApp> {
        let gpu = GpuContext::headless_or_skip()?;
        let scene = load_gltf(asset("cube_on_plane.gltf")).expect("the test scene loads");
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let home = OrbitCamera::default();
        Some(StudioApp::headless(
            gpu,
            &scene,
            home,
            HostCaps::default(),
            format,
        ))
    }

    #[test]
    fn a_library_row_loads_off_the_frame_at_its_camera_and_frame_scene_returns_there() {
        let Some(mut app) = studio() else {
            return;
        };
        assert!(
            app.scene_framing().is_some_and(Framing::auto_orbit),
            "the cube is fitted to its bounds"
        );
        let entries = ["cube_textured.gltf", "cube_animated.gltf"]
            .map(|name| LibraryEntry::file(&asset(name), name.to_owned()));
        app.set_library(ModelLibrary::new(entries.to_vec()), None);
        assert!(app.act(vec![EditorAction::LoadModel(1)]).is_empty());
        assert!(app.is_loading(), "the load runs off the frame");
        assert!(app.finish_loading(Duration::from_secs(60)));
        assert_eq!(app.loaded_entry(), Some(1));
        let name = app.model.as_ref().map(|model| model.name.as_str());
        assert_eq!(name, Some("assets/cube_animated"));
        let start = app.camera.eye();
        assert!(
            start.abs_diff_eq(Vec3::new(0.0, 1.0, 5.0), 1e-3),
            "its camera: {start}"
        );
        assert!(
            !app.controller.auto_orbit,
            "an authored camera does not spin"
        );
        app.camera.yaw_deg += 90.0;
        assert!(app.act(vec![EditorAction::FrameScene]).is_empty());
        assert!(app.camera.eye().abs_diff_eq(start, 1e-3));
    }

    #[test]
    fn a_failed_load_keeps_the_scene_and_says_why() {
        let Some(mut app) = studio() else {
            return;
        };
        let before = app.model.clone();
        app.open_path(Path::new("/nonexistent/broken.gltf"));
        assert!(app.finish_loading(Duration::from_secs(60)));
        assert_eq!(app.model, before);
        assert_eq!(
            app.loaded_entry(),
            None,
            "the row joined the library, nothing is marked loaded"
        );
        match app.notice() {
            Some(Notice::Error(text)) => assert!(text.contains("broken"), "{text}"),
            other => panic!("expected an error notice, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod sun_tests {
    use super::*;

    fn light(kind: CpuLightKind, direction: [f32; 3]) -> CpuLight {
        CpuLight {
            kind,
            color: [1.0, 0.9, 0.76],
            intensity: 4098.0,
            range: 0.0,
            position: [0.0; 3],
            direction,
            node: None,
        }
    }

    #[test]
    fn the_first_directional_light_is_taken_as_the_sun_and_the_rest_stay() {
        let mut lights = vec![
            light(CpuLightKind::Point, [0.0, -1.0, 0.0]),
            light(CpuLightKind::Directional, [0.6, -0.53, -0.6]),
            light(CpuLightKind::Directional, [0.0, -1.0, 0.0]),
        ];
        let sun = take_sun(&mut lights).expect("a directional light");
        assert_eq!(sun.direction, [0.6, -0.53, -0.6]);
        assert_eq!(lights.len(), 2);
        assert!(matches!(lights[0].kind, CpuLightKind::Point));
        assert!(take_sun(&mut vec![light(CpuLightKind::Point, [0.0; 3])]).is_none());
    }
}
