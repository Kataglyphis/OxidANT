# Shader sources and regeneration

`shaders/` owns the WebGPU Slang entry points and shared math. Its manifest
defines the entry points, WGSL destinations, depth-texture patches and minimum
compiler version. `crates/webgpu_renderer/src/shaders/` and the GUI's shader
directory contain generated WGSL; `histogram.wgsl` remains hand-written.

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

`common/volumetric_clouds.slang` is the volumetric clouds' mathematics for both
renderers: the noise texels, the unpack of the C++ `SceneUBO` cloud block, the
box, the density and the march. Its functions take an `ICloudNoise`, which
Slang specializes per caller, so each renderer binds the noise volume its own
way: `clouds/cloud_march.slang` a texture and a sampler, the C++ `clouds_main` a
combined `Sampler3D`. `march_clouds` takes the sky's ambient as a `CloudAmbient`,
which the Rust host fills from the sky it draws; the seven-argument overload the
C++ engine calls lights by the procedural sky's closed-form means. The block's
`cloudLightMarch.yzw` is the wind's offset of the noise, which the C++ engine
packs as zero, so its clouds stand still.
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
rays around it. `tests/path_estimator_test.slang` emits it to SPIR-V and WGSL.

Instance vertex attributes contain matrix columns. `instance_matrix` transposes
the Slang row constructor so instances and equivalent node transforms agree.
`skinned_bounds::a_non_uniform_instance_scale_shades_like_the_same_node_scale`
checks this on the GPU and stays enabled.

## Color space

Lighting and tonemapping operate in linear space. An sRGB render target encodes
the result in hardware; the web canvas needs the piecewise IEC 61966-2-1 encode
in `tonemap.slang` when `params.w` enables it. The headless regression
`non_srgb_target_is_gamma_encoded_like_an_srgb_one` compares both target formats.
BeschleunigerBallett retains the broader
[texture and swapchain audit](https://github.com/Kataglyphis/BeschleunigerBallett/blob/develop/docs/source/webgpu-srgb-audit.md).
