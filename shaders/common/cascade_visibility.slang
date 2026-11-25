import fullscreen;
// Callers implement shadow_pcf.slang's IShadowTaps.
__exported import shadow_pcf;

// The sun's cascaded shadow for both renderers: each binds its own map through IShadowTaps and keeps its own search.

// A world point in one cascade: its map uv on this target, its depth, and whether the cascade's box holds it.
struct CascadeHit
{
    float2 uv;
    float depth;
    bool inside;
};

// The first cascade whose split lies beyond viewDepth, else the last of count; splits holds the first two splits.
int select_cascade(float viewDepth, float2 splits, int count)
{
    int selected = 0;
    if (viewDepth >= splits.x) selected = 1;
    if (viewDepth >= splits.y) selected = 2;
    return min(selected, count - 1);
}

// worldPos through one cascade's light matrix; uv follows the target's NDC rows, as the shadow pass rasterised them.
CascadeHit cascade_hit(float4x4 lightSpace, float3 worldPos)
{
    float4 lightSpacePos = mul(lightSpace, float4(worldPos, 1.0));
    float3 proj = lightSpacePos.xyz / lightSpacePos.w;
    CascadeHit hit;
    hit.uv = fullscreen_ndc_to_uv(proj.xy);
    hit.depth = proj.z;
    hit.inside = hit.uv.x >= 0.0 && hit.uv.x <= 1.0 && hit.uv.y >= 0.0 && hit.uv.y <= 1.0 && proj.z >= 0.0
                 && proj.z <= 1.0;
    return hit;
}

// The slope-scaled depth bias, times a cascade's depth range against the reference box's.
float cascade_bias(float nDotL, float biasScale)
{
    return max(0.002 * (1.0 - nDotL), 0.0005) * biasScale;
}

// The receiver plane's depth per unit of this target's map uv.
float2 cascade_depth_slope(float4x4 lightSpace, float3 n)
{
    return receiver_plane_depth_slope(lightSpace, n, fullscreen_ndc_uv_y_sign());
}

// Lit fraction of a hit: (2 radius + 1)^2 taps a texel apart, each at the biased receiver plane's depth beneath it.
float cascade_visibility<T : IShadowTaps>(T taps, float4x4 lightSpace, CascadeHit hit, float3 n, float nDotL,
    float biasScale, float2 texel, int radius)
{
    float reference = hit.depth - cascade_bias(nDotL, biasScale);
    return pcf_visibility(taps, hit.uv, texel, reference, cascade_depth_slope(lightSpace, n), radius);
}
