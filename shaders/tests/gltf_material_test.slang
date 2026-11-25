import gltf_material;
import kernel_vectors;

// Every public gltf_material.slang function, one thread per case of vectors/gltf_material.json; both renderers' runners check it.

[shader("compute")]
[numthreads(1, 1, 1)]
void gltf_material_test_main(uint3 tid : SV_DispatchThreadID)
{
    VectorInputs inputs = vector_inputs(tid.x);
    float2 uv = inputs.f32x2();
    float3 row0 = inputs.f32x3();
    float3 row1 = inputs.f32x3();
    float3 v = inputs.f32x3();
    float3 fallback = inputs.f32x3();
    float3 texel = inputs.f32x3();
    float scale = inputs.f32();
    float3 normal = inputs.f32x3();
    float4 tangent = inputs.f32x4();
    float3 tangent_normal = inputs.f32x3();
    bool front_face = inputs.u32() != 0u;
    float3 m0 = inputs.f32x3();
    float3 m1 = inputs.f32x3();
    float3 m2 = inputs.f32x3();
    float factor = inputs.f32();
    float sampled = inputs.f32();
    int mode = inputs.i32();
    float alpha = inputs.f32();
    float cutoff = inputs.f32();

    float3x3 m = float3x3(m0, m1, m2);
    float3x3 normal_m = normal_transform(m);
    VectorOutputs outputs = vector_outputs(tid.x);
    outputs.put(transform_uv(uv, row0, row1));
    outputs.put(normalize_or(v, fallback));
    outputs.put(tangent_space_normal(texel, scale));
    outputs.put(apply_normal_map(normal, tangent, tangent_normal, front_face));
    outputs.put(mirror_sign(m));
    outputs.put(normal_m[0]);
    outputs.put(normal_m[1]);
    outputs.put(normal_m[2]);
    outputs.put(material_metallic(factor, sampled));
    outputs.put(material_roughness(factor, sampled));
    outputs.put(alpha_masked_out(mode, alpha, cutoff) ? 1u : 0u);
    outputs.put(material_occlusion(factor, sampled));
}
