import fullscreen;

// Copies the traced modes' accumulation into the HDR target, so bloom, exposure and the tonemap run as after raster.

// The first field of trace_shading.slang's TraceFrame: the same buffer is bound here.
struct TraceResolveParams
{
    uint4 size;
};

[vk::binding(0, 0)] ConstantBuffer<TraceResolveParams> resolveParams;
[vk::binding(1, 0)] StructuredBuffer<float4> resolveAccumulation;

// Returned directly: a copy into a local struct makes Slang emit FullscreenVsOut with two @location(0), which Tint rejects.
[shader("vertex")]
FullscreenVsOut vs_main(uint vid : SV_VertexID)
{
    return fullscreen_vs(vid);
}

[shader("fragment")]
float4 fs_main(FullscreenVsOut In) : SV_Target
{
    uint2 pixel = uint2(In.svPosition.xy);
    return float4(resolveAccumulation[pixel.y * resolveParams.size.x + pixel.x].rgb, 1.0);
}
