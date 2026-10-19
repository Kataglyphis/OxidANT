# Dependency updates

Crate versions live in `Cargo.toml`/`Cargo.lock`; the `third_party/ANTfrastructure`
gitlink is a dependency too, and the one that silently changes every gate
([AGENTS.md § 3](../AGENTS.md#3-critical-invariant-submodule-pins)).

## What is behind: Renovate as a local CLI

What is behind — the submodule gitlink and the workspace crates — is answered by
Renovate run locally, not by a bot. Run it from WSL; it bootstraps a pinned,
checksum-verified Node and Renovate on first use.

```bash
bash scripts/linux/renovate-local.sh                    # report: every manager the tree has
bash scripts/linux/renovate-local.sh --managers cargo   # only the workspace crates
bash scripts/linux/renovate-local.sh --apply --dry-run  # the plan
bash scripts/linux/renovate-local.sh --apply            # apply the plan
```

The wrapper runs the hub's `renovate-local.sh` on this root. What `--apply` writes, for
the gitlink and for every other ecosystem the repo has, and why the CLI is local:
ANTfrastructure's
[`dependency-updates.md`](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/dependency-updates.md).
This repo's rules for it (report unless asked, the token for `--managers cargo`):
[AGENTS.md § Dependency upgrades](../AGENTS.md#dependency-upgrades).

## Moving the crates with cargo

For the newest versions your current constraints already allow (`Cargo.lock` only,
no manifest edit):

```bash
cargo update
```

To move the manifests as well:

```bash
cargo install cargo-edit
cargo upgrade --dry-run --verbose
cargo upgrade --incompatible
```

[cargo-outdated](https://github.com/kbknapp/cargo-outdated) is a useful tool for the
same question.

## zune-core is no longer held

`zune-core` was held at 0.5.1 in `Cargo.lock` because 0.5.2 broke `zune-jpeg` 0.5.15 in
release builds. 0.5.2 has since been yanked, and 0.5.3 compiles with zune-jpeg 0.5.15
(checked 2026-09-25), so a bare `cargo update` now resolves a working version;
`Cargo.lock` carries 0.5.3 (2026-10-19). The history:
[AGENTS.md § Do not let `cargo update` take zune-core to 0.5.2](../AGENTS.md#do-not-let-cargo-update-take-zune-core-to-052).

## Installed cargo binaries

How to update all installed packages:

1. Install updater:

   ```bash
   cargo install cargo-update
   ```

2. Now update all packages:

   ```bash
   cargo install-update -a
   ```
