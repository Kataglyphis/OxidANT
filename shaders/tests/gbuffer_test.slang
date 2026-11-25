import gbuffer;
import kernel_vectors;

// Every public gbuffer.slang function, one thread per case of vectors/gbuffer.json.

[shader("compute")]
[numthreads(1, 1, 1)]
void gbuffer_test_main(uint3 tid : SV_DispatchThreadID)
{
    VectorInputs inputs = vector_inputs(tid.x);
    SurfacePoint s;
    s.position = inputs.f32x3();
    s.n = inputs.f32x3();
    s.albedo = inputs.f32x3();
    s.metallic = inputs.f32();
    s.roughness = inputs.f32();
    s.occlusion = inputs.f32();
    s.emissive = inputs.f32x3();
    bool unlit = inputs.u32() != 0u;
    float4 stored = inputs.f32x4();

    GBufferTexels packed = gbuffer_pack(s, unlit);
    // A stored normal texel beside the packed rest, so a zero normal or an unlit sign reaches the reader.
    GBufferTexels read = packed;
    read.normalMetallic = stored;
    SurfacePoint back = gbuffer_unpack(read, s.position);
    VectorOutputs outputs = vector_outputs(tid.x);
    outputs.put(packed.normalMetallic);
    outputs.put(packed.albedoOcclusion);
    outputs.put(packed.roughnessEmissive);
    outputs.put(gbuffer_unlit(stored) ? 1u : 0u);
    outputs.put(back.position);
    outputs.put(back.n);
    outputs.put(back.albedo);
    outputs.put(float3(back.metallic, back.roughness, back.occlusion));
    outputs.put(back.emissive);
}
