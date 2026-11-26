import kernel_vectors;
import sun_mask;

// Every public sun_mask.slang function, one thread per case of vectors/sun_mask.json; both renderers' runners check it.

// One radiance with a disk of another, alike at every mip: a texture-free sky both targets sample alike.
struct DiskSky : ISkyRadiance
{
    float3 sky;
    float3 disk;
    float3 disk_dir;
    float disk_cos;

    float3 radiance(float3 dir, float lod)
    {
        return dot(dir, disk_dir) > disk_cos ? disk : sky;
    }
};

[shader("compute")]
[numthreads(1, 1, 1)]
void sun_mask_test_main(uint3 tid : SV_DispatchThreadID)
{
    VectorInputs inputs = vector_inputs(tid.x);
    DiskSky sky;
    sky.sky = inputs.f32x3();
    sky.disk = inputs.f32x3();
    sky.disk_dir = inputs.f32x3();
    sky.disk_cos = inputs.f32();
    float3 radiance = inputs.f32x3();
    float3 dir = inputs.f32x3();

    VectorOutputs outputs = vector_outputs(tid.x);
    SunMask mask = find_sun(sky);
    outputs.put(mask.dir);
    outputs.put(mask.ring);
    outputs.put(sun_masked(radiance, dir, mask));
    outputs.put(sun_mask_level(radiance));
    outputs.put(sun_search_tangent(dir));
}
