import brdf;
import sky_model;

// The sky's share of a surface's light, which both renderers' raster and ray-traced modes add after shadowing.

// The split sum's combine: the diffuse lobe takes 1 - E_spec of the irradiance, the specular lobe E_spec of the reflection.
float3 ambient_split_sum(float3 irradiance, float3 reflection, float3 albedo, float metallic, float3 f0, float2 energy)
{
    float3 specularAlbedo = specular_albedo(energy, f0);
    float3 diffuse = saturate(float3(1.0) - specularAlbedo) * (1.0 - metallic) * albedo * irradiance;
    return diffuse + reflection * specularAlbedo;
}

// Without a baked environment; no sun disk: brdf_direct's lobe carries the sun, and its shadow must take the glint.
float3 analytic_ambient(float3 n, float3 v, float3 albedo, float metallic, float roughness, float3 f0, float2 energy)
{
    float3 irradiance = hemisphere_irradiance(n);
    float3 reflection = lerp(sky_gradient(reflect(-v, n)), irradiance, roughness * roughness);
    return ambient_split_sum(irradiance, reflection, albedo, metallic, f0, energy);
}
