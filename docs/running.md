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
falling back to `autovideosrc`, then `videoconvert`, `capsfilter`, `appsink`), so every
package carries their plugins in `lib/gstreamer-1.0` (Windows: `Build.GStreamerPlugins` in
`scripts/windows/Build-Windows.config.psd1`; Linux: `plugins` in
`scripts/linux/package-linux.sh`; how: [AGENTS.md § Continuous integration](../AGENTS.md#continuous-integration)).
The `media-check` subcommand builds that pipeline without starting it, so with no camera
and no window, and prints the plugin file behind each element:

```bash
cargo run -p kataglyphis_cli -- media-check
```

On Windows ORT registers DirectML; CUDA (NVIDIA) on request, on Linux and Windows:

```bash
# PowerShell
$env:KATAGLYPHIS_ORT_DEVICE="cuda"
cargo run -p kataglyphis_cli -- gui --backend dx12

# bash
KATAGLYPHIS_ORT_DEVICE=cuda cargo run -p kataglyphis_cli -- gui --backend vulkan
```

Optional environment variables:

- `KATAGLYPHIS_ONNX_MODEL` – path to the ONNX model. Unset, the app takes
  `resources\models\deim_dfine_n_coco.onnx` beside its exe, where the portable bundle,
  the MSIX and the MSI all put it, and otherwise the checkout's
  `resources/models/deim_dfine_n_coco.onnx`, a path fixed at compile time. Models are
  not tracked: `scripts/linux/fetch-models.sh` (Windows: `scripts/windows/Get-Models.ps1`)
  downloads what `resources/models/models.lock` names and checks its SHA-256
- `KATAGLYPHIS_ONNX_BACKEND` – `tract` or `ort` (default: automatic)
- `KATAGLYPHIS_ORT_DEVICE` – `cpu` | `auto` | `cuda` (default: `cpu`)
- `KATAGLYPHIS_ORT_THREADS` – ONNX Runtime's intra-op threads (default: `0`, one per core)
- `KATAGLYPHIS_ORT_SPIN` – `0` stops idle ONNX Runtime threads spin-waiting for the next run
  (default: ORT's, spinning); for inference at a capped rate, so the cat cam sets it itself
- `KATAGLYPHIS_PREPROCESS` – `letterbox` | `stretch` (default: `stretch`)
- `KATAGLYPHIS_SWAP_XY` – set to `1` if the model output swaps X and Y (default: `0`)
- `KATAGLYPHIS_SCORE_THRESHOLD` – detection score threshold (default: `0.5`)
- `KATAGLYPHIS_INFER_EVERY_MS` – minimum interval between inference requests in ms (default: `0`: the next frame goes out as soon as the previous result is back)

CUDA notes:

- Needs the NVIDIA driver plus the CUDA/cuDNN runtime on the machine, and a
  chain-built ONNX Runtime with the CUDA provider beside `onnxruntime.dll`.
  Nothing is copied from a download cache any more.
- `KATAGLYPHIS_ORT_DEVICE=auto` takes CUDA when the loaded ONNX Runtime has the provider
  and it registers, and otherwise runs ONNX Runtime on the CPU, with a warning.
  `cuda` fails instead, and the detector falls back to tract.

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

The GTK4 demo GUI (`crates/gui/src/gui.rs`) is not built until the family Linux image
carries GTK4's development files; `BACKLOG.md` holds the change that turns it on.

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
