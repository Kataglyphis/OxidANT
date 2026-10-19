# Getting started

## Prerequisites

You need a Rust toolchain, a C++ compiler and the submodule. The toolchain version
the CI images and every gate use is `RUST_VERSION` in ANTfrastructure's
[`linux/scripts/01-core/versions.env`](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/linux/scripts/01-core/versions.env)
(`third_party/ANTfrastructure/linux/scripts/01-core/versions.env` in a checkout) — read
it there rather than pinning a number here, which is how the last one went stale. The
C++ compiler is for the root package's `build.rs`, which compiles the cxx bridge in
`src/native_only.rs` (on Windows, the Visual Studio C++ build tools, which the MSVC Rust
toolchain needs to link anyway). Nothing else is needed for a default-feature build.

## Build

```bash
git clone --recurse-submodules --branch develop https://github.com/Kataglyphis/OxidANT.git
cd OxidANT
cargo build --workspace --locked
```

The three build profiles (dev, `profile` = release + debuginfo, `release` = fat LTO) are
in [AGENTS.md § 5](../AGENTS.md#5-build-run-test). Every optional feature and the
system libraries it needs: [crates.md § Features](crates.md#features). Running what you
built: [running.md](running.md); testing it: [testing.md](testing.md).

## API docs

```bash
cargo doc --open
```

The Linux x64 lane publishes them from the default branch to
<https://rust.jonasheinle.de>: its *Create docs* step runs ANTfrastructure's
`cargo_build_doc.sh` (`bash scripts/linux/ci-container-steps.sh docs`), which is
`cargo doc --no-deps` for the root package plus the family theme and `images/`. The
`oxidant` crate's front page there is `README.md` itself (`#![doc = include_str!("../README.md")]`
in `src/lib.rs`), which is why the README links with absolute URLs. The guides in
`docs/` are not on that site; they are read on GitHub.

## On a Windows checkout

Two traps hit before anything runs (a CRLF checkout, an image Rust older than its pin),
and a box without the MSVC C++ build tools links nothing:
[AGENTS.md § The two traps a Windows checkout hits before anything runs](../AGENTS.md#the-two-traps-a-windows-checkout-hits-before-anything-runs)
and [§ Verifying locally on a Windows box](../AGENTS.md#verifying-locally-on-a-windows-box-no-msvc-required).
The Windows container build is [windows.md](windows.md).

## Linux containers locally (Rancher Desktop)

The Linux image is **always** the family CI one, in CI and locally — the same reference
the workflows inherit, printed by `bash third_party/ANTfrastructure/linux/scripts/ci-image-ref.sh`
and owned by that submodule's `versions.env`. The command below asks for it rather than
repeating it, so it cannot go stale on a tag bump. Rancher Desktop defaults to the
**containerd** engine, so use `nerdctl`, not `docker` — and from Git Bash disable path
mangling or the mount argument is destroyed.

```pwsh
$env:MSYS_NO_PATHCONV=1; $env:MSYS2_ARG_CONV_EXCL='*'
$image = bash third_party/ANTfrastructure/linux/scripts/ci-image-ref.sh
rdctl shell nerdctl --namespace default run --rm --user root `
  -v kata-cargo-cache:/cargo-cache `
  -v /mnt/d/path/to/repo:/workspace -w /workspace `
  $image `
  bash -lc 'export CARGO_HOME=/cargo-cache; bash third_party/ANTfrastructure/linux/scripts/02-toolchain/rust/cargo_release.sh'
```

The named volume keeps the cargo registry across runs. Any single CI step runs the same
way with `bash scripts/linux/ci-container-steps.sh <step>` in place of the driver
([AGENTS.md § Continuous integration](../AGENTS.md#continuous-integration)). **Shell
scripts must be LF** (verified 2026-08-07): a checkout older than the `.gitattributes`
rule keeps CRLF and bash dies on `set: pipefail\r: invalid option name`; the one-time
fix is in the AGENTS.md section above.

The procedure itself is ANTfrastructure's:
[`rancher-desktop-linux-containers.md`](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/rancher-desktop-linux-containers.md),
its cargo cache in § *Persisting the cargo cache*.
