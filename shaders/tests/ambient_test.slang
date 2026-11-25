import ambient;
import kernel_vectors;

// Every public ambient.slang function, one thread per case of vectors/ambient.json; both renderers' runners check it.

// Maps that return their own arguments, rescaled, so the vectors check which lookups sky_ambient makes.
struct EchoMaps : IAmbientMaps
{
    float3 irradiance(float3 n)
    {
        return n * 0.5 + 0.5;
    }
    float3 reflection(float3 r, float roughness)
    {
        return (r * 0.5 + 0.5) * (1.0 - 0.5 * roughness);
    }
    float2 energy(float nDotV, float roughness)
    {
        return float2(0.5 * nDotV + 0.25, 0.1 * roughness);
    }
};

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
    float3 irradiance = inputs.f32x3();
    float3 reflection = inputs.f32x3();
    bool baked = inputs.u32() != 0u;
    EchoMaps maps;

    VectorOutputs outputs = vector_outputs(tid.x);
    outputs.put(ambient_split_sum(irradiance, reflection, albedo, metallic, f0, energy));
    outputs.put(analytic_ambient(n, v, albedo, metallic, roughness, f0, energy));
    outputs.put(sky_ambient(maps, baked, n, v, albedo, metallic, roughness, f0, energy));
}
