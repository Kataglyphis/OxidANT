import clip_y;
import fullscreen;
import kernel_vectors;

// Every public fullscreen.slang function, one thread per case of vectors/fullscreen.json; both renderers' runners check it.

[shader("compute")]
[numthreads(1, 1, 1)]
void fullscreen_test_main(uint3 tid : SV_DispatchThreadID)
{
    VectorInputs inputs = vector_inputs(tid.x);
    uint vid = inputs.u32();
    float2 uv = inputs.f32x2();
    float2 ndc = inputs.f32x2();

    VectorOutputs outputs = vector_outputs(tid.x);
    // The uv differs per target, so each output is one both share: the positions, x, the round trips and the top row.
    FullscreenVsOut triangle = fullscreen_vs(vid);
    outputs.put(triangle.svPosition);
    outputs.put(fullscreen_uv_to_ndc(triangle.uv));
    outputs.put(fullscreen_ndc_to_uv(triangle.svPosition.xy) - triangle.uv);
    outputs.put(fullscreen_uv_to_ndc(uv).x);
    outputs.put(fullscreen_ndc_to_uv(fullscreen_uv_to_ndc(uv)));
    outputs.put(fullscreen_ndc_to_uv(ndc).x);
    outputs.put(fullscreen_uv_to_ndc(fullscreen_ndc_to_uv(ndc)));
    // Zero on both targets only while each target's uv (0, 0) is its own top-left texel.
    float top = top_row_clip_y();
    outputs.put(triangle.uv.y - 0.5 * (1.0 - triangle.svPosition.y * top));
    outputs.put(fullscreen_uv_to_ndc(float2(uv.x, 0.0)).y - top);
    outputs.put(fullscreen_ndc_to_uv(float2(ndc.x, top)).y);
    // The sign against fullscreen_ndc_to_uv's own y step, zero on each target only while the two agree.
    outputs.put((fullscreen_ndc_to_uv(float2(ndc.x, ndc.y + 1.0)).y - fullscreen_ndc_to_uv(ndc).y) * 2.0
                - fullscreen_ndc_uv_y_sign());
}
