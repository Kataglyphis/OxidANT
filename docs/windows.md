# Windows: build and test in the Stevedore container

The workspace builds and tests inside the [Kataglyphis ANTfrastructure](https://github.com/Kataglyphis/ANTfrastructure)
Windows developer image using [Stevedore](https://github.com/slonopotamus/stevedore)'s
`docker.exe`. The image reference is not written down anywhere in this repository —
including here — because ANTfrastructure's `linux/scripts/01-core/versions.env` owns it;
the driver asks `Get-CiImageReference -Windows` for it and `-Image` overrides for a
one-off. To see the current value:
`pwsh -c "Import-Module third_party/ANTfrastructure/windows/scripts/modules/WindowsContainerImage.Common.psm1; Get-CiImageReference -Windows"`.

## The driver

`scripts\windows\container\Invoke-StevedoreBuild.ps1` **bind-mounts this repository
directly into the container** (as `C:\ws-mnt`) — no copy, so artifacts land straight in
your tree and `third_party/` is available inside. It builds all three profiles
(`dev`/debug, `profile` = release + debuginfo, `release` = fat LTO) and optionally the
full debug test suite:

```pwsh
# build debug + profile + release in the container
pwsh -ExecutionPolicy Bypass -File .\scripts\windows\container\Invoke-StevedoreBuild.ps1

# build AND run cargo test --workspace (unit + integration + proptest fuzz + doc)
pwsh -ExecutionPolicy Bypass -File .\scripts\windows\container\Invoke-StevedoreBuild.ps1 -Test

# only if your host refuses the mount (see below)
pwsh -ExecutionPolicy Bypass -File .\scripts\windows\container\Invoke-StevedoreBuild.ps1 -StageSources
```

`-TestOnly` skips the build. The test run leaves out the WebGPU renderer, whose tests
cannot start in Server Core: [testing.md § On Windows the renderer's tests run outside the container](testing.md#on-windows-the-renderers-tests-run-outside-the-container).
Every cargo write stays container-local (`CARGO_TARGET_DIR=C:\ct`, `CARGO_HOME=C:\ch`),
so only a plain artifact copy crosses the mount; `-StageSources` is for a host whose Dev
Drive filters were never allowed (`docker run` then fails at once with *"Der
Dateisystem-Minifilter kann nicht an das Entwicklervolume angefügt werden"*). The
permanent fix is a host setting, in the performance doc below. `fsutil devdrv query`
needs elevation itself, so a failing query tells you nothing — just try the mount.

## What it leaves behind

Artifacts land in `target\container\{debug,profile,release}` and are mirrored to the
(gitignored) repo-root `debug\`, `profile\`, `release\` folders; each contains the CLI
exe, cdylib (`.dll` + import lib), staticlib (`.lib`) and pdb. Latest verified run
(2026-08-07, rustc 1.97.1): all three profiles built (debug 1m35s, profile 1m32s,
release 1m12s), written straight into the repo through the mount, and the binaries run
on the host, e.g.:

```pwsh
.\release\kataglyphis_cli.exe stats --path .\README.md
```

This driver does not run the CI lane: that is `scripts/windows/Invoke-WindowsLane.ps1`
([AGENTS.md § Continuous integration](../AGENTS.md#continuous-integration)).

## The rules, and the host side

The container rules this driver follows (container-local writes, named containers without
`--rm`, the bounded `Wait-ContainerExit`, `Remove-BuildContainerSafe`) are in
[AGENTS.md § Inside the Stevedore Windows container](../AGENTS.md#inside-the-stevedore-windows-container)
and [§ Build & test in the Stevedore Windows container](../AGENTS.md#build--test-in-the-stevedore-windows-container).
**ANTfrastructure is the authority on the host side**:

| Topic | Read |
| --- | --- |
| Setting up a Windows host for Stevedore (services, `docker-users`, CNI nat conf) | [`docs/windows-host-setup.md`](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/windows-host-setup.md) |
| Windows container internals: wcifs/bindFlt, process isolation, layer-commit bug | [`docs/windows-builds.md`](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/windows-builds.md) |
| Dev Drive filter setup (`fsutil devdrv setFiltersAllowed`), bind mount vs tar-pipe | [`docs/windows-container-build-performance.md`](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/windows-container-build-performance.md) |
| Running Linux containers on Windows (Rancher Desktop) | [`docs/rancher-desktop-linux-containers.md`](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/rancher-desktop-linux-containers.md) |
| Wiring a new project to all of it | [`docs/adopting-in-a-new-project.md`](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/adopting-in-a-new-project.md) |
