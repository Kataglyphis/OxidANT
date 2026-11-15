import fullscreen;

// Copies the traced modes' accumulation into the HDR target, so bloom, exposure and the tonemap run as after raster, and its depth into the depth target.

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

struct ResolveOut
{
    float4 colour : SV_Target;
    float depth : SV_Depth;
};

// The accumulation's alpha is the primary hit's NDC depth, which the single-sample depth target takes for the sky and the clouds.
[shader("fragment")]
ResolveOut fs_main(FullscreenVsOut In)
{
    uint2 pixel = uint2(In.svPosition.xy);
    float4 traced = resolveAccumulation[pixel.y * resolveParams.size.x + pixel.x];
    ResolveOut o;
    o.colour = float4(traced.rgb, 1.0);
    o.depth = traced.a;
    return o;
}
