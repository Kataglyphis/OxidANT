# Backlog

Consumed by the ANTfrastructure agentic loop (`shared/agentic-loop/`) when this
repo adopts it. The file was previously zero bytes, which reads as "the
protocol exists and the backlog is empty" — it was neither.

## Protocol

- `- [ ]` actionable — the planner may pick it up
- `- [b]` blocked — skipped, and excluded from the pending count, so a
  backlog containing only blocked items still lets the planner run again
- `- [x]` completed — pruned on sight; the history lives in git

## Open

- [b] Drop the vendored `third_party/egui-winit-0.36.2` and its
      `[patch.crates-io]` entry once egui 0.37 (or any release carrying
      emilk/egui#8516) is on crates.io, then move the egui family to it.
      Blocked on that release. Steps: `third_party/egui-winit-0.36.2/PATCHED.md`.
      Re-checked 2026-09-25: crates.io's newest `egui-winit` is still 0.36.2.

- [b] Instanced normals shade differently from the equivalent node transform.
      `a_non_uniform_instance_scale_shades_like_the_same_node_scale` fails with
      987 differing pixels against a threshold of 40; confirmed pre-existing and
      deterministic. Blocked here: `src/shaders/*.wgsl` are generated artifacts
      and the `forward.slang` source lives in the C++ engine repo.
      Reproduced byte-for-byte on the image's lavapipe (2026-09-29), where the
      Linux lanes now run the golden suite with `KATAGLYPHIS_REQUIRE_GPU=1`;
      the test is `#[ignore]`d for that, so drop the attribute with the fix.
- [ ] Run `scripts/linux/cat-stream/run-producer-pi.sh` on a real Pi 5 with
      its CSI camera. On 2026-09-29 it stopped bypassing the image's
      entrypoint (`--entrypoint bash`) and hands `/hostlibs` in as the
      caller's `LD_LIBRARY_PATH`, which ANTfrastructure CON23's entrypoint
      keeps ahead of the image's `/opt/libcamera` (with GCC's runtime first).
      Proven in emulation only: the arm64 child under QEMU, the runner's
      recorded `nerdctl run` with an empty stand-in `/hostlibs`, gives the
      right order, a stubbed `/hostlibs/libcamera.so.0.7` wins in `ldd`, and
      `gst-inspect-1.0 libcamerasrc`/`webrtcsink` load. No camera, no
      producer build. Done when the producer streams from the board; if it
      does not, the old `--entrypoint bash` form is in git history.

- [b] Ship the .slang sources with crates/webgpu_renderer, or add a
      slang -> wgsl step, so "there is no .slang file in this repo" stops
      being true. src/shaders/*.wgsl are checked-in GENERATED artifacts whose
      source is the C++ engine's forward.slang; hand-editing the WGSL
      desynchronises it, which is why the instanced-normal bug above cannot be
      fixed here. Blocked on deciding who compiles tex_quad.slang.

- [ ] Find why the renderer's `headless.exe` dies on `windows-11-arm` when its
      tests run in parallel. Run 36889467167 (2026-10-01): WARP (`Microsoft
      Basic Render Driver`, Dx12), `KATAGLYPHIS_REQUIRE_GPU=1`, the process
      exited after `running 39 tests` with no test finished and no panic text,
      the first time any arm64 test rendered. The same binaries on the x64
      runner's WARP pass in parallel, and serially (`--test-threads=1`, set in
      `Stage-CrossTests.ps1`) all 38 pass on arm64 (run 36907253720). One
      sample each, so it may be a race in WARP's arm64 JIT or a flake. Done
      when a parallel arm64 run is green repeatedly and the flag goes, or the
      crash has a name (exit code, faulting module; the hub's
      `Invoke-StagedTests.ps1` does not print the exit code when a binary ends
      without a summary).

- [ ] Decide whether the Linux x64 feature check (`feature-matrix` in
      `linux-x64.yml`) should also run on every push. It is the one job still
      behind a marker (`[build-features]`), six extra image pulls per run (one
      per matrix row since the `gui_windows` row joined on 2026-09-24); the
      2026-09-24 always-on request named the x64, arm64 and Windows lanes
      only. As of 2026-09-25 it has never run: no commit message carries the
      marker and the repository has no `workflow_dispatch` run.
- [ ] The Linux lanes can only produce a tarball. ANTfrastructure's
      `package_archive.sh` writes the tar and stops; its `create_deb()` was
      deleted on 2026-08-08 as unreachable, and `--flatpak-manifest`,
      `--desktop-file` and `--appdata-file` are checked for existence and then
      never read. `PACKAGE_TYPES` now says `tar`, which is honest but narrow.
      Restoring deb/AppImage/Flatpak means changing ANTfrastructure, not this repo
      -- the packaging/flatpak/ files here are ready and unused.
      Re-checked against hub 49be50f0, which DID add a flatpak pair
      (`app_packaging_ensure_flatpak_runtime`,
      `app_packaging_package_cmake_install_flatpak`): it does not close this.
      Both live in `lib/app-packaging.sh` and neither is reachable from
      `06-packaging/package_archive.sh`, which is byte-identical across the
      bump -- still `PACKAGE_TYPES=tar`, still accepting `--flatpak-manifest`
      and never reading it. The new entry point also stages from
      `cmake --install <build_dir>` and asserts an executable at
      `<prefix>/bin/<project_name>`; this repo has no CMakeLists.txt and builds
      with cargo, so it would need a cargo-install-tree twin, not a caller.
      Re-checked again at hub `604294e2`, the one bump that touches
      `lib/app-packaging.sh` (it makes `ostree` a required flatpak tool and says
      so when it is missing): `06-packaging/package_archive.sh` is still
      byte-identical and still never reads `--flatpak-manifest`, so the row is
      unmoved. Re-checked at hub `57ca2b14` (the pin on 2026-09-25):
      `package_archive.sh` last changed in f2c8a78a, its 2026-08-11 restore,
      and is still tar-only.

## Not adopted yet

The loop itself — config, runner wrappers, `scripts/agentic-loop/` — is not set
up. Copy-and-edit templates live in ANTfrastructure's
`shared/agentic-loop/templates/`; a consumer supplies this file, a config JSON,
thin runner wrappers, and optionally per-engine system prompts.
