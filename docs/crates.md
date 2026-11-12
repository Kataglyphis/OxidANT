# The crates

One Cargo workspace. The root `Cargo.toml` is both the workspace and the root
package `oxidant`, a `cdylib`/`staticlib`/`rlib` library. Its consumers are other
repositories, so its `[lib] name` is not free to change.
The Burn demos use Flex through a [patched `burn-dispatch`](https://github.com/Kataglyphis/OxidANT/blob/develop/third_party/burn-dispatch-0.22.1/PATCHED.md)
that excludes the unselected legacy Torch backend from the lockfile.

| Path | Package | What it is |
| --- | --- | --- |
| `crates/core` | `kataglyphis_core` | Config, detection types, the COCO names (the synced `coco-classes.txt`), the chain-marker search, logging |
| `crates/telemetry` | `kataglyphis_telemetry` | CPU/GPU/RAM resource monitoring |
| `crates/inference` | `kataglyphis_inference` | The detector: the chain-built ONNX Runtime first (Linux, Windows; CUDA and, on Windows, DirectML registered), tract when ORT fails and on Android; `ort_runtime` picks and verifies the chain-built ONNX Runtime, `ort_plugin` registers AMD's MIGraphX plugin module by path (Linux) |
| `crates/text` | `kataglyphis_text` | Language models: the chain's ONNX Runtime GenAI in process (Linux, Windows), loaded at run time and verified like ORT, or an OpenAI-compatible endpoint by URL or `backends.json` name ([text.md](text.md)) |
| `crates/media` | `kataglyphis_media` | GStreamer capture, the one frame sink, and the GStreamer setup that makes a package load its own plugins only |
| `crates/gui` | `kataglyphis_gui` | The wgpu camera GUI with detector overlay and the GTK4 camera GUI on Linux (`gui-gtk`); both build wherever supported by the target |
| `crates/burn_demos` | `kataglyphis_burn_demos` | The `burn_demos` binary: Burn versions of PyTorch tasks, and the detector driven through ORT ([running.md](running.md#burn--pytorch-replacement-demos)). Its own crate, so neither consumer links Burn |
| `crates/webgpu_renderer` | `kataglyphis_webgpu_renderer` | wgpu glTF renderer, native and wasm32/WebGPU: PBR+IBL, cascaded shadows, SSAO, bloom, skinning, animations, LOD, headless golden tests |
| `crates/observant` | `kataglyphis_observant` | The ObservANT service (cats by default): picks the camera, burns the detection boxes into the frames, streams them through `webrtcsink` and serves OmniAccelerANT's web build itself, for its Stream page ([observant.md](observant.md)). Raspberry Pi 5 runner: `scripts/linux/observant/run-producer-pi.sh` |
| `crates/esp32-protocol` | `kataglyphis_esp32_proto` | The framed USB-serial protocol OmniAccelerANT and its ESP32-S3 firmware share; `no_std`, so the firmware builds the same crate |
| `crates/esp32` | `kataglyphis_esp32` | The USB-serial bridge to the ESP32-S3 companion board: device discovery plus a threaded serial link. Desktop only: the root package takes it on Linux, Windows and macOS |
| `crates/cli` | `kataglyphis_cli` | The CLI binary, bin target `kataglyphis_cli`: `read` / `stats --path <file>` / `gui` / `media-check`, `chat` / `embed` ([text.md](text.md#the-cli)), and `onnx-runtime` on Linux and Windows. Renamed on 2026-08-07 from `kataglyphis_rustprojecttemplate`, the root package's name until 2026-09-05 ([AGENTS.md § The bin was renamed, not the lib](packaging.md#the-bin-was-renamed-not-the-lib--do-not-undo-it)) |
| `crates/cxx_bridge` | `kataglyphis_cxx_bridge` | The cxx bridge BeschleunigerBallett's C++ engine links as a staticlib through Corrosion, which generates its C++ half. Its own crate, so the engine links neither GStreamer nor ORT |
| `src/` | `oxidant` | The root package: the flutter_rust_bridge surface for OmniAccelerANT (`src/frb_generated.rs`, `src/api/{esp32,onnx,simple,text,webcam}.rs`, `src/webcam_engine.rs`, `src/session_slot.rs`) |
| `tests/` | | The CLI's integration tests ([testing.md](testing.md#where-the-suites-live)) |
| `third_party/egui-winit-0.36.2` | `egui-winit` | Vendored, with upstream emilk/egui#8516 applied, routed in by the root `Cargo.toml`'s `[patch.crates-io]` so the renderer's wasm32 demo compiles. Temporary: its `PATCHED.md` says how to remove it once egui 0.37 is out |
| `third_party/tract-linalg-0.23.8` | `tract-linalg` | Vendored, with one `build.rs` line changed so the x86_64 MSVC build works with clang-cl as `CC`, routed in the same way. Temporary: its `PATCHED.md` holds the upstream report and how to remove it |

## No features: the target decides

**No crate has a Cargo feature** (owner, 2026-10-20). Everything is built where the target
can run it, chosen by `cfg(target_os)` and the root's `cfg(desktop)` (Linux, Windows, macOS):

| Target | Built | What it carries |
| --- | --- | --- |
| Linux (x64, arm64, riscv64), Windows (x64, arm64) | The whole workspace | tract and the chain ORT (CUDA registered, DirectML on Windows), GStreamer capture, the webcam engine, the wgpu GUI, the ESP32 bridge, the burn demos; Linux also builds GTK4 |
| `aarch64-linux-android` (Cargokit) | `oxidant` and what it depends on | The detector on tract; the webcam and ESP32 calls answer that the target has neither |
| `wasm32-unknown-unknown` (the web build) | `oxidant` and what it depends on | No detector: tract builds there, but the model is loaded from a file and the web has no file system, so it would add 15 MB of wasm that cannot run. The detection, webcam and ESP32 calls answer that |

The system libraries behind that: [AGENTS.md § Targets and their system dependencies](../AGENTS.md#targets-and-their-system-dependencies).

## Consumers

[OmniAccelerANT](https://github.com/Kataglyphis/OmniAccelerANT) and
[BeschleunigerBallett](https://github.com/Kataglyphis/BeschleunigerBallett) build this
repository as a submodule, so a rename goes into both. What each one names, file by file:
[AGENTS.md § Consumers](../AGENTS.md#consumers).

## The WebGPU renderer

`crates/webgpu_renderer` runs natively (Vulkan/DX12/Metal) and in the browser (wasm32 +
WebGPU); its features, demos and tests are in its
[README](../crates/webgpu_renderer/README.md). OxidANT owns the WebGPU Slang entry points
and shared mathematics under `shaders/`; BeschleunigerBallett consumes the mathematics
from its recorded OxidANT pin and owns its Vulkan-specific stages. Fresh output is
checked against committed WGSL; `histogram.wgsl` stays hand-written. See
[shaders.md](shaders.md) for regeneration and ownership.

## Literature

The shading references behind the renderer's metallic-roughness BRDF (GGX, Smith,
Fresnel-Schlick) and its split-sum image-based lighting:

- [Real Shading in Unreal Engine 4](https://blog.selfshadow.com/publications/s2013-shading-course/karis/s2013_pbs_epic_notes_v2.pdf) (Karis)
- [Physically Based Shading at Disney](https://blog.selfshadow.com/publications/s2012-shading-course/burley/s2012_pbs_disney_brdf_notes_v3.pdf) (Burley)
- [Understanding the Masking-Shadowing Function in Microfacet-Based BRDFs](https://hal.inria.fr/hal-01024289/) (Heitz)
- [Physically Based Rendering, microfacet models](https://pbr-book.org/3ed-2018/Reflection_Models/Microfacet_Models)
- [Real-Time Rendering](https://www.realtimerendering.com/)

## Where the renderer's design documents live

**They moved on 2026-09-15** (decision D6): this repo owns the renderer, code and
documentation. Three pages left `BeschleunigerBallett/docs/` for
`crates/webgpu_renderer/docs/`, and BeschleunigerBallett kept a pointer file at each old
path, not a copy. Four pages describe both renderers and stayed there.

| Document | Owner |
| --- | --- |
| `renderer-bounds-invariant.md` | **here**, `crates/webgpu_renderer/docs/` |
| `webgpu-renderer-roadmap.md` | **here**, `crates/webgpu_renderer/docs/` |
| `webgpu-gltf-rust-plan.md` | **here**, `crates/webgpu_renderer/docs/` |
| `gpu-golden-testing.md` | stays in BeschleunigerBallett |
| `model-loading.md` | stays in BeschleunigerBallett |
| `shader-sharing.md` | stays in BeschleunigerBallett |
| `webgpu-srgb-audit.md` | stays in BeschleunigerBallett |

**The two halves are referenced differently, on purpose.** The three that live here
are referenced by relative path: `docs/<name>.md` from `crates/webgpu_renderer/README.md`,
and relative to the file in code, `../docs/…` in `src/lib.rs` and `../../docs/…` twice
in `src/render/bounds.rs` (the docs cross-reference gate resolves a bare `docs/…` in code
from the repo root). Those paths resolve in every checkout, which is what the move
bought: this repo is built standalone, from OmniAccelerANT and from BeschleunigerBallett
(see [Consumers](../AGENTS.md#consumers)), and before the move a relative `docs/` prefix meant *the
superproject's* root and pointed at nothing in two of the three. The four that stayed
are absolute URLs
(`https://github.com/Kataglyphis/BeschleunigerBallett/blob/develop/docs/<name>.md`) for
the same reason, and must stay absolute. `bounds.rs` calls
`renderer-bounds-invariant.md` the checklist for not repeating eight identical bugs, so
a dead link there costs more than tidiness.

## Large tracked files

One large file is tracked deliberately, not in git-lfs; `large-files.allow` freezes it
for the hub's `large-files` gate. **Decision D4, "the history is not being rewritten", is
superseded** (owner, 2026-10-17): the family's history rewrite took this pack from 63 to
12 MiB. What it removed: [the hub's `data-repositories.md`](../third_party/ANTfrastructure/docs/data-repositories.md#what-moved-and-why);
a checkout from before follows with
[§ *After a history rewrite*](../third_party/ANTfrastructure/docs/adopting-in-a-new-project.md#after-a-history-rewrite).

| File | Size | Why it is tracked |
| --- | --- | --- |
| `images/Rust.webp` | ~2 MiB | The README hero image, rendered by GitHub; the 7.5 MiB GIF it replaced (2026-10-17) left the history in the rewrite. |

**Models are data, not code, and are never tracked** (owner, 2026-10-14). The AGPL-3.0
`yolov10m.onnx` (59 MiB) left the tree then, and the history in the 2026-10-17 rewrite.
Its successor is DEIM-D-FINE-N (Apache-2.0, 15 MiB), built from its pinned upstream and
released by [ModelANT](https://github.com/Kataglyphis/ModelANT).
`resources/models/models.lock` names each file with its URL and SHA-256, and
`scripts/linux/fetch-models.sh` (Windows: `scripts/windows/Get-Models.ps1`, the *Fetch
Models* build step) puts them beside it, its license and notice included. Both only run the
hub's [`fetch-data-lock.py`](../third_party/ANTfrastructure/docs/data-repositories.md#the-hubs-fetcher)
on that lock (2026-10-19), so Windows needs `python` on `PATH`, as both Windows images have
it. A checked copy waits in `KATAGLYPHIS_MODEL_CACHE`, else `~/.cache/kataglyphis-models`
(Windows: `%LOCALAPPDATA%\kataglyphis-models`), so only the first run needs the network. A new
model goes into ModelANT first, and only the lock changes here: the Rust crate compiles the
`.onnx` row into `MODEL_FILE`, `scripts/linux/lib/models-lock.sh` and
`scripts/windows/Get-ModelLockFile.ps1` read it for packaging, and
`scripts/windows/check_gpu_detector_parity.py` goes through the hub's parser.
`test-models-lock.sh` fails a second tracked file that names the model outside tests and docs.

**One owner per fact.** The 80 class names are the hub's `coco-classes.txt`, copied to
`crates/core/src/coco-classes.txt` and held there by the lint gates' shared-config check; refresh
the copy with `sync-shared-config.sh --write`, never by hand. `kataglyphis_core::provenance` owns
the chain-marker search ORT and GenAI share, and only `crates/media` makes the frame sink
(`kataglyphis_media::frame_sink`).

Everything else tracked is small: the renderer's glTF/GLB/KTX2 test assets are a few
hundred kilobytes each, and stay that way on purpose.

**Nothing new joins them.** `.gitignore`'s *Large binaries* block excludes model weights,
video, packaging artefacts, archives and signing material. A file kept despite it gets a
negation rather than a narrower pattern, so the rule still bites if the file is moved or
renamed (e.g. `!/resources/models/models.lock`). If another large file really is
necessary, add it to the table above in the same commit that force-adds it.

`logs/windows/` used to hold 15 committed Windows build logs (~2.4 MiB) that
`.gitignore`'s own `logs/**/*` rule already covered; they are untracked now and the
files stay on disk. `Build-Windows.config.psd1` still writes there.
