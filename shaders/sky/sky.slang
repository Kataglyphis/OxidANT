// The sky both renderers draw into their HDR target: sky.wgsl is generated from this file, the C++ SPIR-V too.

import aces;
import hdr_safe;
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

// The C++ SKYBOX_DISABLED_RADIANCE: linear navy while the skybox is switched off.
static const float3 SKYBOX_DISABLED = float3(0.0, 0.0, 0.5);

struct SkyVsOut
{
    float4 svPosition : SV_Position;
    float2 ndc : TEXCOORD0;
};

// A fullscreen triangle at depth 1, so the sky only fills what no surface covered.
SkyVsOut sky_vs(uint vid)
{
    SkyVsOut o;
    float x = float(vid / 2) * 4.0 - 1.0;
    float y = float(vid % 2) * 4.0 - 1.0;
    o.svPosition = float4(x, y, 1.0, 1.0);
    o.ndc = float2(x, y);
    return o;
}

[shader("vertex")]
SkyVsOut vs_main(uint vid : SV_VertexID)
{
    return sky_vs(vid);
}

// The C++ renderer's SPIR-V entry: SV_VertexID subtracts BaseVertex, which needs draw parameters a Raspberry Pi 5's v3dv lacks.
[shader("vertex")]
SkyVsOut vs_vulkan_main(uint vid : SV_VulkanVertexID)
{
    return sky_vs(vid);
}

[shader("fragment")]
float4 fs_main(SkyVsOut In) : SV_Target
{
    float4 nearP = mul(sky.inv_view_proj, float4(In.ndc, 0.0, 1.0));
    float4 farP = mul(sky.inv_view_proj, float4(In.ndc, 1.0, 1.0));
    float3 dir = normalize(farP.xyz / farP.w - nearP.xyz / nearP.w);

    if (sky.mode.x > 1.5)
        return float4(aces_tonemap_inverse(SKYBOX_DISABLED), 1.0);
    // The photo skybox holds display colours, so it enters as the radiance whose ACES at 0 EV reproduces them.
    if (sky.mode.x > 0.5)
        return float4(aces_tonemap_inverse(skyboxTex.SampleLevel(skyboxSampler, dir, 0.0).rgb), 1.0);

    float3 color = sky_gradient(dir) + sun_disk(dir, sky.light_dir_intensity.xyz, sky.light_dir_intensity.w);

    return float4(hdr_safe(color), 1.0);
}