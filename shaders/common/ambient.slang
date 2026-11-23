import brdf;
import sky_model;

// The sky's share of a surface's light, which both renderers' raster passes and the C++ ray-traced mode add after shadowing.

// The split sum's combine: the diffuse lobe takes 1 - E_spec of the irradiance, the specular lobe E_spec of the reflection.
float3 ambient_split_sum(float3 irradiance, float3 reflection, float3 albedo, float metallic, float3 f0, float2 energy)
{
    float3 specularAlbedo = specular_albedo(energy, f0);
    float3 diffuse = saturate(float3(1.0) - specularAlbedo) * (1.0 - metallic) * albedo * irradiance;
    return diffuse + reflection * specularAlbedo;
}

// Without a baked environment: the analytic sky's irradiance, and its reflection with the sun blurred towards it by roughness.
float3 analytic_ambient(float3 n, float3 v, float3 albedo, float metallic, float roughness, float3 f0, float2 energy,
    float3 towardsSun, float sunIntensity)
{
    float3 irradiance = hemisphere_irradiance(n);
    float3 reflected = reflect(-v, n);
    float3 sky = sky_gradient(reflected) + sun_disk(reflected, towardsSun, sunIntensity);
    float3 reflection = lerp(sky, irradiance, roughness * roughness);
    return ambient_split_sum(irradiance, reflection, albedo, metallic, f0, energy);
}
