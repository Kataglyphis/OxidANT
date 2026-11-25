# Shader sources and regeneration

`shaders/` owns the WebGPU Slang entry points and shared math. Its manifest
defines the entry points, WGSL destinations, depth-texture patches and minimum
compiler version. `crates/webgpu_renderer/src/shaders/`, the GUI's shader
directory and the renderer's `tests/shaders/` contain generated WGSL, all of
it: `histogram.wgsl` comes from `histogram/histogram.slang` since 2026-11-23.

Every WebGPU pass is target-neutral Slang: a fullscreen pass takes its uv from
`fullscreen_vs` (the IBL bake included, whose own triangle would have mirrored
the faces and the BRDF LUT on SPIR-V) and a projected point's texel from
`fullscreen_ndc_to_uv` (`ssao`, `gpu_cull`). Atomics are Slang's `Atomic<uint>`,
which slangc 2026.17 emits as WGSL's `atomic<u32>` for a storage buffer and a
`groupshared` array alike; `InterlockedAdd` does not compile for WGSL. A pass
gets a SPIR-V row with the C++ change that binds it. The forward pass's shadow
lookups still convert NDC to uv by hand, WebGPU's way; the shared shadow Slang
replaces them.

Run in the family build image from the repository root:

```bash
bash scripts/linux/compile-slang-shaders.sh
bash scripts/linux/compile-slang-shaders.sh --check
```

On Windows use `pwsh -File scripts/windows/Build-SlangShaders.ps1`, with `-Check`
to compare fresh output with the committed WGSL. The check uses a temporary
destination and fails on missing or changed output. Linux's `fmt-clippy` step
runs it. Compiler resolution, import paths, caching and emit validation belong
to ANTfrastructure's Slang driver.

BeschleunigerBallett imports this manifest and shared math from its pinned
OxidANT submodule. Its Vulkan entry points and material modules stay in that
repository. Commit source and generated WGSL here before updating its pin;
an OxidANT checkout needs no parent source tree to run its shader parity tests.

Six leaf modules under `common/` import nothing and hold what more than one shader needs:
`constants.slang` (`PI`, `MIN_ROUGHNESS`), `hdr_safe.slang` (`HDR_MAX`, 65000, `hdr_safe`, which
finds a NaN by its bits because WGSL may assume floats are finite, and `is_finite`, which reads the exponent bits
for the same reason), `trace_constants.slang` (every
secondary ray's `TRACE_TMIN`, `TRACE_TMAX`, `TRACE_SURFACE_OFFSET` and the punctual shadow ray's
`TRACE_LIGHT_SPAN`), `aces.slang` (the five ACES coefficients, `aces_tonemap`, `aces_tonemap_inverse`,
`linear_to_srgb`) and `fullscreen.slang` (`fullscreen_vs`, `fullscreen_uv_to_ndc` and its inverse
`fullscreen_ndc_to_uv`, switched per target). `limits.slang` holds the limits both renderers and Rust share
(`CASCADE_COUNT`, `MAX_PCF_RADIUS`) as `static const int` lines only, because the C++ engine includes it and
`kataglyphis_render_core::shader_limits` reads it at compile time. BeschleunigerBallett's shaders import the same files.
`scripts/check-shared-shader-copies.py`, run by `pre-push-checks.sh`, fails a file outside the owner
that declares one of these names, writes one of their literals (3.14159, 0.045, 65000, an ACES
coefficient, a ray bound) or a NaN check (`isnan`, the exponent mask, `x == x`), or defines one of their functions; a `fullscreen` importer
also may not convert between uv and NDC by hand. `render::tonemap`'s `aces_tonemap` and
`aces_tonemap_inverse` are the CPU twins, whose unit test reads the coefficients from `aces.slang`.

`common/camera_ray.slang` is the camera's ray in both renderers: `camera_ray(clip_to_world, ndc)` unprojects
the near and far plane, so a traced ray starts on the near plane, where raster clips, with the far plane as
its length; `camera_ray_through_pixel` takes a pixel through `fullscreen_uv_to_ndc`. The tracer's `pixel_ray`,
the sky and the cloud march take it (the last two only its direction; the march starts at the eye), and so do
BeschleunigerBallett's ray-generation shader, path tracer and cloud compute pass, which hand it
`mul(inv_view, inv_projection)`. The copy gate also fails a near- or far-plane unprojection, inline or through a
clip-space point declared a statement before, and the eye as a ray origin, written outside it.

`common/ambient.slang` is the sky's share of a surface's light. `ambient_split_sum` combines an irradiance and a
reflection by `brdf_direct`'s compensated albedo: the diffuse lobe takes `1 - E_spec` of the irradiance, the
specular lobe `E_spec` of the reflection. `analytic_ambient` feeds it the procedural sky when no environment is
baked: `sky_model.slang`'s `hemisphere_irradiance`, and the reflected gradient blurred towards that irradiance by
roughness squared. It takes no sun disk: `brdf_direct`'s GGX lobe carries the sun, so a disk here counted it twice
and kept a glossy surface's glint inside the sun's shadow (`forward_ambient.rs`'
`the_analytic_ambient_adds_the_sky_and_no_sun`). `forward_lighting.slang`'s `ibl_ambient` takes one or the other on the
environment flag and scales it by the ambient strength and the occlusion; BeschleunigerBallett's forward, deferred
and closest-hit shaders take `analytic_ambient`, scaled by the Sun's ambient, after the sun's shadow. The tracer's
ray-traced mode has no ambient term yet, and both path tracers get the sky through their bounces' misses. The copy
gate fails a second definition of either function.

`sky/sky.slang` is the sky both renderers draw into their HDR target, tonemapped with
the scene. The C++ renderer compiles `vs_vulkan_main` and `fs_main` to SPIR-V and fills
`SkyUniforms` itself; its vertex entry takes `SV_VulkanVertexID`, because `SV_VertexID`
subtracts the base vertex through draw parameters a Raspberry Pi 5's v3dv lacks. The
combined `sky.wgsl` carries that entry too; the Rust renderer draws `vs_main`. The
manifest builds both C++ entries to SPIR-V here as well, so a change that breaks that
target fails before the pin moves.

`fxaa/fxaa.slang`'s `fs_main` is the C++ renderer's FXAA too, behind its own
`SV_VulkanVertexID` fullscreen vertex, with the same three bindings and
`FxaaUniforms`; a change to either reaches its `FxaaStage` with the pin. Its manifest
row builds `fs_main` to SPIR-V too.

`common/volumetric_clouds.slang` is the volumetric clouds' mathematics for both
renderers: the noise texels, the unpack of the C++ `SceneUBO` cloud block, the
box, the density and the march. Its functions take an `ICloudNoise`, which
Slang specializes per caller, so each renderer binds the noise volume its own
way: `clouds/cloud_march.slang` a texture and a sampler, the C++ `clouds_main` a
combined `Sampler3D`. `march_clouds` takes the sky's ambient as a `CloudAmbient`,
which each host fills from the sky it draws. The block's `cloudLightMarch.yzw`
is the wind's offset of the noise, its velocity times the app's clock wrapped to
the noise's period, which both hosts pack (`CloudSettings::wind_offset`, the C++
`cloudWindOffset`).
`tests/volumetric_clouds_test.slang` compiles the C++ way to SPIR-V, so a change
that breaks that target fails here before it reaches the pin. WebGPU writes
`rgba16float` only through a write-only storage texture, which Slang emits from
`[format("rgba16f")] WTexture3D`; an `RWTexture3D` would emit `read_write`.

`common/shadow_pcf.slang` is the directional shadows' PCF kernel for both
renderers. `pcf_visibility` compares each tap at the receiver plane's depth
beneath it, which `receiver_plane_depth_slope` derives from the shading normal
and the affine light-space matrix (its cofactor rows), not from screen-space
derivatives, so the deferred pass can call it after non-uniform control flow.
The slope stops growing past `PCF_MIN_RECEIVER_COS` (84 degrees to the light
axis). Taps go through an `IShadowTaps`, so `forward/forward_lighting.slang`
binds a texture and a comparison sampler and the C++ `cascaded_shadow.slang` a
combined `Sampler2DArrayShadow`. Each `lit` counts an out-of-map tap as lit in
the statement that samples, where the C++ suite's
`BuildIntegrity.EveryPcfKernelBoundsChecksItsTaps` looks for the check;
`tests/shadow_pcf_test.slang` is the SPIR-V guard.

`common/path_estimator.slang` is both path tracers' estimator, ported from the
C++ `path_tracing.slang`: the PCG RNG and its per-pixel seed, Russian roulette,
the branchless orthonormal basis, visible-normal GGX sampling and its density,
the lobe probability and `sample_bounce`, which picks a lobe, draws the next
direction and weights it by `brdf_sampled` over the mixture's density. It names
no binding, so `tracer/traced.slang` and the C++ kernel each trace their own
rays around it.

`common/brdf.slang` compensates the GGX lobe's lost multiple scattering (Turquin 2019):
the specular term is scaled by `1 + f0 (1 / E_ss - 1)`, and the diffuse term takes
`1 - E_spec(mu)` of the light, not `1 - F`, keeping `1 - metallic`. A white surface then
returns all of a uniform sky at every roughness and angle; single scattering returned 0.67
of it for a white metal at roughness 0.7. `ggx_energy(n_dot_v, roughness)` evaluates E_ss
split by Schlick's Fresnel (scale, bias) from `common/brdf_energy_fit.slang`: a 14 x 14
cubic B-spline over coordinates uniform in n.v's cube root from brdf.slang's n.v floor 1e-4
and in roughness's square root from `MIN_ROUGHNESS`, written out as literal coefficients so
no shader indexes an array. It fits `brdf_sampled`'s own lobe and serves both remaps: the
analytic remap's extra masking shapes a punctual highlight and loses no bounce, and its own
E_ss would multiply a smooth metal's highlight by 13 at n.v 0.05 and 175 at 0.01 (the
sampled lobe's: 1.04 and 1.21). `tests/brdf_energy_bake.slang`'s `cs_bake` integrates
`ggx_single_scatter` on a 64 x 64 lattice with BeschleunigerBallett's furnace quadrature
(`GoldenMetrics.hpp`: 2048 CDF rows over the floored lobe, 128 azimuth columns);
`crates/webgpu_renderer/tests/brdf_energy.rs` fits the spline to it by least squares and fails
a stale one; `KATAGLYPHIS_UPDATE_BRDF_ENERGY=1` with that test refits it, then regenerate the
WGSL.

The fit against the quadrature at the lattice's cell centres, where a spline strays furthest
(measured 2026-11-21 with `brdf_energy.rs`): the albedo within 1.4 % from n.v 1e-4, 0.56 %
from 0.01, 0.13 % from 0.05 and 0.04 % from 0.2, the bias within 0.0098, 0.0047, 0.0012 and
0.0003; the largest misses sit below n.v 0.01, within 0.6 degrees of the horizon. A fit, not
a table, by measurement on aarch64 lavapipe: a 32 x 32 bilinear table matched the quadrature
about as well (2.0 % from n.v 1e-4, 0.21 % from 0.1), but Slang emits a `static const` array
as WGSL `var<private>` (SPIR-V: a constant stored into a function variable), which lavapipe
fills per invocation: a million invocations took 28 ms against 1.2 ms from a storage buffer,
a pipeline's first dispatch spent 6 s in the JIT, and the renderer's test binaries ran 2.3
to 2.9 times as long. The spline's cost: a million invocations evaluating it four times take
12 to 14 ms (the storage-buffer table 1.2 to 2.8, the private one 28 to 42), its module 7 to
9 ms to create (the private table 150) and its first dispatch 16 to 30 ms. Least-squares
polynomials on the same axes missed by 7 % with 55 coefficients a channel and 3.7 % with 78,
rational fits put poles in the domain, a low-rank spline needed rank 6 for 2 %, and Karis'
`env_brdf_approx`, which the analytic sky's ambient used before, misses this lobe by -26 %
to +52 %. `ibl_ambient` takes the same compensation, from the split sum's own BRDF LUT with
a baked environment and from `ggx_energy` without one.

A shading point evaluates the fit once: `brdf_smith_k`, `brdf_direct`, `brdf_sampled` and
`sample_bounce` take `energy`, `ggx_energy(dot(n, v), roughness)`, from their caller, the
value `brdf_smith_k` computed itself (`ggx_energy` clamps n.v and roughness as it floored
them), so `brdf.json` and `path_estimator.json` did not change. `shade_surface` hands it to
the sun, every punctual light and `ibl_ambient`, which branches on the environment flag, a
uniform, instead of sampling the environment and evaluating the analytic sky for every
fragment; the ray tracer evaluates it once per hit, the path tracer once per path vertex.
`tests/brdf.rs`' `the_energy_fit_runs_once_per_shading_point` counts its call sites in
`forward.wgsl` and `traced.wgsl`. On aarch64 lavapipe the light loop paid nothing extra before:
LLVM already hoisted the loop-invariant fit, so a 512 x 512 plane under 0 to 64 point lights
renders in the same GPU time (new against old within -2.3 % and +1.6 %). Pipelines compile about
2 % faster and the render-heavy test binaries ran 4 % faster (166.2 s against 173.3 s,
interleaved); the explicit hoist keeps the cost off compilers that do not move it (2026-11-22).

Instance vertex attributes contain matrix columns. `instance_matrix` transposes
the Slang row constructor so instances and equivalent node transforms agree.
`skinned_bounds::a_non_uniform_instance_scale_shades_like_the_same_node_scale`
checks this on the GPU and stays enabled.

## Vector kernels

`tests/<module>_test.slang` runs every public function of `common/<module>.slang`
once per case of `tests/vectors/<module>.json`: `brdf`, `noise`, `path_estimator`, `hdr_safe`,
`aces`, `fullscreen`, `camera_ray` and `ambient` today. `hdr_safe` takes its radiance as bits, so a case can hand it NaN and
the infinities, which JSON cannot spell; `fullscreen`'s uv differs per target, so its kernel writes
only what both targets share: the positions, x, each mapping's round trip through its inverse, and its
distance from `tests/clip_y.slang`'s own top row per target, so a swapped `__target_switch` branch fails;
`camera_ray`'s writes its pixel ray's difference from the ray through that file's NDC of the same pixel.
`tests/constants_test.slang` is the SPIR-V guard of the two constant-only modules. A file holds the inputs, the f64 references and each
output's tolerance: an f32 passes within `abs + rel * |ref|`, a u32 or i32 must
be equal. `tests/kernel_vectors.slang` reads the inputs and writes the outputs
one 32-bit word per component, in the file's field order, after a header of
input and output words per case. The references come from
`scripts/shader_model.py`, a Python twin of each module with every literal at
its f32 value, never from the kernel: `scripts/generate-shader-vectors.py`
evaluates each case in f64 and in emulated f32, stops on one an f32 kernel
cannot meet or whose branch sits within 1e-5 of its edge, and writes the files;
`--check`, in `pre-push-checks.sh`, fails a stale one. The brdf model reads
the generated `common/brdf_energy_fit.slang`'s coefficients and mirrors its
evaluation, so a refit stales `brdf.json` and `path_estimator.json`.
`crates/webgpu_renderer/tests/shader_vectors.rs` dispatches the committed WGSL
(`crates/webgpu_renderer/tests/shaders/`) on the test device and names every
component that misses; under `KATAGLYPHIS_REQUIRE_GPU=1` a missing adapter
fails it. BeschleunigerBallett checks the SPIR-V of the same kernels against
the same files.

The tolerances assume correctly rounded f32 `sin` and `cos`, as the model computes
them. Vulkan and WGSL promise only 2^-11 absolute error inside [-pi, pi], and
`sample_ggx_visible_normal` and `sample_bounce`'s diffuse lobe take both of an
angle that reaches 2 pi, so those outputs and `sample_bounce`'s weight could miss
on a conforming device with a coarse `sin`. They stay tight (owner decision
2026-11-21): loosen one only when a device that runs correct code fails it.

A new shared module adds the same four things: a kernel with a manifest row
(`spirv`, `wgsl`) and a `wgslMap` row into `crates/webgpu_renderer/tests/shaders/`;
its functions in `shader_model.py`; a `Module` with its cases and tolerances in
the generator; a `KERNELS` row in `shader_vectors.rs`, whose
`every_public_function_has_vectors` fails while a function has no output.

## Color space

Lighting and tonemapping operate in linear space. An sRGB render target encodes
the result in hardware; the web canvas needs the piecewise IEC 61966-2-1 encode
in `tonemap.slang` when `params.w` enables it. The headless regression
`non_srgb_target_is_gamma_encoded_like_an_srgb_one` compares both target formats.
BeschleunigerBallett retains the broader
[texture and swapchain audit](https://github.com/Kataglyphis/BeschleunigerBallett/blob/develop/docs/source/webgpu-srgb-audit.md).
