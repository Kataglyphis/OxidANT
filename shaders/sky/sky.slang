// Rust/WebGPU procedural sky: the checked-in sky.wgsl is generated from this file.

import sky_model;

struct SkyUniforms
{
    float4x4 inv_view_proj;
    float4 light_dir_intensity; // xyz: dir towards light, w: intensity
    float4 mode;                // x: 0 procedural, 1 cube map, 2 switched off
};
[vk::binding(0, 0)] ConstantBuffer<SkyUniforms> sky;
[vk::binding(1, 0)] TextureCube<float4> skyboxTex;
[vk::binding(2, 0)] SamplerState skyboxSampler;

// Below fp16's 65504, as forward_lighting.slang's: a brighter sun would store +Inf in the HDR target.
static const float HDR_MAX = 65000.0;

// The C++ SKYBOX_DISABLED_RADIANCE: linear navy while the skybox is switched off.
static const float3 SKYBOX_DISABLED = float3(0.0, 0.0, 0.5);

// The C++ sky is never tonemapped, so it enters as the radiance whose ACES at 0 EV reproduces its colour.
float3 untonemapped(float3 display)
{
    const float a = 2.51;
    const float b = 0.03;
    const float c = 2.43;
    const float d = 0.59;
    const float e = 0.14;
    float3 y = clamp(display, float3(0.0), float3(1.0));
    float3 qa = a - c * y;
    float3 qb = b - d * y;
    return (-qb + sqrt(qb * qb + 4.0 * qa * e * y)) / (2.0 * qa);
}

struct SkyVsOut
{
    float4 svPosition : SV_Position;
    float2 ndc : TEXCOORD0;
};

[shader("vertex")]
SkyVsOut vs_main(uint vid : SV_VertexID)
{
    SkyVsOut o;
    float x = float(vid / 2) * 4.0 - 1.0;
    float y = float(vid % 2) * 4.0 - 1.0;
    o.svPosition = float4(x, y, 1.0, 1.0);
    o.ndc = float2(x, y);
    return o;
}

[shader("fragment")]
float4 fs_main(SkyVsOut In) : SV_Target
{
    float4 nearP = mul(sky.inv_view_proj, float4(In.ndc, 0.0, 1.0));
    float4 farP = mul(sky.inv_view_proj, float4(In.ndc, 1.0, 1.0));
    float3 dir = normalize(farP.xyz / farP.w - nearP.xyz / nearP.w);

    if (sky.mode.x > 1.5)
        return float4(untonemapped(SKYBOX_DISABLED), 1.0);
    if (sky.mode.x > 0.5)
        return float4(untonemapped(skyboxTex.SampleLevel(skyboxSampler, dir, 0.0).rgb), 1.0);

    float3 color = sky_gradient(dir) + sun_disk(dir, sky.light_dir_intensity.xyz, sky.light_dir_intensity.w);

    return float4(min(color, float3(HDR_MAX)), 1.0);
}