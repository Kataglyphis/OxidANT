import bloom_filter;
import kernel_vectors;

// Every public bloom_filter.slang function, one thread per case of vectors/bloom_filter.json; both renderers' runners check it.

[shader("compute")]
[numthreads(1, 1, 1)]
void bloom_filter_test_main(uint3 tid : SV_DispatchThreadID)
{
    VectorInputs inputs = vector_inputs(tid.x);
    float3 exposed = inputs.f32x3();
    int offset = inputs.i32();

    VectorOutputs outputs = vector_outputs(tid.x);
    outputs.put(bloom_bright(exposed));
    outputs.put(bloom_tap_weight(offset));
    float sum = 0.0;
    for (int i = -BLOOM_TAP_RADIUS - 1; i <= BLOOM_TAP_RADIUS + 1; i++)
        sum += bloom_tap_weight(i);
    outputs.put(sum);
}
