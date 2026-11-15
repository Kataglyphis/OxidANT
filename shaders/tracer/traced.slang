import brdf;
import bvh_traverse;
import path_estimator;
import sky_model;
import trace_shading;

// The ray- and path-traced modes' kernels: each dispatch shades one tile into the accumulation, which trace_resolve.slang copies into the HDR target.

// The C++ ray-traced mode, no ambient term; alpha is the hit's NDC depth for the clouds, as raytrace.rgen.slang writes it.
float4 ray_trace_pixel(uint2 pixel)
{
    TraceRay ray = camera_ray(float2(pixel) + 0.5);
    TraceHit hit = trace_closest(ray);
    if (hit.instance == TRACE_NO_HIT)
        return float4(sky_with_sun(ray.direction), 1.0);
    TracedSurface s = shade_hit(ray, hit);
    float3 colour = s.unlit ? s.albedo : sun_light(s, -ray.direction) + s.emissive;
    return float4(colour, ndc_depth(ray, hit));
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

// path_tracing.slang's primary_hit_depth: the first surface through the pixel centre, unjittered, for the clouds.
float primary_hit_depth(uint2 pixel)
{
    TraceRay ray = camera_ray(float2(pixel) + 0.5);
    return ndc_depth(ray, trace_closest(ray));
}

bool furnace_on()
{
    return traceFrame.furnace.w > 0.5;
}

// The furnace's one white surface everywhere, with its own metallic and roughness.
TracedSurface path_surface(TraceRay ray, TraceHit hit)
{
    TracedSurface s = shade_hit(ray, hit);
    if (furnace_on())
    {
        s.albedo = float3(1.0);
        s.emissive = float3(0.0);
        s.unlit = false;
        s.metallic = traceFrame.furnace.y;
        s.roughness = clamp(traceFrame.furnace.z, MIN_ROUGHNESS, 1.0);
    }
    return s;
}

// The sky a path ends in: a camera ray sees the raster's; a bounce gets the light the raster's ambient term does, without the sun NEE brings.
float3 miss_radiance(float3 dir, uint bounce)
{
    if (furnace_on())
        return float3(traceFrame.furnace.x);
    return bounce == 0u ? sky_with_sun(dir) : sky_gradient(dir) * traceFrame.light_radiance.w;
}

// The sun through one shadow ray; the furnace has a uniform sky and no sun.
float3 next_event(TracedSurface s, float3 v)
{
    if (furnace_on())
        return float3(0.0);
    return sun_light(s, v);
}

// Russian roulette from the fourth segment, survival in [0.05, 0.95]; the 1/survival reweight keeps it unbiased.
bool survives(uint bounce, inout float3 throughput, inout uint rng)
{
    if (bounce < 3u)
        return true;
    float survival = clamp(max(throughput.r, max(throughput.g, throughput.b)), 0.05, 0.95);
    if (rng_next(rng) > survival)
        return false;
    throughput /= survival;
    return true;
}

// One path of at most the bounce cap's segments from a point jittered inside the pixel, the C++ estimator.
float3 trace_path(uint2 pixel, inout uint rng)
{
    float2 jitter = float2(rng_next(rng), rng_next(rng));
    TraceRay ray = camera_ray(float2(pixel) + jitter);
    float3 throughput = float3(1.0);
    float3 radiance = float3(0.0);
    for (uint bounce = 0u; bounce < traceFrame.size.z; bounce++)
    {
        if (!survives(bounce, throughput, rng))
            break;
        TraceHit hit = trace_closest(ray);
        if (hit.instance == TRACE_NO_HIT)
        {
            radiance += throughput * miss_radiance(ray.direction, bounce);
            break;
        }
        TracedSurface s = path_surface(ray, hit);
        // KHR_materials_unlit ends the path at its albedo; emission joins before this surface absorbs.
        if (s.unlit)
        {
            radiance += throughput * s.albedo;
            break;
        }
        float3 v = -ray.direction;
        radiance += throughput * (s.emissive + next_event(s, v));
        Bounce next = sample_bounce(s.normal, s.face_normal, v, s.albedo, s.metallic, s.roughness, rng);
        if (all(next.weight == float3(0.0)))
            break;
        throughput *= next.weight;
        ray.origin = s.position + s.face_normal * TRACE_SURFACE_OFFSET;
        ray.tmin = TRACE_TMIN;
        ray.direction = next.direction;
        ray.tmax = TRACE_TMAX;
    }
    return radiance;
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
    uint rng = index + traceTile.samples.z * 2654435769u;
    float3 sum = float3(0.0);
    for (uint i = 0u; i < traceTile.samples.y; i++)
        sum += hdr_safe(trace_path(pixel, rng));
    float before = float(traceTile.samples.z);
    float4 held = traceAccumulation[index];
    float3 previous = traceTile.samples.z > 0u ? held.rgb : float3(0.0);
    // The frame's first chunk traces the depth; its later chunks over the same tile keep it.
    float depth = traceTile.samples.x == 0u ? primary_hit_depth(pixel) : held.a;
    traceAccumulation[index] = float4((previous * before + sum) / (before + float(traceTile.samples.y)), depth);
}
