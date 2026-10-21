# Windows: build and test in the Stevedore container

The workspace builds and tests inside the [Kataglyphis ANTfrastructure](https://github.com/Kataglyphis/ANTfrastructure)
Windows developer image, with [Stevedore](https://github.com/slonopotamus/stevedore)'s
`docker.exe`. No file here names the image ([AGENTS.md § Build & test in the Stevedore Windows container](../AGENTS.md#build--test-in-the-stevedore-windows-container));
the hub's runner asks `Get-CiImageReference -Windows`, and `-Image` overrides it for a one-off.
The current value:
`pwsh -c "Import-Module third_party/ANTfrastructure/windows/scripts/modules/WindowsContainerImage.Common.psm1; Get-CiImageReference -Windows"`.

## The driver

`scripts\windows\container\Invoke-StevedoreBuild.ps1` hands its spec to the hub's
`Invoke-RepoContainerBuild.ps1`, which **streams this repository into a reused container**
(`kata-rust-build`, workspace `C:\ws`) and the artifacts back out; nothing is
bind-mounted. It builds all three profiles (`dev`/debug, `profile` = release + debuginfo,
`release` = fat LTO) and optionally runs the debug test suite:

```pwsh
# build debug + profile + release in the container
pwsh -ExecutionPolicy Bypass -File .\scripts\windows\container\Invoke-StevedoreBuild.ps1

# build AND run cargo test --workspace (unit + integration + proptest fuzz + doc)
pwsh -ExecutionPolicy Bypass -File .\scripts\windows\container\Invoke-StevedoreBuild.ps1 -Test

# print the in-container commands, start nothing
pwsh -ExecutionPolicy Bypass -File .\scripts\windows\container\Invoke-StevedoreBuild.ps1 -Test -WhatIf
```

`-TestOnly` skips the build. The test run leaves out the WebGPU renderer, whose tests
cannot start in Server Core: [testing.md § On Windows the renderer's tests run outside the container](testing.md#on-windows-the-renderers-tests-run-outside-the-container).
The container keeps cargo's target (`C:\ct`) and home (`C:\ch`), so a second run is
incremental; `-FreshContainer` discards it. Without a bind mount, a host whose Dev Drive
filters were never allowed builds as well (the old driver needed `-StageSources` there).

## What it leaves behind

Artifacts land in `target\container\{debug,profile,release}`, mirrored to the
(gitignored) repo-root `debug\`, `profile\`, `release\` folders; each holds the CLI and
ObservANT exes, the `oxidant` and renderer cdylibs (`.dll` + import lib), the `oxidant`
and `kataglyphis_cxx_bridge` staticlibs (`.lib`) and their pdbs. The run fails when an exe built
in the container is missing on the host. The in-container logs come back as
`logs\in-container-build.log` and `logs\in-container-test.log`, a failed run's too.
Last verified run (2026-10-19, rustc 1.99.0, a new container): debug 1m46s, profile
1m50s, release 1m15s, then 325 tests in 25 binaries, 6m09s in all; a second build-only run
in the kept container took 12 s. The binaries run on the host:

```pwsh
.\release\kataglyphis_cli.exe stats --path .\README.md
```

The driver does not run the CI lane, and no local driver wraps that yet:
`scripts/windows/Invoke-WindowsLane.ps1`
([AGENTS.md § Continuous integration](../AGENTS.md#continuous-integration)).

## The rules, and the host side

The container rules this driver follows (the tar-pipe, cargo's trees outside the workspace,
the delivery check):
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
