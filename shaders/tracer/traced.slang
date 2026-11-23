import brdf;
import bvh_traverse;
import constants;
import hdr_safe;
import path_estimator;
import trace_constants;
import trace_shading;

// The ray- and path-traced modes' kernels: each dispatch shades one tile into the accumulation, which trace_resolve.slang copies into the HDR target.

// The ray tracer's light `index`: the sun first, then the punctual lights in the raster's order.
LightSample direct_sample(TracedSurface s, float3 v, float2 energy, uint index)
{
    if (index == 0u)
        return sun_sample(s, v, energy);
    return punctual_sample(s, v, energy, index - 1u);
}

// The sun and every punctual light, each through its own shadow ray, as raytrace.rchit.slang lights a hit.
float3 direct_light(TracedSurface s, float3 v)
{
    float3 lit = float3(0.0);
    // The fit once per hit, not once per light.
    float2 energy = ggx_energy(dot(s.normal, v), s.roughness);
    // One shadow-ray call site, as in next_event: SwiftShader inlines a whole traversal per call site.
    for (uint i = 0u; i <= traceFrame.lights.x; i++)
        lit += shadowed(s, direct_sample(s, v, energy, i));
    return lit;
}

// The C++ ray-traced mode less its ambient.slang term; alpha is the hit's NDC depth for the clouds, as raytrace.rgen.slang writes it.
float4 ray_trace_pixel(uint2 pixel)
{
    TraceRay ray = pixel_ray(float2(pixel) + 0.5);
    TraceHit hit = trace_closest(ray);
    if (hit.instance == TRACE_NO_HIT)
        return float4(sky_with_sun(ray.direction), 1.0);
    TracedSurface s = shade_hit(ray, hit);
    float3 colour = s.unlit ? s.albedo : direct_light(s, -ray.direction) + s.emissive;
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
    TraceRay ray = pixel_ray(float2(pixel) + 0.5);
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
    return bounce == 0u ? sky_with_sun(dir) : bounce_sky(dir);
}

// The sun and one punctual light, each through a shadow ray; the furnace has a uniform sky and no lights.
float3 next_event(TracedSurface s, float3 v, float2 energy, inout uint rng)
{
    if (furnace_on())
        return float3(0.0);
    LightSample lights[2] = { sun_sample(s, v, energy), punctual_light_sample(s, v, energy, rng) };
    float3 lit = float3(0.0);
    // One shadow-ray call site in a loop: SwiftShader inlines a whole traversal per call site, and compiles for minutes.
    for (uint i = 0u; i < 2u; i++)
        lit += shadowed(s, lights[i]);
    return lit;
}

// One path of at most the bounce cap's segments from a point jittered inside the pixel, the C++ estimator.
float3 trace_path(uint2 pixel, inout uint rng)
{
    float2 jitter = float2(rng_next(rng), rng_next(rng));
    TraceRay ray = pixel_ray(float2(pixel) + jitter);
    float3 throughput = float3(1.0);
    float3 radiance = float3(0.0);
    for (uint bounce = 0u; bounce < traceFrame.size.z; bounce++)
    {
        if (!russian_roulette_survives(bounce, throughput, rng))
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
        // The fit once per hit: both lights and the bounce share it.
        float2 energy = ggx_energy(dot(s.normal, v), s.roughness);
        radiance += throughput * (s.emissive + next_event(s, v, energy, rng));
        Bounce next = sample_bounce(s.normal, s.face_normal, v, s.albedo, s.metallic, s.roughness, energy, rng);
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
    uint rng = path_rng_seed(index, traceTile.samples.z);
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
