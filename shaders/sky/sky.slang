// Rust/WebGPU procedural sky: the checked-in sky.wgsl is generated from this file.

import sky_model;

struct SkyUniforms
{
    float4x4 inv_view_proj;
    float4 light_dir_intensity; // xyz: dir towards light, w: intensity
};
[vk::binding(0, 0)] ConstantBuffer<SkyUniforms> sky;

// Below fp16's 65504, as forward.slang's: a brighter sun would store +Inf in the HDR target.
static const float HDR_MAX = 65000.0;

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

    float3 color = sky_gradient(dir) + sun_disk(dir, sky.light_dir_intensity.xyz, sky.light_dir_intensity.w);

    return float4(min(color, float3(HDR_MAX)), 1.0);
}