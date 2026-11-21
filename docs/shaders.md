# Shader sources and regeneration

`shaders/` owns the WebGPU Slang entry points and shared math. Its manifest
defines the entry points, WGSL destinations, depth-texture patches and minimum
compiler version. `crates/webgpu_renderer/src/shaders/`, the GUI's shader
directory and the renderer's `tests/shaders/` contain generated WGSL;
`histogram.wgsl` remains hand-written.

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

`sky/sky.slang` is the sky both renderers draw into their HDR target, tonemapped with
the scene. The C++ renderer compiles `vs_vulkan_main` and `fs_main` to SPIR-V and fills
`SkyUniforms` itself; its vertex entry takes `SV_VulkanVertexID`, because `SV_VertexID`
subtracts the base vertex through draw parameters a Raspberry Pi 5's v3dv lacks. The
combined `sky.wgsl` carries that entry too; the Rust renderer draws `vs_main`.

`fxaa/fxaa.slang`'s `fs_main` is the C++ renderer's FXAA too, behind its own
`SV_VulkanVertexID` fullscreen vertex, with the same three bindings and
`FxaaUniforms`; a change to either reaches its `FxaaStage` with the pin.

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

Instance vertex attributes contain matrix columns. `instance_matrix` transposes
the Slang row constructor so instances and equivalent node transforms agree.
`skinned_bounds::a_non_uniform_instance_scale_shades_like_the_same_node_scale`
checks this on the GPU and stays enabled.

## Vector kernels

`tests/<module>_test.slang` runs every public function of `common/<module>.slang`
once per case of `tests/vectors/<module>.json`: `brdf`, `noise` and
`path_estimator` today. A file holds the inputs, the f64 references and each
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
