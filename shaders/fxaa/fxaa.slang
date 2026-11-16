import fullscreen;

// Rust/WebGPU FXAA (the FXAA 3.11 quality edge walk): fxaa.wgsl is generated from this file; bindings match fxaa.rs.

[vk::binding(0, 0)] Texture2D<float4> ldrTex;
[vk::binding(1, 0)] SamplerState ldrSampler;

struct FxaaUniforms
{
    // xy: 1 / target size, z: 1.0 when ldrTex samples linear values (an sRGB target), w: unused.
    float4 params;
};
[vk::binding(2, 0)] ConstantBuffer<FxaaUniforms> fxaaUniforms;

// A step below max(EDGE_MIN, local max * EDGE_MAX) is texture or noise, not an edge, and stays untouched.
static const float EDGE_MIN = 0.0312;
static const float EDGE_MAX = 0.125;
static const float SUBPIXEL_QUALITY = 0.75;
static const int SEARCH_STEPS = 12;

// Step widths of the edge walk, wider as it goes, as in FXAA 3.11's quality preset 39.
float search_step(int i)
{
    if (i < 5)
        return 1.0;
    if (i == 5)
        return 1.5;
    if (i < 10)
        return 2.0;
    if (i == 10)
        return 4.0;
    return 8.0;
}

// Perceptual luma: an sRGB target hands back linear values, which the square root brings near gamma 2.
float luma_of(float3 rgb)
{
    float luma = dot(rgb, float3(0.299, 0.587, 0.114));
    return fxaaUniforms.params.z > 0.5 ? sqrt(luma) : luma;
}

float luma_at(float2 uv)
{
    return luma_of(ldrTex.SampleLevel(ldrSampler, uv, 0.0).rgb);
}

[shader("vertex")]
FullscreenVsOut vs_main(uint vid : SV_VertexID)
{
    return fullscreen_vs(vid);
}

struct EdgeSpan
{
    float2 uv;
    float2 offset;
    float stepLength;
    float lumaAverage;
    float gradientScaled;
};

// Walks both ways along the edge until the luma leaves its average; returns the shift across it in steps of stepLength.
float edge_offset(float2 uv, EdgeSpan span, float lumaCenter, bool horizontal)
{
    float2 uv1 = span.uv - span.offset;
    float2 uv2 = span.uv + span.offset;
    float end1 = luma_at(uv1) - span.lumaAverage;
    float end2 = luma_at(uv2) - span.lumaAverage;
    bool reached1 = abs(end1) >= span.gradientScaled;
    bool reached2 = abs(end2) >= span.gradientScaled;
    for (int i = 1; i < SEARCH_STEPS && !(reached1 && reached2); i++)
    {
        if (!reached1)
        {
            uv1 -= span.offset * search_step(i);
            end1 = luma_at(uv1) - span.lumaAverage;
            reached1 = abs(end1) >= span.gradientScaled;
        }
        if (!reached2)
        {
            uv2 += span.offset * search_step(i);
            end2 = luma_at(uv2) - span.lumaAverage;
            reached2 = abs(end2) >= span.gradientScaled;
        }
    }
    float distance1 = horizontal ? uv.x - uv1.x : uv.y - uv1.y;
    float distance2 = horizontal ? uv2.x - uv.x : uv2.y - uv.y;
    bool towards1 = distance1 < distance2;
    float nearest = min(distance1, distance2);
    float pixelOffset = 0.5 - nearest / (distance1 + distance2);
    // Only the end where the luma crosses the other way than at the centre bounds the span.
    bool centerSmaller = lumaCenter < span.lumaAverage;
    bool crosses = ((towards1 ? end1 : end2) < 0.0) != centerSmaller;
    return crosses ? pixelOffset : 0.0;
}

[shader("fragment")]
float4 fs_main(FullscreenVsOut In) : SV_Target
{
    float2 rcp = fxaaUniforms.params.xy;
    float2 uv = In.uv;
    float4 center = ldrTex.Load(int3(int2(In.svPosition.xy), 0));
    float lumaCenter = luma_of(center.rgb);
    float lumaDown = luma_at(uv + float2(0.0, rcp.y));
    float lumaUp = luma_at(uv - float2(0.0, rcp.y));
    float lumaLeft = luma_at(uv - float2(rcp.x, 0.0));
    float lumaRight = luma_at(uv + float2(rcp.x, 0.0));
    float lumaMin = min(lumaCenter, min(min(lumaDown, lumaUp), min(lumaLeft, lumaRight)));
    float lumaMax = max(lumaCenter, max(max(lumaDown, lumaUp), max(lumaLeft, lumaRight)));
    float range = lumaMax - lumaMin;
    if (range < max(EDGE_MIN, lumaMax * EDGE_MAX))
        return float4(center.rgb, 1.0);

    float lumaDownLeft = luma_at(uv + float2(-rcp.x, rcp.y));
    float lumaUpRight = luma_at(uv + float2(rcp.x, -rcp.y));
    float lumaUpLeft = luma_at(uv - rcp);
    float lumaDownRight = luma_at(uv + rcp);
    float downUp = lumaDown + lumaUp;
    float leftRight = lumaLeft + lumaRight;
    float leftCorners = lumaDownLeft + lumaUpLeft;
    float downCorners = lumaDownLeft + lumaDownRight;
    float rightCorners = lumaDownRight + lumaUpRight;
    float upCorners = lumaUpRight + lumaUpLeft;
    float edgeHorizontal = abs(-2.0 * lumaLeft + leftCorners) + abs(-2.0 * lumaCenter + downUp) * 2.0
                           + abs(-2.0 * lumaRight + rightCorners);
    float edgeVertical = abs(-2.0 * lumaUp + upCorners) + abs(-2.0 * lumaCenter + leftRight) * 2.0
                         + abs(-2.0 * lumaDown + downCorners);
    bool horizontal = edgeHorizontal >= edgeVertical;

    // luma1 lies one texel the negative way (up or left), luma2 the positive way.
    float luma1 = horizontal ? lumaUp : lumaLeft;
    float luma2 = horizontal ? lumaDown : lumaRight;
    float gradient1 = luma1 - lumaCenter;
    float gradient2 = luma2 - lumaCenter;
    bool steepest1 = abs(gradient1) >= abs(gradient2);
    EdgeSpan span;
    span.gradientScaled = 0.25 * max(abs(gradient1), abs(gradient2));
    span.stepLength = horizontal ? rcp.y : rcp.x;
    span.lumaAverage = 0.5 * ((steepest1 ? luma1 : luma2) + lumaCenter);
    if (steepest1)
        span.stepLength = -span.stepLength;
    span.uv = uv;
    if (horizontal)
        span.uv.y += span.stepLength * 0.5;
    else
        span.uv.x += span.stepLength * 0.5;
    span.offset = horizontal ? float2(rcp.x, 0.0) : float2(0.0, rcp.y);
    float offset = edge_offset(uv, span, lumaCenter, horizontal);

    // Sub-pixel aliasing: a lone pixel against its neighbourhood blends by how far it stands out.
    float lumaAverage = (2.0 * (downUp + leftRight) + leftCorners + rightCorners) / 12.0;
    float subpixel = saturate(abs(lumaAverage - lumaCenter) / range);
    subpixel = (-2.0 * subpixel + 3.0) * subpixel * subpixel;
    offset = max(offset, subpixel * subpixel * SUBPIXEL_QUALITY);

    float2 finalUv = uv;
    if (horizontal)
        finalUv.y += offset * span.stepLength;
    else
        finalUv.x += offset * span.stepLength;
    return float4(ldrTex.SampleLevel(ldrSampler, finalUv, 0.0).rgb, 1.0);
}
