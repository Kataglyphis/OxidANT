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
    // x: 1.0 while depthSpread holds this frame's MSAA spread, the forward path's.
    float4 depth_spread;
};
[vk::binding(0, 0)] ConstantBuffer<CloudFrame> frame;
[vk::binding(1, 0)] Texture3D<float4> noiseTex;
[vk::binding(2, 0)] SamplerState noiseSampler;
// The resolved scene depth, bound as unfilterable float: depth formats allow it, and no WGSL patch is needed.
[vk::binding(3, 0)] Texture2D<float4> sceneDepth;
// The farthest MSAA sample and the share of samples at it, from depth_resolve.slang.
[vk::binding(4, 0)] Texture2D<float4> depthSpread;

struct SampledNoise : ICloudNoise
{
    // Explicit LOD: the march loops, and implicit derivatives need uniform control flow.
    float4 sample_noise(float3 uvw)
    {
        return noiseTex.SampleLevel(noiseSampler, uvw, 0.0);
    }
};

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
    float4 nearPoint = mul(frame.inv_view_proj, float4(ndc, 0.0, 1.0));
    float4 farPoint = mul(frame.inv_view_proj, float4(ndc, 1.0, 1.0));
    float3 rayDirection = normalize(farPoint.xyz / farPoint.w - nearPoint.xyz / nearPoint.w);

    int3 pixel = int3(int2(In.svPosition.xy), 0);
    float depth = sceneDepth.Load(pixel).r;
    float sceneDistance = cloud_scene_distance(frame.inv_view_proj, ndc, depth, frame.eye.xyz);

    SampledNoise noise;
    Clouds cloud = cloud_unpack(frame.params);
    float4 nearest = march_clouds(noise, frame.eye.xyz, rayDirection, sceneDistance, cloud, frame.to_sun.xyz,
                                  frame.sun_radiance.rgb);
    if (frame.depth_spread.x < 0.5)
        return nearest;
    float2 spread = depthSpread.Load(pixel).rg;
    if (spread.y <= 0.0 || spread.x <= depth)
        return nearest;
    // A silhouette pixel: the samples behind the nearest surface see the cloud beyond it.
    float farDistance = cloud_scene_distance(frame.inv_view_proj, ndc, spread.x, frame.eye.xyz);
    float4 farthest = march_clouds(noise, frame.eye.xyz, rayDirection, farDistance, cloud, frame.to_sun.xyz,
                                   frame.sun_radiance.rgb);
    return lerp(nearest, farthest, spread.y);
}
