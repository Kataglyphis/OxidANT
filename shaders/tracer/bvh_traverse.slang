// The tracer's scene and its two-level BVH traversal for compute kernels; render::tracer documents the layout.

// render::tracer::query::STACK_SIZE: a top-level path (32) and a bottom-level path (64) below it.
static const uint TRACE_STACK_SIZE = 96;
static const uint INSTANCE_STRIDE = 7;
static const uint MATERIAL_STRIDE = 14;
static const uint TRACE_NO_HIT = 0xFFFFFFFFu;
static const uint TRACE_NO_LAYER = 0xFFFFu;
static const uint TRIANGLE_MASKED = 0x80000000u;
static const uint TRIANGLE_MATERIAL = 0x00FFFFFFu;
static const uint INSTANCE_MIRRORED = 1u;
static const float TRACE_FAR = 3.0e38;
// PBRT's 1 + 2 gamma(3): rounding must not miss a box a triangle lies flat in.
static const float TRACE_EXIT_SCALE = 1.00000036;

static const uint SLOT_BASE_COLOR = 0u;
static const uint SLOT_METALLIC_ROUGHNESS = 1u;
static const uint SLOT_NORMAL = 2u;
static const uint SLOT_EMISSIVE = 3u;
static const uint SLOT_OCCLUSION = 4u;
static const uint ALPHA_MODE_MASK = 3u;
static const uint ALPHA_MASK = 1u;
static const uint MATERIAL_DOUBLE_SIDED = 4u;
static const uint MATERIAL_UNLIT = 8u;
static const uint MATERIAL_UV1_SHIFT = 4u;
static const uint MATERIAL_WRAP_SHIFT = 9u;
static const uint WRAP_MIRROR = 1u;
static const uint WRAP_CLAMP = 2u;

struct TraceSceneInfo
{
    uint tlas_root;
    uint instance_base;
    uint material_base;
    uint instance_count;
    uint material_count;
    uint srgb_layer_size;
    uint linear_layer_size;
    uint flags;
};

// A node's max half and a vertex's UV1 and colour bind apart, so no binding passes WebGPU's 128 MiB on a large scene.
[vk::binding(0, 1)] StructuredBuffer<float4> tracerRecords;
[vk::binding(1, 1)] StructuredBuffer<uint4> tracerTriangles;
[vk::binding(2, 1)] StructuredBuffer<float4> tracerPositions;
[vk::binding(3, 1)] StructuredBuffer<uint4> tracerVertices;
[vk::binding(4, 1)] ConstantBuffer<TraceSceneInfo> tracerInfo;
[vk::binding(5, 1)] Texture2DArray<float4> tracerSrgbLayers;
[vk::binding(6, 1)] Texture2DArray<float4> tracerLinearLayers;
[vk::binding(7, 1)] SamplerState tracerLinearSampler;
[vk::binding(8, 1)] SamplerState tracerNearestSampler;
[vk::binding(9, 1)] StructuredBuffer<float4> tracerNodeMax;
[vk::binding(10, 1)] StructuredBuffer<uint4> tracerVertexExtras;

struct TraceRay
{
    float3 origin;
    float tmin;
    float3 direction;
    float tmax;
};

// instance is TRACE_NO_HIT on a miss; barycentrics weigh vertices 1 and 2.
struct TraceHit
{
    float t;
    uint instance;
    uint triangle;
    float2 barycentrics;
};

struct BvhNode
{
    float3 lo;
    uint first;
    float3 hi;
    uint count;
};

struct TraceInstance
{
    float4 world_to_object[3];
    float4 object_to_world[3];
    uint blas_root;
    uint flags;
    uint primitive;
    uint mesh;
};

struct TraceMaterial
{
    float4 base_color;
    float3 emissive;
    float alpha_cutoff;
    float metallic;
    float roughness;
    float occlusion_strength;
    float normal_scale;
    uint flags;
    uint3 layers;
};

struct TraceVertex
{
    float3 normal;
    float4 tangent;
    float2 uv0;
    float2 uv1;
    float4 color;
};

struct TraceSurface
{
    float3 position;
    // World space, unit length, as the triangle winds; front_face says which side the ray came from.
    float3 geometric_normal;
    float3 normal;
    float4 tangent;
    float2 uv0;
    float2 uv1;
    float4 color;
    uint material;
    bool front_face;
};

BvhNode load_node(uint index)
{
    float4 a = tracerRecords[index];
    float4 b = tracerNodeMax[index];
    BvhNode node;
    node.lo = a.xyz;
    node.first = asuint(a.w);
    node.hi = b.xyz;
    node.count = asuint(b.w);
    return node;
}

TraceInstance load_instance(uint index)
{
    uint base = tracerInfo.instance_base + index * INSTANCE_STRIDE;
    TraceInstance inst;
    // Constant indices: FXC cannot address a struct's array by a loop counter and then fails to unroll naga's bounded loop.
    inst.world_to_object[0] = tracerRecords[base];
    inst.world_to_object[1] = tracerRecords[base + 1u];
    inst.world_to_object[2] = tracerRecords[base + 2u];
    inst.object_to_world[0] = tracerRecords[base + 3u];
    inst.object_to_world[1] = tracerRecords[base + 4u];
    inst.object_to_world[2] = tracerRecords[base + 5u];
    uint4 tail = asuint(tracerRecords[base + 6u]);
    inst.blas_root = tail.x;
    inst.flags = tail.y;
    inst.primitive = tail.z;
    inst.mesh = tail.w;
    return inst;
}

TraceMaterial load_material(uint index)
{
    uint base = tracerInfo.material_base + index * MATERIAL_STRIDE;
    float4 emissive = tracerRecords[base + 1u];
    float4 factors = tracerRecords[base + 2u];
    uint4 words = asuint(tracerRecords[base + 3u]);
    TraceMaterial m;
    m.base_color = tracerRecords[base];
    m.emissive = emissive.xyz;
    m.alpha_cutoff = emissive.w;
    m.metallic = factors.x;
    m.roughness = factors.y;
    m.occlusion_strength = factors.z;
    m.normal_scale = factors.w;
    m.flags = words.x;
    m.layers = words.yzw;
    return m;
}

float2 unpack_snorm2(uint bits)
{
    return max(float2(float(int(bits << 16u) >> 16u), float(int(bits) >> 16u)) / 32767.0, float2(-1.0, -1.0));
}

float2 unpack_unorm2(uint bits)
{
    return float2(float(bits & 0xFFFFu), float(bits >> 16u)) / 65535.0;
}

float3 oct_decode(float2 e)
{
    float3 v = float3(e.x, e.y, 1.0 - abs(e.x) - abs(e.y));
    float t = max(-v.z, 0.0);
    v.x += v.x >= 0.0 ? -t : t;
    v.y += v.y >= 0.0 ? -t : t;
    return normalize(v);
}

// geometry::pack_vertex: the bitangent sign rides in bit 16 of the tangent word.
TraceVertex load_vertex(uint index)
{
    uint4 a = tracerVertices[index];
    uint4 b = tracerVertexExtras[index];
    TraceVertex v;
    v.normal = oct_decode(unpack_snorm2(a.x));
    float handedness = (a.y & 0x10000u) != 0u ? -1.0 : 1.0;
    v.tangent = float4(oct_decode(unpack_snorm2(a.y & ~0x10000u)), handedness);
    v.uv0 = asfloat(a.zw);
    v.uv1 = asfloat(b.xy);
    v.color = float4(unpack_unorm2(b.z), unpack_unorm2(b.w));
    return v;
}

float3 trace_position(uint vertex)
{
    return tracerPositions[vertex].xyz;
}

float3 transform_point(float4 rows[3], float3 p)
{
    float4 h = float4(p, 1.0);
    return float3(dot(rows[0], h), dot(rows[1], h), dot(rows[2], h));
}

float3 transform_vector(float4 rows[3], float3 v)
{
    return float3(dot(rows[0].xyz, v), dot(rows[1].xyz, v), dot(rows[2].xyz, v));
}

// The inverse transpose as the transpose of the world-to-object rows: normals stay perpendicular under any scale.
float3 transform_normal(float4 world_to_object[3], float3 n)
{
    return world_to_object[0].xyz * n.x + world_to_object[1].xyz * n.y + world_to_object[2].xyz * n.z;
}

float nudge(float c)
{
    return abs(c) > 1e-30 ? c : (c < 0.0 ? -1e-30 : 1e-30);
}

// A zero component would make the slab test compute 0 * inf.
float3 safe_inverse(float3 d)
{
    return float3(1.0 / nudge(d.x), 1.0 / nudge(d.y), 1.0 / nudge(d.z));
}

float box_entry(BvhNode node, float3 origin, float3 inv, float tmin, float tmax)
{
    float3 t0 = (node.lo - origin) * inv;
    float3 t1 = (node.hi - origin) * inv;
    float3 lo = min(t0, t1);
    float3 hi = max(t0, t1);
    float enter = max(max(lo.x, lo.y), max(lo.z, tmin));
    float exit = min(min(hi.x, hi.y), min(hi.z, tmax)) * TRACE_EXIT_SCALE;
    return enter <= exit ? enter : TRACE_FAR;
}

// Möller-Trumbore, two-sided: x the distance (negative on a miss), yz the barycentrics of vertices 1 and 2.
float3 intersect_triangle(float3 o, float3 d, float3 p0, float3 p1, float3 p2)
{
    float3 e1 = p1 - p0;
    float3 e2 = p2 - p0;
    float3 p = cross(d, e2);
    float det = dot(e1, p);
    float3 miss = float3(-1.0, 0.0, 0.0);
    if (det == 0.0)
        return miss;
    float inv = 1.0 / det;
    float3 s = o - p0;
    float u = dot(s, p) * inv;
    if (u < 0.0 || u > 1.0)
        return miss;
    float3 q = cross(s, e1);
    float v = dot(d, q) * inv;
    if (v < 0.0 || u + v > 1.0)
        return miss;
    return float3(dot(e2, q) * inv, u, v);
}

// Ties go to the lower instance, then triangle, so every visiting order commits the same hit.
bool trace_closer(float t, uint instance, uint triangle, TraceHit best)
{
    return t < best.t || (t == best.t && (instance < best.instance || (instance == best.instance && triangle < best.triangle)));
}

uint material_layer(TraceMaterial m, uint slot)
{
    uint word = slot < 2u ? m.layers.x : (slot < 4u ? m.layers.y : m.layers.z);
    return (slot & 1u) == 0u ? (word & 0xFFFFu) : (word >> 16u);
}

// The slot's UV set and KHR_texture_transform rows.
float2 material_slot_uv(uint material, TraceMaterial m, uint slot, float2 uv0, float2 uv1)
{
    float2 uv = ((m.flags >> (MATERIAL_UV1_SHIFT + slot)) & 1u) != 0u ? uv1 : uv0;
    uint base = tracerInfo.material_base + material * MATERIAL_STRIDE + 4u + 2u * slot;
    float4 r0 = tracerRecords[base];
    float4 r1 = tracerRecords[base + 1u];
    return float2(r0.x * uv.x + r0.y * uv.y + r0.z, r1.x * uv.x + r1.y * uv.y + r1.z);
}

// The sampler repeats; clamp and mirror are folded into the coordinate, half a texel of the level from the edge.
float wrap_coordinate(float c, uint mode, float half_texel)
{
    if (mode == WRAP_MIRROR)
        c = 1.0 - abs(frac(c * 0.5) * 2.0 - 1.0);
    if (mode == WRAP_MIRROR || mode == WRAP_CLAMP)
        c = clamp(c, half_texel, 1.0 - half_texel);
    return c;
}

// A material slot's texel at `lod`, or the forward pass's fallback (white, or the flat normal) without a layer.
float4 sample_material_slot(uint material, TraceMaterial m, uint slot, float2 uv0, float2 uv1, float lod)
{
    uint layer = material_layer(m, slot);
    if (layer == TRACE_NO_LAYER)
        return slot == SLOT_NORMAL ? float4(0.5, 0.5, 1.0, 1.0) : float4(1.0, 1.0, 1.0, 1.0);
    bool srgb = slot == SLOT_BASE_COLOR || slot == SLOT_EMISSIVE;
    float size = float(srgb ? tracerInfo.srgb_layer_size : tracerInfo.linear_layer_size);
    float half_texel = 0.5 * exp2(max(lod, 0.0)) / size;
    uint wrap = m.flags >> (MATERIAL_WRAP_SHIFT + 4u * slot);
    float2 uv = material_slot_uv(material, m, slot, uv0, uv1);
    float3 coord = float3(wrap_coordinate(uv.x, wrap & 3u, half_texel), wrap_coordinate(uv.y, (wrap >> 2u) & 3u, half_texel), float(layer));
    bool nearest = ((m.layers.z >> (16u + slot)) & 1u) != 0u;
    if (srgb)
        return nearest ? tracerSrgbLayers.SampleLevel(tracerNearestSampler, coord, lod) : tracerSrgbLayers.SampleLevel(tracerLinearSampler, coord, lod);
    return nearest ? tracerLinearLayers.SampleLevel(tracerNearestSampler, coord, lod) : tracerLinearLayers.SampleLevel(tracerLinearSampler, coord, lod);
}

// The forward pass's MASK test at level 0: base colour alpha, texture alpha and vertex alpha against the cutoff.
bool alpha_masked_out(uint4 tri, float2 barycentrics)
{
    uint material = tri.w & TRIANGLE_MATERIAL;
    TraceMaterial m = load_material(material);
    if ((m.flags & ALPHA_MODE_MASK) != ALPHA_MASK)
        return false;
    float3 w = float3(1.0 - barycentrics.x - barycentrics.y, barycentrics.x, barycentrics.y);
    TraceVertex a = load_vertex(tri.x);
    TraceVertex b = load_vertex(tri.y);
    TraceVertex c = load_vertex(tri.z);
    float2 uv0 = a.uv0 * w.x + b.uv0 * w.y + c.uv0 * w.z;
    float2 uv1 = a.uv1 * w.x + b.uv1 * w.y + c.uv1 * w.z;
    float alpha = m.base_color.a * (a.color.a * w.x + b.color.a * w.y + c.color.a * w.z);
    alpha *= sample_material_slot(material, m, SLOT_BASE_COLOR, uv0, uv1, 0.0).a;
    return alpha < m.alpha_cutoff;
}

TraceHit trace_miss(float tmax)
{
    TraceHit hit;
    hit.t = tmax;
    hit.instance = TRACE_NO_HIT;
    hit.triangle = 0u;
    hit.barycentrics = float2(0.0, 0.0);
    return hit;
}

// Both queries in one loop over one stack: a top-level leaf enters its instance's mesh, and the mesh's floor on the stack leaves it.
TraceHit trace_scene(TraceRay ray, bool any_hit)
{
    TraceHit best = trace_miss(ray.tmax);
    if (tracerInfo.instance_count == 0u)
        return best;
    float3 origin = ray.origin;
    float3 direction = ray.direction;
    float3 inv = safe_inverse(direction);
    uint stack[TRACE_STACK_SIZE];
    uint sp = 0u;
    uint blas_floor = TRACE_NO_HIT;
    uint instance = 0u;
    BvhNode node = load_node(tracerInfo.tlas_root);
    bool live = box_entry(node, origin, inv, ray.tmin, best.t) < TRACE_FAR;
    while (live)
    {
        if (node.count == 0u)
        {
            BvhNode a = load_node(node.first);
            BvhNode b = load_node(node.first + 1u);
            float ta = box_entry(a, origin, inv, ray.tmin, best.t);
            float tb = box_entry(b, origin, inv, ray.tmin, best.t);
            bool flip = tb < ta;
            float t_near = flip ? tb : ta;
            float t_far = flip ? ta : tb;
            if (t_near < TRACE_FAR)
            {
                if (t_far < TRACE_FAR && sp < TRACE_STACK_SIZE)
                {
                    stack[sp] = flip ? node.first : node.first + 1u;
                    sp++;
                }
                node = flip ? b : a;
                continue;
            }
        }
        else if (blas_floor == TRACE_NO_HIT)
        {
            instance = node.first;
            TraceInstance inst = load_instance(instance);
            origin = transform_point(inst.world_to_object, ray.origin);
            direction = transform_vector(inst.world_to_object, ray.direction);
            inv = safe_inverse(direction);
            blas_floor = sp;
            node = load_node(inst.blas_root);
            if (box_entry(node, origin, inv, ray.tmin, best.t) < TRACE_FAR)
                continue;
        }
        else
        {
            for (uint i = 0u; i < node.count; i++)
            {
                uint triangle = node.first + i;
                uint4 tri = tracerTriangles[triangle];
                float3 hit = intersect_triangle(origin, direction, trace_position(tri.x), trace_position(tri.y), trace_position(tri.z));
                if (hit.x < ray.tmin || !trace_closer(hit.x, instance, triangle, best))
                    continue;
                if ((tri.w & TRIANGLE_MASKED) != 0u && alpha_masked_out(tri, hit.yz))
                    continue;
                best.t = hit.x;
                best.instance = instance;
                best.triangle = triangle;
                best.barycentrics = hit.yz;
                if (any_hit)
                    return best;
            }
        }
        live = false;
        while (true)
        {
            if (sp == blas_floor)
            {
                blas_floor = TRACE_NO_HIT;
                origin = ray.origin;
                direction = ray.direction;
                inv = safe_inverse(direction);
            }
            if (sp == 0u)
                break;
            sp--;
            node = load_node(stack[sp]);
            if (box_entry(node, origin, inv, ray.tmin, best.t) < TRACE_FAR)
            {
                live = true;
                break;
            }
        }
    }
    return best;
}

// The closest hit in [tmin, tmax], MASK materials alpha-tested.
TraceHit trace_closest(TraceRay ray)
{
    return trace_scene(ray, false);
}

// Whether anything blocks [tmin, tmax]: the first accepted hit ends the search.
bool trace_any(TraceRay ray)
{
    return trace_scene(ray, true).instance != TRACE_NO_HIT;
}

// The hit's world position, normals, tangent frame, UVs, colour and material; `hit` must not be a miss.
TraceSurface trace_surface(TraceRay ray, TraceHit hit)
{
    uint4 tri = tracerTriangles[hit.triangle];
    TraceInstance inst = load_instance(hit.instance);
    float3 w = float3(1.0 - hit.barycentrics.x - hit.barycentrics.y, hit.barycentrics.x, hit.barycentrics.y);
    float3 p0 = trace_position(tri.x);
    float3 p1 = trace_position(tri.y);
    float3 p2 = trace_position(tri.z);
    TraceVertex a = load_vertex(tri.x);
    TraceVertex b = load_vertex(tri.y);
    TraceVertex c = load_vertex(tri.z);
    TraceSurface s;
    s.position = ray.origin + ray.direction * hit.t;
    s.geometric_normal = normalize(transform_normal(inst.world_to_object, cross(p1 - p0, p2 - p0)));
    float3 n = transform_normal(inst.world_to_object, a.normal * w.x + b.normal * w.y + c.normal * w.z);
    s.normal = dot(n, n) > 1e-30 ? normalize(n) : s.geometric_normal;
    float4 t = a.tangent * w.x + b.tangent * w.y + c.tangent * w.z;
    float3 tw = transform_vector(inst.object_to_world, t.xyz);
    float mirror = (inst.flags & INSTANCE_MIRRORED) != 0u ? -1.0 : 1.0;
    s.tangent = float4(dot(tw, tw) > 1e-30 ? normalize(tw) : float3(0.0, 0.0, 0.0), (t.w < 0.0 ? -1.0 : 1.0) * mirror);
    s.uv0 = a.uv0 * w.x + b.uv0 * w.y + c.uv0 * w.z;
    s.uv1 = a.uv1 * w.x + b.uv1 * w.y + c.uv1 * w.z;
    s.color = a.color * w.x + b.color * w.y + c.color * w.z;
    s.material = tri.w & TRIANGLE_MATERIAL;
    s.front_face = dot(s.geometric_normal, ray.direction) < 0.0;
    return s;
}
