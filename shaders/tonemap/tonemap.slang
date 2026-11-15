import aces;
import fullscreen;

// Rust/WebGPU tonemap: tonemap.wgsl is generated from this file; bindings match forward.rs's TonemapPass.

[vk::binding(0, 0)] Texture2D<float4> hdrTex;
[vk::binding(1, 0)] SamplerState hdrSampler;
[vk::binding(2, 0)] Texture2D<float4> bloomTex;
[vk::binding(4, 0)] Texture2D<float4> aoTex;
// Premultiplied cloud colour and 1 - transmittance from cloud_march.slang; read only while params.z is set.
[vk::binding(6, 0)] Texture2D<float4> cloudTex;

struct TonemapUniforms
{
    // x: bloom strength, y: SSAO strength, z: 1.0 to composite clouds, w: 1.0 to encode sRGB here (web canvas).
    float4 params;
};
[vk::binding(3, 0)] ConstantBuffer<TonemapUniforms> tonemapUniforms;

// [adapted EV, target EV] from cs_reduce_exposure, the one exposure source (bloom reads it too).
[vk::binding(5, 0)] StructuredBuffer<float> exposureState;

// Returned directly: a copy into a local struct makes Slang emit FullscreenVsOut with two @location(0), which Tint rejects.
[shader("vertex")]
FullscreenVsOut vs_main(uint vid : SV_VertexID)
{
    return fullscreen_vs(vid);
}

[shader("fragment")]
float4 fs_main(FullscreenVsOut In) : SV_Target
{
    float3 hdr = hdrTex.Sample(hdrSampler, In.uv).rgb;
    float3 bloom = bloomTex.Sample(hdrSampler, In.uv).rgb;
    float aoRaw = aoTex.Sample(hdrSampler, In.uv).r;
    float ao = lerp(1.0, aoRaw, tonemapUniforms.params.y);
    float exposure = exp2(exposureState[0]);
    float4 cloud = float4(0.0);
    if (tonemapUniforms.params.z > 0.5)
        cloud = cloudTex.Sample(hdrSampler, In.uv);
    // Premultiplied-over in linear HDR, before exposure and ACES, so sky and geometry get the same composite.
    float transmittance = 1.0 - cloud.a;
    // ACES saturates long before 1e4, and capping there keeps its x*x terms off Inf/Inf = NaN.
    float3 scene = min((cloud.rgb + hdr * ao * transmittance) * exposure
                       + bloom * tonemapUniforms.params.x * transmittance, float3(1e4));
    float3 mapped = aces_tonemap(scene);

    // WebGPU canvases have no sRGB surface format, so the web encodes here (docs/shaders.md#color-space).
    if (tonemapUniforms.params.w > 0.5)
        return float4(linear_to_srgb(mapped), 1.0);
    return float4(mapped, 1.0);
}
