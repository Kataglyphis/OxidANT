import brdf;
import hdr_safe;
import punctual_light;
import punctual_shadow;
import shadow_pcf;
import sky_model;

// The lighting forward.slang's forward and deferred lighting passes share, so the two paths cannot drift apart.

// Must match CASCADE_COUNT in forward.rs (one light_space* matrix each).
static const int CASCADE_COUNT = 3;

// Must match render::shadow_settings::MAX_PCF_RADIUS, the C++ MAX_PCF_RADIUS.
static const int MAX_PCF_RADIUS = 5;

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

// One group for the lights and their tile grid: WebGPU allows 4 bind groups (maxBindGroups), so no group 4.
[vk::binding(0, 3)] StructuredBuffer<float4> punctualLights;
[vk::binding(1, 3)] StructuredBuffer<uint2> tileLightGrid;
[vk::binding(2, 3)] StructuredBuffer<uint> tileLightIndices;
// One layer per cube face (slot * 6 + face): a 2D array needs no cube-array support, which WebGPU compatibility lacks.
[vk::binding(3, 3)] Texture2DArray<float> punctualShadowMap;

// What lighting needs of a surface point, whether the forward pass sampled it or the deferred pass read it back.
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

// The first cascade whose split lies beyond the view depth, else the last, as cascaded_shadow.slang selects.
int select_cascade(float viewDepth, int count)
{
    int selected = 0;
    if (viewDepth >= frame.cascade_splits.x) selected = 1;
    if (viewDepth >= frame.cascade_splits.y) selected = 2;
    return min(selected, count - 1);
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

// Visibility (1 = lit), the inverse of cascaded_shadow.slang's calc_cascaded_shadow, with its bias and (2r+1)^2 taps.
float shadow_visibility(float3 worldPos, float3 n, float nDotL)
{
    int count = clamp(int(frame.shadow_params.x), 0, CASCADE_COUNT);
    if (count == 0) return 1.0;
    float viewDepth = dot(worldPos - frame.camera_position.xyz, frame.shadow_view_dir.xyz);

    // A fragment the selected box misses retries coarser cascades before counting as lit.
    float3 proj = float3(0.0, 0.0, 0.0);
    float2 uv = float2(0.0, 0.0);
    int cascade = select_cascade(viewDepth, count);
    bool inBounds = false;
    for (; cascade < count; cascade++)
    {
        float4 lightSpacePos = mul(light_space_for(cascade), float4(worldPos, 1.0));
        proj = lightSpacePos.xyz / lightSpacePos.w;
        uv = float2(proj.x * 0.5 + 0.5, 0.5 - proj.y * 0.5);
        if (uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0 && proj.z >= 0.0 && proj.z <= 1.0)
        {
            inBounds = true;
            break;
        }
    }
    if (!inBounds) return 1.0;

    float bias = max(0.002 * (1.0 - nDotL), 0.0005) * bias_scale_for(cascade);
    int radius = clamp(int(frame.shadow_params.y), 0, MAX_PCF_RADIUS);
    float2 slope = receiver_plane_depth_slope(light_space_for(cascade), n, -1.0);

    uint shadowWidth, shadowHeight, shadowLayers;
    shadowMap.GetDimensions(shadowWidth, shadowHeight, shadowLayers);
    float texel = 1.0 / float(shadowWidth);
    CascadeTaps taps = { float(cascade) };
    return pcf_visibility(taps, uv, float2(texel), proj.z - bias, slope, radius);
}

// Local wrapper: this pass's light comes from FrameUniforms, not sky.slang's SkyUniforms.
float3 sky_radiance(float3 dir, bool withSun)
{
    float3 color = sky_gradient(dir);
    if (withSun)
        color += sun_disk(dir, frame.light_dir_ambient.xyz, frame.light_color_intensity.w);
    return color;
}

// One 2x2 PCF tap of slot's cube along d; uv flips NDC y, as WebGPU's framebuffer rows run down.
float punctual_shadow_tap(float3 d, int slot, float reference)
{
    PunctualShadowFace f = punctual_shadow_face(d);
    float2 uv = float2(f.ndc.x * 0.5 + 0.5, 0.5 - f.ndc.y * 0.5);
    float layer = float(uint(slot) * 6u + f.face);
    return punctualShadowMap.SampleCmpLevelZero(shadowSampler, float3(uv, layer), reference);
}

// Fraction of the light reaching worldPos: a tap at the centre and four around it, the C++ punctual_lights.slang's.
float punctual_shadow_visibility(PunctualShadow shadow, float3 lightPos, float3 worldPos, float3 n, float3 l)
{
    uint width, height, layers;
    punctualShadowMap.GetDimensions(width, height, layers);
    // A texel spans 2 * distance / width at the receiver; offsetting by it along n and l keeps acne off.
    float texel = 2.0 * length(worldPos - lightPos) / float(width);
    float3 d = worldPos + n * (1.5 * texel) + l * texel - lightPos;
    float majorAxis = max(abs(d.x), max(abs(d.y), abs(d.z)));
    if (majorAxis >= shadow.farPlane)
        return 1.0;
    float reference = punctual_shadow_depth(max(majorAxis, shadow.nearPlane), shadow.nearPlane, shadow.farPlane);

    float3 axis = normalize(d);
    float3 tangent = normalize(cross(abs(axis.y) < 0.99 ? float3(0.0, 1.0, 0.0) : float3(1.0, 0.0, 0.0), axis));
    float3 bitangent = cross(axis, tangent);
    float spread = 1.5 * 2.0 * majorAxis / float(width);
    float lit = punctual_shadow_tap(d, shadow.slot, reference);
    lit += punctual_shadow_tap(d + tangent * spread, shadow.slot, reference);
    lit += punctual_shadow_tap(d - tangent * spread, shadow.slot, reference);
    lit += punctual_shadow_tap(d + bitangent * spread, shadow.slot, reference);
    lit += punctual_shadow_tap(d - bitangent * spread, shadow.slot, reference);
    return lit * 0.2;
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
            visibility = punctual_shadow_visibility(shadow, light.position, worldPos, n, incidence.l);
        total += brdf_direct(n, v, incidence.l, albedo, metallic, roughness, f0,
            light.radiance * incidence.attenuation * visibility, energy);
    }
    return total;
}

// The ambient term from the baked environment, or from the analytic sky and the surface's energy without one.
float3 ibl_ambient(SurfacePoint s, float3 v, float3 f0, float2 surfaceEnergy)
{
    float3 reflected = reflect(-v, s.n);
    float3 irradiance;
    float3 reflection;
    float2 energy = surfaceEnergy;
    // A uniform flag: every fragment takes one side, so none pays for the other's samples or sky.
    if (iblParams.enabled_maxmip_intensity.x > 0.5)
    {
        float envIntensity = iblParams.enabled_maxmip_intensity.z;
        irradiance = irradianceMap.SampleLevel(iblSampler, s.n, 0.0).xyz * envIntensity;
        reflection = prefilteredMap.SampleLevel(iblSampler, reflected, s.roughness * iblParams.enabled_maxmip_intensity.y).xyz * envIntensity;
        // The split sum's own table, so a uniform one returns E_spec exactly; level 0, legal after the deferred early return.
        energy = brdfLut.SampleLevel(iblSampler, float2(max(dot(s.n, v), 0.0), s.roughness), 0.0).rg;
    }
    else
    {
        irradiance = hemisphere_irradiance(s.n);
        reflection = lerp(sky_radiance(reflected, true), irradiance, s.roughness * s.roughness);
    }
    float3 specularAlbedo = specular_albedo(energy, f0);
    float3 diffuseIbl = saturate(float3(1.0) - specularAlbedo) * (1.0 - s.metallic) * s.albedo * irradiance;
    float3 specularIbl = reflection * specularAlbedo;

    return frame.light_dir_ambient.w * s.occlusion * (diffuseIbl + specularIbl);
}

// The lit HDR colour of `s`; fragCoord picks its light tile.
float3 shade_surface(SurfacePoint s, float4 fragCoord)
{
    float3 v = normalize(frame.camera_position.xyz - s.position);
    float3 l = normalize(frame.light_dir_ambient.xyz);

    float3 f0 = lerp(float3(0.04), s.albedo, s.metallic);
    float3 radiance = frame.light_color_intensity.rgb * frame.light_color_intensity.w;
    float nDotL = max(dot(s.n, l), 0.0);
    // The fit once per fragment, not once per light: the sun, every punctual light and the analytic ambient share it.
    float2 energy = ggx_energy(dot(s.n, v), s.roughness);

    float3 directLight = brdf_direct(s.n, v, l, s.albedo, s.metallic, s.roughness, f0, radiance, energy);
    // Per fragment: a per-vertex depth interpolates too far inside big triangles, picking coarse cascades.
    float visibility = shadow_visibility(s.position, s.n, nDotL);
    // The C++ color *= 1 - shadow * strength, on the sun alone.
    directLight *= lerp(1.0, visibility, frame.shadow_params.z);

    float3 punctual = punctual_lighting(s.position, s.n, v, s.albedo, s.metallic, s.roughness, f0, energy, fragCoord);
    float3 color = directLight + punctual + ibl_ambient(s, v, f0, energy) + s.emissive;
    return hdr_safe(color);
}
