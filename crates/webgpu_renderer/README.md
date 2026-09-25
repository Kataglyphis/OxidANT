# kataglyphis_webgpu_renderer

A WebGPU (wgpu) renderer with glTF loading, written in Rust. The same code
runs natively on Vulkan/DX12/Metal and in the browser on the WebGPU API.

Companion to the C++ Vulkan engine in
[BeschleunigerBallett](https://github.com/Kataglyphis/BeschleunigerBallett).
The renderer's own design documents live beside the code, in [`docs/`](docs):
the [roadmap](docs/webgpu-renderer-roadmap.md), the [bounds
invariant](docs/renderer-bounds-invariant.md) and the [original glTF plan](docs/webgpu-gltf-rust-plan.md). The pages that describe both
renderers stay with the C++ engine: the [sRGB audit](https://github.com/Kataglyphis/BeschleunigerBallett/blob/develop/docs/webgpu-srgb-audit.md)
and the [shader-sharing guide](https://github.com/Kataglyphis/BeschleunigerBallett/blob/develop/docs/shader-sharing.md).

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
compute pass), LOD simplification, a validated render graph, hot shader reload,
an egui overlay, and screenshot capture.

## Running

```bash
# Native viewer (drop a .gltf/.glb on the window to load it)
cargo run -p kataglyphis_webgpu_renderer --example viewer [model.gltf]

# Headless + unit tests (GPU tests self-skip without an adapter)
cargo test -p kataglyphis_webgpu_renderer

# Translate the WGSL shaders to SPIR-V/GLSL with naga, for inspection only: the
# C++ engine compiles its SPIR-V from the shared Slang sources (shader-sharing guide)
cargo run -p kataglyphis_webgpu_renderer --example export_shaders -- out_dir

# Browser demo (drop a self-contained .glb onto the page to load it)
cargo build -p kataglyphis_webgpu_renderer --target wasm32-unknown-unknown --release
wasm-bindgen target/wasm32-unknown-unknown/release/kataglyphis_webgpu_renderer.wasm \
  --out-dir crates/webgpu_renderer/web/pkg --target web
python -m http.server 8931 --directory crates/webgpu_renderer/web
```

Viewer controls: drag to orbit, wheel to zoom, **S** screenshot,
**R** reload shaders, **Esc** quit.

## Testing approach

GPU tests render headlessly and assert *structural* pixel properties
(colour dominance, coverage ratios, energy deltas) rather than exact
images, so they survive driver differences. They skip themselves when no
adapter is present and still report as passed. The Linux CI lanes, which run
them, have no adapter, so a green CI run has not drawn them (the Windows x64
lane runs only the lib tests, on its runner host). Set `KATAGLYPHIS_REQUIRE_GPU=1`
to turn a missing adapter into a failure.
