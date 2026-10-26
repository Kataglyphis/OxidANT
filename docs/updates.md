# Dependency updates

Crate versions live in `Cargo.toml`/`Cargo.lock`. The `third_party/ANTfrastructure`
gitlink is a dependency too, and the one that silently changes every gate
([AGENTS.md § 3](../AGENTS.md#3-critical-invariant-submodule-pins)).

## What is behind: Renovate as a local CLI

Renovate run locally, not a bot, answers what is behind: the submodule gitlink and the
workspace crates. **A run only reports; `--apply` only when the owner asked for it**
([AGENTS.md § Dependency upgrades](../AGENTS.md#dependency-upgrades)).

```bash
bash scripts/linux/renovate-local.sh                    # report: every manager the tree has
bash scripts/linux/renovate-local.sh --managers cargo   # only the workspace crates
bash scripts/linux/renovate-local.sh --apply --dry-run  # the plan
bash scripts/linux/renovate-local.sh --apply            # gitlink, manifests and Cargo.lock
```

Here `--apply` moves both direct gitlinks to each remote's latest default-branch commit,
regardless of `.gitmodules branch =`. It initializes a missing checkout, rewrites
`Cargo.toml` values and refreshes `Cargo.lock`; it stages and commits nothing.
Report and dry-run do not clone, so they need the hub checkout already initialized.
`.github/renovate.json` skips Cargo scans inside the three documented patched crates under
`third_party/`: `egui-winit-0.36.2`, `tract-linalg-0.23.8`, and `burn-dispatch-0.22.1`.
None is a submodule. Dependabot is off (owner, 2026-10-20): this is the only
dependency watch.

The `burn-dispatch` patch keeps the deprecated Torch backend unavailable. Burn demos use Flex;
the old optional Torch path pulled `zip 0.6.6` and vulnerable `zstd-safe`/`zstd-sys` into
`Cargo.lock` even though no workspace feature selected them. Cargo audit checks the entire
lockfile. The [patch record](https://github.com/Kataglyphis/OxidANT/blob/develop/third_party/burn-dispatch-0.22.1/PATCHED.md) gives its source,
proof and removal conditions; do not add advisory ignores for that unused branch.
The root Burn pin is exact because cargo lock maintenance otherwise drops a versioned patch;
Renovate reports a newer Burn but requires review before applying it. Upgrade Burn and the
vendor copy together, then run `python3 scripts/linux/check-burn-lock-graph.py Cargo.lock`,
`cargo audit`, and the Burn tests. The local `--apply` wrapper fails if its lock maintenance
reintroduces the old chain; the CI script suite tests that verdict with mutated lockfiles.

Run it from WSL. For a Windows Git checkout, the bootstrap uses native Git so CRLF-only
`.gitmodules` differences do not block a clean update. Why the CLI is local, what `--apply`
writes per ecosystem, its exit codes, and `GITHUB_COM_TOKEN` for managers that ask
`api.github.com`: ANTfrastructure's
[`dependency-updates.md`](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/dependency-updates.md).

## Moving the crates with cargo

The newest versions your constraints already allow (`Cargo.lock` only, no manifest
edit):

```bash
cargo update
```

The manifests as well:

```bash
cargo install cargo-edit
cargo upgrade --dry-run --verbose
cargo upgrade --incompatible
```

[cargo-outdated](https://github.com/kbknapp/cargo-outdated) answers the same question.

## zune-core is no longer held

`zune-core` was held at 0.5.1 in `Cargo.lock` from 2026-08-07: 0.5.2 changed a macro
`zune-jpeg` 0.5.15 consumes, and a release build of the workspace failed in
`zune-jpeg-0.5.15/src/mcu_prog.rs:463` (*macro expansion ends with an incomplete
expression*). 0.5.2 has since been yanked, and 0.5.3 compiles with zune-jpeg 0.5.15
(checked 2026-09-25), so a bare `cargo update` resolves a working version; `Cargo.lock`
carries 0.5.3 (2026-10-19). Both arrive through `image` into the renderer, so only the
lockfile names them.

## Installed cargo binaries

```bash
cargo install cargo-update   # once
cargo install-update -a      # update every installed package
```
