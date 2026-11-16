# Backlog

For the ANTfrastructure agentic loop (`shared/agentic-loop/`), once this repo adopts it
(see *Not adopted yet*). The file was once zero bytes, which read as "the protocol exists
and the backlog is empty"; it was neither.

## Protocol

- `- [ ]` actionable — the planner may pick it up
- `- [b]` blocked — skipped, and excluded from the pending count, so a
  backlog containing only blocked items still lets the planner run again
- `- [x]` completed — pruned on sight; the history lives in git

Entries are sorted by priority, and the executor takes the first `- [ ]` in file order:
**P0** broken or blocking, **P1** high value, **P2** medium, **P3** ideas, chores and blocked work.
The renderer's feature status per row is
[`crates/webgpu_renderer/docs/webgpu-renderer-roadmap.md`](crates/webgpu_renderer/docs/webgpu-renderer-roadmap.md);
its open work is here.

## P0 — broken or blocking

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

## P1 — high value

- [ ] **Diagnose the intermittent headless GPU-test SIGSEGV** (M). A full default-parallel
      workspace run at `38b1c597` crashed in the `occlusion` test binary after 7/11 cases
      (SIGSEGV 11, exit 101). The same ELF then passed default and serial runs and five
      further default repeats; source CI on `920ca1b7` passed. The crash remains unexplained.
      Next: capture the GPU backend/driver and all-thread stack or core, distinguish test
      resource lifetime from a driver fault, then add a regression for the proven cause.
      Do not serialize all tests or call it fixed solely because CI is green.
      Attempts: 2026-11-03, 100 runs of the `occlusion` binary without a crash (5 at a time, llvmpipe, 15 min bound).
      Since 2026-11-14 the Linux test step's JUnit record (`scripts/linux/cargo-test-junit.py`) names a binary
      that ended early, so the next crash in CI says which one.

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

## P2 — medium

- [ ] **lavapipe refuses the full Colosseum's traced scene** (M, ★★). lavapipe binds at most 128 MiB per
      storage buffer, and the full Colosseum's vertex buffer is larger, so `check`
      (`crates/webgpu_renderer/src/render/tracer/limits.rs`) ends the build in `TraceStatus::Failed` with the
      buffer named; desktop GPUs bind it and the browser takes Colosseum-web. Next, if CI or a software host
      should trace it: drop uv1 when no material samples it (24 bytes a vertex instead of 32) or split the
      vertices over two bindings. Guard: a limits test at the full Colosseum's counts.

- [ ] **Skins and morph targets are traced at their first pose** (M, ★★). `sync_animated_instances`
      (`crates/webgpu_renderer/src/render/forward/traced.rs`) moves rigid instances only; a skinned or morphed mesh keeps the pose the
      build saw, so its traced frame and shadow disagree with raster once it moves. Next: refit that mesh's
      BVH from the skinned positions on a pose change. Guard: `skinned_bar.gltf` posed away from its rest,
      traced against raster as `the_traced_scene_follows_an_animated_node` does for a rigid node.

- [ ] **The path tracer ignores punctual lights and a loaded HDR environment** (M, ★★). NEE samples the
      sun alone and a bounce that misses sees the procedural gradient (`shaders/tracer/traced.slang`,
      `miss_radiance`), so `KHR_lights_punctual` scenes and a bound `.hdr` light the raster frame only.
      Next: punctual lights in NEE, then the environment as the miss term. Guard: a lone card lit by a point
      light, traced against raster within a tolerance.

- [ ] **The deferred path has no anti-aliasing** (M, ★★). The G-buffer is one sample, as in the C++ engine, so
      the Colosseum's sub-pixel crowd aliases where forward's 4x MSAA does not (`crates/webgpu_renderer/tests/deferred.rs`; 6.4
      levels apart on average, `CHANGELOG.md`). Next: FXAA after the lighting pass, off with forward.
      Guard: the deferred-against-forward edge measurements in `crates/webgpu_renderer/tests/deferred.rs` shrink.

- [ ] **Clouds are dark and grainy in both renderers** (M, ★★). The shared estimator
      (`shaders/common/volumetric_clouds.slang`) has no ambient term, so a cloud is darker than the sky
      around it, and 8 steps through a slab seen at a grazing angle undersample the fine noise. Fixing it moves
      the C++ cloud goldens too, so change both renderers in one pin bump. Guard: a cloud's mean luminance
      against the sky behind it in `crates/webgpu_renderer/tests/clouds.rs` and the C++ golden.

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
      camera. On himbeere2 that link does not fit `pi-ci`'s 5 GB at all: rustc was killed linking
      `kataglyphis_cli` on 2026-11-15, and `CARGO_PROFILE_RELEASE_LTO=false
      CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16` built it; the setup needs that override, or a larger
      `PI_CI_MEMORY`, before a Pi baseline. Then first give the detector commands in `quality-report.toml` ObservANT's two
      threads without spinning, and record it on a quiet night as one `systemd-run` unit. If
      one new gated bench is wanted: preprocessing at 1280x720, the app's default size. No
      allocation-count tests.

- [ ] **Rust mutation testing is by hand** (M, ★★). Audit 2026-11-12. The coverage half is done (2026-11-15): the
      `coverage` step leaves `src/frb_generated.rs` out (1157 of `src`'s 1795 lines, never run by a Rust test), which
      took `src` from 24 % to 67.6 %, and its floor from 19 % to 65 %. The `cargo mutants` scores in `docs/testing.md`
      are from 2026-10-15; no workflow runs the tool, and the image does not carry it (hub CON238). If the owner accepts one more scheduled job, a
      weekly one runs the per-file commands of `docs/testing.md` with `--jobs 1 -- --lib` on
      the pure modules and uploads `mutants.out`. The five `crates/inference` recipes are in
      `scripts/linux/tests/ort-license-mutations.json` since 2026-11-15 (`consumer.oxidant-inference-*`, each
      proved to bite). Guard: the
      raised floor, and a `kind = "count"` artifact over `mutants.out/missed.txt` in
      `quality-report.toml`. The capture loop got six tests on 2026-11-15 (the real model, a late
      file, the stall, the re-probe, the engine's two end reasons): the first run after CON238 writes
      `capture.rs`'s new count into `docs/testing.md` and adds `src/session_slot.rs` to its file list.

- [ ] **No packaged ObservANT has answered 502 to a panicking question yet** (S, ★★). Since 2026-11-14 every
      ObservANT build uses its own profile (`[profile.observant]`, release with `panic = "unwind"`, owner decision of
      2026-11-13): OxidANT's producer helper (790f8bb) and OmniAccelerANT's three package paths (8108bd6). Left: the
      proof in the field. Done when a packaged service answers 502 to a question whose answer panics and keeps
      streaming (`crates/observant/src/ask.rs`, the poisoned-lock recovery and the failed-task reply).

### Quality queue

The top 5 of `logs/code-quality/queue.md` (`bash scripts/linux/quality-report.sh`), leaving out `doc-dupes-hub.allow`, which the owner keeps in every AGENTS.md; re-sync when it moves.

- [ ] `large-files.allow:3` budget 2031 (allow_budget_total)
- [ ] `rust-function-size.allow:15` budget 281 (allow_budget_total)
- [ ] `function-size.allow:6` budget 171 (allow_budget_total)
- [ ] `rust-function-size.allow:14` budget 171 (allow_budget_total)
- [ ] `function-size.allow:5` budget 156 (allow_budget_total)

## P3 — ideas, chores and blocked work

- [ ] **Decide whether the path tracer's bounce misses match the C++ sky** (S). On purpose, a bounce that misses
      gets the renderer's sky gradient times the ambient strength and the environment intensity, where the C++
      path tracer uses a fixed gradient at half intensity (roadmap, ray- and path-traced row). Owner decision;
      if it should match, the C++ side changes, since its raster has no ambient term to follow.

- [ ] **A new `set_instances` count waits for the next upload** (S). A count change flags
      `Tracer::instances_current` false and the traced scene keeps the old instances until a scene upload;
      the same count moves in place. Next: rebuild the top level on a count change.

- [ ] **Bloom glows through clouds** (S). The bright pass reads the HDR before the clouds, and the tonemap only
      scales it by the cloud's transmittance (`shaders/tonemap/tonemap.slang`), so a hidden sun's halo shows
      at a cloud's edge. Next: bloom after the cloud composite, if the cost is acceptable.

- [ ] **A one-pixel rim without clouds at silhouettes** (S). The march stops at the resolved depth, the nearest
      of the four MSAA samples (`shaders/depth_resolve/depth_resolve.slang`), so an edge pixel half sky gets no cloud.

- [ ] **egui text renders heavier natively than in the browser** (S). The native surface is sRGB
      (`surface_config` in `crates/webgpu_renderer/src/context.rs`), so egui's coverage blends in linear light;
      the browser canvas is not, so it blends in sRGB. Next: paint egui into a non-sRGB view of the frame.

- [ ] **egui's Style editor and inspection windows show tofu** (S). The studio fonts
      (`crates/webgpu_renderer/src/ui/fonts.rs`, `FontDefinitions::empty()`) carry no emoji, which those two debug windows
      (`crates/webgpu_renderer/src/ui/editor/windows.rs`) draw. Next: add egui's emoji face as the last fallback, inside the wasm budget.

- [ ] **Debug builds trace slowly** (S). The dev profile builds the renderer at opt-level 0 (`Cargo.toml` raises
      only dependencies), so building the full Colosseum's BVH in a debug viewer takes about half a minute.
      Owner decision: `[profile.dev.package.kataglyphis_webgpu_renderer] opt-level = 1`.

- [ ] **The tracer uploads in one frame and keeps its CPU arrays** (M). About 0.5 GB for the full Colosseum goes
      to the GPU in one frame (a hitch), and about 480 MB of CPU arrays stay for queries and top-level rebuilds.
      Next: spread the upload over frames; free the arrays a build no longer needs.

- [ ] **F5 leaves the post passes alone** (S). `reload_shaders` rebuilds forward, sky, deferred, clouds and both
      traced kernels; the tonemap (with the cloud composite), bloom, SSAO, histogram and depth-resolve WGSL keep
      the build's. Next: a `TonemapPass` reload in the same error scope first.

- [ ] **WARP crashes when the renderer tests run in parallel on windows-11-arm** (S). `docs/ci.md`, the arm64 lane's run job:
      `d3d10warp.dll` faults with `STATUS_ACCESS_VIOLATION`; serially all pass. Re-dispatch
      `windows-arm64-cross.yml` with `parallel-renderer-tests` after a runner image update.

- [ ] **A model run that hangs cannot be ended** (S, ★). A run in flight for over 20 s shows in `/healthz`, its
      boxes are dropped and a start in the app gives up after 10 s, but the run itself goes on: nothing calls
      `RunOptions::terminate`. Leave it until a GPU provider ships; on the CPU no hung run is on record. The four
      guards of 2026-11-13 are unit tests; add `capture.rs` and `src/session_slot.rs` to the `cargo mutants` file
      list in `docs/testing.md` at the next mutation run (75 minutes on a Pi 5, not run for this change).

- [ ] **`capture.rs` mixes the camera with pure logic** (M, ★). Audit 2026-11-12; its first half, `to_json` holding
      `detected` while it took other locks, is done (2026-11-15, a test that deadlocked on the old order). Left:
      split `crates/observant/src/capture.rs` into the part that needs a camera and the pure part (`Visits`,
      `counted`, `Backoff`, geometry), which helps the weakest mutation score; split `status.rs` (31 fields) when the
      next feature adds one. Guard: the tests move unchanged, and the file names in the `cargo mutants` commands of
      `docs/testing.md` change in the same commit.

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

- [b] **`/ask` shows no text until the whole answer is there** (L, ★★). Audit 2026-11-12:
      `Assistant::run` in `crates/observant/src/ask.rs` throws the pieces away and `web.rs` has
      `Body = Full<Bytes>`, so the Stream page waits up to 310 s behind one empty bubble.
      Blocked on the owner's decision: streaming the reply (the facts first, then the pieces,
      then a done line, with the JSON reply kept for clients that do not ask) changes the body
      type of the whole web module, the Dart client and their tests. It is not part of the
      timeout work (2026-11-15).

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
  from seeds 2 to 4. The Windows chain builds GenAI with llguidance too since `254b7ea`
  (`CHANGELOG.md`).

## Not adopted yet

The loop itself (config, runner wrappers, `scripts/agentic-loop/`) is not set up.
Copy-and-edit templates live in ANTfrastructure's `shared/agentic-loop/templates/`; a
consumer supplies this file, a config JSON, thin runner wrappers, and optionally
per-engine system prompts.
