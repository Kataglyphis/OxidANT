// What the ray- and path-traced kernels share: their bindings, the camera's rays, a hit's material as the forward pass reads it, and the sky.

import ambient;
import bvh_traverse;
import camera_ray;
import drawn_sky;
import gltf_material;
import path_integrator;
import punctual_light;
import sky_model;
import surface_lighting;
import trace_constants;

// render::tracer::modes::KERNEL_GROUP_SIZE: each kernel's workgroup is this square of pixels.
static const uint TRACE_GROUP_SIZE = 8;

// render::tracer::modes::TraceFrameUniforms; `size` stays first, as trace_resolve.slang reads only it.
struct TraceFrame
{
    uint4 size;              // width, height, the path tracer's bounce cap, unused
    float4x4 inv_view_proj;  // clip to world: the raster camera's, the viewport's lens shift included
    float4 light_dir;        // towards the sun, unit; w: the sky's sun-disk intensity
    float4 light_radiance;   // colour times intensity; w: the sky's light, the raster's ambient strength times its environment's
    float4 furnace;          // the white furnace's sky radiance, metallic, roughness; w: 1 when on
    float4x4 view_proj;      // world to clip, the raster's, so a hit's depth is the one raster writes
    uint4 lights;            // punctual light count, 1 when an environment is bound, the prefiltered map's top mip, sky.slang's mode
};

// One dispatch's pixels and samples, bound at a dynamic offset: render::tracer::modes::TraceDispatch.
struct TraceTile
{
    uint4 rect;     // first pixel x, y, then width, height
    uint4 samples;  // first sample, count, samples the accumulation held before this dispatch, unused
};

[vk::binding(0, 0)] ConstantBuffer<TraceFrame> traceFrame;
// One running mean per pixel, row-major; WebGPU has no read-write rgba32f storage texture. Alpha: the primary hit's NDC depth.
[vk::binding(1, 0)] RWStructuredBuffer<float4> traceAccumulation;
[vk::binding(2, 0)] ConstantBuffer<TraceTile> traceTile;
// The raster's punctual lights, sky and IBL maps, at render::tracer::modes::TRACE_LIGHTS_GROUP.
[vk::binding(0, 2)] StructuredBuffer<float4> tracerLights;
// The cube map the sky pass draws, its black stand-in without one: what a path-traced camera miss sees.
[vk::binding(1, 2)] TextureCube<float4> tracerSkybox;
// The raster's baked IBL maps, the 1x1 stand-ins without an environment, and their clamping sampler.
[vk::binding(2, 2)] TextureCube<float4> tracerIrradiance;
[vk::binding(3, 2)] TextureCube<float4> tracerPrefiltered;
[vk::binding(4, 2)] Texture2D<float4> tracerBrdfLut;
[vk::binding(5, 2)] SamplerState tracerIblSampler;

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

// The raster camera's ray through `pixel` (top-left origin, in pixels), from its near plane to its far plane.
TraceRay pixel_ray(float2 pixel)
{
    CameraRay camera = camera_ray_through_pixel(traceFrame.inv_view_proj, pixel, float2(traceFrame.size.xy));
    TraceRay ray;
    ray.origin = camera.origin;
    ray.tmin = 0.0;
    ray.direction = camera.direction;
    ray.tmax = camera.length;
    return ray;
}

// The NDC depth raster writes for a world point, 1 for nothing hit, as a cleared depth buffer holds.
float ndc_depth(TraceRay ray, TraceHit hit)
{
    if (hit.instance == TRACE_NO_HIT)
        return 1.0;
    float4 clip = mul(traceFrame.view_proj, float4(ray.origin + ray.direction * hit.t, 1.0));
    return saturate(clip.z / clip.w);
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
    float4 occlusion = sample_material_slot(s.material, m, SLOT_OCCLUSION, s.uv0, s.uv1, 0.0);
    TracedSurface o;
    o.position = s.position;
    o.face_normal = s.front_face ? s.geometric_normal : -s.geometric_normal;
    o.normal = apply_normal_map(s.normal, s.tangent, tangent_space_normal(normal.xyz, m.normal_scale), s.front_face);
    o.albedo = m.base_color.rgb * base.rgb * s.color.rgb;
    o.emissive = m.emissive * emission.rgb;
    o.metallic = material_metallic(m.metallic, mr.b);
    o.roughness = material_roughness(m.roughness, mr.g);
    o.occlusion = material_occlusion(m.occlusion_strength, occlusion.r);
    o.unlit = (m.flags & MATERIAL_UNLIT) != 0u;
    return o;
}

// The raster sky, sun disk included: what a camera ray that hits nothing sees.
float3 sky_with_sun(float3 dir)
{
    return sky_gradient(dir) + sun_disk(dir, traceFrame.light_dir.xyz, traceFrame.light_dir.w);
}

// Punctual light `index` of the raster's buffer.
[ForceInline]
PunctualLight tracer_punctual(uint index)
{
    return unpack_punctual_light(tracerLights[index * 4u], tracerLights[index * 4u + 1u], tracerLights[index * 4u + 2u],
        tracerLights[index * 4u + 3u]);
}

// The tracer's IBL maps, read as the forward pass reads its own.
struct TracerAmbientMaps : IAmbientMaps
{
    float3 irradiance(float3 n)
    {
        return tracerIrradiance.SampleLevel(tracerIblSampler, n, 0.0).rgb;
    }
    float3 reflection(float3 r, float roughness)
    {
        return tracerPrefiltered.SampleLevel(tracerIblSampler, r, roughness * float(traceFrame.lights.z)).rgb;
    }
    float2 energy(float nDotV, float roughness)
    {
        return tracerBrdfLut.SampleLevel(tracerIblSampler, float2(nDotV, roughness), 0.0).rg;
    }
}

// The raster's ambient term, unshadowed as every C++ mode adds it: the baked IBL with an environment, else the analytic sky.
float3 traced_ambient(TracedSurface s, float3 v, float2 energy)
{
    TracerAmbientMaps maps;
    float3 ambient = sky_ambient(maps, traceFrame.lights.y != 0u, s.normal, v, s.albedo, s.metallic, s.roughness,
        surface_f0(s.albedo, s.metallic), energy);
    return traceFrame.light_radiance.w * s.occlusion * ambient;
}

// The ray segment as bvh_traverse traces it.
[ForceInline]
TraceRay trace_ray(TracedSegment segment)
{
    TraceRay ray;
    ray.origin = segment.origin;
    ray.tmin = segment.tmin;
    ray.direction = segment.direction;
    ray.tmax = segment.tmax;
    return ray;
}

// The tracer's scene for path_integrator.slang: bvh_traverse's rays, the raster's sky, lights and IBL.
struct TracerScene : ITracedScene
{
    [ForceInline]
    bool closest(TracedSegment segment, out TracedSurface s)
    {
        TraceRay ray = trace_ray(segment);
        TraceHit hit = trace_closest(ray);
        bool found = hit.instance != TRACE_NO_HIT;
        s = found ? shade_hit(ray, hit) : no_surface();
        return found;
    }
    [ForceInline]
    bool visible(TracedSurface s, float3 l, float distance)
    {
        TraceRay ray;
        ray.origin = s.position + s.face_normal * TRACE_SURFACE_OFFSET;
        ray.tmin = TRACE_TMIN;
        ray.direction = l;
        ray.tmax = min(distance, TRACE_TMAX);
        return !trace_any(ray);
    }
    [ForceInline]
    float3 camera_sky(float3 dir)
    {
        float3 cube = tracerSkybox.SampleLevel(tracerIblSampler, dir, 0.0).rgb;
        return drawn_sky(float(traceFrame.lights.w), cube, dir, traceFrame.light_dir.xyz, traceFrame.light_dir.w);
    }
    [ForceInline]
    float3 bounce_sky(float3 dir)
    {
        float3 sky = traceFrame.lights.y != 0u ? tracerPrefiltered.SampleLevel(tracerIblSampler, dir, 0.0).rgb : sky_gradient(dir);
        return sky * traceFrame.light_radiance.w;
    }
    [ForceInline]
    float3 sun_direction()
    {
        return traceFrame.light_dir.xyz;
    }
    [ForceInline]
    float3 sun_radiance()
    {
        return traceFrame.light_radiance.rgb;
    }
    [ForceInline]
    uint punctual_count()
    {
        return traceFrame.lights.x;
    }
    [ForceInline]
    PunctualLight punctual(uint index)
    {
        return tracer_punctual(index);
    }
    [ForceInline]
    bool furnace_on()
    {
        return traceFrame.furnace.w > 0.5;
    }
    [ForceInline]
    float3 furnace_sky()
    {
        return float3(traceFrame.furnace.x);
    }
    [ForceInline]
    float2 furnace_material()
    {
        return traceFrame.furnace.yz;
    }
}
