import path_estimator;

// CI-only entry point: proves path_estimator.slang emits SPIR-V, as the C++ path_tracing.slang imports it, and WGSL; nothing loads it.

RWStructuredBuffer<float4> outBuf;

[shader("compute")]
[numthreads(64, 1, 1)]
void path_estimator_test_main(uint3 tid : SV_DispatchThreadID)
{
    uint rng = path_rng_seed(tid.x, 3u);
    float3 throughput = float3(0.5);
    bool alive = russian_roulette_survives(4u, throughput, rng);
    float3 n = normalize(float3(0.2, 1.0, -0.1));
    float3 v = normalize(float3(0.0, 1.0, 1.0));
    float3 albedo = float3(0.8, 0.2, 0.1);
    Bounce next = sample_bounce(n, float3(0.0, 1.0, 0.0), v, albedo, 0.5, 0.4, rng);
    float3 t;
    float3 b;
    orthonormal_basis(n, t, b);
    float3 h = sample_ggx_visible_normal(float3(0.0, 0.6, 0.8), 0.16, float2(rng_next(rng), rng_next(rng)));
    float pdf = ggx_visible_normal_pdf(0.8, h.z, 0.16);
    float chance = specular_lobe_probability(albedo, 0.5, lerp(float3(0.04), albedo, 0.5), 0.8);
    outBuf[tid.x] = float4(next.weight * throughput + next.direction + t + b, alive ? pdf * chance : 0.0);
}
