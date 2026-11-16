import volumetric_clouds;

// CI-only entry point: proves volumetric_clouds.slang emits SPIR-V the way the C++ engine imports it; nothing loads it.

[vk::binding(0, 0)] RWTexture2D<float4> outputImage;
[vk::binding(1, 0)] Sampler3D noiseTexture;
[vk::binding(2, 0)] RWTexture3D<float4> noiseVolume;

struct CombinedNoise : ICloudNoise
{
    float4 sample_noise(float3 uvw)
    {
        return noiseTexture.SampleLevel(uvw, 0.0);
    }
};

[shader("compute")]
[numthreads(16, 16, 1)]
void clouds_test_main(uint3 tid : SV_DispatchThreadID)
{
    CloudParams params;
    params.cloudMeshScale = float4(630.0, 3.15, 630.0, 0.63);
    params.cloudMeshOffset = float4(-0.364, 367.0, -18.351, 0.493);
    params.cloudParameters = float4(0.966, 0.034, 1.0, 8.0);
    params.cloudLightMarch = float4(3.0, 0.0, 0.0, 0.0);
    float4x4 identity = float4x4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0);
    float2 ndc = float2(tid.xy) / 8.0 - 1.0;
    float sceneDistance = cloud_scene_distance(identity, ndc, 0.5, float3(0.0));

    CombinedNoise noise;
    float3 rayDirection = normalize(float3(ndc.x, 1.0, ndc.y));
    outputImage[tid.xy] = march_clouds(noise, float3(0.0), rayDirection, sceneDistance, cloud_unpack(params),
                                       float3(0.5, 0.8, 0.3), float3(5.0));
    noiseVolume[uint3(tid.xy, 0)] = noise_volume_texel(uint3(tid.xy, 0));
}
