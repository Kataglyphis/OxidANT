import brdf;
import kernel_vectors;

// Every public brdf.slang function, one thread per case of vectors/brdf.json; both renderers' runners check it.

[shader("compute")]
[numthreads(1, 1, 1)]
void brdf_test_main(uint3 tid : SV_DispatchThreadID)
{
    VectorInputs inputs = vector_inputs(tid.x);
    float3 n = inputs.f32x3();
    float3 v = inputs.f32x3();
    float3 l = inputs.f32x3();
    float3 albedo = inputs.f32x3();
    float metallic = inputs.f32();
    float roughness = inputs.f32();
    float3 f0 = inputs.f32x3();
    float3 radiance = inputs.f32x3();
    float k = inputs.f32();
    float n_dot_h = inputs.f32();
    float n_dot_v = inputs.f32();
    float n_dot_l = inputs.f32();
    float cos_theta = inputs.f32();
    float2 t = inputs.f32x2();
    float2 energy = inputs.f32x2();

    VectorOutputs outputs = vector_outputs(tid.x);
    outputs.put(lambert_diffuse(albedo));
    outputs.put(distribution_ggx(n_dot_h, roughness));
    outputs.put(smith_k_analytic(roughness));
    outputs.put(smith_k_sampled(roughness));
    outputs.put(geometry_smith_k(n_dot_v, n_dot_l, k));
    outputs.put(fresnel_schlick(cos_theta, f0));
    outputs.put(ggx_single_scatter(n_dot_v, n_dot_l, n_dot_h, cos_theta, roughness, f0, k));
    outputs.put(ggx_energy_node(t));
    outputs.put(ggx_energy_coordinates(n_dot_v, roughness));
    outputs.put(ggx_energy(n_dot_v, roughness));
    outputs.put(multiscatter_gain(energy, f0));
    outputs.put(specular_albedo(energy, f0));
    // The energy a shading point hands its lights, not the energy input, which probes the gain alone.
    float2 surface = ggx_energy(dot(n, v), roughness);
    outputs.put(brdf_smith_k(n, v, l, albedo, metallic, roughness, f0, radiance, k, surface));
    outputs.put(brdf_direct(n, v, l, albedo, metallic, roughness, f0, radiance, surface));
    outputs.put(brdf_sampled(n, v, l, albedo, metallic, roughness, f0, surface));
}
