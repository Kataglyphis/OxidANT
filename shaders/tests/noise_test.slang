import noise;
import kernel_vectors;

// Every public noise.slang function, one thread per case of vectors/noise.json; both renderers' runners check it.

[shader("compute")]
[numthreads(1, 1, 1)]
void noise_test_main(uint3 tid : SV_DispatchThreadID)
{
    VectorInputs inputs = vector_inputs(tid.x);
    float3 p = inputs.f32x3();
    float4 x = inputs.f32x4();
    float freq = inputs.f32();
    int octaves = inputs.i32();

    VectorOutputs outputs = vector_outputs(tid.x);
    outputs.put(mod289(x.xyz));
    outputs.put(mod289(x));
    outputs.put(permute(x));
    outputs.put(taylorInvSqrt(x));
    outputs.put(snoise(p));
    outputs.put(fbm(p, freq, octaves));
}
