import exposure;
import kernel_vectors;

// Every public exposure.slang function, one thread per case of vectors/exposure.json; both renderers' runners check it.

[shader("compute")]
[numthreads(1, 1, 1)]
void exposure_test_main(uint3 tid : SV_DispatchThreadID)
{
    VectorInputs inputs = vector_inputs(tid.x);
    float3 color = inputs.f32x3();
    float lum = inputs.f32();
    uint bin = inputs.u32();
    float log2Mean = inputs.f32();
    float currentEv = inputs.f32();
    float targetEv = inputs.f32();
    float dt = inputs.f32();
    float speed = inputs.f32();
    float ev = inputs.f32();

    VectorOutputs outputs = vector_outputs(tid.x);
    outputs.put(exposure_luminance(color));
    outputs.put(histogram_bin(lum));
    outputs.put(bin_luminance(bin));
    outputs.put(exposure_target_ev(log2Mean));
    outputs.put(adapt_exposure_ev(currentEv, targetEv, dt, speed));
    outputs.put(exposure_scale(ev));
}
