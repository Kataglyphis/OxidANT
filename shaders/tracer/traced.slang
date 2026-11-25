import brdf;
import bvh_traverse;
import camera_ray;
import hdr_safe;
import path_estimator;
import path_integrator;
import trace_shading;

// The ray- and path-traced modes' kernels: each dispatch shades one tile into the accumulation, which trace_resolve.slang copies into the HDR target.

// The ray tracer's light `index`: the sun first, then the punctual lights in the raster's order.
LightSample direct_sample(TracerScene scene, TracedSurface s, float3 v, float2 energy, uint index)
{
    if (index == 0u)
        return sun_sample(s, v, energy, scene.sun_direction(), scene.sun_radiance());
    return punctual_sample(s, v, energy, scene.punctual(index - 1u));
}

// The sun and every punctual light, each through its own shadow ray, as raytrace.rchit.slang lights a hit.
float3 direct_light(TracedSurface s, float3 v, float2 energy)
{
    TracerScene scene;
    float3 lit = float3(0.0);
    // One shadow-ray call site, as in next_event: SwiftShader inlines a whole traversal per call site.
    for (uint i = 0u; i <= scene.punctual_count(); i++)
        lit += shadowed(scene, s, direct_sample(scene, s, v, energy, i));
    return lit;
}

// The C++ ray-traced mode; alpha is the hit's NDC depth for the clouds, as raytrace.rgen.slang writes it.
float4 ray_trace_pixel(uint2 pixel)
{
    TraceRay ray = pixel_ray(float2(pixel) + 0.5);
    TraceHit hit = trace_closest(ray);
    if (hit.instance == TRACE_NO_HIT)
        return float4(sky_with_sun(ray.direction), 1.0);
    TracedSurface s = shade_hit(ray, hit);
    if (s.unlit)
        return float4(s.albedo, ndc_depth(ray, hit));
    float3 v = -ray.direction;
    // The fit once per hit: every light and the ambient share it.
    float2 energy = ggx_energy(dot(s.normal, v), s.roughness);
    return float4(direct_light(s, v, energy) + traced_ambient(s, v, energy) + s.emissive, ndc_depth(ray, hit));
}

[shader("compute")]
[numthreads(TRACE_GROUP_SIZE, TRACE_GROUP_SIZE, 1)]
void cs_ray_trace(uint3 id : SV_DispatchThreadID)
{
    uint2 pixel;
    if (!tile_pixel(id.xy, pixel))
        return;
    float4 traced = ray_trace_pixel(pixel);
    traceAccumulation[pixel_index(pixel)] = float4(hdr_safe(traced.rgb), traced.a);
}

// The camera's segment through `pixel` (top-left origin, in pixels), from its near plane to its far plane.
TracedSegment camera_segment(float2 pixel)
{
    CameraRay ray = camera_ray_through_pixel(traceFrame.inv_view_proj, pixel, float2(traceFrame.size.xy));
    TracedSegment segment = { ray.origin, 0.0, ray.direction, ray.length };
    return segment;
}

// One path from a point jittered inside the pixel, over the tracer's scene.
float3 trace_pixel_path(uint2 pixel, inout uint rng, inout PathDepth depth)
{
    float2 jitter = float2(rng_next(rng), rng_next(rng));
    TracerScene scene;
    return trace_path(scene, camera_segment(float2(pixel) + jitter), traceFrame.size.z, rng, depth);
}

// The NDC depth raster writes for the probe's surface, 1 for nothing hit, as a cleared depth buffer holds.
float probe_depth(PathDepth depth)
{
    float4 clip = mul(traceFrame.view_proj, float4(depth.position, 1.0));
    return depth.hit ? saturate(clip.z / clip.w) : 1.0;
}

// Adds the dispatch's samples to the pixel's running mean; the first after a reset overwrites what was there.
[shader("compute")]
[numthreads(TRACE_GROUP_SIZE, TRACE_GROUP_SIZE, 1)]
void cs_path_trace(uint3 id : SV_DispatchThreadID)
{
    uint2 pixel;
    if (!tile_pixel(id.xy, pixel))
        return;
    uint index = pixel_index(pixel);
    // The samples already accumulated pick the stream, so each dispatch of each frame draws new ones.
    uint rng = path_rng_seed(index, traceTile.samples.z);
    // The frame's first chunk probes the depth through its first sample's pass; its later chunks keep it.
    PathDepth probe = no_depth();
    probe.wanted = traceTile.samples.x == 0u;
    probe.ray = camera_segment(float2(pixel) + 0.5);
    float3 sum = float3(0.0);
    for (uint i = 0u; i < traceTile.samples.y; i++)
    {
        sum += hdr_safe(trace_pixel_path(pixel, rng, probe));
        probe.wanted = false;
    }
    float before = float(traceTile.samples.z);
    float4 held = traceAccumulation[index];
    float3 previous = traceTile.samples.z > 0u ? held.rgb : float3(0.0);
    float depth = traceTile.samples.x == 0u ? probe_depth(probe) : held.a;
    traceAccumulation[index] = float4((previous * before + sum) / (before + float(traceTile.samples.y)), depth);
}
