// The sky both renderers draw into their HDR target: sky.wgsl is generated from this file, the C++ SPIR-V too.

import camera_ray;
import drawn_sky;

struct SkyUniforms
{
    float4x4 inv_view_proj;
    float4 light_dir_intensity; // xyz: dir towards light, w: intensity
    float4 mode;                // x: 0 procedural, 1 cube map, 2 switched off
};
[vk::binding(0, 0)] ConstantBuffer<SkyUniforms> sky;
[vk::binding(1, 0)] TextureCube<float4> skyboxTex;
[vk::binding(2, 0)] SamplerState skyboxSampler;

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
    float3 dir = camera_ray(sky.inv_view_proj, In.ndc).direction;
    float3 cube = skyboxTex.SampleLevel(skyboxSampler, dir, 0.0).rgb;
    return float4(drawn_sky(sky.mode.x, cube, dir, sky.light_dir_intensity.xyz, sky.light_dir_intensity.w), 1.0);
}