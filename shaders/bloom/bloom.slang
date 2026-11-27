import bloom_filter;
import exposure;
import fullscreen;
import hdr_safe;

// Both renderers' bloom (bloom.wgsl and the C++ SPIR-V); common/bloom_filter.slang holds its threshold and taps.

[vk::binding(0, 0)] Texture2D<float4> srcTex;
[vk::binding(1, 0)] SamplerState srcSampler;

// [adapted EV, target EV] from cs_reduce_exposure, shared with tonemap.slang as the one exposure source.
[vk::binding(2, 0)] StructuredBuffer<float> exposureState;
// Premultiplied cloud colour and 1 - transmittance from cloud_march.slang; read only while params.x is set.
[vk::binding(3, 0)] Texture2D<float4> cloudTex;

struct BloomUniforms
{
    // x: 1.0 to composite the clouds before thresholding.
    float4 params;
};
[vk::binding(4, 0)] ConstantBuffer<BloomUniforms> bloomUniforms;

float3 blur(float2 uv, float2 dir)
{
    uint w, h;
    srcTex.GetDimensions(w, h);
    float2 texel = dir / float2(float(w), float(h));
    float3 color = srcTex.Sample(srcSampler, uv).rgb * bloom_tap_weight(0);
    // Unrolled: straight-line taps, as the blur was written before the taps had a module.
    [ForceUnroll]
    for (int i = 1; i <= BLOOM_TAP_RADIUS; i++)
    {
        color += srcTex.Sample(srcSampler, uv + texel * float(i)).rgb * bloom_tap_weight(i);
        color += srcTex.Sample(srcSampler, uv - texel * float(i)).rgb * bloom_tap_weight(-i);
    }
    return color;
}

[shader("vertex")]
FullscreenVsOut vs_main(uint vid : SV_VertexID)
{
    return fullscreen_vs(vid);
}

[shader("fragment")]
float4 fs_brightpass(FullscreenVsOut In) : SV_Target
{
    float3 scene = hdr_safe(srcTex.Sample(srcSampler, In.uv).rgb);
    // Thresholds what the tonemap shows: a sun the clouds hide must not glow around them.
    if (bloomUniforms.params.x > 0.5)
    {
        float4 cloud = cloudTex.Sample(srcSampler, In.uv);
        scene = cloud.rgb + scene * (1.0 - cloud.a);
    }
    return float4(bloom_bright(hdr_safe(scene) * exposure_scale(exposureState[0])), 1.0);
}

[shader("fragment")]
float4 fs_blur_h(FullscreenVsOut In) : SV_Target
{
    return float4(blur(In.uv, float2(1.0, 0.0)), 1.0);
}

[shader("fragment")]
float4 fs_blur_v(FullscreenVsOut In) : SV_Target
{
    return float4(blur(In.uv, float2(0.0, 1.0)), 1.0);
}