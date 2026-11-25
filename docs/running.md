# Running

Every build carries everything its target can run, with no features to name
([crates.md § No features](crates.md#no-features-the-target-decides)); the system libraries:
[AGENTS.md § Targets and their system dependencies](../AGENTS.md#targets-and-their-system-dependencies).
ObservANT, the cat cam service, has its own page: [observant.md](observant.md).

## The CLI

The CLI is its own package, not the root one, so `cargo run` needs `-p kataglyphis_cli`:

```bash
cargo run -p kataglyphis_cli -- read --path README.md
```

## The camera GUI with ONNX detection (wgpu)

```bash
cargo run -p kataglyphis_cli -- gui
```

The detector runs on the chain-built ONNX Runtime, loaded at run time; nothing is
downloaded at build time. When ORT cannot load, it says why in a warning and runs on tract
instead (`KATAGLYPHIS_ONNX_BACKEND=tract` picks tract, `ort` refuses the fallback). The only
ORT it may load is the family's chain build (owner rule 2026-09-23): inside the images
`$env:ONNX_ROOT\bin\onnxruntime.dll` (Windows) or `/usr/local/lib/onnxruntime-cpu/lib`
(Linux), found automatically; anywhere else, stage it (on Windows plus `DirectML.dll` and
`onnxruntime_providers_shared.dll`) next to the exe or in its `lib/`, or point
`ORT_DYLIB_PATH` at it. There is no fallback to the `onnxruntime.dll` Windows ships in
System32, and a file that is not the chain build (a pip wheel's, a GitHub release's) is
refused at load time. Every package carries the chain copy: the Windows bundle
(`dist\windows-<x64|arm64>\bundle`), MSIX and MSI, and the Linux tarball, `.deb`, AppImage
and flatpak.

The `onnx-runtime` subcommand (Linux and Windows) prints the path of the ONNX Runtime the
exe loaded, without starting the GUI, or fails when no chain ORT loads. Both Windows lanes
run it on the packaged exe, the Linux lanes on every package:

```bash
cargo run -p kataglyphis_cli -- onnx-runtime
```

The GUI captures through `kataglyphis_media`'s session, as the app's engine does, at the camera's own size: its
GStreamer elements are created by name (`mfvideosrc`, else `ksvideosrc`, `autovideosrc` or `v4l2src`, then
`videoconvert`, `videoscale`, the frame sink), and a camera that ends, fails or sends nothing for 10 s ends the
window with that reason. Every package carries their plugins in `lib/gstreamer-1.0` (Windows:
`Build.GStreamerPlugins` in
`scripts/windows/Build-Windows.config.psd1`; Linux: `plugins` in
`scripts/linux/package-linux.sh`; how: [AGENTS.md § Continuous integration](../AGENTS.md#continuous-integration)).
The `media-check` subcommand builds that pipeline without starting it, so with no camera
and no window, and prints the plugin file behind each element:

```bash
cargo run -p kataglyphis_cli -- media-check
```

ONNX inference uses the CPU by default. To request a GPU execution provider, set
`KATAGLYPHIS_ORT_DEVICE` before starting the application:

```bash
# PowerShell
$env:KATAGLYPHIS_ORT_DEVICE="cuda"
cargo run -p kataglyphis_cli -- gui --backend dx12

# bash
KATAGLYPHIS_ORT_DEVICE=cuda cargo run -p kataglyphis_cli -- gui --backend vulkan
```

CUDA first runs one kernel on the card. On a card the images' ONNX Runtime has no kernel for
(an RTX 20xx today), `cuda` fails naming the card and its compute capability, and `auto` moves on,
to the CPU in the end. An NVIDIA package defaults to `cuda`, so such a card sets `cpu` or `auto`.

To check the selected ONNX Runtime and device without a camera, run `oxidant onnx-check`.
It loads the configured model and runs one synthetic frame. Set
`KATAGLYPHIS_ORT_DEVICE=cpu` to check CPU inference in a GPU package.

Flutter consumers detect any class through `detect_rgba(rgba, width, height, classes)`: the
default model, scores from 0.25, and only the given class ids (empty keeps every class).
`detector_labels()` returns the class names at their ids, and `default_score_threshold()` the
0.25 a slider starts at (hub CON235). `detector_latency_json(rgba, width, height, runs)` times
`runs` such detections after five untimed ones and returns `{"p50_ms":..,"p95_ms":..}`, the line
OmniAccelerANT's `--detect-latency` prints. `WebcamStreamConfig.classes` filters
the live stream the same way, and each `DetectionEvent` names its `backend` (`ort` or `tract`)
and `execution_provider` (`cpu`, `xnnpack`, `cuda`, ...). `detect_persons_rgba` and
`start_webcam_inference` stay; the person filter is `classes: [0]`.
`stop_webcam_inference()` only asks: it returns at once on Dart's UI thread, the session ends on a
thread of its own, and the next start waits until the camera is free.

Desktop Flutter consumers can call `profile_person_detection` through the Rust
bridge to run the same detector on a synthetic frame and receive the completed
ORT profile path. It uses `KATAGLYPHIS_ORT_DEVICE`; inspect the profile's model
node providers to establish where inference ran. The call requires Linux or
Windows and propagates model and provider errors. Regenerate the matching Dart
bindings when updating a consumer to this Rust bridge: the bridge rejects
different Rust and Dart content hashes at startup.

Optional environment variables:

- `KATAGLYPHIS_ONNX_MODEL` – path to the ONNX model. Unset, the app takes
  `resources\models\deim_dfine_n_coco.onnx` beside its exe, where the portable bundle,
  the MSIX and the MSI all put it, and otherwise the checkout's
  `resources/models/deim_dfine_n_coco.onnx`, a path fixed at compile time. Models are
  not tracked: `scripts/linux/fetch-models.sh` (Windows: `scripts/windows/Get-Models.ps1`)
  downloads what `resources/models/models.lock` names and checks its SHA-256
- `KATAGLYPHIS_ONNX_BACKEND` – `tract` or `ort` (default: automatic)
- `KATAGLYPHIS_ORT_DEVICE` – `cpu` | `auto` | `cuda` | `migraphx` (Linux) | `directml` (Windows) |
  `xnnpack` | `dnnl`; default: `cpu`. Every value but `cpu` and `auto` names one provider: when it
  is missing or cannot load, the load fails with `ProviderUnavailable`, never a CPU session.
  The chain ONNX Runtime builds XNNPACK on every arch and DNNL on x86_64 only, so `dnnl` is
  `ProviderUnavailable` on arm64; the current `:latest` also lacks oneDNN's `libdnnl.so.3`, so
  `dnnl` is `ProviderUnavailable` on amd64 too (BACKLOG.md).
- `KATAGLYPHIS_DIRECTML_DEVICE_ID` – optional non-negative Windows DXGI adapter index for DirectML; unset uses ONNX Runtime's default GPU selection
- `KATAGLYPHIS_ORT_THREADS` – ONNX Runtime's intra-op threads (default: the usable CPUs but one, 1 to 8, the
  family's rule since 2026-11-15; `0` keeps ORT's own one per core). A usable CPU is in the affinity mask and
  within the cgroup's CPU quota: a container started with `--cpus 3` on a Pi 5 has 3, not 4
- `KATAGLYPHIS_ORT_SPIN` – `0` stops idle ONNX Runtime threads spin-waiting for the next run
  (default: ORT's, spinning); for inference at a capped rate, so the cat cam sets it itself
- `KATAGLYPHIS_PREPROCESS` – `letterbox` | `stretch` (default: `stretch`). A model that names
  the other one in its metadata (`kataglyphis.preprocess`) is refused at load, and so is one whose
  `kataglyphis.output` is not `xyxy_score_class`, the one layout this detector decodes: either
  would give wrong boxes on every frame. A model that carries neither key loads as before.
- `KATAGLYPHIS_SWAP_XY` – set to `1` if the model output swaps X and Y (default: `0`)
- `KATAGLYPHIS_SCORE_THRESHOLD` – detection score threshold (default: `0.25`, the family's one default)
- `KATAGLYPHIS_INFER_EVERY_MS` – minimum interval between inference requests in ms (default: `0`: the next frame goes out as soon as the previous result is back)

GPU inference notes:

- The inference crate's `[package.metadata.kataglyphis].ort-devices` lists the GPU
  devices its source supports. Consumers can check it before enabling a device; the
  marker does not prove that the installed ONNX Runtime or hardware supports it.
- `cuda` requires NVIDIA drivers, compatible CUDA/cuDNN runtime libraries and a
  chain-built ONNX Runtime with the CUDA provider. `directml` uses the DirectML provider on
  Windows and supports compatible GPUs from multiple vendors.
- `migraphx` (Linux) runs on AMD's plugin module `libmigraphx-ep.so`; the chain ONNX Runtime has
  no MIGraphX provider of its own. It needs an AMD GPU and the ROCm/MIGraphX runtime on the host.
  The module is taken from `$ORT_AMDGPU_EP_ROOT/lib` when that variable is not empty (the
  `:latest-rocm` image); a root without the module is an error, never a fallback. With the variable
  empty or unset it comes from the directory of the loaded ONNX Runtime library, where an AMD package
  carries it. The policy is the hub's `ort_plugin_eps.rs`. It is registered once per process under the
  name `MIGraphX`, or a registration another library of the process made is used.
  AMD's module takes that name as its provider name, so an ONNX Runtime profile should say
  `MIGraphX` too; on the RX 9070 XT it does (2026-11-25). A missing module, a failed
  registration and a module that lists no device are each `ProviderUnavailable: MIGraphX` with
  the paths tried or ONNX Runtime's reason. Before the registration four bytes are copied to the
  GPU in a child process; the load fails with `ProviderUnavailable` if that takes over
  30 seconds, as it does under WSL on an RX 9070 XT. Not run on a card yet: [BACKLOG.md](../BACKLOG.md).
- These options select an execution provider; they do not install its libraries.
  The current CPU packages do not include the GPU runtime closure.
- `auto` tries CUDA, then MIGraphX on Linux or DirectML on Windows, and falls back
  to CPU when registration or session creation fails. An explicit GPU request fails instead
  of silently switching to tract if the provider cannot be registered. ORT may
  still execute unsupported model operators on its CPU provider. An explicit GPU
  request cannot be combined with `KATAGLYPHIS_ONNX_BACKEND=tract`.
- Provider registration alone does not prove that inference ran on a GPU; verify
  it on a supported device with the vendor's monitoring tools.

The overlay shows FPS, inference latency, CPU/RSS and a CPU history, and inference
can be toggled from it.

## The GUI's backends

```bash
# Auto-select (wgpu PRIMARY: Vulkan, DX12 or Metal; also the default)
cargo run -p kataglyphis_cli -- gui --backend auto

# Vulkan backend
cargo run -p kataglyphis_cli -- gui --backend vulkan

# DirectX 12 (Windows only)
cargo run -p kataglyphis_cli -- gui --backend dx12
```

## The GTK4 camera GUI (Linux)

```bash
cargo run -p kataglyphis_cli -- gui-gtk
```

GTK4 >= 4.10 is required at build time. The new family image supplies it; both GUIs
build on Linux without a feature switch. The GTK window uses `gtk4paintablesink` when
available, with Play, Pause and Stop controls. It retains the GStreamer bus watch for
the window session and stops the pipeline when the window/application closes.
The wgpu GUI above owns the detector overlay; the GTK GUI presents camera video.
For a headless GUI check use Xvfb; set `GTK_A11Y=none` where no accessibility bus runs.

## Resource usage logging (CPU/GPU/RAM)

```bash
cargo run -p kataglyphis_cli -- --resource-log --resource-log-interval-ms 1000 --resource-log-gpu=true gui
```

Also to a file:

```bash
cargo run -p kataglyphis_cli -- --resource-log --resource-log-file .\resource.log gui
```

## Burn / PyTorch-replacement demos

Their own crate and binary, `crates/burn_demos`, so the library the consumers link carries
no Burn. The ORT detector demo exists on Linux and Windows.

```bash
cargo run -p kataglyphis_burn_demos -- --help
```

Examples:

```bash
cargo run -p kataglyphis_burn_demos -- tensor-demo

cargo run -p kataglyphis_burn_demos -- linear-regression --epochs 50 --steps-per-epoch 50 --lr 0.02 --batch-size 256

cargo run -p kataglyphis_burn_demos -- xor --epochs 2000 --lr 0.05

cargo run -p kataglyphis_burn_demos -- two-moons --epochs 200 --steps-per-epoch 50 --lr 0.01 --batch-size 256

# ONNX Runtime detector demo (default model: resources/models/deim_dfine_n_coco.onnx; fetch it first)
scripts/linux/fetch-models.sh
cargo run -p kataglyphis_burn_demos -- onnx-detector --runs 1 --print-topk 3
```

## The WebGPU viewer and its scenes

The native viewer opens AssetANT's Colosseum by default (CC BY 4.0, Sulo456). Scenes are data,
never tracked: `resources/scenes/scenes.lock` pins each archive by SHA-256, and the fetch unpacks
it into `resources/scenes/<name>/` through the hub's `KATAGLYPHIS_ASSET_CACHE`, the cache
BeschleunigerBallett's scene fetch uses too. No package and no CI step fetches them.

```bash
scripts/linux/fetch-models.sh --scenes   # Windows: scripts\windows\Get-Models.ps1 -Scenes
cargo run -p kataglyphis_webgpu_renderer --example viewer [scene.gltf|scene.glb|model.obj] [--adapter <name>]
```

**The adapter.** The viewer logs every adapter at startup (`RUST_LOG=info`) and takes the first
that matches, case-insensitively, a part of its `name (API)` label:

1. `--adapter <name>` (or `--adapter=<name>`);
2. `KATAGLYPHIS_ADAPTER`, else wgpu's `WGPU_ADAPTER_NAME`;
3. else the best usable one: discrete, integrated, virtual, unknown, then a CPU rasterizer, never
   a CPU one while hardware is usable. wgpu's `WGPU_POWER_PREF=low` puts integrated first, and
   `WGPU_BACKEND` narrows Vulkan, Metal and Direct3D 12; GL stays refused.

A name that matches nothing logs a warning and falls back to 3. The Renderer tab's ADAPTER list
marks the one in use; *Use* on another starts the viewer again with `--adapter <label>` and the
original arguments, then exits. The browser offers one adapter: Chrome ignores the page's power
preference on Windows (crbug.com/369219127) and takes the GPU set under Windows Settings > System >
Display > Graphics > Chrome. The status bar names it from `GPUAdapterInfo` (`WebGPU · intel
gen-12lp`) and marks an integrated one with that hint.

**Quality.** The Renderer tab's presets: Low (deferred and FXAA instead of 4x MSAA, no SSAO or
bloom, one 1024 cascade, Auto scale from 0.75), Medium (deferred and FXAA, SSAO and bloom, two 1024
cascades, Auto scale from 1), High (the defaults: forward at 4x MSAA, FXAA on every frame, three 2048
cascades, scale 1) and Ultra (High with 4096 cascades and PCF radius 3). Editing a setting a
preset owns shows Custom. The browser starts at Medium on an integrated or unknown GPU, Low on a
CPU one and High on a discrete one; the viewer starts at High. Nothing is persisted.
**VSync** is on (Fifo), as in the C++ engine; the Renderer tab's DISPLAY switch or the Render menu
turns it off, which presents Mailbox where the surface offers it, else Immediate, else Fifo. The
browser always presents in step with the display, so its switch stays on.
**Render scale** renders the scene at 0.25 to 1 of the viewport's size and upsamples in the
tonemap; the chip shows the internal size. **Auto** steers it toward a target frame rate (60 FPS by
default) on raster frames: down by the root of the over-run when a frame takes 15 % over the
target, up a 0.05 step after two seconds of headroom, at most once a second and 40 frames. It
reads the GPU's pass times where timestamps exist (Chrome included), else the median frame time,
which vsync caps, so a frame that meets the target counts as headroom there; a step up taken back
within four seconds doubles the wait before the next (up to 32 s).

The startup scene is the first that exists of:

1. the command line's file;
2. `KATAGLYPHIS_MODEL_OVERRIDE`, the C++ engine's variable: a path under BeschleunigerBallett's
   `Resources/`, absolute, or from the working directory;
3. `resources/scenes/Colosseum-gltf/`, then BeschleunigerBallett's `Resources/Models/Colosseum-gltf/`
   when run from that checkout;
4. the embedded cube, with the fetch command in the status bar.

A missing or broken file never stops the viewer: the status bar says why, and the scene on screen
stays. The library lists `.gltf`, `.glb` and `.obj` (converted in memory) under BeschleunigerBallett's
`Resources/Models` (found from the working directory up, as the C++ finds it), under
`resources/scenes`, and in the command line file's folder. A load parses on a worker thread and
uploads when it is done; the status bar and the viewport chip show it. A scene starts at the camera
its glTF names in `scenes[0].extras.active_camera`, else at its first camera, else fitted to its
bounds, with near and far planes from its size. Its first directional light becomes the Sun: its
direction and colour, at the Sun's own intensity, since glTF gives lux and the renderer has no
exposure for it (the Colosseum's sun is 4098 lx). A scene's `asset.copyright` shows in the status bar
and in About while it is loaded.

The browser demo starts on its embedded cube. The library's `Colosseum` row fetches
`Colosseum-web.glb` beside the page, or from `?scene-base=<folder URL>`: GitHub release downloads
send no CORS header, so the site that serves the demo must serve the file. In the browser the
profiler starts hidden, and a narrow canvas gives up the inspector before the scene dock, the
reverse of the C++ order, so the docs site's iframe (about 800 px) keeps the library.

## Checking the wasm32 and Android builds

The root package alone, as OmniAccelerANT's web build and Cargokit build it:

```bash
cargo build -p oxidant --lib --locked --target wasm32-unknown-unknown
cargo build -p oxidant --lib --locked --target aarch64-linux-android   # with the NDK's clang as linker and CC
```

## Cameras

Raw `gst-launch-1.0` pipelines (listing a device's formats, bisecting a capture problem
below the application, Raspberry Pi CSI and V4L2 sources, MJPEG, hardware encoders that
under-declare their caps) are ANTfrastructure's:
[`docs/runtime-services.md`](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/runtime-services.md)
§ *Raw `gst-launch-1.0` pipelines (debugging below the app)*. Three of them once sat in
the README as bare commands, with no word on when to reach for them.
