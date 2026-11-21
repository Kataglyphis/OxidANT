import path_estimator;
import kernel_vectors;

// Every public path_estimator.slang function, one thread per case of vectors/path_estimator.json; both renderers' runners check it.

[shader("compute")]
[numthreads(1, 1, 1)]
void path_estimator_test_main(uint3 tid : SV_DispatchThreadID)
{
    VectorInputs inputs = vector_inputs(tid.x);
    uint pixel_index = inputs.u32();
    uint stream = inputs.u32();
    uint rng = inputs.u32();
    uint bounce = inputs.u32();
    float3 throughput = inputs.f32x3();
    float3 n = inputs.f32x3();
    float3 face_normal = inputs.f32x3();
    float3 v = inputs.f32x3();
    float3 albedo = inputs.f32x3();
    float metallic = inputs.f32();
    float roughness = inputs.f32();
    float3 f0 = inputs.f32x3();
    float3 local_v = inputs.f32x3();
    float alpha = inputs.f32();
    float2 u = inputs.f32x2();
    float n_dot_v = inputs.f32();
    float n_dot_h = inputs.f32();

    VectorOutputs outputs = vector_outputs(tid.x);
    outputs.put(path_rng_seed(pixel_index, stream));
    uint drawn_rng = rng;
    float drawn = rng_next(drawn_rng);
    outputs.put(drawn);
    outputs.put(drawn_rng);
    uint roulette_rng = rng;
    float3 kept = throughput;
    bool alive = russian_roulette_survives(bounce, kept, roulette_rng);
    outputs.put(alive ? 1u : 0u);
    outputs.put(kept);
    outputs.put(roulette_rng);
    float3 t;
    float3 b;
    orthonormal_basis(n, t, b);
    outputs.put(t);
    outputs.put(b);
    outputs.put(sample_ggx_visible_normal(local_v, alpha, u));
    outputs.put(ggx_visible_normal_pdf(n_dot_v, n_dot_h, alpha));
    outputs.put(specular_lobe_probability(albedo, metallic, f0, n_dot_v));
    uint bounce_rng = rng;
    Bounce next = sample_bounce(n, face_normal, v, albedo, metallic, roughness, bounce_rng);
    outputs.put(next.direction);
    outputs.put(next.weight);
    outputs.put(bounce_rng);
}
