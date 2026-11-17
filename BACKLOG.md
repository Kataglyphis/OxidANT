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

- [ ] **Detection quality has no labelled frame set** (M, ★★★). Audit 2026-11-12
      (not double-checked): `crates/inference/tests/fixtures` holds one photo, checked loosely
      (two cats, one at 0.8 or more). Since 2026-11-16 the visit rules replay five scenarios in
      the `test` step (`crates/observant/tests/scenarios`), synthesised from the hand
      observations in `docs/observant.md`; the owner replaces each with a `--record-scenario`
      run from the Pi. Next: a labelled set of 20 to 50 frames as a checksum-locked fetched file
      like `models.lock`, never tracked; the owner supplies own or free pictures. Guard: the frame
      set under `KATAGLYPHIS_REQUIRE_MODEL` with a budget (cats found, false boxes at 0.25) for
      ORT and tract, so a new model passes it before it reaches a camera.

## P2 — medium

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

- [ ] **Rust mutation scores wait for cargo-mutants in the image** (S, ★★). The owner accepted a weekly job
      (2026-11-16): `rust-mutants.yml` runs the five per-file groups of `docs/testing.md` on Sundays through
      `scripts/linux/run-mutants.sh` and uploads `mutants.out`, but warns and runs nothing while the image lacks the
      tool (hub CON238). The `cargo mutants` scores in `docs/testing.md` are from 2026-10-15. The first run after
      CON238 writes `capture.rs`'s and `capture/logic.rs`'s new counts (six capture tests since 2026-11-15) into
      `docs/testing.md`. Next: a group for the root crate's `src/session_slot.rs` (`-p` of the root package); a
      `kind = "count"` artifact over `mutants.out/missed.txt` in `quality-report.toml`. Done when a weekly run has
      uploaded all five groups and the table holds its counts.

- [ ] **No packaged ObservANT has answered 502 to a panicking question yet** (S, ★★). Since 2026-11-14 every
      ObservANT build uses its own profile (`[profile.observant]`, release with `panic = "unwind"`, owner decision of
      2026-11-13): OxidANT's producer helper (790f8bb) and OmniAccelerANT's three package paths (8108bd6). Left: the
      proof in the field. Done when a packaged service answers 502 to a question whose answer panics and keeps
      streaming (`crates/observant/src/ask.rs`, the poisoned-lock recovery and the failed-task reply).

### Quality queue

The top 5 of `logs/code-quality/queue.md` (`bash scripts/linux/quality-report.sh`), leaving out `doc-dupes-hub.allow`, which the owner keeps in every AGENTS.md; re-sync when it moves.

- [ ] `large-files.allow:3` budget 2031 (allow_budget_total); left 2026-11-16: `images/Rust.webp`'s 136 frames are
      lossy VP8 already, and a quarter of the size is a visibly worse hero image, the owner's call
- [ ] `rust-function-size.allow:13` budget 111 (allow_budget_total)
- [ ] `rust-function-size.allow:9` budget 108 (allow_budget_total)
- [ ] `rust-function-size.allow:12` budget 108 (allow_budget_total)
- [ ] `rust-function-size.allow:16` budget 107 (allow_budget_total)

## P3 — ideas, chores and blocked work

- [b] **An audit ignore waits for basisu to move to ruzstd 0.9.1** (S, ★). Since 2026-11-16 `.cargo/audit.toml`
      ignores RUSTSEC-2026-0344 (owner decision): `ruzstd` 0.7.3 has a decompression bomb, fixed in 0.9.1, and
      `basisu` 0.1.0, the only release, requires `^0.7` (upstream issue marcogomez/basisu#1). The renderer enables
      `basisu` without its `zstd` feature, so `ruzstd` is locked but not built, and cargo-deny never meets it.
      Blocked on a basisu release. Then: `cargo update -p basisu` and delete the ignore. Done when the ignore is gone.

- [ ] **Texture streaming past the mip tail** (L, ★). BeschleunigerBallett's "[P3] Texture streaming / bindless".
      Since 2026-11-16 the renderer streams material mips by screen-space need (roadmap, *Texture streaming*), a
      step copies the resident levels on the GPU (the GL backend keeps the CPU path for compressed textures), and a
      texture drops levels it has not needed for a while; the CPU chains fit a budget and rebuild from the scene's
      images past it. Open: (4) bindless material textures for scenes with thousands of materials. Measured blockers,
      2026-11-16, slangc 2026.17 in `:latest`: a WGSL target emits `Texture2D t[64]` as `array<texture_2d<f32>, 64>`,
      which WGSL forbids (naga takes texture arrays only as `binding_array<..>`), and refuses
      `NonUniformResourceIndex` outright, so `shaders/forward/forward.slang` cannot index an array as is. The work:
      (a) a bindless variant of the eight `forward.slang` entry points (`vs_main`/`fs_main`,
      `vs_shadow_masked`/`fs_shadow_masked`, `fs_gbuffer`, ...;
      group 0 bindings 3-12 become one texture and one sampler array in their own group plus a per-primitive
      material index in `PrimUniforms`), emitted as a second WGSL map entry and rewritten to `binding_array` by the
      shader build, with `scripts/check-slang-imports.py` and the generated-WGSL check covering it; (b) runtime
      detection of `TEXTURE_BINDING_ARRAY` and `SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING` in
      `context.rs` (never in a browser: WebGPU has neither), the per-primitive groups staying the fallback; (c) the
      main, G-buffer, masked-shadow and mirrored pipelines in both layouts; (d) streaming swapping one array slot
      per step instead of rebuilding each user's group 0 (`rebind_primitives`); (e) a GPU test that the two paths
      render identical frames. Guard: `tests/texture_streaming.rs`.

- [ ] **An amd64 crash of the GPU tests is unreproduced** (S). On 2026-10-19 the headless tests segfaulted now and
      then in parallel on the Ryzen 9950X dev host. The arm64 crash has its cause, llvmpipe at 256-bit vectors, which
      the tests now refuse ([testing.md](docs/testing.md#the-gpu-tests-must-render)); amd64 has none. When one dies,
      the JUnit record names the binary and its `ADAPTER:` line the width: loop that binary at `--test-threads 4`
      under `gdb -batch -ex run -ex "thread apply all bt"` and record the stack here.

- [ ] **WARP crashes when the renderer tests run in parallel on windows-11-arm** (S). `docs/ci.md`, the arm64 lane's run job:
      `d3d10warp.dll` faults with `STATUS_ACCESS_VIOLATION`; serially all pass. Re-dispatch
      `windows-arm64-cross.yml` with `parallel-renderer-tests` after a runner image update.

- [ ] **A model run that hangs cannot be ended** (S, ★). A run in flight for over 20 s shows in `/healthz`, its
      boxes are dropped and a start in the app gives up after 10 s, but the run itself goes on: nothing calls
      `RunOptions::terminate`. Leave it until a GPU provider ships; on the CPU no hung run is on record. The four
      guards of 2026-11-13 are unit tests; add `capture.rs`, `capture/logic.rs` and `src/session_slot.rs` to the `cargo mutants` file
      list in `docs/testing.md` at the next mutation run (75 minutes on a Pi 5, not run for this change).

- [ ] **`status.rs` holds 31 fields** (S, ★). Audit 2026-11-12; `capture.rs` is split since 2026-11-16 (the camera
      half and `capture/logic.rs`). Split `crates/observant/src/status.rs` when the next feature adds a field. Guard: the
      tests move unchanged, and the file names in the `cargo mutants` commands of `docs/testing.md` change in the same
      commit.

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
