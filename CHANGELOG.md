# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

It also carries the **dated engineering history** that used to sit in `AGENTS.md`:
measured baselines and the post-mortems of three migrations. Those are records of
what happened on a date, not guidance about what to do now, and keeping them in
`AGENTS.md` is what made that file 490 lines over 19 headings. They were moved here
on 2026-09-15, verbatim. Nothing was deleted.

## [Unreleased]

### Security (2026-11-16, an audit ignore for ruzstd, which is locked but not built)
- `.cargo/audit.toml` ignores RUSTSEC-2026-0344 (owner decision), which had stopped the Linux lane since 216b0a1:
  `ruzstd` 0.7.3 comes only through `basisu` 0.1.0, whose only release requires `ruzstd ^0.7`, and the renderer
  enables `basisu` without `zstd`, so cargo-deny does not meet the crate in the built graph. A BACKLOG entry drops
  the ignore once basisu releases on `ruzstd` 0.9.1.

### Changed (2026-11-16, owner decision: the renderer builds at opt-level 1 in debug)
- **`[profile.dev.package.kataglyphis_webgpu_renderer] opt-level = 1`** in the workspace `Cargo.toml`; dependencies stay
  at 2. Measured in `:latest` on the 8-core arm64 host, `CARGO_BUILD_JOBS=2`, with a throwaway example over 5.9 M random
  boxes (the Colosseum's triangle count) through `tracer::bvh`: `build_parallel` on 8 threads 16.1-21.3 s at
  opt-level 0, 1.31-1.63 s at 1; the single-threaded `build` 75.6-76.7 s, then 4.2-4.4 s.
- **The cost is compile time**: the renderer's lib from clean 20 to 35 s, `cargo test -p kataglyphis_webgpu_renderer
  --no-run` after it 208 to 296 s; the whole suite then ran in 159 s instead of 186 (855 passed).

### Changed (2026-11-16, owner decision: clouds march at half resolution by default)
- **`CloudSettings::half_resolution` defaults on** (`render/clouds.rs`, the one place the default is set; the Sky tab,
  the web demo and `editor_snapshot` take it from there), so every cloud image moves: a quarter of the pixels march
  and the depth-aware upsample fills the rest, 63 to 80 % less cloud time on lavapipe (the measurements of the entry
  that added the switch). A half-resolution frame marches once per pixel without the forward path's MSAA spread, so a
  silhouette pixel in front of a cloud shows bare again unless the switch is off. The C++ engine flips its default in a
  separate BeschleunigerBallett change.
- Tests: three cloud tests measure what only the full march shows and now set `half_resolution: false`:
  `silhouette_pixels_in_front_of_a_cloud_are_clouded` tests the spread (at half resolution its 54 silhouette pixels
  showed no cloud); `enabling_clouds_changes_the_frame_and_adds_detail`, the C++ test's port, still passed but only
  by 0.0001 (detail 0.0063 without clouds, 0.0186 with the full march, 0.0064 with the half); and
  `a_grazing_view_through_the_slab_is_not_grainy` tests the estimator's grain, which the upsample smooths (luminance
  contrast 0.044 full, 0.019 half, both under its 0.15). The rest pass at the default with their thresholds untouched. `half_resolution_clouds_keep_off_the_wall_...` now
  asserts the default is on and switches the full march on explicitly: mean 0.049 levels, 40 of 16384 px over 2, none
  over 8, 1.011 of the rim's alpha. `gpu_timing.rs` keeps timing forward at full and deferred at half resolution.

### Changed (2026-11-16, owner decision: the C++ path tracer's bounce misses follow this one)
- **A path-traced bounce that misses keeps this renderer's sky**: the gradient, or the loaded environment, times the
  ambient strength and the environment's intensity. The owner decided that the C++ path tracer changes to match, not
  this one; BeschleunigerBallett tracks that work, so the `BACKLOG.md` item is gone. `tracer.md` and the roadmap's
  traced row say so.

### Security (2026-11-16, Zstandard KTX2 levels without ruzstd 0.7, RUSTSEC-2026-0344)
- **`ruzstd` 0.7.3 is out of the lock**: its decoder never bounded a block's decompressed size, so a file under
  128 KiB could allocate gigabytes (RUSTSEC-2026-0344, fixed in 0.9.1). It came only from `basisu` 0.1.0's default
  `zstd` feature, and `basisu` has no release past 0.1.0 (upstream's `ruzstd ^0.9.1` PR is unmerged), so the
  renderer builds `basisu` without that feature and inflates Zstandard-supercompressed UASTC KTX2 itself
  (`src/asset/ktx2_zstd.rs`) with `ruzstd` 0.9.1 (MIT, no `hash` feature), then hands basisu the uncompressed file.
- **Bounded**: each level must declare and inflate to exactly its 4x4 blocks' size, the chain at most 512 MiB; the
  read stops one byte past the level, and a frame's window is capped at its level's size (1 MiB at least). Tests: an
  8 KiB frame of RLE blocks that would inflate to 256 MiB stops at 4097 bytes, a 2 TiB window is refused, a
  Zstandard UASTC file transcodes and retargets to the same blocks as the uncompressed one.
  Zstandard on other formats stays refused (ETC1S is always BasisLZ).

### Added (2026-11-16, a weekly job for the Rust mutation scores)
- `rust-mutants.yml` runs ObservANT's five `cargo mutants` groups from `docs/testing.md` on Sundays and by hand, one
  job per group, through the new `scripts/linux/run-mutants.sh` (container step `mutants <group>`), and uploads each
  `mutants.out`; missed and timed-out mutants are its report, a failing baseline fails it (owner decision). Until the
  image pins cargo-mutants (hub CON238) each job warns and runs nothing. `test-run-mutants.sh` holds the file lists,
  the exit codes and the no-op.

### Changed (2026-11-16, streamed textures keep their CPU chains within a budget)
- **`TextureStreaming::cpu_bytes`** (256 MiB in `INTERACTIVE`) bounds the level chains streaming keeps on the CPU.
  Past it the least recently stepped chains are dropped, during the upload already, one texture at a time, so its
  peak is the budget plus one chain. A step that needs a dropped chain rebuilds it from the scene's image through
  the upload's own `material_chain`; the app keeps that image alive as the traced scene anyway, so the bound costs
  no second copy; the rebuild counts toward the frame's byte budget. A chain that no longer rebuilds as uploaded
  freezes the texture at its levels. A texture whose tail is its finest level, and every texture while streaming is
  off, keeps no chain. `TextureResidency` reports `cpu_bytes` and `regenerated`.
- **Measured** (`tests/texture_streaming.rs`, release, lavapipe on arm64 WSL): sixteen 2048x2048 RGBA8 textures keep
  256.0 MiB of chains instead of 341.3 MiB. A rebuild costs what the upload's own chain did: a CPU box filter of
  about 0.13 s for a 2048x2048 RGBA8 image (2.1 s for sixteen), a copy for BC. Thirteen rebuilt chains of sixteen
  render byte-identical to chains kept whole.

### Added (2026-11-16, streamed textures drop the levels the camera left behind)
- **Eviction** (`render/texture_stream.rs`, `coarser_base`, `evict_now`): a texture holding more than one level
  finer than any user needs (the margin, `EVICTION_MARGIN`) for `TextureStreaming::evict_after_frames` frames in a
  row (120 in `INTERACTIVE`, two seconds at 60 Hz; 0 never evicts) drops one level a frame, never past its upload's
  tail and never past the level it wants. Evictions share the frame's byte budget and run after every finer step;
  with the copy path an eviction copies its levels and writes nothing from the CPU. The margin keeps a camera at a
  level boundary from thrashing, the frame count one that comes and goes. A streaming texture now keeps its CPU
  chain for as long as streaming is on, since an evicted level may be needed again. `TextureResidency::evictions`
  counts them.
- **Measured** (`tests/texture_streaming.rs`, lavapipe): sixteen 1024x1024 RGBA8 textures, all resident (85.3 MiB),
  then the camera flies 400 units away. Residency per frame: 61.3, 37.3, 13.3, 3.3, then the 1.33 MiB tails from the
  fifth frame on, in 48 evictions that copied 29.3 MiB and wrote nothing. A texture evicted to its tail and streamed
  back renders byte-identical to the whole upload.

### Changed (2026-11-16, a streaming step copies the resident levels on the GPU)
- **A finer step writes only its new levels** (`render/forward/streamed_texture.rs`): the new texture gets the
  levels the old one lacks from the CPU chain and the rest by `copy_texture_to_texture` from the old texture, in one
  submit a frame. Before, every resident level was written again from the CPU. A backend that cannot copy the format
  (GL with a compressed texture, `gpu_copy_allowed`, checked at run time) keeps the CPU path, as does
  `TextureStreaming::gpu_copies = false`. `TextureResidency` counts `step_written_bytes` and `step_copied_bytes`.
- **Measured** (`tests/texture_streaming.rs`, lavapipe): one 1024x1024 texture from its 128-texel tail to the full
  chain in three steps. RGBA8: 5.25 MiB written from the CPU and 1.75 MiB copied, against 7.0 MiB written; BC1:
  0.66 MiB and 0.22 MiB against 0.88 MiB. Both paths render byte-identical frames.

### Added (2026-11-16, mip-tail texture streaming in the WebGPU renderer)
- **`ForwardRenderer::texture_streaming`** (`TextureStreaming`, off by default; the app sets
  `TextureStreaming::INTERACTIVE`): an upload writes each material texture's levels up to 128 texels a side and
  keeps the finer ones on the CPU. Each frame works out, per texture, the finest level any primitive using it needs:
  log2 of texels per pixel at the nearest point of the primitive's world AABB, from the slot's UV density (UV area
  over surface area), its `KHR_texture_transform` and the model's largest axis scale. Visible textures step first,
  then the furthest from what they need, then the cheapest; one level a frame each, within 8 MiB, the first step
  always. A step writes a new texture from the finer base (whole 4x4 blocks for BC/ETC2/ASTC, never past the device's
  side) and rebuilds the bind groups of the primitives that sample it, the MASK casters' shadow bundles included.
  Switching streaming off finishes every texture on the next frame; `finish_texture_streaming` does it at once,
  `texture_residency` reports resident and full bytes. Plain WebGPU, so the browser streams as well.
- **The upload path is one level chain** (`render/texture.rs`): RGBA8 box-filtered, BC passthrough, Basis
  retargeted to ETC2/ASTC or BC decoded on the CPU all become a `LevelChain` written by `write_chain` from any base.
- **Measured** (`tests/texture_streaming.rs`, release, lavapipe on arm64 WSL): sixteen 2048x2048 textures after the
  upload. RGBA8: 1.33 MiB on the GPU instead of 341.3 MiB, but 2.15 s against 2.27 s, as the CPU box filter, which
  the tail needs too, is nearly all of it. BC1 (no filter): 0.17 MiB instead of 42.7 MiB, in 2.6 ms instead of
  17.4 ms. A fully streamed frame is byte-identical to the whole upload's.

### Changed (2026-11-16, indirect draws from a GPU draw cull in the WebGPU renderer)
- **Every scene draw is `draw_indexed_indirect`** (`crates/webgpu_renderer/src/render/draw_cull.rs`,
  `shaders/draw_cull/draw_cull.slang`). A compute pass, `FramePass::DrawCull` first in every raster frame
  (`TimedPass::DrawCull`, "DrawCull"), takes one slot per primitive and winding run and writes its
  `DrawIndexedIndirectArgs` for the camera and each active cascade: the camera frustum, last frame's occlusion and the
  eye-inside guard for the camera, each cascade's frustum without its near plane for the casters; a culled slot keeps
  its index count and gets no instances. The forward, G-buffer, blended and shadow draws read those arguments; the
  mirrored run binds the instance buffer at its offset, so `first_instance` stays 0 without `INDIRECT_FIRST_INSTANCE`.
- **No CPU readback decides a draw any more.** The occlusion query pass resolves into a storage buffer and the compute
  culler keeps its visibility buffer; the next frame's draw cull reads either on the GPU. Their readback rings remain
  for `occlusion_visibility`, `occlusion_samples` and `gpu_culling_visibility` only, and both now measure every frame,
  skipping only the copy when no ring slot is free. The Visibility counts (`occlusion_cull_stats`, `primitive_stats`,
  `shadow_caster_stats`) are GPU counters behind a non-blocking readback: current after `render_to_pixels`, a frame or
  two late in a window.
- **The CPU still skips encoding what its exact frustum test culls**, camera and cascades alike: wgpu's encode and
  submit cost grows with every draw, culled or not. Encoding all 1600 cubes of a 40x40 grid and leaving the frustum to
  the GPU took the median CPU frame (encode + submit, release, llvmpipe, 320x180) from 5.4 to 11.8 ms. With the
  pre-cull, close (441 drawn) and overview (1449 drawn), frustum only / with the compute culler / with queries: before
  5.96 / 6.02 / 6.14 and 7.01 / 7.13 / 7.38 ms, after 6.38 / 8.74 / 9.54 and 9.29 / 12.49 / 10.09 ms. Most of it is
  wgpu's default indirect-call validation (`InstanceFlags::VALIDATION_INDIRECT_CALL`, a check pass per indirect draw):
  with `WGPU_VALIDATION_INDIRECT_CALL=0` the same run took 6.23 / 6.42 / 6.89 and 7.45 / 7.72 / 8.04 ms. Release builds
  therefore switch it off (`context::instance_flags`; the variable set to 1 restores it), debug builds and the tests keep
  it. Occlusion-culled draws are still encoded, since only the GPU knows them.
- One draw per slot, natively and on the web: every draw rebinds its material group and buffers, so
  `multi_draw_indexed_indirect` needs a shared vertex/index buffer and bindless materials first.
- Images: a hash of the frame, HDR, depth and shadow map with the Visibility counts over 72 settings (forward and
  deferred, frustum culling on and off, no occlusion, queries and the compute culler, one and three cascades, three
  cameras, a 123-primitive scene with blended, double-sided and mirrored-instanced primitives) was identical before
  and after.
- Tests: `tests/indirect_draws.rs` (a culled draw has no instances and a drawn one exact arguments, a mirrored run is
  its own draw, a camera turn culls and draws again, blended draws are frustum-culled, each cascade keeps only its
  casters, and occlusion by queries and by compute culls the hidden cubes while the frame stays identical to an
  unculled one); `tests/cull_constants.rs` pins the draw cull's workgroup and layouts.

### Added (2026-11-16, glTF `KHR_texture_basisu` and ETC2/ASTC Basis targets in the WebGPU renderer)
- **glTF `KHR_texture_basisu`**: `load_gltf` and `load_gltf_slice` no longer go through `gltf::import`, which
  decodes every image itself and refuses `image/ktx2`. `src/asset/gltf_images.rs` parses without gltf's
  validation, accepts the extension in `extensionsRequired` and as a texture's only source, then validates the
  rest as gltf does. A KTX2 image (by MIME type or `.ktx2` URI; GLB view, data URI or file) loads through
  `load_ktx2`, every other image through gltf's own `image::Data::from_source`, so every existing glTF imports
  the same images. A texture takes its extension image when that loads, else its core `source` with a warning;
  without a fallback the failure fails the load. Fixtures: `cube_basisu.gltf` (required, no fallback,
  `ramp_uastc.ktx2`) and `cube_basisu_fallback.gltf` (a KTX2 that is not one, and a 2x2 red PNG fallback). On
  llvmpipe the first draws the ramp's red, green and blue corners (555/452/624 dominant pixels at 128x128, with
  BC and decoded alike), the second only red.
- **Basis targets by adapter**: `CompressedFormat::Etc2Rgba8Unorm` and `Astc4x4RgbaUnorm`; the device requests
  `TEXTURE_COMPRESSION_ETC2` and `_ASTC` where offered (`GpuContext::supports_etc2`, `supports_astc`). The load
  still transcodes to BC7 and keeps the KTX2 (`CompressedTexture::basis_source`); an upload without BC
  transcodes it again to ETC2, else ASTC 4x4, and only without either decodes BC7 on the CPU. A failed
  transcode or upload problem falls back to that BC7 path.
- **Tested on the CPU only**: llvmpipe and lavapipe expose neither ETC2 nor ASTC, so the GPU test of that path
  skips there. `basis.rs` decodes the transcoded blocks: ETC2 through a test decoder written from the
  specification (ETC1 modes plus EAC alpha; it panics on T, H and planar), ASTC through basisu's own LDR block
  decoder. Level 0 against the source ramp: ETC1S to ETC2 7.0 mean, identical to basisu's RGBA32; ETC1S to
  ASTC 7.0 (0.35 from RGBA32); UASTC to ETC2 4.1 (its ETC1 hint); UASTC to ASTC 1.3, identical to RGBA32.

### Added (2026-11-16, Basis KTX2 transcoding in the WebGPU renderer)
- **`load_ktx2` transcodes Basis Universal KTX2**: ETC1S (BasisLZ) and UASTC LDR (uncompressed; Zstandard levels
  go through `ruzstd`, untested here) become a BC7 mip chain (`src/asset/basis.rs`), or RGBA8 when the base is not whole 4x4 blocks. An adapter
  without BC takes the existing CPU BC7 decode, so the same file draws everywhere. The transcoder is the
  pure-Rust `basisu` 0.1.0 (Apache-2.0, a port of basisu v2_1_0's transcoder, plus `ruzstd` 0.7.3, MIT), so the
  wasm32 demo builds it too, where the `basis-universal` C++ binding would need a C++ toolchain for
  `wasm32-unknown-unknown`.
  UASTC HDR, ASTC/XUASTC sources, video, cubemaps and arrays are refused by name.
- **Fixtures**: `ramp_etc1s.ktx2` (835 B) and `ramp_uastc.ktx2` (1664 B), a 32x32 ramp with six levels encoded
  by `basis-universal` 0.3.1 (basisu 1.16) and repacked from `.basis` by `tests/assets/repack_basis_ktx2.py`;
  `green_uastc.ktx2` (332 B), solid-colour UASTC blocks a unit test rebuilds byte for byte. Measured against the
  source on level 0: ETC1S to BC7 5.1 mean (34 max) of 255, to RGBA32 7.0; UASTC to BC7 1.3 (9 max). The tests
  bound ETC1S at 10 and UASTC at 2, with the transposed ramp as a negative control, and upload both, with and
  without BC, inside a validation scope.
- **ETC2/ASTC targets and glTF `KHR_texture_basisu`** followed the same day (next entry).

### Added (2026-11-16, ObservANT's visit rules replay against scenarios)
- **Five hard cases replay through the visit rules in the `test` step**: `crates/observant/tests/scenarios.rs`
  reads `tests/scenarios/*.jsonl` and drives each run through the default `score` and `max_box_area`, the spots
  marked not a cat, `counted` and `Presence` (`capture::replay`, the same `Visits::record` the camera calls, which
  now returns the change), asserting the visits and stays within 1 s. The scenarios are synthesised from the hand
  observations in `docs/observant.md` and here by `tests/scenarios/synthesise.py`, not recorded on a Pi, and their
  `provenance` says so: `drying-rack-still` (the rack at 0.27 to 0.34 that never moves: no visit),
  `drying-rack-cat-in-front` (the rack under its ignored spot and a cat in front of it: one visit of 44 s),
  `cat-behind-the-sofa` (50 s out of sight: one visit of 112 s), `feeding-hour` (two cats, breaks of 40 and 30 s,
  then 98 s: two visits) and `dark-room` (lone 0.2x hits, a lamp's whole-frame box at 0.31 and 0.2x boxes, a dim cat
  at 0.25 to 0.31: one visit of 51 s).
- **Each rule fails its scenario when broken**, measured by planting the change and replaying:

  | Change | Fails | Replayed |
  | --- | --- | --- |
  | `CONFIRM` 2 → 1 | `dark-room` | a lone hit arrives at 15 s |
  | `RETURN` 60 → 40 s | `cat-behind-the-sofa` | two visits |
  | `RETURN` 60 → 120 s | `feeding-hour` (and two more stay open) | the 98 s break merges |
  | `score` 0.25 → 0.3 | `dark-room` | the dim cat never arrives |
  | `score` 0.25 → 0.2 | `dark-room` | the lamp's 0.2x boxes arrive at 150.5 s |
  | `max_box_area` 0.9 → 1.0 | `dark-room` | the lamp's whole-frame box arrives |
  | `IGNORE_IOU` 0.5 → 0.99 | `drying-rack-cat-in-front` | the rack arrives at 1 s and never leaves |
  | `counted` without the motion gate | `drying-rack-still` | the rack arrives at 61 s |
- **`--record-scenario FILE`** (`record_scenario`, off by default) writes a live run in that format: a header (size,
  `inference_fps`, the ignored spots at the start, `floor`, `provenance`), then per model run `t`, the boxes and the
  lately moved blocks as runs (`scenario::Recorder`). While it records the model is asked down to 0.1
  (`RECORD_FLOOR`) and the service applies `score` itself, so a replay can judge a lower threshold. A recording
  replaces a synthesised scenario once its visits are written into `EXPECTED`.

### Changed (2026-11-16, render graph v2: the post chain)
- **Bloom, SSAO and FXAA render into the frame graph's pool** (`render/forward/frame_graph.rs`'s
  `transient_targets`): bloom's half-resolution output and scratch (`Bloom`, `BloomScratch`), SSAO's raw and blurred
  occlusion (`AmbientRaw`, `Ambient`) and FXAA's input (`Tonemapped`, in the tonemap's output format, which
  `FrameConfig::output_format` carries). `BloomPass::bind`, `SsaoPass::bind` and `FxaaPass::prepare` take the pool's
  views and rebind when it moves one; a frame without the row backs none, and the tonemap samples a zero texel in
  their place, as their strength is zero there. On an even output bloom's output takes the half-resolution cloud
  march's texture. 1920x1080 deferred + clouds + bloom, SSAO and FXAA: 206.7 MiB of these targets before the graph,
  95.9 MiB with a texture each, 80.1 MiB aliased (scene and clouds 63.3, bloom 7.9, SSAO 1.0, FXAA 7.9); with the
  half-resolution march 210.6, 99.9 and still 80.1 MiB.
- **The trace resolve records in its own row**: `TracedPasses::encode` records the kernel and `encode_resolve` the
  resolve (`TraceResolve` holds its targets and the sky that draws after it), so `trace_resolve` no longer rides on
  the kernel's row. `TimedPass` and its names are unchanged.
- No image moves: frame, HDR, depth and cloud hashes over 21 settings (forward, deferred, clouds full and half, FXAA
  off and always, bloom or SSAO off, auto exposure, both cullers, an odd output, ray and path traced with and
  without clouds), two frames each on a fresh renderer and again in one renderer switching through all of them, were
  bit-identical before and after.
- Tests: `frame_graph::tests` (the post targets exist exactly when their rows run, bloom takes the half march only
  when the sizes agree, the measurement above with a row per pass); `tests/deferred.rs`'s pool bytes count bloom,
  SSAO and FXAA.

### Changed (2026-11-16, render graph v2)
- **The frame is compiled from a render graph every frame** (`crates/webgpu_renderer/src/render/graph.rs`). Passes
  declare what they read and write; `FrameGraph::compile` orders them by those edges (Kahn, ties to the order rows were
  added in), culls passes no exported resource needs, fails a cycle, an undefined read or a second writer, and packs
  transient textures into slots: equal descriptions, lifetimes that never meet. wgpu places the barriers itself, so the
  graph's job is order and lifetimes. `TransientPool` backs the slots, reuses textures of equal description across
  frames, drops the rest, and says when a target moved, which rebinds what samples it.
- **The renderer's frame is a graph of its settings** (`render/forward/frame_graph.rs`): drawn mode, raster path,
  clouds (half resolution, noise pending), bloom, SSAO, auto exposure, culling and FXAA choose the rows; each row is one
  `ForwardRenderer` method (`render/forward/record.rs`). The rows are added in the old record order, so the derived
  order is that order and no image moves: a hash of the frame, HDR, depth and cloud targets over 16 settings (forward,
  deferred, clouds full and half, FXAA off, SSAO and bloom off, auto exposure, both cullers, ray and path traced) was
  bit-identical before and after. `TimedPass` and its names are unchanged.
- **Transients**: the forward MSAA HDR and depth, the HDR and depth targets, the depth spread, the G-buffer and the
  cloud targets. A deferred frame holds no MSAA targets, a forward one no G-buffer, a traced one neither, and a
  deferred frame's clouds reuse the G-buffer normal's texture. At 1920x1080, deferred + clouds + bloom, SSAO and FXAA:
  189.8 MiB of these targets before, 79.1 MiB with a texture each, 63.3 MiB aliased; forward + clouds stays 150.3 MiB,
  a traced frame with clouds takes 39.6. Bloom's chain, SSAO's targets and FXAA's intermediate stay their passes'
  own (`BACKLOG.md`). `read_cloud_target` returns `None` after a frame without clouds, where it returned the last march.
- Tests: `graph::tests` (derived order, stable tie-break, culling, exported side outputs, the cycle, undefined read and
  double write errors, aliasing, and 2000 random graphs whose aliased transients never overlap);
  `frame_graph::tests` (all 960 setting combinations compile and record every row, the hand-written order, the
  v1 timing and cloud/FXAA order checks, only the drawn path's targets, the measurement above); `tests/deferred.rs`
  `each_path_holds_only_its_own_targets_and_deferred_clouds_alias_the_gbuffer` (the pool's bytes on a device).

### Removed (2026-11-16, shader leftovers)
- **`traced.slang` drops its dead `import sky_model`**: unused since 18e178a, as `trace_shading.slang` imports
  `sky_model` for `sky_with_sun` and `bounce_sky` itself. No WGSL changes.
- **`brdf.slang` drops `geometry_smith`**: no shader called it after 7354e2d; `geometry_smith_k` with
  `smith_k_analytic` is the same value. A consumer calling it (BeschleunigerBallett's `tests/brdf_test.slang`) passes
  `smith_k_analytic(roughness)` to `geometry_smith_k` instead.

### Added (2026-11-16, Slang import gate)
- **`scripts/check-slang-imports.py` fails a dead Slang `import`** (a module none of whose top-level names the
  importer mentions), run by `pre-push-checks.sh` and so by `lint-gates.yml`'s script suites; fixtures in
  `scripts/linux/tests/test-slang-imports.sh`. `docs/testing.md` says what counts as a use.

### Added (2026-11-16, clouds at half resolution, off by default)
- **`CloudSettings::half_resolution` marches a quarter of the pixels** (`render/forward/cloud_pass.rs`,
  `shaders/clouds/cloud_march.slang`): the march writes a target half as wide and high, each texel marching to the
  nearest scene depth of the 2x2 pixels it covers, and `fs_upsample` fills the full-size cloud target from the four
  nearest texels, weighted bilinearly and by `cloud_upsample_depth_weight` (`volumetric_clouds.slang`: the relative
  distance difference, so a texel across a depth edge counts for nothing). The tonemap and bloom read the full-size
  target as before; such a frame marches once per pixel, without the MSAA spread. Off by default, so no image moves;
  the Sky tab's CLOUDS section has the switch. `dump_gpu_timings` takes `--clouds` and `--half-res-clouds` (the camera
  inside a thin box, so every pixel marches).
- **Measured on lavapipe** (1280x720, 96 frames, the `Clouds` span covering march and upsample; another build shared
  the host, so absolute times are loose, the ratios less so): the camera inside a thin box over `cube_on_plane`
  320.7 to 65.6 ms (-80 %), the image 0.002 levels apart on average; the default slab seen from below at a grazing
  angle 356.8 to 108.0 ms (-70 %), mean 0.87 levels, 7.5 % of pixels more than 2 levels and 0.40 % more than 8 apart,
  at most 25; the dense low slab of `enabling_clouds_changes_the_frame_and_adds_detail` 146.7 to 54.8 ms (-63 %), mean
  1.18 levels, 11.3 % over 2, 2.9 % over 8, 0.17 % over 24, at most 40, where the fine noise's thin streaks show
  stair steps. The default stayed off for the owner to decide; it is on since the owner's decision the same day.
- Tests: `half_resolution_clouds_keep_off_the_wall_and_close_to_the_full_march` (`tests/clouds.rs`, 128x128): no cloud
  over a wall in front of the box, the sky just beside it keeps 1.01 of the full march's alpha (a plain bilinear
  upsample: 0.85), the shown frame 0.05 levels apart on average, none over 8; switching it off restores the full
  march exactly. `gpu_timing.rs` times the deferred run at half resolution.

### Added (2026-11-16, the wind moves the clouds)
- **The noise drifts through the cloud box with time** (`sample_density`, `shaders/common/volumetric_clouds.slang`):
  `cloud_unpack` reads a wind offset from `cloudLightMarch.yzw`, which the C++ engine packs as zero, and the noise
  lookup subtracts it; the box stays put. The host packs wind velocity times time, wrapped in f64 to the noise's
  256-unit period (`CloudSettings::wind_offset`, `NOISE_PERIOD`), so a long run keeps f32 precision and an offset of
  zero is today's image. `ForwardRenderer::cloud_time_seconds` is the clock: the app sets it from its own each frame,
  everything else leaves it at 0, so every test and snapshot stays deterministic. The default wind is (4, 0, 1.5)
  units per second (a fine noise tile every 15 s); the Sky tab's CLOUDS section has a Wind row. The march costs one
  subtraction more.
- Tests: `the_wind_moves_the_clouds_with_time_and_a_pinned_time_repeats` (`tests/clouds.rs`): time 0 draws the still
  cloud bit for bit, a pinned time repeats its frame, 20 units of wind move the alpha of 1671 of the box's 1792 pixels
  by more than 0.02 with no cloud outside the box, and 256 units later the frame is the same again;
  `the_wind_wraps_at_a_period_both_noise_lookups_share` (`cloud_constants.rs`), the offset's unpack
  (`cloud_math.rs`), its wrap after a day (`render::clouds`) and the app's clock (`app::frame`).

### Changed (2026-11-16, Scale alone sizes the cloud box)
- **Density no longer resizes the cloud box** (`cloud_unpack`, `shaders/common/volumetric_clouds.slang`): the half
  extents are `cloudMeshScale.xyz * 10`, where they were that times Density (`cloudMeshScale.w`), so a thinner cloud
  shrank its box with it. Density still multiplies every noise sample. The default Scale becomes (630, 3.15, 630), the
  old (1000, 5, 1000) times the default Density 0.63, so the default slab keeps its 6300 by 31.5 half extents, and
  `CloudSettings::place_box` takes the half extents without Density: no test image moves. The C++ engine packs the
  same block, so its boxes grow by 1/Density with the next pin bump until its default Scale and `place_cloud_box`
  follow (`BACKLOG.md`).
- Tests: `density_and_scale_change_apart` (`tests/clouds.rs`): a quarter Density keeps all 1728 clouded pixels of a
  box (0 change; the old unpack moved 1648) while its alpha falls from 171 to 45, and a box 1.5 times as wide changes
  no ray it does not lengthen (alpha bit for bit, inside the back face's outline); `cloud_math.rs` unpacks one
  Scale at two Densities to one radius.

### Changed (2026-11-16, one path estimator for both path tracers)
- **`shaders/tracer/path_estimator.slang` moves to `shaders/common/path_estimator.slang`**, so the C++
  `path_tracing.slang` can import it with the next pin bump instead of keeping its own copies. Russian roulette
  (`russian_roulette_survives`, was `traced.slang`'s `survives`) and the per-pixel seed (`path_rng_seed`) join the
  RNG, the basis, the visible-normal GGX sampling, its pdf, the lobe probability and `sample_bounce`. Diffed against
  the C++ kernel: RNG constants, seed, basis, sampler, pdf, lobe probability, clamps and roulette are the same; the
  C++ bounce still weights by `brdf_direct`'s analytic Smith remap, which over-masks a sampled lobe, so its goldens
  move when it takes the shared `sample_bounce` and `brdf_sampled`. The emitted `traced.wgsl` changes only by those
  two names.
- Tests: `shaders/tests/path_estimator_test.slang` calls every function and emits to SPIR-V and WGSL.

### Changed (2026-11-16, the NVIDIA promotion test checks its contract per job)
- **`scripts/linux/tests/test-nvidia-gpu-promotion.py`: `valid` (171 lines, cc 21) and `test_wiring_and_mutations`
  (156) fall under the limits**, and their rows leave `function-size.allow` and `code-complexity.allow`. `valid` cuts
  the sections once (`jobs_of`) and asks one checker per concern: the callers, the matrix outputs, the attempt guards,
  trigger and select, the producer, clean-host, gpu-profile and stage. The mutation tables are module constants and
  `assert_each_occurrence_rejected` mutates one occurrence at a time. The old and new `valid` agreed on 574 workflow
  variants (the mutation tables plus every single-line deletion of both workflows).

### Changed (2026-11-16, the OBJ converter's two largest functions are split)
- **`asset/obj_to_gltf.rs`: `to_gltf` (281 lines) and `parse_obj_with_materials` (171) fall under the 80-line limit**,
  their rows leave `rust-function-size.allow`. `to_gltf` assembles the document from `write_buffer`,
  `primitives_json`, `image_uris`, `texture_arrays_json`, `materials_json` (one `material_json` per material, with
  `metallic_factor_json`, `roughness_factor_json` and `emissive_json`) and `color_json`; the parser keeps its
  keyword match and hands each directive to an `ObjParser`. `material_texture_uris` replaces the three copies of the
  texture-slot list. The output is byte-identical to the old code's on 17 OBJs (with and without materials, the
  error messages included); the tests are unchanged.

### Fixed (2026-11-16, a path-traced bounce masks with the Smith remap for sampled light)
- **The path tracer's bounces take Epic's `k = alpha / 2`** (`brdf_sampled`, `shaders/common/brdf.slang`;
  `sample_bounce`, `shaders/tracer/path_estimator.slang`). The bounce weight evaluated `brdf_direct`, whose
  `k = (r + 1)^2 / 8` is meant for analytic lights and over-masks a sampled lobe at grazing angles. A white metal card
  in the furnace, roughness 0.3: 0.974 to 0.987 at `mu` 1 and 0.668 to 0.899 at `mu` 0.5 (the exact height-correlated
  Smith lobe: 0.991 and 0.975); roughness 0.7: 0.638 to 0.671 and 0.524 to 0.613 (exact 0.698 and 0.704). NEE and the
  raster keep `brdf_direct`, bit-identical in the emitted WGSL; the shadow rig's furnace mean moves from 0.987 to 0.994.
- **New in `brdf.slang`, `brdf_direct` and `geometry_smith` unchanged**: `smith_k_analytic`, `smith_k_sampled`,
  `geometry_smith_k` and `brdf_smith_k` (the BRDF with a given remap), and `brdf_sampled`. The C++ path tracer's
  bounce (`path_tracing.slang`) should switch to `brdf_sampled` with the next pin bump, and its furnace oracle with it.
- Tests: the furnace quadrature takes the remap (`Smith`), the cards expect `brdf_sampled`'s E(mu), and the metal card
  must land closer to the exact Smith lobe than the analytic remap (failed before at roughness 0.3, `mu` 1: 0.974
  against the remap's 0.976 and the exact 0.991). The dielectric card at roughness 0.25, `mu` 0.5 now reads 1.024
  (E(mu) 1.022, against the analytic remap's 1.004): Epic's diffuse does not shrink with the masking.

### Fixed (2026-11-16, PCF taps compare at the receiver plane's depth)
- **A receiver sloped against the light no longer shadows itself at larger PCF radii** (`shadow_visibility`,
  `shaders/forward/forward_lighting.slang`). Every tap compared against the centre's depth with one bias, so an outer
  tap a few texels away read the receiver's own nearer depth as an occluder. On a lone ground plane under 15, 30 and 50
  degree suns (2048 texels, 3 cascades), the pixels darkened by 3+ levels were 0, 0 and 0 at radius 2, 36893, 905 and
  743 at radius 3, and all 63994 at radius 5; now 0 at every radius. Radius 0 is bit-identical.
- **The kernel is shared**: `shaders/common/shadow_pcf.slang` (`receiver_plane_depth_slope`, `IShadowTaps`,
  `pcf_visibility`). The slope comes from the shading normal and the affine light-space matrix's cofactor rows, not
  from derivatives, which the deferred pass cannot take; it stops growing past 84 degrees to the light axis
  (`PCF_MIN_RECEIVER_COS`). The C++ `cascaded_shadow.slang` can import it with the next pin bump;
  `shaders/tests/shadow_pcf_test.slang` emits it to SPIR-V with a combined `Sampler2DArrayShadow` as that file binds.
- Tests: `a_sloped_receiver_stays_lit_at_every_pcf_radius_and_shadows_still_darken` (a lone ground at radius 0, 2, 3
  and 5 under three suns, and a card's shadow on it: 191 to 624 pixels at every radius). Controls: a zero slope
  reproduces the old counts exactly; the uv y sign flipped darkens the whole ground from radius 2.

### Fixed (2026-11-16, the cloud noise tiles, so no seam crosses a cloud box)
- **Every channel of the noise volume tiles** (`noise_volume_texel`, `shaders/common/volumetric_clouds.slang`). The
  march wraps box-local coordinates with `frac`, so a channel that does not continue across the volume's faces showed
  a faint cross through the box centre, as looking up at the default slab. `fbm_value` takes x's period and
  `valueNoise` wraps its lattice cells into it per octave (`wrap_cell`), each octave hashing the cells the old
  `x * 2 + 100` recurrence hashed. Worley did not tile either: it wrapped neighbour cells with `fmod`, which keeps the
  dividend's sign, so cell -1 was not cell `numCells - 1`. Across a face the mean step was 7 to 27 times an inner texel
  step for all four channels (0.10 to 0.17 against 0.006 to 0.031); now within 1.1 times, and below it for the fBm
  channels, whose lattice points meet at the face.
- **The box centre reads the volume's middle** (`sample_density` adds 0.5 inside both `frac`s). The tiled noise at the
  face, where every small box sat, held about two thirds of the old density (a CPU replica's mean over the tests'
  16 x 10 x 8 box: 0.026 against 0.039); the middle, which tiling leaves as it was, holds 0.037. Without the shift five
  cloud tests found too little cloud to judge.
- **The C++ engine reads both**: its `noise_volume.slang` writes `noise_volume_texel` and its march calls
  `sample_density`, so its cloud goldens move with the next pin bump (`BACKLOG.md`).
- Tests: `every_noise_channel_tiles_across_the_volume_faces` (each channel's step across each face under twice its
  inner step; all twelve failed before). `a_daylit_cloud_is_not_much_darker_than_the_sky_behind_it` marches density
  1.5, as its box at 1.0 now holds 604 dense pixels, under its floor of 819; it shows 77 % of the sky (79 % before).

### Changed (2026-11-16, ObservANT's capture loop keeps its camera apart from its logic)
- **`crates/observant/src/capture.rs` holds only what needs a camera**: the worker, its pipelines, the bus and the
  inference thread. `capture/logic.rs` holds the rest, which a test drives with set values: `Timing`, `Backoff` and
  the pause, the boxes' lifetime, `Visits` (now built by `Visits::new` from the config) and `counted`, the arrival
  photo, and the frame geometry (`capture_size`, `I420Layout`, `isp_scales`, `flip_method`, `exact_frame`).
  `capture::Timing` stays where `main.rs` names it. The tests of the moved code moved with it, unchanged; the
  `cargo mutants` row and file list in `docs/testing.md` name both files.

### Fixed (2026-11-16, the clouds are lit by the sky and no longer grainy at a grazing angle)
- **The shared estimator scatters the sky as ambient** (`shaders/common/volumetric_clouds.slang`). It lit a cloud by
  the sun's single scattering alone, so dense cloud showed 48 % of the luminance of the sky behind it (the default sun
  and procedural sky, a box at the horizon, linear HDR). A sample now also scatters `CloudAmbient`, the drawn sky's
  mean radiance over the upper and the lower hemisphere, blended by its height in the box: a conservatively scattering
  medium under uniform sky light shows that sky's radiance. The host fills it per sky mode
  (`render::clouds::CloudAmbient`): the procedural gradient's per-hemisphere integrals in closed form, a cube map's
  faces sRGB-decoded and untonemapped as the sky pass draws them (64 texels a face edge at most), or the navy; a
  path-traced frame always takes the procedural one, which its camera misses keep. The sun
  adds Wrenninge et al.'s scattering octaves (three, each halving energy, light depth and g), and each step's
  in-scattering is integrated exactly (`1 - exp(-density * dt)`), so a long step cannot overshoot. The same box now
  shows 79 %; with powder off 109 %.
- **A long path through the box takes more steps.** The slider's count spans the box's thinnest extent; a longer path
  takes proportionally more, at most four times the slider's and 128 (`cloud_march_steps`), so the default 8 become
  at most 32. Powder reads the slider's step through that extent instead of the step taken, so the count does not
  change it. Looking up through the default slab at 3 to 34 degrees, neighbouring pixels differed by 42 % of the
  cloud's luminance (8 steps) and 31 % at 128; now 4.5 % (128 steps: 2.0 %), and alpha's neighbour roughness went
  from 1.58 to 1.11 times the 128-step march's. Sampling the coarse noise in place of the fine one on long steps was
  measured too and dropped: at a fixed count it took alpha from 1.58 to 2.2 times the 128-step roughness.
- **The C++ engine keeps its call.** `march_clouds` keeps its seven-argument overload, which lights by the analytic
  sky; the eight-argument one takes `CloudAmbient`. `light_march` became `light_optical_depth`, which the octaves need.
  The engine's cloud goldens move with the next pin bump (`BACKLOG.md`).
- Tests: `a_daylit_cloud_is_not_much_darker_than_the_sky_behind_it` (above 70 %),
  `a_grazing_view_through_the_slab_is_not_grainy` (luminance roughness under 15 %, alpha within 1.25 times 128
  steps'; 1.58 with the step count fixed), `the_cloud_ambient_is_the_drawn_sky` (the cube map's and the navy's
  ambient against the HDR sky the pass drew), `a_longer_path_takes_more_steps_within_a_bound` (`cloud_math.rs`), and
  the closed forms against the panorama's integral and a solid cube map (`render::clouds`).
  `clouds_appear_when_enabled_and_vanish_when_disabled`, `geometry_in_front_of_the_clouds_hides_them` and
  `feature_matrix.rs`'s cloud scene now look at the navy, as the C++ goldens do, the first and the last without
  powder: over the horizon, the procedural sky's brightest band, an ambient-lit cloud changed only 191 of 2992
  pixels by more than 5 levels. The path-traced frame keeps that horizon, so the matrix checks its march against
  forward's alone there.

### Fixed (2026-11-16, the headless GPU tests' intermittent crash has a cause on arm64)
- **llvmpipe at 256-bit vectors corrupts the heap on aarch64.** Looping every renderer GPU test binary at
  `--test-threads 4` in the image on the arm64 dev host (Mesa 26.0.8, LLVM 21.1.8): at the image's
  `LP_NATIVE_VECTOR_WIDTH=256`, 6 of 270 binary runs crashed (`occlusion` 4 of 10), plus 4 in about 50 under gdb;
  at 128, none of 405, nor `occlusion` 100 times at 14 threads. The `occlusion` crash of 2026-10-22, after 7 of 11
  cases, has this shape; its width was not recorded. The stacks: a SIGSEGV in
  `llvm::SelectionDAG::createOperands` under MCJIT on an llvmpipe thread, and glibc's `double free or corruption` and
  `free(): invalid size` when a test thread frees its arena on exit. Serially, 25 runs each of `deferred` and
  `occlusion` at 256 passed: it takes concurrent compiles at 256 bits, which points at a race in LLVM's AArch64 backend.
- **The tests refuse that adapter.** `GpuContext::headless_or_skip` checks the adapter before it makes a device and
  refuses llvmpipe wider than 128 bits on aarch64 (`check_test_adapter`), naming `LP_NATIVE_VECTOR_WIDTH=128`: under
  `KATAGLYPHIS_REQUIRE_GPU` every GPU test fails at once instead of a binary crashing one run in a few. CI already
  exported 128 on arm64; a bare `cargo test` in the image did not. The app keeps the adapter.
  Afterwards, 405 more binary runs at 128 passed, and at 256 all 270 failed at once with that message, none by a signal.
- Tests: `llvmpipe_wider_than_neon_is_refused_on_aarch64_only` (256 refused on aarch64; 128 there, 256 and 512 on
  x86_64 and a hardware adapter pass). The amd64 crash of 2026-10-19 stays unreproduced (BACKLOG P3).

### Changed (2026-11-16, the traced scene uploads over frames and frees its CPU geometry)
- **The first upload of a traced scene spreads over frames.** `TraceUpload` (`render/tracer/upload.rs`) makes every
  buffer and texture array at once, then writes at most `TraceBuildOptions::upload_bytes_per_frame` a frame
  (`UPLOAD_BYTES_PER_FRAME`, 64 MiB: the full Colosseum's half gigabyte in about eight frames, not one), in pieces of at
  most 64 K entries or one mip level; the frame rasters meanwhile and `TraceStatus::Building` reports
  `TraceStage::Uploading` with the share written. `set_instances` during the upload restarts the build as during the
  build, and a model transform set meanwhile reaches the top level when the upload ends. `TraceGpuScene::upload` and
  `finish_build` still upload in one go.
- **After the upload the CPU keeps only what the top level is rebuilt from.** `TraceScene::release_geometry` drops the
  triangles, positions, packed vertices and texels (the full Colosseum: about 175 MiB of nodes stay of about 480 MiB),
  unless a mesh deforms, which re-poses from them, or `TraceBuildOptions::keep_cpu_geometry` keeps them for CPU queries;
  `triangle_count`, `vertex_count` and the stats keep the counts, and a recount keeps the uploaded layers' bytes.
- Tests: `tests/tracer.rs` uploads a textured cube at 256 bytes a frame over 8 frames, untraced until the last, then
  finds its CPU geometry gone, its nodes and stats equal to a one-shot upload's and every one of 1600 rays hitting the
  same triangle on both GPUs and on the CPU; a skinned bar keeps its geometry. Mutants: ignoring the budget, never
  writing the triangles and never releasing each fail it.

### Fixed (2026-11-16, no tofu in egui's debug windows)
- **egui's Style editor and inspection windows draw their symbols.** Two subsets of the fonts egui itself ships
  (epaint_default_fonts 0.37.0) follow Roboto and Cousine in both families: Noto Emoji (OFL-1.1) cut to the 18 symbols
  of those windows it has, and emoji-icon-font (MIT) cut to the 8 it lacks (`assets/fonts/NotoEmoji-Subset.ttf`,
  `EmojiIcon-Subset.ttf`, 11 KiB together). `assets/fonts/NOTICE` records each source, its hash, the `pyftsubset`
  command that reproduces the subset and the MIT text, so the files BeschleunigerBallett's docs build stages beside
  the demo (`NOTICE`, `OFL-1.1.txt`, `Apache-2.0.txt`) still carry every licence.
- Tests: `ui::fonts` renders both windows and finds no character without a glyph (the old chain drew 12 as tofu),
  checks the 26 symbols in both families, and pins both subsets' hashes to the ones `NOTICE` records.

### Fixed (2026-11-16, egui text weighs natively what it weighs in the browser)
- **egui paints through the output's linear twin.** On an sRGB surface egui's coverage blended in linear light, so
  text came out a different weight than on the browser's non-sRGB canvas. The surface now carries its non-sRGB view
  format (`view_formats`, where the adapter has `SURFACE_VIEW_FORMATS` and `VIEW_FORMATS`), `GpuContext::ui_format`
  names it, and `StudioApp` paints the scene through the sRGB view and the editor through the linear one
  (`Overlay::format`), also offscreen (`create_viewable_2d_texture`). Without the flags it paints as before.
- Tests: `app::frame` `editor_text_on_an_srgb_target_weighs_what_it_does_on_a_linear_one` compares the menu bar of an
  sRGB and a linear offscreen frame: identical (913887 levels of ink on both, worst texel 0), where the old path left
  884632 against 913887 and a texel 30 levels off.

### Added (2026-11-16, F5 reloads the post chain)
- **`reload_shaders` rebuilds every pass of the frame.** `ShaderSources` carries twelve files (`FILES`, read in order by
  `ShaderSources::from_files`): beside the scene, sky, cloud and traced WGSL now the tonemap, FXAA, bloom, SSAO,
  histogram and depth resolve. The code moved to `render/forward/reload.rs`, compiled in one validation scope and
  installed only when it passes. Bloom, SSAO, the histogram and the depth resolve swap their pipelines
  (`compile`/`replace_pipelines`); the tonemap pair is compiled there once and kept as `TonemapShaders`, which every
  `TonemapPass` the renderer draws with rebuilds from on its next frame (`TonemapPass::use_shaders`), so the window's
  pass and an F12 screenshot's fresh one (`TonemapPass::with_shaders`) show the same reloaded tonemap.
- Tests: `tests/post_reload.rs` refuses each of the six post shaders without its entry point and keeps the frame,
  and takes each valid edit: a flat tonemap in the screenshot and the window's pass, then the shipped one back; FXAA,
  a constant resolved depth, a shifted manual exposure, a blue bloom and full occlusion. Mutants: skipping
  `use_shaders` and dropping the resolve's new pipeline each fail one test.

### Fixed (2026-11-16, clouds reach the pixels a silhouette half covers)
- **A silhouette pixel marches to both of its depths.** The forward path's depth resolve
  (`shaders/depth_resolve/depth_resolve.slang`) writes, beside the nearest sample, the farthest one and the share of
  samples nearer to it than to the nearest into `depth_spread` (`Rg32Float`, 8 bytes a pixel). Where they differ the
  cloud march (`shaders/clouds/cloud_march.slang`) marches a second time to the farthest and mixes the two results by
  that share, so a pixel half sky behind an edge no longer shows a one-pixel rim without cloud. Deferred and traced
  frames have no MSAA and march once (`depth_spread.x` in the cloud uniforms). Slang now emits the resolve's
  `@builtin(frag_depth)` from the output struct itself, so the manifest's retargeting patch went.
- Tests: `tests/clouds.rs` `silhouette_pixels_in_front_of_a_cloud_are_clouded` reads the cloud alpha on the columns
  a wall's left and right edges cut: 0.107 against 0.162 beside them, where the old march left all 54 at 0;
  `shader_export` pins the resolve's one colour output beside its depth.

### Fixed (2026-11-16, bloom no longer glows through clouds)
- **Bloom's bright pass thresholds the clouded scene.** The clouds march before bloom now, and `fs_brightpass`
  (`shaders/bloom/bloom.slang`) composites their premultiplied target over the HDR first, as the tonemap does, so a
  sun or lamp they hide spills no halo past their edge. The tonemap no longer scales bloom by the transmittance. The
  cost is one more sample per half-resolution texel; `BloomPass::rebuild` takes `BloomInputs` with the cloud view, and
  `TimedPass::Clouds` reports before `Bloom`.
- Tests: `tests/clouds.rs` `a_light_the_clouds_hide_does_not_bloom_around_them` hides a glowing card in a dense box:
  the halo outside the box falls from 14100 colour levels to 110 (the old shaders: 2362 of 2362 in the first layout,
  and the bright pass with its cloud flag ignored, 14100 of 14100); the graph test puts the clouds before bloom.

### Added (2026-11-16, a new `set_instances` count reaches the built traced scene)
- **`set_instances` with a new count re-instances a built traced scene** instead of waiting for the next upload. Each
  instanced primitive keeps a template (`TraceScene::templates`, moved with its node), and `set_instance_extras`
  rebuilds the top level from it, returning `InstanceChange::{Moved, Recounted, Refused}`. A new count gets new records
  and node-maxima buffers, a new scene uniform and bind group (`TraceGpuScene::write_records`); the meshes' nodes stay
  on the CPU and the vertex, triangle and texture buffers on the GPU. A primitive merged into the world-space mesh is
  still refused and flagged by `instances_current()`, now per primitive.
- Tests: `tests/feature_matrix.rs` adds a second traced cube where raster adds it (the centroid shift within 0.05 px
  of raster's) and goes back to one, and a merged cube stays put, flagged; keeping the old bind group after a recount
  draws no cube at all. On the CPU three, then moved, then one instance trace as brute force over unchanged meshes.

### Added (2026-11-16, the traced scene follows skins and morphs)
- **A skin whose joints an animation moves, or a morph whose weights one drives, traces as its own mesh** and follows
  the raster's pose: `Tracer::pose_deformed`, called with the animation's node matrices and each primitive's weights,
  skins and blends the rest vertices again on the CPU as `forward.slang` does, refits the mesh's BVH bottom-up in
  place (`bvh::refit`) and writes only its vertices, its nodes and the top level (`TraceGpuScene::write_meshes`). A
  pose change bumps the tracer generation, so the path tracer's mean restarts. Skins and morphs no animation moves
  still merge at their first pose.
- Tests: `skinned_bar.gltf` bent in `tests/traced_modes.rs` moves its traced upper half by 14.5 px against raster's
  14.6; without the re-pose the traced bar stays (65.4 px against 48.9), without the refit it lags (60.8). On the CPU
  a bent skin and an animated morph trace as brute force, every node bounding its children; a skin no animation
  moves merges.

### Changed (2026-11-16, lavapipe traces the full Colosseum)
- **A node's and a packed vertex's two 16-byte halves bind apart** (`tracerNodeMax`, `tracerVertexExtras` at
  bindings 9 and 10 of the scene group), so every tracer buffer holds 16 bytes an entry. The full Colosseum needed
  175 MiB of records and 143 MiB of vertices, past lavapipe's and the browser's 128 MiB per storage binding; now its
  largest buffer is the triangles' 90 MiB, and a browser scene holds about 8.4 M stored triangles, vertices and nodes.
  The scene binds 6 storage buffers, so the modes' kernels use all 8 of WebGPU's default per stage. Uploads write the
  split buffers 64 K entries at a time. `TraceBufferSizes` names six buffers; `check_buffers` checks counts alone.
- Tests: a scene behind 4.3 M padding nodes and vertices traces and decodes on lavapipe as on the CPU (writing every
  upload chunk at the first chunk's offset fails it, and only it); the limits test at the full Colosseum's counts;
  `the_full_colosseum_traces_on_this_device`, ignored by default (`docs/testing.md`): 1600 of 1600 rays agree with
  the CPU traversal on lavapipe, whose binding limit is 128 MiB, in 84 s unoptimised.

### Added (2026-11-16, the path tracer's punctual lights and environment)
- **The path tracer lights with the scene's `KHR_lights_punctual` lights.** Each segment picks one light uniformly
  and divides its term by the pick's chance, 1 / count, through one shadow ray that stops just short of the light, so
  a punctual light casts a traced shadow; the raster's punctual lights stay shadowless. The falloff, KHR range window
  and spot cone moved into `shaders/common/punctual_light.slang`, which `forward_lighting.slang` and
  `trace_shading.slang` both import (`forward.wgsl` regenerated, the same arithmetic). The kernels bind the raster's
  light buffer itself and the environment cube at a third group (`TRACE_LIGHTS_GROUP`), 6 of WebGPU's 8 storage
  buffers per stage. The ray-traced mode keeps no punctual lights, as the C++ one has none.
- **A bounce that misses sees the loaded environment** (`set_environment`) at mip 0, times the ambient strength and
  the environment's intensity as the raster's IBL ambient takes it; without one, the gradient as before. The camera's
  misses keep the procedural sky. The accumulation starts over when a light changes (a hash of the packed lights) or
  an environment is set or cleared (`AccumulationKey::lights`, `environment`).
- `tests/traced_lights.rs` on lavapipe: two ranged point lights lit where raster lights them (IoU 0.976 of the pixels
  over a tenth of the peak), nothing past their range windows, the centre 0.55 % under the analytic radiance (raster
  0.08 % over); a spot cone cut where raster cuts it (IoU 1.0); the ground behind a wall black in the traced frame
  where shadowless raster lights it (0.0000 against 0.0969), beside it within 2 %; a white card under a constant
  coloured environment at 0.994 of its colour in every channel. Mutations, each failing its test: dropping the
  1/count (centre 0.390 against 0.784), the shadow ray (0.0961 behind the wall), the environment sample (the card at
  0.36, 0.80 and 1.99 of the environment). `tests/traced_modes.rs` adds the environment to the reset premises.
- **One shadow-ray call site in the path tracer.** SwiftShader inlines a whole traversal per call site: with one
  `trace_any` call for the sun and one for the punctual light, the web demo's path-traced frame did not arrive within
  the browser test's 300 s. `next_event` now traces both from one loop (`LightSample`, `shadowed`): 117 s, against
  124 s before the punctual lights, and the same verdicts (light 165.7, tint 28.3).

### Changed (2026-11-16, the browser test judges the traced modes by their shadows)
- **A traced frame must draw its own shadow, not just change 1 % of the viewport**, which the ray and path tracers
  cleared by 2.6 and 3.2 %. In the cube's sun shadow, which the ray tracer leaves black for want of an ambient term,
  the ray-traced frame must hold 0.05 % of the viewport at every channel under 48 and ten times forward's count; the
  path-traced frame must be sky-lit there (mean brightest channel 80) and tinted by the cube's red bounce and the
  blue sky (mean (r + b) / 2 − g 10). Measured on SwiftShader at 960x528: a black shadow of 0.174 % (forward none),
  165.7 bright and tinted 28.3 at 88 samples (forward 0.8, the ray tracer 32 bright). Clouds keep their 1 % floor
  against deferred (33.0 %). `webgpu-demo-browser.js` exports `shadowSignature`; `test-webgpu-demo-browser.sh` checks
  it and the new verdicts.

### Fixed (2026-11-16, clippy on the renderer's wasm32 build)
- `cargo clippy -p kataglyphis_webgpu_renderer --target wasm32-unknown-unknown -- -D warnings` failed on
  `large_enum_variant`: the browser's tracer job holds its whole `TraceSceneBuilder` inline, so `State::Building`
  dwarfed the other variants (`render/tracer/job.rs`). It boxes the job now. The `fmt-clippy` step runs that
  invocation after the workspace clippy, so the browser-only code is linted in CI: 1 min 49 s from a cold target
  directory at two jobs.

### Added (2026-11-16, FXAA on the deferred path)
- **The deferred path is anti-aliased.** `render::fxaa` runs FXAA 3.11's quality edge walk (`shaders/fxaa/fxaa.slang`,
  the generated `fxaa.wgsl`, a manifest row) after the tonemap: on a filtered frame the tonemap draws into an
  intermediate of the output's format and FXAA filters that into the output. Luma comes from the encoded values on the
  web canvas and from the square root of the linear ones an sRGB target hands back. `ForwardRenderer::fxaa`
  (`FxaaMode`): `Off`, `Deferred` (the default: deferred raster frames, which shade one sample) and `Always` (forward
  on top of its 4x MSAA, and the traced modes). The Renderer tab's FXAA row (live in every mode) and
  `RendererSettings::fxaa` through `Default`, `of` and `apply`. `TonemapPass::prepare_fxaa` builds it the first
  filtered frame and reallocates the intermediate on a resize.
- **Measured** (`tests/fxaa.rs`, lavapipe, 256x256): a dark card whose edge runs 8.6° off vertical against the sky.
  One sample per pixel leaves no blended pixel in any of 128 rows; FXAA blends all 128 on an sRGB target and 102 on
  the web's `Rgba8Unorm` (forward's MSAA 74 and 75) and cuts the edge band's squared luma steps from 3.11 M to 1.80 M
  (MSAA 2.36 M). No pixel more than two from a step moves by more than one level, and forward frames at the default
  stay bit-identical to FXAA off. On cube_on_plane the deferred frame's gradient energy goes from 1.42 times forward's
  to 0.83, and its mean channel difference to forward within two pixels of an edge from 2.58 to 2.28 levels, 0.02
  elsewhere either way. Headless Chrome draws the deferred step with FXAA (1474 colours, was 1099).
- `TimedPass::Fxaa` (index 14, profiler label "FXAA") after `Tonemap`; `name()` stays first and its existing arms
  unchanged. **BeschleunigerBallett's `Compare-RendererTimings.ps1` must treat `Fxaa` as optional** beside `GBuffer`,
  `DeferredLighting`, `Clouds`, `RayTrace`, `PathTrace` and `TraceResolve`: a default `dump_gpu_timings` run draws
  forward, which FXAA leaves alone. The profiler drops the row at once when FXAA stops filtering, as on a switch of
  mode or path.
- `tests/deferred.rs`'s parity views turn FXAA off and keep comparing the shading: FXAA also smooths the texture steps
  MSAA keeps, which took cube_textured's mean channel difference to forward to 1.15 levels, over the 1.0 bound.
- Render graph: the tonemap writes `Tonemapped` and a last `fxaa` row reads it into `Output`, in every frame.
- Web demo: `demo_view()` reports `fxaa`, and the browser test fails a deferred frame without FXAA or a forward one
  with it.

### Added (2026-11-16, the web demo's every render mode drawn in headless Chrome)
- **A browser has drawn a traced frame, the deferred path and the clouds.** `scripts/linux/webgpu-demo-browser.js`
  opens the release wasm demo in the image's Chrome for Testing 157 on SwiftShader's WebGPU (the hub's
  `webgpu-canvas-readback.js` read-back and `check-docs-sidebars.js` launcher), turns the camera to 5°, and steps through
  forward, deferred, deferred with clouds, ray traced and path traced through two new wasm exports, `set_demo_view` and
  `set_demo_camera`, plus `demo_view()`, which reports the drawn mode (`drawn_mode`), the path, the clouds, the tracer
  and the samples (`app/web.rs`, `wasm_demo.rs`). Each frame is read back and must hold 256 colours with none over
  90 % in the scene viewport; clouds must change 1 % of it against deferred and a traced frame 1 % against forward;
  any console error or warning, browser log warning or uncaught exception fails the run. Measured: 3678 colours and
  32.9 % changed with clouds, 2.6 % ray traced, 3.2 % path traced at 72 samples, no WebGPU message; the five frames
  look as `editor_snapshot` draws them on lavapipe, so Tint and SwiftShader found no bug to fix. `ci-container-steps.sh
  web-browser` builds and runs it; `web-browser.yml` runs that on request (`[ci web]`, `[ci browser]`) or by hand and
  uploads the frames; `test-webgpu-demo-browser.sh` checks its verdicts and console filter
  ([testing.md](docs/testing.md#the-web-demo-in-a-browser)). Closes the P1 item "No browser has drawn a traced frame,
  the deferred path or the clouds".

### Fixed (2026-11-16, an OBJ opened in memory keeps its masks and its TGA and BMP textures)
- **An OBJ opened in memory drew its `map_d` cut-outs as solid cards and dropped every texture that was not PNG or
  JPEG** (Sponza's vase plant and chain). `parse_mtl` reads `map_d` into `ObjMaterial::alpha_texture`; `obj_to_glb`
  (`app/obj_glb.rs`) multiplies the base colour's alpha by the mask's red channel, as the C++ shaders test it, packs
  that as its own PNG and makes the material MASK at 0.5 (`ObjLoader.cpp`'s cutoff); a mask without a base colour
  texture composites over white. A texture glTF cannot carry is decoded by its extension (BMP, TGA; `image` gains
  those codecs and JPEG, no new crate) and packed as PNG. The file converter `convert_file` still writes no composite
  and warns per masked material. `tests/assets/obj_mask` is a card with `map_Kd red.bmp` and `map_d half_mask.tga`:
  `an_obj_card_with_a_map_d_mask_draws_only_its_uncut_half` keeps 1306 of the opaque card's 2627 red pixels (0 red
  before: the BMP was dropped), `a_map_d_mask_becomes_the_base_colour_alpha_of_a_mask_material` failed on the OPAQUE
  mode before, and `map_d_is_read_apart_from_d_and_the_other_maps` covers the MTL. This also covers
  BeschleunigerBallett's idea "`obj_to_gltf` drops `map_d`" for the editor's library, which opens OBJ through it.

### Fixed (2026-11-16, the camera GUI makes a lost surface anew)
- **`crates/gui` answered a lost surface with a configure only**, which wgpu 30 cannot satisfy natively, so the camera
  window stayed black after the platform dropped its surface. The renderer's rule moved from `context.rs` into
  `crates/webgpu_renderer/src/surface_recovery.rs` (`recovery` and `recreate_surface`), which the GUI includes by
  `#[path]`: a dependency on the renderer would bring its GL backend into the GUI. The GUI keeps the instance, window and
  adapter backend, recreates a natively lost surface (a browser's Lost still reconfigures) and retries a recreate that
  failed on the next frame. `a_lost_surface_is_made_anew_and_a_covered_or_stalled_one_left_alone` expects the
  recreate where the GUI's old mapping test expected a reconfigure.

### Fixed (2026-11-16, a BC texture on an adapter without BC keeps its texels)
- **A BC texture the device could not take became one white texel**, whatever the slot: a normal map then bent every
  normal to (1, 1, 1). `upload_material_texture` (`render/texture.rs`) now decodes BC1, BC3, BC5 and BC7 on the CPU
  (`asset/bc_decode.rs`) when the adapter lacks `TEXTURE_COMPRESSION_BC` or wgpu refuses the payload's size, keeps its
  mip chain, and uploads BC5 linear as the adapter would. A payload whose base level does not decode leaves the slot
  its neutral default (the flat normal for a normal map; `an_undecodable_normal_map_leaves_the_normal_flat` shaded 692
  against the flat 720 before). `a_bc5_normal_map_keeps_its_normal_on_an_adapter_without_bc`
  shaded 692 where the texels shade 641 before; `the_cpu_decode_of_every_bc_format_draws_like_the_adapters_own` draws
  each format's random blocks within 2 of lavapipe's own decode (10.3 apart on average for BC1 before), and
  `every_bc7_mode_decodes_like_the_basis_universal_reference` pins 4096 blocks per BC7 mode to basisu's `unpack_bc7`.

### Fixed (2026-11-16, the tile-light grid is conservative)
- **A point or spot light was cut off in tiles that listed other lights**: `render/tile_grid.rs` bounded its range
  sphere by the screen AABB of the centre and the six `pos ± range * axis` points, which under-covers the silhouette
  by up to `D / sqrt(D² - r²)`. The rect now comes from the sphere's tangents per screen axis (Mara & McGuire 2013),
  exact for the perspective lens and the docked viewport's remap, with a pixel of padding; a sphere reaching the eye
  plane takes the whole grid. `every_point_a_light_reaches_shades_in_a_tile_that_lists_it` (300 random cameras, 32
  lights with ranges from 0.01 to 50, 64 points inside each range) missed 4009 of 313052 on-screen points before;
  `a_sphere_is_bound_by_its_tangents_not_its_axis_points` pins one rect.
- **An empty tile iterated every light**, up to 256 per fragment, to hide that under-coverage. `forward_lighting.slang`
  now shades nothing there. An overflowing tile stores its true count past `MAX_LIGHTS_PER_TILE` (32, pinned against
  the shader by `max_lights_per_tile_matches_the_slang_constant`) and still shades every light.

### Fixed (2026-11-15, the browser demo's crate texts are reviewed)
- `packaging/cargo-license-policy.json` covers `kataglyphis_webgpu_renderer`'s graph for `wasm32-unknown-unknown`, the
  WebGPU demo BeschleunigerBallett's docs site ships; the hub's stager stopped at the first two gaps. `bevy_mikktspace`
  1.0.0 declares `Zlib AND (MIT OR Apache-2.0)` and ships only `LICENSE-APACHE` and `LICENSE-MIT`: its reviewed entry
  names the crate's own `src/lib.rs`, whose Copyright section carries Morten S. Mikkelsen's Zlib notice for the
  MikkTSpace code it reimplements (upstream has no Zlib file at the crate's commit 9de78a28 either).
  `kataglyphis_webgpu_renderer` takes the repository's `LICENSE` like every workspace crate, plus the texts of the
  two Dear ImGui fonts it embeds (`assets/fonts/`). The stager now stages 258 packages for that root and target;
  `test-cargo-license-policy.sh` stages it too.

### Changed (2026-11-15, the Colosseum releases are published)
- AssetANT published `Colosseum-gltf-v1` and `Colosseum-web-v1`; both archives match the SHA-256 that
  `resources/scenes/scenes.lock` pins, and `bash scripts/linux/fetch-models.sh --scenes --dest <dir>` unpacked both on
  a clean cache (276 MiB and 59 MiB). The P0 backlog entry is gone, and the roadmap's demo-scene row is done.

### Fixed (2026-11-15, the traced modes' fallback)
- **An output whose accumulation does not fit the device reports raster.** The frame rastered, but `drawn_mode()`
  still said traced, so the tonemap dropped SSAO and the chip, outliner and profiler named the traced mode. The fit is
  now part of the drawn mode (`accumulation_bytes` against the device's storage binding and buffer limits, for the
  current output), `ForwardRenderer::traced_fallback` gives the reason, and the chip reads
  `Raster  ·  Forward  ·  accumulation too large, rastering` with the reason in the switch's tooltip. A test caps the
  limit one byte below a 32 x 32 accumulation: the frame is the raster one with its SSAO, the drawn mode Raster, and a
  16 x 16 output traces again; without the fix it reads PathTraced and the frame lacks its SSAO.

### Added (2026-11-15, the C++ dielectric furnace checks)
- `tests/traced_modes.rs`: a lone white dielectric card at roughness 0.5 and 0.25, mu 1 and 0.5, within 0.01 of
  E(mu), as the C++ `PathTracingPassesTheWhiteFurnaceTest` grades it (measured 0.9938/0.9943, 0.9913/0.9910,
  0.9984/0.9995, 1.0041/1.0036). The furnace golden moved to the C++ shadow rig and grades the HDR mean against the C++
  0.98..1.002 (measured 0.987) and 98 % of pixels within 0.1 of the sky. A 1.5 % gain a bounce fails both: the rig
  reads 1.008, the first card 1.0085 against 0.9943.

### Fixed (2026-11-15, the tracer with the raster settings, deferred path and clouds)
- **A traced frame replaces either raster path.** The deferred G-buffer and lighting passes sit beside the forward pass
  and depth resolve inside the traced frame's skip, so a ray- or path-traced pick over `RasterPath::Deferred` runs no
  raster pass; `raster_path` applies while the frame rasters, and the chip, Renderer tab and Render menu name or
  enable it only then (`Raster  ·  Deferred  ·  BVH 42 %` while a traced pick waits).
- **The clouds march to the traced depth.** Both kernels write the primary hit's NDC depth into the accumulation's
  alpha (the ray tracer its hit, the path tracer an unjittered ray through the pixel centre, as the C++
  `raytrace.rgen.slang` and `path_tracing.slang` write theirs; 1 on a miss), and the resolve writes it into the
  single-sample depth target, so the clouds stop at traced geometry as they stop at raster's: the C++ engine's
  `cloudDepthSourceForThisFrame`. No storage binding is added. Before, a traced frame's clouds marched to whatever
  depth the last raster frame left, or the cleared target, and drew over a wall in front of them.
- **A ray-traced miss shows the sky pass.** The resolve's render pass draws the one-sample sky against the traced
  depth: the DOOM2016 cube map, the navy with the skybox off, or the procedural sky, as raster does; the path tracer
  keeps its own sky term, as the C++ one does.
- **The model transform and `set_instances` move the traced scene.** `Tracer` keeps the root
  (`set_root_transform` on every change, kept across a build, reset by an upload) and the explicit instances, which a
  build instances and a built scene moves in place when the count matches; each bumps the generation, so the path
  tracer's mean starts over. A new instance count waits for the next upload (`Tracer::instances_current`).
- **F5 rebuilds the traced kernels.** `ShaderSources` gains `traced` and `trace_resolve` (six files);
  `reload_shaders` builds both kernels, the resolve and the ray tracer's sky inside its error scope, though they
  otherwise compile lazily, so a module the device refuses keeps the old ones.
- **The profiler shows a traced frame's own passes.** A pass kept its average for 32 measured frames after it stopped
  running, and the traced rows came last, so a traced frame's profiler listed the raster rows above the fold for 32
  frames (minutes on lavapipe). A switch of drawn mode or raster path now drops the passes the new frames do not record
  (`GpuTiming::forget`, their stamps in readbacks still in flight too), and the rows follow record order; a test
  switches forward, ray, path, deferred, ray and forward and reads each frame's exact pass list after four frames.
- **A traced frame reads 0 / 0 in the Visibility tab** (`primitive_stats` too, as the C++ engine zeroes its meshes).
- **The traced shadow golden compares at full shadow strength**, as the other shadow goldens do since the C++
  default became 0.65 (IoU 0.935 against the view-fitted cascades).

### Added (2026-11-15, the tracer with the raster settings, deferred path and clouds)
- `tests/feature_matrix.rs` over the traced modes, picked over the deferred path (lavapipe, structural): clouds on in
  both modes leave a wall in front untouched (0 pixels) while 10743 and 10748 of 13332 sky pixels change, and the march
  over the open sky is bit-identical to forward's; the traced depth matches the deferred one within 4.2e-7 with no
  pixel disagreeing on coverage (108 against forward's four-sample resolve, all on silhouettes); a ray-traced miss
  shows each cube face, the navy and the procedural sky as raster does, a path-traced one its own gradient; the model
  transform set before the build and changed after it moves the traced cube by raster's shift (0.13 px apart) and
  restarts the mean; `set_instances` before the build and in place after it moves the traced cube as raster's, and a
  new count is flagged; each raster path and traced mode times exactly its passes with every feature on; raster frames
  on both paths are bit-identical after a trip through both modes, whose frames count 0 / 0.
- `tests/traced_modes.rs`: a reload refuses a kernel or resolve without its entry point before the path kernel ever
  compiled, keeps the frame, draws an edited kernel and comes back with the shipped one.
- `app::frame`: across a switch to path tracing the scene rect stays put for 40 frames while the profiler drops the
  raster rows, and the mean grows by one sample each frame (the scrolling pass list already keeps the dock's height).
- Editor: with a traced frame drawn, clicking the Renderer tab's or the Render menu's Deferred leaves the raster path
  alone; the profiler's value column fits a five-digit millisecond time (a path-traced Colosseum frame on lavapipe).
- `ForwardRenderer::read_depth_target` (tests and diagnostics), `traced_kernel_compiled`; `Tracer::set_instances`,
  `root_transform`, `instances_current`; `TraceScene::set_instance_extras`; `editor_snapshot` takes `spp=N` and
  `frames=N`.

### Changed (2026-11-15, the tracer with the raster settings, deferred path and clouds)
- `TimedPass::ALL` lists 14 passes in record order for either kind of frame: `RayTrace`, `PathTrace` and
  `TraceResolve` (indices 11 to 13, profiler labels "Ray trace", "Path trace", "Trace resolve") sit after
  `OcclusionCull`, where a traced frame records them; `name()` stays first and unchanged. BeschleunigerBallett's `Compare-RendererTimings.ps1` must treat `GBuffer`, `DeferredLighting`,
  `Clouds`, `RayTrace`, `PathTrace` and `TraceResolve` as optional, as it does `OcclusionCull`.
- `RendererSettings` carries `render_mode`, `trace` and `reset_accumulation` through `Default`, `of` and `apply`;
  `TraceFrameUniforms` gains `view_proj`; the render graph's traced frames keep the cloud rows.

### Fixed (2026-11-15, the raster settings, deferred path and clouds together)
- **The deferred path follows the raster settings.** The settings' shadow code (PCF radius, strength on the sun
  term, the C++ bias and its per-cascade scale, selection by view depth) lives in `forward_lighting.slang`'s
  `shadow_visibility`, which `fs_main` and `fs_deferred_lighting` share through `shade_surface`. The deferred sky is
  built on the cube-map sky layout, so the DOOM2016 faces and the switched-off navy show on both paths.
  `sync_shadow_map` also rebinds the deferred lighting pass's group 0 (`rebind_deferred_shadow_map`), which binds
  the shadow map too: without it the frame after a new resolution or cascade count sampled the old map, 32206 of
  65536 pixels off at 512 texels and one cascade. The G-buffer and lighting passes count the Visibility tab's
  primitives, blended ones included, as the forward pass does, and the clouds march on the depth the G-buffer pass
  writes as on the forward path's resolved one.
- **F5 rebuilds the cloud pipelines.** `ForwardRenderer::reload_shaders` takes `ShaderSources` (forward, sky, cloud
  march and cloud noise WGSL; `ShaderSources::shipped()`, `ShaderSources::FILES`) and builds the march and noise
  pipelines inside the same error scope as the others, even with clouds off, so a cloud shader that cannot build is
  refused and every old pipeline stays. Accepted, the march rebinds its target and the noise volume refills from the
  new shader on the next cloud frame; before clouds first run, that frame builds on the reloaded pipelines. The
  viewer watches and reloads all four files. Tests: a refused reload of either cloud shader leaves the frame and the
  volume byte-identical, an edited noise and march land in every texel, the shipped shaders bring the frame back
  exactly, and a reload with clouds off is what their first frame uses. The tonemap (and with it the cloud
  composite), bloom, SSAO, histogram and depth-resolve WGSL are built once and not reloaded.
- **A reload checks the deferred entry points before the first deferred frame.** The deferred pipelines are built
  lazily, so a reload on a forward-only renderer skipped them: a module without `fs_deferred_lighting` was accepted,
  and the first deferred frame then failed pipeline creation outside any error scope. `reload_shaders` now builds
  them in its scope either way and keeps them only once deferred has run (a test refuses that module on a
  forward-only renderer, then draws a clean deferred frame).
- **The Sun tab leaves an unlisted shadow resolution alone.** Showing it wrote 2048 over a resolution such as 600,
  which the renderer draws at its nearest listed size, 512; the row now selects that nearest size and writes only
  when clicked.
- **`editor_snapshot` aims every shot from the host's start view.** `zoom=` multiplied the radius left by the shot
  before and `pitch=` carried into later shots; both now apply to the view the host opened on (a unit test, which
  `cargo test` runs since the example is marked `test = true`).

### Added (2026-11-15, the raster settings, deferred path and clouds together)
- `tests/feature_matrix.rs`, every feature on both raster paths (lavapipe, structural): the shadow masks of forward
  and deferred at PCF radius 5, strength 1 and one cascade agree (IoU 0.994), the radius widens the penumbra on both,
  a new resolution and cascade count while deferred match a fresh deferred renderer within 10 pixels at 512, 4096
  and 1024 texels (32206 pixels off without the rebind), the cube faces and the navy show alike on both, clouds
  change the sky alike and leave a wall in front untouched (0 pixels) with a bit-identical march over the open sky,
  the model transform moves the cube by the same vector, the primitive, caster and occlusion counts match with
  culling on and off, and with every feature on each path reports exactly the passes it runs (the timings list
  each pass at most once and in `TimedPass::ALL` order, so this does not check the order they record in).
- The `RendererSettings` round trip writes a value away from its default in every field, so dropping any of
  `apply`'s assignments fails it.
- Editor tests over the merged rows (`ui::editor::tests::combined`): the Render menu's radios and culling switch,
  the Renderer tab's raster path and culling switch, the Sky tab's SKYBOX and CLOUDS switches, both outliner eyes
  and the Sun's cascade count, and the profiler scrolling to the last of eleven passes beside the culled counts.
- `graph` tests: each raster path's graph has a row for every pass it times and times every other row, and the
  clouds march after the depth writer and before the tonemap on both; `TimedPass` labels are unique.

### Added (2026-11-15, the volumetric clouds)
- The C++ engine's volumetric clouds in the WebGPU renderer, with the C++ bugs fixed rather than copied:
  `ForwardRenderer::clouds` (`render::clouds::CloudSettings`, the C++ ten controls, defaults and clamps), off by
  default. The box is the axis-aligned one the C++ tooltips describe: Offset moves it, Scale times Density times
  ten are its half extents, and the noise is sampled box-local, so moving the box carries its clouds. The march
  stops at the scene depth, so geometry in front hides the clouds and geometry inside cuts the march short, and the
  tonemap composites them premultiplied-over in linear HDR, before exposure and ACES, the same over sky and geometry.
- `shaders/common/volumetric_clouds.slang`, the cloud mathematics both renderers share: the noise volume's texels,
  `cloud_unpack` of the C++ `SceneUBO` cloud block, the box, density, Henyey-Greenstein phase, light march and
  `march_clouds`. They take an `ICloudNoise`, which Slang specializes per caller, so the C++ engine keeps its
  combined `Sampler3D` and WGSL takes a texture and a sampler. `shaders/tests/volumetric_clouds_test.slang` emits it to SPIR-V as the C++ engine will
  import it. A ray with a zero direction component no longer turns the slab test into NaN.
- `render/forward/cloud_pass.rs`: the 128³ `rgba16float` noise volume (a write-only 3D storage texture, the one
  way WebGPU writes that format) is made and filled by `cloud_noise.slang` the first frame clouds are on and kept;
  `cloud_march.slang` marches every pixel per frame in a full-screen pass that reads the single-sample depth (the
  forward path resolves it, the deferred G-buffer pass writes it), bound as unfilterable float so the WGSL needs no
  depth patch. `TimedPass::Clouds`, graph rows `cloud_noise` (untimed, it
  runs once) and `clouds`, and `TonemapPass`'s binding 6 with `params.z` as the clouds switch.
- The Sky tab's CLOUDS section (switch, March steps, Light steps, Density, Coverage, Pillowness, Cirrus, Powder
  effect, Scale on ImGui's logarithmic drag, Offset) with the C++ ranges, formats and tooltips, and the outliner's
  eye on Clouds, which was disabled. `editor_snapshot` takes `cloudy`, `pitch=DEG` and `zoom=F`.
- Tests: the C++ cloud goldens ported (`EnablingCloudsChangesTheFrameAndAddsDetail`, the device-survival frames,
  Offset moving the box, the depth, the composite) with stronger checks: the clouds stay inside the projected box
  and leave every other pixel's bytes untouched, switching them off restores the frame exactly, a box behind the
  camera or above the view draws nothing, the noise fills all four channels in all three dimensions, and every
  pixel over sky and over a card matches `cloud + hdr x (1 - alpha)`. On the adapter, `cloud_march.wgsl`'s box,
  phase function and unpack are checked against CPU references; `tests/cloud_constants.rs` pins the Rust bounds
  to the Slang literals.

### Changed (2026-11-15, the volumetric clouds)
- The profiler's GPU passes scroll once there are more than the dock shows.

### Added (2026-11-15, the deferred raster path)
- **A deferred raster path beside forward**, switched at run time like the C++ engine's Forward/Deferred control:
  `ForwardRenderer::raster_path` (`RasterPath`), `RendererSettings::raster_path`, the Renderer tab's *Raster path*
  segments and the Render menu's radios, and the viewport chip reads `Raster  ·  Deferred`. A G-buffer pass
  (`fs_gbuffer`) writes, at one sample, normal + metallic (`Rgba16Float`; a negative metallic marks
  `KHR_materials_unlit`), albedo + occlusion (`Rgba8UnormSrgb`) and roughness + emissive (`Rgba16Float`, so an
  emissive strength above 1 survives): 24 of WebGPU's 32 colour bytes per sample. Its depth goes straight into the
  single-sample depth target, so SSAO and both occlusion cullers read it and the MSAA depth resolve is skipped. One
  more pass (`fs_deferred_lighting`) lights every covered pixel full-screen, rebuilding its position from that depth,
  then draws the sky and the sorted BLEND primitives forward against the same read-only depth with single-sample
  twins of their pipelines; mirrored primitives get the clockwise-front twins. Shadows, the tiled punctual lights,
  IBL and the post chain are the forward path's. The first deferred frame builds the G-buffer and its pipelines, a
  resize rebuilds the G-buffer, and a shader reload rebuilds the deferred pipelines inside its error scope.
- **One lighting module for both paths**: `shaders/forward/forward_lighting.slang` holds the frame, IBL and light
  bindings, the cascaded shadows, the punctual lights, the IBL ambient and `shade_surface`; `fs_main` and `fs_gbuffer`
  share `sample_material`, and the lighting pass reads the G-buffer back into the same `SurfacePoint`. The BRDF table
  is read at level 0, its only mip, which stays legal after the lighting pass's non-uniform return; the forward frames
  of eight test scenes are bit-identical to before. Nothing under `shaders/common/` changed.
- **Forward stays 4x MSAA and deferred one sample, measured rather than hidden** (`tests/deferred.rs`, lavapipe,
  256x256, seven views): mean luminance at most 0.37 levels apart, coverage 0.0021, mean channel difference at most
  0.50, away from edges at most 0.30 (a 0.9 albedo rounds down 0.4 % in the 8-bit target), and every channel
  difference above 24 levels lies on a silhouette or crease. On the Colosseum's start view at 1600x900 the frames are
  6.4 levels apart on average and 8.8 % of the pixels differ visibly: 0.3 to 0.6 levels on the flat arena floor, 14
  on the crowd, whose figures are smaller than a pixel and come out 3.6 levels darker on one sample. With SSAO and
  bloom off the numbers stay (6.2 levels, 3.6 darker), which leaves the resolve order: MSAA averages linear radiance
  before the concave ACES and sRGB curves. GPU time on lavapipe at 1280x720 (`dump_gpu_timings`, forward vs G-buffer + lighting):
  cube_on_plane 41.3 vs 13.3 + 27.2 ms, point_light 52.5 vs 14.2 + 32.4 ms, cube_alpha 47.2 vs 14.4 + 27.8 ms; a CPU
  rasterizer's numbers, not a GPU's. Rebuilt from 32-bit depth at the Colosseum's clip planes, a position is 6 mm
  off at the arena's 150 m and 40 cm at 1 km.
- `TimedPass::GBuffer` ("G-buffer") and `TimedPass::DeferredLighting` ("Lighting + sky"); `name()` stays first and its
  existing arms unchanged. The render graph's `deferred_frame_graph` swaps the forward and depth-resolve rows for
  them, with a `GBuffer` resource. `dump_gpu_timings --deferred` times the deferred path; BeschleunigerBallett's
  `Compare-RendererTimings.ps1` derives its required Rust passes from `name()` and must treat `GBuffer`,
  `DeferredLighting`, `Clouds`, `RayTrace`, `PathTrace` and `TraceResolve` as optional, as it does `OcclusionCull`:
  a default `dump_gpu_timings` run draws forward raster without clouds, so it reports none of the six.
- `editor_snapshot` takes a `deferred` shot option.
- Tests: `tests/deferred.rs` (parity on the shadowed plane, with a baked environment, under punctual lights, on MASK
  and BLEND, on a transformed texture, on an emissive strength of 3 and on a mirrored instance; shadows landing
  where forward's do, IoU 0.999 against 0.000 for the mirror image; MASK cut-outs and BLEND composites; punctual
  pools within 5 %; a double-sided MASK card from behind and its culled single-sided twin; unlit; the switch both
  ways across a resize; SSAO and occlusion queries on the G-buffer depth; a hidden cube culled by both cullers; a
  shader reload, and one refused for a missing deferred entry point), the GPU-timing pass test on both paths, the
  graph's deferred rows, the G-buffer's colour-byte budget, the depth rebuild's precision, and the editor's switch,
  Render menu and chip.

### Added (2026-11-15, the C++ inspector's raster settings in the WebGPU renderer)
- **Shadow controls with BeschleunigerBallett's ranges and defaults** (`render::shadow_settings`,
  `ForwardRenderer::shadows`), bound in the Sun tab's SHADOWS section: the switch, resolution 512 to 4096, 1 to 3
  cascades, PCF radius 0 to 5 ((2r+1)² comparison taps, each a bilinear 2×2 PCF), strength (0.65, on the sun term
  alone), distance 5 to 500 (60) and the split lambda (0). A new resolution or cascade count reallocates the depth
  array and rebuilds every primitive's group 0 (`sync_shadow_map`); the outliner's Sun row says `N cascades` or
  `no shadows`. Shadows off skips the cascade passes and zeroes the caster counts.
- **Cascades fitted to the view frustum**, the C++ `computeCascadeDataInto` (`render::cascades::fit_cascades`): the
  practical split scheme up to the shadow distance, each slice's bounding sphere as a texel-snapped ortho box in a
  world-fixed light basis, splits and selection by view depth. They were fitted to the scene, so the Colosseum's 8 km
  ground got metre-sized texels and a bias of metres. WebGPU core has no depth clamp to pancake casters, so a box's
  near plane reaches the scene's point nearest the light and the shader scales the bias by the C++ box's depth range
  over this one's: the bias keeps the C++ size in metres.
- **One shadow-bias formula in both renderers**: `forward_lighting.slang` takes `cascaded_shadow.slang`'s
  `max(0.002 * (1 - N·L), 0.0005)` (BeschleunigerBallett's backlog idea; the old `clamp(... + 0.0005, ..., 0.004)` is
  gone). Shared shaders under `shaders/common/` are unchanged.
- **Shadow casters culled per cascade** against the cascade's box without its near plane, as the C++ multiview pass
  culls; each cascade's bundle is re-recorded only when its caster set changes, and `shadow_caster_stats` counts
  draws over the cascades.
- **The C++ cube-map skybox and its switch** (`asset::skybox`, `ForwardRenderer::set_skybox`, `skybox_enabled`): the
  native viewer and `editor_snapshot` load BeschleunigerBallett's `Resources/Textures/Skybox/DOOM2016` (six PNG faces,
  found in the working directory or an ancestor like `Resources/Models`); elsewhere, and in the browser, the
  procedural sky stays. The sky enters the HDR target inverse-tonemapped, so at 0 EV it shows the faces' own colours as
  the untonemapped C++ sky pass does; switched off it shows the C++ navy. The outliner's eye and the Sky tab's SKYBOX
  switch toggle it. The faces carry no licence in BeschleunigerBallett's `licenses/` or `LICENSES-README.md`, so the
  web demo does not ship them.
- **The Model tab's transform** (`render::model_transform`, `ForwardRenderer::model_transform`): position and XYZ
  rotation (T·Rz·Ry·Rx, the C++ `makeGuiModelTransform`) and Reset transform. It is one root matrix left of every
  primitive's instances, so bounds, culling, cascades and shadows follow, and it moves the scene's punctual lights; a
  load resets it, as the fixed C++ editor does.
- **The frustum-culling switch** (`ForwardRenderer::frustum_culling`) in Render › Frustum culling, the Renderer tab's
  FRUSTUM CULLING header and the profiler's Visibility tab, which counts the primitives drawn
  (`primitive_stats`) and casters drawn and what both culled, as the C++ tab does.
- A scene's start view fits the shadow distance: twice its distance to the target, never under the C++ 60 m, so
  the Colosseum's arena is shadowed from `Kamera_Arena` (`Framing::shadow_distance`).
- `image` is a normal dependency for the skybox's PNG faces; `gltf` already built it, so `Cargo.lock` is unchanged.
- Tests: `tests/shadow_controls.rs` (the penumbra grows with the PCF radius, every resolution reallocates and matches a
  fresh renderer, 1 to 3 cascades each shadow from their own layers, strength scales the shadow and the switch removes
  it and its casters, a card 50 m away on an 8 km ground keeps its shadow), `tests/scene_controls.rs` (each cube face
  along its axis, the switch's navy, the model transform moving the model and its shadow, frustum culling counts,
  the light colour), CPU tests for the split scheme, frustum coverage, texel stabilisation, the bias scale and the
  fitted distance, and editor tests for every bound row. The existing shadow tests measure at strength 1, as the C++
  goldens do; `a_far_extra_caster_leaves_the_near_shadow_intact` now checks that caster culling engages, and
  `every_cascade_layer_is_rendered_with_its_own_light_matrix` and the orbit-fit tests of `render::cascades` gave way
  to the frustum-slice ones, since their probes assumed every cascade covers the whole scene.


### Fixed (2026-11-15, the app host after the two editor merges)
- **The animation clock got f32 seconds**: `StudioApp::advance` narrowed its elapsed time before
  `set_animation_time`, which takes f64 since the scene fixes, so a clip stepped by 1/64 s after 36 hours and by
  1/8 s after twelve days. It passes f64; the idle spin's yaw stays wrapped in f64. Test:
  `the_animation_clock_keeps_its_fraction_after_days_of_uptime`.
- **A wheel or trackpad scroll zoomed by physical pixels**: at devicePixelRatio 2 one Chrome notch (100 CSS px,
  200 physical) zoomed twice as far as at 100 %. The orbit controller takes the window's scale factor
  (`OrbitController::set_scale_factor`) and divides wheel pixels, drags and fingers by it once; the app no longer
  divides the drag rate itself, so nothing scales twice. Tests:
  `a_wheel_scroll_zooms_as_far_per_logical_pixel_at_every_scale`, `each_pixel_delta_is_scaled_once`;
  `an_orbit_drag_turns_as_far_per_logical_pixel_at_every_scale` moved from `app::input` to the controller.
- **The wheel and a pinch held the radius to 0.2..500 m**: the first notch on a bounds-fitted Colosseum, a 15 km
  fit, snapped in to 500 m, and from `Kamera_Arena` the view could not back off farther than 500 m. Framing derives
  `Framing::radius_range` from the scene's clip planes, twice the near plane to half the far (the default camera's
  0.2..500 m), and `apply_framing` hands it to the controller. Tests:
  `a_bounds_fitted_eight_kilometre_scene_zooms_from_its_start_radius`,
  `the_wheel_and_a_pinch_keep_to_the_radius_range`.

### Added (2026-11-15, the editor's model library and the Colosseum)
- `resources/scenes/scenes.lock` pins AssetANT's `Colosseum-gltf` and `Colosseum-web` archives;
  `scripts/linux/fetch-models.sh --scenes` (Windows: `Get-Models.ps1 -Scenes`) unpacks them into
  `resources/scenes/<name>/` through the hub's fetcher. Nothing packages or fetches them in CI.
- The native viewer's startup chain: the command line, `KATAGLYPHIS_MODEL_OVERRIDE` (the C++ engine's variable),
  the fetched Colosseum (OxidANT's `resources/scenes`, then BeschleunigerBallett's `Resources/Models`), then the
  embedded cube with the fetch command in the status bar (`app::discovery`).
- The model library (`app::library`, `ui/editor/library.rs`): BeschleunigerBallett's `Resources/Models`, OxidANT's
  `resources/scenes` and the command line file's folder, `.gltf`, `.glb` and `.obj` (converted in memory,
  `app::obj_glb`); the C++ folder groups, order and filter; a click picks, a double-click on the pick, Enter while
  the pick holds it, or Load loads; the mark is the row the host loaded; tooltips with the path and the scene's credit.
  The browser lists its showcase scenes and a `Colosseum` row that fetches `Colosseum-web.glb` beside the page or
  from `?scene-base=`, with streaming progress; the page's `<select>` gains it.
- Loads off the frame (`app::loader`): a worker thread parses natively, the main thread uploads; the scene on
  screen stays until the new one is ready, a failure shows in the status bar instead of panicking, and the status
  bar and the viewport chip show the progress.
- Start views (`app::framing`): a load starts at `scenes[0].extras.active_camera`, else the first glTF camera,
  else a fit of the bounds; near and far follow the scene's size, so the 8 km Colosseum ground neither clips nor
  z-fights at the arena. `EditorAction::FrameScene` (View › Frame scene) returns there.
- A loaded scene's `asset.copyright` in the status bar, the Model tab and About, which CC BY 4.0 asks for; a
  narrow status bar shortens the device to its API before it cuts the credit.
- A loaded scene's first directional light becomes the Sun (direction and colour; the Sun keeps its intensity and
  its cascaded shadows). Added as a punctual light at its 4098 lx, the Colosseum's sun drew the frame white.
- `TimedPass::label()`, the profiler's readable pass names; `name()` stays first and unchanged for
  BeschleunigerBallett's timing script.
- `DockPrefs::web()`: the browser starts without the profiler and drops the inspector before the scene dock, so the
  docs iframe keeps the library; Reset layout returns to it.

### Changed (2026-11-15, the editor's model library and the Colosseum)
- `StudioApp::load_scene` frames the scene as a load does; `editor_snapshot` takes `--model=` and `--web-model=`.
- `obj_to_gltf::read_obj` and `texture_source`, split out of `convert_file`, which keeps its behaviour.

### Fixed (2026-11-15, the editor's model library and the Colosseum)
- **The viewer panicked on a missing or broken model**; it shows the error and keeps the scene on screen.
- **A new scene was never framed**: both shells reset to a 4 m orbit at the origin, so a large scene showed from
  inside or not at all.

### Added (2026-11-15, the Studio Pro fly camera and Camera tab)
- `scene::fly`, the C++ engine's fly camera: W A S D move along the view and its right vector, Q and E turn
  90°/s, the held right button looks around at 0.25°/px with the pointer held (winit's `Locked`, `Confined` where
  a platform has no lock, the Pointer Lock API in a browser; raw `DeviceEvent::MouseMotion`), the pitch stays
  within ±89° and a step integrates at most 0.1 s. It drives the same `OrbitCamera` as the orbit controller:
  flying moves eye and target together and turns about the eye, orbiting turns about the target, so an orbit after
  a flight circles the point ahead. The speed starts at a quarter of the scene's diagonal per second, the C++
  10 units/s in a 40-unit Sponza, so a 1 m cube and an 8 km city both take four seconds to cross; a start view
  closer to its target caps it at half that distance per second (`app::framing`).
- Input routing: a press egui consumes never reaches the fly camera, a release always does, losing the window's
  focus releases every key and ends the look, and winit's synthetic presses on focus gain are ignored. While
  looking egui sees no pointer moves; the pointer comes back where the look began, as GLFW's does.
- The Camera tab: field of view, near and far planes on log scales (far stays twice near), the fly speed, and
  *Frame the scene*, which emits `EditorAction::FrameScene`: back to the scene's start view (`app::framing`) at
  its fly speed. CONTROLS lists W A S D, Q E and the right mouse; a test keeps every fly key in it and off every
  shortcut.

### Changed (2026-11-15, the Studio Pro fly camera and Camera tab)
- The screenshot key moved from S to F12: S flies back.

### Fixed (2026-11-15, the app, UI and web bugs)
- **A Timeout, Occluded or Validation acquire ended the viewer's frame loop**: nothing asked for the next redraw.
  `GpuContext::acquire` repairs the surface and says when to retry, next frame or, occluded, on
  `WindowEvent::Occluded(false)`. A Suboptimal frame reconfigures after presenting, as `context.rs` claimed; a
  native Lost surface is made anew from the window, where reconfiguring it was a validation panic, and a browser's
  Lost, a failed configure, reconfigures.
- **A window or canvas over 8192 px panicked in surface configure**: the surface scales down within the device's
  `max_texture_dimension_2d` with the window's aspect (`context::surface_extent`, the one clamp the render fix
  below uses too), and egui paints at the same scale, so it still hit-tests where it draws.
  `headless_render` and `dump_gpu_timings` check their size (`GpuContext::check_target_size`).
- **The windowed viewer took a GL adapter** and panicked translating SSAO's WGSL. Every context asks for
  `context::BACKENDS` (`Backends::PRIMARY`; the render fix below had asked natively for every backend but GL),
  and `context::check_adapter` refuses any other backend with a reason, windowed and headless.
- `dump_gpu_timings` wrote `frames_measured: 96` without timestamps; it writes 0 then, as the C++ schema does, and
  otherwise the frames whose timestamps arrived. Its JSON tests run with `cargo test` (`[[example]] test = true`).
- **A touch egui consumed left a ghost finger** in the orbit controller (a tap on a panel was enough), so every
  later one-finger orbit pinched instead; a consumed lift or move updates the fingers, and lost focus clears them.
- An orbit drag turned per physical pixel, so a HiDPI screen or a phone orbited 2-3x faster; it turns 0.25° per
  logical pixel.
- The idle spin's yaw is computed in f64 and wrapped to 360°; f32 degrees stepped visibly after a few hours.
- Web: a browser with `navigator.gpu` but no adapter panicked behind a live "drag to orbit" status. The page shows
  its WebGPU fallback with the reason (`webgpuDemoFailed`), and the scene picker opens only once the device is up
  (`webgpuDemoReady`); a native test checks that the page defines each hook the demo calls.
- Web: winit cancelled Tab, F5, Ctrl+R and Ctrl +/-/0 on the focused canvas, a keyboard trap in the docs iframe.
  A capture-phase listener on the canvas's parent leaves them to the browser unless an egui widget has focus.
- Web: after a drop the scene picker still showed the startup scene, which then could not be picked again; a drop
  clears it (`webgpuDemoDropped`).
- `crates/gui`: a minimized, covered or stalled window (an Occluded, Timeout or Validation acquire) quit the camera
  GUI; it skips the frame, and a Suboptimal one reconfigures after presenting. `submit_frame` is 57 lines and left
  `rust-function-size.allow`.
- `crates/gui`: the overlay painted at the window's scale, so egui's Ctrl +/- zoom drew the UI off its hit areas and
  the detection boxes off the video; it paints at the pass's own `pixels_per_point`.
- `crates/gui`: the CPU graph sat at the top whenever the process used more than one core; it scales to 100 % or
  the history's peak.

### Added (2026-11-15, the ray-traced and path-traced modes)
- The C++ engine's ray-traced mode on the tracer core: one ray through each pixel centre, the forward pass's
  GGX and Lambert BRDF lit by the sun through a traced shadow ray (any-hit, MASK alpha-tested), emission, unlit
  albedo, no ambient term; a miss shows the raster sky. `shaders/tracer/traced.slang` (`cs_ray_trace`).
- Its path tracer (`cs_path_trace`, the estimator of BeschleunigerBallett's `path_tracing.slang` in
  `path_estimator.slang`): a jittered primary ray, NEE to the sun, a one-sample mix of the GGX (visible normals) and
  cosine lobes, Russian roulette, 1-64 samples a frame and 1-16 bounces (`ForwardRenderer::trace_settings`), a
  running mean a pixel in a storage buffer that starts over when the camera, viewport, output size, sun, sky light,
  settings or traced scene change, and a white-furnace mode (`TraceSettings::furnace`). The untextured metallic factor
  counts. A bounce that misses gets the sky's light at the raster's ambient strength.
- `trace_resolve.slang` copies the accumulation into the HDR target, so exposure, bloom and the tonemap apply as
  after raster. Dispatches cover 256 x 256 tiles with at most 4 M path segments each, short for a browser's GPU
  watchdog. Each kernel compiles the first time its mode draws.
- `ForwardRenderer::drawn_mode`, `accumulated_samples` and `reset_accumulation`; GPU timing for `RayTrace`,
  `PathTrace` and `TraceResolve`; the render graph's traced frames.
- The editor: the menu bar's switch, the Render menu, the outliner, the inspector title and the viewport chip name the
  drawn mode, raster while the BVH builds, with the build's progress in the chip and the switch's tooltip; the
  Renderer tab's PATH TRACING rows (samples, bounces, white furnace, accumulated samples, restart).
- `editor_snapshot` takes `rt` and `pt`; `tests/traced_modes.rs` ports the C++ PT and RT goldens to lavapipe.

### Changed (2026-11-15, the ray-traced and path-traced modes)
- A ray- or path-traced frame skips the shadow cascades, the forward pass, the depth resolve, occlusion culling and
  SSAO, and reports no drawn primitives or shadow casters, as the C++ engine's traced frames do.
- Picking the path-traced mode stops the viewer's idle camera spin, which would restart its accumulation every frame.
- The traced scene follows rigid animated nodes; `Tracer::generation` counts its changes.

### Added (2026-11-15, the compute-shader tracer core)
- `render::tracer`, the core of the ray-traced and path-traced modes: a binned-SAH BVH the CPU builds (16 bins on
  three axes, sampled above 16 K boxes, leaves of 4, depth capped at the traversal stack) in two levels, repeated
  and animated meshes instanced in object space and the rest merged in world space; the flat GPU arrays (records,
  triangles, positions, packed vertices), the material table and the textures resampled into two mipmapped texture
  arrays; the CPU traversal the GPU one is tested against. Layout, limits and API: `crates/webgpu_renderer/docs/tracer.md`.
- `shaders/tracer/bvh_traverse.slang`: closest- and any-hit traversal over one stack for both levels, with glTF's
  MASK alpha test at level 0, surface and material fetch; `trace_debug.slang` traces a ray buffer for the tests.
- `Tracer::set_root_transform` and `set_primitive_transforms` move instances (the model transform, animated nodes)
  by rebuilding only the top level.
- The traced scene builds the first time `ForwardRenderer::render_mode` is traced: on a worker thread natively, a
  budget per frame in the browser; `trace_status()` reports its stage and progress, the Renderer tab shows it. The
  full Colosseum (6.0 M triangles) builds in about 2 s natively.
- The menu bar's switch, the Render menu and the Renderer tab set the render mode; the frame rasters until the
  modes land.
- `examples/trace_build.rs`: a glTF's build times per stage, buffer sizes and fit to the browser's limits.

### Changed (2026-11-15, the compute-shader tracer core)
- Natively `GpuContext` requests the adapter's `max_storage_buffer_binding_size` and `max_buffer_size`; every other
  limit, and everything in the browser, stays at WebGPU's defaults.
- `StudioApp` hands each scene it uploads to the renderer's tracer (`ForwardRenderer::set_trace_source`), which drops
  it once the traced scene is built.

### Added (2026-11-15, the Studio Pro editor shell)
- `ui::Editor`, BeschleunigerBallett's docked C++ editor in egui, one module per dock under
  `crates/webgpu_renderer/src/ui/editor/`: the menu bar (brand, File/View/Render/Help, the centred renderer switch,
  the frame rate), the status bar, the scene dock (outliner; the library for now over the loaded model), the
  inspector with its five tabs, the profiler (frame figure, frame-time graph, GPU passes, visibility), the viewport
  chip and the About modal. No `CentralPanel`: the scene is the root `Ui`'s leftover rect, which becomes
  `camera.viewport`.
- The C++ dock geometry (`DockPlan`): default, minimum and maximum sizes, drop order on narrow and low windows, whole
  pixels at every scale; custom splitters inside the scene edge that re-anchor every frame, never rewrite a
  preference on a click, ignore a pointer another button holds, and are egui's so a press there never orbits.
- F1 hides and shows the editor (the camera gets the whole frame), F5 reloads the shaders, S takes a screenshot; one
  key table feeds the handlers and the Camera tab's CONTROLS. Esc quits only when, before the pass, nothing had focus
  and no menu or About was open. The double-click window follows the frame rate, as in the C++ editor.
- `src/ui/widgets/` and `src/ui/icons.rs`: the C++ custom widgets as egui painter widgets (fill slider, axis drags,
  pill toggle, segmented control, tab bar, list row, keycap, metric row, status dot, captions, property rows) and
  its eleven stroke icons, each with headless tests.
- `StudioApp::headless` and `render_offscreen`, and `examples/editor_snapshot.rs`, which renders the editor over a
  scene into PNGs at any size and scale, natively or as the browser draws it, on lavapipe without a window.

### Changed (2026-11-15, the Studio Pro editor shell)
- Body text is a 12.8-point em: Dear ImGui sizes a face by ascent minus descent, so the C++ 15 px Roboto is that em
  and egui's 15 points drew it 17 % larger. Rows are 15 points and frames 23, as in the C++ editor.
- The inspector carries the old overlay's controls: the sun's radiance, colour and direction (from the sun into the
  scene, as in C++, instead of azimuth and elevation), the ambient light, bloom, SSAO, exposure, auto exposure and
  occlusion culling with its counts. `RendererSettings` reads them before the egui pass and writes them back after
  it, so the editor needs no GPU.
- Shader reload moved from R to F5, the C++ key.

### Removed (2026-11-15, the Studio Pro editor shell)
- `OverlayControls` and its re-export: the editor replaces the floating panel.

### Fixed (2026-11-15, the renderer's frame, lighting and post core)
- **Every shadow cascade rendered with cascade 0's light matrix** (since 2026-07-24): a render bundle sees only the
  bind groups it sets itself, so the one cached caster bundle's cascade index 0 beat each pass's own, and layers 1
  and 2 held cascade 0's projection. Every fragment past the first split, most of a typical view, got a misplaced,
  mis-scaled shadow. Each cascade records its own bundle now; `every_cascade_layer_is_rendered_with_its_own_light_matrix`
  reads the layers back (`read_shadow_map`) and checks two points' depths against each cascade's matrix.
- **Removed instances kept casting shadows, and a shader reload left shadows on the old shaders**: the bundles bake
  each draw's instance count and pipelines, but only a reallocated instance buffer or a new scene re-recorded them.
  `set_instances` re-records them whenever a count changes (the clearing path too), `reload_shaders` after every
  successful reload. Tests: `shrinking_or_clearing_instances_removes_their_shadows`,
  `reloading_shaders_rerecords_the_shadow_bundles`.
- **Point and spot lights without a glTF `range` lit one 16×16 tile** (Blender's default export): range 0 means
  unbounded, but the tile binning collapsed its footprint to the tile under the light's centre, or none behind the
  camera, and a KHR directional light leaves no tile empty for the all-lights fallback. Such a light is binned into
  every tile now. A tile more than 32 lights overlap dropped the rest without a trace; it lists none now, so the
  shader shades every light there. Tests in `tile_grid.rs`.
- **The occlusion-culling checkbox panicked on scenes past 4096 primitives**, and below it whenever doubling the
  query set overshot (2100 primitives, then 2200, asked for 4200): one query set holds at most
  `wgpu::QUERY_SET_MAX_QUERIES`. Growth stops at the limit and only the first 4096 boxes are queried; the rest stay
  visible. `occlusion.rs:358` is `:396` for `buffer_desc_single_definition.rs` now. Tests:
  `query_capacity_never_grows_past_the_query_set_limit`,
  `more_boxes_than_the_query_set_limit_record_without_a_validation_error`.
- **Double-sided sheets cast no shadow with their back to the light, and vertex-alpha cut-outs cast solid shadows**:
  every caster but a textured MASK one went through the back-culled shadow pipeline, although the camera pass draws
  double-sided materials from both sides and discards MASK fragments by vertex alpha too. Double-sided casters use
  an unculled pipeline now, and every MASK caster the alpha-testing one (with the white fallback when untextured).
  Tests: `a_double_sided_sheet_casts_a_shadow_with_its_back_to_the_light`,
  `vertex_alpha_cut_outs_of_an_untextured_mask_cast_no_shadow`.
- **Mirrored nodes and instances rendered inside out** (Khronos NegativeScaleTest, a part mirrored with scale -1):
  glTF reverses a primitive's winding under a negative determinant, but every pipeline culled clockwise faces, and
  the instance normal transform (the bare cofactor, det·A⁻ᵀ) pointed mirrored normals inward. A primitive whose node
  mirrors space, and each mirrored instance (stored after the upright ones and drawn separately), use clockwise-front
  pipelines, built the first frame one is drawn; the cofactor carries the determinant's sign and the bitangent sign
  follows the mirror, so normal maps keep their relief. Skinned normals took the forward matrix and sheared under a
  non-uniform joint or instance scale; they take the same sign-corrected cofactor. A double-sided back face flipped
  the normal and bitangent but not the tangent, mirroring a normal map's X relief; the whole mapped normal flips
  now. Tests: `tests/mirrored.rs`,
  `a_non_uniform_instance_scale_shades_a_skinned_mesh_like_an_unskinned_one`.
- **A screenshot at the window's own size froze the window's bloom and AO** until the next resize (the viewer's S at
  a 1920×1080 surface): `render_to_pixels` forced bloom and SSAO to rebuild, and only its own temporary tonemap
  pass was rebound to the new textures. Each rebuild now names its input set, every `TonemapPass` remembers the set
  it samples and rebinds when they differ, and a screenshot rebuilds nothing. Test:
  `a_screenshot_at_the_window_size_leaves_the_window_post_inputs_live`.
- **Radiance past the half-float range blackened hot spots**: the Rgba16Float HDR target stores anything above
  65504 as +Inf on most GPUs (lavapipe rounds it to 65504), bloom spread the Inf and ACES turned it into NaN, so a
  lamp inside a fixture or a strong emitter drew a black block. The forward and sky passes clamp their output to
  65000, the bloom bright pass clamps after its exposure scale, and the tonemap caps its input at 1e4, where ACES
  has long saturated. Test: `a_hot_spot_past_the_half_float_range_saturates_instead_of_turning_black`, whose
  infinite-emissive scene (a glTF `emissiveStrength` past f32's range) fails on lavapipe without the clamps; it
  reads the target back through the doc-hidden `read_hdr_target`.
- **The shared GGX NDF clipped every specular peak below roughness 0.154** (`shaders/common/brdf.slang`, which
  BeschleunigerBallett's C++ renderer compiles too): its `max(PI * denom², 1e-6)` guard engaged at the peak, so a
  0.045-roughness lobe kept 1.45% of its energy and highlights dimmed as surfaces got smoother. The guard now only
  keeps a rounding error off zero, and the NDF raises roughness below 0.045 to that floor itself, so every caller,
  the C++ ones whose textured roughness reaches 0 included, gets a finite lobe. BeschleunigerBallett's goldens need
  a re-run with the next OxidANT pin. Tests: `tests/brdf.rs` evaluates the emitted WGSL on the GPU; the peak is
  1/(πr⁴) and the projected lobe integrates to one from roughness 0.045 to 1.
- **Bloom thresholded at the previous frame's exposure**: its bright pass multiplies by the exposure the reduce pass
  writes, but ran before it, so a fresh renderer at EV -3 bloomed its first frame at EV 0 and an auto-exposure step
  reached bloom a frame late. The histogram and the reduce run before bloom now, the render graph declares bloom's
  `Exposure` read (`forward_graph_is_valid` fails on the old order), and `TimedPass::ALL` follows the new record
  order. The roadmap's "validated in debug builds" was never true: the unit test validates the graph. Test:
  `the_first_frame_blooms_at_its_own_manual_exposure`.
- **A new scene inherited the last one's adapted exposure**, so with auto exposure on a dark model dropped after a
  bright one started 32× underexposed; `HistogramPass::reset_exposure` existed but nothing outside the tests could
  call it. `upload_scene` resets the exposure to `exposure_ev`, and `ForwardRenderer::reset_exposure` does it on
  demand. Test: `uploading_a_scene_resets_the_adapted_exposure` (through the doc-hidden `read_exposure`).
- **`ibl_intensity` was ignored after `set_environment`**: the bake was the only write of it into the IBL uniforms,
  unlike every other public knob. They are written every frame now. Test:
  `ibl_intensity_takes_effect_after_set_environment`.
- **Re-enabling occlusion or compute culling culled with visibility from an old camera**: both readback paths kept
  their last result while switched off, and the compute path's unread slots drained after the switch back, so
  objects revealed in between popped in for a few frames. Any change of the two culling flags now resets both
  paths. Test: `re_enabling_culling_forgets_the_visibility_from_before`.
- `MSAA_SAMPLE_COUNT`'s "(1 disables MSAA)" was false: the forward pass always sets a resolve target and the depth
  resolve reads a multisampled texture, so 1 fails validation. A const assertion now refuses it at compile time.
- **`render_to_pixels_with_format` mislabelled BGRA and panicked on wider formats**: it promises RGBA8 bytes, so
  `Bgra8Unorm` (the browser's preferred canvas format) came back channel-swapped and `Rgba16Float` overran the
  4-byte row pitch in a validation panic. It returns an error for anything but `Rgba8Unorm` and `Rgba8UnormSrgb`.
  Test: `render_to_pixels_refuses_formats_that_are_not_rgba8`.
- The docs claimed shadow-caster culling that does not exist: `bounds.rs` said the cascade pass culls casters, and
  the bounds invariant listed it as a consumer, but the cached bundles draw every caster. Both say so now, and
  `caster_culling_engages_and_shadows_survive` is `a_far_extra_caster_leaves_the_near_shadow_intact`, which pins
  drawn == considered.
- **An older occlusion readback could overwrite a newer one**: both readback rings drained their ready slots in
  index order, which after a wrap is not age order, so frame 3 in slot 2 could land after frame 4 in slot 0 and
  culling lagged an extra frame. Each slot records a sequence number and only a readback newer than the last applied
  one is applied (`occlusion::newer_readback`, shared by `gpu_occlusion.rs`). Test:
  `a_late_older_readback_never_overwrites_a_newer_one`.
- **GPU timings kept reporting a pass after it stopped running** (bloom or SSAO at strength 0, auto exposure off,
  culling off): each frame resolved its slot's whole query range, and an unwritten query resolves the stamps an
  earlier frame left there, which passed the plausibility filter. Each `PassScope` now marks the stamps it hands
  out, a frame samples only the passes it stamped, and a pass missing for a full averaging window drops out of
  `gpu_timings_ms`. Test: `a_pass_that_stops_running_drops_out_of_the_timings`.
- **Large triangles picked coarse shadow cascades**: the cascade came from an eye distance computed per vertex and
  interpolated, which overestimates inside a big triangle, so a floor under the camera read its far corners'
  distance and took cascade 1 or 2, with their coarser texels and larger world-space bias. `fs_main` measures each
  fragment's own distance now; the `viewDepth` varying is gone. Test:
  `a_huge_floor_takes_its_cascade_from_each_fragment_distance`.
- **A cascade clipped casters between the light and its box**: each cascade's depth range was a fixed ±4r band
  around its centre, so with the focus off the scene centre and a low sun a tall caster's top fell before cascade
  0's near plane and its shadow tip vanished near the camera. The near plane now also reaches the scene box's corner
  nearest the light; ranges that already reached it are unchanged. Test:
  `no_cascade_clips_a_caster_between_the_light_and_its_box`.
- **BC5 normal maps shaded inside out**: BC5 stores a normal map's x and y and samples blue as 0, so the shader's
  `normal.z = 2b - 1` was -1 and a flat texel turned the normal around. A normal sample with blue exactly 0 now
  rebuilds z from the unit length glTF requires; RGB maps never sample blue 0 for a valid normal, so they shade as
  before. Test: `a_bc5_normal_map_shades_like_a_flat_one`.
- **A GL adapter could win adapter selection natively**: both contexts asked for every backend, and wgpu ranks a
  GL adapter (an integrated or unknown GPU) above lavapipe, so headless tests skipped on hosts where Vulkan worked,
  and a windowed host whose best adapter was GL (a Windows VM) panicked building the depth-resolve and SSAO
  pipelines, whose depth `textureLoad` has no GLSL form. No context requests GL (`context::BACKENDS`, above).
  Test: `native_contexts_never_request_the_gl_backend`.
- **A window wider or taller than 8192 px panicked**: the device asks for WebGPU's default limits, but the surface
  took any size, so a window across two 5K monitors failed surface validation. The surface now scales down to the
  device's largest texture side with its aspect kept, and the scene renders at that size. Test:
  `a_surface_past_the_texture_limit_scales_down_with_its_aspect`.
- **The IBL bake dropped small bright sources**: the panorama-to-cube pass took one bilinear tap per cube texel
  although a 2048-wide panorama holds about five texels per face-centre cube texel, and the irradiance pass read
  mip 0 on a 128x64 grid, so a sun a few texels wide was lit or missed by its sub-texel position (a 2x2-texel sun
  vanished entirely). The cube pass now averages a tap grid sized to the panorama's texels (up to 16 per side), and
  each irradiance tap reads the mip whose texels match its patch of the hemisphere. Test:
  `a_small_sun_gives_the_same_irradiance_at_any_sub_texel_offset` (within 3% of the analytic irradiance).
- **Compute occlusion culling culled boxes seen through narrow gaps**: it tested 8×8 point taps across each box's
  screen rect, so a box visible only through a gap between taps (a slit, a colonnade) was culled, and stayed culled
  since it never re-entered the depth buffer; the roadmap called the path conservative. A rect of up to 4096 texels
  now has every texel tested (early out at the first one at or behind the box), and a larger rect is drawn. Test:
  `a_box_seen_through_a_two_pixel_slit_is_never_gpu_culled`.
- **A vertex on a zero-scale joint, or under an instance with a zero scale axis, shaded its triangles with NaN**:
  the forward pass's `normalize` of the collapsed normal or tangent is NaN, and every fragment of a triangle that
  touches the vertex interpolated it (near-black on lavapipe, whose min and max drop the NaN). Rigs hide parts this
  way. `forward.slang`'s `normalize_or` passes zero from the vertex, which leaves the direction to the triangle's
  other vertices; a fragment whose vertices all collapsed faces the camera, and a zero tangent leaves the mapped
  normal at the geometric one. Tests: `a_vertex_on_a_zero_scale_joint_never_shades_nan`,
  `a_flattened_instance_never_shades_nan`.

### Fixed (2026-11-15, the renderer's scenes and assets)
- **A skinned mesh in a scene without animations drew in raw mesh space**: its joint buffer stayed identity,
  because only `set_animation_time` posed joints and it returns early without clips. `upload_scene` now poses the
  joints from the file's node transforms, and its scene bounds come from the skin-widened boxes, so the cascades
  fit the posed mesh from the first frame.
- **Every mipmapped KTX2 BCn texture failed wgpu validation** (a panic on native): levels below 4x4 were copied
  at their logical size, and wgpu copies whole blocks. They copy their block-rounded size now. A base that is not
  whole 4x4 blocks (6x6, 1000x750), which WebGPU cannot create, is refused by `load_ktx2`, and a payload built in
  code that wgpu would reject falls back to white with a warning. `tests/ktx2.rs` uploads through a material now;
  its old upload test drew a scene without primitives, so the texture was never created.
- **A glTF primitive whose triangles were all dropped panicked the first frame** (`buffer slice can not be
  empty`, also on the web demo's drag-and-drop): every index out of range, or a strip of two indices, left an
  empty index buffer. The loader skips such a primitive with a warning, and `upload_scene` never creates an empty
  vertex or index buffer, so a scene built in code with an empty primitive draws nothing for it instead.
- **A glTF node cycle overflowed the stack** (an abort natively, a dead canvas on the web): the loader recursed
  into children with no visited set. It walks the node trees with an explicit stack and visits each node once,
  warning on a second visit; `CpuScene::compute_world_transforms` resolves parents iteratively too, so a
  50 000-level chain, valid glTF, no longer overflows either.
- **A NaN keyframe time panicked every animation frame**: both end tests of the keyframe search fail for NaN,
  which left the binary search at 0 and underflowed. The loader drops a channel whose times are empty, not finite
  or decreasing, whose values are not finite, or whose value count does not fit its keys, and the search pins a
  NaN time or key to the first key.
- **A glTF texture or an equirect over 8192 px a side panicked at upload** on native (nothing rendered on the
  web): the device runs at WebGPU's default limits. A material texture now uploads from the first level of its
  CPU mip chain within `max_texture_dimension_2d`, and the IBL bake box-halves an equirect until it fits; a BCn
  payload over the limit falls back to white.
- **`decode_hdr` let a 40-byte header allocate a 512 MiB scanline**, and on wasm32 an image over 2^26 pixels
  panicked with `capacity overflow` instead of returning an error. The width has a cap of its own (65 536), the
  pixel cap is 8192 x 4096 on 32-bit targets (a compile-time check keeps twice the image allocatable), and the
  output grows with `try_reserve`, so a failed allocation is `HdrError::BadDimensions`.
- **A node hidden with scale 0 reappeared at full size once animation ran**: `compute_world_transforms` turned an
  all-zero scale into 1. The singular matrix is finite, the normal matrix already guards its inverse, and zero
  scale is how exporters hide a node, so it stays; only non-finite inputs are replaced.
- **An indexed glTF without NORMAL was not flat-shaded**: each shared vertex kept the normal of its last face (24
  of 36 corners on an 8-vertex cube). Such a primitive is un-indexed first, one vertex per corner with its morph
  deltas, as glTF's flat normals need. A zero-area face no longer overwrites a corner's normal with zero, and a
  zero or non-finite normal or tangent, from a degenerate face or from the file, is repaired (face normal or +Y, a
  perpendicular tangent, `w` a sign), since the shader's `normalize` turned it into NaN shading.
- **Generated tangents always followed TEXCOORD_0**, so a normal map on TEXCOORD_1 was lit along the wrong axes.
  The Lengyel and MikkTSpace generators read the UV set the normal texture samples, as glTF specifies.
  `generate_tangents_mikktspace`'s callbacks moved out of its body, which leaves its `rust-function-size.allow` row.
- **With `KATAGLYPHIS_MIKKTSPACE_TANGENTS`, morph deltas landed on the wrong vertices**: MikkTSpace renumbers
  vertices in first-use order and splits them at seams, and the deltas kept the file's numbering. It returns each
  new vertex's source, and every target's deltas are gathered through it, split copies included.
- **Skins over 256 joints were truncated silently**, so vertices on joint 256 and up read a clamped or zero matrix
  and their bounds ignored those joints. `MAX_JOINTS` is gone: the joint buffer holds the whole skin, within the
  device's storage-binding limit. The loader drops influences on joints the node's skin does not have, and every
  influence of a weighted mesh on a node without a skin, which the shader skinned with no palette and drew at the
  mesh origin instead of the node transform.
- **Every animation clip played at once**, each clip overwriting the ones before it per channel (Fox.glb always
  showed Run). `set_animation_time` samples one clip, the first by default; `animation_names`,
  `active_animation` and `set_active_animation` pick it, and a switch returns nodes and morph weights to the
  file's pose first.
- **Animation stuttered after a day or two of uptime**: `set_animation_time` took f32 seconds and wrapped them in
  f32, which steps by 1/64 s after 36 hours. It takes anything `Into<f64>` now (an f32 still compiles) and wraps
  each clip in f64 before narrowing. `StudioApp::advance` still converts its seconds to f32 (`src/app/frame.rs`).
- **Punctual lights on animated nodes never moved**: a light's pose was baked at load. `CpuLight` carries its node
  (`node`, `None` for a fixed light), and `set_animation_time` re-poses every such light from its node's world
  transform (`CpuLight::pose_at`, the loader's rule: the node's origin, pointing down its -Z) and repacks them.
- **Morph culling bounds assumed weights in [0, 1]**, so a default weight of 2 or a CUBICSPLINE overshoot drew
  geometry the frustum test, the shadow casters and the cascade fit did not see; `renderer-bounds-invariant.md`
  called the box conservative outside that range, which it was not. Each target now covers `min(0, w)` to
  `max(1, w)` of its current weight, and an animation frame that moves the weights rebuilds the box.
- **QEM LODs broke normals and UVs at every hard edge and seam**: the weld merged coincident vertices and kept the
  first one's attributes, so 24 of `cube.gltf`'s 36 corners took a neighbouring face's normal and UV before any
  collapse. Welding is topology only now: each face corner keeps its own attribute set, a collapse blends the sets
  its removed faces pair across the edge and refuses one that would need two at a corner, and seam edges get the
  same constraint planes as open rims.
- **glTF's NEAREST and LINEAR min filters still sampled the generated mip chain**, so minified pixel art blurred
  into box-filtered averages and LINEAR got trilinear 16x anisotropic filtering. `CpuSampler::mipmaps` is false
  for them, and such a slot samples a one-level view of the texture (a zero LOD clamp would not do: Vulkan counts
  a clamped LOD of 0 as magnified and switches to the mag filter); the upload stays shared.
- **An image URI with an uppercase extension, or the common wrong MIME type `image/jpg`, failed the whole glTF
  load**: the importer matches `png`, `jpg` and `jpeg` exactly. The renderer enables `gltf`'s `guess_mime_type`,
  which decides by the PNG or JPEG bytes. `obj2gltf` writes a `mimeType` for every image and refuses a texture
  that is neither PNG nor JPEG, which converted fine and then failed to load.
- **One large wheel event snapped the orbit radius to its minimum**: the zoom factor `1 - amount * zoom_step`
  reached zero at ten notches (one 300 CSS-px flick at devicePixelRatio 2) and went negative beyond, and zooming
  in and out by the same amount did not return. It is `(1 - zoom_step)^amount` now. Dividing pixel deltas by the
  window's scale factor needs the host's DPI and stays with the editor work.

### Fixed (2026-11-15, the renderer's egui overlay and its shells)
- **No egui text rendered since the egui 0.37 bump**: 0.37 bundles no font unless its `default_fonts` feature is
  on, so every glyph was a box in the renderer's overlay and in `crates/gui`. The renderer embeds the C++ editor's
  Roboto Medium and Cousine (`crates/webgpu_renderer/assets/fonts/`, byte for byte BeschleunigerBallett's Dear ImGui
  copies, with their Apache-2.0 and OFL-1.1 texts and a NOTICE; a test pins all four hashes). `crates/gui` turns on
  egui's `default_fonts`, and `packaging/cargo-license-policy.json` gets `epaint_default_fonts@0.37.0` back with the
  0.37.0 archive's font licences. A test in each crate lays the UI's strings out under `MissingGlyphPolicy::Panic`.
- **The overlay paints at egui's own scale** (`FullOutput::pixels_per_point`, zoom times the window's), passes the
  device's largest texture side, pins the dark theme and turns keyboard zoom off; it painted at the window's scale,
  so a Ctrl `+` zoom drew the UI off its hit areas.
- **The viewer's S, R and Esc fired while typing**: they are read inside the egui pass now, and do nothing while a
  text field has focus. Esc first releases focus or a popup, then quits; the browser demo never quits.
- **The web demo animated and auto-orbited per frame**, so their speed followed the display rate, and the auto-orbit
  reset the wheel zoom every frame. It runs on time like the viewer, and only the yaw orbits.

### Added (2026-11-15, the Studio Pro foundation)
- `src/ui/{palette,theme,fonts}.rs`: the C++ editor's palette, style and type scale in egui points.
- `SceneViewport` and `OrbitCamera::viewport`: the C++ `makeViewportRemap` lens shift, Y negated for WebGPU clip
  space, so the camera centres on the rect the docks leave free while every pass stays full-frame. A full-frame
  camera's projection is bit for bit what it was.
- `src/app/`: `StudioApp`, the one host `examples/viewer.rs` and `src/wasm_demo.rs` run, in the C++ frame order:
  acquire, one egui pass, its actions, the scene with the camera on the free rect, egui paint, present. It starts
  GPU timing. The viewer opens at 1600×900 logical like the C++ app, and screenshots with the full-frame camera.
  `viewer.rs redraw` and `wasm_demo.rs window_event` leave `rust-function-size.allow`.

### Changed (2026-11-15, the forward renderer is one module per pass)
- `render/forward.rs` keeps `ForwardRenderer`, its public API and the frame's pass order; each pass and the
  resources it owns moved to a module under `render/forward/`: `shadow`, `main_pass`, `sky`, `targets` (with the
  depth resolve), `culling`, `post` (bloom, SSAO, exposure, tonemap), `light_grid`, `frame`, `environment`, `upload`,
  `animate`, `primitive` and `readback`. `render_tonemapped` is now one `encode_*` call per pass. No function there
  is over 56 lines in the hub's count, so the five `forward.rs` rows of `rust-function-size.allow` (`new`,
  `upload_scene`, `render_tonemapped`, `render_to_pixels_with_format`, `set_animation_time`) are gone. The two
  source-text tests scan the new files too.

### Fixed (2026-11-15, three new tests on the Windows arm64 cross lane)
- The lane runs the cross-built test binaries on a machine without the source tree (run 43750810210), so the
  capture tests' photo, read through `CARGO_MANIFEST_DIR`, was missing and `one_camera_pipeline` found no `src`. The
  photo is now in the test binary (`include_bytes!`) and written to a temporary folder, as the inference crate's
  tests do; the source-reading guard says SKIP without a source tree, which every lane that builds has.

### Changed (2026-11-15, `src`'s coverage leaves the generated bridge out)
- The `coverage` step passes `src/frb_generated.rs` to tarpaulin's `--exclude-files` beside `third_party/*`: the file
  was 1157 of `src`'s 1795 lines and no Rust test runs it, so `src` read 24 % (run 43737706756) and 67.6 % without
  it, and the workspace 78.8 % against 85.9 %. `src`'s floor goes from 19 % to 65 %; `docs/testing.md` says so.

### Changed (2026-11-15, the wgpu GUI captures through the media crate; two camera pipelines left)
- `crates/gui` drops its own pipeline (`build_pipeline`, which never read its bus) and runs a
  `kataglyphis_media::CaptureSession` at the camera's own size: `CaptureConfig` takes width or height 0 for the
  camera's own size and framerate 0 for any. A camera that ends, fails or goes silent for 10 s now ends the window
  with that reason. `media-check` builds the same session unstarted (`CaptureSession::unstarted_pipeline`), so it
  reports `videoconvert`, `videoscale` and the frame sink, and the Linux package checks `videoscale` instead of
  `capsfilter`. `crates/media/tests/one_camera_pipeline.rs` fails when a crate other than `crates/media` or
  ObservANT builds an appsink. `gstreamer-app` left the GUI's dependencies.

### Fixed (2026-11-15, the app's camera session ends when the camera goes silent)
- `kataglyphis_media`'s frame slot stamps the session's start and each frame; `take_latest` closes it with
  `no frame for 10.0 s` (`capture::STALL_AFTER`, the same 10 s as ObservANT) once nothing came for that long. The app
  engine's loop waited on a silent camera forever; it now ends with that reason, which the app shows. Tests: a launch
  line that never sends and one whose valve closes after a frame, in the media crate and through the engine
  (`FrameSlot::set_stall_after`, hidden, shortens the time).

### Added (2026-11-15, mutation recipes for the detector's pre- and postprocessing)
- Five `consumer.oxidant-inference-*` recipes run `cargo test -p kataglyphis_inference --lib` against red and blue
  swapped, the score threshold ignored, a NaN corner kept, the corners unclamped and one scale for both axes; each
  proved to bite with two mirrors (26 min on a Pi 5, cold builds included). `crates/inference` had none.

### Added (2026-11-15, ObservANT's stall and re-probe times are a `Timing` value)
- `capture::Timing` holds the 10 s stall and the 30 s re-probe of a stand-in, both formerly constants, as a field of
  `Worker` with today's numbers as default. Tests: an image at one frame a second against a 300 ms stall ends the
  capture with `no frame for 0.3 s`, and a stand-in pattern ends at its re-probe time while a chosen test pattern or
  image has none. The stall message names tenths of a second now (`no frame for 10.0 s`). `main.rs`'s `run` moved its
  status setup into `build_status` (112 to 91 lines in `rust-function-size.allow`).

### Added (2026-11-15, the app engine's end reasons are tested)
- `WebcamEngine::start` builds the detector and the capture and hands them to `run`, whose worker
  (`infer_until_done`) takes the model as a closure; `kataglyphis_media::CaptureSession::from_launch` (hidden, for
  tests) starts a session from a launch line. Three tests: a camera that ends (`videotestsrc num-buffers=5`) ends
  the session with `the camera stream ended`, a model that fails every frame ends it after exactly 30 runs, and one
  good run in 25 keeps it going past 100. `start` left `rust-function-size.allow` (107 lines, now under 80).

### Added (2026-11-15, ObservANT's capture loop runs in tests with the real model)
- `capture.rs` tests run the whole worker: the real model on `two_cats.jpg` opens one visit with both cats through
  the pacer and `spawn_inference` (skipped without the model, required where `KATAGLYPHIS_REQUIRE_MODEL=1`), and an
  `image:` path that is missing at start is tried again after the 1 s backoff and delivers frames once the file
  exists. The test helper became `Capturing`, which a test can start with a detector. 4 s on a Pi 5 for both.

### Fixed (2026-11-15, `/healthz` takes no lock while it holds another)
- `Status::to_json` copies what it needs out of `detected` and releases it before it locks `clips`, `stats` or
  anything `health()` reads; it held `detected` throughout, so the lock order was right by convention only. A test
  holds `clips` on one thread and then wants `detected` while `to_json` runs on another: on the old order the two
  wait forever and the test fails after 5 s (checked), on the new one it returns.

### Changed (2026-11-15, the endpoint client has a limit per phase; ObservANT's `llm_timeout_s`)
- `Endpoint` waits 5 s for the connection (`connect_timeout`) and 120 s for any one read, the response head
  included (`read_timeout`), beside the 300 s cap on the whole request (`timeout`); it had the cap alone, so a
  server that took the question and went quiet held ObservANT's one `/ask` slot for five minutes. A timeout is
  the new `TextError::EndpointTimedOut` (kind `endpoint_timed_out`), no longer `EndpointUnreachable`, and
  ObservANT answers it with 504. `llm_timeout_s` (default 300, at least 1) sets both the endpoint's cap and the
  answer's limit. Tests: a server silent before its head, after it and after a piece; a stream that keeps
  talking until the cap; on Linux a connection that never opens (a full accept queue); a silent endpoint behind
  ObservANT with `llm_timeout_s = 1`. Five mutation recipes, `consumer.oxidant-text-*-timeout`, `-request-cap`
  and `-timeout-kind-*`, each proved to bite.
- `docs/observant.md` replaces an unmeasured reload time with a measured one: a fresh CLI process that loads
  SmolLM2-135M and gives one token takes 0.7 s on a Pi 5, 1.3 s with a cold page cache.

### Added (2026-11-15, ObservANT unloads an idle language model)
- `llm_idle_unload_s` (default 600; 0 keeps it) unloads `llm_model_dir`'s model after that many seconds without a
  question, and the next question loads it again (owner decision). A task started beside the web server asks the
  `Assistant` every quarter of that time, at most a minute; a running answer holds the model, and an endpoint has
  nothing to unload. Tests: an idle model unloads and reloads, a running answer and an endpoint keep theirs, and
  only an in-process model with a non-zero setting unloads.
- The app's text bridge (`src/api/text.rs`) does the same for its cached local model: a watcher thread, started
  with the first load, drops a model unused for 600 s (`IDLE_UNLOAD`), skipping a turn in flight through
  `try_lock`; each generation and embedding marks the model as used. No generated binding moved.

### Changed (2026-11-15, the Rust detector follows the family's ORT thread rule)
- Without `KATAGLYPHIS_ORT_THREADS` the detector's session takes the usable CPUs but one, 1 to 8
  (`kataglyphis_core::config::family_threads`, from `available_parallelism`), as OrchestrANT and AccelerANTgine
  do (owner decision; was ORT's own default, a written choice of 2026-10-11). `0` still keeps ORT's own count;
  ObservANT's 2, the bench's 2 and every set value stay. Measured on a Pi 5 (4 CPUs, two runs each, p50): ORT's
  default 237-247 ms, 3 threads 234-242 ms, 4 threads 253-262 ms, 7 threads 525-567 ms. `docs/running.md` and
  `docs/observant.md` say so.

### Changed (2026-11-15, a text failure keeps its kind)
- `TextError::kind()` names each variant with a fixed snake_case word (`invalid_options`, `model_load_failed`,
  `endpoint_unreachable`, ...), and `every_error_names_its_kind` asserts them distinct. ObservANT maps it: the
  caller's `InvalidOptions` is 400, a library, model or provider that cannot load 503, everything past it 502, each
  with the text body as before (`each_kind_of_text_failure_has_its_status`). The bridge's `text_generate` and
  `text_embed` errors start with `kind: `, so no generated binding moved; the web build says
  `library_unavailable: `.

### Fixed (2026-11-15, the wasm32 build)
- `text_default_options` has read `kataglyphis_text`'s defaults since dc3916e, which wasm32 did not depend on, so
  `cargo check --target wasm32-unknown-unknown -p oxidant` failed; no lane builds it. The root crate depends on the
  text crate there too (its endpoint and GenAI stay native), and six native-only helpers are gated off wasm32, so
  clippy passes for both targets.

### Added (2026-11-15, ObservANT's in-process model is capped and reports its speed)
- `llm_threads` (default 2; 0 leaves GenAI's own count) holds `llm_model_dir`'s decoder to that many intra-op
  threads beside the detector's two: `GenaiGenerator::load_with` lays `thread_overlay(n)` over the model's
  `genai_config.json` through `OgaConfigOverlay`, bound optionally, and refuses to load when the library lacks it.
- `/ask` and `POST /summary` return `time_to_first_token_ms` and `decode_tokens_per_second`, and each answer logs
  its token count and both figures; the goldens carry the two keys. Tests: the overlay, a capped tiny-model load
  that still gives the oracle's continuation, and the `llm_threads` default and parse.

### Changed (2026-11-15, a greedy GenAI turn continues the last generator)
- `GenaiGenerator` keeps the generator of a greedy turn without `json_schema` and every token it holds; the next
  prompt rewinds it to the start the two share (`OgaGenerator_RewindTo`) and appends only the rest
  (`OgaGenerator_AppendTokens`). Room: prompt, budget and 1024 more tokens, capped at `genai_config.json`'s
  `context_length`; the decode loop now ends at `max_new_tokens` itself. Sampling, a schema, a prompt sharing
  nothing, one that does not fit or a GenAI without the five symbols (bound optionally) start a fresh generator.
  New test `a_continued_generator_gives_the_tokens_of_a_fresh_one`. On a Pi 5, turn 5 of a five-turn chat reaches
  its first token after 48-52 ms instead of 539-540 ms (release CLI, two runs each).
- `kataglyphis_cli chat --then TEXT` (repeatable) asks each text as one more turn of the same conversation;
  `perf-host.sh text-turns` and `quality-report.toml`'s `text_turns_rust` time turn 1 and turn 5.

### Added (2026-11-15, goldens for the rest of ObservANT's JSON)
- `tests/fixtures/api/{settings,visits,summary,summary_words,ask}.json`: what `/settings`, `/visits`, `GET` and
  `POST /summary` and `/ask` answer for a camera with one visit and a stub model, arrival, day and times fixed.
  `the_api_answers_match_their_goldens` in `web.rs` compares them; `OBSERVANT_UPDATE_GOLDEN=1` rewrites them.
  A renamed settings key fails it (proven by hand in the image). The `/healthz` test shares the new
  `golden` test module.
- OmniAccelerANT 2cabbff reads them: `test/observant_api_goldens_test.dart` parses `/settings`, `/visits`, `/ask` and
  `POST /summary` from this directory at its pin.

### Fixed (2026-11-15, the healthz goldens on the Windows arm64 runner)
- `healthz_matches_its_goldens` embeds the three goldens with `include_str!`; it read them at run time, and the
  cross-built test on `windows-11-arm` runs where no checkout of the fixtures sits, so it compared against an empty
  text (run 43645553880). `OBSERVANT_UPDATE_GOLDEN=1` still rewrites the files.

### Changed (2026-11-14, crates/esp32 under the function-size gate)
- `crates/esp32` is the host's USB-serial bridge, not firmware, so clippy checks it like any crate: its root sets
  `#![warn(clippy::too_many_lines)]` and `check-rust-function-sizes.sh` scans it. `read_loop` (89 lines) hands
  each board message to the new `device_event`, so no row and no `#[expect]` are needed.

### Fixed (2026-11-14, the healthz golden test on a Windows checkout)
- `healthz_matches_its_goldens` compares the golden with its line ends made LF: a Windows checkout hands the JSON
  over with CRLF, so the test failed on both Windows lanes while ObservANT's answer was right.

### Fixed (2026-11-14, a text recipe ran the other mirror's binary)
- The 13 `consumer.oxidant-text-*` recipes build into `${RUNNER_TEMP:-/tmp}/oxidant-text-mutations-<mirror>`, one
  target directory per mirror. They shared one: cargo names a workspace member's artefacts without its absolute
  path, so two mirrors running in parallel overwrote each other's fingerprints and a mutant could run the other
  mirror's unmutated test binary. `consumer.oxidant-text-unknown-provider-on-cpu` survived that way in CI, and bites
  with two jobs now.

### Changed (2026-11-14, five more crates under the function-size gate)
- `crates/text`, `crates/core`, `crates/esp32-protocol`, `crates/cli` and `crates/telemetry` set
  `#![warn(clippy::too_many_lines)]`, and `check-rust-function-sizes.sh` scans them. Two functions were over 80 lines
  in the hub's count: the CLI's `main` (92 in clippy's count, `#[expect]` and a row) and telemetry's
  `run_monitor_loop` (a row only: clippy counts it under 80 without its comments). Left out: the GUI, the demos,
  the ESP32 firmware crate, the C++ bridge and the renderer (`BACKLOG.md`).
- `crates/gui` too: `try_build_ui` (the GTK window, `#[expect]` and a row) and `submit_frame` (a row only, under 80
  in clippy's count).
- `crates/burn_demos` too: its `main` (`#[expect]` and a row) and `onnx_detector_demo` (a row only).
- `crates/cxx_bridge` too, with no function over the limit.
- `crates/webgpu_renderer` too: eleven of its long passes are over 80 in clippy's count and carry an `#[expect]`
  and a row; eleven more (shorter in clippy's count, or in its integration tests, example and wasm demo) a row
  only. Only the ESP32 firmware crate is left, since it builds for the board.

### Changed (2026-11-14, one guard for a cancel while the model loads; a client that leaves is tested)
- `kataglyphis_text::after_load` runs a generation on what a loader opened, or gives `None` when the cancel fired
  while it loaded (a backend clears the token as a run starts, so that cancel would be lost). ObservANT's
  `Assistant::run` and the app's `src/api/text.rs` each had their own copy; both call it now, the app still
  clearing its shared token first and answering an empty cancelled turn, ObservANT refusing the question.
- `web.rs` test `a_client_that_leaves_cancels_its_answer`: a held model gets the cancel within a second of the
  client closing its socket mid-answer, as the docs promise. Unit test in `crates/text`; recipe
  `consumer.oxidant-text-cancel-while-loading-lost`.

### Changed (2026-11-14, the small text rules live once, in `kataglyphis_text`)
- **`Provider` parses its own name** (`impl FromStr`): `cpu` or nothing, `cuda`, `dml`; any other name is
  `ProviderUnavailable`. ObservANT's `ask.rs` took an unknown name for the CPU (only `config.rs` stood in the way)
  and `src/api/text.rs` refused it with its own copy of the match; both parse through it now. The CLI keeps its
  typed `--provider` values.
- **`FinishReason::as_str`** gives `stop`, `length` or `cancelled`, where `ask.rs` and `src/api/text.rs` each
  spelled them out. **`text_default_options`** is built from `GenerationOptions::default()`, not a second list of
  the same numbers.
- Unit tests in `crates/text` (every name, three refused ones, the three reasons); recipe
  `consumer.oxidant-text-unknown-provider-on-cpu`.

### Added (2026-11-14, `/healthz` has goldens OmniAccelerANT can read)
- `crates/observant/tests/fixtures/healthz/{idle,visit,problem}.json`: what `/healthz` answers for a camera with an
  ONNX Runtime model, idle, during a visit (boxes, a kept clip, the stats) and with a model run that has not come
  back for 30 s. `healthz_matches_its_goldens` in `status.rs` fails when `to_json` differs from them, with the clock
  and build fields set to fixed values; `OBSERVANT_UPDATE_GOLDEN=1` rewrites them. OmniAccelerANT parses these files
  instead of its copy of one answer. A renamed key fails the test (proven by a mutation in the image; no manifest
  recipe, since this repository's mutation lane runs without GStreamer).

### Changed (2026-11-14, ObservANT's own builds use its profile)
- `observant_build_producer` (`scripts/linux/observant/lib/producer-container.sh`) builds
  `cargo build --profile observant`, and `run-producer-pi.sh` runs `target/observant/kataglyphis_observant`, so
  the camera service built here is release with `panic = "unwind"` (owner decision of 2026-11-13): a panic in one
  answer costs a 502, not the camera. Built once in the arm64 image: the profile compiles and the binary prints its
  config. `docs/observant.md` shows the profile's path; `test-observant-producer-container.sh` holds the command.
  OmniAccelerANT's packages take it at their next pin of this repository.

### Fixed (2026-11-14, ObservANT names a frame of the wrong size)
- `push` in `crates/observant/src/capture.rs` took the first `frame::size(width, height)` bytes of a sample, so a
  longer buffer (row padding, the 854-pixel shear of 2026-11-11) was cut and a shorter one skipped, without a word.
  `exact_frame` now ends the capture with an error that names both sizes and row padding as the likely cause; the
  loop logs it and backs off. Unit test with a buffer 4 bytes too long and one 4 bytes too short.
- **The other three silent fallbacks around a size are said or refused.** A model that names no fixed input size
  logs "640x640 assumed" in both backends (`dims_from_nchw` in `ort_backend.rs` and `tract_backend.rs`); an output
  shorter than its shape is an error in `postprocess.rs` instead of fewer rows; and the media appsink keeps why a
  sample is no frame (no buffer, no caps, caps that are no video, a buffer it cannot map) as the capture's end,
  where the bus said only "internal data stream error". One unit test each.

### Added (2026-11-14, genai.rs's hand-typed GenAI signatures are checked against the header)
- `scripts/linux/check-genai-signatures.py` takes the declarations of the 37 symbols `crates/text/src/genai.rs`
  binds (its optional `OgaGeneratorParamsSetGuidance` lookup included) from the image's `ort_genai_c.h` and compares
  them with `crates/text/genai-signatures.txt`. The Linux `test` step runs it first, so a GenAI bump that changes a
  signature fails before the build and prints the old and the new line; `--write` renews the snapshot.
- Tests: `test-genai-signatures.sh` (a declaration over two lines, a commented-out one, a changed and a vanished
  signature, no header) and the step suite; recipes `consumer.oxidant-genai-signatures-unchecked`,
  `consumer.oxidant-genai-lookup-uncounted` and `consumer.oxidant-genai-check-not-in-test-step`.

### Changed (2026-11-14, AGENTS.md knows the Windows GenAI has llguidance)
- `AGENTS.md` § 5 still named Windows as the lane whose GenAI lacks llguidance; both chains build it since 254b7ea,
  as `docs/text.md` says. The sentence now says so; a GenAI built without it still refuses a schema with
  `TextError::JsonSchemaUnsupported`, and that unit test stays.

### Added (2026-11-14, a per-test record of the Rust tests)
- **The Linux test step writes `logs/test-results/cargo-test.xml`.** `scripts/linux/cargo-test-junit.py` turns the
  step's `cargo test` log into JUnit, also after a failed run: one case per test with its outcome, an ignored test's
  reason, a failed test's captured output, and one failed case `(binary ended early)` naming the signal for a binary
  that died before its summary, as the golden suite's SIGSEGV did. The lane uploads it in
  `lane-reports-<suffix>`, and `quality-report.toml` declares it (`cargo-test`), so `quality-report.sh --compare`
  grades tests passed and skipped. Stable libtest prints no per-test time; each suite carries its binary's total.
- Tests: `test-cargo-test-junit.sh` over two recorded logs (verbose with a failure, a crash and doc-tests; plain with
  colour codes and the slow-test notice) and the step suite; recipes `consumer.oxidant-junit-crash-dropped` and
  `consumer.oxidant-junit-failure-unrecorded`.

### Changed (2026-11-13, egui 0.37; the vendored egui-winit is gone; owner request)
- **The egui family moves from 0.36 to 0.37** (`egui`, `egui-winit`, `egui-wgpu` in `crates/gui` and
  `crates/webgpu_renderer`); wgpu stays at 30. 0.37 carries emilk/egui#8516, so the vendored, patched
  `third_party/egui-winit-0.36.2` and its `[patch.crates-io]` row are deleted; `cargo check -p
  kataglyphis_webgpu_renderer --target wasm32-unknown-unknown` builds without them. `egui::Id::new` is deprecated in
  0.37; the overlay's detection layer uses `Id::unique` with the same source. The lock gains `wl-clipboard-rs`,
  `petgraph`, `os_pipe`, `tree_magic_mini`, `fixedbitset` and `unicode-properties` through `egui-winit`'s clipboard.
- **The license policy follows.** `accesskit` 0.25.2 and the six egui crates at 0.37.0 publish no license file;
  their entries in `packaging/cargo-license-policy.json` take the originals from accesskit's commit f3026222 and from
  egui's tag 0.37.0 (the crates name commit 7c0b4040, which is not on GitHub); every text is byte for byte the one
  reviewed before. `unicode-properties` gets its reviewed expression `(MIT OR Apache-2.0) AND Unicode-3.0`, as
  `unicode-ident` has. All four packaged targets stage.
- Proved on the arm64 dev host: clippy `-D warnings` on `kataglyphis_gui` and `kataglyphis_webgpu_renderer` with all
  targets, the wasm32 check, and `kataglyphis_gui`'s tests. The renderer's GPU tests and the Windows build run in CI.

### Added (2026-11-13, a hard gate on function size; owner decision)
- **A new function over 80 lines fails clippy in four crates.** `crates/observant`, `crates/inference`,
  `crates/media` and the root crate set `#![warn(clippy::too_many_lines)]`; `clippy.toml` sets the limit to 80.
  The four functions over it (`encode_mp4`, `handle`, `run` in ObservANT, `WebcamEngine::start`) carry
  `#[expect(clippy::too_many_lines)]`.
- **A frozen function may not grow.** `scripts/linux/check-rust-function-sizes.sh`, run by `pre-push-checks.sh`
  and so by the lint lane, holds every function over 80 lines in those crates to its row in
  `rust-function-size.allow`; a new one or a grown one fails. Suite `test-rust-function-sizes.sh`, recipes
  `consumer.oxidant-rust-size-*`. The renderer, the GUI and the demos follow (BACKLOG).

### Changed (2026-11-13, four owner decisions)
- **The default score threshold is 0.25 everywhere.** `KATAGLYPHIS_SCORE_THRESHOLD` defaulted to 0.5 while
  ObservANT and the bridge floor used 0.25, the value measured at the cameras; one picture gave different results
  per product. The variable's default is now `DEFAULT_SCORE_THRESHOLD`, 0.25 (hub CON235). **A caller that relied
  on 0.5 sets the variable.**
- **The endpoint sends `top_p` and `top_k` only when the caller moved them.** `chat_body` sent both with every
  sampled request, OmniAccelerANT's web client neither, so a strict server could refuse `top_k` on one path only.
  Each is now sent only when it differs from its default (1.0 and 50); the server chooses otherwise.
- **ObservANT has a build profile of its own.** `[profile.observant]` inherits release and sets
  `panic = "unwind"`, so a panic in one answer costs a 502 and not the camera. Nothing builds with it yet:
  OmniAccelerANT's package script is the next step (BACKLOG).
- **The arm64 lane runs once a week.** `linux-arm64.yml` gets a `schedule` (Sundays 03:43 UTC), an exception to
  the on-request rule as `linux-riscv64.yml` has one: it had run for 3 of 32 commits to the camera code.

### Added (2026-11-13, the detector checks what the model says about itself)
- **A model that names another preprocessing or another layout is refused at load.** ModelANT's models carry
  `kataglyphis.preprocess` and `kataglyphis.output` in their ONNX metadata; the Rust detector read neither, so a
  later model that wants letterbox, or rows in another layout, would have given wrong boxes without an error.
  `PersonDetector` now reads both keys from the file before a backend loads
  (`crates/inference/src/person_detection/model_says.rs`) and fails with both names when they contradict
  `KATAGLYPHIS_PREPROCESS` or the one layout it decodes, `xyxy_score_class`. The keys come from the file's
  `metadata_props`, so ONNX Runtime and tract, the Android backend, get the same answer. A model without the keys
  loads as before, and no default changes. C++ and Python hold the same check.
- Tests: three unit tests and one load test per backend with files that hold only metadata; the shipped model
  still loads. The open question of the two default score thresholds (0.5 and 0.25) is the hub's CON235.

### Changed (2026-11-13, a tract that stands in for ONNX Runtime is a problem; owner decision)
- **`/healthz` reports the stand-in.** When ONNX Runtime does not load and tract runs in its place, the camera goes
  on detecting and `/healthz` gives `ok: false` with
  `the model runs on tract, ONNX Runtime did not load: <ORT's reason>` (the first 200 characters). Before, only
  `backend: "tract"` showed it. A camera without a frame, a run that hangs and a model that fails are named first.
  A tract that was asked for by `KATAGLYPHIS_ONNX_BACKEND=tract` is no problem.
- **The detector keeps ORT's reason.** `PersonDetector::fallback_reason()` is `Some` only for a tract that stands in.
- Tests: `healthz_calls_a_tract_that_stands_in_for_onnx_runtime_a_problem` and one more assertion in
  `crates/inference/tests/ort_fallback.rs`.

### Fixed (2026-11-13, a model run that hangs or takes minutes is noticed)
- **Boxes of a run long ago are dropped.** The capture loop drew the last boxes on every frame whatever their age,
  so with `privacy = "pixelate"` their area stayed sharp while the model hung, and whoever walked through it was
  shown. Boxes now count for two runs at the slowest rate a cat in view is looked at, each with the model's own
  time, and for 2 s at least (`box_lifetime` and `fresh` in `crates/observant/src/capture.rs`); after that the
  list is emptied, which pixelates the whole frame, and `/healthz` lists no box. This also ends a box that stayed
  on screen after the picture went dark.
- **`/healthz` says when the model does not answer.** A run in flight for more than 20 s gives
  `ok: false` and `problem: "the model has not answered for N s"`; before, `health()` judged the camera, the
  frame age and three failed runs, never a run in flight. The run's time or its failure ends the problem.
- **A stop keeps the last sighting although the worker hangs.** `main.rs` persists before it joins the capture
  worker, and again after.
- **A start in the app does not wait for ever.** `SessionSlot::start` waited without a bound for the session
  before to let go of the camera; it now gives up after 10 s with `the session before still holds the camera`,
  keeps waiting for that session at the next start, and takes the camera once it has ended.
- Five unit tests. Not done: ending the run itself (`RunOptions::terminate`), kept in `BACKLOG.md`; no hung run
  was provoked on a real model.

### Added (2026-11-13, `/healthz` names the detector's backend)
- **ObservANT's `/healthz` has `backend` (`ort` or `tract`) and `execution_provider` (`cpu`, `xnnpack`, ...).**
  Both are read from the detector once, when the service builds its `Status`, and both are `null` without a model.
  The two keys are new and no other key changed, so OmniAccelerANT's parsers read the body as before.
  `healthz_names_the_backend_and_the_provider` in `crates/observant/src/status.rs` holds them.
- **A fall back to tract is visible, and still no problem.** `ok` stays true when ONNX Runtime did not load and tract
  runs. Whether the service unit refuses the fall back or `/healthz` reports it as a `problem` is the owner's
  decision, kept as a blocked entry in `BACKLOG.md`.

### Changed (2026-11-13, the text contract comes from the ModelANT pin)
- **`crates/text/tests/contract.rs` compiles in `third_party/ModelANT/contracts/text-generation-v1.json`.** The hand
  copy `crates/text/tests/contracts/text-generation-v1.json`, which nothing compared with the original, is deleted;
  its bytes equalled the file at the recorded gitlink (`a6eaca1`). No script and no other test read the copy.
- **The photo is held to ModelANT's copy.** `the_photo_is_byte_for_byte_the_one_modelant_holds` in
  `crates/inference/tests/real_model.rs` compares `crates/inference/tests/fixtures/two_cats.jpg` with
  `third_party/ModelANT/contracts/fixtures/two_cats.jpg`. The photo stays here: ModelANT cites it as the original, and
  OmniAccelerANT reads that path.
- **No lane stages a new file.** The contract and both photos are compiled into the test binaries, as the inference
  contract already is, so `scripts/windows/Stage-CrossTests.ps1` and the riscv64 lane are unchanged.

### Changed (2026-11-12, `migraphx` loads AMD's plugin module)
- **`KATAGLYPHIS_ORT_DEVICE=migraphx` registers AMD's plugin module `libmigraphx-ep.so` by path (owner, 2026-11-12;
  hub CON231 step 2).** The hub removed the in-tree MIGraphX provider from `:latest-rocm` on 2026-11-08, so the by-name
  append had nothing to reach and the AMD path was dead (on the arm64 CPU build it answers `MIGraphX execution
  provider is not supported in this build`).
  `crates/inference/src/ort_plugin.rs` holds one row per plugin (the registration name, the variable that names its
  root, the module's file name) and takes the module from `$ORT_AMDGPU_EP_ROOT/lib` when that variable is not empty and
  the file is there, else from the directory of the loaded ONNX Runtime library, where a package carries it. It
  registers the module once per process as `MIGraphX`, never unregisters it, and appends exactly one of the devices
  listed under that name, with `device_id` as the only option.
- **The provider's name is `MIGraphX`, no longer `MIGraphXExecutionProvider`.** `ProviderUnavailable` and the log say
  `MIGraphX`. AMD's module takes the registration name as the name of its devices and of its provider (read in its
  source), so an ONNX Runtime profile should say it too, and `scripts/verify-ort-gpu-profile.py --device migraphx`
  expects it. The device name `migraphx` and the label `execution_provider()` returns are unchanged.
- **`migraphx_mem_limit` is no longer passed.** The plugin has no such option and fails the creation of its provider
  on an option it does not list.
- **A named `migraphx` that cannot load still fails; it does not become a CPU session.** No module, a module ONNX
  Runtime cannot register and a module that lists no device are each `ProviderUnavailable: MIGraphX`, with the paths
  tried or ONNX Runtime's reason; `auto` logs the reason and tries the next choice. The HIP copy check runs before
  the registration, which already asks HIP for its devices.
- **The AMD package carries the CPU ONNX Runtime, the plugin module and the plugin's texts.**
  `package-linux.sh --ort-variant amd` takes ONNX Runtime from the CPU prefix (the rocm image has no GPU build), keeps
  the ELF closure from patching the module, and stages the image's plugin licence tree and `provenance.txt` as
  `licenses/onnxruntime-ep-amdgpu/`; the hub has no helper for those texts, so
  `scripts/linux/lib/ort-runtime-bundle.sh` has one. `verify-gpu-packages.sh` compares the module and the texts with
  the image's in the tar, the deb and the AppImage and refuses an AMD tar or deb with a `libonnxruntime_providers_*`
  file. The package checks clear `ORT_AMDGPU_EP_ROOT`, so they load the package's own module.
- **`linux-x64-amd-gpu.yml` proves the CPU core against its wheel, requires the image's module, and runs the packaged
  launcher without a card:** `migraphx` must fail with `ProviderUnavailable: MIGraphX failed to register` naming the
  packaged module, and `auto` must run on the CPU.
- **`ort`'s `migraphx` feature is gone from `crates/inference`**; it only built the in-tree wrapper. `Cargo.lock` is
  unchanged.
- **Not proved.** No line of this ran with `libmigraphx-ep.so`: this machine is arm64 without an AMD card, and the
  rocm image is amd64. Proved here against the image's ONNX Runtime 1.31.1: the three failures above (Qualcomm's
  plugin module stands in for a module that registers and lists no device), the registration guard, and the append
  call with the CPU provider's device. The AMD package build, the workflow and every run on a card are open
  (`BACKLOG.md`). The hub pin moves from d66d390 to da9b23a, whose `verify_ort_profile.py` expects `MIGraphX` for
  the parity run. CUDA keeps its present loading (`BACKLOG.md`).
- **A registration another library of the process made is used.** AccelerANTgine and this crate can share one
  loaded ONNX Runtime in one app, and ONNX Runtime refuses a second registration of a name. `migraphx` now takes the
  devices that are already listed under `MIGraphX` instead of failing with `already registered` (found by
  AccelerANTgine's review of the same move).
- **The AMD package's ONNX Runtime is proven as the stripped twin of the wheel's library.** The first run of the
  candidate lane on `:latest-rocm` (run 43303579930) stopped in its first step, `expected one wheel matching every
  native library; found 0`: the step and `verify-gpu-packages.sh` asked for the wheel's exact bytes, which holds for
  the NVIDIA prefix only. The image strips the CPU core, the one an AMD package carries. Both ask for exact bytes on
  NVIDIA alone now; the package check's test and one recipe hold it.

### Changed (2026-11-13, the hub's hook pair and the census option)
- **A push runs what the lint lane runs.** The hub pin moves to the commit that holds the consumer hook pair and
  the census option. `scripts/linux/pre-push-checks.sh` is the lint lane's script suites in one file:
  `lint-gates.yml` calls it, and the hub's pre-push hook runs it after the lint gates, so a suite a pin move turns
  red stops the push (as it did not on 2026-11-12). The wrapper loses its own stale-recipe loop: the hub's gates
  check staleness now. `AGENTS.md` § 5 names the one config line.
- **The AMD package passes the hub's census the supported way.** `package-linux.sh` tells the census the
  launcher's library path (`--library-path lib/glibc:lib`) for the AMD variant: the module must stay the image's
  bytes and has no run path of its own, which stopped the second candidate build on `:latest-rocm` (run
  43314831939). The wiring test and one recipe hold the call. The next run (43345664993) passed the census, showed
  the packaged module refusing a request by name without a card (`HIP failure 100`) while `auto` took the CPU, and
  stopped at the hub's AppImage verifier, which still asks an AMD AppImage for exact wheel bytes (`BACKLOG.md`).

### Added (2026-11-12, ModelANT's decode cases are replayed)
- **The detector replays the decode cases of ModelANT's `inference-v1.json`.** The contract held four cases, all for
  preprocessing; since ModelANT a6eaca1 it also holds the five rules for end-to-end rows (a finite score at least
  the threshold, a class inside the range, finite corners in order, no overlap suppression, boxes cut to the frame)
  as raw rows with their expected detections. `postprocess.rs` runs every case through `parse_yolo_like_detections`
  and compares the list in row order; a case ModelANT appends is replayed without an edit here. The `ModelANT`
  gitlink moves from 9e8ffde to that commit.

### Changed (2026-11-12, rows no detector of the family keeps)
- **A row with no such class or with its corners swapped is no detection (owner, 2026-11-12).** AccelerANTgine
  dropped such rows; this decoder and OrchestrANT's kept them, so a class past the 80 names reached the caller with no
  label, swapped corners became a box of negative width, and the bare cast made a NaN class a person. The class is
  checked as the float it is and the corners must be in order; a row whose corners are equal, a point, stays.

### Fixed (2026-11-12, thinking models on the text path)
- **A thinking model's reasoning is counted and kept out of the answer (audit genai-native-4).** The endpoint
  parser read only `delta.content`, so the reasoning of llama-server and vLLM (`reasoning_content`) and of Ollama
  (`reasoning`) vanished without a trace, and a model in process would have given its `<think>` notes as the answer.
  `parse_stream_line` yields them as the new `StreamEvent::Thinking`; `<think>...</think>` inside the text is cut out
  for both backends, also a block that never closes, and a tag split over two pieces is held back the way a stop
  string is. `GenerationResult::thinking_chars` counts what was thinking.
- **A `</think>` without its `<think>` ends thinking, and the answer starts with its first word (owner,
  2026-11-12).** Qwen3's template opens the block in the prompt on some servers; everything before the lone tag is
  thinking, as OrchestrANT's `answers.py` reads it, so `result.text` is the text after it (`on_text` has shown the
  earlier text by then). The white space a model writes directly after `</think>` is dropped.
- **`/ask` and `POST /summary` say when the budget went into thinking.** The 502 for a reply without words names the
  thinking count and the two ways out, a larger `llm_max_new_tokens` or thinking switched off at the endpoint; a
  model that ended by itself gets `it only thought` without the budget advice. A successful `/ask` carries
  `thinking_chars`.
- **Not in this change.** No option switches thinking off, a plain `llm_base_url` still cannot send the switch, the
  bridge's `TextEvent` and the CLI's figures do not carry the count. Nothing was run against a real thinking
  model: the tests replay stream lines written in each server's shape.

### Changed (2026-11-12, ObservANT offers H.264 first on Windows)
- **On Windows the camera service lists H.264 before VP8 in its offer (owner, 2026-11-12).** Since the Windows chain
  ships the VP8 plugin, viewers took VP8, the sender's default first codec, and got no picture: OmniAccelerANT's
  installer test shows the VP8 packets reaching `vp8dec`, which never outputs a frame and logs nothing. H.264 is the
  codec that test proved before. The other codecs keep their order and stay on offer, and other systems keep the
  sender's own order.

### Changed (2026-11-12, hub pin)
- **The hub moves from 62bf45f to d66d390 (159 commits).** CI already builds in the image and with the actions of the hub's
  `develop`; the pin only decided which copy of the hub's scripts and gates this checkout runs, and it had fallen behind
  the image (owner, 2026-11-12: move all pins). No shared file this repo takes from the hub changed, and the
  consumer-pin, shared-config and ratchet gates pass at the new pin.

### Changed (2026-11-12, the local gates see stale mutation recipes)
- **`scripts/linux/run-lint-gates.sh` checks the mutation recipes first.** A refactor that moves a line a recipe names makes
  the recipe stale, and only CI's mutation gate said so, after the push. The hub's `verify_mutations.py --stale-check`
  takes a tenth of a second and runs before the gates now. `core.<pid>` and `vgcore.*` files are ignored, so a test that
  aborts a child cannot be committed with its dump again.

### Changed (2026-11-12, JSON schemas on Windows)
- **The schema test expects an answer on Windows too.** The Windows chain builds GenAI with llguidance now (hub LAI-H2),
  so the test that asserted `JsonSchemaUnsupported` there failed on the first Windows lane run with the new image: the
  generator answered `{"a":"#$%"}`. One test holds the answer to the schema on both lanes; the mapping of GenAI's
  "guidance is unavailable" to `JsonSchemaUnsupported` keeps its unit test.

### Fixed (2026-11-12, an endpoint's error as plain text)
- **A streamed `{"error": "..."}` is the generation's error.** Only OpenAI's nested `error.message` was read; Ollama
  sends the text itself, which was skipped, so the run ended with "the stream ended without a finish reason" and the
  server's own words were lost.

### Fixed (2026-11-12, a stop that does not wait)
- **`stop_webcam_inference()` returns at once.** It is a sync call on Dart's UI thread and joined the worker, so the app
  froze for the running inference and the camera driver's close, and for a whole model load when Stop was pressed while
  a start was under way. It asks the session to end; a thread of its own waits for it, and the next start waits for that
  thread, since the camera is free only then. A stop during a start ends the new session as soon as it exists.
- **`text_cancel()` while the local model loads ends the turn.** The backend clears the token as it starts, so the
  cancel was lost and the whole answer generated; the turn ends with `cancelled` and no token.
- **`kataglyphis_cli chat` stops generating when nobody reads it.** After the first failed write (`chat | head`) it
  decoded the rest of the answer before it reported the broken pipe.
- **The wgpu GUI takes its boxes away when inference is switched off.** The boxes of the last inferred frame stayed over
  the live picture, and a frame still in flight brought them back.

### Fixed (2026-11-12, questions the model did not answer)
- **A model that answers nothing is a 502 that says so.** A model that spent its token budget thinking returned no text,
  and `/ask` and `POST /summary` passed that on as a 200 with an empty answer.
- **A client that left while the model loaded is not answered.** The backend clears the cancel token as it starts, so the
  cancel was lost and a whole answer was decoded for nobody while every other question got 503.
- **A request body that never comes is refused after 10 s** with 408 on `/ask`, `/summary` and `/settings`. The limit on
  the request head ends with the head, so such a client held one of the 128 connection slots for as long as it liked.
- **An endpoint's error body is kept when it is cut inside a character or is not UTF-8.** It was read as a string, which
  kept nothing of it, so the reply named a status and no reason.
- **A cancel ends an endpoint's generation at once.** It was seen only when the server's next line arrived, so while a
  slow server worked on its first token a client that had left kept `/ask` busy, and `text_cancel()` did nothing. The
  stream is read on a thread of its own and the generation polls its token every 20 ms.
- **The test that crashes a child on purpose leaves no core file.** Where dumps are on it wrote 19 MB into
  `crates/inference/`, which the large-file ratchet then refused.

### Changed (2026-11-11, webcam preview, capture errors, detector copies)
- **The webcam preview keeps the camera's rate.** The engine pushed a frame to the Flutter texture
  only when it took one for the model, so the preview ran at the inference rate (about 6 frames a
  second on a Pi 5). `CaptureSession::start_observed` now calls an observer for every captured
  frame, and the texture push is that observer; the model still takes the newest frame when it is
  free.
- **A webcam session that ends says why.** The capture pipeline's bus was never read: an unplugged
  camera or refused caps ended the frames in silence. A bus handler now closes the frame slot with
  the pipeline's error (`FrameSlot::error`), the engine reports it once through its event callback
  (`Result<EngineEvent, String>`), and `start_webcam_inference` sends that, a failed start and a
  model that fails 30 runs in a row to Dart as stream errors. A finished session no longer blocks
  the next start with "already running".
- **The camera index picks the camera on Linux.** `CameraSource::Device(n)` was ignored wherever
  the source is not Media Foundation or kernel streaming; it now builds the n-th listed device's
  own source element.
- **Capture caps ask for square pixels and prefer the asked frame rate.** Without the pixel aspect
  ratio a 4:3 camera asked for 1280x720 arrived stretched; the frame rate in `CaptureConfig` was
  never used. A camera without that rate still starts, at its own.
- **The detector copies less per frame.** ONNX Runtime reads the preprocess buffer through a
  `TensorRef` instead of an owned copy (4.9 MB at 640x640), and preprocessing divides each column
  offset out once per frame and normalises through a 256-entry table. The floats are unchanged, bit
  for bit. On a Pi 5, 640x480 to 640x640: 2.5 ms to 1.8 ms, plus 0.9 ms for the copy.
- A frame nobody took lends its buffer to the next one, so a consumer slower than the camera no
  longer costs an allocation per frame.

### Fixed (2026-11-11, padded widths, hung probes, NaN boxes)
- **ObservANT refuses a width GStreamer pads.** I420 rows are padded to four bytes, luma and
  half-width chroma alike, and `frame.rs` reads them tightly packed: at 854x480 a frame is
  616320 bytes, not the 614880 the service sliced, so picture, model input and snapshot sheared.
  `width` must now be a multiple of 8.
- **A camera probe that hangs is killed.** `run_with_timeout` gave up on `rpicam-hello` after its
  timeout but left the process and its reader thread, one more with every retry.
- A detection with a NaN or infinite corner is dropped; it passed every clamp and reached the
  overlay as a box that cannot be drawn.
- A capture pipeline that fails to start is set to Null, and its error names the element and the
  bus message, not just the failed state change.

### Changed (2026-11-04)
- The GPU lanes no longer install `libgtk-4-dev`: `cargo build -p kataglyphis_cli` links without it on the published
  `:latest-rocm` (`05518138`) and `:latest-nvidia` (`70bc45db`, amd64). The lanes themselves have not run without it yet.

### Fixed (2026-11-04, debug GUI panic)
- **Every debug `kataglyphis_cli gui` and renderer `viewer` died on their first frame.** Both applied egui's texture
  deltas by reference and then dropped them, and epaint's debug-only drop check panics on a delta left in place
  (`Dropped TexturesDelta with 1 unapplied deltas`). Release builds drew correctly. Both now drain the deltas, and the GUI
  frees textures on a failed frame too. `gui_finishes_its_first_frame` runs the real window path under Xvfb, and the
  renderer has a headless test ([testing.md](docs/testing.md#the-whole-suite)).
- **The GUI aborted when it drew before the first camera sample, in release builds too.** It issued the camera quad's
  draw with no bind group, and wgpu's validation error ended the process. Whether it hit depended on which came first,
  the first redraw or the first sample. The GUI now skips that draw until a frame exists.

### Changed (2026-11-04, a too-small runner)
- **A job the hub's `prepare-linux-ci-host` refused for runner disk re-runs alone** (`retry-disk-refusal.yml`, hub CON216). Any other
  failure stays failed. `On request`, `release.yml` and GPU candidate lanes are not named, so they never re-run.

### Added (2026-11-04, LAI-O4)
- **ObservANT answers questions about its visits.** `POST /ask` gives a language model the camera's figures of the last
  seven days and returns its answer with those facts. `GET /summary` serves one local day's computed figures; `POST
  /summary` adds the model's words, and a day without visits asks no model. The model is a GenAI directory or an endpoint
  (`llm_model_dir`, `llm_base_url`, `llm_backend`), loaded at the first question. Unconfigured, both POSTs answer 503.
  Both are refused from any page but the camera's own, and `/healthz`'s `ask` says whether a model is configured
  ([observant.md](docs/observant.md#questions-about-the-visits)).

### Added (2026-11-04, LAI-O5)
- **GenAI embeds, through `crates/text`.** `GenaiGenerator::embed` reads a decoder's `hidden_states` output, takes the
  mean over the text's tokens and scales it to unit length. `kataglyphis_cli embed --model-dir DIR` and the bridge's
  `text_embed` with a `model_dir` use it. A model exported without that output fails with
  `TextError::EmbeddingsUnsupported`, naming `include_hidden_states=true`. Proven on the hub's tiny model; no released
  model has the output yet ([text.md](docs/text.md#embedding)).

### Fixed (2026-11-03, a card without a CUDA kernel)
- **A card the NVIDIA packages have no kernel for gets a message, or the CPU.** `person_detection/cuda_probe.rs` runs a
  one-`Relu` model on CUDA before the real session. On `cudaErrorNoKernelImageForDevice`, `cuda` fails with
  `ProviderUnavailable` naming the card and its compute capability, and `auto` moves on. It used to create the session,
  fail on the first node and, on `auto`, stop with no CPU run. Every other probe failure stays for the real session to
  report. Seen on an RTX 2080 (compute capability 7.5) with the images' `86;87;89;120`.

### Added (2026-11-03, fresh registry)
- **A unit test keeps webrtcsink out of OxidANT's scanner-less packages**, whose in-process scan of a fresh registry could
  leave it without codecs. OmniAccelerANT's ObservANT package carries both and proves the worst order.

### Changed (2026-11-03, allow debt)

- **`test-detector-parity.py`'s preflight test delegates to two helpers** and leaves `function-size.allow`.

### Fixed (2026-11-03, MIGraphX hang)
- **A MIGraphX load no longer waits without end for an AMD GPU that does not answer.** Under WSL on an RX 9070 XT the first
  copy to the GPU never returns (hub CON202). The load now tries one copy first and fails with `ProviderUnavailable`
  after 30 seconds; `auto` goes on to the CPU. There is still no CPU fallback for `migraphx`.

### Changed (2026-11-03, unsafe blocks)
- **`crates/text/src/genai.rs` has three `unsafe` blocks fewer.** The optional guidance symbol shares `bind!`'s lookup, and each
  read-then-release of a GenAI string is one block, so the repo is back at its 34 blocks.

### Changed (2026-11-03, detector model)
- **The detector model is `deim_dfine_n_coco-v2`.** v1 finds nothing on DirectML, where MatMul of a matrix by a
  1-D vector computes wrongly (hub CON220).

### Fixed (2026-11-02, riscv64 text contract)
- **The `linux-riscv64` lane runs the text contract again.** `riscv64-test` fetches `llm.lock` and runs
  `crates/text/tests/contract.rs` apart, on scalar MLAS with GenAI and the model required, because ORT
  1.31.0's RVV `MatMulNBits` reads null scales on the 4-bit model (`docs/ci.md` § The riscv64 lane,
  `BACKLOG.md`). Every other test stays on RVV. `test-ci-container-steps.sh` holds the wiring, with five
  mutation recipes.

### Changed (2026-11-02, hub ORT package checks)
- **`verify-gpu-packages.sh` calls the hub's `ort-package-proof.sh`** instead of carrying its ORT library and
  notice checks, with `--reject-invalid --reject-links` keeping its stricter rule. Exit statuses are unchanged.

### Added (2026-11-02, perf-host)
- **`quality-report.sh --perf-host` latencies** in `quality-report.toml`, with
  `scripts/linux/perf-host.sh`, the CLI's `detect-latency` and a tracked `perf/baseline-<cpu>.json`.

### Changed (2026-11-02, AGENTS.md back to what to do now)
- **The lint gates hold it there.** `run-lint-gates.sh` and `lint-gates.yml` run the hub's
  `verify_agents_drift.py`.
- **`AGENTS.md` shrank from 1,025 to about 440 lines.** The lane detail moved to the new
  `docs/ci.md`, the cat cam to `docs/observant.md`, the Windows container to `docs/windows.md`, the
  bin rename and package closure to `docs/packaging.md`, the WSL recipe to `docs/getting-started.md`,
  coverage to `docs/testing.md`, the renderer's documents and large files to `docs/crates.md`.
  Section 5 names every workflow, and `docs/text.md` and `docs/ci.md` are in `docs.yml`'s guides.

### Changed (2026-11-02, one owner per fact)
- **The COCO class names come from the hub.** `crates/core/src/coco-classes.txt` is the synced copy
  of `shared/config/coco-classes.txt`, held byte-identical by the lint gates' shared-config check;
  `coco.rs` parses it at compile time, so a short, long or CRLF copy fails the build.
- **`models.lock` owns the model's file name.** `MODEL_FILE` compiles from its one `.onnx` row;
  Linux packaging reads it through `scripts/linux/lib/models-lock.sh`, Windows packaging and the
  GPU candidate workflow through `scripts/windows/Get-ModelLockFile.ps1`, and the parity check
  through the hub's lock parser. `test-models-lock.sh` runs the GPU profile against a fixture lock
  naming a different model and fails a second tracked owner of the name.
- **`embeds_marker` lives in `kataglyphis_core::provenance`**; ORT and GenAI re-export it.
- **`kataglyphis_media::frame_sink` makes every frame sink**, in the capture session, ObservANT
  and the GUI. The GUI's sink no longer sets `emit-signals`, which only the signal API reads.

### Changed (2026-11-02, shared script helpers)

- The Linux GPU check scripts share `scripts/linux/lib/gpu-check-common.sh`; the argument parsers of `clippy-windows.sh`, `check-ort-chain-only.sh`, `package-linux.sh` and `run-producer-pi.sh` share `scripts/linux/lib/cli-args.sh`. It stays local because the hub's `parse_cli_value_options` accepts an empty value, which these scripts refuse.
- The Linux suites share `tests/lib-suite.sh` and the GPU fixtures `tests/lib-fake-gpu.sh`; a mirror test copies those companions beside the script it copies.
- The renderer pages take their origin and path note from the crate README. Duplicated blocks fall from 47 to 16 without any allow row.
- `frb-codegen.sh` keeps its private parser: OmniAccelerANT's tests copy that file alone, so a sourced helper would break them.

### Fixed (2026-11-01, webrtcsink on a fresh registry)
- **A supplied plugin scanner keeps the registry scan out of process.** Since c7d3593
  `ensure_gst_initialized` added `GST_REGISTRY_FORK=no` even when the caller set
  `GST_PLUGIN_SCANNER`, as OmniAccelerANT's Linux launcher and `install::adopt` on Windows do.
  The in-process scan then registered webrtcsink before `vpx` and the RTP payloaders whenever the
  filesystem listed it first, and ObservANT's first start on a fresh registry failed with
  `No codec available for encoding stream video_0` (OmniAccelerANT runs 41275429526, 41278759760).
  A scanner in `GST_PLUGIN_SCANNER` or `GST_PLUGIN_SCANNER_1_0` now leaves `GST_REGISTRY_FORK`
  unset; `a_caller_scanner_keeps_the_scan_out_of_process` holds it.

### Added (2026-11-01, language AI)
- **`seed`, `json_schema` and `stop` in `kataglyphis_text`.** `GenerationOptions` carries them to GenAI
  (`random_seed`, `OgaGeneratorParamsSetGuidance("json_schema", ...)`, stop strings in the decode loop with
  `finish_reason` `Stop`) and to endpoints (`seed`, `response_format`, `stop`); the CLI takes `--seed`,
  `--json-schema` and `--stop`, and the bridge's `TextOptions` the same three fields. A GenAI without
  llguidance, the Windows chain's, refuses a schema with the new `TextError::JsonSchemaUnsupported`.
- **`resources/models/llm.lock`** with OrchestrANT's SmolLM2-135M-Instruct row, fetched by
  `scripts/linux/fetch-models.sh --llm` in the test and coverage steps and by `Invoke-DebugTests.ps1` into
  `target\llm-models`, and **`crates/text/tests/contract.rs`**, which replays ModelANT's text-generation-v1
  capital-of-france case to its exact ids and `Paris`.
- **GenAI tests for the options** on the hub tiny model, whose tokenizer llguidance accepts (`BACKLOG.md`
  § Recorded probes), and **five mutation recipes** on the hosted lane: the seed and the guidance in
  `genai::configure`, greedy decoding, and the endpoint's `seed` and `response_format`.

### Changed (2026-11-01, tests that can fail)
- **Per-crate coverage floors sit within 3 points of their figures.** Raised from runs 41256294473
  and 41275077677: observant 83, inference 85, text 94, cli 87, core 80, gui 29; the others were
  already close.
  `check-crate-coverage.py --max-slack 3` warns about a larger slack without failing, and a crate
  without a floor now fails. Ten new recipes in `ort-license-mutations.json` prove both changes.
- **The lane uploads the reports quality-report reads.** The `-all` artifact listed
  `tarpaulin-report.xml`, which tarpaulin never writes; it now carries `cobertura.xml`. A new
  `lane-reports-<suffix>` artifact holds `cobertura.xml` and `logs/code-quality/rustdoc.txt` from
  the new `rustdoc-log` step, so coverage and rustdoc warnings are collected at the same SHA. The step
  turns cargo's colours off: the lane's `CARGO_TERM_COLOR=always` hid every warning from the count.

### Added (2026-11-01, cv)
- **Any-class detection over the bridge.** `detect_rgba(rgba, width, height, classes)` runs the
  default model and keeps the given class ids (empty keeps all), `detector_labels()` returns the class
  names by id, and `WebcamStreamConfig.classes` filters the live stream. `detect_persons_rgba` and
  `start_webcam_inference` stay as wrappers with their tests unchanged. `DetectionEvent` names its
  `backend` and `execution_provider`. `src/frb_generated.rs` is regenerated; OmniAccelerANT must
  regenerate its Dart side with this pin.
- **XNNPACK and DNNL through `KATAGLYPHIS_ORT_DEVICE=xnnpack|dnnl`.** A named provider that is
  missing or fails to register returns `ProviderUnavailable` (`kataglyphis_inference::provider`),
  never a CPU session; DNNL is `ProviderUnavailable` off x86_64. `PersonDetector::with_ort_device` and
  `execution_provider` are new; `onnx-detect --tract` writes tract's results.
- **The test step's backend parity check.** `scripts/linux/check-backend-parity.sh` compares ORT on CPU
  against tract, XNNPACK and DNNL on ModelANT's `two_cats.jpg` with the hub's
  `compare_detector_parity.py` and checks each ORT profile with `verify_ort_profile.py`. Five
  `consumer.oxidant-backend-parity-*` mutation recipes. Hand-run mutant: a channel swap in tract's
  input fails it (`Detector parity failed: CPU and GPU detections do not match within tolerance`).

### Changed (2026-11-01, cv)
- **The COCO class names moved from `kataglyphis_observant::coco` to `kataglyphis_core::coco`**, so
  the bridge reaches them on every target; observant re-exports the module.
- **The `:latest` amd64 image's DNNL provider cannot load**: `libonnxruntime_providers_dnnl.so` needs
  `libdnnl.so.3`, which the image does not ship (hub BACKLOG CON191). `dnnl` is `ProviderUnavailable`
  there until a chain ships oneDNN; `cpu_accelerators.rs` then requires DNNL to run.

### Fixed (2026-11-01)
- **The riscv64 media test keeps QEMU's loader and CPU settings for its isolated child.**
  The existing plugin-path isolation and explicit library-path handling are preserved.

### Changed (2026-11-01, bench-history)
- **Benchmarks run only the `[[bench]]` targets, after the release build.** `ci-container-steps.sh bench`
  calls `cargo bench --workspace --locked --bench '*'`: `--benches` had also run all 16 library and binary
  unit-test harnesses under the bench profile, which compiled 532 crates in 23 min 24 s (job 124574433901).
  It runs in its own *Benchmarks* job with its own sccache key (`<arch>-bench`): the bench profile builds
  every dependency with `panic=unwind`, unlike the release build's `abort`, so it shares nothing with the
  release build, and inside the build job's 1.5 GB cache its 213 crates were evicted before each run
  (8 min 07 s and 10 min 07 s in runs 41260356146 and 41268852734).
  In that job a cold cache took 11 min 08 s (job 124777515954) and a warm one 4 min 33 s (job 124793215868).
- **The hub's `bench-history` action grades the means** (keys `criterion-x64`, `criterion-arm64`) against
  the median of the default branch's last 20 runs, read from 90-day artifacts instead of a 7-day cache. The
  gated benches are the five from 100 µs, as before; `draw_box` only warns.
- **Removed `scripts/linux/bench-report.py` and `scripts/linux/tests/test-bench-report.sh`**: the hub's
  `bench-history.py` prints the identical table on that suite's fixtures, and its own suite covers the cases.

### Changed (2026-11-01)
- **PowerShell complexity is strict.** `powershell-quality.yml` passes `strict: true` on push and PR
  after the three findings reached 0: `Build-Windows.ps1` runs each build step from a named function,
  and `New-GpuCandidate.ps1` and `Stage-CrossTests.ps1` check their inputs in helper functions. The
  build steps, their order and their checks are unchanged.
- **Python static analysis is strict.** `python-static.yml` passes `strict: true` to the hub's
  reusable workflow on every push, PR and dispatch, after the Ruff findings (12) and ty diagnostics
  (16) reached 0: shebang scripts got their executable mode, regex and import-spec results are
  narrowed before use, the detector parity runner takes explicit `cwd`/`env` parameters, and the
  burn lock checker returns an exact three-part version and raises `TypeError` for a non-string field.

### Added (2026-10-31)
- **`scripts/linux/quality-report.sh`** runs the hub's `quality-report.sh` on this checkout; the
  tracked `quality-report.toml` declares the docs build list, lane outputs and repo metrics it reads.
  The code-quality-report workflow sets `quality-report: true`, so every push and PR is graded against
  the newest successful develop record and uploads `code-quality-measurements`.
- **Security audit lane and enforced `sha-pin`.** `security-audit.yml` calls the hub's lockfile audit on push, PR, Mondays and dispatch; the lint gates arm the hub's `sha-pin` workflow convention.

### Added (2026-10-31)
- The `fmt-clippy` step regenerates `src/frb_generated.rs` with the hub-pinned generator, after full
  cross-target `cargo metadata --locked`, refuses a Dart loader stem other than `oxidant`, and fails
  when the committed file differs. `scripts/linux/frb-codegen.sh` is the one bridge input and flag
  set; OmniAccelerANT's generator calls it. cargo-expand is pinned at 1.0.127 by checksum. See
  [docs/testing.md](docs/testing.md#the-bridge-bindings).

### Security (2026-10-31)
- **cargo-deny checks every build target.** `deny.toml` adds `x86_64-pc-windows-msvc`,
  `aarch64-pc-windows-msvc`, `aarch64-linux-android` and `wasm32-unknown-unknown` to the three Linux
  targets; `cargo deny check advisories licenses bans sources` (0.20.2) passes on all seven with no new
  allowance.
- **The endpoint backend redacts its key and bounds stream lines.** `Endpoint`'s `Debug` prints
  `api_key` as `<redacted>`, and its server-sent stream reader refuses a line over 1 MiB with
  `TextError::StreamLineTooLong` instead of buffering it. See [docs/text.md](docs/text.md).

### Added
- Allow scoped reusable GPU workflow calls and independent Windows GPU vendor selection.
- Allow NVIDIA Linux candidate runs to select x64, arm64 or both architectures.
- Prepare an opt-in Linux AMD x64 and NVIDIA x64/arm64 checked-artifact stage with current-attempt receipts; promotion remains blocked until detector output parity is verified on hardware.
- Add a real-image ONNX detector diagnostic that can repeat inference in one session.
- Prepare CPU/GPU detection comparison for checking each finished Linux GPU package.
- Scope Windows GPU candidate uploads and hardware profiles to the current workflow attempt without overwriting prior artifacts.
- Scope Linux GPU candidate uploads and their clean-host and GPU checks to the current workflow attempt without overwriting prior artifacts.
- Reject Linux GPU DEBs with incorrect embedded package identity, target architecture or GPU vendor version.
- Allow the NVIDIA Linux and Windows GPU candidate workflows to be called by other workflows while keeping hardware profiling optional and outputs unverified.
- **Language models (LAI-O1 to O3).** `crates/text` generates text on the chain's ONNX Runtime GenAI,
  loaded at run time from the chain install and refused unless it embeds the chain's build tree, or on
  an OpenAI-compatible endpoint by URL or `backends.json` name, with streaming, cancel and the speed
  figures. The CLI gains `chat` and `embed`; `src/api/text.rs` streams the same to Dart. The suites
  decode the hub's tiny GenAI model, which ANTfrastructure ad6177f6 carries, and CI requires it
  through `KATAGLYPHIS_REQUIRE_GENAI=1`. See [docs/text.md](docs/text.md).
- The GenAI backend's time to the first token includes the prefill: its clock starts before the prompt is
  encoded, not after GenAI took it in.
- The Cargo license policy reviews ring 0.17.14 (`Apache-2.0 AND ISC`), which the CLI now ships through
  the endpoint client's rustls; its five original texts, the once_cell polyfill's included, are pinned by SHA-256.

### Fixed
- Register MIGraphX through ORT's generic provider options, avoiding the legacy struct's malformed empty model-cache path.
- **riscv64 tests no longer try the amd64 GStreamer plugins.** ANTfrastructure is pinned at 0aef805d,
  whose `riscv64_cross_env` drops the host-triplet entries from `GST_PLUGIN_PATH`,
  `GI_TYPELIB_PATH` and `LD_LIBRARY_PATH`; each test process had warned about 300 times.
- **Patched source copies stay in the lock graph.** Lock maintenance moved tract to 0.23.9, past the
  clang-cl copy in `third_party/tract-linalg-0.23.8`, and cargo only warned. `tract-onnx` is pinned
  to `=0.23.8` as Burn is, and `scripts/linux/check-cargo-patches-used.py` fails when any
  `[patch.crates-io]` copy is missing from `Cargo.lock`; the `--apply` wrapper runs it, and
  `test-cargo-patches-used.sh` proves it red.
- `Cargo.lock` refreshed within the current versions (tokio 1.54.1, ureq 3.5.0, cc 1.10.0, ICU 2.4).
  ort rc.15 and egui 0.37 stay deferred: ort changed its session and environment API, and
  egui-winit 0.37.0 still lacks the wasm32 fix the vendored 0.36.2 copy carries.
- The Cargo license policy follows clipboard-win to 5.4.3; its release commit's LICENSE is byte-identical
  to the reviewed copy. The policy suite's mutations find crates by name, not version, and assert their
  own step succeeds, so a lock refresh can no longer turn one into a vacuous pass.
- Cover `kataglyphis_cxx_bridge`'s crate graph in the reviewed Cargo license policy so BeschleunigerBallett can ship its texts, and stop the Windows license suite leaking the verifier's expected failure status into the Pester runner.
- Ship the original license texts of every crate the CLI links, per packaged target, in all Linux and Windows formats, using ANTfrastructure's Cargo license stager and a reviewed policy for crates whose archives lack them; Windows payloads that ship the chain ONNX Runtime also carry the matching wheel's license and notices.
- Generate the MSI's installed RTF from the complete project license, retaining the existing installer flow.
- Require the project logo for MSIX creation and reject missing or invisible images.
- Match bundled Linux ONNX Runtime libraries to one wheel and verify its original license and notices in every package.
- Stage the project license in Linux bundles and Windows portable, MSI and MSIX payloads using ANTfrastructure's shared helpers.
- Use the project logo for the Windows MSI shortcut and Add/Remove Programs entry.
- Embed the project icon in the Windows CLI using ANTfrastructure's shared Rust build helper.
- Remove the unused optional Burn Torch/ZIP/Zstd dependency chain from `Cargo.lock` through a
  documented `burn-dispatch` 0.22.1 patch; the strict cargo audit now needs four fewer exceptions.
  A CI lock-graph mutation test and Renovate's reviewed Burn pin prevent the chain returning.
- Fix rustdoc's README and ObservANT guide links to the shared producer helper; they now lead to
  its source on GitHub instead of a missing path in the published site.
- Share ANTfrastructure's Windows config selection and source-copy loop fixture with
  Ballett while preserving this repo's path validation and shader mutation assertions.
- Stage the owned `forward.slang` source with Windows arm64 test artifacts so both native
  renderer shader-constant tests can read it through their embedded workspace path.
- The shared riscv64 cross environment isolates GStreamer pkg-config from the amd64 image's
  metadata, and its smoke rejects x86_64 link paths before the workspace tests run.
- The README's shader guide uses a GitHub permalink, so its link also resolves
  when the README is included as the crate's rustdoc front page.
- Shared shader comments now point to OxidANT's color-space guide; the migrated
  sources no longer refer to a documentation path found only in BeschleunigerBallett.

### Changed
- Profile the Linux GPU tarball, DEB payload and AppImage separately on the
  hardware runner, so each finished package must execute model operators on
  its requested provider before the optional GPU check passes.
- Pass Windows container root exclusions to ANTfrastructure's shared selector;
  keep the existing Rust output policy and deep tar exclusions.
- Share ObservANT's container binary probe and Cargo build transport with
  OmniAccelerANT; keep this Pi runner's explicit `--build` policy and image entrypoint.
- Share the arm64 stage-directory Pester assertions with OrchestrANT through an
  ANTfrastructure fixture; keep the Cargo-specific reset command and callback here.
- Request direct submodules in both Windows build lanes, avoiding the
  ANTfrastructure/DocumANTation recursion during checkout.
- Reuse ANTfrastructure's empty-stage directory helper for Windows cross-test
  staging; test stale cleanup and failed allocation.
- Pin ANTfrastructure's shared Bash/PowerShell reuse scanner so CI reports
  consumer-to-hub overlap candidates.
- Pin ANTfrastructure to the Renovate driver that updates both direct submodules to their
  remote default branches and initializes missing checkouts on apply, including WSL over
  Windows Git checkouts.
- Verify the Renovate wrapper and shared config in CI without applying updates.
- Reuse the hub's PowerShell quality workflow for automatic complexity reports and a manual
  strict limit check; keep existing strict analyzer coverage while refactoring two findings.
- **Shared contracts and standalone shaders** (2026-10-22): preprocessing runs the
  ModelANT RGB/RGBA, stride, offset and letterbox references; each concurrent request
  retains its model even if another path replaces the bounded cache. Texture ABI types,
  result codes and header are generated from one contract, and loaded plugins must
  report the expected version before Rust resolves their frame callback.
- **GTK4 builds on Linux** (2026-10-22): `gui-gtk` uses the image's GTK4 >= 4.10,
  keeps its bus watch through the session and stops video on shutdown. Linux tests
  require its lifecycle check under Xvfb; packages include GTK's closure and video sink.
- **The README header shows OxidANT's own logo, `images/logo.svg`** (2026-10-21, owner): a rusted
  ant inside a gear, from the family set DocumANTation's `scripts/make_repo_logos.py` draws.
  `images/logo.png`, the package icon and the rustdoc logo, is rendered from it at 256 px.
- **The docs site carries the guides** (2026-10-21, ANTfrastructure's rustdoc builder takes `guides`
  since that day). `docs.yml` passes all eight `docs/` pages; each is rendered in rustdoc's layout
  beside `oxidant/index.html` and listed atop every page's sidebar, and a link into the repository
  leads to GitHub. Proof in `:latest`: 8 guides, 478 of 594 pages navigated, the sidebar check green
  on the crate page and every guide at 1400, 700 and 400 px, the link gate 595 documents with its 24
  frozen findings.
- **The cxx bridge is its own crate, `crates/cxx_bridge`** (`kataglyphis_cxx_bridge`, 2026-10-21).
  BeschleunigerBallett's engine calls one function, `rusty_cxxbridge_integer`, and took it from the
  root staticlib, which since 3.0.0 links GStreamer and the ORT loader on Linux and Windows. The new
  crate depends on `cxx` alone (native-static-libs on Linux: `-lstdc++ -lgcc_s -lutil -lrt -lpthread
  -lm -ldl -lc`), and BeschleunigerBallett imports it with `CRATES kataglyphis_cxx_bridge`. The root
  package loses `src/native_only.rs`, `cxx` and `cxx-build`; its `build.rs` only sets `cfg(desktop)`.
  **Breaking for a `corrosion_add_cxxbridge` over `oxidant`'s `native_only.rs`**: BeschleunigerBallett
  was the only one and moves with its gitlink.
- **3.0.0: no Cargo features any more; every target builds what it can run** (2026-10-20, owner:
  "They are no longer just an opt-in feature. They will stay there forever"). Every `[features]`
  table is gone, each dependency is unconditional or `[target.'cfg(...)'.dependencies]`, and the code
  gates on `cfg(target_os)` and the root's `cfg(desktop)`. **Breaking for anyone passing
  `--features`**: cargo now refuses the names, OmniAccelerANT's `KATAGLYPHIS_RUST_FEATURES` included.
  - Linux (x64, arm64, riscv64) and Windows (x64, arm64) build the whole workspace: tract and the chain
    ORT with CUDA registered (DirectML on Windows), GStreamer capture, the webcam engine, the ESP32
    bridge, the wgpu GUI and the burn demos. `aarch64-linux-android` builds `oxidant` with the
    detector on tract. `wasm32-unknown-unknown` builds it without the detector: tract built there and
    took the release wasm from 0.66 to 15.5 MB, but the model loads from a file and the web has no file
    system. The calls a target cannot serve answer so; no frb signature changed. The GTK4 GUI waits
    for GTK4 in the Linux image (`BACKLOG.md` has the change).
  - ORT first, tract as the fallback, loudly: with `KATAGLYPHIS_ONNX_BACKEND` unset the detector warns
    `Falling back to tract: <ORT's reason>`, and a model neither loads names both reasons.
    `PersonDetector::with_backend` and `backend_name` are new. `KATAGLYPHIS_ORT_DEVICE=auto` on an
    ORT without the CUDA provider now runs ORT on the CPU, where it dropped to tract.
    `model_errors.rs`, which compiled out when both backends were on, tests each backend's error and
    the pair's; `real_model.rs` checks that ORT is the backend and that tract finds both cats too;
    `ort_fallback.rs` (new) points `ORT_DYLIB_PATH` at nothing and checks the warning and tract's result.
  - The burn demos are their own crate, `crates/burn_demos` (`kataglyphis_burn_demos`, bin
    `burn_demos`, no longer `burn-demos`: a hyphen in `CARGO_BIN_EXE_*` does not survive the hub's sh
    sccache launcher), so `oxidant`, which both consumers link, carries no Burn; neither does it depend
    on `kataglyphis_gui` any more. The demos return their results and have 19 tests (each demo, the
    binary, the detector demo on the real model), which found `binary_cross_entropy` using `1 - pred`
    where `1 - target` belongs. Plots use plotters' `ab_glyph` and the system's TrueType font
    (`fc-match`, then the usual files; Lucida Console in Server Core) instead of font-kit, whose
    pkg-config link paths put the distro's `libstdc++` ahead of GCC 16.2's under tarpaulin, where the
    chain ORT then failed to `dlopen`.
  - The GUI's wgpu has no GL backend, which its `GpuBackend` never picks: the `opengl32.dll` import
    kept the CLI from loading in Server Core. The GUI crate, untested until now, has 15 tests, and the
    CLI's integration tests run `media-check`.
  - `kataglyphis_media::ensure_gst_initialized` sets `GST_PLUGIN_SYSTEM_PATH(_1_0)=""`,
    `GST_REGISTRY_FORK=no` and a per-copy `GST_REGISTRY` beside the bundled `GST_PLUGIN_PATH`, each
    only when unset (the hub's rule for a package's GStreamer).
  - **The Linux packages carry their closure**: `scripts/linux/package-linux.sh` replaces the hub's
    one-binary `package_archive.sh` call; the bundle has the image's GStreamer, GLib, FFmpeg and GCC
    runtime (`elf-closure.sh`), the chain ORT unpatched, the GUI's plugins, the model and the image's
    glibc for an older host (`bundle-loader.sh --beside`), and is proven before packing. The new
    `clean-host` job installs and starts the `.deb`, the tarball and the AppImage on a bare runner.
  - CI: `TEST_FEATURES`, `FEATURE_CHECK_SETS` and the `feature-check` step, `linux-x64.yml`'s
    `feature-matrix` job and its `feature-checks-per-job` input, `Invoke-WindowsConfigMatrix.ps1`
    (a featureless `base` and four feature builds; `Invoke-AppProfiles.ps1` now builds and runs the
    full CLI in debug and release, since its `profile` build ran the 16 GB runner out of LLVM memory), `Build.CargoFeatures`/`CARGO_FEATURES` and every `--features` in the
    Windows scripts are gone, and so is `deny.toml`'s `all-features`. Coverage now measures
    `crates/media`, `src/webcam_engine.rs`, the GUI and the burn demos, which no module hides behind
    a feature any more: 70.98 % of 12357 lines (72.9 % of fewer lines on 2026-10-19), with new floors for
    the three crates and `src` raised to 19 %.
  - Proof, 2026-10-20: in `:latest` every container step of the Linux lane (720 tests passed, the
    bench, release, docs and the packages with their closure, G6 and element checks), the riscv64 step
    under QEMU (720 passed, 8 min 33 s), `oxidant` for `aarch64-linux-android` (the NDK's clang) and
    `wasm32-unknown-unknown` (the web lane's nightly and flags) under `clippy -D warnings`, and the
    packages on a bare `ubuntu:26.04` and on `debian:13` (glibc 2.41, so the bundled glibc); in
    `:winamd64` the x64 lane's container half (381 tests, the CLI in each profile, `Build-Windows.ps1`'s
    14 steps) and the bundle's `onnx-runtime` and `media-check` on the host.
- **Dependabot is off** (2026-10-20, owner: dependency upgrades go only through Renovate as a local
  CLI, ANTfrastructure's `docs/dependency-updates.md`). `.github/dependabot.yml` (cargo and GitHub
  Actions) is deleted and the repository's Dependabot security-update PRs are disabled; Dependabot
  alerts stay on. `renovate.json` and `docs/updates.md` no longer call it present.
- **The README logo is the family's ant** (2026-10-17, owner: one icon in every repository),
  drawn from OmniAccelerANT's `assets/icons/kataglyphis_app_icon.svg`.
- **The README's hero image is an animated WebP** (2026-10-17): `images/Rust.gif` was 7.5 MiB, the same
  136 frames as `images/Rust.webp` (ffmpeg's libwebp, quality 70) are 2.1 MiB, with no visible loss
  side by side. A checkout and every README view load 5.4 MiB less.
- **The README is a short overview** (2026-10-19, owner: the family's README rule, ANTfrastructure's
  `docs/repository-family.md`). 671 lines became 83; the rest moved to eight new guides in `docs/`
  (`getting-started`, `crates`, `testing`, `running`, `observant`, `windows`, `packaging`, `updates`),
  and the contributing, licence and contact boilerplate to the family page. Its links are absolute,
  since the README is also the `oxidant` crate's rustdoc front page. Corrected on the way: the test
  and clippy commands take the lanes' four `TEST_FEATURES` and the `KATAGLYPHIS_REQUIRE_*` trio,
  ObservANT's default config is `observant.toml`, zune-core is no longer held at 0.5.1, the image's
  WiX is 7, Renovate's `--apply` is no longer gitlinks only, and the crate table lists the two ESP32
  crates.
- **The release runs on the hub's `release-publish.yml`** (2026-10-19, ANTfrastructure c3fc7585):
  `release.yml`'s version check and release job are its two stages, and the hub's
  `collect-release-assets.sh` picks the files by `release.yml`'s `collect:` options.
  `scripts/linux/collect-release-assets.sh` and its suite are gone; the hub's suite holds OxidANT's
  uploads. On one fake release run in `:latest`, the old and the new collector gave the same 14
  files, byte for byte, the zips' members included.
- **`run-producer-pi.sh` collects the host libcamera closure with the hub's `elf-closure.sh`**
  (2026-10-19): the four roots go in under their SONAMEs, then `--ldd --no-default-system
  --no-runpath` with the same eight C-runtime names as `--system`. In `:latest`, over the image's
  libcamera, libcamera-base and two IPAs (25 libraries) and over `libgstlibcamera.so` plus the chain
  ORT (36), the old walk and the new one hold the same SONAMEs with the same bytes; only the old
  walk's real-name copies beside its soname links (`libcamera.so.0.7.2`) are gone. A library ldd
  cannot find now stops the collection instead of being left out.
- **The script suites run on the hub's `lib/test-harness.sh`** (2026-10-19):
  `test-check-crate-coverage.sh` (8 assertions) and `test-bench-report.sh` (12) drop their own
  `expect`/`check` counters for `t_assert_rc`, `t_assert_grep` and `t_summary`, and now report every
  failed assertion instead of stopping at the first. `lint-gates.yml`'s `script-tests` job checks the
  hub submodule out for it. In `:latest` both pass with the old counts, and four planted bugs in
  `check-crate-coverage.py` and `bench-report.py` fail them.
- **`fetch-models.sh` and `Get-Models.ps1` run the hub's `fetch-data-lock.py`** (2026-10-19,
  ANTfrastructure CON98): `python3 -I` and `python -I` on `resources/models/models.lock`, the
  callers' `--dest`/`-Destination` and `--lock`/`-Lock` passed on. Windows now keeps a cache too
  (`%LOCALAPPDATA%\kataglyphis-models`), and the cache's old bare `<sha256>` entries still serve.
  In `:latest` the old and the new `fetch-models.sh` wrote the same three files, `0644`, byte for
  byte; in `:winamd64`, where `python` is `C:\temp\cpython\PCbuild\amd64\python.exe` (3.15.0) and
  `python3` is not on `PATH`, the old and the new `Get-Models.ps1` did too.
- **The generated-artifacts check is the hub's suite** (2026-10-19, ANTfrastructure 1a174d9c).
  `scripts/windows/tests/Repo.GeneratedArtifacts.Tests.ps1` is gone; `lint-gates.yml`'s
  `generated-artifacts` job runs the hub's `shared/windows/tests/Repo.GeneratedArtifacts.Tests.ps1`,
  which reads the same ten pathspecs from `scripts/windows/generated-artifacts.txt`, then this repo's
  own suites in a second step. Under Pester 3.4.0 and 6.1.0 it passes 2/2 on this tree; a `docs/*`
  line fails it on 9 tracked files, and a `**/__pycache__/` line is refused.
- **`Build-Windows.ps1` reads the MSIX version with the hub's `Get-PackageVersion -VersionFile
  'Cargo.toml' -Required`** (2026-10-19, ANTfrastructure 1a174d9c). `Get-CargoMsixVersion` and its
  four cases in `CargoTarget.Tests.ps1` are gone; the hub's reader takes `[workspace.package]` before
  `[package]` the same way and throws on a `Cargo.toml` without a version. This tree gives `2.3.4.0`
  (MSI `2.3.4`) as before, and `MSIX_VERSION` still overrides it.
- **`Invoke-WindowsLane.ps1` moves the CI compiler cache with the hub's `Restore-CiCompilerCache
  -Cache sccache, cargo` and `Save-CiCompilerCache`** (2026-10-19, ANTfrastructure 1a174d9c), in place
  of its own robocopy pair; it still sets `RUSTC_WRAPPER` and the `SCCACHE_ERROR_LOG` directory.
  In `:winamd64` with stub lane steps, the old and the new script leave the same trees, a failing
  step included: sccache and `registry\cache` back in `CI_COMPILER_CACHE`, `registry\src` and
  `git\checkouts` left behind. A cache that cannot move back now warns instead of failing the lane.
- **`Invoke-HostTests.ps1` grades with `Invoke-StagedTests.ps1 -AssertVerdict`** (2026-10-19,
  ANTfrastructure 1a174d9c) instead of parsing the `TESTS:` line itself. On the host and in
  `:winamd64`, over synthetic manifests, the old and the new script give the same exit codes and
  counts for a pass, a failure, a run whose one test skipped itself and a missing binary; only the
  messages say "staged test(s)" where they said "host test(s)".
- **`Invoke-StevedoreBuild.ps1` runs on the hub's `Invoke-RepoContainerBuild.ps1`** (2026-10-19,
  ANTfrastructure 1a174d9c). The driver keeps only its spec; the hub streams the sources into a
  reused `kata-rust-build` container by tar-pipe and `target\container\<profile>` and `logs` back,
  and checks that every exe built there reached the host. `Build-RustAll.ps1` and `Test-RustAll.ps1`
  take the runner's `-Workspace` and import `WindowsContainerLog.Common` through
  `Resolve-BuildModule.ps1`, where they read `C:\ws-mnt` and `C:\host-scratch`. Gone: the bind
  mounts, the `%LOCALAPPDATA%` scratch dir, `-StageSources`, `-StagingDir` and `-Docker` (now
  `-DockerExe`); new: `-FreshContainer` and `-WhatIf`. `-Test` in `:winamd64` (rustc 1.99.0, a new
  container): debug 1m46s, profile 1m50s, release 1m15s, 325 tests passed in 25 binaries, 6m09s in
  all; 2 exes per profile verified on the host, and `release\kataglyphis_cli.exe stats` runs there. A
  second build-only run in the kept container took 12 s.
- **AGENTS.md and the shipped texts say what is true** (2026-10-19). AGENTS.md: Renovate's
  `--apply` writes every ecosystem, not gitlinks only; the zune-core hold is gone (`Cargo.lock`
  has 0.5.3; its history is in `docs/updates.md`); the clippy, test and bench commands take the
  lanes' four `TEST_FEATURES`; the image's `CARGO_HOME` belongs to uid 1001, so the feature job
  drops its `-e CARGO_HOME`; the crate list names the two ESP32 crates. ObservANT's `--help`,
  its config and COCO comments and the Pi runner's message no longer say YOLO (the model is
  DEIM-D-FINE-N). The root package's description, the CLI's `about` and the MSI and MSIX
  descriptions no longer call it a project template. `docs/crates.md`'s literature keeps the
  renderer's shading references and drops the Vulkan, C++ and path-tracing ones.
- **The docs link the hub instead of restating it** (2026-10-19, ANTfrastructure a447ef3c).
  AGENTS.md's CI rule, workflow names and concurrency, https submodule rule, CRLF fix, riscv64
  mechanism and QEMU traps and history rewrite point at the hub's pages, keeping this repo's
  tokens, release contents and exceptions. `docs/getting-started.md`'s container recipe is the
  one measured that day (PowerShell `nerdctl`, `target/` and the cargo home on named volumes,
  the uid 1001 chown), not a copy of the hub's; `docs/updates.md` keeps this repo's Renovate
  facts. `doc-dupes-hub.allow` turns on the hub's `doc-dupes --against-hub` gate, with the
  owner's docs rule as its one row. Corrected: `[ci linux]` runs the riscv64 lane too, and
  `.github/renovate.json` says `branch = develop`.
- **The docs site has its own workflow** (2026-10-19, ANTfrastructure 89b098c9). `docs.yml` calls
  the hub's `build-docs.yml` with `builder: rustdoc`; the Linux lanes lose the `docs` input, the
  *Create docs* and FTP steps and the `SERVER`/`USERNAME`/`PW` secrets, which `docs.yml` maps onto
  the hub's `FTP_*`. The site documents all 11 crates (the root package alone before), carries the
  hub's `.htaccess` and 404 page, and fails on a dead internal link; `docs/site-links.allow`
  freezes the 20 that rustdoc copies from `tracing`'s blanket `WithSubscriber` docs. It publishes
  on a push to `develop` or `main`, never cancelled, where the lane published from its last step
  with `continue-on-error: true` and not at all after a red step. The site job in `:latest`: 518
  documents, 20 findings, 20 frozen, `.htaccess` and `404.html` in `target/doc`.
- **`resources/web/combined-theme.css` is no longer tracked** (2026-10-19). ANTfrastructure's
  `cargo_build_doc.sh` writes it from the family theme and `rustdoc-mapping.css` on every docs build
  and is its only reader (`--extend-css`), so every theme move left a diff in the checkout (558
  lines in, 114 out after the move to 89b098c9). It is in `.gitignore` now; the Windows bundle, MSIX
  and MSI, which copy `resources/` whole, no longer ship it.
- **AGENTS.md drops "The image's Rust may be older than its own pin"** (2026-10-19). In `:latest`
  (`sha256:b52f8c34…`, the registry's current tag) `rustc` is `/usr/local/cargo/bin`'s 1.99.0 in login
  and non-login shells, the hub's `RUST_VERSION=1.99.0`; Ubuntu's 1.93.1 in `/usr/bin` sits behind it,
  and the hub's cargo drivers hoist the pinned toolchain and log it (`rust toolchain: … rustc 1.99.0`).
  The section is now *The trap a Windows checkout hits before anything runs*, the CRLF one.

### Fixed
- **Every Windows build compiles C and C++ with clang-cl** (2026-10-21). The arm64 cross lane and the
  local Stevedore driver (`Build-RustAll.ps1`, `Test-RustAll.ps1`) left `CC` and `CXX` unset, so
  cc-rs took MSVC's `cl`; they set both now, as the x64 lane does. cc-rs 1.8 adds the triple itself
  (`--target=aarch64-pc-windows-msvc`) and archives with `llvm-lib`.
- **The Windows real-model tests run instead of passing by skipping** (2026-10-21). Neither lane
  fetched the model before its tests, and `Build-Windows.ps1` ran *Unit Tests* before *Fetch
  Models*. `Invoke-DebugTests.ps1`, `Test-RustAll.ps1` and *Unit Tests* fetch it first and set
  `KATAGLYPHIS_REQUIRE_MODEL=1`; `Stage-CrossTests.ps1` stages the model and the chain ORT for the
  arm64 device, whose test command requires the model too, so there the detector also proves the
  DirectML fallback to the CPU.
- **The coverage figure leaves out the vendored crates** (2026-10-21). tarpaulin counts every file
  under the workspace, so the vendored `third_party/tract-linalg-0.23.8` took the Linux lanes'
  figure to 56.36 % against the 70 % floor (6192c6b x64 run 39560313916, 6aa85f8 arm64 run
  39560662137). The `coverage` step passes `--exclude-files 'third_party/*'`, which drops
  egui-winit's 30 lines too: 78.43 % (8775 of 11188 lines) in `:latest` x64, every crate floor kept.
- **The Windows x64 build works with clang-cl as `CC`, and the lane builds that way** (2026-10-21).
  tract-linalg 0.23.8 (0.23.9 alike) renders its x86_64 kernels as MASM on the MSVC target and
  sends the AVX-512 VNNI pair through cc-rs, so clang-cl, whose GAS probe passes, got MASM text:
  `invalid instruction mnemonic '_text'`, then `failed to run custom build command for
  tract-linalg`. OmniAccelerANT's clang-cl build hit it; this repo's x64 lane stayed green because
  cc-rs fell back to MSVC's `cl` there, against the family rule. `third_party/tract-linalg-0.23.8`
  is the published crate (sha256 `1cc37ab3…a222`, the index's and the old lock's) with one
  `build.rs` line changed, `if !use_masm() && assembler_supports_avx512vnni() {`, routed in through
  `[patch.crates-io]`; its `PATCHED.md` holds an upstream-ready report. The x64 lane
  (`Invoke-WindowsLane.ps1`) sets `CC` and `CXX` to the image's clang-cl. The arm64 cross lane and
  the local Stevedore driver still take cc-rs's default (`BACKLOG.md`).
  - **DirectML says when it is not there.** The detector registered DirectML through
    `with_execution_providers`, which drops a failed registration silently, and then logged
    `ORT DirectML execution provider enabled` regardless. It registers directly now and warns
    `ORT DirectML provider unavailable, running on the CPU: <ORT's reason>` on failure. The
    session's fate is unchanged: the arm64 chain ORT does carry the DirectML EP (`providers:
    ['DmlExecutionProvider', 'CPUExecutionProvider']` on `windows-11-arm`, hub run 39544745432),
    and where no D3D12 adapter matches, ORT refuses the registration (`No devices detected that
    match the filter criteria`) and the session runs on the CPU.
  - Proven in `:winamd64` (2026-10-21): the x64 lane (`Invoke-WindowsLane.ps1 -TargetArch amd64`)
    passed in 32 min with every cc-rs build on clang-cl (`CC = Some(clang-cl)` in the build
    scripts' output), 381 workspace tests in 32 binaries, every `Build-Windows.ps1` step through MSI,
    and the bundle's `onnx-runtime` and `media-check` on the host. With the lane's `CC`,
    `cargo build -p tract-linalg` exits 101 (`'_text'`) from crates.io and 0 vendored. In Server Core
    the detector logged the DirectML warning above and still built its ORT session; `real_model.rs`
    found both cats on ORT there. In `:latest` the Linux steps `ort-chain-only`, `security`,
    `fmt-clippy`, `test` (720 passed, the one known ignore) and `release` passed with the vendored
    crate.
- **rustls-webpki 0.103.16 and webpki-roots 1.0.10** (2026-10-20): RUSTSEC-2026-0382 and -0381, published
  that day, failed both Linux lanes' security step; ObservANT's ureq takes both. webpki-roots 0.26.11,
  lockfile-only behind torch-sys's ureq 2 and with no fixed 0.26 release, joins `.cargo/audit.toml`.
  `deny.toml` drops the paste and ttf-parser ignores, which cargo-deny now calls stale.
- **The WebGPU demo draws in the browser again** (2026-10-20). The docs site's demo stayed white:
  Chrome refused `forward_pipeline_layout` (`bindGroupLayoutCount (5) is larger than the maximum
  allowed (4)`), so all four forward pipelines and every forward pass were invalid, and Tint refused
  `tonemap.wgsl` (`'@location(0)' appears multiple times`). The tile light grid and its indices moved
  from group 4 into group 3 beside the punctual lights (bindings 1 and 2, one layout and bind group in
  `render/forward.rs`), and `tonemap.wgsl`'s vertex stage returns `FullscreenVsOut` itself, both
  regenerated from BeschleunigerBallett's Slang. Native tests never saw it: `GpuContext` asked for
  the adapter's `max_bind_groups`; it asks for `wgpu::Limits::default()`, WebGPU's, everywhere now,
  and `tests/shader_export.rs` fails on a `@group` past 3 and on a repeated `@location`. On the old
  tree those two tests and 57 GPU tests failed under `KATAGLYPHIS_REQUIRE_GPU=1` (lavapipe, `:latest`);
  the new one passes them all, and Chrome 155 on SwiftShader draws the demo with no WebGPU warning.
- **A `.gltf` with `data:` images loads from memory** (2026-10-20). gltf 1.4's `import_slice` refuses
  every image URI when it has no base path, an embedded `data:` one too, so the web demo's "PBR
  textures" scene failed with `external reference in slice only import`. `load_gltf_slice` imports
  the images itself after refusing an external one by name; its two new tests in `tests/headless.rs`
  failed on the old loader with that error, and Chrome 155 now loads all five demo scenes.
- **The Linux lanes' clippy passes again** (2026-10-19). `EngineConfig::on_detections`, added with
  the ESP32 bridge in 0a4f9a7, failed `clippy -D warnings` with `type_complexity` at
  `src/webcam_engine.rs:34`; its type is the `DetectionsHook` alias now. `ci-container-steps.sh
  fmt-clippy` in `:latest` reproduced the error before the change and passes after it.
- **A board found only through `/dev/serial/by-id` reports its MAC** (2026-10-19). Its serial
  number kept udev's interface suffix (`E8:06:90:9D:3E:78-if00`), so `DeviceInfo::mac()` was
  `None` for exactly the board the libudev-less enumeration missed. `crates/esp32/src/discovery.rs`
  cuts the suffix; the new by-id test failed on the old code with that string.
- **The ESP32 bridge is tested, and the Linux lanes' coverage passes again** (2026-10-19). 0a4f9a7
  added 747 measured lines, 99 of them run by a test, so run 39220063931 stopped at `70.00% coverage,
  7468/10669 lines covered` against `--fail-under 70`. The tests check behaviour: the protocol's twelve
  wire tags, a byte-exact Ping frame, the decoder's foreign version, oversized length, unknown variant
  and full buffer, and three proptest properties (`crates/esp32-protocol/tests/framing.rs`); the link
  over an in-memory serial line through the new `Link::start` (handshake retry, events in order, echoes
  ignored, sequence numbers, the detection throttle and per-mille clamp, write and read failures, the
  6 s idle disconnect, close and drop); discovery's port mapping and order; the frb API's event mapping
  and every command with and without a link; the webcam engine's detections hook. The API's connect
  checks and fills the link slot under one lock, so two connects cannot both pass. `ci-container-steps.sh
  coverage` in `:latest`: 72.89 % (7782/10676); `crates/esp32` (92.9 %) and `crates/esp32-protocol`
  (98.8 %) get floors of 91 and 97, `src/` (14.6 %) 13 instead of 5.
- **The rustdoc logo shows on every page of rust.jonasheinle.de** (2026-10-19). `html_logo_url` was
  `../logo.png`, which reaches the site root only from `oxidant/*.html`; one level deeper
  (`oxidant/api/…`, 60 of the 64 logo pages in today's build) it named `/oxidant/logo.png`, a 404.
  It is `https://rust.jonasheinle.de/logo.png` now, the file `cargo_build_doc.sh` copies there from
  `images/logo.png`.

## [2.3.4] - 2026-10-16

### Added

- **Releases** (2026-10-16, owner). Every push to `main` runs the new `release.yml`: a version
  check against `Cargo.toml` and the CHANGELOG, all five lanes, then the tag `v<version>` and a
  GitHub release with the Linux packages for x64 and arm64, the Windows MSI, MSIX and portable
  zip for both arches and a `SHA256SUMS`, only when every lane is green.
  `scripts/linux/collect-release-assets.sh` picks the files (each flatpak gets its arch, the
  portable folder is zipped, test bundles and reports stay out; 9 checks in
  `test-collect-release-assets.sh`, also run on a real release run's uploads), the hub's
  `publish-github-release.sh` publishes them. `linux-x64.yml` no longer runs on pushes to `main`
  itself, since `release.yml` calls it there.

- **ObservANT's `/visits`: every visit of the last 30 days** (2026-10-15). `/healthz` lists only
  the last 20 visits, too few for statistics per day or hour. `Status` now keeps a history of
  `[arrival, stay]` pairs in `visits.json` beside `recent`, at most 30 days and 10 000 visits; a
  file from before starts it from its `recent`, and a restart closes a visit it cut short at
  `last_seen`, as it does for `recent`. `GET /visits` serves it under the same host check and peer
  CORS as `/healthz`. Tests: counting, stays and a restart, the 30-day edge and the cap, an older
  file, and the endpoint.

- **Mutation testing of the capture loop, and the riscv64 lane meets the real model**
  (2026-10-15). cargo-mutants on `capture.rs`: 87 of 124 compiling mutants caught at first, 98
  now. The arrival photo's privacy test only checked that the filtered photo differs, so a
  filter swapped between on and off passed; it now decodes the JPEG and wants the corner far
  from the cat flat with the filter and sharp without. New tests for `bus_says_done` (an error
  with its text, end of stream as done) and for the restart pause, now `sleep_unless`, which
  fails after 5 s instead of hanging when it never ends; a zero width is not divided by. The
  riscv64 test step fetches the model and sets `KATAGLYPHIS_REQUIRE_MODEL=1` like the Linux one:
  its real-model tests reported `ok` having skipped. The Windows lanes do the same and are in
  `BACKLOG.md`, since their tests run before the model is fetched.

- **Mutation testing of ObservANT's frame, clip and stats code** (2026-10-15). cargo-mutants on
  `clip.rs`, `frame.rs`, `stats.rs`, `output.rs` and `precompress.rs`: 376 of 412 compiling
  mutants caught at first, 400 now. The privacy filter had the most gaps, because its tests used
  one-colour chroma: they now check the 15 % margin to the pixel in both directions, that a block
  takes its own mean, and that the colour planes are kept inside a box and pixelated outside.
  New tests also pin the box outline's exact pixels at every thickness and at the frame's last row
  and column, a box with swapped corners, each pixel's own luma and 2x2 chroma in `to_rgba`, the
  clip's frame counts (`frames_kept`, split out of `Clip::spawn`), a new scratch directory per
  encode, and the stats sampler reading `/proc`. The output pipeline's tests fail after 10 s instead
  of hanging when `run` never returns. Of the 12 left, 9 only widen the privacy filter's
  whole-block shortcut, whose per-pixel path paints the same picture.

- **Mutation testing of ObservANT's camera choice, config and webhook** (2026-10-15). cargo-mutants
  on `camera.rs`, `config.rs` and `notify.rs` caught about 90 % of 145 compiling mutants; now 134.
  New tests: a webcam counts by its UVC driver or its USB bus, and needs a format it can decode;
  `rpicam-vid` without a listed sensor leaves the choice to libcamera; a configured `/dev/video`
  path the providers list is that device; every config limit is valid itself (0 inference fps,
  60 s clips, a one-port ICE range, http and https webhooks); the retries fit inside the 10-minute
  age limit; a short webhook host keeps its path out of the log. The 11 left read real camera
  devices or change a log line. Two decisions in `probe` became `probe_rpicam` and
  `asks_providers`, so they are tested without a Pi camera, and `status.rs` uses
  `notify::unix_now` instead of a copy of its own; the behaviour is unchanged. The webhook tests
  fail after 10 s instead of hanging when no event comes.

- **The camera's restart backoff is tested** (2026-10-15). `Capture::run` waits 1 s doubling to 32 s
  between failed captures, starts over after one that ran more than a minute, and re-probes at once
  when a stand-in test pattern ran its course; the audit found none of it tested. The rule is the
  `Backoff` type now, unchanged, with tests for each case and both boundaries.

- **Mutation testing of ObservANT's arrival logic** (2026-10-15). cargo-mutants planted 393 bugs in
  `motion.rs`, `presence.rs` and `status.rs`; the tests caught about 92 % of the 379 that compile.
  New tests for each gap: a box reaches exactly one block around itself and needs two moving, and
  does not wrap past the right edge; a change of 12 is noise and 24 of 4 800 blocks are motion;
  motion within a period waits for the next look; an IoU of exactly 0.5 keeps a detection; only a
  missing visits file falls back to the old name; a name and the ignored spots may reach their
  limits; a frame 21 s old is a problem and 20 s not; the counters and the failure count; visit
  times are wall-clock seconds. Now 375 of 379; the 4 left change a log line or a bound no client
  sees.

- **Telemetry tests that test it** (2026-10-15). `kataglyphis_telemetry` stood at 8.4 % with
  tests that built a struct and read it back, and one that expected the process-wide counters
  at zero, which a second recording test would have broken. Now: `tick` turns frames,
  inferences and their durations into rates over the elapsed time and falls to zero when idle;
  the GPU fields appear only when sampled; `write_line` appends; the monitor thread writes every
  field to its log file and stops on drop.

- **Mutation testing of ObservANT's network-facing code** (2026-10-15). cargo-mutants planted 163
  bugs in `web.rs` and `static_files.rs`; the tests caught about 84 % of those that compile. New
  tests for each gap: every content type, the gzip cache (once per version), route resolution
  (a file, a directory's index, an app route, a missing asset), the peer origin echo, bytes the
  signalling server sends with its 101, a response head split over packets, an endless one cut
  off at 16 KiB before the handshake timeout, and a refusal re-framed without the upstream's
  `Transfer-Encoding`. Now 142 of 145; the 3 left are equivalent (AGENTS.md).

- **Tests from the audit of 2026-10-15** (the owner asked whether the tests catch regressions):
  - *The bridged core is tested with its real features.* `TEST_FEATURES` adds
    `oxidant/gstreamer,oxidant/onnxruntime`, so `src/webcam_engine.rs` and the webcam API
    compile for tests at all; `src/` stood at 6.2 %. Eight tests: the engine on
    `videotestsrc` with the real model, headless, delivers frames in order with an inference
    time and a frame rate and stops its worker (the channel closes); a missing model, a
    missing texture library and a library without `knt_push_frame` each say so; the rolling
    frame rate; the probe and the default plugin name.
  - *Per-crate coverage floors.* `scripts/linux/check-crate-coverage.py` grades each crate
    and `src/` from tarpaulin's `cobertura.xml` against its own floor, so the renderer's
    90 % no longer carries a crate at 8 %. Its suite plants a crate under its floor. The
    floors sit just under that day's figures (observant 82.7 %, renderer 90.8 %, inference
    79.2 %, cli 74.1 %, core 63.6 %, telemetry 8.4 %, `src/` 5.7 %), and tarpaulin gets
    `--timeout 300`. Measuring them showed that tarpaulin 0.37.5 leaves every
    `#[cfg(feature = …)]` module out of its report (`crates/media`, `src/webcam_engine.rs`),
    so those modules' new tests count as tests, not as coverage (AGENTS.md).
  - *Property tests on what ObservANT reads from the network and its config file*, in each
    module's `mod properties`: the Range header (a served range never leaves the file, a
    well-formed one is served as asked), the query, the Host/Origin check (a public name is
    refused unless allowed, an address literal passes), request paths (none, however
    encoded, leaves the web root), the camera spec and the config (a config that validates
    keeps its numbers in range). Two planted bugs, `..` let through and an unclamped range
    end, each fail them with a shrunk input (`..`, `bytes=0-` on one byte). They replace
    `tests/fuzz_test.rs`, which exercised a placeholder function of its own.
  - *`KATAGLYPHIS_REQUIRE_GST=1`* in the test and coverage steps turns the clip and
    `webrtcsink` tests' self-skip into a failure; `cargo test` hides a passing test's skip line.
  - *Benchmarks against the median of the last 20 runs.* `bench-report.py` warns above
    +25 % and fails a mean from 100 µs that doubles, once 3 earlier runs exist; the cache
    keeps a small `.bench-history/` instead of `target/criterion`. `lint-gates.yml` runs the
    script suites (`scripts/linux/tests/`).

- **Criterion benchmarks, tracked in CI** (2026-10-14, owner: benchmarks belong in each project's own
  tooling, for regression testing). `crates/observant/benches/frame_stages.rs` and
  `crates/inference/benches/detector.rs` run under the existing `cargo bench` step, which now fetches the
  model and runs every bench of the workspace. The Linux lanes cache `target/criterion` from the default
  branch per arch, and `scripts/linux/bench-report.py` writes each mean and its change into the run summary,
  warning above +25 %. First numbers on a Pi 5: a detection 261 ms, I420 to RGBA 1.61 ms, the privacy
  blocks 0.59 ms, a snapshot JPEG 11.0 ms, the motion thumbnail 0.52 ms. They replace the two `#[ignore]`d
  `stage_costs` tests of the same day. `kataglyphis_observant` is a library plus `main.rs`, so benches and
  integration tests can reach its modules.

- **The detector is tested with its real model** (2026-10-14). `crates/inference/tests/real_model.rs`
  runs DEIM-D-FINE-N on a 480 px photo of two cats (`tests/fixtures/two_cats.jpg`, 85 kB):
  both found, the front one at 0.8 or more, every box inside the photo, neither taken for a
  dog or a person, and a class filter keeps only its class. The Linux lanes fetch the model
  and set `KATAGLYPHIS_REQUIRE_MODEL=1`, so there it cannot skip. The inference crate was at
  37.6 % line coverage with no test that ran a model.
- **A coverage floor**: the `coverage` step fails below 70 % of the workspace's lines
  (tarpaulin `--fail-under`); it measured 71.3 % before these tests. Raise it as tests land.

- **`/healthz` says when ObservANT's picture is too dark** (2026-10-14). The motion gate
  already let the model idle on a black picture; `dark` now says so, and `ok` stays, since
  a night without IR is no outage.
- **Two measurements, ignored by default** (replaced the same day by the Criterion benches above). On a
  Pi 5: DEIM-D-FINE-N 420 / 258 / 265 ms at 1 / 2 / 4 threads, preprocessing 3 ms, postprocessing nil, a
  snapshot JPEG 10.7 ms, I420 to RGBA 1.7 ms.

- **Configurable classes, cats by default** (2026-10-14). `classes` (COCO names, `["cat"]`
  unless set) replaces the hard-wired cat filter, in the settings and through
  `POST /settings {"classes": [...]}`, checked whole (one to 80 known names, each once) and
  kept across restarts in `settings.json`; the model's runs read them each time.
  `/healthz` lists `classes`, each visit names its `class` and each `arrived` event too, the
  class of the best detection. `coco.rs` holds the 80 names. `all_classes` still counts every
  class.

- **`/healthz` reports what the cat cam achieves and costs** (2026-10-14): `stats`, sampled
  every 2 s by a thread of its own (`stats.rs`), names the frames pushed per second, the
  model's time per run (smoothed) and runs per second, and from `/proc` and `/sys` its CPU
  share of one core, the cores, its resident memory, the 1-minute load, the board's
  temperature, and its uptime. A number it cannot read is `null`, so off Linux only the
  rates show. The web app's Stats panel reads it.

- **A privacy filter for showing the cat cam in public** (2026-10-14). `privacy =
  "pixelate"` in the settings blocks out everything but the detection boxes (each grown by
  15 %, 32-pixel blocks at 640 wide) in the stream, `/snapshot.jpg`, the arrival photos and
  the clips; the model still sees the whole frame. Only the file sets it; `/healthz` names it
  (`privacy`). `frame::pixelate_outside`, with tests for blocks, kept boxes, edges, the
  capture pipeline and the arrival photo.

- **A visit keeps the boxes it arrived with** (2026-10-14): `/healthz`'s `cat.recent[]` and
  `cat.json` carry `boxes` (fractions), so a page can mark a past false arrival "not a cat";
  older visits have none.

- **`scripts/linux/clippy-windows.sh`** (2026-10-14) runs the Windows lanes' lint, clippy
  `--all-targets -D warnings` for `x86_64-pc-windows-gnu`, on Linux in minutes: as root in
  `:latest` it installs mingw for ring's C and adds the target. With the `I420Layout` slip
  below put back, it fails with the CI's message.

- **Peers, a viewer cap and the build in `/healthz`** (2026-10-14).
  - `peers` (origins) in the config: a GET of `/healthz`, `/snapshot.jpg`, `/arrival.jpg`
    or `/arrival.mp4` whose `Origin` is one of them gets `Access-Control-Allow-Origin` and
    `Cross-Origin-Resource-Policy: cross-origin`, so another cat cam's page can show this
    one; `/healthz` lists them for the page's overview.
  - `max_viewers` (8, 0 for no limit) caps the open `/webrtc-ws` sockets, `503` beyond;
    `/healthz` counts them as `viewers`. Each viewer costs an encoder.
  - `version` in `/healthz` and the start log: `CATCAM_VERSION` at build time, which
    `package-catcam.sh` sets to the package version and commit, else the crate's `-dev`.

- **A cat must move to arrive, and spots can be marked "not a cat"** (2026-10-14).
  - The motion pacer stamps each 8x8 block when it changes; a detection starts a visit
    only with two such blocks from the last 10 s in or around its box (`motion::Moving`).
    An open visit takes every detection, so a sleeping cat stays counted. The first look
    counts as motion everywhere: whatever the camera starts on has just come into view.
  - `POST /settings {"ignore": [[x1, y1, x2, y2], ...]}` (fractions, at most 16) drops a
    detection whose IoU with a spot exceeds 0.5 before it counts, is drawn or scores.
    Kept in `settings.json`; a rotation change clears them. `/healthz` lists the boxes
    drawn now as `cat.boxes`, which the web app's "Not a cat" button posts.
  - `last_score` and the pacer's still-cat rate follow the counted detections, so a still
    false cat costs the idle rate, not half the full one.

- **The cat cam's review fixes (2026-10-13).**
  - Each of the last five arrivals keeps its own picture, `/arrival.jpg?t=<arrival time>`
    as for the clips, so a later false visit no longer replaces the picture a phone
    notification shows. An earlier version's single `arrival.jpg` is taken over.
  - A second arrival while one's clip records gets a clip of its own; overlapping clips
    share their frames, at most three record at once, two wait for the encoder.
  - The webhook tries an event again after 5 s, 15 s, 45 s and 2 min, in order, and drops
    one older than ten minutes: a Home Assistant restart no longer loses a visit.
  - `/healthz` says `ok: false` with a `problem` when no frame came for 20 s or a test
    pattern stands in for a missing camera, and reports `frame_age_s` and `model_age_s`.
  - `/webrtc-ws` and `POST /settings` take requests only from the cat cam's own page: a
    browser's `Origin` must match its `Host`, and the host must be an IP, a bare or local
    name, or listed in the new `allowed_hosts` (another site's page and DNS rebinding get 403).
  - `/snapshot.jpg` encodes once per frame, one at a time, however often it is asked for.
- **Each cat arrival is a video too (2026-10-12).** `cat_webrtc` keeps the last 3 s of
  annotated frames at 10 fps (`clip.rs`, buffers held by reference, no copy), and on an
  arrival adds the next 7 s and encodes them on a thread of its own with `openh264enc`,
  `h264parse` and `mp4mux` (faststart, so playback starts before the end loads).
  `/arrival.mp4` serves it with byte ranges, which Safari's `<video>` needs; `/healthz`
  names the newest one's arrival as `cat.clip`; and `notify_url` gets a `clip` event once
  it is written, with the arrival's time and score, so a notification can add it. The last
  five clips stay in the state directory as `clip-<time>.mp4`, and `?t=<time>` serves that
  arrival's own or `404`, so a false visit right after the cat cannot put its clip behind
  the cat's notification; arrival events carry the visit log's own time for that. `clip_seconds` (default 10, 0 for none) sets the
  length. A GStreamer without one of those elements records no clip and says which.
- **The page changes the cat cam's settings at run time (2026-10-13).** `GET /settings`
  answers the `name`, `rotate` and `score`; `POST /settings` takes any of them as JSON (JSON
  only, so another site's form cannot send it) and checks the whole post before it applies
  any of it. A turn (0, 90, 180 or 270 clockwise) reaches `videoflip`, always in the capture
  pipeline now, on the next frame, with no restart; a score (0.05 to 0.95) the next model
  run; a name the next webhook event. `/healthz` reports `rotate`, `score` and the `frame`
  size and rate. What the page set is kept in the state directory's `settings.json` and wins
  over the config file, key by key. The frame caps carry square pixels, so a quarter turn is
  bordered into the frame instead of stretched: before, 90 and 270 squeezed the turned 3:4
  picture into 4:3. `rpicam-vid --rotation 180` is gone; 180 goes through `videoflip` like
  the other angles. OmniAccelerANT's Camera page is the front end.
- **`--precompress DIR` ships a web build smaller (2026-10-12).** It writes a zopfli
  `<file>.gz` beside every file the server would gzip, on every core, and the server sends
  that file instead of compressing its own, unless the original is newer. OmniAccelerANT's
  packager runs it: on a Pi 5, a phone's first visit to the cat cam page went from about
  4.62 MB to 4.46 MB (`skwasm_heavy.wasm` 2291 to 2214 kB, `main.dart.wasm` 1479 to
  1418 kB; plain HTTP at a LAN address is no secure context, so Flutter loads the heavy
  renderer), 3.93 to 3.75 MB over loopback, and nothing is compressed or held in memory
  for it at run time. `zopfli` 0.8.3; its dependencies
  were in the lockfile already.
- **The kept clips are listed, and a departure names its arrival (2026-10-12).**
  `/healthz` gains `cat.clips`: the arrival of every clip `?t=` still serves, newest
  first, for a page that lists them all. A `left` event carries the arrival it ends
  (`arrived`) and the score that arrival was confirmed at (`best_score`), so a receiver
  can update the notification it sent for that arrival and drop a false visit's
  departure under the same threshold. A `clip` event of a visit already over carries its
  `stayed_s`, as a visit shorter than the clip's post-roll ends before the clip is written.

### Changed

- **The version lives in `Cargo.toml` alone: 2.3.4** (2026-10-15, owner). `[workspace.package]`
  said 0.1.0 while `VERSION.txt` said 2.3.4, and CI, the packages and the MSIX read the text file.
  `VERSION.txt` is gone; `compute-version.sh` and both Windows lanes' `version-file` hand the hub's
  `version_util.sh` the `Cargo.toml`, and a local Windows build's MSIX version comes from it through
  `Get-CargoMsixVersion` (`WindowsCargoTarget.Common`) instead of `Build-Windows.config.psd1`'s
  `0.1.0.0`. Every workspace crate is 2.3.4, so ObservANT's `-dev` build reports `2.3.4-dev`.

- **CI: a push to `develop` runs Linux x64 and the gates; the other build lanes run on request**
  (2026-10-15, owner rule for the family, ANTfrastructure's `docs/branch-and-release-strategy.md`).
  Linux arm64, Linux riscv64, Windows x64 and Windows arm64 lost their push and pull-request
  triggers. A commit message asks with `[ci <lane>]` (`[ci windows]`, `[ci arm64]`,
  `[ci riscv64]`, `[ci all]`), which the new `on-request.yml` reads through the hub's
  `requested-lanes` action, and each lane has *Run workflow*. Each lane's concurrency group is
  its literal name now, since `github.workflow` is the caller's inside a called workflow and
  would put every requested lane in one group. riscv64 keeps its weekly GPU run.
- **The default model is DEIM-D-FINE-N (Apache-2.0), fetched rather than tracked**
  (2026-10-14, owner). The AGPL-3.0 `resources/models/yolov10m.onnx` (59 MiB) is no longer
  tracked; `MODEL_FILE` is `deim_dfine_n_coco.onnx` (15 MiB, COCO 80, 43.0 AP), built from
  DEIM's pinned checkpoint by [ModelANT](https://github.com/Kataglyphis/ModelANT) in the
  same `[1, 300, 6]` layout, so the parser and the default plain-resize preprocessing stay.
  `resources/models/models.lock` names it, its license and notice with URL and SHA-256;
  `scripts/linux/fetch-models.sh` and `scripts/windows/Get-Models.ps1` (the Windows build's
  *Fetch Models* step) fetch and check them, through a cache. Against YOLO26n on 12 arrival
  pictures of three cat cams it found cats more often and more surely, at about 260 ms a
  run instead of 190 ms on a Pi 5. The burn demo `onnx-yolov10` is `onnx-detector` (the old
  name stays an alias). Test fixtures use generic private addresses, not the cat cams' real ones.
- **The cat cam is ObservANT** (2026-10-14, the owner's name for it). Cats stay the default,
  detecting is general. The crate is `crates/observant` (`kataglyphis_observant`), the Pi
  script `scripts/linux/observant/`. `/healthz`'s `cat` object is `detection`, the visits file
  `visits.json`, the settings `observant.toml`, the variable `KATAGLYPHIS_OBSERVANT_CONFIG`, the
  build stamp `OBSERVANT_VERSION` and the default name `ObservANT`. An install from before
  carries over: `catcam.toml`, `cat.json`, `KATAGLYPHIS_CATCAM_CONFIG`, `CATCAM_VERSION` and
  Windows' `omni-accelerant-catcam` folder are still read while the new ones are absent.
- **A cat back within a minute continues its visit** (2026-10-14). At the feeding trough
  one cat walking about had made 21 visits in an hour, 10 to 90 s apart, each with its own
  arrival event, clip and count. Being in view still ends after about 3 s without a hit (the
  boxes, the motion gate and the still rate follow it); the visit ends only a minute after the
  last sighting, so the departure event comes a minute later and `stayed_s` spans the whole
  stay.
- **A Pi camera turned a quarter captures the picture as small as it ends up** (2026-10-14).
  At 90 and 270 degrees `rpicam-vid` and libcamera deliver 480x360 (for 640x480) through
  the ISP's free scaler, so `videoscale` only adds the side borders. Measured on a Pi 5,
  30 fps: resampling the turned 480x640 cost 20 % of a core, borders alone 3 %. A turn
  between landscape and portrait restarts such a capture once (`Ended::Turned`); other
  sources keep their size and still turn live. On tabblscatcam (rotate 270) the service
  went from 21 % to 4 % of a core.
  - `rpicam-vid --codec yuv420` pads each row to 128 bytes: 480 wide writes 512-byte rows
    (276480 bytes a frame, measured). `rawvideoparse` now gets those strides and offsets;
    read as tight rows the picture was green stripes. Only a width that pads alike at 64
    and 128 bytes is captured smaller, so the stride holds on either ISP.
- **Renovate skips the vendored `third_party/egui-winit-0.36.2` (2026-10-14).** A packageRule
  in `.github/renovate.json` (`matchFileNames`, `enabled: false`) keeps its upstream
  `Cargo.toml` and `Cargo.lock` out of the report and out of lock maintenance. Its rows had
  made the 2026-10-07 apply refuse its whole run, and cargo exits 101 on that lock. The
  egui family stays watched through the root `Cargo.toml`. The hub's planner
  (`renovate_planner.py lockmaint`) now lists only the root and crate manifests.
- **The six Linux feature checks run in one job (2026-10-13).** `linux-x64.yml`'s
  `feature-matrix` pulled the 12.8 GB image six times, once per feature set, and each job
  spent 6-9 min on that prologue for 1-3 min of `cargo check`: 56-67 runner-minutes a run
  for about 13 min of work (runs 38394017265, 38400294322, 38407158265). Now one job runs
  `bash scripts/linux/ci-container-steps.sh feature-check`, which checks every set in
  `FEATURE_CHECK_SETS` in one container and one warm target directory. Each set has its own
  log group and, on failure, its own `::error` annotation, and every set runs even after
  one fails. All six pass that way in `:latest` on the dev box. Dispatching `linux-x64.yml` with
  `feature-checks-per-job` brings back one job per set.

### Fixed

- **`inference = "auto"` runs the model on 1 GB boards** (2026-10-15). It skipped the model
  below 1024 MiB of RAM to spare a 512 MB Pi Zero 2 W, but the GPU keeps part of every board's
  RAM: a 1 GB Pi 3 Model B reports 905 MiB, so it streamed without detection. The threshold is
  768 MiB (`auto_inference_fits`), between the two, and a test pins both boards' figures.

- **An infinite box coordinate no longer overflows `Moving::near`** (2026-10-15). `index(v) + 1`
  overflowed for `inf` (`usize::MAX`), a panic in a debug build; it saturates now, and a box far
  past the frame is still cut to it. Found while checking the last mutant of the arrival logic.

- **`KATAGLYPHIS_SCORE_THRESHOLD` and `KATAGLYPHIS_INFER_EVERY_MS` take surrounding spaces**
  (2026-10-15), as `KATAGLYPHIS_ORT_THREADS` already did: `" 0.4"` fell back to 0.5 without a
  word. `crates/core`'s env readers parse through pure functions now (`parse_preprocess`,
  `parse_or`, `parse_log_level`), so the rules have tests; the crate had none of its own.

- **A missing model file says so** (2026-10-15). `PersonDetector::new` answered a wrong path
  with "No ONNX backend available. Build with --features onnx_tract…", which sends one
  looking for a missing runtime. It now checks the file first ("no ONNX model at <path>"),
  and when ORT fails on a file that exists, its own reason is kept ("ORT could not load
  <path>: …") instead of the generic line. Found by the new webcam engine test;
  `crates/inference/tests/model_errors.rs` pins both messages.

- **`list_cameras` no longer runs on the caller's thread** (2026-10-15). It was
  `#[frb(sync)]`, and the device monitor it starts brings libcamera up: OmniAccelerANT's
  Stream page called it as its feature probe, which held the app's first frame 0.94 s on a
  Pi 5. It is async now, and `webcam_inference_available()` answers the probe from
  `cfg!(all(feature = "gstreamer", onnx))`, which also stops a gstreamer-only build from
  offering a pipeline whose start would fail.
- **ObservANT, from the deep review of 2026-10-14:**
  - *Reads need a local host name too.* `own_host` now runs on every request, so a page
    that DNS rebinding pointed at the board cannot read `/snapshot.jpg`, the arrival
    photos or the clips as its own; `own_page` adds the `Origin` check for the acting ones.
  - *No connection flood.* At most `MAX_CONNECTIONS` (128) are served at once, a newer one
    is closed unanswered and logged once a minute, and hyper gets a timer, so a request
    head must arrive within 10 s (without one hyper 1.x skips its header timeout).
  - *A failing model shows.* Three failed runs in a row make `/healthz` say `ok: false`
    with `the model fails: …`; every failed run clears the drawn boxes, which stayed sharp
    under the privacy filter before; the log keeps the first failure and one line a minute.
  - *The privacy filter locks the classes.* With `privacy = "pixelate"`, `POST /settings`
    refuses other `classes` and classes a page saved earlier give way at start, since a box
    of `person` stays sharp; `GET /settings` names `privacy`.

- **A browser that vanished frees its viewer place** (2026-10-14). The web server's
  connections carry TCP keepalive (60 s idle, then a probe every 10 s), so one whose peer
  slept or left the Wi-Fi without closing ends in about 2.5 minutes, with its `/webrtc-ws`
  socket and its `max_viewers` place. Before, nothing ended it until the server sent
  something, and enough of them would have kept new tabs out.

- **Windows arm64 built again after the quarter-turn capture** (2026-10-14). `I420Layout`
  serves only the `cfg(unix)` rpicam source, so Windows' clippy `-D warnings` called it dead
  code (run 38496162066); it is `cfg(unix)` now, and the next Windows round passed.
- **A GStreamer without the clip encoder no longer leaves a scratch directory per arrival
  (2026-10-13).** `encode_mp4` made its directory before it built `openh264enc`,
  `h264parse` and `mp4mux`, and returned early when one was missing, past the cleanup at
  its end. Windows arm64's GStreamer has no `openh264enc`, so the test that a failed
  encode leaves nothing behind failed there from a9e3b58 on, while x64 stayed green. The
  directory is removed on drop now, after the pipeline; a test hides `openh264enc` from
  the registry for one encode, and fails against the old code (939990b).
- **The ORT loader accepts the chain ORT of the 2026-10-08 `:winamd64` (2026-10-10).** That image's
  `onnxruntime.dll` names its sources `C:/temp/onnx-src/onnxruntime/core/...`, with forward and mixed
  separators, and no longer the backslash form `verify_chain_build` looked for. So every Windows ORT
  session fell back and failed, and OmniAccelerANT's Windows cat cam test lost its cat. `embeds_marker` now
  treats `/` and `\` alike, as it already ignored case. In `:winamd64`, the cat cam found 2 cats (best score
  0.72) with the image's ORT, where it failed before the change.
- **The cat cam runs ORT on two threads that sleep between runs (2026-10-11).** ORT's
  default, four threads spin-waiting for the next run, kept himbeere2's live service at a
  133 % average over 5.8 days at 2 Hz. On ANThology's photo the process took 183 % that
  way and 81 % now, with the same detections. Installed there, the live service went from
  131 % to 70 % of a core (60 s each, same camera, same scene). `inference_threads` sets
  the count. The inference crate keeps ORT's default for every other caller, and
  `KATAGLYPHIS_ORT_SPIN` / `KATAGLYPHIS_ORT_THREADS` set both for any of them.
- **A cat counts after two hits in a row (2026-10-11).** A lone detection near the score
  threshold logged an arrival and a departure a second apart, and drew a box for half a
  second. 52 of the 72 arrivals in himbeere2's journal were that. A cat now leaves after
  3 s without a hit (longer at slow rates). The journal keeps its words.
- **The cat cam's frames stay I420 from camera to encoder (2026-10-12).** The capture
  asked for RGBA, so every frame was converted from the camera's I420 and back for the
  encoder. Boxes are drawn into the YUV planes now, and only the model's frames and
  snapshots are converted. himbeere2's idle service went from 13 % to 9 % of a core. On
  ANThology's photo the best score moved from 0.72 to 0.76: the photo now passes through
  chroma subsampling, as a camera's frames always did.
- **A Pi camera mounted upside down is flipped by the sensor (2026-10-11).** `rotate = 180`
  becomes `rpicam-vid --rotation 180` instead of a `videoflip` over every RGBA frame, which
  halved the capture thread on himbeere2 (11 % to 5 %). Two 4 s stills of its imx219, with
  and without the flag, show the same scene turned 180°, as the `videoflip` turned it.

### Added
- **The cat cam keeps a log of the cat's visits (2026-10-12).** `/healthz`'s `cat.recent`
  lists the latest 20, newest first: `arrived` in Unix seconds and, once the cat has left,
  `stayed_s`. The log lives in `cat.json` beside the count, and a `cat.json` from before it
  still loads. The service also writes that file once more when it stops, so the next start
  closes a visit the stop cut short at the cat's last sighting; after a power cut, at the
  last arrival or departure written.
- **The cat cam remembers its cat across restarts (2026-10-12).** Visits, the last
  sighting and score, and the arrival frame are written to systemd's `$STATE_DIRECTORY` on
  every arrival and departure and read back at start, so an upgrade or a reboot no longer
  turns the Stream page back to "No cat seen yet".
- **The cat cam tells a webhook when a cat comes or goes (2026-10-12).** `notify_url`
  gets `{"event": "arrived"|"left", ...}` as JSON over http or https, from a thread of its
  own; `/arrival.jpg` is the frame the arrival was confirmed on, boxes drawn. Logs never
  carry the URL's path, which is a Home Assistant webhook's secret.
- **The cat cam serves its web build gzipped (2026-10-12).** The Flutter boot's main files
  went from 20.2 MB to 7.5 MB for a browser that accepts gzip. Each file is compressed on
  its first request (614 ms for the 7 MB CanvasKit on a Pi 5) and kept in memory while it
  is unchanged. A revalidation, the common request of a return visit, no longer reads the
  file at all.
- **The cat cam idles its model while nothing moves (2026-10-11).** `motion.rs` compares
  8x8-block mean luma between looks; with no motion and no cat in sight the model runs at
  `idle_inference_fps` (default 0.2) instead of `inference_fps`. A still scene without a
  cat dropped from 74 % to 12 % of a Pi 5 core at 2 Hz. A cat in view, a fresh hit or
  motion keeps the full rate. `/healthz` gains `model_runs`. A black frame (2026-10-12)
  skips the idle run as well, which took the dark room at night from 9 % to 2 %.
  A cat in view with nothing moving is looked at every `still_period`, twice the full
  rate's period (2026-10-13); motion restores the full rate, and the visit lingers past
  the still period. himbeere2's drying rack, a false cat at 0.34, went from 69 % to 36 %.
- **`/snapshot.jpg` and the cat in `/healthz` (2026-10-11).** The cat cam's web server
  encodes the latest frame, boxes included, when asked. `/healthz` gains `cat`: whether
  one is in view, when one was last seen, the visits counted and the last best score.
- **`crates/cat_webrtc` is the cat cam service (2026-10-05).** One process now does what
  a container, nginx and a board-local script did:
  - It picks the camera: a Pi camera through `rpicam-vid`, else a libcamera CSI camera,
    else a UVC webcam. With none it streams a test pattern and re-probes every 30 s.
  - It serves OmniAccelerANT's web build with `/webrtc-ws` proxied to its signalling
    server, which moved to `127.0.0.1` by default.
  - It reads a TOML config under the flags, pins the ICE ports, turns STUN off by
    default and stops cleanly on SIGTERM.
  - Measured in `:latest` (x64), with nothing else running:
    - Headless Chrome played the stream from the built-in server over plain HTTP from a
      non-loopback address: 640x480, 875 frames in 29 s.
    - Every ICE candidate sat in 40000-40099.
    - Inference found both cats in ANThology's photo (best score 0.59).
    - A missing camera and a broken source recovered as designed.
  - **On Windows the exe runs from its install folder (2026-10-06).**
    - `install.rs` points GStreamer, ONNX Runtime and the model at the folder beside the
      exe, and the settings file moves to `%ProgramData%\omni-accelerant\catcam.toml`
      there.
    - In a `:winamd64` rebuilt with the hub's WebRTC fixes, OmniAccelerANT's MSI
      installed it, and it ran with only System32 on PATH, loading modules only from that
      folder and Windows.
    - A `webrtcsrc` viewer decoded 60 frames, and the bundled `yolo26n` on the bundled
      chain ORT found the cat.
  - **First board run: himbeere2 (Raspberry Pi 5, imx219, Debian 13), 2026-10-05.** It
    ran as OmniAccelerANT's packaged service and picked the camera via `rpicam-vid`.
    Headless Chrome on another host played it at 29 fps. Three fixes came out of it:
    - `rpicam-vid` is now owned by a guard. A failed pipeline setup no longer leaves one
      zombie per retry.
    - Probes skip the device providers when rpicam has the camera, and probe nothing for
      `test` and `image:`. The host's libcamera, with its TensorFlow Lite and abseil, no
      longer loads into the service.
    - `inference_fps` (default 2, `--inference-fps`) caps the model's rate. Without a
      cap, YOLO held the Pi 5 at 380 % CPU; capped, it uses 132 % of one core.
  - **`run-producer-pi.sh` streamed from that board's CSI camera on 2026-10-06, the first
    time outside QEMU.**
    - `--build` built the producer, and the image's entrypoint kept `/hostlibs` ahead of
      its own libcamera (CON23's order), so `libcamerasrc` ran the host's `v0.7.2+rpt`
      with libpisp 1.7.
    - `yolov10m` ran on ORT.
    - Through `serve.sh --http`, Chrome on another host played 701 frames in 23 s.
    - A trap for anyone sharing the `kataglyphis-cat-target` volume: a build from the
      superproject layout leaves `env!("CARGO_MANIFEST_DIR")` pointing at
      `/workspace/third_party/OxidANT`. Cargo sees the same relative paths under the
      runner's mount and does not rebuild, so `--build` keeps that model path. `cargo
      clean -p` on the workspace crates cures it.
- **Every test runs on every Linux and Windows lane (2026-10-01).** Before this, the Linux
  lanes ran 358 tests and both Windows lanes 167.
  - All lanes test with `--features 'kataglyphis_media?/gstreamer,kataglyphis_inference?/onnxruntime'`
    (`TEST_FEATURES` in `scripts/linux/ci-container-steps.sh`). `crates/media`'s
    `capture_test` is `cfg(feature = "gstreamer")` and had compiled empty everywhere. The
    `?/` form selects the member's own feature: the plain one also switched on the optional
    `kataglyphis_media` dependency of `kataglyphis_gui` and the root package
    (`cargo tree -e features`). Linux clippy takes the same features.
  - Windows x64: `Invoke-DebugTests.ps1` runs the workspace minus the renderer, adding
    `kataglyphis_inference` (20), `kataglyphis_telemetry` (4) and the capture tests (2). It
    builds the renderer's lib and all 18 integration test binaries for the host, listed in
    `target\host-tests\tests.json` and checked against `tests\*.rs`. `Invoke-HostTests.ps1`
    runs them through the hub's `Invoke-StagedTests.ps1`, which the arm64 lane uses too.
  - Windows arm64: `Stage-CrossTests.ps1` builds the whole workspace's tests, renderer
    included, with the same features. It stages the product's GStreamer plugins beside the
    capture test, and of the programs only those an integration test starts.
  - Both Windows lanes set `KATAGLYPHIS_REQUIRE_GPU=1`, so a renderer test with no
    adapter fails instead of skipping itself. `GpuContext` carries the `adapter_info`, and
    `headless_or_skip()` writes it once per test binary straight to stderr, past libtest's
    capture, as an `ADAPTER:` line. lavapipe (x64 and arm64) and the x64 runner's WARP
    (`Microsoft Basic Render Driver`, Dx12) all report `bc=true timestamps=true`, so under
    the flag `ktx2::uploads_bc1_when_supported` and
    `gpu_timing::every_pass_reports_a_finite_non_negative_duration` fail rather than skip
    without those features.
  - Both Windows lanes' clippy covers `--workspace --all-targets --locked`, as on Linux; it
    covered the root package, `kataglyphis_cli` and `kataglyphis_gui` only.
  - Coverage passes `--workspace`, the test features and `KATAGLYPHIS_REQUIRE_GPU=1` to the
    hub's `cargo_coverage.sh`. Without `--workspace`, tarpaulin measured the root package
    alone, which has no unit tests: `0.00% coverage, 0/754 lines covered`.
  - Measured at 338c4da, which still carried a separate adapter-report test (360 without
    it): Linux x64 and arm64 361 passed, 1 ignored (runs 36889465993, 36889466121), coverage
    65.23% (4994/7656 lines); Windows x64 361 passed, 1 ignored, 30 in the container and 331
    on the host (run 36889466941). The Windows arm64 run (36889467167) stopped in
    `headless.exe`: the process died after `running 39 tests` with no test finished and no
    panic text, the first rendering any arm64 test had done on WARP (the tests that ran
    before it create devices but draw nothing). Its renderer binaries now run one test at
    a time (`--test-threads=1`), which passes; the parallel crash is not root-caused
    (BACKLOG.md).
  - Measured at 776685b, every lane green and the same set everywhere: Linux x64 and arm64
    360 passed, 1 ignored (runs 36907253295, 36907253309), coverage 65.26% (5000/7662);
    Windows x64 360 passed, 1 ignored, 30 in the container and 330 on the host (run
    36907253841); Windows arm64 `TESTS: passed=360 failed=0 skipped=1` on `windows-11-arm`
    (run 36907253720). WARP reports `bc=true timestamps=true` on both Windows runners.
- **The arm64 cross lane runs the x64 lane's tests on `windows-11-arm` (hub CON43, 2026-10-01).**
  - `Invoke-WindowsLane.ps1 -StageTests` calls the new `Stage-CrossTests.ps1`. It builds
    the four targets of `Invoke-DebugTests.ps1` with `cargo test --no-run` for the target
    triple, from `C:\ws` like the x64 tests.
  - It stages them under their repo-relative paths, together with `kataglyphis_cli.exe`
    (for `CARGO_BIN_EXE`), the per-directory DLL closure, the renderer's source tree and
    fixtures, the hub's `Invoke-StagedTests.ps1` and a `tests.json`.
  - On the runner, the test-command links `C:\ws` to that mirror, so the paths compiled
    into the binaries resolve.
  - The renderer runs with `--nocapture`, and its `SKIP: no GPU adapter` lines count as
    skips, not passes.
  - Measured on x64 in `:winamd64`: 4 test binaries and the CLI staged, and
    `TESTS: passed=4 failed=0 skipped=0`, the renderer left out because Server Core has no
    `opengl32.dll`.
- **The Linux lanes render the WebGPU golden tests, on lavapipe (2026-09-29).** The
  `test` step of `scripts/linux/ci-container-steps.sh` exports
  `KATAGLYPHIS_REQUIRE_GPU=1`, so a missing adapter panics instead of skipping. The
  family image has a software Vulkan device since ANTfrastructure CON19; proven
  locally on the amd64 child (`vulkaninfo`: deviceType CPU, `llvmpipe`), the step
  green with no `SKIP: no GPU` line. The one test that fails on it,
  `a_non_uniform_instance_scale_shades_like_the_same_node_scale` (the known
  instanced-normal bug, 987 px, blocked upstream), is `#[ignore]`d with its reason.
- **Every Windows package carries the GUI's GStreamer plugins and its model, and
  both lanes prove it (2026-09-25).**
  - The camera pipeline creates its elements by name, so their plugins were in no
    import table: the bundle, MSIX and MSI shipped GStreamer's DLLs but not one
    plugin, and the GUI could not start on a machine without the image's
    `C:\runtime`. *Stage GStreamer Plugins* now copies `Build.GStreamerPlugins`
    (`Build-Windows.config.psd1`) into `lib\gstreamer-1.0` beside an exe that links
    GStreamer, and a plugin the image lacks fails the build. The plugins seed the
    DLL closure, so what they import lands beside the exe.
  - The GUI initialises GStreamer through `kataglyphis_media::ensure_gst_initialized`,
    as the webcam engine does, which points `GST_PLUGIN_PATH` at the plugins beside
    the exe; `kataglyphis_media::bundled_plugin_dir` names that directory for both.
  - `kataglyphis_cli media-check` (with `gui_windows`) builds the pipeline without
    starting it and prints the plugin behind each element. It fails on a missing
    element, and when an exe that carries plugins takes one from anywhere else.
    The x64 host step and the arm64 run job both run it on the packaged exe.
  - `resolve_model_path` takes `resources\models\yolov10m.onnx` beside the exe
    before the compile-time checkout path, so a packaged app finds its model with
    no `KATAGLYPHIS_ONNX_MODEL`. The MSI now installs `resources\` too.
  - The MSI installs the bundle's whole tree: its generated group
    (`msi-payload-files.wxs`, was `msi-payload-dlls.wxs`) holds one component per
    file, placed with WiX v4's `Subdirectory`. `New-OrtProvenPayload -IncludeDirectory`
    copies `lib\` into each payload before G6 proves it.
- **The Windows exe loads the chain ONNX Runtime, and every package carries its
  DLL closure on both arches (owner decisions 2026-09-25).**
  - The CLI's `onnxruntime_directml`/`onnxruntime_cuda` (and `onnx_tract`) features
    now reach the GUI's inference (`kataglyphis_gui?/...`) and the root crate's
    `onnxruntime`. Until now nothing in the exe called ORT, so Build-Windows staged
    none and no package shipped one (the x64 log said so on every run).
  - `kataglyphis_cli onnx-runtime` loads the chain ORT through
    `ort_runtime::ensure_ort_loaded` and prints its path. The arm64 run job and the
    x64 host step both run it.
  - The GUI's person-detection overlay had never been compiled: `clippy -D warnings`
    found five unused imports and an `if let Some` over a field that is no `Option`
    (`poll_inference`). Both are fixed, and `kataglyphis_telemetry`'s two inference
    counters are `pub`, because the GUI's inference thread calls them. Checked for
    `onnxruntime_directml`, `onnx_tract` and `onnxruntime_cuda`.
  - `Build-Windows.ps1` stages the DLL closure on x64 too, with the hub's
    `Get-ProductDllSearchPath` (the chain ORT, then `C:\runtime\bin`, then the
    target's VC++ runtime). Both arches write `dist\windows-<x64|arm64>`: the
    portable bundle (exe, `resources\`, closure), the MSIX, and the MSI, whose
    generated group now holds every payload DLL.
  - The package features live in `Build-Windows.config.psd1`
    (`gui_windows,onnxruntime_directml`), not in two workflows' env.
- **The Windows x64 lane is a thin caller of the hub's `container-ci-windows.yml`**
  (the family's next sharing step). `scripts/windows/Invoke-WindowsLane.ps1` runs
  the debug tests, the config matrix and `Build-Windows.ps1` in the container. The
  lane's `host-command` runs the renderer tests and the packaged exe on the host.
  The version comes from `version-file: VERSION.txt`, and `dist/windows-x64`
  uploads as one artifact, as the arm64 lane's does. `New-ReleaseArchive.ps1` is
  gone: the bundle is what the zip held, now with its closure and `resources\`.
- **A Windows arm64 lane that cross-builds and then runs the product (owner
  decision 2026-09-25).** `windows-arm64-cross.yml` ("Windows arm64 · cross
  build + run") is a thin caller of ANTfrastructure's reusable
  `container-ci-windows.yml`: `Build-Windows.ps1 -TargetArch arm64` in the
  family image's arm64 bundle, the hub's arch gate over `dist/windows-arm64`
  (every binary arm64, every import resolvable on a clean device), and
  `kataglyphis_cli.exe --help` + `stats` on GitHub's `windows-11-arm`.
  `Build-Windows.ps1` takes `-TargetArch amd64|arm64` (default: the image's
  `WINDOWS_TARGET_ARCH`). A cross build builds with `--target`, runs clippy for
  aarch64, leaves audit/deny, fmt and the tests to the x64 lane, stages the DLL
  closure beside the exe (hub `Copy-PeImportClosure`), and writes the portable
  bundle, `*_arm64.msix` and `*-arm64.msi`. `WindowsCargoTarget.Common` holds
  every arch-dependent path and name, pinned by `CargoTarget.Tests.ps1`; the
  x64 paths are unchanged. The AppxManifest takes its `ProcessorArchitecture`
  from `__MSIX_ARCH__`, and the MSI's generated DLL group is `PayloadDlls`
  (was `OrtRuntime`): the chain ORT on x64, every payload DLL on arm64. Its
  fragment moved out of `dist\msi` into the target directory, and the MSI's
  `InstallerVersion` is 500, which an ARM64 package requires (WIX1143).
  Verified in the arm64 bundle: 10/10 steps, 12 arm64 binaries in the bundle
  (the exe, the VC++ runtime, GStreamer/GLib), the arch gate clean with no
  unresolved import. The hub pin moves from 2fdddbc7 to 6b576c27, which
  carries the reusable lane and `Copy-PeImportClosure`.
- **Three gates this repo did not have.** `--ratchets` on the lint lane (the
  docs cross-reference gate plus eight measurement gates, frozen at
  `<repo>/<gate>.allow`); a PowerShell lint job running ANTfrastructure's
  `Invoke-Lint.ps1 -Path scripts -FailOnAnalyzer`; and
  `scripts/windows/tests/Repo.GeneratedArtifacts.Tests.ps1`, which asserts that
  nothing generated is tracked. Seeded freeze files: `comment-size.allow`,
  `code-complexity.allow`, `dead-functions.allow`.
- **The renderer's three design documents, in `crates/webgpu_renderer/docs/`**:
  `renderer-bounds-invariant.md`, `webgpu-renderer-roadmap.md` and
  `webgpu-gltf-rust-plan.md`. They moved out of `BeschleunigerBallett/docs/`
  under decision D6 — this repo owns the WebGPU renderer, code *and*
  documentation — and BeschleunigerBallett keeps a pointer file at each old
  path rather than a copy. The four pages that describe *both* renderers
  (`gpu-golden-testing.md`, `model-loading.md`, `shader-sharing.md`,
  `webgpu-srgb-audit.md`) stayed there and are still referenced absolutely.
  The four references to the moved pages — `crates/webgpu_renderer/README.md`,
  `src/lib.rs` and twice in `src/render/bounds.rs` — point at the moved pages
  again. Three of them are written `../docs/<name>.md` / `../../docs/<name>.md`,
  relative to the file rather than to the crate: a bare `docs/<name>.md` in code
  is resolved from the REPO ROOT by the docs cross-reference gate, which is what
  it means to a reader too.
- **`scripts/linux/cat-stream/run-producer-pi.sh`**, the Raspberry Pi 5 runner
  for `crates/cat_webrtc`, moved here from OmniAccelerANT under decision D12:
  it belongs with the crate it builds and starts. OmniAccelerANT keeps a
  pointer and still owns the web half, `serve.sh`.

### Changed
- **Lock file maintenance moves 61 packages (2026-10-13, the Renovate report).**
  - `cargo update` takes every in-range release, among them cxx 1.0.205, proptest 1.12.0
    (with rand_xorshift 0.5.0), serde_json 1.0.154, turso 0.8.3, uuid 1.28.0, zopfli 0.8.4
    and syn 3.0.9. `ort` stays `=2.0.0-rc.13`: rc.14 targets ONNX Runtime 1.31.0 with a new
    `Environment` API, and the images still carry 1.30.0. `zune-core` stays 0.5.3.
  - `third_party/egui-winit-0.36.2/Cargo.lock` is again not refreshed. cargo exits 101 there
    (`current package believes it's in a workspace when it's not`), because the crate sits
    inside this workspace without being a member; a copy outside it updates. No build reads
    that lock, and `PATCHED.md` vouches for every file but `src/lib.rs` as published.
  - Verified in `:latest` (x64): `ci-container-steps.sh` `ort-chain-only`, `security`,
    `fmt-clippy`, `test` (457 passed, the one known ignore, lavapipe with
    `KATAGLYPHIS_REQUIRE_GPU=1`), `release` and `docs`; `cargo check` for each of the six
    `feature-matrix` rows; the renderer's wasm32 check; `run-lint-gates.sh --ratchets`.
- **tokio 1.53.1 → 1.53.2, hyper 1.11.1 → 1.12.0, upload-artifact v7.0.1 → v7.0.2
  (2026-10-07, the evening Renovate report).**
  - Both crates are lockfile-only moves (`cargo update -p tokio@1`, then `-p hyper@1`,
    the order `renovate-local.sh --apply` runs them). hyper 1.12 drops `futures-channel`,
    and the lock loses windows-sys 0.60 and the windows-targets 0.53 family with it.
  - The report's five rows for `third_party/egui-winit-0.36.2` (log, serde, webbrowser,
    smithay-clipboard, objc2) were not taken. That `Cargo.lock` is the published crate's
    and is never read: cargo refuses to refresh it because the crate sits inside this
    workspace, which is also why the apply half refused the whole run.
  - Verified in `:latest` (x64): `ci-container-steps.sh fmt-clippy`, `test` (395 passed,
    the one known ignore, lavapipe with `KATAGLYPHIS_REQUIRE_GPU=1`), `security` and
    `ort-chain-only` pass, so do the renderer's wasm32 check and `run-lint-gates.sh`.
- **burn 0.21.0 → 0.22.0 and glam 0.33 → 0.34 (2026-10-07, from the Renovate report).**
  - burn 0.22 drops the backend type parameter: models and tensors are plain types
    (`Tensor<2>`, `nn::Linear`), and the backend is a runtime `Device`. The burn demos
    run on `Device::flex()`, with `.autodiff()` for training. Flex replaces NdArray,
    which 0.22 deprecates.
  - `burn-ndarray` and `burn-autodiff` are gone as direct dependencies. The root package
    enables burn's `flex` and `optim` features instead, and `optim` implies `autodiff`.
  - Two smaller API changes: the optimizer is a `ModuleOptimizer`, and `save-load` writes
    a burnpack file through `Module::save_file`/`try_load_file`, because 0.22 removed the
    `Recorder` API.
  - `tensor-demo` prints `TensorData` through `Display`, because its `Debug` shows raw
    bytes in 0.22.
  - The bincode advisory (RUSTSEC-2025-0141) left both ignore lists: burn-core no longer
    pulls bincode, and cargo-deny reported it `advisory-not-detected`.
  - glam 0.34 removed the deprecated `Mat4` projection and `look_at_*` methods. The
    renderer already used `glam::camera`, so it needed no code change.
  - Verified in `:latest` (x64):
    - `ci-container-steps.sh fmt-clippy`, `test`, `security`, `release` and
      `ort-chain-only` pass, and so does `run-lint-gates.sh`. The test step ran
      395 tests on lavapipe with `KATAGLYPHIS_REQUIRE_GPU=1`, with 0 failed and the one
      known ignore.
    - `cargo clippy --all-targets --features burn_demos -D warnings` is clean.
    - All six `feature-matrix` rows and the renderer's wasm32 check build.
    - Every burn demo ran, including `onnx-yolov10` with adapter training on the chain ORT.
- **arm64's renderer tests run llvmpipe at 128-bit vectors (2026-10-06).** The image's
  256-bit setting (hub CON44, for Mesa's BVH sort) made LLVM's AArch64 JIT kill a renderer
  test binary in four of about twenty arm64 runs since 2026-10-05. Three SIGABRTs were
  `LLVM ERROR: Do not know how to split this operator's operand!` (`forward_ambient`,
  `occlusion`, `headless`) and one a SIGSEGV in `forward_ambient`. OxidANT uses no ray
  tracing and no subgroups, so `ci-container-steps.sh` exports `LP_NATIVE_VECTOR_WIDTH=128`
  on aarch64, the native NEON width.
- **The arm64 parallel renderer crash has a name (2026-10-06).** Dispatch 37422053110 ran the
  renderer tests in parallel on `windows-11-arm`: `ibl.exe` died with `0xC0000005
  STATUS_ACCESS_VIOLATION`, faulting module `d3d10warp.dll` at offset 0x80528 (WARP
  10.0.26100.9278), reported by the hub runner's exit-code and WER naming. The fault is in
  Microsoft's WARP, so the renderer tests stay serial there and the BACKLOG row is closed.
- **The Linux x64 feature check runs on every push (2026-10-06).** `feature-matrix` lost its
  `[build-features]` marker, the last opt-in job here: the owner's rule is that every lane
  runs. Its first run ever, dispatch 37423257667, passed all six rows (`gstreamer`,
  `gui_linux`, `gui_linux,gstreamer`, `gui_windows`, `onnxruntime`, `burn_demos`).
- **The arm64 lane can run the renderer tests in parallel on dispatch (2026-10-06).** The
  `parallel-renderer-tests` input of `windows-arm64-cross.yml` drops Stage-CrossTests.ps1's
  `--test-threads=1` from `tests.json` on the runner, so one run can name the WARP crash
  with the hub runner's exit-code and WER reporting. Push runs stay serial.
- **The Linux lanes ship a deb, an AppImage and a flatpak beside the tar (2026-10-06, hub
  `1b99f7b5`).** `PACKAGE_TYPES` was `tar`, because the hub's `package_archive.sh` wrote the
  tar and stopped. It now builds all four from the release `kataglyphis_cli`, installed as
  `oxidant`:
  - the deb as `/usr/bin/oxidant`, depending on `libc6, libgcc-s1` (the binary links
    `libc`, `libm` and `libgcc_s`);
  - the AppImage;
  - the flatpak as `org.kataglyphis.oxidant` on freedesktop 24.08.

  The lane passes `packaging/linux/oxidant.desktop` (`Name=OxidANT`, `Terminal=true`) and
  `images/logo.png`, a real 128×128 PNG. `packaging/flatpak/` is gone: its `icon.png` was
  the escaped text of a 1×1 PNG, `icon.png.b64` a 1×1 image, and its hand-written
  manifest pointed at `../../target/release/oxidant` and was never read. Proven in
  `:latest` on this binary: the deb's binary runs, the AppImage runs, and the flatpak
  installs and runs through `flatpak run`. The BACKLOG row is closed.
- **The renderer iterates fixed chunks as arrays (2026-10-06).** All 46 sites in
  `crates/webgpu_renderer` that clippy 1.98's `chunks_exact_to_as_chunks` flagged now use
  `as_chunks::<N>()`, so each element is a `&[T; N]`:
  - the glTF and OBJ triangle loops, with `tri.map(|i| i as usize)`;
  - the HDR tests;
  - histogram;
  - IBL, whose BRDF read-back is now a plain `.to_vec()`;
  - LOD and QEM;
  - 26 in the integration tests, which clippy reaches only once the lib compiles.

  The two GPU timestamp read-backs drop their `try_into().expect(…)` for
  `u64::from_le_bytes(*b)`. The workspace-wide allow in `Cargo.toml` is gone. The one
  `chunks_exact` left, `channels * 2` in the 16-bit PNG path, has no constant size.
  In `:latest`: workspace clippy with CI's features and `-D warnings` is clean, rustfmt is
  clean, and the renderer's 163 unit tests and every integration suite pass, the 38
  `headless` GPU tests on lavapipe included.
- **The MSI step and the helper table became hub calls (2026-10-05, hub `b9247b03`, CON55).**
  The *MSI Packaging* step of `scripts/windows/Build-Windows.ps1` calls the hub's
  `Invoke-MsiPackage`. It used to resolve `wix.exe` itself, assemble the `wix build` argv and
  write the payload fragment, about 50 lines. It still decides what the payload is: the G6-proven
  DLLs, `lib\` and `resources\`. `AGENTS.md` § 2 links the hub's
  *Reach for these before writing a helper* table instead of carrying a copy of it. With those
  two, `BACKLOG.md` § *Waiting on ANTfrastructure* is empty and gone.
- **Five local copies became hub calls (2026-10-05, hub `54e7d7fc`, CON55).** The hub's
  `Get-ANTfrastructurePin` replaces `Resolve-CargoToolPin` in `scripts/windows/Build-Windows.ps1`.
  The hub's cargo drivers register `CARGO_SAFE_DIRECTORY`, so `ci-container-steps.sh` only
  exports it; `riscv64-test`, which runs cargo itself, sources the hub's `_cargo_wrapper.sh`.
  The README links the hub's MSIX trust steps instead of repeating them. The hub's unused
  `windows/scripts/rust/Build-Windows.ps1` is deleted by owner decision: this repo owns its
  Windows Rust build.
- **Comments are one line, only the why (2026-09-30).** The family rule, linked from
  `AGENTS.md` to [the hub's `AGENTS.md`](third_party/ANTfrastructure/AGENTS.md#comments-one-line-only-the-why):
  140 files, 5.7k lines out and 1.2k in. Traps moved to their docs pages (for example
  `crates/webgpu_renderer/docs/renderer-bounds-invariant.md`), and `AGENTS.md` gains
  *Tests and scripts that read source text*. The line-number pins in
  `buffer_desc_single_definition.rs` follow the edit (`occlusion.rs:358`,
  `histogram.rs:102`). `cargo fmt --check`, clippy `-D warnings` and the source-reading
  tests pass. ANTfrastructure moves to `497ba711` for the gate that grades every
  language and the bootstrap templates the two local copies follow.
- **`run-producer-pi.sh` runs through the image's entrypoint (2026-09-29).** All three
  `nerdctl run`s drop `--entrypoint bash`; `/hostlibs` is handed in as the caller's
  `LD_LIBRARY_PATH`, which ANTfrastructure CON23's entrypoint keeps ahead of the
  image's libcamera, behind GCC's runtime. The prologue appends the image's other
  media paths and refuses an image older than CON23. Proven under QEMU only; a board
  run is open in BACKLOG.md.
- **The chain ORT staging and every payload's proof are the hub's
  `WindowsOrtPayload.Common`** (hub `73a8dd74`, 2026-09-25), which grew out of this
  repo's own module of that name. The local copy and its Pester suite are gone;
  the cases live on in the hub's `OrtPayload.Common.Tests.ps1`. The hub's copy
  refuses an ONNX_ROOT that is not the chain install for the target arch before
  it copies anything, where the local one only checked for `bin\onnxruntime.dll`.
  `Build-Windows.ps1` names the hub commit it needs when the pin is older.
- **Build-Windows' clippy lints the CLI and the GUI crates as well as the root
  package**, with the CLI's features qualified (`kataglyphis_cli/...`). Linting the
  root package alone had left the Windows-only GUI code, which only the CLI's
  features switch on, unlinted until it reached the release build (2026-09-25).
- **Every platform lane runs on every push and PR, under the family's new
  workflow names (owner request, 2026-09-24).** `rust_ubuntu26_04.yml` became
  `linux-x64.yml` ("Linux x64 · build + test") and `linux-arm64.yml` ("Linux
  arm64 · build + test"), two thin callers of one `reusable-linux.yml` ("Linux ·
  reusable build", `workflow_call`) that holds every step both architectures
  share; the x64 caller keeps the docs publish and the opt-in feature check.
  `rust_windows2025.yml` became `windows-x64.yml` ("Windows x64 · build +
  test"). The arm64 row no longer needs `[build-arm]` and the Windows lane no
  longer needs `[build-win]` - no build job carries an `if:` - so neither
  reports `skipped` behind a badge that looks like a pass. `workflow_dispatch`
  stays on all three. `lint-gates.yml` is displayed as "Lint gates" and
  `submodule-pins.yml` as "Submodule pins". Each triggered build workflow has
  its own concurrency group (the Windows lane had none); every build job
  carries `timeout-minutes` and a `permissions:` block, and every artifact
  upload `if-no-files-found: error`, which takes this repo's hub
  workflow-conventions census from 9 findings to 0. README badges, AGENTS.md,
  BACKLOG.md, `.github/actionlint.yaml` and the script headers that named the
  old files follow.
- **ONNX Runtime is the family's chain build only, loaded at run time
  (owner rule 2026-09-23).** `ort/download-binaries` is gone from every
  feature: `onnxruntime`, `onnxruntime_directml`, `onnxruntime_cuda`,
  `burn_demos` and `crates/gui`'s `onnxruntime` statically linked pyke's
  prebuilt ORT 1.28.0 from cdn.pyke.io, including the shipped
  `kataglyphis_cli` release. Every ORT feature is `load-dynamic` now
  (`onnxruntime_dynamic` stays as an alias); `Cargo.lock` lost ureq 3, the
  TLS stack and `openssl-sys` with it, nothing was upgraded.
  `crates/inference/src/ort_runtime.rs` picks the dylib — `ORT_DYLIB_PATH`,
  the exe's directory, then the image's chain prefix — and refuses the bare-name
  load that reached Windows ML's `System32\onnxruntime.dll`. It hands `ort` an
  absolute path only (a relative `ORT_DYLIB_PATH` went to the OS search
  verbatim), and refuses any file that does not embed the chain's ORT source
  path (`C:\temp\onnx-src\onnxruntime\core\`, `/opt/onnxruntime/onnxruntime/core/`),
  so a PyPI, GitHub-release or Windows ML copy named by `ORT_DYLIB_PATH` no
  longer loads just because it clears ort's API-24 floor. The CUDA path no
  longer copies provider DLLs out of pyke's download cache. The Windows release
  zip (`scripts/windows/New-ReleaseArchive.ps1`), MSIX and MSI carry the chain
  `onnxruntime.dll` (+ `DirectML.dll`, `onnxruntime_providers_shared.dll`)
  staged from `$env:ONNX_ROOT\bin`. Each package ships a payload that
  ANTfrastructure's ORT census (G6, `Test-OrtProvenanceTree`) proves byte for
  byte against the image's chain ORT, with every importer resolving to it
  (`scripts/windows/modules/WindowsOrtPayload.Common.psm1`); a hub pin before its
  ORT single-source commit of 2026-09-23 stops the build naming it. Whether a
  payload loads ORT is decided over the exe and every DLL it ships, so an
  ORT-consuming DLL beside a plain exe gets the chain ORT and its G6 proof too,
  instead of shipping with nothing beside it for System32's copy. The Linux lane gates
  it with `scripts/linux/check-ort-chain-only.sh` (lock + resolved feature
  graph, all targets, and every `ort` declaration in the graph must carry
  `load-dynamic` — the workspace dependency declares it, so no feature subset
  can link ORT). `run-producer-pi.sh` defaults `ORT_DYLIB_PATH` to
  `/usr/local/lib/onnxruntime-cpu/lib`, not `/opt/opencv5`'s copy.
- **`crates/cat_webrtc` no longer bakes in a path into a sibling submodule
  checkout.** `--image` has no compile-time default; it falls back to
  `$KATAGLYPHIS_CAT_IMAGE`, and with no image and no live-source flag the binary
  exits with a message naming both. The old default pointed out of this
  repository, past its superproject and into another submodule's working tree,
  and resolved nowhere else (decision D5).
- **The Linux CI lane runs `scripts/linux/ci-container-steps.sh <step>`** instead
  of eight inlined copies of the same container prologue.
- **The docs publish and `cancel-in-progress` follow the repository's actual
  default branch** rather than a typed `refs/heads/main`.
- **`AGENTS.md` was rebuilt on ANTfrastructure's six-section template.**
- **`third_party/ANTfrastructure` moved to `d339cd52`, and the `powershell-lint`
  job became a `uses:`** — the retirement the job's own comment specified one
  pin earlier ("when a reusable lane carries this gate without a Python build
  attached"). Hub `d339cd52` added `build-python-package` to the reusable
  `python-ci-windows.yml` (boolean, `default: true`, so every caller written
  before it is byte-for-byte unaffected), gated
  `build-test-python-package-on-windows` on it, made `GHCR_PAT`
  `required: false` — a required secret is refused at call time and would have
  kept the lint unreachable for exactly the callers it was added for — and made
  the build job assert the token itself, naming `build-python-package: false`
  as the alternative. The call here passes `build-python-package: false`,
  `lint-powershell: true`, `lint-path: scripts` and **no `secrets:` block**;
  nothing was held back from the deleted job. It was compared field by field
  first, not assumed: same `windows-2025` (the lane's `runs-on` default), same
  `timeout-minutes: 20`, the same pinned `actions/checkout@3d3c42e5` (v7.0.1)
  with `submodules: true` and `fetch-depth: 1`, the same
  `Install-Module PSScriptAnalyzer -RequiredVersion 1.25.0 -Force -Scope CurrentUser`,
  and the same `Invoke-Lint.ps1 -Path <lint-path> -FailOnAnalyzer` — where
  `-FailOnAnalyzer` is unconditional upstream and is deliberately not an input,
  which is what this tree measures (`PSSA: 0 error(s), 0 warning(s)
  [PSScriptAnalyzer 1.25.0]` over all 9 files). `shell: pwsh` arrives from the
  lane's workflow-level `defaults` instead of the deleted job-level block. The
  one difference is **additive**: upstream tests for the gate script first and
  throws a message naming the submodule, where the copy here would have
  reported a bare `pwsh -File <absent>` path error. This supersedes the
  "the job stays" decision recorded in the `35960a87` row below. The
  shared-config templates did not move across this bump either, which
  `sync-shared-config.sh --repo-root . --check` confirms rather than the commit
  range being taken on trust ("Shared config in sync", both rows OK). All three
  gate jobs in `lint-gates.yml` are now one `uses:` onto ANTfrastructure.
- **`third_party/ANTfrastructure` moved to `35960a87`**, and nothing local was
  deleted for it — deliberately, row by row. The shared-config templates are
  byte-identical across the bump, which
  `sync-shared-config.sh --repo-root . --check` confirms rather than the commit
  range being taken on trust ("Shared config in sync", both rows OK).
  `lint-powershell`/`lint-path` on the hub's reusable `python-ci-windows.yml`
  reproduce this repo's `powershell-lint` job exactly — same runner, same
  pinned PSScriptAnalyzer 1.25.0, same unconditional `-FailOnAnalyzer`, and
  `lint-path` defaults to the `scripts` this job passes — but they are inputs
  to the family's **Python** lane, whose build job has no `if:` and would run a
  Python package build and demand a `GHCR_PAT` alongside the two-minute lint;
  the job stays, with that recorded where it lives.
  `WindowsMediaRuntime.Common` stages a GStreamer/ONNX DLL closure and replaces
  nothing here (the `onnxruntime_*` names in `scripts/windows/` are cargo
  features, not a staging step). `fix_bind_mount_ownership` is a `chown` of a
  container-written tree, not the `git safe.directory` guard
  `ci-container-steps.sh` keeps, so that `BACKLOG.md` row stays blocked.
  `cmake-build.sh --configure-arg` and the bandit argument fix have no caller
  here — no CMakeLists.txt, no bandit. The flatpak pair does not reach
  `package_archive.sh`, which is unchanged, so the tarball-only row stays open.
  All eight "Waiting on ANTfrastructure" rows were re-checked against the new
  pin and say so.
- **`third_party/ANTfrastructure` was pinned at `3c83a5e3`**, and three local
  forks went with the bump. `scripts/linux/ci-container-steps.sh`'s `fmt-clippy`
  case delegates to `cargo_fmt_clippy.sh` now that the driver takes
  `CARGO_CLIPPY_ARGS` instead of hard-coding `--all-features`; the ~100-line
  MSIX orchestration in `scripts/windows/Build-Windows.ps1` is one
  `Invoke-MsixPackage` call; and both of that script's inline version parses are
  one `Get-PackageVersion`. The vendored `scripts/linux/lib/antfrastructure.sh`
  was re-synced to the rewritten template.
- **Both gate lanes are one `uses:` onto a reusable ANTfrastructure workflow**
  rather than a copied job — `lint-gates.yml` and `submodule-pins.yml`.
- **`.gitmodules` uses an `https://` url**, not `git@github.com:`: an SSH pin is
  inherited by a recursive checkout from OmniAccelerANT, where no key exists.
- **`BACKLOG.md` no longer claims the ARM lane cannot go green.**
  `:latest-cross` is a multi-arch OCI index carrying amd64, arm64 and riscv64 —
  verified 2026-09-15 with `nerdctl manifest inspect`, and AGENTS.md has said so
  since the index landed on 2026-09-04. The lane stays opt-in via `[build-arm]`,
  which is a runner-minutes decision and not a blocker.
- **The Pi runner asks ANTfrastructure for both things it used to retype.**
  The CI image reference comes from `linux/scripts/ci-image-ref.sh` (a literal
  in a tracked `*.sh` is what the CI-image-ref lint gate refuses), and the
  loader path is composed by the image's own `media-env.sh` plus
  `path-helpers.sh`'s `_path_prepend_unique` instead of a hand-typed list that
  had `/opt/gcc-16.2.0` frozen into it.

### Fixed
- **MSIX signing works, and `Build-Windows.ps1` does it (2026-09-25).** The README's
  route ran ANTfrastructure's standalone `New-MsixPackage.ps1` over
  `packaging/msix/AppxManifest.template.xml`, but that script fills `__PACKAGE_NAME__`-style
  tokens and the template carries the `__MSIX_*__` tokens `Build-Windows.ps1` fills, so the
  manifest came out unfilled. `Build-Windows.ps1`'s MSIX step now passes the hub's
  `-Sign -SigningRoot`: a `*.pfx` at the repository root and `MSIX_PFX_PASSWORD` sign and
  verify the package, and without one it stays unsigned, as in CI. The README makes a
  test certificate with the hub's `GenerateCertificateMSIX.ps1`, and its MSIX and MSI
  paths follow `dist\windows-<arch>`. Needs the hub commit where `-Sign` takes
  `-SigningRoot`.
- **The ORT payload suite follows the hub's stricter G6 fingerprint (2026-09-25).**
  Since hub 502b9089, the census counts a source path as an ORT fingerprint only when it
  is a whole `__FILE__` string ending in NUL. The hub bump to 2fdddbc7, made for the
  Windows sccache fix, brought that rule here. `OrtPayload.Tests.ps1`'s fake chain and
  stale DLLs wrote the path followed by a space, so the stale case read as `UNPROVEN`,
  not `STALE`, and the lint lane's Pester step failed (run 36101778073). The fixtures
  now end the path in NUL, as OmniAccelerANT's `OrtRunner.Tests.ps1` does: 10 of 10 pass.
- **The `gui_windows` build compiles again (2026-09-24).** The wgpu 30 upgrade set
  the new `SurfaceConfiguration::color_space` in the renderer but missed the second
  literal in `crates/gui/src/gui_wgpu/wgpu_init.rs`. That module compiles only under
  `gui_windows`, so no Linux build had ever seen it. The Windows lane stopped there
  with `E0063` (run 36044802271). It now uses `SurfaceColorSpace::Auto` like the
  renderer. Checked in the Linux image with `cargo check --locked`: the root crate
  with `gui_windows`, plus all four GUI rows of the Windows config matrix built as
  `kataglyphis_cli`. Without the fix the same check reproduces the `E0063`. The
  opt-in `feature-matrix` job gains a `gui_windows` row for that reason, and the
  "`gui_linux` is the wgpu path" claim is corrected in the job's comment and in
  AGENTS.md: that feature pulls the wgpu dependencies but compiles no GUI module.
- **The WebGPU renderer compiles for wasm32 again (2026-09-24).** No published
  `egui-winit` builds for `wasm32-unknown-unknown`: on wasm32, egui's `DroppedFile`
  requires `bytes_async`, and `egui-winit`'s `NativeFile` implements only `bytes`
  (emilk/egui#8436). The renderer's wasm demo drives its overlay through
  `egui_winit::State`, so BeschleunigerBallett's wasm size-budget gate, which builds
  it for wasm32, failed (run 36020443791). Upstream fixed it in #8516, merged to
  egui's main on 09-07 but not in 0.36.2. `third_party/egui-winit-0.36.2` is the
  published crate (sha256 checked against the crates.io index) with #8516's `lib.rs`
  change applied unchanged. The root `Cargo.toml` routes `egui-winit` to it through
  `[patch.crates-io]`, and the egui family moves to 0.36.2 (the patched crate needs
  it). Checked in the Linux image: `cargo check --locked` for wasm32 and native, and
  the gate's own `cargo build --release` for wasm32 (10,241,933 bytes before
  wasm-opt, budget 12 MiB after). Temporary: `PATCHED.md` and a blocked BACKLOG
  item say when and how to remove it.
- **The host run of the renderer's lib tests finds their fixtures (2026-09-24).** The
  first host run (36038436509) started the binary and ran 163 tests: 159 passed and
  4 failed, every one on a path. The tests embed `env!("CARGO_MANIFEST_DIR")` at compile
  time, which in the container is `C:\ws\crates\webgpu_renderer`. The runner's checkout
  is `D:\ws`, so `tests\assets\*.gltf` and the crate's `src` tree were not found.
  `Invoke-HostTests.ps1 -ContainerRoot 'C:\ws'` now links that path to the workspace
  with a directory junction (no admin needed). It refuses an existing `C:\ws` that is
  not a junction to this tree. Tested in a throwaway tree: create and read through the
  junction, accept a matching one, refuse a real directory.
- **The Windows lane runs the WebGPU renderer's lib tests on the runner host
  (2026-09-24).** Its first always-on run (36019995362) built everything and
  then failed `Invoke-DebugTests.ps1`'s renderer step: the lib test binary
  exited `0xc0000135` (`STATUS_DLL_NOT_FOUND`) before `main`, like every
  renderer test binary. wgpu's `gles` backend imports `opengl32.dll` at load
  time, and servercore does not ship it (README, *Tests*). The container now
  only builds that binary (`cargo test --no-run --message-format=json`) and
  records its path in `target\host-tests\`. A new host step,
  `Invoke-HostTests.ps1`, runs it from the same bind-mounted tree. It refuses
  a missing list, an empty list and a missing binary. Tested against cargo's
  message shapes under strict mode, and against those failure cases.
- **A pull request no longer reds the Linux lane at its artifact upload
  (2026-09-24).** The artifact was named after `github.ref_name`, which on a PR
  is `<number>/merge`, and `upload-artifact` refuses a `/` in a name - run
  35752031200 (2026-09-22) built, tested and packaged green and then failed
  there. It is named after `VERSION` now, as the Windows lane's artifacts and
  the tarball already were. Found while making the lane run on every PR on
  both architectures.
- **The `TEXTURE_2D_SHAPE_OK` regression guard now matches the code rustfmt
  actually produces.** `texture_desc_single_definition` demanded the marker
  comment on the *same line* as the `wgpu::TextureDescriptor {` it exempts, but
  that line runs past `max_width` at all three non-goal sites, so rustfmt 1.9.0
  (toolchain 1.98.1) moves the comment down into the literal's body and puts it
  back there on every `cargo fmt`. The two gates contradicted each other: no
  source text could satisfy both. The guard now accepts the marker on the
  literal's own line *or* as the first line of its body. Every assertion is
  unchanged - an unmarked literal still fails, the marked count is still exactly
  3, and `render/texture.rs` still holds exactly 1 definition. Latent since
  2026-08-07, when the graphics-stack upgrade reflowed the three comments; it
  surfaced only now, because until the security gate above went green the lane
  failed at `cargo audit` and never reached `cargo test`.
- **The security gate is green again**, entirely by upgrading rather than by
  ignoring: `h2` 0.4.15 → 0.4.19 (RUSTSEC-2026-0258), `chacha20` 0.10.1 → 0.10.2
  (yanked), `stable-vec` 0.4.2 → 0.4.3 (unsound) and `rustls` 0.23.43 → 0.23.45
  with `rustls-webpki` 0.103.13 → 0.103.15 (RUSTSEC-2026-0285). No RUSTSEC id
  was added to either ignore list; two stale ones were removed from both
  (quick-xml RUSTSEC-2026-0194 / -0195, fixed by quick-xml 0.41.0).
- Formatting and clippy against the pinned toolchain 1.98.1: two rustfmt hunks,
  and clippy 1.98's new `chunks_exact_to_as_chunks` allowed once at the
  workspace level with a reason (see `BACKLOG.md`).
- `scripts/windows/Build-Windows.ps1` installs cargo-audit and cargo-deny
  `--version`-pinned from `versions.env`, and no longer swallows the install
  failure or prepends a scoop shims directory that does not exist in the image.

### Removed
- 15 committed Windows build logs under `logs/windows/` (untracked, kept on
  disk), the empty README *Prerequisites*, *Roadmap* and *Acknowledgements*
  sections, and the Lorem-ipsum feature table.

## 2026-08-07

### Verified baselines (container, 32 CPUs)

**2026-08-07, winamd64, rustc 1.97.1** — `Invoke-StevedoreBuild.ps1 -MemoryGb 32`:

- Builds: debug 1m35s, profile 1m32s, release 1m12s — all three green. Release binary verified on the host: `stats --path README.md` → `Lines: 476, Words: 1905, Bytes: 20104`. (Those figures describe the README **as it stood that day**; the file has since grown — `wc -lwc README.md` measured 537/2512/24364 on 2026-09-06 — so a re-run printing bigger numbers is the tool working on a bigger file, not a regression. Compare a re-run against the README of the same date, not against this line.)
- Tests: the 8 that predate `crates/webgpu_renderer` still pass (3 integration, 1 proptest, 4 telemetry). **`kataglyphis_webgpu_renderer` is excluded from the container run** — `scripts/windows/container/Test-RustAll.ps1` passes `--exclude kataglyphis_webgpu_renderer` and logs that it did. Its test binaries exit `0xc0000135` (`STATUS_DLL_NOT_FOUND`) before `main`, because linking wgpu with the `gles` backend makes the executable import `opengl32.dll`, which Server Core does not ship. The loader resolves that import, so no runtime flag helps; without the exclusion the whole `cargo test --workspace` crashed and reported nothing. `gles` stays on purpose (OpenGL fallback for hosts without Vulkan/DX12) — run `cargo test -p kataglyphis_webgpu_renderer --locked` on a desktop Windows machine instead.

  Not a regression from the wgpu 30 upgrade. The old "8 passed / 0 failed" baseline was recorded on 2026-07-17, and the renderer crate landed on 2026-07-18 — the container test lane has therefore *never* run with that crate present. The image is Server Core with no GPU stack; a wgpu-linked binary needs graphics DLLs it does not ship.

  `Invoke-StevedoreBuild.ps1 -Test` used to fail as a whole because of this. It no longer does — the exclusion lives in `Test-RustAll.ps1`, so the container lane is green and reports 8 passed. Drop the `--exclude` once the image carries the missing DLLs.

**2026-07-17** (superseded, kept because it is what the 8-test figure refers to): builds debug 1m11s / profile 1m31s / release 1m08s; tests 8 passed / 0 failed, 1 doc-test ignored — measured before `crates/webgpu_renderer` existed.

### Packaging: what actually happens when you run it

Verified 2026-08-07 by running `Build-Windows.ps1 -SkipTests` in `:winamd64`:

- **MSIX works.** `Kataglyphis.RustProjectTemplate_2.3.4.0_x64.msix`, 51.69 MB, manifest with every token substituted. `makeappx.exe` resolves via `Resolve-WindowsSdkToolPath` to `Windows Kits\10\bin\10.0.26100.0\x64\`. The identity became `Kataglyphis.OxidANT` on 2026-09-05, so a build today writes `Kataglyphis.OxidANT_<VERSION>_x64.msix`; the old filename stands here because it is what that run actually produced. Windows treats the two identities as different apps, so an installation predating that date is not upgraded — it has to be uninstalled first, see the MSIX section of the README.
- **MSI works, but only since the WiX v4 migration** (2026-08-07). It had never produced a file. Two independent faults, both masked by the step being optional:
  1. `cargo wix -p kataglyphis_cli` looks for WXS files inside the package it was pointed at (`crates/cli/wix/`); this repo keeps its single WiX source at the workspace root. `Msi.WxsFile` had been sitting unread in the config the whole time.
  2. Even with the path fixed, **cargo-wix cannot drive this image.** 0.3.9 is its newest release and it shells out to WiX v3's `candle.exe`/`light.exe`. ANTfrastructure installs **WiX 4.0.6** as a dotnet tool — a single `wix.exe`, no candle — so it failed with *"The compiler application ('candle') does not exist at the 'C:\WiX' path"*.

  `Build-Windows.ps1` now calls `wix.exe build` directly (resolved from `$env:WIX`, then PATH) and `wix/main.wxs` is **WiX v4 schema**: `<Package>` instead of `<Product>` + inner `<Package>`, `<SummaryInformation>`, `<StandardDirectory>` instead of the `TARGETDIR` nesting, `Bitness='always64'` for `Win64='yes'`, `AllowAbsent` for `Absent`, and `<ui:WixUI>` for `<UIRef>`. `WixUI_FeatureTree` needs `-ext WixToolset.UI.wixext`, which the image already ships (4.0.4). Paths that move with the build — the binary follows `CARGO_TARGET_DIR` — go in as `-d Version= / ExeSource= / LicenseRtf=` preprocessor variables, so the WXS never assumes a `target\release` beside the workspace root.

  **If you touch this: cargo-wix is not an option again unless it gains WiX 4 support.** Check its releases before reintroducing it.

**Packaging and security steps are now `Invoke-BuildStep -Critical`, not `Invoke-BuildOptional`.** That matters because ANTfrastructure's `Invoke-BuildOptional` is `try { & $Script } catch { Write-BuildLogWarning }` and **never registers the step with the build context** — so it cannot appear in the summary at all. The pre-fix run reported **"7 steps, 7 succeeded, 0 failed (100% success rate)"** while MSI *and* the license check had failed. If you see a suspiciously perfect summary, that percentage covers only the `Invoke-BuildStep` steps; read the WARNING lines.

`cargo-deny licenses` also failed on that run (advisories, bans and sources passed) and was equally invisible. Fixed by allowing `BSL-1.0` in `deny.toml` — xxhash-rust via cubecl-common → burn; it was the only rejection.

`CARGO_TARGET_DIR` may be absolute — the in-container scripts set `C:\ct`. `Build-Windows.ps1` now handles that (`IsPathRooted`); before, `Join-Path` produced `C:\...\workspace\C:\ct\msix-staging` and MSIX died on "The filename, directory name, or volume label syntax is incorrect".

### The graphics-stack upgrade

`wgpu` 29→30, `naga` 29→30, `egui`/`egui-wgpu`/`egui-winit` 0.35→0.36, `glam` 0.30→0.33 and `pollster` 0.4→1.0 moved as one coupled set (`egui-wgpu` 0.35 pins `wgpu ^29`, 0.36 pins `^30`, so none of them could move alone). What changed, so the next person does not have to rediscover it:

- **`VertexState::buffers` is `&[Option<VertexBufferLayout>]`** — a slot can now be left unbound without shifting the ones after it.
- **`BufferSlice::get_mapped_range` returns `Result`.** Seven call sites. Only `render_to_pixels_with_format` returns `Result` and propagates; the other six are in functions whose caller has already awaited the map, so they `expect` with a message naming that invariant.
- **Presentation moved from `SurfaceTexture::present(self)` to `Queue::present(&self, texture)`.** Three sites, two of which the default Linux build never compiles (one is `cfg(wasm32)`, one is behind a GUI feature) — check them by hand or with the `feature-matrix` job.
- **`RequestAdapterOptions::apply_limit_buckets`** (new, no default): rounds reported adapter limits to coarse presets so a host exposing wgpu to *untrusted* content cannot fingerprint the machine. This renderer is the trusted application, so it is `false` — real limits, as wgpu 29 had.
- **`SurfaceConfiguration::color_space`** (new, no default): set to `SurfaceColorSpace::Auto`, the type's own default and the only value guaranteed supported for every format in `SurfaceCapabilities::formats`. Anything else (an HDR space) needs a capability check first.
- **glam 0.33 moved the camera constructors off `Mat4`** and split them by clip-space convention: `opengl` (NDC Z −1..1), `directx` (Z 0..1, Y up), `vulkan` (Z 0..1, Y down). **`directx` is the one that matches** — it reproduces the old `Mat4::perspective_rh`/`orthographic_rh`/`perspective_infinite_rh` bit for bit. That was verified by compiling both against glam 0.33.3 and comparing the matrices, not inferred from the names: the `vulkan` module is Y-**down**, and picking it would have flipped the image with no compile error. The old methods are deprecated but still present, so `-D warnings` is what forces the migration.

Guard this with the golden tests, not with the compiler: a clip-space or Y-axis mistake compiles perfectly and only shows up in pixels. See the GPU note above for how to make them actually run.

The upgrade was verified rendering-neutral: 333 tests pass, the only failure is the pre-existing instance-normal bug, and it still reports **exactly 987 differing pixels** — the same figure as before the upgrade. That number is the useful signal here; a changed clip-space or flipped Y would have moved it.

### The tract 0.22 → 0.23 migration

Dependabot offered this as `build(deps): bump tract-onnx from 0.22.3 to 0.23.4`. It is **not** a drop-in bump — it breaks in four separate ways, none of which the PR title suggests, and only `crates/inference/src/person_detection/{mod,tract_backend}.rs` are affected (the `onnx_tract` feature is off by default, so nothing else notices).

- **`SimplePlan` is gone from the prelude.** It was renamed to `RunnableModel`; the alias to use is `TypedRunnableModel`, and it is **fully applied** — `pub type TypedRunnableModel = SimplePlan<TypedFact, Box<dyn TypedOp>>`. Passing it a generic argument (the old third `TypedModel` parameter) fails with *"type alias takes 0 generic arguments but 1 generic argument was supplied"*.
- **`run` takes `self: &Arc<Self>`.** A `Box<TractPlan>` does not resolve the method at all — the error is a bare *"no method named `run`"*, which reads like a missing trait import and is not.
- **`into_runnable()` already returns an `Arc`.** So the Arc is neither ours to add nor to strip; `load_tract_model` returns `Arc<TractPlan>` and the `Backend::Tract` variant stores it directly.
- **`Tensor::as_slice` was removed.** The safe replacement is `to_plain_array_view::<f32>()`, which errors unless the storage is plain *and* the datum type matches — the same two conditions the old call checked. rustc's *"there is a method `slice` with a similar name"* suggestion points somewhere else entirely; do not follow it.

The lockfile also gains `tract-extra`, `tract-pulse`, `tract-pulse-opl`, `tract-transformers` and `typeid`. `cargo deny check licenses` passes with them (verified, exit 0) — no new `deny.toml` allowances were needed.
