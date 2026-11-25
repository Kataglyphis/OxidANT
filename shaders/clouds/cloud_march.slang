import camera_ray;
import cloud_upsample;
import fullscreen;
import volumetric_clouds;

// Rust/WebGPU cloud march: cloud_march.wgsl is generated from this file; bindings match forward/cloud_pass.rs.

struct CloudFrame
{
    float4x4 inv_view_proj;
    float4 eye;
    // Normalized on the host.
    float4 to_sun;
    // The sun's colour times its intensity.
    float4 sun_radiance;
    CloudParams params;
    // x: 1.0 while depthSpread holds this frame's MSAA spread, the forward path's; y: 1.0 for a half-resolution march.
    float4 depth_spread;
    // The mean radiance of the sky this frame draws, over its upper (top) and lower (bottom) hemisphere.
    float4 ambient_top;
    float4 ambient_bottom;
};
[vk::binding(0, 0)] ConstantBuffer<CloudFrame> frame;
[vk::binding(1, 0)] Texture3D<float4> noiseTex;
[vk::binding(2, 0)] SamplerState noiseSampler;
// The resolved scene depth, bound as unfilterable float: depth formats allow it, and no WGSL patch is needed.
[vk::binding(3, 0)] Texture2D<float4> sceneDepth;
// The farthest MSAA sample and the share of samples at it, from depth_resolve.slang.
[vk::binding(4, 0)] Texture2D<float4> depthSpread;
// The half-resolution march, which only fs_upsample reads.
[vk::binding(5, 0)] Texture2D<float4> halfClouds;

struct SampledNoise : ICloudNoise
{
    // Explicit LOD: the march loops, and implicit derivatives need uniform control flow.
    float4 sample_noise(float3 uvw)
    {
        return noiseTex.SampleLevel(noiseSampler, uvw, 0.0);
    }
};

// This pass's depth and half march, along one pixel's ray; fs_upsample recomputes depths, so no target holds them.
struct PixelTexels : ICloudUpsampleTexels
{
    float2 ndc;

    float scene_depth(int2 pixel)
    {
        return sceneDepth.Load(int3(pixel, 0)).r;
    }

    float4 half_cloud(int2 texel)
    {
        return halfClouds.Load(int3(texel, 0));
    }

    float scene_distance(float depth)
    {
        return cloud_scene_distance(frame.inv_view_proj, ndc, depth, frame.eye.xyz);
    }
};

int2 scene_depth_size()
{
    uint width, height;
    sceneDepth.GetDimensions(width, height);
    return int2(int(width), int(height));
}

// Returned directly: a copy into a local struct makes Slang emit FullscreenVsOut with two @location(0), which Tint rejects.
[shader("vertex")]
FullscreenVsOut vs_main(uint vid : SV_VertexID)
{
    return fullscreen_vs(vid);
}

[shader("fragment")]
float4 fs_main(FullscreenVsOut In) : SV_Target
{
    float2 ndc = fullscreen_uv_to_ndc(In.uv);
    // The march starts at the eye, so it takes the camera ray's direction alone.
    float3 rayDirection = camera_ray(frame.inv_view_proj, ndc).direction;

    int3 pixel = int3(int2(In.svPosition.xy), 0);
    PixelTexels texels = { ndc };
    float depth = frame.depth_spread.y > 0.5 ? half_texel_depth(texels, pixel.xy, scene_depth_size())
                                             : sceneDepth.Load(pixel).r;
    float sceneDistance = cloud_scene_distance(frame.inv_view_proj, ndc, depth, frame.eye.xyz);

    SampledNoise noise;
    Clouds cloud = cloud_unpack(frame.params);
    CloudAmbient ambient;
    ambient.top = frame.ambient_top.rgb;
    ambient.bottom = frame.ambient_bottom.rgb;
    float4 nearest = march_clouds(noise, frame.eye.xyz, rayDirection, sceneDistance, cloud, frame.to_sun.xyz,
                                  frame.sun_radiance.rgb, ambient);
    if (frame.depth_spread.x < 0.5)
        return nearest;
    float2 spread = depthSpread.Load(pixel).rg;
    if (spread.y <= 0.0 || spread.x <= depth)
        return nearest;
    // A silhouette pixel: the samples behind the nearest surface see the cloud beyond it.
    float farDistance = cloud_scene_distance(frame.inv_view_proj, ndc, spread.x, frame.eye.xyz);
    float4 farthest = march_clouds(noise, frame.eye.xyz, rayDirection, farDistance, cloud, frame.to_sun.xyz,
                                   frame.sun_radiance.rgb, ambient);
    return lerp(nearest, farthest, spread.y);
}

// The half-resolution march at full resolution, cloud_upsample.slang's four-texel blend.
[shader("fragment")]
float4 fs_upsample(FullscreenVsOut In) : SV_Target
{
    PixelTexels texels = { fullscreen_uv_to_ndc(In.uv) };
    uint width, height;
    halfClouds.GetDimensions(width, height);
    return cloud_upsample(texels, int2(In.svPosition.xy), int2(int(width), int(height)), scene_depth_size());
}
