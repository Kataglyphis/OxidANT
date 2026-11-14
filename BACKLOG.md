# Backlog

For the ANTfrastructure agentic loop (`shared/agentic-loop/`), once this repo adopts it
(see *Not adopted yet*). The file was once zero bytes, which read as "the protocol exists
and the backlog is empty"; it was neither.

## Protocol

- `- [ ]` actionable — the planner may pick it up
- `- [b]` blocked — skipped, and excluded from the pending count, so a
  backlog containing only blocked items still lets the planner run again
- `- [x]` completed — pruned on sight; the history lives in git

## Open

- [ ] **Diagnose the intermittent headless GPU-test SIGSEGV** (M). A full default-parallel
      workspace run at `38b1c597` crashed in the `occlusion` test binary after 7/11 cases
      (SIGSEGV 11, exit 101). The same ELF then passed default and serial runs and five
      further default repeats; source CI on `920ca1b7` passed. The crash remains unexplained.
      Next: capture the GPU backend/driver and all-thread stack or core, distinguish test
      resource lifetime from a driver fault, then add a regression for the proven cause.
      Do not serialize all tests or call it fixed solely because CI is green.
      Attempts: 2026-11-03, 100 runs of the `occlusion` binary without a crash (5 at a time, llvmpipe, 15 min bound).

- [ ] **The NVIDIA packages have not run on a GPU they support** (S, ★★★). The Linux and Windows packages are built and
      passed on the CPU, and hosted runners have no GPU. Done on an RTX 30xx or newer when
      `compare_detector_parity.py` passes against the CPU and the profile shows the provider; the hub's
      `docs/gpu-artifact-receipts.md` has the commands. DirectML on an NVIDIA card is unproven too.
      The owner runs this on a machine with an NVIDIA GPU. No release until this and the Linux AMD check are done.

- [ ] **AMD's MIGraphX plugin has not run from this repo, on a card or in its image** (S, ★★★). Hub CON231 step 2.
      Since 2026-11-12 `migraphx` registers `libmigraphx-ep.so` by path (`crates/inference/src/ort_plugin.rs`) and the
      AMD package carries the module and its texts. Proved on arm64 only: the search for the module, the option keys,
      the registration guard, and that a missing or unloadable module and a module without a device fail a named request
      and leave `auto` the CPU. Seen in `:latest-rocm` on a runner without a card (candidate run 43345664993,
      2026-11-13): the package builds, the census passes with the launcher's library path, the packaged module is
      found beside the packaged ONNX Runtime and registered, AMD's factory answers `HIP failure 100: no ROCm-capable
      device is detected`, the request by name is refused with that text and `auto` runs on the CPU; the tarball and
      the DEB pass the package check. The run stops at the AppImage: the hub's `verify-ort-appimage-payload.sh` asks
      an AMD AppImage for the wheel's exact bytes, and the image strips its CPU core (hub CON256 i; the fix needs an
      x86-64 host). Never run: a session on the module, and anything on a card. Next: when the hub's verifier is
      fixed, move the pin and dispatch `linux-x64-amd-gpu.yml` again for the clean-host job; then, under native
      Ubuntu on the RX 9070 XT (under WSL even a plain HIP copy does not return, hub CON202), run
      `scripts/linux/profile-gpu-packages.sh <dir> migraphx crates/inference/tests/fixtures/two_cats.jpg`.
      The hub's `shared/python/verify_ort_profile.py`, which that parity run uses, expects `MIGraphX` since hub
      366b8259; the pin here is past it. Done when `compare_detector_parity.py` passes against the
      CPU for the tar, the deb and the AppImage and each run's profile has model operators on the provider `MIGraphX`;
      the hub's `docs/gpu-artifact-receipts.md` has the commands.

- [b] **CUDA still loads the old way; move it to the plugin row when the hub publishes its new NVIDIA image** (S, ★★).
      Owner, 2026-11-12: only AMD now. The hub's contract (`docs/consumer-image-contract.md`, *The CUDA plugin EP*)
      says a Rust consumer of the new `:latest-nvidia` registers `libonnxruntime_providers_cuda.so` by path under the
      name `CUDAExecutionProvider`: the module there has no `GetProvider` bridge, and `ort`'s `CUDA::default()`
      appends in the in-tree form (`SessionOptionsAppendExecutionProvider_CUDA_V2`). Not run against that image.
      Blocked: the hub records its runtime wrappers and receipts as open. Then: one more row in
      `crates/inference/src/ort_plugin.rs` (the name, the variable that names its root, the module's file name), that
      row in `providers_for_device` in place of `CUDA::default()`, `cuda_probe` taking the row's provider, and `ort`'s
      `cuda` feature dropped if nothing else needs it. The module lies beside the GPU ONNX Runtime library, in the
      image and in the NVIDIA package, which the search's second step finds; the hub names no root variable for it.

- [ ] **The capture and inference loops cannot run with a fake detector or a failing camera**
      (M, ★★★). Audit 2026-11-12: no test here runs `spawn_inference`
      (`crates/observant/src/capture.rs`; the test helper passes `detector: None`), the engine's
      two end reasons (`src/webcam_engine.rs`: the camera ended, 30 failed runs), the 10 s stall
      or the 30 s re-probe. `docs/testing.md` calls `capture.rs` the weakest file, 98 of 124
      mutants caught. Next: two tests that need nothing new (the real model on
      `image:two_cats.jpg` opens a visit; a missing image file is retried after the backoff and
      recovers). Then the smallest seam: each loop body takes the detector as a closure,
      `CaptureSession` gets a `#[doc(hidden)]` launch-line constructor, and `STALL_AFTER` and
      `REPROBE_EVERY` move into a `Timing` value on `Worker` with today's numbers as default.
      No public trait and no dev-only crate. Guard: the new tests in the `test` step, the
      `crates/observant` and `src` coverage floors raised to the new figures, and the new
      `capture.rs` count of `cargo mutants` written into `docs/testing.md`.

- [ ] **Detection quality has no regression set** (M, ★★★). Audit 2026-11-12
      (not double-checked): `crates/inference/tests/fixtures` holds one photo, checked loosely
      (two cats, one at 0.8 or more). The rules in `crates/observant/src/presence.rs` and
      `config.rs` (2 hits, 60 s return, score 0.25) come from hand observations on the Pi,
      written in `docs/observant.md`. Next: record what the model returned run by run for the
      known hard cases (the drying rack, a cat behind the sofa and back, the feeding hour, a
      dark room) as JSON lines under `crates/observant/tests/scenarios`, and replay them through
      `counted`, `Presence` and `Visits::record`, asserting the visits and stays. Later a
      labelled set of 20 to 50 frames as a checksum-locked fetched file like `models.lock`,
      never tracked; the owner supplies own or free pictures. Guard: the replays in the `test`
      step; the frame set under `KATAGLYPHIS_REQUIRE_MODEL` with a budget (cats found, false
      boxes at 0.25) for ORT and tract, so a new model or a retuned constant passes it before
      it reaches a camera.

- [ ] **A model run that hangs cannot be ended** (S, ★). A run in flight for over 20 s shows in `/healthz`, its
      boxes are dropped and a start in the app gives up after 10 s, but the run itself goes on: nothing calls
      `RunOptions::terminate`. Leave it until a GPU provider ships; on the CPU no hung run is on record. The four
      guards of 2026-11-13 are unit tests; add `capture.rs` and `src/session_slot.rs` to the `cargo mutants` file
      list in `docs/testing.md` at the next mutation run (75 minutes on a Pi 5, not run for this change).

- [ ] **Measure whether ORT's default thread count costs the Rust detector half its speed**
      (S, ★★). Audit 2026-11-12, from the recorded perf-host baselines of the four repos (same
      host, model and photo, 8 pinned CPUs): Rust 50.7 ms p50, C++ 30.3 ms, Python 24.1 ms, and
      Python at ORT's default 51.3 ms. `ort_backend.rs` sets intra-op threads only when
      `KATAGLYPHIS_ORT_THREADS` is above 0. The cause is inferred, not confirmed by a Rust run,
      and nothing shows that an unpinned host or a Pi is slower. Next, on the perf host: run
      `KATAGLYPHIS_ORT_THREADS=7 target/release/kataglyphis_cli detect-latency --image
      third_party/ModelANT/contracts/fixtures/two_cats.jpg --runs 30` once pinned and once
      unpinned. Keeping ORT's default was a written choice (2026-10-11), so the owner decides on
      the numbers. If it changes, only the unset case moves (the usable CPUs, at most 8);
      ObservANT's 2, the bench's 2 and every override stay, and `docs/running.md` and
      `docs/observant.md` change in the same commit. Guard: a perf-host row at ORT's default
      beside the shipped one with a ratio check, as OrchestrANT's `quality-report.toml` has.

- [ ] **No performance number is tracked for the Pi** (M, ★★). Audit 2026-11-12, which measured
      nothing: the only perf-host baseline in `perf/` is for a Ryzen 9 5950X, and the bench gate
      in `.github/workflows/reusable-linux.yml` fails only on a doubling. On a Pi 5 the model is
      about 99 % of a detection (261 ms against 1.6 ms and 1.8 ms for the two conversions,
      `CHANGELOG.md`), so frame copies are not worth chasing. The number that exists is the one
      `/healthz` serves; keeping a history of it is hub work (Home Assistant). Next: run the
      service one evening with `KATAGLYPHIS_ORT_DEVICE=xnnpack`, read `stats.model_ms` and write
      the result into `CHANGELOG.md`; on x86 XNNPACK is 3.7 times slower, on ARM nobody knows.
      A Cortex-A76 perf-host baseline only if it is still wanted: its setup builds the release
      CLI with fat LTO in the container and refuses a load above 2, a heavy job beside a live
      camera. Then first give the detector commands in `quality-report.toml` ObservANT's two
      threads without spinning, and record it on a quiet night as one `systemd-run` unit. If
      one new gated bench is wanted: preprocessing at 1280x720, the app's default size. No
      allocation-count tests.

- [ ] **The `src` coverage floor is 19 %, and Rust mutation testing is by hand** (M, ★★). Audit
      2026-11-12: `scripts/linux/ci-container-steps.sh` holds `src` to 19 % and excludes only
      `third_party/*`. `src/frb_generated.rs` is 2301 of the 5405 lines under `src`, but nobody
      has read its share of the coverage report. The `cargo mutants` scores in
      `docs/testing.md` are from 2026-10-15; no workflow runs the tool, and the image does not
      carry it. Next: read the per-file rates in the x64 lane's `cobertura.xml`; if the
      generated file is the drag, exclude it with a second `--exclude-files` and set the floor
      within 3 points of the new figure. Then, if the owner accepts one more scheduled job, a
      weekly one runs the per-file commands of `docs/testing.md` with `--jobs 1 -- --lib` on
      the pure modules and uploads `mutants.out`. Also (not double-checked):
      `scripts/linux/tests/ort-license-mutations.json` has no recipe for `crates/inference`; add
      five that run `cargo test -p kataglyphis_inference --lib` (swap red and blue, ignore the
      threshold, drop the finite check, drop the clamp, one scale for both axes). Guard: the
      raised floor, and a `kind = "count"` artifact over `mutants.out/missed.txt` in
      `quality-report.toml`.

- [ ] **The endpoint client has one 300 s budget for everything** (M, ★★). Audit 2026-11-12, not
      measured: `Endpoint::post` in `crates/text/src/endpoint.rs` sets only `timeout_global`, so
      the connect, the first byte and the whole stream share one clock, and a timeout arrives
      as `EndpointUnreachable`, like a refused port. `ANSWER_TIMEOUT` in
      `crates/observant/src/ask.rs` is 300 s too, and no config key changes either. A client
      that leaves still cancels within 20 ms, so the one `/ask` slot is held only while a
      client keeps waiting. Next: `timeout_connect` (about 5 s) and `timeout_per_read`
      (generous, about 120 s, it also covers the response head) beside the cap; the cap becomes
      a field ObservANT sets from a new `llm_timeout_s` key; ureq's timeout maps to a new
      `TextError` kind that `/ask` answers with 504. Guard: cases on the `silent_after` server
      in `crates/text/tests/endpoint.rs` with short limits, asserting the kind and an upper
      bound on the time, and one mutation recipe per timeout.

- [ ] **ObservANT's in-process model has no thread cap and reports no speed** (M, ★★). Audit
      2026-11-12, not measured: `load()` in `crates/text/src/genai.rs` sets no thread count, so
      GenAI uses its config's (ORT's default, one per core) beside a detector held to 2 threads
      for a measured reason (`docs/observant.md`). `/ask` drops the time to first token and the
      decode rate its result already has (`crates/observant/src/ask.rs`). A packaged board is
      not hit today: no package carries GenAI, so it answers through an endpoint. Next:
      `llm_threads`, default 2, applied at load (the overlay call if the pinned `ort_genai_c.h`
      has it, else the config rewrite the hub's `genai-model-threads.py` does); this is the
      consumer half of hub CON218. Return and log the two figures; they are the measurement any
      default needs. An idle unload for `llm_model_dir` waits for the owner: "the model stays
      loaded" is a written choice, and nothing is measured on the Pi. A bridge `text_unload`
      must land with OmniAccelerANT's regenerated bindings. Guard: a recorder test that `load()`
      passes the thread count, and a test for the figures in the reply.

- [ ] **A text failure loses its kind on the way out** (M, ★★). Audit 2026-11-12: `text_generate`
      in `src/api/text.rs` returns `Result<(), String>`, so the app cannot tell a missing model
      directory from a dead endpoint without matching English text. `refused` in
      `crates/observant/src/ask.rs` answers every `TextError` with 502, also a question with a
      NUL character, which is the caller's fault. Next: `TextError::kind()` returns a fixed
      word, with `every_error_names_its_kind` in `crates/text/src/lib.rs` extended; ObservANT
      maps the kind to a status (400 invalid request, 503 the model cannot load, 502 the
      endpoint) and keeps the text body. Then the bridge puts the kind at the front of its error
      string, so no generated binding moves, and Dart maps it to a translated sentence. A typed
      bridge error waits for a planned regeneration, `ContextExceeded` for a measurement of how
      often the playground hits it. Guard: the extended unit test and one ObservANT test per
      status.

- [ ] **ObservANT's JSON has no example files the app's tests read** (M, ★★). Audit 2026-11-12:
      `to_json` in `crates/observant/src/status.rs` builds `/healthz` from a `json!` literal of
      about 30 field names, and the Rust tests assert on Rust's own output. OmniAccelerANT's
      four Dart parsers are tested on hand-typed bodies and fall back to a default for every
      missing field, so a renamed field shows as "no cat, 0 visits". Only its on-request package
      test checks the real fields, with `jq`. Next: `crates/observant/tests/fixtures/api/` with
      `/healthz` (idle, a cat in view, not ok), `/visits`, `/settings`, `/ask` and `/summary`.
      A Rust test compares its output with each file after blanking `version`, the ages and
      `stats`, and an environment switch rewrites the files after an intended change.
      OmniAccelerANT's Dart tests and `web-smoke-server.js` then read them from this submodule.
      Guard: the Rust test here, and the Dart tests failing in the pin-bump commit when a field
      moves.

- [ ] **Every chat turn runs the whole conversation through the model again** (M, ★★). Audit
      2026-11-12 (not double-checked), not measured: `generate` in `crates/text/src/genai.rs`
      templates and encodes all messages and builds a new generator on every call, and
      `src/api/text.rs` caches the model only. So the time to first token should grow with the
      chat. Next: measure first. Replay a fixed five-turn chat on SmolLM2 and compare the time
      to first token of turn 1 and turn 5. Only if it matters: keep the last generator and its
      token ids with the cached model and append only the new tokens when the old ids are a
      prefix, after checking that the pinned `ort_genai_c.h` can append to a live generator.
      Guard: a `text_turns` command in `scripts/linux/perf-host.sh` under
      `quality-report.sh --perf-host`, and a tiny-model test that a turn 2 which extends turn 1
      gives the same ids as a fresh run.

- [ ] **Three camera pipelines, each with its own failure rules** (L, ★★). Audit 2026-11-12:
      `crates/media/src/capture.rs` (the Flutter engine), `crates/observant/src/capture.rs` and
      `crates/gui/src/gui_wgpu/pipeline.rs` each open a camera and keep the newest frame. Only
      ObservANT notices a stall: the engine's loop continues on a timeout
      (`src/webcam_engine.rs`), and the wgpu GUI never reads its pipeline bus, the bug fixed in
      `crates/media` on 2026-11-11. The three loops differ on purpose, so no shared worker.
      Next, two steps. `kataglyphis_media` stamps the last frame time in the appsink callback
      and fails the slot with `no frame for N s`, tested with a launch line that delivers
      nothing. Then the wgpu GUI uses `CaptureSession` (`CaptureConfig` learns "size 0 means the
      camera's own size"), which deletes the third pipeline and gives the GUI its errors.
      ObservANT's pipeline stays in its crate. Guard: the media tests for end, error and stall,
      and a source-reading test that only `crates/media` and `crates/observant` build an
      appsink.

- [ ] **Three small silent fallbacks around a frame's size** (S, ★). Audit 2026-11-12. The capture's own case
      is done (2026-11-14: `exact_frame` in `crates/observant/src/capture.rs` ends the capture with both sizes
      when the bytes are not one frame). Left: mark the 640x640 default in the load log (`ort_backend.rs`,
      `tract_backend.rs`), make an output shorter than its shape an error (`postprocess.rs`), and pass the cause
      to `slot.fail` in the media appsink callback. Guard: one unit test per item.

- [ ] **The "cancelled while the model loaded" check is written twice** (S, ★). Audit
      2026-11-12: a backend clears the cancel token as a run starts, so each caller that loads
      a model first has its own guard: `Assistant::run` in `crates/observant/src/ask.rs` and
      `after_load` in `src/api/text.rs`, both tested. No test closes a socket in the middle of
      an answer, though the docs promise that a client that leaves cancels it. Next: move
      `after_load` into `kataglyphis_text` as a public function both call, with its recipe in
      `scripts/linux/tests/ort-license-mutations.json`. Add one `web.rs` test: post `/ask`
      against the held stub, close the socket, expect the stub to see the cancel within a
      second. A session type only when idle unload and timeouts need an owner.

- [ ] **Small text rules are retyped in each caller** (S, ★). Audit 2026-11-12
      (not double-checked): provider names are parsed in `crates/observant/src/ask.rs` (an
      unknown name becomes the CPU; only `config.rs` stops it), in `src/api/text.rs` (an unknown
      name is an error) and in `crates/cli/src/text.rs`. Finish-reason names are typed in
      `ask.rs` and `src/api/text.rs`, the default options in `crates/text/src/lib.rs` and
      `src/api/text.rs`. Next: one `Provider` parser in `kataglyphis_text` that refuses an
      unknown name with `ProviderUnavailable`, `FinishReason::as_str`, and
      `text_default_options` built from `GenerationOptions::default()`. Guard: unit tests in
      `crates/text` and one recipe (mapping an unknown provider to the CPU must fail).

- [ ] **`to_json` holds one lock while it takes others** (M, ★). Audit 2026-11-12: in
      `crates/observant/src/status.rs`, `to_json` keeps the `detected` guard while it locks
      `clips` and, through `health()`, `last_frame`, `last_run` and `model_error`. The order is
      right today, by convention only; nothing is broken. Next: copy the needed values out of
      `detected` and drop the guard first, with a test that holds `clips` on another thread and
      expects `to_json` to return. Then split only `capture.rs` into the part that needs a
      camera and the pure part (`Visits`, `counted`, `Backoff`, geometry), which helps the
      weakest mutation score; split `status.rs` (31 fields) when the next feature adds one.
      Guard: the tests move unchanged, and the file names in the `cargo mutants` commands of
      `docs/testing.md` change in the same commit.

- [b] **RVV `MatMulNBits` segfaults on a 4-bit model in the chain's ORT 1.31.0** (M, ★★). Crash:
      `kataglyphis_text --test contract`, `signal: 11, SIGSEGV`, run 41463196239 and locally on
      image `7d4d4129`. Under QEMU, intra-op workers fault in `libonnxruntime.so.1.31.0` at vaddr
      `0x150150c`, `flw fa5, 0x0(s10)`, in stripped code matching `SQ4BitGemmKernel_CompInt8_Impl`, with
      `si_addr = RangeStartN * 18 * 4` (for example `0x1f80`), so `QuantBScale` is null. The
      reachable cause, from the v1.31.0 source: `UseSharedPrePackedBuffers` narrows
      `shared_packed_b_finalized` under `MLAS_TARGET_ARM64` while `PrePack` uses
      `!MLAS_TARGET_AMD64_IX86`, so riscv64 sets `scales_are_packed_`. ORT `main` carries the same
      guard (checked 2026-11-02). `RUST_MIN_STACK`, `QEMU_STACK_SIZE` and `vlen=256/512` still crash;
      `ORT_MLAS_RISCV_FORCE_SCALAR=1` passes with the exact ids. Fix: a hub ORT patch or an upstream
      fix, which needs an image chain; then drop the scalar run from `riscv64-test`. Until then a
      riscv64 process that loads a 4-bit `accuracy_level=4` model on RVV crashes: the override covers
      this test, not a shipped riscv64 binary. Real silicon is untested, and the guard theory has no
      patched-build proof yet.

- [b] **Accept DNNL on amd64 once `:latest` ships oneDNN** (S). The chain ORT's
      `libonnxruntime_providers_dnnl.so` needs `libdnnl.so.3`, which the image lacks, so
      `KATAGLYPHIS_ORT_DEVICE=dnnl` is `ProviderUnavailable` on amd64 too. Blocked on hub
      BACKLOG CON191 (an image chain). Then `cpu_accelerators.rs` and `check-backend-parity.sh`
      require DNNL to run and match the CPU provider on amd64; nothing here changes.

- [ ] **OmniAccelerANT still packages ObservANT with `--release`** (S, ★★). The service has its own profile
      since 2026-11-13 (`[profile.observant]`: release with `panic = "unwind"`, owner decision), so the
      poisoned-lock recovery and the failed-task reply in `crates/observant/src/ask.rs` can run in the field.
      Nothing builds with it yet: OmniAccelerANT's `package-observant.sh` runs `cargo build --release` and
      takes `target/release/kataglyphis_observant`. Done when that script builds `--profile observant` at a
      pin that has the profile, and a packaged service answers 502 to a question whose answer panics.

- [ ] **The renderer, the GUI and the demos are outside the function-size gate** (S, ★). Since 2026-11-13
      (owner decision) `crates/observant`, `crates/inference`, `crates/media` and `src` fail a new
      function over 80 lines (`#![warn(clippy::too_many_lines)]`, `clippy.toml`) and hold today's four
      to `rust-function-size.allow` (`docs/testing.md` § Formatting and lints). Left: the other crates,
      about 30 functions, most of them the renderer's long passes. Next: one crate per change, its root
      gets the attribute, its offenders an `#[expect]` and a row, and `check-rust-function-sizes.sh`
      its directory in `scopes`.

- [b] **`/ask` shows no text until the whole answer is there** (L, ★★). Audit 2026-11-12:
      `Assistant::run` in `crates/observant/src/ask.rs` throws the pieces away and `web.rs` has
      `Body = Full<Bytes>`, so the Stream page waits up to 310 s behind one empty bubble.
      Blocked on the owner's decision: streaming the reply (the facts first, then the pieces,
      then a done line, with the JSON reply kept for clients that do not ask) changes the body
      type of the whole web module, the Dart client and their tests. It is not part of the
      timeout entry above.

- [b] **A push type-checks Rust for the host target only** (L, ★★). Audit 2026-11-12: the
      `fmt-clippy` step lints the host target, and no workflow or step calls
      `scripts/linux/clippy-windows.sh`. Code that fails on Windows alone is first compiled when
      a commit asks for the Windows lane; the recorded case is `I420Layout`, a struct nothing
      used on Windows (run 38496162066). Of 43 commits to the Windows lane's paths since
      2026-10-16, 24 carried no Windows token; each was built by a later commit, after about a
      day in the median, so the cost is a late find. Blocked on the hub image: the script
      installs mingw and adds the `x86_64-pc-windows-gnu` target as root, and the lane
      container runs as uid 1001. Once the image carries both: one `clippy-windows` step for
      `kataglyphis_observant`, `kataglyphis_text` and the root package, with the added minutes
      measured (about 5 a package, `docs/observant.md`) before it runs on every push.

## Recorded probes

- **llguidance accepts the hub tiny model's tokenizer** (GenAI 0.17.0 in `:latest`, index
  `7d4d4129`, amd64). `GeneratorParams.set_guidance("json_schema", ...)` on the tiny model decodes
  `abc` under
  `{"type":"object","properties":{"a":{"type":"string","maxLength":3}},"required":["a"],"additionalProperties":false,"x-guidance":{"whitespace_flexible":false}}`
  to the ids `[123, 34, 97, 34, 58, 34, 35, 36, 37, 34, 125, 256]`, `{"a":"#$%"}` then the end
  token 256. A schema that allows whitespace makes the successor rule loop on `\t\n` until the
  budget ends, so the schema test keeps the tight form. No SmolLM2-135M fallback is needed for
  schema tests; SmolLM2-135M runs the ModelANT contract (`crates/text/tests/contract.rs`).
  Sampling at temperature 1 over the full vocabulary gives seed 1 a different 24-token answer
  from seeds 2 to 4. The Windows chain builds GenAI without llguidance, where the generator
  refuses a schema as `TextError::JsonSchemaUnsupported`. That no longer holds: the Windows
  chain builds GenAI with llguidance now (`CHANGELOG.md`, `254b7ea`).

## Not adopted yet

The loop itself (config, runner wrappers, `scripts/agentic-loop/`) is not set up.
Copy-and-edit templates live in ANTfrastructure's `shared/agentic-loop/templates/`; a
consumer supplies this file, a config JSON, thin runner wrappers, and optionally
per-engine system prompts.

## Quality queue

The top 5 of `logs/code-quality/queue.md` (`bash scripts/linux/quality-report.sh`), leaving out `doc-dupes-hub.allow`, which the owner keeps in every AGENTS.md; re-sync when it moves.

- [ ] `large-files.allow:3` budget 2031 (allow_budget_total)
- [ ] `function-size.allow:6` budget 171 (allow_budget_total)
- [ ] `function-size.allow:5` budget 156 (allow_budget_total)
- [ ] `function-size.allow:4` budget 127 (allow_budget_total)
- [ ] `function-size.allow:3` budget 102 (allow_budget_total)
