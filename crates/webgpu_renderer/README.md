# kataglyphis_webgpu_renderer

A WebGPU (wgpu) renderer with glTF loading, in Rust. The same code runs
natively on Vulkan/DX12/Metal and in the browser on the WebGPU API; companion
to the C++ Vulkan engine in
[BeschleunigerBallett](https://github.com/Kataglyphis/BeschleunigerBallett).
Its design documents live beside the code, in [`docs/`](docs):
the [roadmap](docs/webgpu-renderer-roadmap.md), the [bounds
invariant](docs/renderer-bounds-invariant.md), the [tracer core](docs/tracer.md) and the
[original glTF plan](docs/webgpu-gltf-rust-plan.md).
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
second UV set, skins, morph targets, animations, punctual lights, cameras,
`KHR_mesh_quantization` (gltfpack's integer attributes). KTX2
textures with BC1/3/5/7 passthrough, and Basis ETC1S/UASTC KTX2 (also through
`KHR_texture_basisu`) transcoded to BC7, ETC2 or ASTC by adapter. Wavefront OBJ through a built-in OBJ→glTF
converter (`asset::obj_to_gltf`, the `obj2gltf` example).

**Shading** — metallic-roughness PBR (GGX + Smith + Fresnel-Schlick, with
multiple-scattering energy compensation from a fitted spline, so a white surface
returns all of a uniform sky at any roughness) with base color / metallic-roughness / normal / emissive / occlusion maps,
`KHR_materials_unlit`, alpha OPAQUE/MASK/BLEND, double-sided materials,
CPU-generated mip chains. Two raster paths, switched at run time like the C++
engine's (`ForwardRenderer::raster_path`): forward, and deferred, whose
single-sample G-buffer (normal + metallic, albedo + occlusion, roughness +
emissive: 24 of WebGPU's 32 colour bytes per sample) one full-screen pass lights
with the forward pass's own lighting module (`shaders/forward/forward_lighting.slang`);
the sky and blended materials draw forward after it.

**Lighting** — directional sun with the C++ engine's cascaded shadow maps
(`render::shadow_settings`: 1 to 3 cascades fitted to slices of the view frustum by
the practical split scheme and texel-stabilised, 512 to 4096 texels, PCF radius 0 to 5,
strength, distance and split lambda, its bias formula, casters culled per cascade) and
per-pixel alpha-tested shadows for cut-out (MASK) materials, up to 256
`KHR_lights_punctual` point/spot lights binned per 16×16-pixel screen tile, the first
four with the C++ engine's 512 px depth-cube shadows (six layers of a 2D array each),
the C++ cube-map skybox (Poly Haven's CC0 `kloofendal_43d_clear_puresky`, shown untonemapped as the C++ draws it):
natively from BeschleunigerBallett's `Resources/Textures/Skybox`, in the browser from a
`kloofendal_43d_clear_puresky/` folder beside the page (`?scene-base=` too), which BeschleunigerBallett's docs build
stages with the faces' CC0 notice. Without the faces a procedural sky with an analytic sun stands in, and
image-based lighting: analytic
(hemisphere irradiance + sky reflections via the split-sum approximation), or
from a Radiance `.hdr` environment map (irradiance and prefiltered specular
cubemaps + a BRDF LUT). `demo_view().skybox` says which sky the browser draws.

**Clouds** — the C++ engine's volumetric clouds, sharing its mathematics
([`shaders/common/volumetric_clouds.slang`](../../shaders/common/volumetric_clouds.slang)):
a 128³ noise volume generated once, then per frame a ray march through an
axis-aligned box (Offset, Scale × 10) with a light march toward the
sun, Henyey-Greenstein phase in three scattering octaves and a powder term, lit
besides by the drawn sky's mean radiance per hemisphere (`CloudAmbient`); a path
longer than the box's thinnest extent takes up to four times the slider's steps. It stops at the single-sample scene
depth either raster path leaves, or in a traced frame at the primary hit's depth the
kernel writes, and the tonemap composites it in linear HDR, the same over sky and geometry. Off by default (`ForwardRenderer::clouds`), with the C++
ranges in the Sky tab.

**Post** — 4× MSAA on the forward path; deferred renders one sample and FXAA
smooths its edges after the tonemap (`render::fxaa`, on every frame by default since
2026-11-21, as in BeschleunigerBallett, or on deferred frames only; sub-pixel detail still differs, `tests/deferred.rs` and
`tests/fxaa.rs` measure it), HDR `Rgba16Float` target, bloom (bright-pass + separable
Gaussian), SSAO (depth reconstruction), manual or histogram auto-exposure, ACES
tonemapping.

**Tracing** — the C++ engine's ray-traced mode (a primary ray, the sun and every punctual
light each through a traced shadow ray, emission) and path-traced mode (NEE to the sun and
one punctual light, GGX and Lambert lobes, Russian
roulette, a jittered, accumulated running mean, 1-64 samples and 1-16 bounces, a white
furnace), over a CPU-built, binned-SAH BVH in two levels (repeated and moving meshes
instanced) that compute kernels traverse natively and in the browser, alpha-testing MASK
materials; the scene's materials and textures as one table and two texture arrays. Built the
first time a traced mode is picked, off the frame (the full 6 M-triangle Colosseum in about
2 s natively); the frame rasters meanwhile. A ray-traced miss shows the sky pass (the cube
map, or the navy with it off), the clouds march to the traced depth, and the model transform,
animated nodes and `set_instances` move the traced instances: [`docs/tracer.md`](docs/tracer.md).

**Runtime** — GPU skinning, morph-target and TRS animation playback, GPU
instancing, a model transform over the whole scene (the C++ Model tab's), frustum
culling that can be switched off plus occlusion culling (hardware queries, or an
opt-in compute pass), LOD simplification, a render graph that orders the passes and aliases their
transient targets, hot shader reload
and screenshot capture.

**Editor** — BeschleunigerBallett's docked C++ editor ("Studio Pro") in egui
(`ui::Editor`): menu bar, status bar, a scene dock (outliner and model library),
an inspector with the Model, Sun, Camera, Sky and Renderer tabs, a profiler, the
viewport chip and an About modal, in the C++ palette, metrics and fonts (Roboto
Medium and Cousine, plus two small subsets of egui's emoji faces for egui's own debug
windows, embedded from `assets/fonts/` with their licences). The docks
take the C++ sizes, caps and drop order, and custom splitters inside the scene's
edge resize them. The native viewer and the browser demo run one host,
`app::StudioApp`; its camera centres on the rect the docks leave free
(`SceneViewport`, the C++ `makeViewportRemap`) while every pass stays full-frame.
The library lists the scenes on disk (BeschleunigerBallett's `Resources/Models`,
`resources/scenes`, the command line file's folder; `.gltf`, `.glb` and `.obj`), or in
the browser the showcase scenes and the Colosseum, grouped by folder as in the C++
editor: a click picks, a double-click, Enter or Load loads off the frame, and the
scene on screen stays until the new one is ready. A scene starts at the camera its
glTF left active, else its first, else fitted to its bounds, and its
`asset.copyright` shows in the status bar and About. In the browser the profiler
starts hidden and a narrow canvas drops the inspector first, so the docs iframe
keeps the library. The inspector's C++ rows are live: the Sun's SHADOWS section, the
Model tab's transform, the skybox switch and the Sky tab's CLOUDS section (each also an
eye in the outliner), frustum culling (also in the Render menu and the profiler's
Visibility tab, with the C++ counts) and the raster path (also the Render menu's radios,
named by the viewport chip while the frame rasters); a scene's start view sets the shadow
distance to twice its distance to the target, never under the C++ 60 m. The menu bar's
switch, the Render menu and the Renderer tab pick raster, ray or path tracing, and the
Renderer tab's PATH TRACING rows set its samples, bounces and white furnace.

**Performance and adapters** — the Renderer tab's QUALITY presets (Low, Medium, High,
Ultra; Custom after an edit) set the raster path, FXAA, SSAO, bloom, the shadow maps and
the render scale (`render::quality`). The render scale (0.25 to 1) sizes the scene
targets and the tonemap upsamples to the output; Auto steers it toward a frame rate
(`render::render_scale`), and the viewport chip names the internal size. The browser
starts at Medium on an integrated or unknown GPU and High on a discrete one; natively
the start is High. The viewer picks discrete over integrated over a CPU rasterizer,
takes `--adapter <name>` or `KATAGLYPHIS_ADAPTER`, and the Renderer tab's ADAPTER list
restarts it on another (`adapter`). The status bar names a browser adapter from its
vendor and architecture and hints where Windows chooses Chrome's GPU. The DISPLAY
section's VSync switch, also in the Render menu, is on (Fifo) by default; off presents
Mailbox, else Immediate (`context::present_mode`). A browser always presents with the display.

**Camera** — the C++ engine's fly camera (`scene::fly`) and the orbit controller
share one `OrbitCamera`: flying moves eye and target together and turns about the
eye, orbiting turns about the target and the wheel changes the distance between
them, so an orbit after a flight circles the point the camera looked at. The fly
speed is a quarter of the scene's diagonal per second (the C++ 10 units/s in a
40-unit Sponza), at most half the start view's distance to its target (33 m/s
from the Colosseum's `Kamera_Arena`); the Camera tab sets it with the field of
view and the clip planes, and *Frame the scene* returns to the start view. The
wheel and a pinch keep the radius between twice the near plane and half the far,
which the start view fits to the scene, so an 8 km scene zooms from where it starts.

## Running

```bash
# Native viewer: the fetched Colosseum, or a .gltf, .glb or .obj (drop one on the window to load it)
scripts/linux/fetch-models.sh --scenes
cargo run -p kataglyphis_webgpu_renderer --example viewer [model] [--adapter <name>]

# Headless + unit tests (GPU tests self-skip without an adapter)
cargo test -p kataglyphis_webgpu_renderer

# Translate the WGSL shaders to SPIR-V/GLSL with naga, for inspection only: the
# C++ engine compiles its SPIR-V from the shared Slang sources (shader-sharing guide)
cargo run -p kataglyphis_webgpu_renderer --example export_shaders -- out_dir

# The tracer's build of a glTF: times per stage, buffer sizes, the browser's limits
cargo run --release -p kataglyphis_webgpu_renderer --example trace_build -- model.gltf

# The editor rendered headless to PNGs (default: 1600x900 at 100 % and 150 %, 2400x1350, the 800x600 docs iframe);
# rt and pt render the ray- or path-traced mode, spp= and frames= set the path tracer's samples and frames
cargo run -p kataglyphis_webgpu_renderer --example editor_snapshot -- out_dir \
  [--model=scene.gltf] [--web-model=Colosseum-web.glb] [1600x900@1.5,sun,about,web,deferred,cloudy,pt,spp=2,pitch=0,zoom=4 ...]

# Browser demo (drop a self-contained .glb onto the page; the Colosseum row fetches Colosseum-web.glb beside it)
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
`tests/mirrored.rs` holds mirrored (negative-determinant) nodes, instances and skins, and
double-sided back faces, to the shading of their unmirrored originals. `tests/traced_modes.rs`
holds the traced modes to the C++ goldens and the path tracer's white furnace to analytic values;
`tests/feature_matrix.rs` runs the shadow settings, skybox, clouds, model transform and culling
on both raster paths and in both traced modes.
