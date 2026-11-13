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

The GUI creates its camera pipeline's GStreamer elements by name (`mfvideosrc`,
falling back to `autovideosrc`, then `videoconvert`, `capsfilter`, the frame sink), so every
package carries their plugins in `lib/gstreamer-1.0` (Windows: `Build.GStreamerPlugins` in
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
`detector_labels()` returns the class names at their ids. `WebcamStreamConfig.classes` filters
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
- `KATAGLYPHIS_ORT_THREADS` – ONNX Runtime's intra-op threads (default: `0`, one per core)
- `KATAGLYPHIS_ORT_SPIN` – `0` stops idle ONNX Runtime threads spin-waiting for the next run
  (default: ORT's, spinning); for inference at a capped rate, so the cat cam sets it itself
- `KATAGLYPHIS_PREPROCESS` – `letterbox` | `stretch` (default: `stretch`). A model that names
  the other one in its metadata (`kataglyphis.preprocess`) is refused at load, and so is one whose
  `kataglyphis.output` is not `xyxy_score_class`, the one layout this detector decodes: either
  would give wrong boxes on every frame. A model that carries neither key loads as before.
- `KATAGLYPHIS_SWAP_XY` – set to `1` if the model output swaps X and Y (default: `0`)
- `KATAGLYPHIS_SCORE_THRESHOLD` – detection score threshold (default: `0.5`)
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
  The module is taken from `$ORT_AMDGPU_EP_ROOT/lib` when that variable is not empty and the file
  is there (the `:latest-rocm` image), else from the directory of the loaded ONNX Runtime library,
  where an AMD package carries it. It is registered once per process under the name `MIGraphX`.
  AMD's module takes that name as its provider name, so an ONNX Runtime profile should say
  `MIGraphX` too (read in AMD's source, not seen on a card). A missing module, a failed
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
