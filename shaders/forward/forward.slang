import forward_lighting;
import fullscreen;
import gltf_material;

// Rust/WebGPU forward PBR pass and its deferred twin: the checked-in forward.wgsl is generated from this file.

struct PrimUniforms
{
    float4x4 model;
    float4x4 normal_matrix;
    float4 base_color;
    float4 material_factors;  // x: metallic, y: roughness, z: occlusion, w: normal scale
    float4 emissive_factor;   // rgb: emissive, w: alphaCutoff
    float4 base_uv_row0;
    float4 base_uv_row1;
    float4 mr_uv_row0;
    float4 mr_uv_row1;
    float4 normal_uv_row0;
    float4 normal_uv_row1;
    float4 emissive_uv_row0;
    float4 emissive_uv_row1;
    float4 occlusion_uv_row0;
    float4 occlusion_uv_row1;
    float4 material_flags;    // x: unlit, y: uv mask bits, z: alpha_mode.slang's mode
};

// The primitive's glTF alphaMode.
int prim_alpha_mode()
{
    return int(prim.material_flags.z);
}

// Per-slot UV accessors, so no call site pairs one slot's rows with another's; base colour is split across stages.

// Base-colour slot's uv-set selection (mask bit 1).
float2 base_color_uv_select(float2 uv0, float2 uv1)
{
    return (uint(prim.material_flags.y) & 1u) != 0u ? uv1 : uv0;
}

// Base-colour slot's KHR_texture_transform.
float2 base_color_uv_rows(float2 uv)
{
    return transform_uv(uv, prim.base_uv_row0.xyz, prim.base_uv_row1.xyz);
}

// Base-colour slot's full UV: selection composed with the row pair.
float2 base_color_uv(float2 uv0, float2 uv1)
{
    return base_color_uv_rows(base_color_uv_select(uv0, uv1));
}

// Metallic-roughness slot's uv-set selection (mask bit 2) and KHR_texture_transform.
float2 metallic_roughness_uv(float2 uv0, float2 uv1)
{
    float2 uv = (uint(prim.material_flags.y) & 2u) != 0u ? uv1 : uv0;
    return transform_uv(uv, prim.mr_uv_row0.xyz, prim.mr_uv_row1.xyz);
}

// Normal slot's uv-set selection (mask bit 4) and KHR_texture_transform.
float2 normal_uv(float2 uv0, float2 uv1)
{
    float2 uv = (uint(prim.material_flags.y) & 4u) != 0u ? uv1 : uv0;
    return transform_uv(uv, prim.normal_uv_row0.xyz, prim.normal_uv_row1.xyz);
}

// Emissive slot's uv-set selection (mask bit 8) and KHR_texture_transform.
float2 emissive_uv(float2 uv0, float2 uv1)
{
    float2 uv = (uint(prim.material_flags.y) & 8u) != 0u ? uv1 : uv0;
    return transform_uv(uv, prim.emissive_uv_row0.xyz, prim.emissive_uv_row1.xyz);
}

// Occlusion slot's uv-set selection (mask bit 16) and KHR_texture_transform; the C++ engine has no such slot.
float2 occlusion_uv(float2 uv0, float2 uv1)
{
    float2 uv = (uint(prim.material_flags.y) & 16u) != 0u ? uv1 : uv0;
    return transform_uv(uv, prim.occlusion_uv_row0.xyz, prim.occlusion_uv_row1.xyz);
}

[vk::binding(0, 0)] ConstantBuffer<PrimUniforms> prim;
[vk::binding(3, 0)] Texture2D<float4> baseColorTex;
[vk::binding(4, 0)] SamplerState baseColorSampler;
[vk::binding(5, 0)] Texture2D<float4> metalRoughTex;
[vk::binding(6, 0)] SamplerState metalRoughSampler;
[vk::binding(7, 0)] Texture2D<float4> normalTex;
[vk::binding(8, 0)] SamplerState normalSampler;
[vk::binding(9, 0)] Texture2D<float4> emissiveTex;
[vk::binding(10, 0)] SamplerState emissiveSampler;
[vk::binding(11, 0)] Texture2D<float4> occlusionTex;
[vk::binding(12, 0)] SamplerState occlusionSampler;
[vk::binding(13, 0)] StructuredBuffer<float4x4> jointMatrices;

// Which view a shadow pass draws: index.x is the cascade, or 3 for a punctual cube face, whose matrix is `face`.
struct ShadowView
{
    uint4 index;
    float4x4 face;
};

// Shares set 1 binding 0 with iblParams: the forward and shadow pipelines use different bind-group layouts.
[vk::binding(0, 1)] ConstantBuffer<ShadowView> shadowView;

float4x4 shadow_view_proj()
{
    uint view = shadowView.index.x;
    if (view == 1u) return frame.light_space_1;
    if (view == 2u) return frame.light_space_2;
    if (view == 3u) return shadowView.face;
    return frame.light_space;
}

// The deferred lighting pass's group 0 starts past the material's bindings, so no binding has two meanings.
[vk::binding(14, 0)] Texture2D<float4> gbufferNormal;
[vk::binding(15, 0)] Texture2D<float4> gbufferAlbedo;
[vk::binding(16, 0)] Texture2D<float4> gbufferMaterial;
[vk::binding(17, 0)] Texture2D<float> gbufferDepth;

struct DeferredUniforms
{
    float4x4 inv_view_proj;
};
[vk::binding(18, 0)] ConstantBuffer<DeferredUniforms> deferredParams;

struct VsIn
{
    float3 position : POSITION;
    float3 normal : NORMAL;
    float2 uv : TEXCOORD0;
    float4 tangent : TANGENT;
    float4 joints : JOINTS;
    float4 weights : WEIGHTS;
    float4 color : COLOR;
    float2 uv1 : TEXCOORD1;
    float4 instance0 : INSTANCE0;
    float4 instance1 : INSTANCE1;
    float4 instance2 : INSTANCE2;
    float4 instance3 : INSTANCE3;
};

float4x4 instance_matrix(VsIn In)
{
    // Instance attributes are columns, while the Slang constructor takes rows.
    return transpose(float4x4(In.instance0, In.instance1, In.instance2, In.instance3));
}

float4x4 skin_matrix(VsIn In)
{
    float4 w = In.weights;
    float total = w.x + w.y + w.z + w.w;
    if (total <= 0.0001) return prim.model;
    // Scale each joint matrix by its weight, then sum.
    float4x4 m = jointMatrices[uint(In.joints.x)] * w.x;
    m += jointMatrices[uint(In.joints.y)] * w.y;
    m += jointMatrices[uint(In.joints.z)] * w.z;
    m += jointMatrices[uint(In.joints.w)] * w.w;
    return m * (1.0 / total);
}

struct VsOut
{
    float4 svPosition : SV_Position;
    float3 worldNormal : TEXCOORD0;
    float2 uv : TEXCOORD1;
    float4 lightSpacePos : TEXCOORD2;
    float4 worldTangent : TEXCOORD3;
    float3 worldPosition : TEXCOORD4;
    float4 vertexColor : TEXCOORD6;
    float2 uv1 : TEXCOORD7;
};

[shader("vertex")]
VsOut vs_main(VsIn In)
{
    VsOut o;
    float4x4 model = mul(instance_matrix(In), skin_matrix(In));
    float4 worldPos = mul(model, float4(In.position, 1.0));
    o.svPosition = mul(frame.view_proj, worldPos);
    // A collapsed vertex passes zero, which leaves the interpolated direction to its triangle's other vertices.
    if (In.weights.x + In.weights.y + In.weights.z + In.weights.w > 0.0001)
    {
        o.worldNormal = normalize_or(mul(normal_transform((float3x3)model), In.normal), float3(0.0));
    }
    else
    {
        float3 nModel = mul(prim.normal_matrix, float4(In.normal, 0.0)).xyz;
        o.worldNormal = normalize_or(mul(normal_transform((float3x3)instance_matrix(In)), nModel), float3(0.0));
    }
    // A mirroring transform reverses the tangent frame's handedness, so the bitangent sign follows.
    float handedness = In.tangent.w * mirror_sign((float3x3)model);
    o.worldTangent = float4(normalize_or(mul(model, float4(In.tangent.xyz, 0.0)).xyz, float3(0.0)), handedness);
    o.uv = In.uv;
    o.lightSpacePos = mul(frame.light_space, worldPos);
    o.worldPosition = worldPos.xyz;
    o.vertexColor = In.color;
    o.uv1 = In.uv1;
    return o;
}

[shader("vertex")]
float4 vs_shadow(VsIn In) : SV_Position
{
    float4 world = mul(mul(instance_matrix(In), skin_matrix(In)), float4(In.position, 1.0));
    return mul(shadow_view_proj(), world);
}

struct VsShadowMaskedOut
{
    float4 pos : SV_Position;
    float2 uv : TEXCOORD0;
    float alpha : TEXCOORD1;
};

[shader("vertex")]
VsShadowMaskedOut vs_shadow_masked(VsIn In)
{
    VsShadowMaskedOut o;
    float4 world = mul(mul(instance_matrix(In), skin_matrix(In)), float4(In.position, 1.0));
    o.pos = mul(shadow_view_proj(), world);
    o.uv = base_color_uv_select(In.uv, In.uv1);
    o.alpha = In.color.a;
    return o;
}

[shader("fragment")]
void fs_shadow_masked(VsShadowMaskedOut In)
{
    float2 uv = base_color_uv_rows(In.uv);
    float alpha = prim.base_color.a * baseColorTex.Sample(baseColorSampler, uv, 0).a * In.alpha;
    if (alpha_masked_out(prim_alpha_mode(), alpha, prim.emissive_factor.w)) discard;
}

// The material at a fragment, where fs_main and fs_gbuffer both start.
struct MaterialSample
{
    SurfacePoint surface;
    float alpha;
};

MaterialSample sample_material(VsOut In, bool isFrontFace)
{
    float2 baseUv = base_color_uv(In.uv, In.uv1);
    float2 mrUv = metallic_roughness_uv(In.uv, In.uv1);
    float2 normalUv = normal_uv(In.uv, In.uv1);
    float2 emissiveUv = emissive_uv(In.uv, In.uv1);
    float2 occlusionUv = occlusion_uv(In.uv, In.uv1);

    float4 baseSample = baseColorTex.Sample(baseColorSampler, baseUv);
    float4 mrSample = metalRoughTex.Sample(metalRoughSampler, mrUv);
    float4 normalSample = normalTex.Sample(normalSampler, normalUv);
    float4 emissiveSample = emissiveTex.Sample(emissiveSampler, emissiveUv);
    float4 occlusionSample = occlusionTex.Sample(occlusionSampler, occlusionUv);

    float4 albedo = prim.base_color * baseSample * In.vertexColor;
    MaterialSample m;
    m.alpha = albedo.a;
    m.surface.position = In.worldPosition;
    m.surface.albedo = albedo.rgb;
    m.surface.metallic = material_metallic(prim.material_factors.x, mrSample.b);
    m.surface.roughness = material_roughness(prim.material_factors.y, mrSample.g);
    m.surface.occlusion = lerp(1.0, occlusionSample.r, prim.material_factors.z);
    m.surface.emissive = prim.emissive_factor.rgb * emissiveSample.rgb;

    float3 v = normalize(frame.camera_position.xyz - In.worldPosition);
    // A fragment whose vertices all collapsed faces the camera; a zero tangent leaves the mapped normal at nGeom.
    float3 nGeom = normalize_or(In.worldNormal, v);
    float3 nTs = tangent_space_normal(normalSample.xyz, prim.material_factors.w);
    float3 n = apply_normal_map(nGeom, In.worldTangent, nTs, isFrontFace);
    m.surface.n = n;
    return m;
}

[shader("fragment")]
float4 fs_main(VsOut In, bool isFrontFace : SV_IsFrontFace) : SV_Target
{
    MaterialSample m = sample_material(In, isFrontFace);
    if (alpha_masked_out(prim_alpha_mode(), m.alpha, prim.emissive_factor.w)) discard;
    if (prim.material_flags.x > 0.5) return float4(m.surface.albedo, m.alpha);
    return float4(shade_surface(m.surface, In.svPosition), m.alpha);
}

// 24 bytes a sample, under WebGPU's 32: metallic's sign carries unlit, since albedo's alpha holds the occlusion.
struct GBufferOut
{
    float4 normalMetallic : SV_Target0;
    float4 albedoOcclusion : SV_Target1;
    float4 roughnessEmissive : SV_Target2;
};

[shader("fragment")]
GBufferOut fs_gbuffer(VsOut In, bool isFrontFace : SV_IsFrontFace)
{
    MaterialSample m = sample_material(In, isFrontFace);
    if (alpha_masked_out(prim_alpha_mode(), m.alpha, prim.emissive_factor.w)) discard;
    SurfacePoint s = m.surface;
    GBufferOut g;
    g.normalMetallic = float4(s.n, prim.material_flags.x > 0.5 ? -1.0 : s.metallic);
    g.albedoOcclusion = float4(s.albedo, s.occlusion);
    // Half floats keep an emissive strength above 1, which an 8-bit target would clip.
    g.roughnessEmissive = float4(s.roughness, s.emissive);
    return g;
}

// Returned directly: a copy into a local struct makes Slang emit FullscreenVsOut with two @location(0), which Tint rejects.
[shader("vertex")]
FullscreenVsOut vs_deferred_lighting(uint vid : SV_VertexID)
{
    return fullscreen_vs(vid);
}

[shader("fragment")]
float4 fs_deferred_lighting(FullscreenVsOut In) : SV_Target
{
    int3 texel = int3(int2(In.svPosition.xy), 0);
    float depth = gbufferDepth.Load(texel);
    // Nothing was drawn here: the sky fills it after this pass, testing the same depth.
    if (depth >= 1.0) discard;
    float4 normalMetallic = gbufferNormal.Load(texel);
    float4 albedoOcclusion = gbufferAlbedo.Load(texel);
    if (normalMetallic.w < -0.5) return float4(albedoOcclusion.rgb, 1.0);
    float4 roughnessEmissive = gbufferMaterial.Load(texel);
    float4 world = mul(deferredParams.inv_view_proj, float4(fullscreen_uv_to_ndc(In.uv), depth, 1.0));

    SurfacePoint s;
    s.position = world.xyz / world.w;
    s.n = normalize_or(normalMetallic.xyz, float3(0.0, 1.0, 0.0));
    s.albedo = albedoOcclusion.rgb;
    s.metallic = normalMetallic.w;
    s.roughness = roughnessEmissive.r;
    s.occlusion = albedoOcclusion.a;
    s.emissive = roughnessEmissive.gba;
    return float4(shade_surface(s, In.svPosition), 1.0);
}
