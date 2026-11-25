import fullscreen;
import kernel_vectors;
import punctual_shadow;

// Every public punctual_shadow.slang function, one thread per case of vectors/punctual_shadow.json.

// A cube whose lit fraction falls with depth and varies smoothly with the direction, the same on both targets.
struct RampTaps : IPunctualShadowTaps
{
    float lit(float3 d, float reference)
    {
        return saturate(0.5 + 0.1 * (d.x + 0.5 * d.y - 0.25 * d.z) - 4.0 * (reference - 0.9));
    }
};

[shader("compute")]
[numthreads(1, 1, 1)]
void punctual_shadow_test_main(uint3 tid : SV_DispatchThreadID)
{
    VectorInputs inputs = vector_inputs(tid.x);
    float4 row = inputs.f32x4();
    float majorAxis = inputs.f32();
    float3 r = inputs.f32x3();
    float3 lightPos = inputs.f32x3();
    float3 worldPos = inputs.f32x3();
    float3 n = inputs.f32x3();
    float3 l = inputs.f32x3();
    float width = inputs.f32();

    PunctualShadow shadow = unpack_punctual_shadow(row);
    PunctualShadowFace face = punctual_shadow_face(r);
    float3 coord = punctual_shadow_face_coord(r, max(shadow.slot, 0));
    RampTaps taps;
    VectorOutputs outputs = vector_outputs(tid.x);
    outputs.put(uint(shadow.slot + 1));
    outputs.put(float2(shadow.nearPlane, shadow.farPlane));
    outputs.put(punctual_shadow_depth(majorAxis, shadow.nearPlane, shadow.farPlane));
    outputs.put(face.face);
    outputs.put(face.ndc);
    // The uv differs per target, so its NDC is what both share.
    outputs.put(fullscreen_uv_to_ndc(coord.xy));
    outputs.put(coord.z);
    outputs.put(punctual_shadow_visibility(taps, shadow, lightPos, worldPos, n, l, width));
}
