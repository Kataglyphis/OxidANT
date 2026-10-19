# Tests

## The whole suite

Unit + integration + proptest fuzz + doc tests at the debug profile, as every Linux lane
runs them (`bash scripts/linux/ci-container-steps.sh test`, whose `TEST_FEATURES` these
are):

```bash
scripts/linux/fetch-models.sh
KATAGLYPHIS_REQUIRE_GPU=1 KATAGLYPHIS_REQUIRE_MODEL=1 KATAGLYPHIS_REQUIRE_GST=1 \
  cargo test --workspace --locked \
    --features 'kataglyphis_media?/gstreamer,kataglyphis_inference?/onnxruntime,oxidant/gstreamer,oxidant/onnxruntime'
```

The features reach the suites a default build compiles empty: `kataglyphis_media`'s
capture tests are `cfg(feature = "gstreamer")` and need GStreamer installed, the
inference crate's ORT-loader tests are `cfg(feature = "onnxruntime")`, and the root
package's own two compile `src/webcam_engine.rs` and the webcam API, which no test
compiled before they arrived on 2026-10-15. Why each member's feature takes the `?/`
form: [AGENTS.md § Feature combinations and their system dependencies](../AGENTS.md#feature-combinations-and-their-system-dependencies).

Each `KATAGLYPHIS_REQUIRE_*` turns a self-skip (no GPU, no model, a missing GStreamer
element) into a failure, since `cargo test` hides a passing test's `SKIP:` line.
`fetch-models.sh` puts the model the real-model tests need in place.

## Formatting and lints

CI gates on both, as hard failures. Run them before pushing:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked \
    --features 'kataglyphis_media?/gstreamer,kataglyphis_inference?/onnxruntime,oxidant/gstreamer,oxidant/onnxruntime' -- -D warnings
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

## Where the suites live

- Unit tests inside the workspace crates (`kataglyphis_telemetry`, `kataglyphis_inference`,
  and the renderer's own suite). The ESP32 link's run over an in-memory serial line
  (`Link::start`), so they need no board.
- Integration tests: `tests/integration/integration.rs`, which run the CLI and are
  compiled as `kataglyphis_cli`'s `integration` test (`crates/cli/tests/integration.rs`);
  `crates/media/tests/capture_test.rs` (a `videotestsrc` pipeline and camera
  enumeration); and the renderer's 18 binaries under `crates/webgpu_renderer/tests/`.
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
OpenGL fallback for machines without Vulkan/DX12. Run those tests on a desktop Windows
host, where the DLL exists:

```pwsh
$env:KATAGLYPHIS_REQUIRE_GPU = '1'; cargo test -p kataglyphis_webgpu_renderer --locked
```

CI splits the same way: `windows-x64.yml` builds the renderer's tests in the container and
runs them on the runner, a desktop Windows Server with `opengl32.dll`;
`windows-arm64-cross.yml` runs the whole workspace's tests on `windows-11-arm`. How:
[AGENTS.md § Continuous integration](../AGENTS.md#continuous-integration).
