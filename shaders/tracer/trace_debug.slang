import bvh_traverse;

// Traces a buffer of rays against the tracer's scene, for the tests that hold the GPU traversal to the CPU one.

struct TraceDebugParams
{
    uint ray_count;
    uint pad0;
    uint pad1;
    uint pad2;
};

[vk::binding(0, 0)] ConstantBuffer<TraceDebugParams> debugParams;
// Two float4 per ray: (origin, tmin), (direction, tmax).
[vk::binding(1, 0)] StructuredBuffer<float4> debugRays;
// Four float4 per ray, see render::tracer::debug::DebugHit.
[vk::binding(2, 0)] RWStructuredBuffer<float4> debugHits;

static const uint DEBUG_HIT_STRIDE = 4;

TraceRay debug_ray(uint index)
{
    float4 a = debugRays[index * 2u];
    float4 b = debugRays[index * 2u + 1u];
    TraceRay ray;
    ray.origin = a.xyz;
    ray.tmin = a.w;
    ray.direction = b.xyz;
    ray.tmax = b.w;
    return ray;
}

[shader("compute")]
[numthreads(64, 1, 1)]
void cs_trace_closest(uint3 id : SV_DispatchThreadID)
{
    if (id.x >= debugParams.ray_count)
        return;
    TraceRay ray = debug_ray(id.x);
    TraceHit hit = trace_closest(ray);
    uint out_index = id.x * DEBUG_HIT_STRIDE;
    debugHits[out_index] = float4(hit.t, asfloat(hit.instance), asfloat(hit.triangle), hit.barycentrics.x);
    if (hit.instance == TRACE_NO_HIT)
    {
        debugHits[out_index + 1u] = float4(0.0, 0.0, 0.0, 0.0);
        debugHits[out_index + 2u] = float4(0.0, 0.0, 0.0, 0.0);
        debugHits[out_index + 3u] = float4(0.0, 0.0, 0.0, 0.0);
        return;
    }
    TraceSurface s = trace_surface(ray, hit);
    TraceMaterial m = load_material(s.material);
    float4 base = sample_material_slot(s.material, m, SLOT_BASE_COLOR, s.uv0, s.uv1, 0.0);
    debugHits[out_index + 1u] = float4(hit.barycentrics.y, asfloat(s.material), s.front_face ? 1.0 : 0.0, s.tangent.w);
    debugHits[out_index + 2u] = float4(s.normal, s.color.a);
    debugHits[out_index + 3u] = float4(s.uv0, base.r, base.a);
}

[shader("compute")]
[numthreads(64, 1, 1)]
void cs_trace_any(uint3 id : SV_DispatchThreadID)
{
    if (id.x >= debugParams.ray_count)
        return;
    bool blocked = trace_any(debug_ray(id.x));
    uint out_index = id.x * DEBUG_HIT_STRIDE;
    debugHits[out_index] = float4(blocked ? 1.0 : 0.0, 0.0, 0.0, 0.0);
}
