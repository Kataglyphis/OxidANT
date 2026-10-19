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

The shading references behind the renderer's metallic-roughness BRDF (GGX, Smith,
Fresnel-Schlick) and its split-sum image-based lighting:

- [Real Shading in Unreal Engine 4](https://blog.selfshadow.com/publications/s2013-shading-course/karis/s2013_pbs_epic_notes_v2.pdf) (Karis)
- [Physically Based Shading at Disney](https://blog.selfshadow.com/publications/s2012-shading-course/burley/s2012_pbs_disney_brdf_notes_v3.pdf) (Burley)
- [Understanding the Masking-Shadowing Function in Microfacet-Based BRDFs](https://hal.inria.fr/hal-01024289/) (Heitz)
- [Physically Based Rendering, microfacet models](https://pbr-book.org/3ed-2018/Reflection_Models/Microfacet_Models)
- [Real-Time Rendering](https://www.realtimerendering.com/)
