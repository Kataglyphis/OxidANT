import gltf_material;
// Callers build and read surface_lighting.slang's SurfacePoint.
__exported import surface_lighting;

// Both renderers' G-buffer layout, 24 bytes a sample under WebGPU's 32: rgba16f normal, rgba8 sRGB albedo, rgba16f.

// The three texels a geometry pass writes and a lighting pass reads back at one pixel.
struct GBufferTexels
{
    float4 normalMetallic;
    float4 albedoOcclusion;
    float4 roughnessEmissive;
};

// metallic's sign carries unlit, since albedo's alpha holds the occlusion; half floats keep emissive above 1.
GBufferTexels gbuffer_pack(SurfacePoint s, bool unlit)
{
    GBufferTexels t;
    t.normalMetallic = float4(s.n, unlit ? -1.0 : s.metallic);
    t.albedoOcclusion = float4(s.albedo, s.occlusion);
    t.roughnessEmissive = float4(s.roughness, s.emissive);
    return t;
}

// An unlit surface: the lighting pass shows its albedo as stored.
bool gbuffer_unlit(float4 normalMetallic)
{
    return normalMetallic.w < -0.5;
}

// The surface a lit pixel's texels describe at position; a zero normal faces up.
SurfacePoint gbuffer_unpack(GBufferTexels t, float3 position)
{
    SurfacePoint s;
    s.position = position;
    s.n = normalize_or(t.normalMetallic.xyz, float3(0.0, 1.0, 0.0));
    s.albedo = t.albedoOcclusion.rgb;
    s.metallic = t.normalMetallic.w;
    s.roughness = t.roughnessEmissive.r;
    s.occlusion = t.albedoOcclusion.a;
    s.emissive = t.roughnessEmissive.gba;
    return s;
}
