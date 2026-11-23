import aces;
import kernel_vectors;

// Every public aces.slang function, one thread per case of vectors/aces.json; both renderers' runners check it.

[shader("compute")]
[numthreads(1, 1, 1)]
void aces_test_main(uint3 tid : SV_DispatchThreadID)
{
    VectorInputs inputs = vector_inputs(tid.x);
    float3 radiance = inputs.f32x3();
    float3 display = inputs.f32x3();
    float3 encoded = inputs.f32x3();

    VectorOutputs outputs = vector_outputs(tid.x);
    outputs.put(aces_tonemap(radiance));
    outputs.put(aces_tonemap_inverse(display));
    outputs.put(aces_tonemap(aces_tonemap_inverse(display)));
    outputs.put(linear_to_srgb(encoded));
}
