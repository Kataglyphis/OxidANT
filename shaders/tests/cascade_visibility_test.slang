import cascade_visibility;
import fullscreen;
import kernel_vectors;

// Every public cascade_visibility.slang function, one thread per case of vectors/cascade_visibility.json.

// A map whose lit fraction falls with depth and rises along x, which both targets' uv share.
struct RampTaps : IShadowTaps
{
    float lit(float2 uv, float reference)
    {
        return saturate(0.5 + 2.0 * (uv.x - 0.5) - 4.0 * (reference - 0.5));
    }
};

[shader("compute")]
[numthreads(1, 1, 1)]
void cascade_visibility_test_main(uint3 tid : SV_DispatchThreadID)
{
    VectorInputs inputs = vector_inputs(tid.x);
    float4 row0 = inputs.f32x4();
    float4 row1 = inputs.f32x4();
    float4 row2 = inputs.f32x4();
    float4 row3 = inputs.f32x4();
    float3 world = inputs.f32x3();
    float viewDepth = inputs.f32();
    float2 splits = inputs.f32x2();
    int count = inputs.i32();
    float3 n = inputs.f32x3();
    float nDotL = inputs.f32();
    float biasScale = inputs.f32();
    float2 texel = inputs.f32x2();
    int radius = inputs.i32();

    float4x4 lightSpace = float4x4(row0, row1, row2, row3);
    CascadeHit hit = cascade_hit(lightSpace, world);
    RampTaps taps;
    VectorOutputs outputs = vector_outputs(tid.x);
    outputs.put(uint(select_cascade(viewDepth, splits, count)));
    // The uv differs per target, so its NDC is what both share.
    outputs.put(fullscreen_uv_to_ndc(hit.uv));
    outputs.put(hit.depth);
    outputs.put(hit.inside ? 1u : 0u);
    outputs.put(cascade_bias(nDotL, biasScale));
    // y in NDC's direction on both targets only while the slope takes the target's sign.
    outputs.put(cascade_depth_slope(lightSpace, n) * float2(1.0, fullscreen_ndc_uv_y_sign()));
    outputs.put(cascade_visibility(taps, lightSpace, hit, n, nDotL, biasScale, texel, radius));
}
