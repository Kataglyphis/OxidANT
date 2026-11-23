import hdr_safe;
import kernel_vectors;

// Every public hdr_safe.slang function, one thread per case of vectors/hdr_safe.json; both renderers' runners check it.

[shader("compute")]
[numthreads(1, 1, 1)]
void hdr_safe_test_main(uint3 tid : SV_DispatchThreadID)
{
    VectorInputs inputs = vector_inputs(tid.x);
    // Bits, so a case can hand it NaN and the infinities, which JSON cannot spell.
    uint x = inputs.u32();
    uint y = inputs.u32();
    uint z = inputs.u32();

    VectorOutputs outputs = vector_outputs(tid.x);
    outputs.put(hdr_safe(asfloat(uint3(x, y, z))));
}
