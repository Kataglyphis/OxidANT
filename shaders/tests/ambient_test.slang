import ambient;
import kernel_vectors;

// Every public ambient.slang function, one thread per case of vectors/ambient.json; both renderers' runners check it.

[shader("compute")]
[numthreads(1, 1, 1)]
void ambient_test_main(uint3 tid : SV_DispatchThreadID)
{
    VectorInputs inputs = vector_inputs(tid.x);
    float3 n = inputs.f32x3();
    float3 v = inputs.f32x3();
    float3 albedo = inputs.f32x3();
    float metallic = inputs.f32();
    float roughness = inputs.f32();
    float3 f0 = inputs.f32x3();
    float2 energy = inputs.f32x2();
    float3 towards_sun = inputs.f32x3();
    float sun_intensity = inputs.f32();
    float3 irradiance = inputs.f32x3();
    float3 reflection = inputs.f32x3();

    VectorOutputs outputs = vector_outputs(tid.x);
    outputs.put(ambient_split_sum(irradiance, reflection, albedo, metallic, f0, energy));
    outputs.put(analytic_ambient(n, v, albedo, metallic, roughness, f0, energy, towards_sun, sun_intensity));
}
