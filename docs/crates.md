# The crates

One Cargo workspace. The root `Cargo.toml` is both the workspace and the root
package `oxidant` — a `cdylib`/`staticlib`/`rlib` library whose consumers are
other repositories, so its `[lib] name` is not free to change.

| Crate | Package | What it is |
| --- | --- | --- |
| `crates/core` | `kataglyphis_core` | Config, detection types, logging |
| `crates/telemetry` | `kataglyphis_telemetry` | CPU/GPU/RAM resource monitoring |
| `crates/inference` | `kataglyphis_inference` | ONNX backends, feature-gated: `onnx_tract`, `onnxruntime`, `onnxruntime_directml`, `onnxruntime_cuda` |
| `crates/media` | `kataglyphis_media` | GStreamer capture, feature-gated (`gstreamer`) |
| `crates/gui` | `kataglyphis_gui` | Feature-gated GUI: `gui_windows`, `gui_linux`, `gui_wgpu`, `gui_unix` |
| `crates/webgpu_renderer` | `kataglyphis_webgpu_renderer` | wgpu glTF renderer, native and wasm32/WebGPU: PBR+IBL, cascaded shadows, SSAO, bloom, skinning, animations, LOD, headless golden tests |
| `crates/observant` | `kataglyphis_observant` | ObservANT service (cats by default): picks the camera, draws the detector's cat boxes, WebRTC, and the web server for OmniAccelerANT's Stream page ([observant.md](observant.md)) |
| `crates/esp32-protocol` | `kataglyphis_esp32_proto` | The framed USB-serial protocol OmniAccelerANT and its ESP32-S3 firmware share; no `std`, so the firmware builds the same crate |
| `crates/esp32` | `kataglyphis_esp32` | The USB-serial bridge to the ESP32-S3 companion board; the root package takes it on Linux, Windows and macOS only |
| `crates/cli` | `kataglyphis_cli` | The CLI binary (`read` / `stats` / `gui`, plus `onnx-runtime` with an `onnxruntime_*` feature and `media-check` with `gui_windows`) |
| `src/` | `oxidant` | The root package: the flutter_rust_bridge surface for OmniAccelerANT, plus the feature-gated `burn-demos` bin |

## Features

**Default features are empty.** GUI, ONNX, GStreamer and the burn demos only
compile with an explicit `--features` (see [running.md](running.md)), and each one
needs system libraries — [AGENTS.md § Feature combinations and their system dependencies](../AGENTS.md#feature-combinations-and-their-system-dependencies)
has the feature/dependency table.

## Consumers

Two repositories build this one as a submodule, so a rename has to be carried into
both: [OmniAccelerANT](https://github.com/Kataglyphis/OmniAccelerANT) (the root
package through flutter_rust_bridge, and `crates/observant`) and
[BeschleunigerBallett](https://github.com/Kataglyphis/BeschleunigerBallett) (the
root package as a staticlib through Corrosion, and `crates/webgpu_renderer` through
cargo for its wasm demo and its tests). What each one names, file by file:
[AGENTS.md § Consumers](../AGENTS.md#consumers).

## The WebGPU renderer

The largest crate is **`crates/webgpu_renderer`**, a WebGPU (wgpu) glTF renderer
that runs natively (Vulkan/DX12/Metal) and in the browser (wasm32 + WebGPU): PBR
with IBL, cascaded shadow maps, SSAO, bloom, GPU skinning, animations, LOD, hot
shader reload, and headless golden tests. See
[`crates/webgpu_renderer/README.md`](../crates/webgpu_renderer/README.md) for the
demos, and for how it shares its shaders with the C++ Vulkan engine in
BeschleunigerBallett: one Slang source compiles to the engine's SPIR-V and to the
WGSL checked in under `crates/webgpu_renderer/src/shaders/` (`histogram.wgsl` alone
is hand-written).

## Literature

Some very helpful literature, tutorials, etc. The list came with the project
template and sat commented out in the README until 2026-10-19.

CMake/C++

- [Cpp best practices](https://github.com/cpp-best-practices/cppbestpractices)

Vulkan

- [Udemy course by Ben Cook](https://www.udemy.com/share/102M903@JMHgpMsdMW336k2s5Ftz9FMx769wYAEQ7p6GMAPBsFuVUbWRgq7k2uY6qBCG6UWNPQ==/)
- [Vulkan Tutorial](https://vulkan-tutorial.com/)
- [Vulkan Raytracing Tutorial](https://developer.nvidia.com/rtx/raytracing/vkray)
- [Vulkan Tutorial; especially chapter about integrating imgui](https://frguthmann.github.io/posts/vulkan_imgui/)
- [NVidia Raytracing tutorial with Vulkan](https://nvpro-samples.github.io/vk_raytracing_tutorial_KHR/)
- [Blog from Sascha Willems](https://www.saschawillems.de/)

Physically Based Shading

- [Advanced Global Illumination by Dutre, Bala, Bekaert](https://www.oreilly.com/library/view/advanced-global-illumination/9781439864951/)
- [The Bible: PBR book](https://pbr-book.org/3ed-2018/Reflection_Models/Microfacet_Models)
- [Real shading in Unreal engine 4](https://blog.selfshadow.com/publications/s2013-shading-course/karis/s2013_pbs_epic_notes_v2.pdf)
- [Physically Based Shading at Disney](https://blog.selfshadow.com/publications/s2012-shading-course/burley/s2012_pbs_disney_brdf_notes_v3.pdf)
- [RealTimeRendering](https://www.realtimerendering.com/)
- [Understanding the Masking-Shadowing Function in Microfacet-Based BRDFs](https://hal.inria.fr/hal-01024289/)
- [Sampling the GGX Distribution of Visible Normals](https://pdfs.semanticscholar.org/63bc/928467d760605cdbf77a25bb7c3ad957e40e.pdf)

Path tracing

- [NVIDIA Path tracing Tutorial](https://github.com/nvpro-samples/vk_mini_path_tracer/blob/main/vk_mini_path_tracer/main.cpp)
