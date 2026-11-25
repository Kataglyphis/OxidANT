// Both renderers' path tracer: a scene gives its rays, its sky and its lights, and trace_path walks one path over them.

import brdf;
import constants;
import path_estimator;
import punctual_light;
import surface_lighting;
import trace_constants;

// A hit as both renderers shade it.
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
    float occlusion;
    bool unlit;
};

// A ray over [tmin, tmax] along a unit direction.
struct TracedSegment
{
    float3 origin;
    float tmin;
    float3 direction;
    float tmax;
};

// A light's unshadowed term at a surface, and the shadow ray that decides it.
struct LightSample
{
    float3 radiance;
    float3 l;
    float distance;
};

// What a renderer gives its path tracer: the C++ engine's over ray queries, OxidANT's over bvh_traverse.
interface ITracedScene
{
    // The closest surface along the segment, shaded at level 0; false when the segment hits nothing.
    bool closest(TracedSegment ray, out TracedSurface s);
    // Whether nothing lies within `distance` along `l` from the surface, MASK cut-outs alpha-tested.
    bool visible(TracedSurface s, float3 l, float distance);
    // What a camera ray that hits nothing sees: the drawn sky.
    float3 camera_sky(float3 dir);
    // What a bounce that misses gets, as the raster's ambient term: the IBL's sharpest level or the analytic sky, scaled.
    float3 bounce_sky(float3 dir);
    // Towards the sun, unit.
    float3 sun_direction();
    float3 sun_radiance();
    uint punctual_count();
    PunctualLight punctual(uint index);
    bool furnace_on();
    // The white furnace's uniform sky.
    float3 furnace_sky();
    // The white furnace's surface: metallic, roughness.
    float2 furnace_material();
}

// What a miss leaves in closest's surface: nothing shades it, and the depth probe reads only its position.
[ForceInline]
TracedSurface no_surface()
{
    TracedSurface s = {};
    return s;
}

[ForceInline]
LightSample unlit_sample()
{
    LightSample o;
    o.radiance = float3(0.0);
    o.l = float3(0.0, 1.0, 0.0);
    o.distance = 0.0;
    return o;
}

// A directional light towards `l`, through the raster's BRDF; energy is the hit's, as brdf_direct takes it.
LightSample sun_sample(TracedSurface s, float3 v, float2 energy, float3 l, float3 radiance)
{
    LightSample o = unlit_sample();
    o.l = l;
    o.distance = TRACE_TMAX;
    if (dot(s.normal, l) <= 0.0)
        return o;
    o.radiance = brdf_direct(s.normal, v, l, s.albedo, s.metallic, s.roughness, surface_f0(s.albedo, s.metallic),
        radiance, energy);
    return o;
}

// A punctual light as punctual_lighting lights it, its shadow ray stopping short of the light.
LightSample punctual_sample(TracedSurface s, float3 v, float2 energy, PunctualLight light)
{
    LightSample o = unlit_sample();
    PunctualIncidence incidence = punctual_incidence(light, s.position);
    o.l = incidence.l;
    o.distance = incidence.distance * TRACE_LIGHT_SPAN;
    if (incidence.attenuation <= 0.0 || dot(s.normal, incidence.l) <= 0.0)
        return o;
    o.radiance = brdf_direct(s.normal, v, incidence.l, s.albedo, s.metallic, s.roughness, surface_f0(s.albedo, s.metallic),
        light.radiance * incidence.attenuation, energy);
    return o;
}

// Its term, or nothing when the shadow ray is blocked; a zero term traces no ray.
float3 shadowed<S : ITracedScene>(S scene, TracedSurface s, LightSample light)
{
    return any(light.radiance > 0.0) && scene.visible(s, light.l, light.distance) ? light.radiance : float3(0.0);
}

// One punctual light picked uniformly, over its 1/count chance; no draw without lights.
LightSample picked_punctual_sample<S : ITracedScene>(S scene, TracedSurface s, float3 v, float2 energy, inout uint rng)
{
    uint count = scene.punctual_count();
    LightSample o = unlit_sample();
    if (count > 0u)
    {
        o = punctual_sample(s, v, energy, scene.punctual(min(uint(rng_next(rng) * float(count)), count - 1u)));
        o.radiance *= float(count);
    }
    return o;
}

// The sun and one punctual light, each through a shadow ray; the furnace has a uniform sky and no lights.
float3 next_event<S : ITracedScene>(S scene, TracedSurface s, float3 v, float2 energy, inout uint rng)
{
    if (scene.furnace_on())
        return float3(0.0);
    LightSample lights[2] = { sun_sample(s, v, energy, scene.sun_direction(), scene.sun_radiance()),
        picked_punctual_sample(scene, s, v, energy, rng) };
    float3 lit = float3(0.0);
    // One shadow-ray call site in a loop: SwiftShader inlines a whole traversal per call site, and compiles for minutes.
    for (uint i = 0u; i < 2u; i++)
        lit += shadowed(scene, s, lights[i]);
    return lit;
}

// The furnace's one white surface everywhere, with its own metallic and roughness, and nothing emitted.
TracedSurface furnace_surface<S : ITracedScene>(S scene, TracedSurface s)
{
    float2 material = scene.furnace_material();
    s.albedo = float3(1.0);
    s.emissive = float3(0.0);
    s.unlit = false;
    s.metallic = material.x;
    s.roughness = clamp(material.y, MIN_ROUGHNESS, 1.0);
    return s;
}

// The sky a path ends in: a camera ray sees the drawn sky; a bounce the ambient term's light, without the sun NEE brings.
float3 path_miss<S : ITracedScene>(S scene, float3 dir, uint bounce)
{
    float3 sky;
    if (scene.furnace_on())
        sky = scene.furnace_sky();
    else if (bounce == 0u)
        sky = scene.camera_sky(dir);
    else
        sky = scene.bounce_sky(dir);
    return sky;
}

// The clouds' depth probe: the unjittered ray through the pixel centre, and the first surface it meets.
struct PathDepth
{
    bool wanted;
    TracedSegment ray;
    bool hit;
    float3 position;
};

PathDepth no_depth()
{
    PathDepth d;
    d.wanted = false;
    d.ray.origin = float3(0.0);
    d.ray.tmin = 0.0;
    d.ray.direction = float3(0.0, 0.0, 1.0);
    d.ray.tmax = 0.0;
    d.hit = false;
    d.position = float3(0.0);
    return d;
}

// One path of at most `max_bounces` segments from `camera`, a ray from the near plane jittered inside its pixel.
float3 trace_path<S : ITracedScene>(S scene, TracedSegment camera, uint max_bounces, inout uint rng, inout PathDepth depth)
{
    // A wanted depth probe takes the loop's first pass: SwiftShader inlines one traversal per closest-hit call site.
    bool probing = depth.wanted;
    TracedSegment ray = probing ? depth.ray : camera;
    float3 throughput = float3(1.0);
    float3 radiance = float3(0.0);
    uint bounce = 0u;
    while (probing || bounce < max_bounces)
    {
        if (!probing && !russian_roulette_survives(bounce, throughput, rng))
            break;
        TracedSurface s;
        bool hit = scene.closest(ray, s);
        if (probing)
        {
            depth.hit = hit;
            depth.position = s.position;
            probing = false;
            ray = camera;
            continue;
        }
        if (!hit)
        {
            radiance += throughput * path_miss(scene, ray.direction, bounce);
            break;
        }
        if (scene.furnace_on())
            s = furnace_surface(scene, s);
        // KHR_materials_unlit ends the path at its albedo; emission joins before this surface absorbs.
        if (s.unlit)
        {
            radiance += throughput * s.albedo;
            break;
        }
        float3 v = -ray.direction;
        // A normal tilted away from the viewer leaves no lobe to shade, so the face normal shades; raster has no such rule.
        if (dot(s.normal, v) < 1e-4)
            s.normal = s.face_normal;
        // The fit once per hit: both lights and the bounce share it.
        float2 energy = ggx_energy(dot(s.normal, v), s.roughness);
        radiance += throughput * (s.emissive + next_event(scene, s, v, energy, rng));
        Bounce next = sample_bounce(s.normal, s.face_normal, v, s.albedo, s.metallic, s.roughness, energy, rng);
        if (all(next.weight == float3(0.0)))
            break;
        throughput *= next.weight;
        ray.origin = s.position + s.face_normal * TRACE_SURFACE_OFFSET;
        ray.tmin = TRACE_TMIN;
        // sample_bounce normalizes it, which tmin and tmax assume.
        ray.direction = next.direction;
        ray.tmax = TRACE_TMAX;
        bounce++;
    }
    return radiance;
}
