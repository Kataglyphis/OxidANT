# Windows: build and test in the Stevedore container

The workspace builds and tests inside the [Kataglyphis ANTfrastructure](https://github.com/Kataglyphis/ANTfrastructure)
Windows developer image, with [Stevedore](https://github.com/slonopotamus/stevedore)'s
`docker.exe`. No file here names the image ([AGENTS.md § Build & test in the Stevedore Windows container](../AGENTS.md#build--test-in-the-stevedore-windows-container));
the driver asks `Get-CiImageReference -Windows`, and `-Image` overrides it for a one-off.
The current value:
`pwsh -c "Import-Module third_party/ANTfrastructure/windows/scripts/modules/WindowsContainerImage.Common.psm1; Get-CiImageReference -Windows"`.

## The driver

`scripts\windows\container\Invoke-StevedoreBuild.ps1` **bind-mounts this repository
into the container** as `C:\ws-mnt`, not a copy, so artifacts land straight in your tree
and `third_party/` is available inside. It builds all three profiles (`dev`/debug,
`profile` = release + debuginfo, `release` = fat LTO) and optionally runs the debug test
suite:

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
`-StageSources` restores the old robocopy-to-`%LOCALAPPDATA%\Temp` path for a host whose
Dev Drive filters were never allowed: `docker run` then exits at once with *"Der
Dateisystem-Minifilter kann nicht an das Entwicklervolume angefügt werden"*. The
permanent fix is a host setting (`fsutil devdrv setFiltersAllowed`, in the hub's
performance doc linked below). `fsutil devdrv query` needs elevation itself, so a failing
query tells you nothing; just try the mount.

## What it leaves behind

Artifacts land in `target\container\{debug,profile,release}`, mirrored to the
(gitignored) repo-root `debug\`, `profile\`, `release\` folders; each holds the CLI exe,
cdylib (`.dll` + import lib), staticlib (`.lib`) and pdb. Last verified run (2026-08-07,
rustc 1.97.1): all three profiles built (debug 1m35s, profile 1m32s, release 1m12s),
straight into the repo through the mount, and the binaries run on the host:

```pwsh
.\release\kataglyphis_cli.exe stats --path .\README.md
```

The driver does not run the CI lane, and no local driver wraps that yet:
`scripts/windows/Invoke-WindowsLane.ps1`
([AGENTS.md § Continuous integration](../AGENTS.md#continuous-integration)).

## The rules, and the host side

The container rules this driver follows (container-local writes, named containers without
`--rm`, the bounded `Wait-ContainerExit`, `Remove-BuildContainerSafe`):
[AGENTS.md § Inside the Stevedore Windows container](../AGENTS.md#inside-the-stevedore-windows-container).
**ANTfrastructure is the authority on the host side**:
[setting up a Windows host for Stevedore](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/windows-host-setup.md)
(services, `docker-users`, CNI nat conf),
[Windows container internals](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/windows-builds.md)
(wcifs/bindFlt, process isolation, the layer-commit bug),
[Dev Drive filter setup and bind mount vs tar-pipe](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/windows-container-build-performance.md),
[Linux containers on Windows (Rancher Desktop)](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/rancher-desktop-linux-containers.md)
and [wiring a new project to all of it](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/adopting-in-a-new-project.md).
The rest: [AGENTS.md § 2](../AGENTS.md#2-what-antfrastructure-owns--links-only).
