# The compute-shader tracer core

*Origin and path convention: [the crate README](../README.md).*

The ray-traced and path-traced modes trace a BVH that the CPU builds and a compute kernel
walks (`shaders/tracer/bvh_traverse.slang`, emitted to WGSL by Slang). It runs wherever
wgpu runs, natively and in the browser; there is no hardware ray-query backend (owner
decision, 2026-11-15). This page is the core the modes build on: the build, the GPU data,
the limits and the kernel API. Code: `src/render/tracer/`.

| Module | Holds |
| --- | --- |
| `bvh` | Binned SAH builder, resumable in steps or parallel over subtrees; the 32-byte node |
| `geometry` | The scene as meshes and instances, the packed vertex attributes |
| `material` | The material table and which texture lands in which array layer |
| `layers` | Textures resampled into square, mipmapped array layers |
| `scene` | `TraceScene`: the flat arrays, the top level, the record layout |
| `build` | `TraceSceneBuilder`: stages, progress, cancellation |
| `job` | `Tracer` (the lazy build the renderer owns), `TraceStatus`, `RenderMode` |
| `gpu` | `TraceGpuScene`: upload and the bind group |
| `limits` | Buffer sizes against device limits; the limits the device requests |
| `query` | The traversal on the CPU, step for step as the shader runs it |
| `debug` | `TraceDebugPass`: traces a ray buffer on the GPU (`trace_debug.slang`), for tests |

## Build

`ForwardRenderer` owns a `Tracer`. `upload_scene` drops the traced scene; the app hands the
uploaded `CpuScene` over with `set_trace_source(Arc<CpuScene>)` (`StudioApp` does for each
scene it uploads), and the tracer drops it once the traced scene is built. The first frame with `render_mode` traced starts the build: natively on a
`tracer-build` thread, in the browser `job::WEB_STEP_BUDGET` work units per frame. The frame
rasters until the scene is uploaded. `trace_status()` (`StudioApp::trace_status`, the
Renderer tab) reports `Building { stage, fraction }`, `Ready(TraceStats)` or `Failed(why)`;
a new source cancels a running build at its next stage boundary. Tests and snapshots block
with `tracer_mut().finish_build(gpu, timeout)`. After the upload the tracer keeps the CPU
arrays (`cpu_scene()`, for CPU queries and top-level rebuilds) and the sRGB layers' first
level, which the CPU alpha test reads.

**Two levels.** Primitives with equal vertices, indices and material share one
object-space mesh, instanced once per copy (the Colosseum's 1112 repeated primitives, its
trees among them, become 15 meshes). So do primitives on an animated node, or with explicit instances
(`TraceSceneBuilder::new`'s `explicit`, as `set_instances`). Everything else merges into one
world-space mesh, skins and morph targets baked at their first pose; a mirroring transform
flips that mesh's winding and bitangent sign there. Zero-scale and degenerate geometry is
dropped. `set_root_transform` (the model transform) and `set_primitive_transforms` (animated
nodes' world matrices, by scene primitive) rebuild only the top level, and
`TraceGpuScene::write_instances` rewrites it in place; `Tracer` has both for the uploaded
scene.

**The BVH.** Binned SAH, 16 bins on all three axes, a strided sample of about 16 K boxes for
larger nodes; leaves of at most 4 triangles, the top level one instance per leaf. Depth is
capped at 64 (meshes) and 32 (top level), median splits taking over before the cap, so the
traversal stack never overflows. Natively a large mesh's top levels split on the build
thread and the subtrees below them on every core.

Measured on an 8-core Arm64 host under WSL, release, best of three:

| Scene | Triangles (stored), meshes, instances | Build |
| --- | --- | --- |
| Colosseum-web | 1.45 M (1.31 M), 16, 1113 | 0.31 s: gathering 0.07, BVHs 0.17, textures 0.06 |
| Colosseum, full | 6.02 M (5.87 M), 16, 1113 | 2.1 s: gathering 0.26, BVHs 1.4, textures 0.27, assembly 0.2 |

`cargo run --release -p kataglyphis_webgpu_renderer --example trace_build -- model.gltf`
prints the same breakdown. The crate itself is unoptimised in `cargo test` and in `cargo run`
without `--release`: 0.3 M triangles/s there, so the full Colosseum takes half a minute.

## GPU data

Group 1 (`TRACE_SCENE_GROUP`), compute-visible; a kernel keeps group 0 for itself.

| Binding | WGSL | Content |
| --- | --- | --- |
| 0 `tracerRecords` | `array<vec4<f32>>` | Mesh nodes, top-level nodes, instances, materials |
| 1 `tracerTriangles` | `array<vec4<u32>>` | Three global vertex indices; material in bits 0-23, MASK in bit 31; in leaf order |
| 2 `tracerPositions` | `array<vec4<f32>>` | Vertex position, xyz |
| 3 `tracerVertices` | `array<vec4<u32>>` | Two per vertex: oct normal (snorm16 x2), oct tangent (snorm16 x2, bitangent sign in bit 16), UV0 (f32 x2); UV1 (f32 x2), colour (unorm16 x4) |
| 4 `tracerInfo` | uniform | `tlas_root`, `instance_base`, `material_base`, counts, both layer sides |
| 5, 6 | `texture_2d_array<f32>` | sRGB layers (base colour, emissive) and linear layers (metallic-roughness, normal, occlusion); square, power of two, full mips |
| 7, 8 | `sampler` | Repeat, linear and nearest |

Records, in float4 units: a node is 2 (`min.xyz, first; max.xyz, count`; a leaf has
`count > 0` and indexes triangles or one instance, an inner node's children are `first` and
`first + 1`). An instance is 7: three rows of world-to-object, three of object-to-world,
then `blas_root, flags (bit 0 mirrored), primitive, mesh`. A material is 14: base colour;
emissive rgb and MASK cutoff (-1 when not MASK); metallic, roughness, occlusion strength,
normal scale; flags and layers; two `KHR_texture_transform` rows per slot. Material flags:
bits 0-1 alpha mode (0 opaque, 1 MASK, 2 BLEND), bit 2 double-sided, bit 3 unlit, bits 4-8
the slots on TEXCOORD_1, from bit 9 four bits per slot for the U and V wrap (0 repeat,
1 mirror, 2 clamp). Layers: base | metallic-roughness << 16, normal | emissive << 16,
occlusion | nearest-filter mask << 16; `0xFFFF` means the factor alone. The untextured
metallic factor is kept, unlike the C++ path tracer's texture-only branch.

Per stored triangle about 16 bytes, per vertex 48, per node 32 (about one node per
triangle); textures at most 1024 texels natively and 512 in the browser, each array within
512 MiB (128 MiB in the browser). Compressed (KTX2) textures and those past 256 layers per
array render with their factor alone.

## Limits

The scene binds 4 storage buffers, so a kernel keeps 4 of WebGPU's default 8 per stage,
plus 2 sampled textures and 2 samplers. In the browser the device has WebGPU's defaults:
128 MiB per storage binding, room for about 4 M vertices. Natively `GpuContext` requests the
adapter's `max_storage_buffer_binding_size` and `max_buffer_size` and every other limit at
the web's default, so native tests still fail where the browser would. `TraceGpuScene::upload`
checks every buffer and array first and fails with the buffer's name and size
(`TraceLimitError`), which `TraceStatus::Failed` shows. The full Colosseum needs 175 MiB of
records and 143 MiB of vertices: desktop GPUs bind that, lavapipe (128 MiB) does not, and
the browser takes Colosseum-web.

## Kernel API

```slang
import bvh_traverse;

TraceRay ray;  // origin, tmin, direction (any length; t is in its units), tmax
TraceHit hit = trace_closest(ray);          // hit.instance == TRACE_NO_HIT on a miss
bool blocked = trace_any(shadow_ray);       // ends at the first accepted hit
TraceSurface s = trace_surface(ray, hit);   // world position, normals, tangent, UVs, colour, material, front_face
TraceMaterial m = load_material(s.material);
float4 base = sample_material_slot(s.material, m, SLOT_BASE_COLOR, s.uv0, s.uv1, 0.0);
```

Triangles are two-sided; hits are accepted in `[tmin, tmax]`, ties go to the lower
instance and then triangle, so every visiting order returns the same hit. MASK materials
are alpha-tested in both queries at level 0: base colour alpha times texture alpha times
vertex alpha against the cutoff, as the forward pass discards. BLEND traces as opaque, as in
the C++ engine. The kernel uses `textureSampleLevel` only and no barrier, so Tint's
uniformity rules have nothing to reject. On the Rust side:

```rust
let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
    label: Some("my_kernel"),
    bind_group_layouts: &[Some(&my_group0), Some(&TraceGpuScene::bind_group_layout(device))],
    immediate_size: 0,
});
// Each frame, once renderer.tracer().gpu_scene() is Some(scene):
pass.set_bind_group(TRACE_SCENE_GROUP, scene.bind_group(), &[]);
```

`TraceDebugPass` is a complete example: `src/render/tracer/debug.rs` and
`shaders/tracer/trace_debug.slang`.

## Tests

`src/render/tracer/tests/`: closest and any hits equal brute force bit for bit on random
rays over seven scenes (soup, slivers, a height field's shared edges, 30 instanced cubes
with a mirrored one, MASK cards, two glTF assets); the GPU kernel equals the CPU traversal
in triangle and within 1e-4 in distance on every ray rounding cannot flip; the alpha test
on both; decoded surfaces; a stepped build equals a threaded one; the root transform; a
build-time budget. `tests/tracer.rs` pins the Slang constants and bindings and the lazy
build through `ForwardRenderer`; `limits.rs` the browser and native limits.
