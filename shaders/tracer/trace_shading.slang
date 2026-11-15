// What the ray- and path-traced kernels share: their bindings, the camera's rays, a hit's material as the forward pass reads it, and the sky.

import bvh_traverse;
import brdf;
import sky_model;

// render::tracer::modes::KERNEL_GROUP_SIZE: each kernel's workgroup is this square of pixels.
static const uint TRACE_GROUP_SIZE = 8;
// The C++ kernels' ray bounds: a hit nearer than TRACE_TMIN is the surface the ray left.
static const float TRACE_TMIN = 0.001;
static const float TRACE_TMAX = 10000.0;
// Along the face normal, against self-intersection, as the C++ path tracer offsets.
static const float TRACE_SURFACE_OFFSET = 0.0001;
// Below fp16's 65504: a larger radiance stores as +Inf in the HDR target, and ACES turns Inf into NaN.
static const float TRACE_HDR_MAX = 65000.0;

// render::tracer::modes::TraceFrameUniforms; `size` stays first, as trace_resolve.slang reads only it.
struct TraceFrame
{
    uint4 size;              // width, height, the path tracer's bounce cap, unused
    float4x4 inv_view_proj;  // clip to world: the raster camera's, the viewport's lens shift included
    float4 light_dir;        // towards the sun, unit; w: the sky's sun-disk intensity
    float4 light_radiance;   // colour times intensity; w: the sky's light, the raster's ambient strength times its environment's
    float4 furnace;          // the white furnace's sky radiance, metallic, roughness; w: 1 when on
};

// One dispatch's pixels and samples, bound at a dynamic offset: render::tracer::modes::TraceDispatch.
struct TraceTile
{
    uint4 rect;     // first pixel x, y, then width, height
    uint4 samples;  // first sample, count, samples the accumulation held before this dispatch, unused
};

[vk::binding(0, 0)] ConstantBuffer<TraceFrame> traceFrame;
// One running mean per pixel, row-major; WebGPU has no read-write rgba32f storage texture.
[vk::binding(1, 0)] RWStructuredBuffer<float4> traceAccumulation;
[vk::binding(2, 0)] ConstantBuffer<TraceTile> traceTile;

// A hit as both kernels shade it.
struct TracedSurface
{
    float3 position;
    // Both face the ray's origin: the face normal offsets the next ray and bounds it, the shading normal lights.
    float3 face_normal;
    float3 normal;
    float3 albedo;
    float3 emissive;
    float metallic;
    float roughness;
    bool unlit;
};

// WGSL may assume floats are finite, so a NaN is found by its bits; clamp then caps +Inf.
float3 hdr_safe(float3 radiance)
{
    bool3 nan = (asuint(radiance) & 0x7FFFFFFFu) > 0x7F800000u;
    return clamp(select(nan, float3(0.0), radiance), float3(0.0), float3(TRACE_HDR_MAX));
}

float3 normalize_or(float3 v, float3 fallback)
{
    return dot(v, v) > 1e-30 ? normalize(v) : fallback;
}

// The pixel this thread shades, or false past the dispatch's rect.
bool tile_pixel(uint2 thread, out uint2 pixel)
{
    pixel = traceTile.rect.xy + thread;
    return thread.x < traceTile.rect.z && thread.y < traceTile.rect.w;
}

uint pixel_index(uint2 pixel)
{
    return pixel.y * traceFrame.size.x + pixel.x;
}

// From the near plane through `pixel` (top-left origin, in pixels) to the far plane: what the raster camera sees.
TraceRay camera_ray(float2 pixel)
{
    float2 size = float2(traceFrame.size.xy);
    float2 ndc = float2(pixel.x / size.x * 2.0 - 1.0, 1.0 - pixel.y / size.y * 2.0);
    float4 near = mul(traceFrame.inv_view_proj, float4(ndc, 0.0, 1.0));
    float4 far = mul(traceFrame.inv_view_proj, float4(ndc, 1.0, 1.0));
    float3 origin = near.xyz / near.w;
    float3 span = far.xyz / far.w - origin;
    TraceRay ray;
    ray.origin = origin;
    ray.tmin = 0.0;
    ray.direction = normalize(span);
    ray.tmax = length(span);
    return ray;
}

// forward.slang's normal mapping: a two-channel map rebuilds z, and a back face turns the whole frame.
float3 mapped_normal(TraceSurface s, float4 texel, float scale)
{
    float3 t = normalize_or(s.tangent.xyz - s.normal * dot(s.normal, s.tangent.xyz), float3(0.0));
    float3 b = cross(s.normal, t) * s.tangent.w;
    float3 n = texel.xyz * 2.0 - 1.0;
    if (texel.b == 0.0)
        n.z = sqrt(saturate(1.0 - dot(n.xy, n.xy)));
    n = float3(n.xy * scale, n.z);
    float3 mapped = normalize_or(mul(n, float3x3(t, b, s.normal)), s.normal);
    return s.front_face ? mapped : -mapped;
}

// The hit's material at level 0, read like the forward pass; the kernels trace unit-length rays.
TracedSurface shade_hit(TraceRay ray, TraceHit hit)
{
    TraceSurface s = trace_surface(ray, hit);
    TraceMaterial m = load_material(s.material);
    float4 base = sample_material_slot(s.material, m, SLOT_BASE_COLOR, s.uv0, s.uv1, 0.0);
    float4 mr = sample_material_slot(s.material, m, SLOT_METALLIC_ROUGHNESS, s.uv0, s.uv1, 0.0);
    float4 normal = sample_material_slot(s.material, m, SLOT_NORMAL, s.uv0, s.uv1, 0.0);
    float4 emission = sample_material_slot(s.material, m, SLOT_EMISSIVE, s.uv0, s.uv1, 0.0);
    TracedSurface o;
    o.position = s.position;
    o.face_normal = s.front_face ? s.geometric_normal : -s.geometric_normal;
    float3 n = mapped_normal(s, normal, m.normal_scale);
    // A normal tilted away from the viewer leaves no lobe to shade, so the face normal shades, as in the C++ path tracer.
    o.normal = dot(n, -ray.direction) < 1e-4 ? o.face_normal : n;
    o.albedo = m.base_color.rgb * base.rgb * s.color.rgb;
    o.emissive = m.emissive * emission.rgb;
    o.metallic = clamp(m.metallic * mr.b, 0.0, 1.0);
    o.roughness = clamp(m.roughness * mr.g, MIN_ROUGHNESS, 1.0);
    o.unlit = (m.flags & MATERIAL_UNLIT) != 0u;
    return o;
}

// Whether the sun reaches the surface: any hit on the way blocks it, MASK cut-outs alpha-tested.
bool sun_visible(TracedSurface s)
{
    TraceRay ray;
    ray.origin = s.position + s.face_normal * TRACE_SURFACE_OFFSET;
    ray.tmin = TRACE_TMIN;
    ray.direction = traceFrame.light_dir.xyz;
    ray.tmax = TRACE_TMAX;
    return !trace_any(ray);
}

// The sun's GGX and Lambert term through the forward pass's BRDF, or nothing in its shadow.
float3 sun_light(TracedSurface s, float3 v)
{
    float3 l = traceFrame.light_dir.xyz;
    if (dot(s.normal, l) <= 0.0 || !sun_visible(s))
        return float3(0.0);
    float3 f0 = lerp(float3(0.04), s.albedo, s.metallic);
    return brdf_direct(s.normal, v, l, s.albedo, s.metallic, s.roughness, f0, traceFrame.light_radiance.rgb);
}

// The raster sky, sun disk included: what a camera ray that hits nothing sees.
float3 sky_with_sun(float3 dir)
{
    return sky_gradient(dir) + sun_disk(dir, traceFrame.light_dir.xyz, traceFrame.light_dir.w);
}
