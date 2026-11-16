import fullscreen;

// MSAA depth resolve keeping the nearest sample; the checked-in depth_resolve.wgsl is generated from this file.

[vk::binding(0, 0)] Texture2DMS<float> msaaDepth;

struct ResolveOut
{
    // The farthest sample and the share of samples nearer to it than to the nearest, for the cloud march.
    float2 spread : SV_Target0;
    float depth : SV_Depth;
};

[shader("vertex")]
FullscreenVsOut vs_main(uint vid : SV_VertexID)
{
    // uv.y must be 0 at the top: fs_main reads at uv but writes at fragCoord, else the depth is mirrored.
    return fullscreen_vs(vid);
}

[shader("fragment")]
ResolveOut fs_main(FullscreenVsOut In)
{
    uint w, h, samples;
    msaaDepth.GetDimensions(w, h, samples);
    int2 coords = int2(In.uv * float2(float(w), float(h)));
    float minDepth = 1.0;
    float maxDepth = 0.0;
    for (uint i = 0u; i < samples; i++)
    {
        float d = msaaDepth.Load(coords, i);
        minDepth = min(minDepth, d);
        maxDepth = max(maxDepth, d);
    }
    float middle = 0.5 * (minDepth + maxDepth);
    uint far = 0u;
    for (uint j = 0u; j < samples; j++)
        far += msaaDepth.Load(coords, j) > middle ? 1u : 0u;
    ResolveOut o;
    o.depth = minDepth;
    o.spread = float2(maxDepth, float(far) / float(samples));
    return o;
}
