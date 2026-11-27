import kernel_vectors;
import surface_lighting;

// Every public surface_lighting.slang function, one thread per case of vectors/surface_lighting.json.

[shader("compute")]
[numthreads(1, 1, 1)]
void surface_lighting_test_main(uint3 tid : SV_DispatchThreadID)
{
    VectorInputs inputs = vector_inputs(tid.x);
    float3 albedo = inputs.f32x3();
    float metallic = inputs.f32();
    float3 sun = inputs.f32x3();
    float visibility = inputs.f32();
    float strength = inputs.f32();
    float3 punctual = inputs.f32x3();
    float3 ambient = inputs.f32x3();
    float3 emissive = inputs.f32x3();
    float occlusion = inputs.f32();
    float occlusionStrength = inputs.f32();
    uint blended = inputs.u32();

    VectorOutputs outputs = vector_outputs(tid.x);
    outputs.put(surface_f0(albedo, metallic));
    outputs.put(surface_lighting(sun, visibility, strength, punctual, ambient, emissive));
    outputs.put(screen_space_occlusion(occlusion, occlusionStrength, blended != 0u));
}
