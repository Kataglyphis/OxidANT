// What the ray- and path-traced kernels share: their bindings, the camera's rays, a hit's material as the forward pass reads it, and the sky.

import bvh_traverse;
import brdf;
import camera_ray;
import gltf_material;
import path_estimator;
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
    uint4 lights;            // punctual light count, 1 when an environment is bound, unused, unused
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
// The raster's punctual lights and IBL environment, at render::tracer::modes::TRACE_LIGHTS_GROUP.
[vk::binding(0, 2)] StructuredBuffer<float4> tracerLights;
[vk::binding(1, 2)] TextureCube<float4> tracerEnvironment;

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
    TracedSurface o;
    o.position = s.position;
    o.face_normal = s.front_face ? s.geometric_normal : -s.geometric_normal;
    float3 n = apply_normal_map(s.normal, s.tangent, tangent_space_normal(normal.xyz, m.normal_scale), s.front_face);
    // A normal tilted away from the viewer leaves no lobe to shade, so the face normal shades, as in the C++ path tracer.
    o.normal = dot(n, -ray.direction) < 1e-4 ? o.face_normal : n;
    o.albedo = m.base_color.rgb * base.rgb * s.color.rgb;
    o.emissive = m.emissive * emission.rgb;
    o.metallic = material_metallic(m.metallic, mr.b);
    o.roughness = material_roughness(m.roughness, mr.g);
    o.unlit = (m.flags & MATERIAL_UNLIT) != 0u;
    return o;
}

// Whether a light `distance` along `l` reaches the surface: any hit on the way blocks it, MASK cut-outs alpha-tested.
bool light_visible(TracedSurface s, float3 l, float distance)
{
    TraceRay ray;
    ray.origin = s.position + s.face_normal * TRACE_SURFACE_OFFSET;
    ray.tmin = TRACE_TMIN;
    ray.direction = l;
    ray.tmax = min(distance, TRACE_TMAX);
    return !trace_any(ray);
}

// A light's unshadowed term at a surface, and the shadow ray that decides it.
struct LightSample
{
    float3 radiance;
    float3 l;
    float distance;
};

LightSample unlit_sample()
{
    LightSample o;
    o.radiance = float3(0.0);
    o.l = float3(0.0, 1.0, 0.0);
    o.distance = 0.0;
    return o;
}

// Its term, or nothing when the shadow ray is blocked; a zero term traces no ray.
float3 shadowed(TracedSurface s, LightSample light)
{
    return any(light.radiance > 0.0) && light_visible(s, light.l, light.distance) ? light.radiance : float3(0.0);
}

// The sun's GGX and Lambert term through the forward pass's BRDF; energy is the hit's, as brdf_direct takes it.
LightSample sun_sample(TracedSurface s, float3 v, float2 energy)
{
    LightSample o = unlit_sample();
    o.l = traceFrame.light_dir.xyz;
    o.distance = TRACE_TMAX;
    if (dot(s.normal, o.l) <= 0.0)
        return o;
    float3 f0 = surface_f0(s.albedo, s.metallic);
    o.radiance = brdf_direct(s.normal, v, o.l, s.albedo, s.metallic, s.roughness, f0, traceFrame.light_radiance.rgb, energy);
    return o;
}

// The raster sky, sun disk included: what a camera ray that hits nothing sees.
float3 sky_with_sun(float3 dir)
{
    return sky_gradient(dir) + sun_disk(dir, traceFrame.light_dir.xyz, traceFrame.light_dir.w);
}

// Punctual light `index` of the raster's buffer, lit as punctual_lighting lights it.
LightSample punctual_sample(TracedSurface s, float3 v, float2 energy, uint index)
{
    LightSample o = unlit_sample();
    PunctualLight light = unpack_punctual_light(tracerLights[index * 4u], tracerLights[index * 4u + 1u],
        tracerLights[index * 4u + 2u], tracerLights[index * 4u + 3u]);
    PunctualIncidence incidence = punctual_incidence(light, s.position);
    o.l = incidence.l;
    o.distance = incidence.distance * TRACE_LIGHT_SPAN;
    if (incidence.attenuation <= 0.0 || dot(s.normal, incidence.l) <= 0.0)
        return o;
    float3 f0 = surface_f0(s.albedo, s.metallic);
    o.radiance = brdf_direct(s.normal, v, incidence.l, s.albedo, s.metallic, s.roughness, f0, light.radiance * incidence.attenuation,
        energy);
    return o;
}

// One punctual light picked uniformly, over its 1/count chance.
LightSample punctual_light_sample(TracedSurface s, float3 v, float2 energy, inout uint rng)
{
    uint count = traceFrame.lights.x;
    if (count == 0u)
        return unlit_sample();
    LightSample o = punctual_sample(s, v, energy, min(uint(rng_next(rng) * float(count)), count - 1u));
    o.radiance *= float(count);
    return o;
}

// The light a bounce that misses gets, as the raster's ambient term: the bound environment, else the sky gradient.
float3 bounce_sky(float3 dir)
{
    float3 sky = traceFrame.lights.y != 0u ? tracerEnvironment.SampleLevel(tracerLinearSampler, dir, 0.0).rgb : sky_gradient(dir);
    return sky * traceFrame.light_radiance.w;
}
