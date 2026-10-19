# The crates

One Cargo workspace. The root `Cargo.toml` is both the workspace and the root
package `oxidant`: a `cdylib`/`staticlib`/`rlib` library plus the feature-gated
`burn-demos` bin. Its consumers are other repositories, so its `[lib] name` is not
free to change.

| Path | Package | What it is |
| --- | --- | --- |
| `crates/core` | `kataglyphis_core` | Config, detection types, logging |
| `crates/telemetry` | `kataglyphis_telemetry` | CPU/GPU/RAM resource monitoring |
| `crates/inference` | `kataglyphis_inference` | ONNX backends, feature-gated: `onnx_tract`, `onnxruntime`, `onnxruntime_directml`, `onnxruntime_cuda`; `ort_runtime` picks and verifies the chain-built ONNX Runtime |
| `crates/media` | `kataglyphis_media` | GStreamer capture, feature-gated (`gstreamer`) |
| `crates/gui` | `kataglyphis_gui` | Feature-gated GUI: `gui_windows`, `gui_linux`, `gui_wgpu`, `gui_unix` |
| `crates/webgpu_renderer` | `kataglyphis_webgpu_renderer` | wgpu glTF renderer, native and wasm32/WebGPU: PBR+IBL, cascaded shadows, SSAO, bloom, skinning, animations, LOD, headless golden tests |
| `crates/observant` | `kataglyphis_observant` | The ObservANT service (cats by default): picks the camera, burns the detection boxes into the frames, streams them through `webrtcsink` and serves OmniAccelerANT's web build itself, for its Stream page ([observant.md](observant.md)). Raspberry Pi 5 runner: `scripts/linux/observant/run-producer-pi.sh` |
| `crates/esp32-protocol` | `kataglyphis_esp32_proto` | The framed USB-serial protocol OmniAccelerANT and its ESP32-S3 firmware share; `no_std`, so the firmware builds the same crate |
| `crates/esp32` | `kataglyphis_esp32` | The USB-serial bridge to the ESP32-S3 companion board: device discovery plus a threaded serial link. Desktop only: the root package takes it on Linux, Windows and macOS |
| `crates/cli` | `kataglyphis_cli` | The CLI binary, bin target `kataglyphis_cli`: `read` / `stats --path <file>` / `gui`, plus `onnx-runtime` with `onnxruntime_directml` or `onnxruntime_cuda` and `media-check` with `gui_windows`. Renamed on 2026-08-07 from `kataglyphis_rustprojecttemplate`, the root package's name until 2026-09-05 ([AGENTS.md § The bin was renamed, not the lib](../AGENTS.md#the-bin-was-renamed-not-the-lib--do-not-undo-it)) |
| `src/` | `oxidant` | The root package: the flutter_rust_bridge surface for OmniAccelerANT (`src/frb_generated.rs`, `src/api/{esp32,onnx,simple,webcam}.rs`, `src/webcam_engine.rs`), the cxx bridge BeschleunigerBallett links (`src/native_only.rs`, compiled by `build.rs` with `cxx-build` on every non-wasm target) and the `burn-demos` bin |
| `tests/` | | The CLI's integration tests ([testing.md](testing.md#where-the-suites-live)) |
| `third_party/egui-winit-0.36.2` | `egui-winit` | Vendored, with upstream emilk/egui#8516 applied, routed in by the root `Cargo.toml`'s `[patch.crates-io]` so the renderer's wasm32 demo compiles. Temporary: its `PATCHED.md` says how to remove it once egui 0.37 is out |

## Features

**Default features are empty.** GUI, ONNX, GStreamer and the burn demos only
compile with an explicit `--features` (see [running.md](running.md)), and each one
needs system libraries: [AGENTS.md § Feature combinations and their system dependencies](../AGENTS.md#feature-combinations-and-their-system-dependencies).

## Consumers

[OmniAccelerANT](https://github.com/Kataglyphis/OmniAccelerANT) and
[BeschleunigerBallett](https://github.com/Kataglyphis/BeschleunigerBallett) build this
repository as a submodule, so a rename goes into both. What each one names, file by file:
[AGENTS.md § Consumers](../AGENTS.md#consumers).

## The WebGPU renderer

`crates/webgpu_renderer` runs natively (Vulkan/DX12/Metal) and in the browser (wasm32 +
WebGPU); its features, demos and tests are in its
[README](../crates/webgpu_renderer/README.md). It shares its shaders with the C++ Vulkan
engine in BeschleunigerBallett: one Slang source compiles to the engine's SPIR-V and to
the WGSL checked in under `crates/webgpu_renderer/src/shaders/` (`histogram.wgsl` alone
is hand-written).

## Literature

The shading references behind the renderer's metallic-roughness BRDF (GGX, Smith,
Fresnel-Schlick) and its split-sum image-based lighting:

- [Real Shading in Unreal Engine 4](https://blog.selfshadow.com/publications/s2013-shading-course/karis/s2013_pbs_epic_notes_v2.pdf) (Karis)
- [Physically Based Shading at Disney](https://blog.selfshadow.com/publications/s2012-shading-course/burley/s2012_pbs_disney_brdf_notes_v3.pdf) (Burley)
- [Understanding the Masking-Shadowing Function in Microfacet-Based BRDFs](https://hal.inria.fr/hal-01024289/) (Heitz)
- [Physically Based Rendering, microfacet models](https://pbr-book.org/3ed-2018/Reflection_Models/Microfacet_Models)
- [Real-Time Rendering](https://www.realtimerendering.com/)
