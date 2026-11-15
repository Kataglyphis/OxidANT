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

**Tracing** — the C++ engine's ray-traced mode (a primary ray, the sun through a traced
shadow ray, emission) and path-traced mode (NEE to the sun, GGX and Lambert lobes, Russian
roulette, a jittered, accumulated running mean, 1-64 samples and 1-16 bounces, a white
furnace), over a CPU-built, binned-SAH BVH in two levels (repeated and moving meshes
instanced) that compute kernels traverse natively and in the browser, alpha-testing MASK
materials; the scene's materials and textures as one table and two texture arrays. Built the
first time a traced mode is picked, off the frame (the full 6 M-triangle Colosseum in about
2 s natively); the frame rasters meanwhile: [`docs/tracer.md`](docs/tracer.md).

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
today; their full versions, the remaining inspector rows and the fly camera follow.

## Running

```bash
# Native viewer (drop a .gltf/.glb on the window to load it)
cargo run -p kataglyphis_webgpu_renderer --example viewer [model.gltf]

# Headless + unit tests (GPU tests self-skip without an adapter)
cargo test -p kataglyphis_webgpu_renderer

# Translate the WGSL shaders to SPIR-V/GLSL with naga, for inspection only: the
# C++ engine compiles its SPIR-V from the shared Slang sources (shader-sharing guide)
cargo run -p kataglyphis_webgpu_renderer --example export_shaders -- out_dir

# The tracer's build of a glTF: times per stage, buffer sizes, the browser's limits
cargo run --release -p kataglyphis_webgpu_renderer --example trace_build -- model.gltf

# The editor rendered headless to PNGs (default: 1600x900 at 100 % and 150 %, 2400x1350, the 800x600 docs iframe);
# rt and pt render the ray- or path-traced mode
cargo run -p kataglyphis_webgpu_renderer --example editor_snapshot -- out_dir [1600x900@1.5,sun,about,web,pt ...]

# Browser demo (drop a self-contained .glb onto the page to load it)
cargo build -p kataglyphis_webgpu_renderer --target wasm32-unknown-unknown --release
wasm-bindgen target/wasm32-unknown-unknown/release/kataglyphis_webgpu_renderer.wasm \
  --out-dir crates/webgpu_renderer/web/pkg --target web
python -m http.server 8931 --directory crates/webgpu_renderer/web
```

Controls (also listed in the inspector's Camera tab): drag to orbit, wheel to
zoom, **F1** hides or shows the editor, **F5** reloads the shaders, **S** saves a
screenshot, **Esc** quits. While a text field has focus these keys do nothing, and
the first **Esc** only leaves the field, closes a menu or the About box. The browser
demo has F1 and Esc, which never quits there.

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
`tests/mirrored.rs` holds mirrored (negative-determinant) nodes and instances, and
double-sided back faces, to the shading of their unmirrored originals. `tests/traced_modes.rs`
holds the traced modes to the C++ goldens and the path tracer's white furnace to analytic values.
