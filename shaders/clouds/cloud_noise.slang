import volumetric_clouds;

// Rust/WebGPU noise volume: cloud_noise.wgsl is generated from this file; bindings match forward/cloud_pass.rs.

// WebGPU stores rgba16float only through a write-only storage texture, which RWTexture3D would not emit.
[vk::binding(0, 0)] [format("rgba16f")] WTexture3D<float4> noiseVolume;

// 256 invocations, WebGPU's default per-workgroup limit, which the C++ engine's 8x8x8 exceeds.
[shader("compute")]
[numthreads(8, 8, 4)]
void cs_noise(uint3 tid : SV_DispatchThreadID)
{
    noiseVolume.Store(tid, noise_volume_texel(tid));
}
