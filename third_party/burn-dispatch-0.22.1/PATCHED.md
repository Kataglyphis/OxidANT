# burn-dispatch 0.22.1, patched to omit the unused Torch backend

This directory is the published `burn-dispatch` 0.22.1 crate with one backend dependency
removed. The root `[patch.crates-io]` entry selects it for the Burn demos, which use the Flex
backend and autodiff. No workspace member enables Burn's deprecated `tch` feature.

## Why

Cargo locks optional dependency paths even when the workspace never activates them. Upstream
`burn-dispatch` includes optional `burn-tch`, whose `tch`/`torch-sys` dependencies require
`zip 0.6.6`. ZIP's old Zstd branch locks `zstd-safe 5.0.2` and `zstd-sys 2.1.1`, reported by
RustSec as RUSTSEC-2026-0403 and RUSTSEC-2026-0410. `zstd-safe 8.2.0` fixes the first issue,
but `zip 0.6.6` requires `zstd 0.11`, which requires `zstd-safe 5`; Cargo proves the version
conflict. Existing audit ignores for ZIP and `webpki-roots` also came only from this path.

## What changed

- **Source:** `https://static.crates.io/crates/burn-dispatch/burn-dispatch-0.22.1.crate`,
  SHA-256 `64d255d7609901f7aaf2138b394a6fe8dd8c7b526d1ed2736ba2c643944028fa`, matching
  the crates.io registry checksum. The upstream tag's MIT and Apache-2.0 licenses
  accompany the copy.
- **Manifest:** `Cargo.toml` drops the optional `burn-tch` dependency and its default, std,
  tracing feature forwarding. The named `tch` feature remains for upstream feature resolution
  but has no backend dependency.
- **Source:** `src/lib.rs` raises a compile error if `tch` is selected, so a future caller
  cannot silently use an unavailable backend. All other Rust source bytes remain upstream.
- **Lockfile:** `cargo update --offline -p burn --precise 0.22.1` drops `burn-tch`, `tch`,
  `torch-sys`, `zip`, `zstd`, `zstd-safe`, `zstd-sys` and their unused closure. The four
  ZIP/`webpki-roots` audit ignores are removed; no new exception is added.

The root pins `burn = "=0.22.1"` and Renovate requires review of its next update. Update the
Burn family, this source copy and its checksum together before changing the pin. The CI
lock-graph test and the local Renovate wrapper reject a reintroduced Torch/Zstd chain.

## Remove when

An upstream Burn release no longer locks this optional Torch chain, or all its dependencies
have supported, advisory-free versions. Then remove this directory and its root patch,
`.gitattributes` rule and Renovate exclusion; update Burn with Cargo and rerun the workspace
tests, `cargo audit`, `cargo deny`, and the Windows/Linux lanes.
