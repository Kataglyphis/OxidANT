# Tests

## The whole suite

Unit + integration + proptest fuzz + doc tests at the debug profile, as every Linux lane
runs them (`bash scripts/linux/ci-container-steps.sh test`):

```bash
scripts/linux/fetch-models.sh
KATAGLYPHIS_REQUIRE_GPU=1 KATAGLYPHIS_REQUIRE_MODEL=1 KATAGLYPHIS_REQUIRE_GST=1 \
  KATAGLYPHIS_REQUIRE_GTK=1 xvfb-run -a cargo test --workspace --locked
```

There are no features to add: the capture tests, the ORT loader, the webcam engine and the
burn demos compile wherever the target runs them, so GStreamer's development files and the
chain ORT must be present ([AGENTS.md § Targets and their system dependencies](../AGENTS.md#targets-and-their-system-dependencies)).

Each `KATAGLYPHIS_REQUIRE_*` turns a self-skip (no GPU, no model, a missing GStreamer
element) into a failure, since `cargo test` hides a passing test's `SKIP:` line.
`fetch-models.sh` puts the model the real-model tests need in place.

The Linux test and coverage steps require GTK under Xvfb. The GTK lifecycle test posts a real EOS
message and checks that the retained bus watch stops the pipeline, then verifies
session shutdown. `gui-gtk --help` is covered by the CLI integration suite.
Coverage keeps the workspace's 70% floor and every per-crate floor; a missing display
must fail rather than silently omit the GTK lifecycle from the measurement.

The CI script keeps a parent shell while `xvfb-run` starts. Replacing that shell
with `exec xvfb-run` can make the launcher PID 1 in the container; Xvfb then
omits the readiness signal and the launcher waits before starting Cargo.
The script regression checks both handoffs, forwarded arguments, failure status and
the coverage report's per-crate floor check.

Inference preprocessing reads ModelANT's pinned `contracts/inference-v1.json` through
`third_party/ModelANT`. The C++ and Python implementations use those same cases.
The bounded model cache test deliberately replaces the cache on another thread and
asserts the first request keeps its original model reference.

`scripts/generate-texture-abi.py --check` rejects drift between `abi/texture.json`,
the generated C header and Rust types. A tiny loaded shared library with a mismatched
ABI version must fail before `knt_push_frame` is looked up, without a model or camera.

**Both ONNX backends meet the real model.** `crates/inference/tests/real_model.rs` checks that
the chain ORT is the backend on Linux and Windows and that tract finds both cats too;
`ort_fallback.rs` points `ORT_DYLIB_PATH` at nothing and checks that the detector warns with
ORT's reason and falls back to tract; `model_errors.rs` checks each backend's error, and that
a model neither loads names both reasons.

## Formatting and lints

CI gates on both, as hard failures. Run them before pushing:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

That is the scope of the lanes' `fmt-clippy` step (`CARGO_CLIPPY_ARGS` in
`scripts/linux/ci-container-steps.sh`); `bash scripts/linux/ci-container-steps.sh fmt-clippy`
inside the image reproduces the step itself. The shell, workflow, secret and config gates
are `bash scripts/linux/run-lint-gates.sh`.

## The GPU tests must render

The `crates/webgpu_renderer` headless golden tests (about 40) need a GPU adapter and
**silently skip without one**: `GpuContext::headless_or_skip()` returns `None`.
`KATAGLYPHIS_REQUIRE_GPU=1` makes it panic instead, so a green run means they rendered.
Every CI lane that renders sets it: the Linux lanes render on the image's software Vulkan
device (lavapipe), the Windows lanes on the runners' software Direct3D 12 adapter (WARP).
Each GPU test binary writes one `ADAPTER:` line straight to stderr, past libtest's
capture, when it first gets a device: the adapter, and whether it offers BC textures and
timestamp queries. Every CI adapter reports `bc=true timestamps=true`, so under the flag
`ktx2::uploads_bc1_when_supported` and
`gpu_timing::every_pass_reports_a_finite_non_negative_duration` fail instead of skipping
without them. Since when each lane sets it, and the one test it cannot hold:
[AGENTS.md § Known gaps](../AGENTS.md#known-gaps).

**They render under the browser's limits.** `GpuContext` requests `wgpu::Limits::default()`,
WebGPU's defaults, on native too: a browser grants no more whatever the adapter offers, so a
layout the web demo would reject (a fifth bind group, say) fails these tests.
`tests/shader_export.rs` checks the WGSL without a GPU: no `@group` past 3, and no struct
that repeats a `@location`, which naga accepts and Tint, the browsers' WGSL compiler, rejects.

## Where the suites live

- Unit tests inside the workspace crates (`kataglyphis_telemetry`, `kataglyphis_inference`,
  and the renderer's own suite). The ESP32 link's run over an in-memory serial line
  (`Link::start`), so they need no board.
- Integration tests: `tests/integration/integration.rs`, which run the CLI and are
  compiled as `kataglyphis_cli`'s `integration` test (`crates/cli/tests/integration.rs`);
  `crates/media/tests/capture_test.rs` (a `videotestsrc` pipeline and camera
  enumeration); `crates/inference/tests/` (the real model on both backends, the fallback,
  the load errors); `crates/burn_demos/tests/` (each demo's result, the detector demo on the
  real model); and the renderer's 18 binaries under `crates/webgpu_renderer/tests/`.
- Property-based (fuzz) tests: [proptest](https://proptest-rs.github.io/proptest/)
  properties on ObservANT's network-facing parsers (Range header, query, Host/Origin
  check, request path, camera spec, config file), 256 random inputs per property and run,
  in the `mod properties` of `web.rs`, `static_files.rs`, `camera.rs` and `config.rs`;
  and on the ESP32 frame codec (any split into reads, noise before a frame, a single bit
  error) in `crates/esp32-protocol/tests/framing.rs`.
  There is no `cargo-fuzz`/libFuzzer target.

Benchmarks and the detector's real-model test: [AGENTS.md § 5](../AGENTS.md#5-build-run-test);
coverage floors and mutation testing:
[§ Coverage and mutation testing](../AGENTS.md#coverage-and-mutation-testing); the riscv64
lane's QEMU run: [§ The riscv64 lane](../AGENTS.md#the-riscv64-lane).

## On Windows the renderer's tests run outside the container

Every renderer test binary links wgpu, whose `gles` backend makes the executable import
`opengl32.dll` at load time. Windows Server Core does not ship that DLL, so the process
dies with `0xc0000135` (`STATUS_DLL_NOT_FOUND`) before `main`. The loader resolves the
import, so no runtime flag avoids it, and letting it run turned the whole
`cargo test --workspace` into a crash with no results. `gles` stays on purpose: it is the
OpenGL fallback for machines without Vulkan/DX12. The camera GUI turns it off (its
`GpuBackend` picks Vulkan, DX12 or Metal only), so `kataglyphis_gui` and the CLI test and run
in Server Core as long as the renderer is not in the same build. Run the renderer's tests on
a desktop Windows host, where the DLL exists:

```pwsh
$env:KATAGLYPHIS_REQUIRE_GPU = '1'; cargo test -p kataglyphis_webgpu_renderer --locked
```

CI splits the same way: `windows-x64.yml` builds the renderer's tests in the container and
runs them on the runner, a desktop Windows Server with `opengl32.dll`;
`windows-arm64-cross.yml` runs the whole workspace's tests on `windows-11-arm`. Its staged
artifact includes `shaders/forward/forward.slang` for the renderer's shader-constant tests.
The lane's build and run sequence is in
[AGENTS.md § Continuous integration](../AGENTS.md#continuous-integration).
