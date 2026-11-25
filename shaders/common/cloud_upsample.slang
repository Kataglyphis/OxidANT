import volumetric_clouds;

// The half-resolution cloud march brought to full resolution, for both renderers' upsample passes.

// A pass's own textures, and its pixel's ray, behind the upsample.
interface ICloudUpsampleTexels
{
    // The full-resolution scene depth at pixel.
    float scene_depth(int2 pixel);
    // The half-resolution march at texel.
    float4 half_cloud(int2 texel);
    // The distance from the eye along this pixel's ray to the surface at an NDC depth.
    float scene_distance(float depth);
}

// The nearest scene depth of the 2x2 pixels a half-resolution texel covers, inside a depthSize target.
float half_texel_depth<T : ICloudUpsampleTexels>(T texels, int2 texel, int2 depthSize)
{
    int2 last = depthSize - 1;
    float nearest = 1.0;
    for (int i = 0; i < 4; i++)
    {
        int2 pixel = min(texel * 2 + int2(i & 1, i >> 1), last);
        nearest = min(nearest, texels.scene_depth(pixel));
    }
    return nearest;
}

// pixel's cloud from its four nearest half texels, each weighted bilinearly and by its depth against the pixel's.
float4 cloud_upsample<T : ICloudUpsampleTexels>(T texels, int2 pixel, int2 halfSize, int2 depthSize)
{
    float pixelDistance = texels.scene_distance(texels.scene_depth(pixel));
    int2 last = halfSize - 1;
    // This pixel's centre in half-resolution texels, whose centres sit at whole numbers.
    float2 position = (float2(pixel) + 0.5) * 0.5 - 0.5;
    int2 base = int2(floor(position));
    float2 f = position - float2(base);

    float4 weighted = float4(0.0);
    float4 bilinear = float4(0.0);
    float weightSum = 0.0;
    for (int i = 0; i < 4; i++)
    {
        int2 corner = int2(i & 1, i >> 1);
        int2 texel = clamp(base + corner, int2(0), last);
        float2 axis = lerp(1.0 - f, f, float2(corner));
        float w = axis.x * axis.y;
        float4 value = texels.half_cloud(texel);
        // The texel's depth along this pixel's ray: neighbours' rays differ by less than the depth edges matter.
        float texelDistance = texels.scene_distance(half_texel_depth(texels, texel, depthSize));
        float depthWeight = w * cloud_upsample_depth_weight(pixelDistance, texelDistance);
        weighted += value * depthWeight;
        weightSum += depthWeight;
        bilinear += value * w;
    }
    // No neighbour at this pixel's depth, a lone sliver: the plain bilinear blend beats a division by almost zero.
    return weightSum > 1e-4 ? weighted / weightSum : bilinear;
}
