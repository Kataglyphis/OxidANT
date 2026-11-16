import shadow_pcf;

// CI-only entry point: proves shadow_pcf.slang emits SPIR-V the way the C++ cascaded_shadow.slang imports it; nothing loads it.

[vk::binding(0, 0)] RWTexture2D<float4> outputImage;
[vk::binding(1, 0)] Sampler2DArrayShadow directionalShadowMaps;

struct CombinedTaps : IShadowTaps
{
    float layer;

    float lit(float2 uv, float reference)
    {
        return (uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0)
          ? directionalShadowMaps.SampleCmpLevelZero(float3(uv, layer), reference)
          : 1.0;
    }
};

[shader("compute")]
[numthreads(16, 16, 1)]
void shadow_pcf_test_main(uint3 tid : SV_DispatchThreadID)
{
    float4x4 lightSpace = float4x4(0.1, 0.0, 0.0, 0.0, 0.0, 0.1, 0.0, 0.0, 0.0, 0.3, 0.05, 0.5, 0.0, 0.0, 0.0, 1.0);
    float2 slope = receiver_plane_depth_slope(lightSpace, float3(0.0, 1.0, 0.0), 1.0);
    CombinedTaps taps = { 0.0 };
    float lit = pcf_visibility(taps, float2(tid.xy) / 256.0, float2(1.0 / 2048.0), 0.5, slope, 2);
    outputImage[tid.xy] = float4(lit, slope, 1.0);
}
