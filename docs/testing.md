# Tests

## The whole suite

Unit + integration + proptest fuzz + doc tests at the debug profile, as every Linux lane
runs them (`bash scripts/linux/ci-container-steps.sh test`):

```bash
scripts/linux/fetch-models.sh
scripts/linux/fetch-models.sh --llm
KATAGLYPHIS_REQUIRE_GPU=1 KATAGLYPHIS_REQUIRE_MODEL=1 KATAGLYPHIS_REQUIRE_GST=1 \
  KATAGLYPHIS_REQUIRE_GTK=1 KATAGLYPHIS_REQUIRE_GENAI=1 xvfb-run -a cargo test --workspace --locked
```

There are no features to add: the capture tests, the ORT loader, the webcam engine and the
burn demos compile wherever the target runs them, so GStreamer's development files and the
chain ORT must be present ([AGENTS.md § Targets and their system dependencies](../AGENTS.md#targets-and-their-system-dependencies)).

Each `KATAGLYPHIS_REQUIRE_*` turns a self-skip (no GPU, no model, a missing GStreamer
element, no chain GenAI) into a failure, since `cargo test` hides a passing test's `SKIP:` line.
The Windows x64 container and CI lane (`Invoke-DebugTests.ps1`) require GenAI too; the riscv64 and Windows arm64 lanes do not yet, so
the text suites' GenAI cases skip there ([text.md](text.md#tests)).
`fetch-models.sh` puts the model the real-model tests need in place; `--llm` unpacks the language
model the text contract replays ([text.md](text.md#tests)).

The Linux test and coverage steps require GTK under Xvfb. The GTK lifecycle test posts a real EOS
message and checks that the retained bus watch stops the pipeline, then verifies
session shutdown. `gui-gtk --help` is covered by the CLI integration suite.
Coverage keeps the workspace's 70% floor and every per-crate floor; a missing display
must fail rather than silently omit the GTK lifecycle from the measurement.
Each per-crate floor in `COVERAGE_FLOORS` stays within 3 points of its printed figure:
`check-crate-coverage.py --max-slack 3` warns about a larger slack without failing, and a crate
that has no floor fails the step. The lane uploads `cobertura.xml` and the `rustdoc-log` step's
`logs/code-quality/rustdoc.txt`, written without colours so its `warning:` lines count, as
`lane-reports-<suffix>`, which quality-report reads.

The CLI suite's `gui_finishes_its_first_frame` starts `kataglyphis_cli gui` under the same display with no
model and waits for its `First GUI frame finished` line. That first frame uploads the font atlas, so a
texture delta left undrained fails it, through epaint's debug-only drop check. With
`KATAGLYPHIS_REQUIRE_GPU` and `KATAGLYPHIS_REQUIRE_GST` set, a GUI that cannot start fails; otherwise it
prints `SKIP:`. It does not cover a failed surface acquire, later frames or a real camera, and proves
nothing under a release profile, where the check is compiled out.

`a_frame_before_the_first_camera_sample_passes_validation` holds the race its window test cannot force: with
no camera frame yet the GUI must skip the quad's draw, since a draw without its bind group fails wgpu validation.

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

**After the tests, the test step compares the backends on ModelANT's photo.**
`scripts/linux/check-backend-parity.sh LAUNCHER FIXTURE` runs `kataglyphis_cli onnx-detect`
on `third_party/ModelANT/contracts/fixtures/two_cats.jpg` with ORT on the CPU provider, with
`--tract`, with `KATAGLYPHIS_ORT_DEVICE=xnnpack` and with `dnnl`, and compares each against the
ORT CPU result with the hub's `shared/python/compare_detector_parity.py` (IoU at least 0.90,
score difference at most 0.05). Each ORT run's profile must name its provider through the hub's
`verify_ort_profile.py`, so an XNNPACK request that ran on the CPU provider fails. A DNNL run may
fail only with `ProviderUnavailable: DnnlExecutionProvider` (arm64, and amd64 while `:latest`
lacks `libdnnl.so.3`). `crates/inference/tests/cpu_accelerators.rs` checks the same providers
in-process and that a missing GPU provider is `ProviderUnavailable`. `plugin_ep.rs` runs AMD's
plugin path where it cannot run, against the image's ONNX Runtime: no module, a file that does
not load, and (on arm64, with Qualcomm's module as the stand-in) a module that registers and
lists no device. Each makes a named `migraphx` fail with `ProviderUnavailable: MIGraphX` and
its reason, and lets `auto` run on the CPU. No test here loads `libmigraphx-ep.so`. The parity check does not
cover GPU providers or packaged launchers. A tract-only channel swap fails it; a channel swap in
the shared preprocessing does not, since both backends then see the same image and still find
both cats.

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

The `lint-gates.yml` script-test job also runs `test-burn-lock-graph.sh` with Python 3.11+
`tomllib`: it checks the real `Cargo.lock` and seven synthetic Torch/ZIP/Zstd reintroductions.
The gate is also run after local Renovate `--apply`. Cargo audit and deny remain the security
verdicts; the lock-graph check does not exercise Burn at runtime.

## The bridge bindings

`src/frb_generated.rs` is generated code, and the `fmt-clippy` step fails when it is stale.
`scripts/linux/frb-codegen.sh` holds the one flutter_rust_bridge input (`crate::api`) and flag
set. OmniAccelerANT's `scripts/linux/generate-frb-bindings.sh` calls the same script with its
own Rust root and Dart output, so neither repository carries a second copy.

`frb-codegen.sh check --hub <ANTfrastructure>` is what the step runs:

1. The generator must report the hub's `FLUTTER_RUST_BRIDGE_VERSION`, and the workspace's exact
   `flutter_rust_bridge` pin must equal it.
2. Full cross-target `cargo metadata --locked` must succeed. The generator reads every target's
   dependencies and silently writes the Dart loader stem `UNKNOWN` when that fails.
3. cargo-expand 1.0.127 is installed from its checksummed crate when another version is found;
   the generator would otherwise install whatever is newest.
4. Both sides are regenerated, the Dart side into a temporary package named `omni_accelerant`:
   that name prefixes every exported Rust symbol.
5. `verify-frb-bindings.py` requires the loader stem to equal `[lib] name` and both sides to carry
   the same generator version and content hash; then `git diff --exit-code src/frb_generated.rs`
   must pass.

After a change under `src/api`, run OmniAccelerANT's `bash scripts/linux/generate-frb-bindings.sh`
in the image: it writes both sides. Commit and push OxidANT first, then move the app's gitlink.
In this repository alone, the check above, run in the image, rewrites `src/frb_generated.rs`.
The first run needs the network for cargo-expand and for the crate archives full metadata reads.
The check does not show that the Dart side compiles or that the library loads; OmniAccelerANT's
tests do.

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

- Script suites: `scripts/linux/tests/test-*.sh`. A harness suite sources `lib-suite.sh`
  (`tests_dir`, `work`, `suite_run`); the GPU check suites source `lib-fake-gpu.sh`
  for their fake launchers and refusal checks.

Benchmarks (the `[[bench]]` targets only, graded by the hub's `bench-history` action under keys
`criterion-x64` and `criterion-arm64`) and the detector's real-model test: [AGENTS.md § 5](../AGENTS.md#5-build-run-test);
coverage floors and mutation testing:
[§ Coverage and mutation testing](#coverage-and-mutation-testing); the riscv64
lane's QEMU run: [§ The riscv64 lane](ci.md#the-riscv64-lane).

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

## Coverage and mutation testing

- **Coverage measures the workspace** (since 2026-10-01). The hub's `cargo_coverage.sh` runs tarpaulin without `--workspace`, so it measured only the root package, which has no unit tests: `0.00% coverage, 0/754 lines covered` (run 36868485805). The `coverage` step passes `--workspace`, and sets `KATAGLYPHIS_REQUIRE_GPU=1` like the `test` step. It fails below 70 % of the workspace's lines, the vendored crates under `third_party/` left out (tarpaulin `--fail-under` and `--exclude-files`; 72.9 % measured 2026-10-19, 71.0 % on 2026-10-20 once the GUI, which needs a window for most of its code, joined the figure); raise the floor as tests are added. Since 2026-10-15 it also grades each crate and `src/` against its own floor (`COVERAGE_FLOORS`, read by `scripts/linux/check-crate-coverage.py` from `cobertura.xml`), so the renderer's 90 % no longer carries telemetry's 8 %, and gives tarpaulin `--timeout 300` (its 60 s per test binary was too short on the Pi for an instrumented ObservANT). Each floor sits within 3 points of the figure the step prints; `--max-slack 3` prints a `warning: floor slack` line for a crate further above its floor, without failing, so raise that floor, and a crate with no floor fails the step.
- **Every module counts since 2026-10-20.** Tarpaulin leaves out every module behind `#[cfg(feature = …)]` (measured 2026-10-15, tarpaulin 0.37.5), so `crates/media` and `src/webcam_engine.rs` were absent from the report while their tests passed; with no features left they, the GUI and the burn demos are in it, each with a floor of its own.
- **A test binary must not see the distro's `libstdc++` first.** Tarpaulin puts every `rustc-link-search` path a build script prints on `LD_LIBRARY_PATH`; font-kit's fontconfig and freetype printed `/usr/lib/x86_64-linux-gnu`, its `libstdc++` (GCC 15) won over GCC 16.2's, and the chain ORT failed to `dlopen` there alone (2026-10-20), which `real_model.rs`'s backend check caught. The burn demos' plots use plotters' `ab_glyph` instead; a new system library linked through pkg-config brings the same trap back.
- **Mutation testing measures whether the tests notice a change** (2026-10-15). `cargo mutants -p kataglyphis_observant --file crates/observant/src/web.rs --file crates/observant/src/static_files.rs --jobs 1 -- --lib` (cargo-mutants 27.1.0, installed with `cargo install --locked cargo-mutants`) plants small bugs in ObservANT's code, one at a time; that command covers its network-facing code. Run it after changing the files below. Caught, of the mutants that compile:

  | Files | At first | After tests for each gap | The gaps | Left, and why |
  | --- | --- | --- | --- | --- |
  | `web.rs`, `static_files.rs` (163 mutants) | about 84 % | 142 of 145 | content types, the gzip cache, route resolution, peer CORS, the signalling proxy's leftover bytes, split and endless response heads, re-framing a refusal | 3 change nothing a client can see (HEAD and 304 bodies hyper drops anyway, a log line's throttle, `>` against `>=` at a 16 KiB limit) |
  | `motion.rs`, `presence.rs`, `status.rs`, the arrival logic (393 mutants) | about 92 % of the 379 that compile | 375 | exact box margins, the change and motion thresholds, an IoU of exactly 0.5, the visits file's fallback, the settings' limits, a stale frame at 21 s, the failure count | 4 only change a log line or a bound no test can see |
  | `camera.rs`, `config.rs`, `notify.rs` | about 90 % of 145 | 134 | a webcam by driver or bus, a format it can decode, rpicam-vid without a sensor, a configured `/dev/video` path, each config limit taken itself, a short webhook host | 11 read real camera devices, which no CI host has, or change a log line |
  | `clip.rs`, `frame.rs`, `stats.rs`, `output.rs`, `precompress.rs` | 376 of 412 | 400 | the privacy filter's margin, block mean and chroma planes, the box outline's exact pixels, the clip's frame counts, a pixel's own luma and chroma, and the stats sampler | of the 12, 9 only widen the privacy filter's whole-block shortcut, whose per-pixel path gives the same picture, and 3 change a log line or an encoder error no test can raise |
  | `capture.rs`, the capture loop around a real camera (the weakest) | 87 of 124 | 98 | the arrival photo's privacy filter (decoded, so a swapped filter fails too), the bus's error and end of stream, the restart pause and a zero width | needs a Pi camera, a V4L2 device or a loaded model, or changes a log line |

  The webhook tests fail after 10 s instead of hanging when no event comes, which a mutant that sends nothing first showed: each one cost the run its 300 s timeout. **Keep `--jobs 1` or give each job its own target dir**: two jobs sharing `CARGO_TARGET_DIR` overwrite each other's test binary and misreport, and so do two runs at once (a check run beside a long one reported caught mutants as missed on 2026-10-15), so each run gets a target dir of its own. The three arrival files take about 75 minutes on a Pi 5, a `--re` over the changed functions minutes.

## Latency on a quiet host

`bash scripts/linux/quality-report.sh --perf-host` times `kataglyphis_cli detect-latency` per CPU
provider and `chat` through `crates/text` in the CI image, against `perf/baseline-<cpu>.json`:
hub `docs/fleet-health.md` § perf-host. `detect-latency --image FILE --runs N` prints warm p50 and p95
as one JSON line, or `not_collected` for a provider the runtime cannot load. It is not run in CI, whose
hosted wall time varies too much to grade.
