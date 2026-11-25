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

// A renderer's baked environment: its irradiance and prefiltered cubes and the split sum's table.
interface IAmbientMaps
{
    float3 irradiance(float3 n);
    // roughness picks the prefiltered mip.
    float3 reflection(float3 r, float roughness);
    float2 energy(float nDotV, float roughness);
}

// The sky's light on a surface: the baked maps when there are some, else the analytic sky with the surface's energy.
float3 sky_ambient<M : IAmbientMaps>(M maps, bool baked, float3 n, float3 v, float3 albedo, float metallic,
    float roughness, float3 f0, float2 surfaceEnergy)
{
    // A uniform flag: every invocation takes one side, so none pays for the other's samples or sky.
    if (baked)
    {
        float2 energy = maps.energy(max(dot(n, v), 0.0), roughness);
        return ambient_split_sum(maps.irradiance(n), maps.reflection(reflect(-v, n), roughness), albedo, metallic, f0,
            energy);
    }
    return analytic_ambient(n, v, albedo, metallic, roughness, f0, surfaceEnergy);
}
