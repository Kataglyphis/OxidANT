# kataglyphis_webgpu_renderer

A WebGPU (wgpu) renderer with glTF loading, in Rust. The same code runs
natively on Vulkan/DX12/Metal and in the browser on the WebGPU API; companion
to the C++ Vulkan engine in
[BeschleunigerBallett](https://github.com/Kataglyphis/BeschleunigerBallett).
Its design documents live beside the code, in [`docs/`](docs):
the [roadmap](docs/webgpu-renderer-roadmap.md), the [bounds
invariant](docs/renderer-bounds-invariant.md) and the [original glTF plan](docs/webgpu-gltf-rust-plan.md).
They come from BeschleunigerBallett's `docs/` (decision D6: this repository owns the
WebGPU renderer's code **and** its documentation). Their paths are relative to this
repository (crate `crates/webgpu_renderer`) unless they name another one. The pages that describe both
renderers stay with the C++ engine: the [sRGB audit](https://github.com/Kataglyphis/BeschleunigerBallett/blob/develop/docs/source/webgpu-srgb-audit.md)
and the [shader-sharing guide](https://github.com/Kataglyphis/BeschleunigerBallett/blob/develop/docs/source/shader-sharing.md).

This repository owns the WebGPU Slang inputs and shared math under `shaders/`.
Regeneration and the check for drift from committed WGSL are described in
[`docs/shaders.md`](../../docs/shaders.md).

## Features

**Assets** — glTF 2.0 and GLB: meshes (triangles, strips and fans), node
hierarchy, samplers + wrap modes, `KHR_texture_transform`, tangents (loaded,
Lengyel-generated, or MikkTSpace via an opt-in), `COLOR_0` vertex colours, a
second UV set, skins, morph targets, animations, punctual lights, cameras. KTX2
textures with BC1/3/5/7 passthrough. Wavefront OBJ through a built-in OBJ→glTF
converter (`asset::obj_to_gltf`, the `obj2gltf` example).

**Shading** — metallic-roughness PBR (GGX + Smith + Fresnel-Schlick) with
base color / metallic-roughness / normal / emissive / occlusion maps,
`KHR_materials_unlit`, alpha OPAQUE/MASK/BLEND, double-sided materials,
CPU-generated mip chains.

**Lighting** — directional sun with 3-cascade shadow maps (3×3 PCF) and
per-pixel alpha-tested shadows for cut-out (MASK) materials, up to 256
`KHR_lights_punctual` point/spot lights binned per 16×16-pixel screen tile,
procedural sky with an analytic sun, and image-based lighting: analytic
(hemisphere irradiance + sky reflections via the split-sum approximation), or
from a Radiance `.hdr` environment map (irradiance and prefiltered specular
cubemaps + a BRDF LUT).

**Post** — 4× MSAA, HDR `Rgba16Float` target, bloom (bright-pass + separable
Gaussian), SSAO (depth reconstruction), manual or histogram auto-exposure, ACES
tonemapping.

**Runtime** — GPU skinning, morph-target and TRS animation playback, GPU
instancing, frustum plus occlusion culling (hardware queries, or an opt-in
compute pass), LOD simplification, a validated render graph, hot shader reload
and screenshot capture.

**Editor** — BeschleunigerBallett's docked C++ editor ("Studio Pro") in egui
(`ui::Editor`): menu bar, status bar, a scene dock (outliner and model library),
an inspector with the Model, Sun, Camera, Sky and Renderer tabs, a profiler, the
viewport chip and an About modal, in the C++ palette, metrics and fonts (Roboto
Medium and Cousine, embedded from `assets/fonts/` with their licences). The docks
take the C++ sizes, caps and drop order, and custom splitters inside the scene's
edge resize them. The native viewer and the browser demo run one host,
`app::StudioApp`; its camera centres on the rect the docks leave free
(`SceneViewport`, the C++ `makeViewportRemap`) while every pass stays full-frame.
The library and the profiler show their C++ layout over what the renderer has
today; their full versions and the remaining inspector rows follow.

**Camera** — the C++ engine's fly camera (`scene::fly`) and the orbit controller
share one `OrbitCamera`: flying moves eye and target together and turns about the
eye, orbiting turns about the target and the wheel changes the distance between
them, so an orbit after a flight circles the point the camera looked at. The fly
speed starts at a quarter of the scene's diagonal per second (the C++ 10 units/s
in a 40-unit Sponza); the Camera tab sets it with the field of view and the clip
planes, and *Frame the scene* returns to the home view with the speed refitted.

## Running

```bash
# Native viewer (drop a .gltf/.glb on the window to load it)
cargo run -p kataglyphis_webgpu_renderer --example viewer [model.gltf]

# Headless + unit tests (GPU tests self-skip without an adapter)
cargo test -p kataglyphis_webgpu_renderer

# Translate the WGSL shaders to SPIR-V/GLSL with naga, for inspection only: the
# C++ engine compiles its SPIR-V from the shared Slang sources (shader-sharing guide)
cargo run -p kataglyphis_webgpu_renderer --example export_shaders -- out_dir

# The editor rendered headless to PNGs (default: 1600x900 at 100 % and 150 %, 2400x1350, the 800x600 docs iframe)
cargo run -p kataglyphis_webgpu_renderer --example editor_snapshot -- out_dir [1600x900@1.5,sun,about,web ...]

# Browser demo (drop a self-contained .glb onto the page to load it)
cargo build -p kataglyphis_webgpu_renderer --target wasm32-unknown-unknown --release
wasm-bindgen target/wasm32-unknown-unknown/release/kataglyphis_webgpu_renderer.wasm \
  --out-dir crates/webgpu_renderer/web/pkg --target web
python -m http.server 8931 --directory crates/webgpu_renderer/web
```

Controls (also listed in the inspector's Camera tab): **W A S D** fly, **Q E**
turn, the held **right mouse** button looks around (0.25° per pixel, the pointer
locked meanwhile), drag to orbit, wheel to zoom, **F1** hides or shows the editor,
**F5** reloads the shaders, **F12** saves a screenshot, **Esc** quits. While a text
field has focus these keys do nothing; a key released there still stops the camera,
and losing the window's focus releases everything. The first **Esc** only leaves
the field, closes a menu or the About box. The browser demo has the camera keys, F1
and Esc, which never quits there; Tab, F5 and Ctrl +/-/0 stay the browser's unless
an egui field has focus.

## Testing approach

GPU tests render headlessly and assert *structural* pixel properties
(colour dominance, coverage ratios, energy deltas) rather than exact
images, so they survive driver differences. Without an adapter they skip and
still report as passed, unless `KATAGLYPHIS_REQUIRE_GPU=1` turns that into a
failure. The Linux x64 and arm64 lanes and both Windows lanes set it and render
on a software adapter: lavapipe on Linux, WARP on Windows (x64 on its runner
host, arm64 on `windows-11-arm`). The `ADAPTER:` line and the BC-upload and
GPU-timing tests:
[`docs/testing.md` § The GPU tests must render](../../docs/testing.md#the-gpu-tests-must-render).
`a_non_uniform_instance_scale_shades_like_the_same_node_scale` is `#[ignore]`d:
a known shading bug ([BACKLOG.md](../../BACKLOG.md)) fails it on every adapter.
