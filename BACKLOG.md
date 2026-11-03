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

- [ ] **LAI-O4: ObservANT's `/ask` and a daily summary over `/visits`** (M, ★★). Hub language-AI
      milestone 4 (`docs/language-ai.md`).
- [ ] **LAI-O5: embeddings in OxidANT** (M, ★★). GenAI's embeddings through `crates/text`; hub
      language-AI milestone 2.
- [ ] Drop the vendored `third_party/egui-winit-0.36.2` and its
      `[patch.crates-io]` entry, then move the egui family to 0.37.
      Steps: `third_party/egui-winit-0.36.2/PATCHED.md`.
      Read 2026-11-02: crates.io's newest `egui` and `egui-winit` are 0.37.0, whose tag contains emilk/egui#8516.

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

- [ ] **The Linux AMD packages have not run on a GPU** (S, ★★★). They are built and passed on the CPU, and hosted runners
      have no GPU. Under WSL on the RX 9070 XT even a plain HIP copy does not return (hub CON202), so this takes native
      Linux: the owner runs it under Ubuntu on that machine. Done when `compare_detector_parity.py` passes against the
      CPU and the profile shows MIGraphX; the hub's `docs/gpu-artifact-receipts.md` has the commands.

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
  refuses a schema as `TextError::JsonSchemaUnsupported`.

## Not adopted yet

The loop itself (config, runner wrappers, `scripts/agentic-loop/`) is not set up.
Copy-and-edit templates live in ANTfrastructure's `shared/agentic-loop/templates/`; a
consumer supplies this file, a config JSON, thin runner wrappers, and optionally
per-engine system prompts.

## Quality queue

The top 5 of `logs/code-quality/queue.md` (`bash scripts/linux/quality-report.sh`); re-sync when it moves.

- [ ] `large-files.allow:3` budget 2031 (allow_budget_total)
- [ ] `function-size.allow:7` budget 171 (allow_budget_total)
- [ ] `function-size.allow:6` budget 156 (allow_budget_total)
- [ ] `function-size.allow:4` budget 127 (allow_budget_total)
- [ ] `function-size.allow:3` budget 102 (allow_budget_total)
