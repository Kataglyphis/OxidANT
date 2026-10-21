# tract-linalg 0.23.8, patched for clang-cl (temporary)

This directory is the published `tract-linalg` 0.23.8 crate with one line of
`build.rs` changed. The root `Cargo.toml` routes `tract-linalg` here through
`[patch.crates-io]`.

## Why

The family builds every Windows C and C++ compile with the image's clang-cl, never
MSVC's `cl`. With `CC=clang-cl`, tract-linalg 0.23.8 fails its build script on
`x86_64-pc-windows-msvc`:

```
avx512vnni_mmm_i32_8x8_0_23_8.S:5:1: error: invalid instruction mnemonic '_text'
error: failed to run custom build command for `tract-linalg v0.23.8`
```

OmniAccelerANT's clang-cl Windows build hit it on 2026-10-21. OxidANT's own Windows
x64 lane stayed green only because cc-rs fell back to `cl` there; it sets
`CC=clang-cl` since (`scripts/windows/Invoke-WindowsLane.ps1`).

## What was applied

- **Source:** `https://static.crates.io/crates/tract-linalg/tract-linalg-0.23.8.crate`,
  sha256 `1cc37ab328b95cef8eef98324122e9f956fd3ca197f0892e09ff90fe495fa222`, the
  checksum of the crates.io index entry and of the `Cargo.lock` it replaced.
- **Change:** `build.rs` line 417,
  `if assembler_supports_avx512vnni() {` became
  `if !use_masm() && assembler_supports_avx512vnni() {`. On the MSVC target the
  AVX-512 VNNI kernels are skipped, as they already were under `cl`; the runtime
  dispatch falls back to the AVX2 i32 kernels.
- Every other file is as published, licence files included. `.gitattributes` keeps
  the directory's line ends as published on a `core.autocrlf` checkout.

Proof (2026-10-21, `:winamd64`, `CC=clang-cl`): `cargo build -p tract-linalg --target
x86_64-pc-windows-msvc` exits 101 on the published crate and 0 on this one.

## Upstream report (not filed yet; filing is the owner's call)

> **tract-linalg: the x86_64 MSVC build fails with clang-cl as `CC`
> (`invalid instruction mnemonic '_text'`)**
>
> On `x86_64-pc-windows-msvc`, `use_masm()` is true, so `preprocess_file` renders
> every x86_64 template in MASM syntax. The bulk kernels then go to `ml64.exe`, but
> the AVX-512 VNNI kernels (`avx512vnni_mmm_i32_{8x8,16x16}.S.j2`) go through
> `cc::Build`. With MSVC's `cl` the GAS probe `assembler_supports_avx512vnni()` fails,
> so the block is skipped. With `CC=clang-cl` the probe passes, and clang-cl is
> handed MASM text: `error: invalid instruction mnemonic '_text'`. The AMX and
> AVX-VNNI blocks below it already skip `os == "windows"`; the VNNI block has no such
> guard. Seen in 0.23.8 and 0.23.9 (`build.rs`, `if assembler_supports_avx512vnni()`).
>
> Fix: `if !use_masm() && assembler_supports_avx512vnni() {`. A fuller fix would
> render the two VNNI templates with `msvc = false` when the compiler is clang-cl, so
> that build keeps the kernels.
>
> Repro: `CC=clang-cl cargo build -p tract-linalg --target x86_64-pc-windows-msvc`
> on a Windows host with LLVM's clang-cl on `PATH`.

## Remove when

A tract release carries the fix. Then:

1. Delete this directory and its `[patch.crates-io]` entry in the root `Cargo.toml`,
   its `.gitattributes` line and its `.github/renovate.json` rule.
2. Move the tract family to that release.
3. Run the Windows x64 lane, which builds with `CC=clang-cl`.

**Moving tract without the fix:** tract's crates pin `tract-linalg` exactly, so a
lockfile at another version leaves this patch unused (cargo warns `Patch
tract-linalg v0.23.8 ... was not used`) and the Windows x64 lane fails again.
Re-vendor that version with the same line changed.
