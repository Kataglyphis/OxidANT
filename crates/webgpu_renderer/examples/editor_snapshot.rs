//! The Studio editor rendered headless into PNGs: `editor_snapshot -- [out_dir] [--model=PATH] [--web-model=PATH] [WxH@scale[,tab][,about][,hidden][,web][,deferred][,cloudy][,pitch=DEG][,zoom=F] ...]`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context as _;
use kataglyphis_webgpu_renderer::app::discovery::{native_startup, skybox_dir};
use kataglyphis_webgpu_renderer::app::library::{ModelLibrary, SceneSource, WEB_COLOSSEUM};
use kataglyphis_webgpu_renderer::render::forward::RasterPath;
use kataglyphis_webgpu_renderer::ui::{DockPrefs, ModelSource, SceneObject};
use kataglyphis_webgpu_renderer::{
    load_gltf, GpuContext, HostCaps, OffscreenFrame, OrbitCamera, StudioApp,
};

/// The C++ default window at 100 % and 150 %, the same window on a 150 % monitor, and the docs iframe.
const DEFAULT_SHOTS: &[&str] = &[
    "1600x900@1",
    "1600x900@1.5",
    "2400x1350@1.5",
    "800x600@1,web",
];
const MODEL: &str = "tests/assets/cube_on_plane.gltf";
/// Frames before the shot: egui lays a new window out in an unseen first pass, and the FPS needs two ticks.
const WARM_UP_FRAMES: u32 = 4;
/// Lavapipe parses and uploads the 5.9 M-triangle Colosseum in minutes, not seconds.
const LOAD_TIMEOUT: Duration = Duration::from_secs(1800);

/// One PNG: size and scale, inspector tab, About, editor hidden, the browser's host, the raster path, clouds on, a pitch and zoom.
struct Shot {
    size: [u32; 2],
    scale: f32,
    object: SceneObject,
    about: bool,
    hidden: bool,
    web: bool,
    path: RasterPath,
    cloudy: bool,
    pitch: Option<f32>,
    /// Multiplies the start view's distance to its target.
    zoom: f32,
}

fn parse(spec: &str) -> anyhow::Result<Shot> {
    let mut parts = spec.split(',');
    let frame = parts.next().unwrap_or_default();
    let (size, scale) = frame.split_once('@').unwrap_or((frame, "1"));
    let (width, height) = size.split_once('x').context("a shot is WxH@scale")?;
    let mut shot = Shot {
        size: [width.parse()?, height.parse()?],
        scale: scale.parse()?,
        object: SceneObject::Model,
        about: false,
        hidden: false,
        web: false,
        path: RasterPath::Forward,
        cloudy: false,
        pitch: None,
        zoom: 1.0,
    };
    for option in parts {
        if let Some(pitch) = option.strip_prefix("pitch=") {
            shot.pitch = Some(pitch.parse()?);
            continue;
        }
        if let Some(zoom) = option.strip_prefix("zoom=") {
            shot.zoom = zoom.parse()?;
            continue;
        }
        match option {
            "about" => shot.about = true,
            "hidden" => shot.hidden = true,
            "web" => shot.web = true,
            "deferred" => shot.path = RasterPath::Deferred,
            "cloudy" => shot.cloudy = true,
            name => {
                shot.object = SceneObject::ALL
                    .into_iter()
                    .find(|object| object.name().eq_ignore_ascii_case(name))
                    .with_context(|| format!("no outliner object named {name}"))?;
            }
        }
    }
    Ok(shot)
}

fn file_name(spec: &str) -> String {
    let safe: String = spec
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("editor-{safe}.png")
}

/// The shot's pitch and zoom applied to the host's start view, so no shot inherits the last one's.
fn aim(camera: &mut OrbitCamera, start: &OrbitCamera, shot: &Shot) {
    camera.pitch_deg = shot.pitch.unwrap_or(start.pitch_deg);
    camera.radius = start.radius * shot.zoom;
}

/// `host.now_s` runs on across shots, so the frame clock never steps back.
fn render(app: &mut StudioApp, host: &mut Host, shot: &Shot, path: &Path) -> anyhow::Result<()> {
    app.editor_mut().select(shot.object);
    app.renderer.raster_path = shot.path;
    app.editor_mut().set_visible(!shot.hidden);
    app.renderer.clouds.enabled = shot.cloudy;
    aim(&mut app.camera, &host.start, shot);
    if shot.about {
        app.editor_mut().show_about();
    } else {
        app.editor_mut().hide_about();
    }
    let mut pixels = Vec::new();
    for _ in 0..=WARM_UP_FRAMES {
        let frame = OffscreenFrame {
            size_px: shot.size,
            pixels_per_point: shot.scale,
            now_s: host.now_s,
            events: Vec::new(),
        };
        pixels = app.render_offscreen(&frame)?.0;
        host.now_s += 1.0 / 60.0;
    }
    let [width, height] = shot.size;
    image::save_buffer(path, &pixels, width, height, image::ColorType::Rgba8)?;
    println!(
        "wrote {} ({width}x{height} at {})",
        path.display(),
        shot.scale
    );
    Ok(())
}

/// One host's clock and the view it opened on.
struct Host {
    now_s: f64,
    start: OrbitCamera,
}

/// The scenes the two hosts open before their first shot: the viewer's model and the browser's Colosseum file.
#[derive(Default)]
struct Models {
    native: Option<PathBuf>,
    web: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let mut args = std::env::args().skip(1);
    let out_dir = PathBuf::from(args.next().unwrap_or_else(|| "snapshots".into()));
    let mut models = Models::default();
    let mut specs: Vec<String> = Vec::new();
    for arg in args {
        if let Some(path) = arg.strip_prefix("--model=") {
            models.native = Some(PathBuf::from(path));
        } else if let Some(path) = arg.strip_prefix("--web-model=") {
            models.web = Some(PathBuf::from(path));
        } else {
            specs.push(arg);
        }
    }
    let specs: Vec<&str> = if specs.is_empty() {
        DEFAULT_SHOTS.to_vec()
    } else {
        specs.iter().map(String::as_str).collect()
    };
    std::fs::create_dir_all(&out_dir)?;
    // Each host keeps its own clock, so its frame history never sees the other's frames.
    let mut hosts: [Option<(StudioApp, Host)>; 2] = [None, None];
    for spec in specs {
        let shot = parse(spec)?;
        let slot = &mut hosts[usize::from(shot.web)];
        if slot.is_none() {
            let app = studio(shot.web, &models)?;
            let start = app.camera.clone();
            *slot = Some((app, Host { now_s: 0.0, start }));
        }
        let (app, host) = slot.as_mut().expect("built above");
        render(app, host, &shot, &out_dir.join(file_name(spec)))?;
    }
    Ok(())
}

/// Native: an sRGB surface, every shell action live; web: a plain canvas format (egui blends in gamma space), none.
fn studio(web: bool, models: &Models) -> anyhow::Result<StudioApp> {
    let model = Path::new(env!("CARGO_MANIFEST_DIR")).join(MODEL);
    let gpu = GpuContext::new_headless()?;
    let scene = load_gltf(&model)?;
    let (caps, format) = if web {
        (HostCaps::default(), wgpu::TextureFormat::Rgba8Unorm)
    } else {
        let all = HostCaps {
            quit: true,
            screenshot: true,
            hot_reload: true,
        };
        (all, wgpu::TextureFormat::Rgba8UnormSrgb)
    };
    let mut app = StudioApp::headless(gpu, &scene, OrbitCamera::default(), caps, format);
    // Named relative to the crate, as the C++ editor names a model relative to its resources.
    app.model = Some(ModelSource::from_path(Path::new(MODEL)));
    if web {
        app.editor_mut().set_layout(DockPrefs::web());
        open_web(&mut app, models.web.as_deref());
    } else {
        // As the viewer: the C++ skybox where a BeschleunigerBallett checkout holds it.
        if let Some(dir) = skybox_dir(&std::env::current_dir()?) {
            app.load_skybox(&dir);
        }
        if let Some(path) = &models.native {
            let (library, _) = native_startup(Some(path));
            app.set_library(library, None);
            app.open_path(path);
        }
    }
    if !app.finish_loading(LOAD_TIMEOUT) {
        anyhow::bail!("the scene did not load within {LOAD_TIMEOUT:?}");
    }
    if let Some(notice) = app.notice() {
        eprintln!("{notice:?}");
    }
    Ok(app)
}

/// The browser's library, its Colosseum read from `path` as the page would fetch it, and opened when given.
fn open_web(app: &mut StudioApp, path: Option<&Path>) {
    let entries = ModelLibrary::web("")
        .entries()
        .iter()
        .cloned()
        .map(|mut entry| {
            if let (SceneSource::Url(_), Some(path)) = (&entry.source, path) {
                entry.source = SceneSource::File(path.to_path_buf());
            }
            entry
        })
        .collect();
    app.set_library(ModelLibrary::new(entries), Some(0));
    let colosseum = app
        .library()
        .entries()
        .iter()
        .position(|entry| entry.display == WEB_COLOSSEUM.0);
    if let (Some(row), Some(_)) = (colosseum, path) {
        app.open_entry(row);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_shot_aims_from_the_start_view_not_the_last_shot() {
        let start = OrbitCamera::default();
        let mut camera = start.clone();
        for spec in ["8x8@1,zoom=2,pitch=10", "8x8@1,zoom=2"] {
            aim(&mut camera, &start, &parse(spec).expect("a valid shot"));
            assert_eq!(camera.radius, start.radius * 2.0, "{spec}");
        }
        assert_eq!(camera.pitch_deg, start.pitch_deg, "the pitch carried over");
        aim(&mut camera, &start, &parse("8x8@1").expect("a valid shot"));
        assert_eq!(camera.radius, start.radius, "the zoom carried over");
    }
}
