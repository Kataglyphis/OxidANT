//! The Studio editor over a glTF, rendered headless into PNGs: `cargo run --example editor_snapshot -- [out_dir] [WxH@scale[,tab][,about][,hidden][,web][,rt|pt] ...]`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context as _;
use kataglyphis_webgpu_renderer::render::tracer::RenderMode;
use kataglyphis_webgpu_renderer::ui::{ModelSource, SceneObject};
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
/// A path-traced shot's frames, so its accumulation holds 128 samples at the default 8 a frame.
const PATH_TRACED_FRAMES: u32 = 16;

/// One PNG: the frame's size in pixels, its scale, the inspector tab, About open, the editor hidden, the browser's host.
struct Shot {
    size: [u32; 2],
    scale: f32,
    object: SceneObject,
    about: bool,
    hidden: bool,
    web: bool,
    mode: RenderMode,
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
        mode: RenderMode::Raster,
    };
    for option in parts {
        match option {
            "about" => shot.about = true,
            "hidden" => shot.hidden = true,
            "web" => shot.web = true,
            "rt" => shot.mode = RenderMode::RayTraced,
            "pt" => shot.mode = RenderMode::PathTraced,
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

/// `now_s` runs on across shots, so the frame clock never steps back.
fn render(app: &mut StudioApp, shot: &Shot, path: &Path, now_s: &mut f64) -> anyhow::Result<()> {
    app.editor_mut().select(shot.object);
    app.editor_mut().set_visible(!shot.hidden);
    if shot.about {
        app.editor_mut().show_about();
    } else {
        app.editor_mut().hide_about();
    }
    app.renderer.render_mode = shot.mode;
    if shot.mode.is_traced() {
        // The shot shows the traced mode, not raster standing in while the BVH builds.
        let built = app
            .renderer
            .tracer_mut()
            .finish_build(&app.gpu, Duration::from_secs(600));
        anyhow::ensure!(built, "the tracer did not build: {:?}", app.trace_status());
    }
    let frames = if shot.mode == RenderMode::PathTraced {
        PATH_TRACED_FRAMES
    } else {
        WARM_UP_FRAMES
    };
    let mut pixels = Vec::new();
    for _ in 0..=frames {
        let frame = OffscreenFrame {
            size_px: shot.size,
            pixels_per_point: shot.scale,
            now_s: *now_s,
            events: Vec::new(),
        };
        pixels = app.render_offscreen(&frame)?.0;
        *now_s += 1.0 / 60.0;
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

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let mut args = std::env::args().skip(1);
    let out_dir = PathBuf::from(args.next().unwrap_or_else(|| "snapshots".into()));
    let specs: Vec<String> = args.collect();
    let specs: Vec<&str> = if specs.is_empty() {
        DEFAULT_SHOTS.to_vec()
    } else {
        specs.iter().map(String::as_str).collect()
    };
    std::fs::create_dir_all(&out_dir)?;
    // Each host keeps its own clock, so its frame history never sees the other's frames.
    let mut hosts: [Option<(StudioApp, f64)>; 2] = [None, None];
    for spec in specs {
        let shot = parse(spec)?;
        let host = &mut hosts[usize::from(shot.web)];
        if host.is_none() {
            *host = Some((studio(shot.web)?, 0.0));
        }
        let (app, now_s) = host.as_mut().expect("built above");
        render(app, &shot, &out_dir.join(file_name(spec)), now_s)?;
    }
    Ok(())
}

/// Native: an sRGB surface, every shell action live; web: a plain canvas format (egui blends in gamma space), none.
fn studio(web: bool) -> anyhow::Result<StudioApp> {
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
    Ok(app)
}
