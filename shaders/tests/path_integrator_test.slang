import brdf;
import kernel_vectors;
import path_integrator;
import punctual_light;

// Every public path_integrator.slang function, one thread per case of vectors/path_integrator.json; both renderers' runners check it.

// A floor at y = 0 seen from above, under a sky: every hit is the case's surface, and a wall blocks lights beyond l.x 0.5.
struct FloorScene : ITracedScene
{
    TracedSurface floor;
    float3 sun_l;
    float3 sun_rad;
    PunctualLight light;
    uint lights;
    float4 furnace;

    bool closest(TracedSegment ray, out TracedSurface s)
    {
        float t = -ray.origin.y / ray.direction.y;
        bool found = ray.direction.y < 0.0 && t >= ray.tmin && t <= ray.tmax;
        s = floor;
        s.position = found ? ray.origin + ray.direction * t : float3(0.0);
        return found;
    }
    bool visible(TracedSurface s, float3 l, float distance)
    {
        return l.x < 0.5;
    }
    float3 camera_sky(float3 dir)
    {
        return dir * 0.5 + 0.5;
    }
    float3 bounce_sky(float3 dir)
    {
        return (dir * 0.5 + 0.5) * 0.25;
    }
    float3 sun_direction()
    {
        return sun_l;
    }
    float3 sun_radiance()
    {
        return sun_rad;
    }
    uint punctual_count()
    {
        return lights;
    }
    PunctualLight punctual(uint index)
    {
        return light;
    }
    bool furnace_on()
    {
        return furnace.w > 0.5;
    }
    float3 furnace_sky()
    {
        return float3(furnace.x);
    }
    float2 furnace_material()
    {
        return furnace.yz;
    }
};

[shader("compute")]
[numthreads(1, 1, 1)]
void path_integrator_test_main(uint3 tid : SV_DispatchThreadID)
{
    VectorInputs inputs = vector_inputs(tid.x);
    FloorScene scene;
    scene.floor = no_surface();
    scene.floor.face_normal = float3(0.0, 1.0, 0.0);
    scene.floor.normal = inputs.f32x3();
    scene.floor.albedo = inputs.f32x3();
    scene.floor.emissive = inputs.f32x3();
    scene.floor.metallic = inputs.f32();
    scene.floor.roughness = inputs.f32();
    scene.floor.occlusion = 1.0;
    scene.floor.unlit = inputs.u32() != 0u;
    scene.sun_l = inputs.f32x3();
    scene.sun_rad = inputs.f32x3();
    float4 a = inputs.f32x4();
    float4 b = inputs.f32x4();
    float4 c = inputs.f32x4();
    float4 d = inputs.f32x4();
    scene.light = unpack_punctual_light(a, b, c, d);
    scene.lights = inputs.u32();
    scene.furnace = inputs.f32x4();
    TracedSegment camera;
    camera.origin = inputs.f32x3();
    camera.tmin = 0.0;
    camera.direction = inputs.f32x3();
    camera.tmax = inputs.f32();
    uint max_bounces = inputs.u32();
    uint rng = inputs.u32();
    bool probe = inputs.u32() != 0u;
    TracedSurface s = scene.floor;
    s.position = inputs.f32x3();
    float3 v = inputs.f32x3();
    float2 energy = ggx_energy(dot(s.normal, v), s.roughness);

    VectorOutputs outputs = vector_outputs(tid.x);
    TracedSurface none = no_surface();
    outputs.put(float4(none.position.x + none.normal.y + none.face_normal.z, none.albedo.x + none.emissive.y, none.roughness + none.metallic + none.occlusion, float(none.unlit)));
    LightSample unlit = unlit_sample();
    outputs.put(float4(unlit.radiance, unlit.distance));
    outputs.put(unlit.l);
    LightSample sun = sun_sample(s, v, energy, scene.sun_l, scene.sun_rad);
    outputs.put(sun.radiance);
    outputs.put(sun.l);
    outputs.put(sun.distance);
    LightSample punctual = punctual_sample(s, v, energy, scene.light);
    outputs.put(punctual.radiance);
    outputs.put(punctual.l);
    outputs.put(punctual.distance);
    outputs.put(shadowed(scene, s, sun));
    outputs.put(shadowed(scene, s, punctual));
    uint picked_rng = rng;
    outputs.put(picked_punctual_sample(scene, s, v, energy, picked_rng).radiance);
    outputs.put(picked_rng);
    uint event_rng = rng;
    outputs.put(next_event(scene, s, v, energy, event_rng));
    outputs.put(event_rng);
    TracedSurface white = furnace_surface(scene, s);
    outputs.put(float4(white.albedo.x, white.emissive.x, white.metallic, white.roughness));
    outputs.put(float(white.unlit));
    outputs.put(path_miss(scene, camera.direction, 0u));
    outputs.put(path_miss(scene, camera.direction, 1u));
    PathDepth depth = no_depth();
    outputs.put(uint(depth.wanted) + 2u * uint(depth.hit));
    depth.wanted = probe;
    depth.ray = camera;
    uint path_rng = rng;
    outputs.put(trace_path(scene, camera, max_bounces, path_rng, depth));
    outputs.put(path_rng);
    outputs.put(uint(depth.hit));
    outputs.put(depth.position);
}
