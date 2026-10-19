# Running

Default features are empty, so everything below beyond `read` and `stats` needs an
explicit `--features` and the system libraries it pulls ([crates.md § Features](crates.md#features)).
ObservANT, the cat cam service, has its own page: [observant.md](observant.md).

## The CLI

The CLI is its own package, not the root one (whose only bin is `burn-demos`), so
`cargo run` needs `-p kataglyphis_cli`:

```bash
cargo run -p kataglyphis_cli -- read --path README.md
```

## Windows: GStreamer + ONNX overlay (wgpu)

Build + Run (CPU via tract):

```bash
cargo run -p kataglyphis_cli --features gui_windows,onnx_tract -- gui --backend dx12
```

Every `onnxruntime*` feature loads ONNX Runtime at run time and downloads
nothing at build time. The only ORT it may load is the family's chain build
(owner rule 2026-09-23): inside the Windows image that is
`$env:ONNX_ROOT\bin\onnxruntime.dll`, found automatically; anywhere else, stage
that file (plus `DirectML.dll` and `onnxruntime_providers_shared.dll`) next to
the exe or point `ORT_DYLIB_PATH` at it. There is no fallback to the
`onnxruntime.dll` Windows ships in System32, and any file that is not the chain
build (a pip wheel's, a GitHub release's) is refused at load time. The portable
bundle (`dist\windows-<x64|arm64>\bundle`), MSIX and MSI carry the chain copy
already.

To check which ONNX Runtime an exe gets without starting the GUI, build it with an
`onnxruntime_*` feature and run its `onnx-runtime` subcommand; it prints the path it
loaded, or fails when no chain ORT loads. Both Windows lanes run it on the packaged
exe:

```bash
cargo run -p kataglyphis_cli --features onnxruntime_directml -- onnx-runtime
```

The GUI creates its camera pipeline's GStreamer elements by name (`mfvideosrc`,
falling back to `autovideosrc`, then `videoconvert`, `capsfilter`, `appsink`), so
their plugins are in no import table. Every Windows package therefore carries them
in `lib\gstreamer-1.0`, and the DLLs they import beside the exe; the list is
`Build.GStreamerPlugins` in `scripts/windows/Build-Windows.config.psd1`.
The `media-check` subcommand (with `gui_windows`) builds that pipeline without
starting it, so with no camera and no window, and prints the plugin file behind each
element; an exe that carries plugins must take every element from them. Both Windows
lanes run it on the packaged exe:

```bash
cargo run -p kataglyphis_cli --features gui_windows -- media-check
```

Build + Run (ONNX Runtime + DirectML):

```bash
cargo run -p kataglyphis_cli --features gui_windows,onnxruntime_directml -- gui --backend dx12
```

Build + Run (ONNX Runtime + CUDA, NVIDIA):

```bash
# PowerShell
$env:KATAGLYPHIS_ORT_DEVICE="cuda"
cargo run -p kataglyphis_cli --features gui_windows,onnxruntime_cuda -- gui --backend dx12

# CMD
set KATAGLYPHIS_ORT_DEVICE=cuda
cargo run -p kataglyphis_cli --features gui_windows,onnxruntime_cuda -- gui --backend dx12
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
  (default: ORT's, spinning). Worth it when inference runs at a capped rate: the cat cam sets it
  itself
- `KATAGLYPHIS_PREPROCESS` – `letterbox` | `stretch` (default: `stretch`)
- `KATAGLYPHIS_SWAP_XY` – set to `1` if the model output swaps X and Y (default: `0`)
- `KATAGLYPHIS_SCORE_THRESHOLD` – detection score threshold (default: `0.5`)
- `KATAGLYPHIS_INFER_EVERY_MS` – minimum interval between inference requests in ms (default: `0`: the next frame goes out as soon as the previous result is back)

CUDA notes:

- Needs the NVIDIA driver plus the CUDA/cuDNN runtime on the machine, and a
  chain-built ONNX Runtime with the CUDA provider beside `onnxruntime.dll`.
  Nothing is copied from a download cache any more.
- If registering the CUDA provider fails, `KATAGLYPHIS_ORT_DEVICE=auto` falls back
  to ONNX Runtime on the CPU. An ONNX Runtime built without the CUDA provider is
  not covered by that: `auto` then gives up on ORT and uses tract if the build has
  `onnx_tract`, and fails otherwise.

The overlay shows FPS, inference latency, CPU/RSS and a CPU history, and inference
can be toggled from it.

## The GUI's backends on Windows

```bash
cargo run -p kataglyphis_cli --features gui_windows -- gui --backend dx12

# Vulkan backend
cargo run -p kataglyphis_cli --features gui_windows -- gui --backend vulkan

# Auto-select (wgpu PRIMARY; also the default)
cargo run -p kataglyphis_cli --features gui_windows -- gui --backend auto
```

## The GUI on Linux

```bash
# WGPU (recommended). The CLI's wgpu GUI feature is gui_windows on every OS;
# it has no gui_wgpu feature.
cargo run -p kataglyphis_cli --features gui_windows -- gui --backend vulkan

# GTK demo
cargo run -p kataglyphis_cli --features gui_unix -- gui
```

## Resource usage logging (CPU/GPU/RAM)

```bash
cargo run -p kataglyphis_cli --features gui_windows,onnxruntime_directml -- --resource-log --resource-log-interval-ms 1000 --resource-log-gpu=true gui
```

Optional, also write it to a file:

```bash
cargo run -p kataglyphis_cli --features gui_windows,onnxruntime_directml -- --resource-log --resource-log-file .\resource.log gui
```

## Burn / PyTorch-replacement demos

A separate binary, behind the `burn_demos` feature.

```bash
cargo run --features burn_demos --bin burn-demos -- --help
```

Examples:

```bash
cargo run --features burn_demos --bin burn-demos -- tensor-demo

cargo run --features burn_demos --bin burn-demos -- linear-regression --epochs 50 --steps-per-epoch 50 --lr 0.02 --batch-size 256

cargo run --features burn_demos --bin burn-demos -- xor --epochs 2000 --lr 0.05

cargo run --features burn_demos --bin burn-demos -- two-moons --epochs 200 --steps-per-epoch 50 --lr 0.01 --batch-size 256

# ONNX Runtime detector demo (default model: resources/models/deim_dfine_n_coco.onnx; fetch it first)
scripts/linux/fetch-models.sh
cargo run --features burn_demos --bin burn-demos -- onnx-detector --runs 1 --print-topk 3
```

## Checking the wasm32 build

```bash
cargo +nightly check --manifest-path Cargo.toml --target wasm32-unknown-unknown -Z build-std=std,panic_abort
```

## Cameras

Raw `gst-launch-1.0` pipelines — listing a device's formats, bisecting a capture
problem below the application, Raspberry Pi CSI and V4L2 sources, MJPEG, hardware
encoders that under-declare their caps — are ANTfrastructure's, in
[`docs/runtime-services.md`](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/runtime-services.md),
§ *Raw `gst-launch-1.0` pipelines (debugging below the app)*. They were once copied
into the README as three bare commands with no explanation of when to reach for them.
