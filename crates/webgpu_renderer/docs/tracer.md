# The compute-shader tracer core

*Origin and path convention: [the crate README](../README.md).*

The ray-traced and path-traced modes trace a BVH that the CPU builds and a compute kernel
walks (`shaders/tracer/bvh_traverse.slang`, emitted to WGSL by Slang). It runs wherever
wgpu runs, natively and in the browser; there is no hardware ray-query backend (owner
decision, 2026-11-15). This page is the core the modes build on (the build, the GPU data,
the limits and the kernel API) and then [the modes](#the-modes). Code: `src/render/tracer/`.

| Module | Holds |
| --- | --- |
| `bvh` | Binned SAH builder, resumable in steps or parallel over subtrees, and the refit; the 32-byte node |
| `geometry` | The scene as meshes and instances, the packed vertex attributes |
| `deform` | Animated skins and morphs as meshes of their own, re-posed on the CPU |
| `material` | The material table and which texture lands in which array layer |
| `layers` | Textures resampled into square, mipmapped array layers |
| `scene` | `TraceScene`: the flat arrays, the top level, the record layout |
| `build` | `TraceSceneBuilder`: stages, progress, cancellation |
| `job` | `Tracer` (the lazy build the renderer owns), `TraceStatus`, `RenderMode` |
| `gpu` | `TraceGpuScene`: the buffers, the bind group and the writes after the upload |
| `upload` | `TraceUpload`: the first upload, a byte budget per frame |
| `limits` | Buffer sizes against device limits; the limits the device requests |
| `query` | The traversal on the CPU, step for step as the shader runs it |
| `debug` | `TraceDebugPass`: traces a ray buffer on the GPU (`trace_debug.slang`), for tests |
| `modes` | The ray- and path-traced modes: settings, accumulation, dispatch plan, the kernels and the resolve |

## Build

`ForwardRenderer` owns a `Tracer`. `upload_scene` drops the traced scene; the app hands the
uploaded `CpuScene` over with `set_trace_source(Arc<CpuScene>)` (`StudioApp` does for each
scene it uploads), and the tracer drops it once the traced scene is built. The first frame with `render_mode` traced starts the build: natively on a
`tracer-build` thread, in the browser `job::WEB_STEP_BUDGET` work units per frame. The frame
rasters until the scene is uploaded. The finished scene uploads over frames (`TraceUpload`):
every buffer and texture array is made at once, then filled at most
`TraceBuildOptions::upload_bytes_per_frame` a frame (`UPLOAD_BYTES_PER_FRAME`, 64 MiB, so the
full Colosseum's half gigabyte takes about eight frames instead of one hitch), in pieces of at
most 64 K entries or one mip level. `trace_status()` (`StudioApp::trace_status`, the
Renderer tab) reports `Building { stage, fraction }` (the stage `Uploading` meanwhile),
`Ready(TraceStats)` or `Failed(why)`; a new source cancels a running build at its next stage
boundary, and `set_instances` during the build or the upload starts it over. Tests and snapshots
block with `tracer_mut().finish_build(gpu, timeout)`, which uploads in one go. After the upload
the tracer keeps the nodes, instances and materials, which a new root or instance count rebuilds
the top level from, and drops the triangles, positions, packed vertices and texels
(`TraceScene::release_geometry`): for the full Colosseum about 175 MiB of nodes stay of the about
480 MiB of arrays. A scene with a deformed mesh keeps them, since a pose re-skins and refits from
them, and `TraceBuildOptions::keep_cpu_geometry` keeps them (and the sRGB layers' first level, which
the CPU alpha test reads) for CPU queries of the uploaded scene (`cpu_scene()`).

**Two levels.** Primitives with equal vertices, indices and material share one
object-space mesh, instanced once per copy (the Colosseum's 1112 repeated primitives, its
trees among them, become 15 meshes). So do primitives on an animated node, or with explicit instances
(`TraceSceneBuilder::new`'s `explicit`, as `set_instances`). A skin whose joints an animation
moves, or a morphed primitive whose weights one drives, gets a mesh of its own (`deform`), a
skin in world space under an identity instance, a morph in object space under its node's.
Everything else merges into one world-space mesh, skins and morph targets no animation moves
baked at their first pose; a mirroring transform flips that mesh's winding and bitangent sign
there. Zero-scale and degenerate geometry is dropped. `set_root_transform` (the model transform) and `set_primitive_transforms` (animated
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
| 0 `tracerRecords` | `array<vec4<f32>>` | Each node's first half (mesh nodes, then top-level ones), instances, materials |
| 1 `tracerTriangles` | `array<vec4<u32>>` | Three global vertex indices; material in bits 0-23, MASK in bit 31; in leaf order |
| 2 `tracerPositions` | `array<vec4<f32>>` | Vertex position, xyz |
| 3 `tracerVertices` | `array<vec4<u32>>` | Oct normal (snorm16 x2), oct tangent (snorm16 x2, bitangent sign in bit 16), UV0 (f32 x2) |
| 4 `tracerInfo` | uniform | `tlas_root`, `instance_base`, `material_base`, counts, both layer sides |
| 5, 6 | `texture_2d_array<f32>` | sRGB layers (base colour, emissive) and linear layers (metallic-roughness, normal, occlusion); square, power of two, full mips |
| 7, 8 | `sampler` | Repeat, linear and nearest |
| 9 `tracerNodeMax` | `array<vec4<f32>>` | Each node's second half, by node index |
| 10 `tracerVertexExtras` | `array<vec4<u32>>` | UV1 (f32 x2), colour (unorm16 x4), by vertex index |

A node and a packed vertex are 32 bytes, each split into two 16-byte halves in two bindings,
so no binding outgrows 128 MiB before the triangles do ([Limits](#limits)). Records, in float4
units: a node's first half is 1 (`min.xyz, first`; its `max.xyz, count` sits at the same index of
`tracerNodeMax`; a leaf has `count > 0` and indexes triangles or one instance, an inner node's
children are `first` and `first + 1`), so `instance_base` is the node count. An instance is 7: three rows of world-to-object, three of object-to-world,
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

The scene binds 6 storage buffers, so a kernel keeps 2 of WebGPU's default 8 per stage,
plus 2 sampled textures and 2 samplers. The modes' kernels use all 8: the scene's 6, the
accumulation and the punctual lights; 3 sampled textures (the environment cube is the third) and
the scene's 2 samplers. A further kernel buffer has to share one of those. Every binding holds
16 bytes an entry, so in the browser, at WebGPU's 128 MiB per storage binding, a scene holds
about 8.4 M stored triangles, vertices and nodes, and 1.1 M instances. Natively `GpuContext`
requests the adapter's `max_storage_buffer_binding_size` and `max_buffer_size` and every other
limit at the web's default, so native tests still fail where the browser would.
`TraceGpuScene::upload` checks every buffer and array first and fails with the buffer's name and
size (`TraceLimitError`), which `TraceStatus::Failed` shows. The full Colosseum (5.74 M nodes,
5.87 M stored triangles, 4.68 M vertices) needs 90 MiB of triangles, 88 MiB for each node half and
71 MiB for each vertex half: lavapipe (128 MiB) traces it since 2026-11-16, where a node or a
vertex whole in one binding needed 175 and 143 MiB. Uploads write a split buffer 64 K entries at
a time, so the CPU arrays are not copied whole once more; the same pieces spread the first upload
over frames.

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
build-time budget; a scene behind 4.3 M padding nodes and vertices, which whole in one binding
would pass 128 MiB each, traces and decodes on the GPU as on the CPU (it fails when an upload
writes every chunk at the first one's offset). `tests/tracer.rs` pins the Slang constants and
bindings and the lazy build through `ForwardRenderer`, and traces the full Colosseum on request
([testing.md](../../../docs/testing.md#the-gpu-tests-must-render)); `limits.rs` the browser and
native limits, the Colosseum's counts among them. A bent skin and an animated morph are re-posed and
refitted (every node bounds its children) and trace as brute force does; a skin no animation moves
merges.

## The modes

`ForwardRenderer::render_mode` picks Raster, RayTraced or PathTraced; `drawn_mode()` says what
a frame draws: the pick once the traced scene is on the GPU, raster until then, on whichever
`raster_path` is set. An output whose 16-byte-per-pixel accumulation passes the device's storage
binding or buffer limit rasters too, and `traced_fallback()` says why (the chip reads
`accumulation too large, rastering`), so SSAO, the profiler and the outliner follow the frame drawn.
A traced frame runs its kernel (`shaders/tracer/traced.slang`) over
256 x 256 tiles into a per-pixel accumulation buffer, then `trace_resolve.slang` copies that into
the HDR target, so exposure, bloom, the clouds and the tonemap run as after raster. The shadow
cascades, either raster path (the forward pass and depth resolve, or the G-buffer and deferred
lighting), occlusion culling and SSAO do not run (the tonemap's SSAO weight is 0), and the
primitive, frustum-culling and shadow-caster counts read 0 / 0, as in the C++ engine's traced
frames. The profiler lists the kernel, the resolve and the post passes in record order; a switch of
mode or raster path drops the rows of passes the new frames do not record at once. Camera rays run
from the near to the far plane through `inverse(view_proj)`: the raster camera, the docked
viewport's lens shift included. Code: `src/render/tracer/modes/`, `src/render/forward/traced.rs`;
shading shared by both kernels: `trace_shading.slang`; the estimator: `shaders/common/path_estimator.slang`,
which the C++ `path_tracing.slang` imports from its pin bump on ([`docs/shaders.md`](../../../docs/shaders.md)).

**Ray traced**, the C++ mode: one ray through each pixel centre; base colour times texture
times vertex colour, the metallic-roughness and normal maps at level 0, the forward pass's
`brdf_direct` (GGX and Lambert) lit by the sun through one shadow ray (any-hit, MASK
alpha-tested), plus emission. An unlit material returns its albedo. No ambient term and no
punctual lights, as the C++ engine has neither; the path tracer has both. A miss is left to the sky pass, drawn in the
resolve's render pass against the traced depth: the cube map, the C++ navy with the skybox
switched off, or the procedural sky and its sun disk without a cube map, as raster draws it
(the C++ ray-traced mode shows its skybox).

**Path traced**, the estimator of BeschleunigerBallett's `path_tracing.slang`: `samples_per_pixel`
(1-64, default 8) paths a pixel and frame of at most `max_bounces` (1-16, default 8) segments; a
primary ray jittered inside the pixel; NEE to the sun and to one punctual light on every segment
through the same BRDF; a
bounce that samples the GGX lobe (visible normals) or the cosine lobe by the lobe's Fresnel share
and divides by the mixture's density, its BRDF `brdf_sampled` (Epic's Smith remap for sampled light,
`k = alpha / 2`; NEE keeps `brdf_direct`'s analytic `(r + 1)^2 / 8`, which over-masks a sampled lobe:
a white metal at roughness 0.3 kept 0.668 of the sky at 60 degrees, now 0.901, the exact Smith lobe 0.975); Russian roulette from the fourth segment, survival in
0.05-0.95; an unlit surface ends its path at its albedo; emission is added before the surface
absorbs. Metallic is the factor times the texture, so an untextured factor counts (the C++
kernel once applied it with a texture only). A camera ray that misses sees the procedural sky, sun
disk included. A bounce that misses gets the sky's light as the raster's ambient term does: the
gradient times the ambient strength (`light_dir_ambient.w`) and the environment's intensity,
without the disk, which NEE brings; with an environment loaded (`set_environment`), the
environment's radiance cube at mip 0 in place of the gradient, as the raster's IBL ambient takes it.
So the path tracer lights a scene as raster does, plus the bounces and the sky's occlusion. The
environment is sampled only where a bounce happens to miss, with no importance sampling, so a small
bright sun in an `.hdr` converges slowly.

**Punctual lights.** Each segment picks one of the raster's `KHR_lights_punctual` lights
uniformly, evaluates it as the raster does (`shaders/common/punctual_light.slang`, shared with
`forward_lighting.slang`: inverse square, the KHR range window, the spot cone's smoothstep;
directional lights unattenuated) and divides by the pick's chance, 1 / count, through a shadow ray
that stops just short of the light. So a punctual light casts a traced shadow, as the raster's
first four point and spot lights cast theirs through depth cubes. The kernels read the raster's light buffer itself, at
`TRACE_LIGHTS_GROUP` (2): binding 0 the lights in `render::lights`' four rows, binding 1 the
environment cube (the IBL's 1x1 stand-in without one); `TraceFrame::lights` carries the count and
whether an environment is bound.

**Accumulation.** One running mean a pixel in `array<vec4<f32>>`, 16 bytes a pixel: WebGPU has
no read-write `rgba32f` storage texture. It starts over when `AccumulationKey` changes (the
view-projection, eye and output size; the sun's direction, colour and intensity; the sky's light;
the settings; the tracer's generation, which a build and every instance move bump; a hash of the
packed punctual lights, so a moved, animated or recoloured light counts; the environment's
generation, which every `set_environment` and `clear_environment` bump), on `reset_accumulation()`,
after a shader reload, and after any frame that is not path traced. A material changes only with
a new scene, which rebuilds the traced one. The RNG stream of a dispatch is its pixel index plus
the samples accumulated before it times 2654435769, the C++ seed with samples for frames.

**Depth and clouds.** Both kernels write the primary hit's NDC depth, through the raster's
view-projection, into the accumulation's alpha, 1 where nothing is hit: the ray tracer its one
ray's hit, the path tracer an unjittered ray through the pixel centre on each frame's first sample
chunk (`path_tracing.slang`'s `primary_hit_depth`). The resolve writes it into the single-sample
depth target (`frag_depth`, compare always), which the clouds march to as they march to either
raster path's depth: the C++ engine's `cloudDepthSourceForThisFrame` and `bindCloudSceneDepth`.
It rides the accumulation, so no storage binding is added to the limits above. On a simple scene
it matches the deferred depth within 4.2e-7 (`tests/feature_matrix.rs`).

**White furnace.** `TraceSettings::furnace` (sky radiance, metallic, roughness) makes every
surface white with that metallic and roughness, without emission or sun, under a uniform sky. A
pixel converges to the radiance times the surface's directional albedo E(mu): 0.99-1.04 for the
default white dielectric (roughness 0.5; above 1 at grazing angles, where Epic's diffuse takes what
the Fresnel leaves however little the masking takes), less for a metal. The C++ kernel keeps NEE in its
furnace, whose test zeroes the light instead.

**Dispatch budget.** A frame's samples are split into chunks so a dispatch traces at most
`DISPATCH_SEGMENTS` (4 M) path segments over one tile: milliseconds on a desktop GPU, far inside a
browser's GPU watchdog. Each dispatch binds its tile and samples at a dynamic uniform offset.
Each kernel compiles the first time its mode draws: SwiftShader, which backs WebGPU in a
GPU-less Chrome, takes about two minutes for the path-tracing kernel and three seconds for the
ray-tracing one; lavapipe takes about a second. SwiftShader inlines a whole traversal at every call site, so the
path tracer keeps one any-hit call: `next_event` traces the sun's and the punctual light's shadow
rays from one loop. With a call each, the web demo's path-traced frame did not arrive within
the browser test's 300 s; with the loop it took 117 s, against 124 before the punctual lights.

**Animation and placement.** The renderer moves the traced instances of rigid animated nodes to
the raster's poses each frame (`set_primitive_transforms`), which restarts the mean. Its deformed
meshes follow the raster's skins and morphs (`Tracer::pose_deformed`, from the animation's node
matrices and each primitive's morph weights): a mesh whose joint matrices, node or weights moved
is skinned and blended again on the CPU as `forward.slang` poses it, its BVH refitted bottom-up in
place (`bvh::refit`; the tree keeps its shape, so a large bend makes it slower, never wrong), and
only its vertices and nodes and the top level are written to the GPU. The model transform is the tracer's root
(`Tracer::set_root_transform` on every change); the tracer keeps it, so a transform set before
the build applies to the built scene, and an upload resets it with the renderer's.
`ForwardRenderer::set_instances` reaches `Tracer::set_instances`: a build takes the transforms as
explicit instances (a running one starts over). A built scene re-instances an instanced primitive
from its template (`TraceScene::templates`, one per instanced primitive, moved with its node) and
rebuilds the top level (`set_instance_extras`): as many instances as before are rewritten in place,
a new count (`InstanceChange::Recounted`) gets new records and node-maxima buffers at the new
layout, with the mesh nodes and materials written again from the kept CPU nodes, a new scene uniform and
bind group; the vertex and triangle buffers and the texture arrays stay. A primitive the build
merged into the world-space mesh has no instances to change, so it keeps its place until the next
upload: `Tracer::instances_current()` turns false and the tracer warns once for it. A count whose
records pass the device's binding limit fails the traced scene (`TraceStatus::Failed`).

**Shader reload.** F5's `ForwardRenderer::reload_shaders` builds both kernels, the resolve and
the ray tracer's one-sample sky from `ShaderSources::traced`, `trace_resolve` and `sky` inside its
error scope, though they otherwise compile on their mode's first frame: a module the device
refuses keeps the old pipelines, an accepted one replaces them and restarts the mean.

**On purpose unlike the C++ engine:** a path-traced bounce that misses gets the renderer's sky
gradient, or the loaded environment, times the ambient strength and the environment's intensity
(C++: an RTIOW gradient at half intensity, whose raster has no ambient term to match; owner
decision 2026-11-16: the C++ path tracer follows this one, tracked in BeschleunigerBallett); the path
tracer lights with the scene's punctual lights (C++: neither of its traced modes has any, so the
ray-traced mode keeps none here); and a path-traced camera miss sees
that procedural sky with its sun disk, not the cube map; offsets and side tests take the
triangle's face normal (C++: the interpolated one); camera rays clip to the near and far planes as raster does (C++: 0.001-10000
from the eye, which bounces keep). The whole frame is traced, docks included, as in C++.

Measured on lavapipe, the 8-core arm64 host under WSL, shared with other builds:

| Scene, output | Mode | A frame |
| --- | --- | --- |
| `cube_on_plane`, 1600 x 900 | ray traced | 0.27-0.36 s on the GPU (`RayTrace`) |
| `cube_on_plane`, 800 x 600 | path traced, 8 spp, 8 bounces | 0.38 s on the GPU (`PathTrace`) |
| Colosseum-web, 1.45 M triangles, 960 x 540 | ray traced | 0.47 s |
| Colosseum-web, 960 x 540 | path traced, 4 spp, 4 bounces | 5.2 s |

The first frame of a mode adds its kernel's compile: 1-2 s for either on lavapipe.

Tests: `tests/traced_modes.rs` (lavapipe; the C++ goldens' ports): the white furnace through the
post chain on the C++ shadow rig (the HDR mean within the C++ 0.98..1.002 of the sky, measured
0.994, and 98 % of pixels within 0.1; a 1.5 % gain a bounce reads 1.008; the 8-bit mean the C++
231.6 within 3 levels), a white metal card against a quadrature of `brdf_sampled` (within 0.03;
measured within 0.002) and closer to the exact Smith lobe's than the analytic remap's, and a white dielectric one at roughness 0.5 and 0.25, mu 1 and 0.5 (within
the C++ 0.01; measured within 0.0022, the 1.5 % gain off by 0.014), an output whose accumulation
does not fit drawing and reporting raster, accumulation that settles, every reset premise, a MASK card's half shadow in both modes,
the untextured metallic factor, unlit albedo in every mode, the traced shadow against the
cascades' (IoU over 0.75; measured 0.98) and black, the lens shift, the raster round trip, and
that no raster pass runs or is timed, that a shader reload rebuilds both kernels or keeps them, and a
skinned bar bending in the traced frame as in raster (14.5 px against 14.6; without the re-pose or
the refit it fails) while a bend restarts the mean.
`tests/traced_lights.rs` (lavapipe): two ranged point lights over a ground plane lit where raster
lights it (IoU of the pixels over a tenth of the peak above 0.9; measured 0.976), nothing past their
range windows, and the centre within 3 % of the analytic radiance (measured 0.5 % under; raster
within 1 %, measured 0.1 %); a spot light's cone where raster cuts it (IoU 1.0), dark past the outer
cone and lit inside the inner one; a wall between a point light and the ground leaving the ground
behind it black, as raster's depth cube does (unshadowed raster lights it), while the ground beside
it matches raster within 2 %; a white card under a constant coloured environment converging to its colour times E(1),
0.994 in each channel. Dropping the 1/count halves the centre (0.390 against 0.784), dropping the
shadow ray lights the wall's shadow (0.096 against 0.097 raster), and the gradient in place of the
environment turns the card blue (0.36, 0.80, 1.99 of the environment): each fails its test.
`tests/feature_matrix.rs` runs the traced modes over the deferred path beside both raster paths:
the clouds stop at a wall in front (0 wall pixels change in either mode while 10743 and 10748 of
13332 sky pixels do), the traced depth against the raster one, the cube map, navy and procedural
sky on a ray-traced miss, the model transform and `set_instances` moving and adding a traced cube as raster
moves it, the passes each mode times, and raster frames bit-identical after a trip through both
modes. The modes' pure parts are unit-tested in place; the Slang
uniforms and workgroup size are pinned in `tests/tracer.rs`. Headless Chrome 157 on SwiftShader
draws both traced modes, the deferred path and the clouds in the web demo and reads each frame back
(`scripts/linux/webgpu-demo-browser.js`, [testing.md](../../../docs/testing.md#the-web-demo-in-a-browser)).
