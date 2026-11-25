import hdr_safe;

// How both renderers' raster and deferred passes combine a surface point's lighting terms.

// What lighting needs of a surface point, whether a forward pass sampled it or a deferred pass read it back.
struct SurfacePoint
{
    float3 position;
    float3 n;
    float3 albedo;
    float metallic;
    float roughness;
    float occlusion;
    float3 emissive;
};

// Normal-incidence reflectance: 4 % for a dielectric, the albedo for a metal.
float3 surface_f0(float3 albedo, float metallic)
{
    return lerp(float3(0.04), albedo, metallic);
}

// The lit HDR colour: the sun's direct light shadowed by strength, then the unshadowed punctual, ambient and emissive.
float3 surface_lighting(float3 sunDirect, float sunVisibility, float shadowStrength, float3 punctual, float3 ambient,
    float3 emissive)
{
    return hdr_safe(sunDirect * lerp(1.0, sunVisibility, shadowStrength) + punctual + ambient + emissive);
}
