# Tests

## The whole suite

Run the complete suite (unit + integration + proptest fuzz + doc tests) at the debug
profile, as every Linux lane does (`bash scripts/linux/ci-container-steps.sh test`, whose
`TEST_FEATURES` these are):

```bash
scripts/linux/fetch-models.sh
KATAGLYPHIS_REQUIRE_GPU=1 KATAGLYPHIS_REQUIRE_MODEL=1 KATAGLYPHIS_REQUIRE_GST=1 \
  cargo test --workspace --locked \
    --features 'kataglyphis_media?/gstreamer,kataglyphis_inference?/onnxruntime,oxidant/gstreamer,oxidant/onnxruntime'
```

The features reach the suites a default build compiles empty: `kataglyphis_media`'s
capture tests are `cfg(feature = "gstreamer")` and need GStreamer installed, the
inference crate's ORT-loader tests are `cfg(feature = "onnxruntime")`, and the root
package's own two compile `src/webcam_engine.rs` and the webcam API. The `?/` form names
each member's own feature. Plain `kataglyphis_media/gstreamer` would also switch on the
optional `kataglyphis_media` dependency of `kataglyphis_gui` and the root package,
because with `--workspace` cargo applies `dep/feature` to every selected package.

Each `KATAGLYPHIS_REQUIRE_*` turns a self-skip (no GPU, no model, a missing GStreamer
element) into a failure; `fetch-models.sh` puts the model the real-model tests need in
place. What each one guards: [AGENTS.md § 5](../AGENTS.md#5-build-run-test).

## Formatting and lints

CI additionally gates on formatting and lints, both as hard failures. Run them before
pushing:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked \
    --features 'kataglyphis_media?/gstreamer,kataglyphis_inference?/onnxruntime,oxidant/gstreamer,oxidant/onnxruntime' -- -D warnings
```

That is the scope of the lanes' `fmt-clippy` step (`CARGO_CLIPPY_ARGS` in
`scripts/linux/ci-container-steps.sh`). The shell, workflow, secret and config gates are
`bash scripts/linux/run-lint-gates.sh`.

## The GPU tests must render

The `crates/webgpu_renderer` headless golden tests need a GPU adapter and **silently
skip without one**. `KATAGLYPHIS_REQUIRE_GPU=1` turns a missing adapter into a failure,
so a green run actually means they rendered. Every CI lane that renders sets it: the
Linux lanes render on the image's software Vulkan device (lavapipe), the Windows lanes on
the runners' software Direct3D 12 adapter (WARP). Each GPU test binary prints one
`ADAPTER:` line to stderr when it first gets a device (`GpuContext::headless_or_skip()`),
naming the adapter and whether it offers BC textures and timestamp queries; every CI
adapter offers both, so under `KATAGLYPHIS_REQUIRE_GPU` the BC-upload and GPU-timing
tests fail instead of skipping without them.

## Where the suites live

- Unit tests inside the workspace crates (`kataglyphis_telemetry`, `kataglyphis_inference`,
  and the renderer's own suite).
- Integration tests: `tests/integration/integration.rs`, which run the CLI and are
  compiled as `kataglyphis_cli`'s `integration` test (`crates/cli/tests/integration.rs`);
  `crates/media/tests/capture_test.rs` (a `videotestsrc` pipeline and camera
  enumeration); and the renderer's 18 binaries under `crates/webgpu_renderer/tests/`.
- Property-based (fuzz) tests: [proptest](https://proptest-rs.github.io/proptest/)
  properties on ObservANT's network-facing parsers (Range header, query, Host/Origin
  check, request path, camera spec, config file), 256 random inputs per property and run,
  in each module's `properties` tests. There is no separate `cargo-fuzz`/libFuzzer setup.

Benchmarks, the detector's real-model test, the coverage floors and mutation testing are
in [AGENTS.md § 5](../AGENTS.md#5-build-run-test) and
[§ Continuous integration](../AGENTS.md#continuous-integration); the riscv64 lane's
QEMU run in [§ The riscv64 lane](../AGENTS.md#the-riscv64-lane).

## On Windows the renderer's tests run outside the container

Any of its test binaries links wgpu, and wgpu's `gles` backend makes the executable
import `opengl32.dll` at load time; Windows Server Core does not ship that DLL, so the
process dies with `0xc0000135` (`STATUS_DLL_NOT_FOUND`) before `main`. The loader
resolves that import, so no runtime flag avoids it — and letting it run turned the entire
`cargo test --workspace` into a crash with no results. The `gles` feature is kept
deliberately: it is the OpenGL fallback for machines without Vulkan/DX12. Run those tests
on a desktop Windows host, where the DLL exists:

```pwsh
$env:KATAGLYPHIS_REQUIRE_GPU = '1'; cargo test -p kataglyphis_webgpu_renderer --locked
```

That is the CI split too. In `windows-x64.yml` the container half (`Invoke-WindowsLane.ps1`,
which starts with `Invoke-DebugTests.ps1`) runs the workspace tests except the renderer's,
then **builds** the renderer's lib and integration tests and lists the executables in
`target\host-tests\tests.json`. The lane's `host-command` runs them on the runner, a
desktop Windows Server with `opengl32.dll`, through `Invoke-HostTests.ps1` and the hub's
`Invoke-StagedTests.ps1`. It fails if the list, a binary or a passing test is missing, so
it can never report green over nothing. `windows-arm64-cross.yml` builds the whole
workspace's tests for arm64 (`Stage-CrossTests.ps1`) and runs them all on
`windows-11-arm` with the same runner script.
