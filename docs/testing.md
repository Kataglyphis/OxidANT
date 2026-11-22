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
that has no floor fails the step. The lane uploads `cobertura.xml`, the `rustdoc-log` step's
`logs/code-quality/rustdoc.txt`, written without colours so its `warning:` lines count, and the
`test` step's `logs/test-results/cargo-test.xml` as `lane-reports-<suffix>`, which quality-report reads.
The hub's `cargo_test.sh` (its `cargo-test-junit.py`, ANTfrastructure CON250) writes that JUnit from the step's log, also after a failed run:
one case per test with its outcome, an ignored test's reason and a failed test's captured output, and
one failed case `(binary ended early)` with the signal for a binary that died before its summary.
Stable libtest prints no per-test time, so each suite carries its binary's total only.
Before it fetches anything, the `test` step runs `scripts/linux/check-genai-signatures.py`: `crates/text/src/genai.rs`
types GenAI's C functions by hand, so the declarations of the 37 symbols it binds are taken from the image's
`ort_genai_c.h` and compared with `crates/text/genai-signatures.txt`; a GenAI bump that changes one fails the step and
prints the changed lines. After checking `genai.rs` against the header, `--write` in the image renews the snapshot.

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
`third_party/ModelANT`, and `postprocess.rs` replays its decode cases: raw rows in, detections out
in row order, under the five rules for end-to-end rows. The C++ and Python implementations use
those same cases. The text contract is compiled in from the same pin ([text.md](text.md#tests)).
`crates/inference/tests/fixtures/two_cats.jpg` stays here, as the original ModelANT's copy cites
and the path OmniAccelerANT reads; `the_photo_is_byte_for_byte_the_one_modelant_holds` in
`real_model.rs` compares it with `third_party/ModelANT/contracts/fixtures/two_cats.jpg`. Both
files are compiled into the test binary, so the test needs no staged input on the cross lanes.
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
`scripts/linux/ci-container-steps.sh`), which then lints the renderer for the browser too:
`cargo clippy -p kataglyphis_webgpu_renderer --locked --target wasm32-unknown-unknown -- -D warnings`.
`bash scripts/linux/ci-container-steps.sh fmt-clippy`
inside the image reproduces the step itself. The shell, workflow, secret and config gates
are `bash scripts/linux/run-lint-gates.sh`; with the hub's hook pair switched on (`AGENTS.md` § 5) a push
runs them and `scripts/linux/pre-push-checks.sh`, the script suites of the lint lane.

**Function size is a hard gate in every crate.** `crates/observant`, `crates/inference`,
`crates/media`, the root crate `src` and, since 2026-11-14, `crates/text`, `crates/core`, `crates/esp32`, `crates/esp32-protocol`,
`crates/cli`, `crates/telemetry`, `crates/gui`, `crates/burn_demos`, `crates/cxx_bridge`, `crates/assets`, `crates/render_core` and `crates/webgpu_renderer` set `#![warn(clippy::too_many_lines)]`, and `clippy.toml`
holds the limit, 80 lines in clippy's count (code lines only), so `-D warnings` fails a new long
function there. Those over it carry `#[expect(clippy::too_many_lines)]` where clippy's count passes 80. An
`#[expect]` does not stop a function from growing, so `scripts/linux/check-rust-function-sizes.sh`,
run by `pre-push-checks.sh`, holds every function over 80 lines in those crates to its row in
`rust-function-size.allow` (the hub's count, comments included): one that grew, or a new one,
fails. Shrink a frozen function and lower its row; split it and drop the row and the `#[expect]`.
On a small host check one crate at a time:
`cargo clippy -p kataglyphis_observant --all-targets -- -D warnings`.

**A dead Slang `import` fails the lint lane.** `scripts/check-slang-imports.py`, run by
`pre-push-checks.sh`, fails an `import` under `shaders/` when the importing file names nothing the module
declares at top level (function, struct, interface, constant, global, `#define`); an `__exported import`
counts for its importers. Slang compiles such an import silently, and `traced.slang` carried one from
18e178a until 2026-11-16. Its fixture suite is `scripts/linux/tests/test-slang-imports.sh`.

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

The `crates/webgpu_renderer` headless golden tests (about 180) need a GPU adapter and
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

**On arm64 llvmpipe runs at 128-bit vectors, or the tests refuse it.** The image sets
`LP_NATIVE_VECTOR_WIDTH=256`, twice NEON's width, and at 256 bits LLVM 21's AArch64 backend
corrupts the heap while it compiles llvmpipe's shaders. Measured 2026-11-16 on the 8-core arm64
dev host in the image (Mesa 26.0.8, LLVM 21.1.8), every GPU test binary once per pass at
`--test-threads 4`:

| Width | Binary runs | Crashed |
| --- | --- | --- |
| 128 | 405 | 0 |
| 256 | 270 | 6 |

At 256 a run dies with a SIGSEGV in `llvm::SelectionDAG::createOperands` under MCJIT on an
llvmpipe thread, or with glibc's `double free or corruption` or `free(): invalid size` when a
test thread exits and frees its arena. Serially, 25 runs each of `deferred` and `occlusion` at
256 passed, so it takes concurrent shader compiles at 256 bits, which points at a race in LLVM's
AArch64 backend; the same concurrency at 128 never crashed. `GpuContext::headless_or_skip` therefore
refuses llvmpipe wider than 128 bits on aarch64 (`check_test_adapter`), so the GPU tests fail at
once naming `LP_NATIVE_VECTOR_WIDTH=128`, or skip without `KATAGLYPHIS_REQUIRE_GPU`, instead of
crashing one run in a few; the app does not refuse it. `scripts/linux/ci-container-steps.sh`
exports 128 on an aarch64 host, so only a bare `cargo test` in the image needs it by hand:

```bash
LP_NATIVE_VECTOR_WIDTH=128 KATAGLYPHIS_REQUIRE_GPU=1 cargo test -p kataglyphis_webgpu_renderer
```

No amd64 crash has been reproduced. A binary that dies in CI is named by the JUnit record
(above), and its `ADAPTER:` line gives the vector width.

**They render under the browser's limits.** `GpuContext` requests `wgpu::Limits::default()`,
WebGPU's defaults, on native too: a browser grants no more whatever the adapter offers, so a
layout the web demo would reject (a fifth bind group, say) fails these tests. The two buffer
sizes are the exception natively, so the tracer can bind a large scene; its own tests check
its buffers against the defaults ([tracer.md](../crates/webgpu_renderer/docs/tracer.md#limits)).
`tests/shader_export.rs` checks the WGSL without a GPU: no `@group` past 3, and no struct
that repeats a `@location`, which naga accepts and Tint, the browsers' WGSL compiler, rejects.

**The full Colosseum traces only on request.** `tests/tracer.rs`'s
`the_full_colosseum_traces_on_this_device` is ignored by default: it needs the scene
(`bash scripts/linux/fetch-models.sh --scenes`), about 3 GiB of memory and a minute and a half
unoptimised. It builds the 5.9 M-triangle scene, checks that every tracer buffer fits the
device (lavapipe binds 128 MiB; the largest buffer is 90 MiB), traces 1600 rays on the GPU
against the CPU traversal and draws one ray-traced frame. Run it alone, since lavapipe holds
about 850 MiB of the scene:

```bash
cargo test -p kataglyphis_webgpu_renderer --test tracer the_full_colosseum -- --ignored --nocapture
```

## The web demo in a browser

`bash scripts/linux/ci-container-steps.sh web-browser [--frames DIR]` in the family image builds
the wasm demo (release, `wasm-bindgen` pinned to `Cargo.lock`) and runs
`scripts/linux/webgpu-demo-browser.js` against it: the image's headless Chrome on SwiftShader's
WebGPU (the hub's `webgpu-canvas-readback.js` flags and read-back, `check-docs-sidebars.js`'s launcher).
It turns the camera to 5° so half the scene is sky and fixes the render scale at 1 (the browser
presets steer it down to a quarter on SwiftShader), then sets five views through the demo's
`set_demo_view(mode, path, clouds)`, `set_demo_camera(yaw, pitch)` and `set_demo_render_scale(scale)`
exports (`src/wasm_demo.rs`, test hooks the page never calls): forward, deferred, deferred with clouds, ray traced and path
traced (2 samples at least). Each step waits until `demo_view()` reports that mode drawn, reads the
second frame back, and fails when:

- the frame drew another mode than asked (`drawn_mode`, the chip's), or the tracer failed;
- the scene viewport, 2 % in from its edges, has fewer than 256 colours or one colour over 90 %;
- clouds change under 1 % of pixels against deferred;
- the forward or the deferred frame ran no FXAA, whose default, Always, filters both (`demo_view().fxaa`);
- the ray-traced frame lacks its own shadow: the cube's sun shadow, black without an ambient term
  (every channel under 48), covers under 0.05 % of the viewport, or forward has a tenth as many such pixels;
- in that shadow the path-traced frame is not lit by the sky (mean brightest channel under 80) or not
  tinted by the cube's red bounce and the blue sky (mean (r + b) / 2 − g under 10, forward's is about 0);
- the page logged any console error or warning, a browser log error or warning (WebGPU's
  validation messages arrive there) or an uncaught exception; Chrome's favicon 404 alone is let pass.

The traced modes are judged by those shadows, not by a share of changed pixels: a traced frame differs
from forward by only 2.6 to 3.2 %, too close to any floor.

`--frames DIR` keeps each read-back as a PNG. `web-browser.yml` runs it on request (`[ci web]`,
`[ci browser]`) and by hand, and uploads the frames. `scripts/linux/tests/test-webgpu-demo-browser.sh`
checks the verdicts and the console filter without a browser.

Receipt, 2026-11-16 (Chrome for Testing 157.0.8103.38 in `:latest`, arm64 host, canvas 960x528):
forward 1095 colours, deferred with FXAA 1474, clouds 4449 and 33.0 % changed; the ray tracer's black
shadow 0.174 % of the viewport (forward none), and in it the path tracer at 88 samples 165.7 bright and
tinted 28.3 (forward 0.8); no console or WebGPU message. The path-tracing kernel's first frame took most
of its 124 s step, the others 3-7 s. The same views from
`editor_snapshot` on lavapipe look alike, the clouds' grain included (the shared estimator's, fixed the
same day: `CHANGELOG.md`).

## Where the suites live

- Unit tests inside the workspace crates (`kataglyphis_telemetry`, `kataglyphis_inference`,
  and the renderer's own suite). The ESP32 link's run over an in-memory serial line
  (`Link::start`), so they need no board.
- Integration tests: `tests/integration/integration.rs`, which run the CLI and are
  compiled as `kataglyphis_cli`'s `integration` test (`crates/cli/tests/integration.rs`);
  `crates/media/tests/capture_test.rs` (a `videotestsrc` pipeline and camera
  enumeration); `crates/inference/tests/` (the real model on both backends, the fallback,
  the load errors); `crates/burn_demos/tests/` (each demo's result, the detector demo on the
  real model); and the renderer's 45 binaries under `crates/webgpu_renderer/tests/`, where
  `shader_vectors.rs` runs the shared Slang modules' vector kernels ([shaders.md](shaders.md#vector-kernels)),
  `feature_matrix.rs` runs the shadow settings, the cube skybox, the clouds, the model transform and
  frustum culling on both raster paths and in both traced modes. `settings_sweep.rs`, BeschleunigerBallett's twin,
  renders every setting at its maximum at 1200x768 (3 cascades at 4096, PCF 5, 128/128 cloud steps, 64 spp at 16
  bounces, 4 shadowed point and spot lights) and 16 seeded random settings over the 3 modes and 2 raster paths inside
  a validation scope; it skips unless `KATAGLYPHIS_SETTINGS_SWEEP=1` and runs weekly in `settings-sweep.yml` (2.5 min
  on lavapipe on a Pi 5).
- Scenario replays: `crates/observant/tests/scenarios.rs` drives the model runs in
  `crates/observant/tests/scenarios/*.jsonl` through ObservANT's visit rules and asserts each
  scenario's visits and stays ([observant.md](observant.md)); a retuned rule fails the scenario
  that holds it (the negative controls are in `CHANGELOG.md`, 2026-11-16).
- Property-based (fuzz) tests: [proptest](https://proptest-rs.github.io/proptest/)
  properties on ObservANT's network-facing parsers (Range header, query, Host/Origin
  check, request path, camera spec, config file), 256 random inputs per property and run,
  in the `mod properties` of `web.rs`, `static_files.rs`, `camera.rs` and `config.rs`;
  and on the ESP32 frame codec (any split into reads, noise before a frame, a single bit
  error) in `crates/esp32-protocol/tests/framing.rs`; and on `load_flat`, the glTF loader
  BeschleunigerBallett's engine calls, over mutated copies of nine self-contained renderer
  assets (512 per run, `crates/assets/tests/flat.rs`): a mutant is an error or consistent arrays.
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
artifact includes `shaders/forward` for the renderer's shader-constant tests, which read `forward_lighting.slang`.
The lane's build and run sequence is in
[AGENTS.md § Continuous integration](../AGENTS.md#continuous-integration).

## Coverage and mutation testing

- **Coverage measures the workspace** (since 2026-10-01). The hub's `cargo_coverage.sh` runs tarpaulin without `--workspace`, so it measured only the root package, which has no unit tests: `0.00% coverage, 0/754 lines covered` (run 36868485805). The `coverage` step passes `--workspace`, and sets `KATAGLYPHIS_REQUIRE_GPU=1` like the `test` step. It fails below 70 % of the workspace's lines, the vendored crates under `third_party/` and flutter_rust_bridge's `src/frb_generated.rs` left out (tarpaulin `--fail-under` and `--exclude-files`; the generated file was 1157 of `src`'s 1795 lines, none run by a Rust test, so `src` read 24 % with it and 67.6 % without on 2026-11-15; 72.9 % measured 2026-10-19, 71.0 % on 2026-10-20 once the GUI, which needs a window for most of its code, joined the figure); raise the floor as tests are added. Since 2026-10-15 it also grades each crate and `src/` against its own floor (`COVERAGE_FLOORS`, read by `scripts/linux/check-crate-coverage.py` from `cobertura.xml`), so the renderer's 90 % no longer carries telemetry's 8 %, and gives tarpaulin `--timeout 300` (its 60 s per test binary was too short on the Pi for an instrumented ObservANT). Each floor sits within 3 points of the figure the step prints; `--max-slack 3` prints a `warning: floor slack` line for a crate further above its floor, without failing, so raise that floor, and a crate with no floor fails the step.
- **Every module counts since 2026-10-20.** Tarpaulin leaves out every module behind `#[cfg(feature = …)]` (measured 2026-10-15, tarpaulin 0.37.5), so `crates/media` and `src/webcam_engine.rs` were absent from the report while their tests passed; with no features left they, the GUI and the burn demos are in it, each with a floor of its own.
- **A test binary must not see the distro's `libstdc++` first.** Tarpaulin puts every `rustc-link-search` path a build script prints on `LD_LIBRARY_PATH`; font-kit's fontconfig and freetype printed `/usr/lib/x86_64-linux-gnu`, its `libstdc++` (GCC 15) won over GCC 16.2's, and the chain ORT failed to `dlopen` there alone (2026-10-20), which `real_model.rs`'s backend check caught. The burn demos' plots use plotters' `ab_glyph` instead; a new system library linked through pkg-config brings the same trap back.
- **Mutation testing measures whether the tests notice a change** (2026-10-15). `cargo mutants -p kataglyphis_observant --file crates/observant/src/web.rs --file crates/observant/src/static_files.rs --jobs 1 -- --lib` (cargo-mutants 27.1.0, installed with `cargo install --locked cargo-mutants`) plants small bugs in ObservANT's code, one at a time; that command covers its network-facing code. Run it after changing the files below. Caught, of the mutants that compile:

  | Files | At first | After tests for each gap | The gaps | Left, and why |
  | --- | --- | --- | --- | --- |
  | `web.rs`, `static_files.rs` (163 mutants) | about 84 % | 142 of 145 | content types, the gzip cache, route resolution, peer CORS, the signalling proxy's leftover bytes, split and endless response heads, re-framing a refusal | 3 change nothing a client can see (HEAD and 304 bodies hyper drops anyway, a log line's throttle, `>` against `>=` at a 16 KiB limit) |
  | `motion.rs`, `presence.rs`, `status.rs`, the arrival logic (393 mutants) | about 92 % of the 379 that compile | 375 | exact box margins, the change and motion thresholds, an IoU of exactly 0.5, the visits file's fallback, the settings' limits, a stale frame at 21 s, the failure count | 4 only change a log line or a bound no test can see |
  | `camera.rs`, `config.rs`, `notify.rs` | about 90 % of 145 | 134 | a webcam by driver or bus, a format it can decode, rpicam-vid without a sensor, a configured `/dev/video` path, each config limit taken itself, a short webhook host | 11 read real camera devices, which no CI host has, or change a log line |
  | `clip.rs`, `frame.rs`, `stats.rs`, `output.rs`, `precompress.rs` | 376 of 412 | 400 | the privacy filter's margin, block mean and chroma planes, the box outline's exact pixels, the clip's frame counts, a pixel's own luma and chroma, and the stats sampler | of the 12, 9 only widen the privacy filter's whole-block shortcut, whose per-pixel path gives the same picture, and 3 change a log line or an encoder error no test can raise |
  | `capture.rs`, the capture loop around a real camera, and `capture/logic.rs`, its part that needs none (the weakest; one file until 2026-11-16) | 87 of 124 | 98 | the arrival photo's privacy filter (decoded, so a swapped filter fails too), the bus's error and end of stream, the restart pause and a zero width | needs a Pi camera, a V4L2 device or a loaded model, or changes a log line |

  `rust-mutants.yml` runs these commands on Sundays and by hand, one job per row (`bash scripts/linux/run-mutants.sh <network|arrival|camera|media|capture>` in the image), and uploads each group's `mutants.out`; it gates nothing, and until the image pins cargo-mutants (hub CON238) each job warns and runs nothing.

  The webhook tests fail after 10 s instead of hanging when no event comes, which a mutant that sends nothing first showed: each one cost the run its 300 s timeout. **Keep `--jobs 1` or give each job its own target dir**: two jobs sharing `CARGO_TARGET_DIR` overwrite each other's test binary and misreport, and so do two runs at once (a check run beside a long one reported caught mutants as missed on 2026-10-15), so each run gets a target dir of its own. The three arrival files take about 75 minutes on a Pi 5, a `--re` over the changed functions minutes. The capture loop's two files: `--file crates/observant/src/capture.rs --file crates/observant/src/capture/logic.rs`; `capture/logic.rs` alone grades `Visits`, `counted`, `Backoff` and the frame geometry without a camera.

## Latency on a quiet host

`bash scripts/linux/quality-report.sh --perf-host` times `kataglyphis_cli detect-latency` per CPU
provider and `chat` through `crates/text` in the CI image, against `perf/baseline-<cpu>.json`:
hub `docs/fleet-health.md` § perf-host. `detect-latency --image FILE --runs N` prints warm p50 and p95
as one JSON line, or `not_collected` for a provider the runtime cannot load. It is not run in CI, whose
hosted wall time varies too much to grade. On a host under 12 GB RAM the setup links the CLI with thin LTO and 16
codegen units: the release profile's fat-LTO link needs more than 6 GB, and that host's figures go to its
own CPU's baseline file.
