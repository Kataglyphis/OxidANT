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

- **Run it from WSL.** The wrapper runs the hub's `renovate-local.sh` on this root and
  bootstraps a pinned, checksum-verified Node and Renovate on first use; the Windows side
  has no Node.
- **Nothing runs it for you.** The Renovate GitHub App is installed on no repo in this
  family and will not be, so this CLI is the only reader of `.github/renovate.json`. No
  workflow calls it and it blocks no commit.
- **`--apply` writes every ecosystem the tree has** (the hub's script since 2026-09-11):
  the `third_party/ANTfrastructure` gitlink (the one `.gitmodules` entry; it declares
  `branch = develop`, which `--apply` requires); a `Cargo.toml` value rewritten on its
  line, with `Cargo.lock` refreshed by `cargo update`; and, through the shared preset's
  lock-file maintenance, `Cargo.lock` moved to the newest releases the manifests allow.
  A run that cannot finish puts everything back and exits 1; exit 2 means a reported
  update was not applied. It stages and commits nothing.
- `third_party/egui-winit-0.36.2` is skipped (`.github/renovate.json`).
  `.github/dependabot.yml` is still present; retiring it is a separate decision.
- **Managers that reach `api.github.com` answer short without a token**, and say so.
  Under `--platform=local` the variable is `GITHUB_COM_TOKEN`, not `RENOVATE_TOKEN`:
  `GITHUB_COM_TOKEN="$(gh auth token)" bash scripts/linux/renovate-local.sh --managers cargo`.

The script header covers the rest. Why the CLI is local, and what `--apply` writes for
each ecosystem: ANTfrastructure's
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
