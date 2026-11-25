import drawn_sky;
import kernel_vectors;

// Every public drawn_sky.slang function, one thread per case of vectors/drawn_sky.json; both renderers' runners check it.

[shader("compute")]
[numthreads(1, 1, 1)]
void drawn_sky_test_main(uint3 tid : SV_DispatchThreadID)
{
    VectorInputs inputs = vector_inputs(tid.x);
    float mode = inputs.f32();
    float3 cube = inputs.f32x3();
    float3 dir = inputs.f32x3();
    float3 light_dir = inputs.f32x3();
    float sun_intensity = inputs.f32();

    VectorOutputs outputs = vector_outputs(tid.x);
    outputs.put(drawn_sky(mode, cube, dir, light_dir, sun_intensity));
}
