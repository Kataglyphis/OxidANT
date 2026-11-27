import ambient;
import brdf;
import cascade_visibility;
import limits;
import punctual_light;
import punctual_shadow;
__exported import surface_lighting;

// The lighting forward.slang's forward and deferred lighting passes share, so the two paths cannot drift apart.

// Must match render::tile_grid::TILE_SIZE, the CPU's light-binning tile size.
static const uint TILE_SIZE = 16;
// Must match render::tile_grid::MAX_LIGHTS_PER_TILE: a tile counting more lists none.
static const uint MAX_LIGHTS_PER_TILE = 32;


struct FrameUniforms
{
    float4x4 view_proj;
    float4x4 light_space;
    float4x4 light_space_1;
    float4x4 light_space_2;
    float4 light_dir_ambient;  // xyz: dir towards light, w: ambient
    float4 light_color_intensity; // rgb: color, w: intensity
    float4 camera_position;    // xyz: pos, w: punctual light count
    float4 cascade_splits;     // x,y: splits, z: tile_w, w: tile_h
    float4 shadow_params;      // x: cascades (0 = off), y: PCF radius, z: strength, w: punctual cubes drawn
    float4 shadow_view_dir;    // xyz: camera forward, the axis the splits are measured on
    float4 shadow_bias_scale;  // xyz: per cascade, the C++ box's depth range over this one's
    float4 ambient_occlusion;  // x: SSAO strength (0 off), yz: 1 / the scene targets' size
};

struct IblParams
{
    float4 enabled_maxmip_intensity;
};

// Group 0 of both passes, beside each pass's own group 0 bindings.
[vk::binding(1, 0)] Texture2DArray<float> shadowMap;
[vk::binding(2, 0)] SamplerComparisonState shadowSampler;

[vk::binding(0, 1)] ConstantBuffer<IblParams> iblParams;
[vk::binding(1, 1)] TextureCube<float4> irradianceMap;
[vk::binding(2, 1)] TextureCube<float4> prefilteredMap;
[vk::binding(3, 1)] Texture2D<float4> brdfLut;
[vk::binding(4, 1)] SamplerState iblSampler;

[vk::binding(0, 2)] ConstantBuffer<FrameUniforms> frame;
// SSAO's half-resolution occlusion of this frame's depth, which only the ambient and IBL light take.
[vk::binding(1, 2)] Texture2D<float4> ambientOcclusionTex;
[vk::binding(2, 2)] SamplerState ambientOcclusionSampler;

// One group for the lights and their tile grid: WebGPU allows 4 bind groups (maxBindGroups), so no group 4.
[vk::binding(0, 3)] StructuredBuffer<float4> punctualLights;
[vk::binding(1, 3)] StructuredBuffer<uint2> tileLightGrid;
[vk::binding(2, 3)] StructuredBuffer<uint> tileLightIndices;
// One layer per cube face (slot * 6 + face): a 2D array needs no cube-array support, which WebGPU compatibility lacks.
[vk::binding(3, 3)] Texture2DArray<float> punctualShadowMap;

float4x4 light_space_for(int cascade)
{
    if (cascade == 1) return frame.light_space_1;
    if (cascade == 2) return frame.light_space_2;
    return frame.light_space;
}

float bias_scale_for(int cascade)
{
    if (cascade == 1) return frame.shadow_bias_scale.y;
    if (cascade == 2) return frame.shadow_bias_scale.z;
    return frame.shadow_bias_scale.x;
}

// This pass's map and comparison sampler, one cascade's layer.
struct CascadeTaps : IShadowTaps
{
    float layer;

    // Out-of-map taps count as lit here: the web target has no ClampToBorder, and ClampToEdge would shadow them.
    float lit(float2 uv, float reference)
    {
        return (uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0)
          ? shadowMap.SampleCmpLevelZero(shadowSampler, float3(uv, layer), reference)
          : 1.0;
    }
}

// Visibility (1 = lit) of the sun; a fragment the selected box misses retries coarser cascades before counting as lit.
float shadow_visibility(float3 worldPos, float3 n, float nDotL)
{
    int count = clamp(int(frame.shadow_params.x), 0, CASCADE_COUNT);
    if (count == 0) return 1.0;
    float viewDepth = dot(worldPos - frame.camera_position.xyz, frame.shadow_view_dir.xyz);

    int cascade = select_cascade(viewDepth, frame.cascade_splits.xy, count);
    CascadeHit hit;
    hit.inside = false;
    for (; cascade < count; cascade++)
    {
        hit = cascade_hit(light_space_for(cascade), worldPos);
        if (hit.inside) break;
    }
    if (!hit.inside) return 1.0;

    int radius = clamp(int(frame.shadow_params.y), 0, MAX_PCF_RADIUS);
    uint shadowWidth, shadowHeight, shadowLayers;
    shadowMap.GetDimensions(shadowWidth, shadowHeight, shadowLayers);
    CascadeTaps taps = { float(cascade) };
    return cascade_visibility(taps, light_space_for(cascade), hit, n, nDotL, bias_scale_for(cascade),
        float2(1.0 / float(shadowWidth), 1.0 / float(shadowHeight)), radius);
}

// This pass's 2D array of cube faces, one 2x2 PCF tap of a light's slot.
struct FaceTaps : IPunctualShadowTaps
{
    int slot;

    float lit(float3 d, float reference)
    {
        return punctualShadowMap.SampleCmpLevelZero(shadowSampler, punctual_shadow_face_coord(d, slot), reference);
    }
}

float3 punctual_lighting(float3 worldPos, float3 n, float3 v, float3 albedo,
    float metallic, float roughness, float3 f0, float2 energy, float4 fragCoord)
{
    float3 total = float3(0.0);
    uint tileX = uint(fragCoord.x) / TILE_SIZE;
    uint tileY = uint(fragCoord.y) / TILE_SIZE;
    uint tileW = uint(frame.cascade_splits.z);
    uint tileH = uint(frame.cascade_splits.w);
    uint tileCount = uint(frame.camera_position.w);
    uint shadowCubes = uint(frame.shadow_params.w);
    uint tileOffset = 0u;
    bool listed = false;
    if (tileW != 0u)
    {
        uint tx = min(tileX, max(tileW, 1u) - 1u);
        uint ty = min(tileY, max(tileH, 1u) - 1u);
        uint tileIndex = ty * tileW + tx;
        uint2 gridEntry = tileLightGrid[tileIndex];
        // The binning is conservative, so an empty tile shades nothing; an overflowing one lists none and shades all.
        listed = gridEntry.x <= MAX_LIGHTS_PER_TILE;
        if (listed)
        {
            tileCount = gridEntry.x;
            tileOffset = gridEntry.y;
        }
    }

    for (uint j = 0u; j < tileCount; j++)
    {
        uint lightIdx = listed ? tileLightIndices[tileOffset + j] : j;
        PunctualLight light = unpack_punctual_light(punctualLights[lightIdx * 4u], punctualLights[lightIdx * 4u + 1u],
            punctualLights[lightIdx * 4u + 2u], punctualLights[lightIdx * 4u + 3u]);
        PunctualIncidence incidence = punctual_incidence(light, worldPos);
        if (incidence.attenuation <= 0.0) continue;
        float visibility = 1.0;
        PunctualShadow shadow = unpack_punctual_shadow(punctualLights[lightIdx * 4u + 3u]);
        if (shadow.slot >= 0 && uint(shadow.slot) < shadowCubes && dot(n, incidence.l) > 0.0)
        {
            uint width, height, layers;
            punctualShadowMap.GetDimensions(width, height, layers);
            FaceTaps taps = { shadow.slot };
            visibility = punctual_shadow_visibility(taps, shadow, light.position, worldPos, n, incidence.l, float(width));
        }
        total += brdf_direct(n, v, incidence.l, albedo, metallic, roughness, f0,
            light.radiance * incidence.attenuation * visibility, energy);
    }
    return total;
}

// The baked environment at its intensity; level 0 for the LUT, legal after the deferred early return.
struct ForwardAmbientMaps : IAmbientMaps
{
    float3 irradiance(float3 n)
    {
        return irradianceMap.SampleLevel(iblSampler, n, 0.0).xyz * iblParams.enabled_maxmip_intensity.z;
    }
    float3 reflection(float3 r, float roughness)
    {
        float lod = roughness * iblParams.enabled_maxmip_intensity.y;
        return prefilteredMap.SampleLevel(iblSampler, r, lod).xyz * iblParams.enabled_maxmip_intensity.z;
    }
    float2 energy(float nDotV, float roughness)
    {
        return brdfLut.SampleLevel(iblSampler, float2(nDotV, roughness), 0.0).rg;
    }
}

// SSAO at the fragment, filtered up from half resolution; 1 while SSAO is off or for a BLEND surface.
float frame_occlusion(float4 fragCoord, bool blended)
{
    float2 uv = fragCoord.xy * frame.ambient_occlusion.yz;
    return screen_space_occlusion(ambientOcclusionTex.SampleLevel(ambientOcclusionSampler, uv, 0.0).r,
        frame.ambient_occlusion.x, blended);
}

// The ambient term from the baked environment, or from the analytic sky and the surface's energy without one.
float3 ibl_ambient(SurfacePoint s, float3 v, float3 f0, float2 surfaceEnergy, float screenOcclusion)
{
    ForwardAmbientMaps maps;
    float3 ambient = sky_ambient(maps, iblParams.enabled_maxmip_intensity.x > 0.5, s.n, v, s.albedo, s.metallic,
        s.roughness, f0, surfaceEnergy);
    return frame.light_dir_ambient.w * s.occlusion * screenOcclusion * ambient;
}

// The lit HDR colour of `s`; fragCoord picks its light tile and its SSAO texel, which a BLEND surface ignores.
float3 shade_surface(SurfacePoint s, float4 fragCoord, bool blended)
{
    float3 v = normalize(frame.camera_position.xyz - s.position);
    float3 l = normalize(frame.light_dir_ambient.xyz);

    float3 f0 = surface_f0(s.albedo, s.metallic);
    float3 radiance = frame.light_color_intensity.rgb * frame.light_color_intensity.w;
    float nDotL = max(dot(s.n, l), 0.0);
    // The fit once per fragment, not once per light: the sun, every punctual light and the analytic ambient share it.
    float2 energy = ggx_energy(dot(s.n, v), s.roughness);

    float3 directLight = brdf_direct(s.n, v, l, s.albedo, s.metallic, s.roughness, f0, radiance, energy);
    // Per fragment: a per-vertex depth interpolates too far inside big triangles, picking coarse cascades.
    float visibility = shadow_visibility(s.position, s.n, nDotL);

    float3 punctual = punctual_lighting(s.position, s.n, v, s.albedo, s.metallic, s.roughness, f0, energy, fragCoord);
    float3 ambient = ibl_ambient(s, v, f0, energy, frame_occlusion(fragCoord, blended));
    return surface_lighting(directLight, visibility, frame.shadow_params.z, punctual, ambient, s.emissive);
}
