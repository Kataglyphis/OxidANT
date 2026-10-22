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

Instance vertex attributes contain matrix columns. `instance_matrix` transposes
the Slang row constructor so instances and equivalent node transforms agree.
`skinned_bounds::a_non_uniform_instance_scale_shades_like_the_same_node_scale`
checks this on the GPU and stays enabled.
