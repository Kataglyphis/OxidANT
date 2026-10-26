# Getting started

## Prerequisites

A Rust toolchain, a C++ compiler and the submodule. The toolchain version the CI images
and every gate use is `RUST_VERSION` in ANTfrastructure's
[`linux/scripts/01-core/versions.env`](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/linux/scripts/01-core/versions.env)
(`third_party/ANTfrastructure/linux/scripts/01-core/versions.env` in a checkout); read it
there, since a number pinned here went stale. The C++ compiler is for `cxx`, which
`crates/cxx_bridge` builds (on Windows the Visual Studio C++ build tools, which the MSVC
Rust toolchain needs to link anyway). GStreamer's
development files and the chain-built ONNX Runtime come from the family images; on a bare
host: [AGENTS.md § Targets and their system dependencies](../AGENTS.md#targets-and-their-system-dependencies).

## Build

```bash
git clone --recurse-submodules --branch develop https://github.com/Kataglyphis/OxidANT.git
cd OxidANT
cargo build --workspace --locked
```

The three build profiles (dev, `profile` = release + debuginfo, `release` = fat LTO):
[AGENTS.md § 5](../AGENTS.md#5-build-run-test). What each target builds:
[crates.md § No features](crates.md#no-features-the-target-decides). Running what you built:
[running.md](running.md); testing it: [testing.md](testing.md).

## API docs

```bash
cargo doc --open
```

`docs.yml` publishes them to <https://rust.jonasheinle.de> on every push to `develop` or
`main`, through the hub's `build-docs.yml` with `builder: rustdoc`: ANTfrastructure's
`cargo_build_doc.sh`, which is `cargo doc --no-deps --workspace` (all 13 crates) plus the
family theme and `images/`, then the hub's `.htaccess` and its built-site link gate, whose
known exceptions are `docs/site-links.allow`. `bash scripts/linux/ci-container-steps.sh docs`
builds its API pages into `target/doc` locally, without the guides.
`bash scripts/linux/check-docs-site.sh` builds those same guides and runs the built-site link
gate locally in the family Linux image. The `oxidant` crate's front page there is
`README.md` itself (`#![doc = include_str!("../README.md")]` in `src/lib.rs`;
[AGENTS.md § 6](../AGENTS.md#6-docs-owned-by-this-repo) says what that demands of the
README). The guides in `docs/` are on that site too (`docs.yml`'s `guides`), each beside the
`oxidant` crate's page and listed atop every page's sidebar; a link into the repository
leads to GitHub.

## On a Windows checkout

A CRLF checkout breaks the container before anything runs, and a box without the MSVC C++
build tools links nothing:
[AGENTS.md § The trap a Windows checkout hits before anything runs](../AGENTS.md#the-trap-a-windows-checkout-hits-before-anything-runs)
and [§ Verifying locally on a Windows box](../AGENTS.md#verifying-locally-on-a-windows-box-no-msvc-required).
The Windows container build is [windows.md](windows.md).

## Linux containers locally (Rancher Desktop)

Any CI step runs as `bash scripts/linux/ci-container-steps.sh <step>` in the family Linux
image ([AGENTS.md § Continuous integration](../AGENTS.md#continuous-integration)). From
PowerShell, with `target/` and the cargo home on named volumes (2026-10-19, `fmt-clippy`
and `docs`):

```pwsh
$image = bash third_party/ANTfrastructure/linux/scripts/ci-image-ref.sh
nerdctl run --rm --platform linux/amd64 `
  -v "C:\path\to\OxidANT:/workspace" -w /workspace `
  --mount type=volume,source=ox-target,target=/workspace/target `
  --mount type=volume,source=ox-cargo-home,target=/tmp/cargo-home -e CARGO_HOME=/tmp/cargo-home `
  $image bash -lc 'bash scripts/linux/ci-container-steps.sh fmt-clippy'
```

- **`target/` is a volume, not a `CARGO_TARGET_DIR` elsewhere**: the `docs` step copies
  into `./target/doc`. The volume also keeps the Linux build off the checkout's MSVC `target/`.
- **A new volume belongs to root**; the image runs as uid 1001, so chown it once:
  `nerdctl run --rm --user root --entrypoint chown --mount type=volume,source=<vol>,target=/vol $image -R 1001:1001 /vol`.
- **The cargo steps need no git; `run-lint-gates.sh` does.** In a submodule checkout `.git`
  points outside the mount, so run that gate in Git Bash on the host.
- Shell scripts must be LF: [AGENTS.md § The trap](../AGENTS.md#the-trap-a-windows-checkout-hits-before-anything-runs).

`nerdctl` rather than `docker`, `:latest`, Git Bash's path mangling, `-v name:/path` and an
empty mount: ANTfrastructure's
[`rancher-desktop-linux-containers.md`](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/rancher-desktop-linux-containers.md).
