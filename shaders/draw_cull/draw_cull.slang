// Rust/WebGPU per-draw culling into indirect draw arguments: the checked-in draw_cull.wgsl is generated from this file.

// Camera planes, then each cascade's; Gribb-Hartmann, normals inward, as `render::bounds::Frustum`.
struct CullViews
{
    float4 camera_planes[6];
    float4 cascade_planes[18];
    // xyz: this frame's eye; the occlusion data was measured from `prev_eye`.
    float4 eye;
    float4 prev_eye;
    uint slot_count;
    uint cascade_count;
    uint flags;
    // Primitives the occlusion source measured; any other counts as visible.
    uint occlusion_count;
    // Slots between one view's arguments and the next's: the args buffer's capacity.
    uint slot_stride;
    uint pad0;
    uint pad1;
    uint pad2;
};

struct DrawInput
{
    float4 aabb_min;
    float4 aabb_max;
    uint primitive;
    uint flags;
    uint camera_index_count;
    uint shadow_index_count;
    uint instance_count;
    uint pad0;
    uint pad1;
    uint pad2;
};

[vk::binding(0, 0)] ConstantBuffer<CullViews> views;
[vk::binding(1, 0)] StructuredBuffer<DrawInput> draws;
// One u32 per primitive (non-zero: visible) or one u64 sample count as two u32 words.
[vk::binding(2, 0)] StructuredBuffer<uint> occlusion;
// DrawIndexedIndirectArgs, five words each: the camera's slots, then each cascade's.
[vk::binding(3, 0)] RWStructuredBuffer<uint> args;
// Opaque considered, opaque drawn, blended drawn, casters considered, casters drawn.
[vk::binding(4, 0)] RWStructuredBuffer<Atomic<uint>> stats;

static const uint VIEW_FRUSTUM = 1u;
static const uint VIEW_OCCLUSION_VISIBILITY = 2u;
static const uint VIEW_OCCLUSION_SAMPLES = 4u;

static const uint DRAW_BLENDED = 1u;
static const uint DRAW_CASTS_SHADOW = 2u;
// The primitive's first slot, which alone counts it in the stats.
static const uint DRAW_COUNTED = 4u;

static const uint ARGS_WORDS = 5u;

// Relative half-extent margin plus 1 cm, `render::occlusion::aabb_contains`'s box.
static const float CONTAINMENT_MARGIN = 0.02;

bool outside_plane(float4 plane, float3 lo, float3 hi)
{
    float3 p = float3(plane.x >= 0.0 ? hi.x : lo.x, plane.y >= 0.0 ? hi.y : lo.y, plane.z >= 0.0 ? hi.z : lo.z);
    return dot(plane.xyz, p) + plane.w < 0.0;
}

bool in_camera(float3 lo, float3 hi)
{
    for (uint i = 0u; i < 6u; ++i)
    {
        if (outside_plane(views.camera_planes[i], lo, hi)) return false;
    }
    return true;
}

// Without the near plane: casters between the light and the cascade box still shadow it.
bool in_cascade(uint cascade, float3 lo, float3 hi)
{
    for (uint i = 0u; i < 6u; ++i)
    {
        if (i == 4u) continue;
        if (outside_plane(views.cascade_planes[cascade * 6u + i], lo, hi)) return false;
    }
    return true;
}

bool contains(float3 lo, float3 hi, float3 p)
{
    float3 expand = (hi - lo) * 0.5 * CONTAINMENT_MARGIN + float3(0.01, 0.01, 0.01);
    return all(p >= lo - expand) && all(p <= hi + expand);
}

// Last frame's measurement; from inside its box only back faces rasterise, so that never culls.
bool occluded(DrawInput draw)
{
    uint p = draw.primitive;
    if (p >= views.occlusion_count) return false;
    bool hidden = false;
    if ((views.flags & VIEW_OCCLUSION_VISIBILITY) != 0u) hidden = occlusion[p] == 0u;
    else if ((views.flags & VIEW_OCCLUSION_SAMPLES) != 0u) hidden = (occlusion[2u * p] | occlusion[2u * p + 1u]) == 0u;
    if (!hidden) return false;
    return !contains(draw.aabb_min.xyz, draw.aabb_max.xyz, views.eye.xyz)
        && !contains(draw.aabb_min.xyz, draw.aabb_max.xyz, views.prev_eye.xyz);
}

void write_args(uint slot, uint index_count, uint instance_count)
{
    uint base = slot * ARGS_WORDS;
    args[base] = index_count;
    args[base + 1u] = instance_count;
    args[base + 2u] = 0u;
    args[base + 3u] = 0u;
    args[base + 4u] = 0u;
}

void count(uint stat, bool counted)
{
    if (counted) stats[stat].add(1u);
}

void cull_camera(uint idx, DrawInput draw, bool counted)
{
    float3 lo = draw.aabb_min.xyz;
    float3 hi = draw.aabb_max.xyz;
    bool frustum = (views.flags & VIEW_FRUSTUM) == 0u || in_camera(lo, hi);
    bool visible = frustum;
    if ((draw.flags & DRAW_BLENDED) != 0u)
    {
        count(2u, counted && visible);
    }
    else
    {
        count(0u, counted && frustum);
        visible = frustum && !occluded(draw);
        count(1u, counted && visible);
    }
    write_args(idx, draw.camera_index_count, visible ? draw.instance_count : 0u);
}

void cull_cascades(uint idx, DrawInput draw, bool counted)
{
    bool caster = (draw.flags & DRAW_CASTS_SHADOW) != 0u;
    for (uint cascade = 0u; cascade < views.cascade_count; ++cascade)
    {
        bool visible = caster
            && ((views.flags & VIEW_FRUSTUM) == 0u || in_cascade(cascade, draw.aabb_min.xyz, draw.aabb_max.xyz));
        count(3u, counted && caster);
        count(4u, counted && visible);
        uint slot = (cascade + 1u) * views.slot_stride + idx;
        write_args(slot, draw.shadow_index_count, visible ? draw.instance_count : 0u);
    }
}

[shader("compute")]
[numthreads(64, 1, 1)]
void cs_main(uint3 gid : SV_DispatchThreadID)
{
    uint idx = gid.x;
    if (idx >= views.slot_count) return;
    DrawInput draw = draws[idx];
    bool counted = (draw.flags & DRAW_COUNTED) != 0u;
    cull_camera(idx, draw, counted);
    cull_cascades(idx, draw, counted);
}
