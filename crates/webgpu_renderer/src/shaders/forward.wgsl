struct _MatrixStorage_float4x4_ColMajorstd140_0
{
    @align(16) data_0 : array<vec4<f32>, i32(4)>,
};

struct PrimUniforms_std140_0
{
    @align(16) model_0 : _MatrixStorage_float4x4_ColMajorstd140_0,
    @align(16) normal_matrix_0 : _MatrixStorage_float4x4_ColMajorstd140_0,
    @align(16) base_color_0 : vec4<f32>,
    @align(16) material_factors_0 : vec4<f32>,
    @align(16) emissive_factor_0 : vec4<f32>,
    @align(16) base_uv_row0_0 : vec4<f32>,
    @align(16) base_uv_row1_0 : vec4<f32>,
    @align(16) mr_uv_row0_0 : vec4<f32>,
    @align(16) mr_uv_row1_0 : vec4<f32>,
    @align(16) normal_uv_row0_0 : vec4<f32>,
    @align(16) normal_uv_row1_0 : vec4<f32>,
    @align(16) emissive_uv_row0_0 : vec4<f32>,
    @align(16) emissive_uv_row1_0 : vec4<f32>,
    @align(16) occlusion_uv_row0_0 : vec4<f32>,
    @align(16) occlusion_uv_row1_0 : vec4<f32>,
    @align(16) material_flags_0 : vec4<f32>,
};

@binding(0) @group(0) var<uniform> prim_0 : PrimUniforms_std140_0;
struct _MatrixStorage_float4x4_ColMajorstd430_0
{
    @align(16) data_1 : array<vec4<f32>, i32(4)>,
};

@binding(13) @group(0) var<storage, read> jointMatrices_0 : array<_MatrixStorage_float4x4_ColMajorstd430_0>;

struct FrameUniforms_std140_0
{
    @align(16) view_proj_0 : _MatrixStorage_float4x4_ColMajorstd140_0,
    @align(16) light_space_0 : _MatrixStorage_float4x4_ColMajorstd140_0,
    @align(16) light_space_1_0 : _MatrixStorage_float4x4_ColMajorstd140_0,
    @align(16) light_space_2_0 : _MatrixStorage_float4x4_ColMajorstd140_0,
    @align(16) light_dir_ambient_0 : vec4<f32>,
    @align(16) light_color_intensity_0 : vec4<f32>,
    @align(16) camera_position_0 : vec4<f32>,
    @align(16) cascade_splits_0 : vec4<f32>,
    @align(16) shadow_params_0 : vec4<f32>,
    @align(16) shadow_view_dir_0 : vec4<f32>,
    @align(16) shadow_bias_scale_0 : vec4<f32>,
};

@binding(0) @group(2) var<uniform> frame_0 : FrameUniforms_std140_0;
@binding(0) @group(1) var<uniform> shadowCascadeIndex_0 : vec4<u32>;
@binding(3) @group(0) var baseColorTex_0 : texture_2d<f32>;

@binding(4) @group(0) var baseColorSampler_0 : sampler;

@binding(5) @group(0) var metalRoughTex_0 : texture_2d<f32>;

@binding(6) @group(0) var metalRoughSampler_0 : sampler;

@binding(7) @group(0) var normalTex_0 : texture_2d<f32>;

@binding(8) @group(0) var normalSampler_0 : sampler;

@binding(9) @group(0) var emissiveTex_0 : texture_2d<f32>;

@binding(10) @group(0) var emissiveSampler_0 : sampler;

@binding(11) @group(0) var occlusionTex_0 : texture_2d<f32>;

@binding(12) @group(0) var occlusionSampler_0 : sampler;

@binding(1) @group(0) var shadowMap_0 : texture_depth_2d_array;

@binding(2) @group(0) var shadowSampler_0 : sampler_comparison;

@binding(1) @group(3) var<storage, read> tileLightGrid_0 : array<vec2<u32>>;

@binding(2) @group(3) var<storage, read> tileLightIndices_0 : array<u32>;

@binding(0) @group(3) var<storage, read> punctualLights_0 : array<vec4<f32>>;

struct IblParams_std140_0
{
    @align(16) enabled_maxmip_intensity_0 : vec4<f32>,
};

@binding(0) @group(1) var<uniform> iblParams_0 : IblParams_std140_0;
@binding(1) @group(1) var irradianceMap_0 : texture_cube<f32>;

@binding(4) @group(1) var iblSampler_0 : sampler;

@binding(2) @group(1) var prefilteredMap_0 : texture_cube<f32>;

@binding(3) @group(1) var brdfLut_0 : texture_2d<f32>;

@binding(17) @group(0) var gbufferDepth_0 : texture_depth_2d;

@binding(14) @group(0) var gbufferNormal_0 : texture_2d<f32>;

@binding(15) @group(0) var gbufferAlbedo_0 : texture_2d<f32>;

@binding(16) @group(0) var gbufferMaterial_0 : texture_2d<f32>;

struct DeferredUniforms_std140_0
{
    @align(16) inv_view_proj_0 : _MatrixStorage_float4x4_ColMajorstd140_0,
};

@binding(18) @group(0) var<uniform> deferredParams_0 : DeferredUniforms_std140_0;
struct VsIn_0
{
     position_0 : vec3<f32>,
     normal_0 : vec3<f32>,
     uv_0 : vec2<f32>,
     tangent_0 : vec4<f32>,
     joints_0 : vec4<f32>,
     weights_0 : vec4<f32>,
     color_0 : vec4<f32>,
     uv1_0 : vec2<f32>,
     instance0_0 : vec4<f32>,
     instance1_0 : vec4<f32>,
     instance2_0 : vec4<f32>,
     instance3_0 : vec4<f32>,
};

fn instance_matrix_0( In_0 : VsIn_0) -> mat4x4<f32>
{
    return transpose(mat4x4<f32>(In_0.instance0_0, In_0.instance1_0, In_0.instance2_0, In_0.instance3_0));
}

fn skin_matrix_0( In_1 : VsIn_0) -> mat4x4<f32>
{
    var _S1 : f32 = In_1.weights_0.x;
    var _S2 : f32 = In_1.weights_0.y;
    var _S3 : f32 = In_1.weights_0.z;
    var _S4 : f32 = In_1.weights_0.w;
    var total_0 : f32 = _S1 + _S2 + _S3 + _S4;
    if(total_0 <= 0.00009999999747379f)
    {
        return mat4x4<f32>(prim_0.model_0.data_0[i32(0)][i32(0)], prim_0.model_0.data_0[i32(1)][i32(0)], prim_0.model_0.data_0[i32(2)][i32(0)], prim_0.model_0.data_0[i32(3)][i32(0)], prim_0.model_0.data_0[i32(0)][i32(1)], prim_0.model_0.data_0[i32(1)][i32(1)], prim_0.model_0.data_0[i32(2)][i32(1)], prim_0.model_0.data_0[i32(3)][i32(1)], prim_0.model_0.data_0[i32(0)][i32(2)], prim_0.model_0.data_0[i32(1)][i32(2)], prim_0.model_0.data_0[i32(2)][i32(2)], prim_0.model_0.data_0[i32(3)][i32(2)], prim_0.model_0.data_0[i32(0)][i32(3)], prim_0.model_0.data_0[i32(1)][i32(3)], prim_0.model_0.data_0[i32(2)][i32(3)], prim_0.model_0.data_0[i32(3)][i32(3)]);
    }
    var _S5 : mat4x4<f32> = mat4x4<f32>(jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(0)][i32(0)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(1)][i32(0)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(2)][i32(0)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(3)][i32(0)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(0)][i32(1)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(1)][i32(1)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(2)][i32(1)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(3)][i32(1)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(0)][i32(2)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(1)][i32(2)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(2)][i32(2)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(3)][i32(2)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(0)][i32(3)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(1)][i32(3)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(2)][i32(3)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(3)][i32(3)]);
    var _S6 : mat4x4<f32> = mat4x4<f32>(_S1, _S1, _S1, _S1, _S1, _S1, _S1, _S1, _S1, _S1, _S1, _S1, _S1, _S1, _S1, _S1);
    var _S7 : mat4x4<f32> = mat4x4<f32>(jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(0)][i32(0)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(1)][i32(0)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(2)][i32(0)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(3)][i32(0)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(0)][i32(1)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(1)][i32(1)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(2)][i32(1)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(3)][i32(1)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(0)][i32(2)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(1)][i32(2)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(2)][i32(2)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(3)][i32(2)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(0)][i32(3)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(1)][i32(3)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(2)][i32(3)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(3)][i32(3)]);
    var _S8 : mat4x4<f32> = mat4x4<f32>(_S2, _S2, _S2, _S2, _S2, _S2, _S2, _S2, _S2, _S2, _S2, _S2, _S2, _S2, _S2, _S2);
    var _S9 : mat4x4<f32> = mat4x4<f32>(jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(0)][i32(0)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(1)][i32(0)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(2)][i32(0)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(3)][i32(0)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(0)][i32(1)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(1)][i32(1)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(2)][i32(1)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(3)][i32(1)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(0)][i32(2)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(1)][i32(2)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(2)][i32(2)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(3)][i32(2)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(0)][i32(3)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(1)][i32(3)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(2)][i32(3)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(3)][i32(3)]);
    var _S10 : mat4x4<f32> = mat4x4<f32>(_S3, _S3, _S3, _S3, _S3, _S3, _S3, _S3, _S3, _S3, _S3, _S3, _S3, _S3, _S3, _S3);
    var _S11 : mat4x4<f32> = mat4x4<f32>(jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(0)][i32(0)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(1)][i32(0)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(2)][i32(0)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(3)][i32(0)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(0)][i32(1)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(1)][i32(1)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(2)][i32(1)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(3)][i32(1)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(0)][i32(2)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(1)][i32(2)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(2)][i32(2)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(3)][i32(2)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(0)][i32(3)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(1)][i32(3)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(2)][i32(3)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(3)][i32(3)]);
    var _S12 : mat4x4<f32> = mat4x4<f32>(_S4, _S4, _S4, _S4, _S4, _S4, _S4, _S4, _S4, _S4, _S4, _S4, _S4, _S4, _S4, _S4);
    var _S13 : mat4x4<f32> = mat4x4<f32>(_S5[0] * _S6[0], _S5[1] * _S6[1], _S5[2] * _S6[2], _S5[3] * _S6[3]) + mat4x4<f32>(_S7[0] * _S8[0], _S7[1] * _S8[1], _S7[2] * _S8[2], _S7[3] * _S8[3]) + mat4x4<f32>(_S9[0] * _S10[0], _S9[1] * _S10[1], _S9[2] * _S10[2], _S9[3] * _S10[3]) + mat4x4<f32>(_S11[0] * _S12[0], _S11[1] * _S12[1], _S11[2] * _S12[2], _S11[3] * _S12[3]);
    var _S14 : mat4x4<f32> = mat4x4<f32>(1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0);
    return mat4x4<f32>(_S13[0] * _S14[0], _S13[1] * _S14[1], _S13[2] * _S14[2], _S13[3] * _S14[3]);
}

fn mirror_sign_0( m_0 : mat4x4<f32>) -> f32
{
    var _S15 : f32;
    if((dot(m_0[i32(0)].xyz, cross(m_0[i32(1)].xyz, m_0[i32(2)].xyz))) < 0.0f)
    {
        _S15 = -1.0f;
    }
    else
    {
        _S15 = 1.0f;
    }
    return _S15;
}

fn normal_transform_0( m_1 : mat4x4<f32>) -> mat3x3<f32>
{
    var r0_0 : vec3<f32> = m_1[i32(0)].xyz;
    var r1_0 : vec3<f32> = m_1[i32(1)].xyz;
    var r2_0 : vec3<f32> = m_1[i32(2)].xyz;
    var _S16 : mat3x3<f32> = mat3x3<f32>(cross(r1_0, r2_0), cross(r2_0, r0_0), cross(r0_0, r1_0));
    var _S17 : mat3x3<f32> = mat3x3<f32>(mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1));
    return mat3x3<f32>(_S16[0] * _S17[0], _S16[1] * _S17[1], _S16[2] * _S17[2]);
}

fn normalize_or_0( v_0 : vec3<f32>,  fallback_0 : vec3<f32>) -> vec3<f32>
{
    var _S18 : vec3<f32>;
    if((dot(v_0, v_0)) > 1.00000000317107685e-30f)
    {
        _S18 = normalize(v_0);
    }
    else
    {
        _S18 = fallback_0;
    }
    return _S18;
}

struct VsOut_0
{
    @builtin(position) svPosition_0 : vec4<f32>,
    @location(0) worldNormal_0 : vec3<f32>,
    @location(1) uv_1 : vec2<f32>,
    @location(2) lightSpacePos_0 : vec4<f32>,
    @location(3) worldTangent_0 : vec4<f32>,
    @location(4) worldPosition_0 : vec3<f32>,
    @location(6) vertexColor_0 : vec4<f32>,
    @location(7) uv1_1 : vec2<f32>,
};

struct vertexInput_0
{
    @location(0) position_1 : vec3<f32>,
    @location(4) normal_1 : vec3<f32>,
    @location(5) uv_2 : vec2<f32>,
    @location(6) tangent_1 : vec4<f32>,
    @location(7) joints_1 : vec4<f32>,
    @location(8) weights_1 : vec4<f32>,
    @location(9) color_1 : vec4<f32>,
    @location(1) uv1_2 : vec2<f32>,
    @location(10) instance0_1 : vec4<f32>,
    @location(11) instance1_1 : vec4<f32>,
    @location(2) instance2_1 : vec4<f32>,
    @location(3) instance3_1 : vec4<f32>,
};

@vertex
fn vs_main( _S19 : vertexInput_0) -> VsOut_0
{
    var _S20 : VsIn_0 = VsIn_0( _S19.position_1, _S19.normal_1, _S19.uv_2, _S19.tangent_1, _S19.joints_1, _S19.weights_1, _S19.color_1, _S19.uv1_2, _S19.instance0_1, _S19.instance1_1, _S19.instance2_1, _S19.instance3_1 );
    var _S21 : mat4x4<f32> = instance_matrix_0(_S20);
    var model_1 : mat4x4<f32> = (((skin_matrix_0(_S20)) * (_S21)));
    var worldPos_0 : vec4<f32> = (((vec4<f32>(_S19.position_1, 1.0f)) * (model_1)));
    var o_0 : VsOut_0;
    o_0.svPosition_0 = (((worldPos_0) * (mat4x4<f32>(frame_0.view_proj_0.data_0[i32(0)][i32(0)], frame_0.view_proj_0.data_0[i32(1)][i32(0)], frame_0.view_proj_0.data_0[i32(2)][i32(0)], frame_0.view_proj_0.data_0[i32(3)][i32(0)], frame_0.view_proj_0.data_0[i32(0)][i32(1)], frame_0.view_proj_0.data_0[i32(1)][i32(1)], frame_0.view_proj_0.data_0[i32(2)][i32(1)], frame_0.view_proj_0.data_0[i32(3)][i32(1)], frame_0.view_proj_0.data_0[i32(0)][i32(2)], frame_0.view_proj_0.data_0[i32(1)][i32(2)], frame_0.view_proj_0.data_0[i32(2)][i32(2)], frame_0.view_proj_0.data_0[i32(3)][i32(2)], frame_0.view_proj_0.data_0[i32(0)][i32(3)], frame_0.view_proj_0.data_0[i32(1)][i32(3)], frame_0.view_proj_0.data_0[i32(2)][i32(3)], frame_0.view_proj_0.data_0[i32(3)][i32(3)]))));
    if((_S19.weights_1.x + _S19.weights_1.y + _S19.weights_1.z + _S19.weights_1.w) > 0.00009999999747379f)
    {
        o_0.worldNormal_0 = normalize_or_0((((_S19.normal_1) * (normal_transform_0(model_1)))), vec3<f32>(0.0f));
    }
    else
    {
        o_0.worldNormal_0 = normalize_or_0(((((((vec4<f32>(_S19.normal_1, 0.0f)) * (mat4x4<f32>(prim_0.normal_matrix_0.data_0[i32(0)][i32(0)], prim_0.normal_matrix_0.data_0[i32(1)][i32(0)], prim_0.normal_matrix_0.data_0[i32(2)][i32(0)], prim_0.normal_matrix_0.data_0[i32(3)][i32(0)], prim_0.normal_matrix_0.data_0[i32(0)][i32(1)], prim_0.normal_matrix_0.data_0[i32(1)][i32(1)], prim_0.normal_matrix_0.data_0[i32(2)][i32(1)], prim_0.normal_matrix_0.data_0[i32(3)][i32(1)], prim_0.normal_matrix_0.data_0[i32(0)][i32(2)], prim_0.normal_matrix_0.data_0[i32(1)][i32(2)], prim_0.normal_matrix_0.data_0[i32(2)][i32(2)], prim_0.normal_matrix_0.data_0[i32(3)][i32(2)], prim_0.normal_matrix_0.data_0[i32(0)][i32(3)], prim_0.normal_matrix_0.data_0[i32(1)][i32(3)], prim_0.normal_matrix_0.data_0[i32(2)][i32(3)], prim_0.normal_matrix_0.data_0[i32(3)][i32(3)])))).xyz) * (normal_transform_0(_S21)))), vec3<f32>(0.0f));
    }
    o_0.worldTangent_0 = vec4<f32>(normalize_or_0((((vec4<f32>(_S19.tangent_1.xyz, 0.0f)) * (model_1))).xyz, vec3<f32>(0.0f)), _S19.tangent_1.w * mirror_sign_0(model_1));
    o_0.uv_1 = _S19.uv_2;
    o_0.lightSpacePos_0 = (((worldPos_0) * (mat4x4<f32>(frame_0.light_space_0.data_0[i32(0)][i32(0)], frame_0.light_space_0.data_0[i32(1)][i32(0)], frame_0.light_space_0.data_0[i32(2)][i32(0)], frame_0.light_space_0.data_0[i32(3)][i32(0)], frame_0.light_space_0.data_0[i32(0)][i32(1)], frame_0.light_space_0.data_0[i32(1)][i32(1)], frame_0.light_space_0.data_0[i32(2)][i32(1)], frame_0.light_space_0.data_0[i32(3)][i32(1)], frame_0.light_space_0.data_0[i32(0)][i32(2)], frame_0.light_space_0.data_0[i32(1)][i32(2)], frame_0.light_space_0.data_0[i32(2)][i32(2)], frame_0.light_space_0.data_0[i32(3)][i32(2)], frame_0.light_space_0.data_0[i32(0)][i32(3)], frame_0.light_space_0.data_0[i32(1)][i32(3)], frame_0.light_space_0.data_0[i32(2)][i32(3)], frame_0.light_space_0.data_0[i32(3)][i32(3)]))));
    o_0.worldPosition_0 = worldPos_0.xyz;
    o_0.vertexColor_0 = _S19.color_1;
    o_0.uv1_1 = _S19.uv1_2;
    return o_0;
}

struct vertexOutput_0
{
    @builtin(position) output_0 : vec4<f32>,
};

struct vertexInput_1
{
    @location(0) position_2 : vec3<f32>,
    @location(4) normal_2 : vec3<f32>,
    @location(5) uv_3 : vec2<f32>,
    @location(6) tangent_2 : vec4<f32>,
    @location(7) joints_2 : vec4<f32>,
    @location(8) weights_2 : vec4<f32>,
    @location(9) color_2 : vec4<f32>,
    @location(1) uv1_3 : vec2<f32>,
    @location(10) instance0_2 : vec4<f32>,
    @location(11) instance1_2 : vec4<f32>,
    @location(2) instance2_2 : vec4<f32>,
    @location(3) instance3_2 : vec4<f32>,
};

@vertex
fn vs_shadow( _S22 : vertexInput_1) -> vertexOutput_0
{
    var _S23 : VsIn_0 = VsIn_0( _S22.position_2, _S22.normal_2, _S22.uv_3, _S22.tangent_2, _S22.joints_2, _S22.weights_2, _S22.color_2, _S22.uv1_3, _S22.instance0_2, _S22.instance1_2, _S22.instance2_2, _S22.instance3_2 );
    var world_0 : vec4<f32> = (((vec4<f32>(_S22.position_2, 1.0f)) * ((((skin_matrix_0(_S23)) * (instance_matrix_0(_S23)))))));
    var cascade_0 : u32 = shadowCascadeIndex_0.x;
    if(cascade_0 == u32(1))
    {
        var _S24 : vertexOutput_0 = vertexOutput_0( (((world_0) * (mat4x4<f32>(frame_0.light_space_1_0.data_0[i32(0)][i32(0)], frame_0.light_space_1_0.data_0[i32(1)][i32(0)], frame_0.light_space_1_0.data_0[i32(2)][i32(0)], frame_0.light_space_1_0.data_0[i32(3)][i32(0)], frame_0.light_space_1_0.data_0[i32(0)][i32(1)], frame_0.light_space_1_0.data_0[i32(1)][i32(1)], frame_0.light_space_1_0.data_0[i32(2)][i32(1)], frame_0.light_space_1_0.data_0[i32(3)][i32(1)], frame_0.light_space_1_0.data_0[i32(0)][i32(2)], frame_0.light_space_1_0.data_0[i32(1)][i32(2)], frame_0.light_space_1_0.data_0[i32(2)][i32(2)], frame_0.light_space_1_0.data_0[i32(3)][i32(2)], frame_0.light_space_1_0.data_0[i32(0)][i32(3)], frame_0.light_space_1_0.data_0[i32(1)][i32(3)], frame_0.light_space_1_0.data_0[i32(2)][i32(3)], frame_0.light_space_1_0.data_0[i32(3)][i32(3)])))) );
        return _S24;
    }
    if(cascade_0 == u32(2))
    {
        var _S25 : vertexOutput_0 = vertexOutput_0( (((world_0) * (mat4x4<f32>(frame_0.light_space_2_0.data_0[i32(0)][i32(0)], frame_0.light_space_2_0.data_0[i32(1)][i32(0)], frame_0.light_space_2_0.data_0[i32(2)][i32(0)], frame_0.light_space_2_0.data_0[i32(3)][i32(0)], frame_0.light_space_2_0.data_0[i32(0)][i32(1)], frame_0.light_space_2_0.data_0[i32(1)][i32(1)], frame_0.light_space_2_0.data_0[i32(2)][i32(1)], frame_0.light_space_2_0.data_0[i32(3)][i32(1)], frame_0.light_space_2_0.data_0[i32(0)][i32(2)], frame_0.light_space_2_0.data_0[i32(1)][i32(2)], frame_0.light_space_2_0.data_0[i32(2)][i32(2)], frame_0.light_space_2_0.data_0[i32(3)][i32(2)], frame_0.light_space_2_0.data_0[i32(0)][i32(3)], frame_0.light_space_2_0.data_0[i32(1)][i32(3)], frame_0.light_space_2_0.data_0[i32(2)][i32(3)], frame_0.light_space_2_0.data_0[i32(3)][i32(3)])))) );
        return _S25;
    }
    var _S26 : vertexOutput_0 = vertexOutput_0( (((world_0) * (mat4x4<f32>(frame_0.light_space_0.data_0[i32(0)][i32(0)], frame_0.light_space_0.data_0[i32(1)][i32(0)], frame_0.light_space_0.data_0[i32(2)][i32(0)], frame_0.light_space_0.data_0[i32(3)][i32(0)], frame_0.light_space_0.data_0[i32(0)][i32(1)], frame_0.light_space_0.data_0[i32(1)][i32(1)], frame_0.light_space_0.data_0[i32(2)][i32(1)], frame_0.light_space_0.data_0[i32(3)][i32(1)], frame_0.light_space_0.data_0[i32(0)][i32(2)], frame_0.light_space_0.data_0[i32(1)][i32(2)], frame_0.light_space_0.data_0[i32(2)][i32(2)], frame_0.light_space_0.data_0[i32(3)][i32(2)], frame_0.light_space_0.data_0[i32(0)][i32(3)], frame_0.light_space_0.data_0[i32(1)][i32(3)], frame_0.light_space_0.data_0[i32(2)][i32(3)], frame_0.light_space_0.data_0[i32(3)][i32(3)])))) );
    return _S26;
}

fn base_color_uv_select_0( uv0_0 : vec2<f32>,  uv1_4 : vec2<f32>) -> vec2<f32>
{
    var _S27 : vec2<f32>;
    if(((u32(prim_0.material_flags_0.y) & (u32(1)))) != u32(0))
    {
        _S27 = uv1_4;
    }
    else
    {
        _S27 = uv0_0;
    }
    return _S27;
}

struct VsShadowMaskedOut_0
{
    @builtin(position) pos_0 : vec4<f32>,
    @location(0) uv_4 : vec2<f32>,
    @location(1) alpha_0 : f32,
};

struct vertexInput_2
{
    @location(0) position_3 : vec3<f32>,
    @location(4) normal_3 : vec3<f32>,
    @location(5) uv_5 : vec2<f32>,
    @location(6) tangent_3 : vec4<f32>,
    @location(7) joints_3 : vec4<f32>,
    @location(8) weights_3 : vec4<f32>,
    @location(9) color_3 : vec4<f32>,
    @location(1) uv1_5 : vec2<f32>,
    @location(10) instance0_3 : vec4<f32>,
    @location(11) instance1_3 : vec4<f32>,
    @location(2) instance2_3 : vec4<f32>,
    @location(3) instance3_3 : vec4<f32>,
};

@vertex
fn vs_shadow_masked( _S28 : vertexInput_2) -> VsShadowMaskedOut_0
{
    var _S29 : VsIn_0 = VsIn_0( _S28.position_3, _S28.normal_3, _S28.uv_5, _S28.tangent_3, _S28.joints_3, _S28.weights_3, _S28.color_3, _S28.uv1_5, _S28.instance0_3, _S28.instance1_3, _S28.instance2_3, _S28.instance3_3 );
    var o_1 : VsShadowMaskedOut_0;
    var world_1 : vec4<f32> = (((vec4<f32>(_S28.position_3, 1.0f)) * ((((skin_matrix_0(_S29)) * (instance_matrix_0(_S29)))))));
    var cascade_1 : u32 = shadowCascadeIndex_0.x;
    if(cascade_1 == u32(1))
    {
        o_1.pos_0 = (((world_1) * (mat4x4<f32>(frame_0.light_space_1_0.data_0[i32(0)][i32(0)], frame_0.light_space_1_0.data_0[i32(1)][i32(0)], frame_0.light_space_1_0.data_0[i32(2)][i32(0)], frame_0.light_space_1_0.data_0[i32(3)][i32(0)], frame_0.light_space_1_0.data_0[i32(0)][i32(1)], frame_0.light_space_1_0.data_0[i32(1)][i32(1)], frame_0.light_space_1_0.data_0[i32(2)][i32(1)], frame_0.light_space_1_0.data_0[i32(3)][i32(1)], frame_0.light_space_1_0.data_0[i32(0)][i32(2)], frame_0.light_space_1_0.data_0[i32(1)][i32(2)], frame_0.light_space_1_0.data_0[i32(2)][i32(2)], frame_0.light_space_1_0.data_0[i32(3)][i32(2)], frame_0.light_space_1_0.data_0[i32(0)][i32(3)], frame_0.light_space_1_0.data_0[i32(1)][i32(3)], frame_0.light_space_1_0.data_0[i32(2)][i32(3)], frame_0.light_space_1_0.data_0[i32(3)][i32(3)]))));
    }
    else
    {
        if(cascade_1 == u32(2))
        {
            o_1.pos_0 = (((world_1) * (mat4x4<f32>(frame_0.light_space_2_0.data_0[i32(0)][i32(0)], frame_0.light_space_2_0.data_0[i32(1)][i32(0)], frame_0.light_space_2_0.data_0[i32(2)][i32(0)], frame_0.light_space_2_0.data_0[i32(3)][i32(0)], frame_0.light_space_2_0.data_0[i32(0)][i32(1)], frame_0.light_space_2_0.data_0[i32(1)][i32(1)], frame_0.light_space_2_0.data_0[i32(2)][i32(1)], frame_0.light_space_2_0.data_0[i32(3)][i32(1)], frame_0.light_space_2_0.data_0[i32(0)][i32(2)], frame_0.light_space_2_0.data_0[i32(1)][i32(2)], frame_0.light_space_2_0.data_0[i32(2)][i32(2)], frame_0.light_space_2_0.data_0[i32(3)][i32(2)], frame_0.light_space_2_0.data_0[i32(0)][i32(3)], frame_0.light_space_2_0.data_0[i32(1)][i32(3)], frame_0.light_space_2_0.data_0[i32(2)][i32(3)], frame_0.light_space_2_0.data_0[i32(3)][i32(3)]))));
        }
        else
        {
            o_1.pos_0 = (((world_1) * (mat4x4<f32>(frame_0.light_space_0.data_0[i32(0)][i32(0)], frame_0.light_space_0.data_0[i32(1)][i32(0)], frame_0.light_space_0.data_0[i32(2)][i32(0)], frame_0.light_space_0.data_0[i32(3)][i32(0)], frame_0.light_space_0.data_0[i32(0)][i32(1)], frame_0.light_space_0.data_0[i32(1)][i32(1)], frame_0.light_space_0.data_0[i32(2)][i32(1)], frame_0.light_space_0.data_0[i32(3)][i32(1)], frame_0.light_space_0.data_0[i32(0)][i32(2)], frame_0.light_space_0.data_0[i32(1)][i32(2)], frame_0.light_space_0.data_0[i32(2)][i32(2)], frame_0.light_space_0.data_0[i32(3)][i32(2)], frame_0.light_space_0.data_0[i32(0)][i32(3)], frame_0.light_space_0.data_0[i32(1)][i32(3)], frame_0.light_space_0.data_0[i32(2)][i32(3)], frame_0.light_space_0.data_0[i32(3)][i32(3)]))));
        }
    }
    o_1.uv_4 = base_color_uv_select_0(_S28.uv_5, _S28.uv1_5);
    o_1.alpha_0 = _S28.color_3.w;
    return o_1;
}

fn apply_uv_transform_0( row0_0 : vec4<f32>,  row1_0 : vec4<f32>,  uv_6 : vec2<f32>) -> vec2<f32>
{
    var _S30 : f32 = uv_6.x;
    var _S31 : f32 = uv_6.y;
    return vec2<f32>(row0_0.x * _S30 + row0_0.y * _S31 + row0_0.z, row1_0.x * _S30 + row1_0.y * _S31 + row1_0.z);
}

fn base_color_uv_rows_0( uv_7 : vec2<f32>) -> vec2<f32>
{
    return apply_uv_transform_0(prim_0.base_uv_row0_0, prim_0.base_uv_row1_0, uv_7);
}

struct pixelInput_0
{
    @location(0) uv_8 : vec2<f32>,
    @location(1) alpha_1 : f32,
};

@fragment
fn fs_shadow_masked( _S32 : pixelInput_0, @builtin(position) pos_1 : vec4<f32>)
{
    if((prim_0.base_color_0.w * (textureSample((baseColorTex_0), (baseColorSampler_0), (base_color_uv_rows_0(_S32.uv_8)), (vec2<i32>(i32(0))))).w * _S32.alpha_1) < (prim_0.emissive_factor_0.w))
    {
        discard;
    }
    return;
}

fn base_color_uv_0( uv0_1 : vec2<f32>,  uv1_6 : vec2<f32>) -> vec2<f32>
{
    return base_color_uv_rows_0(base_color_uv_select_0(uv0_1, uv1_6));
}

fn metallic_roughness_uv_0( uv0_2 : vec2<f32>,  uv1_7 : vec2<f32>) -> vec2<f32>
{
    var uv_9 : vec2<f32>;
    if(((u32(prim_0.material_flags_0.y) & (u32(2)))) != u32(0))
    {
        uv_9 = uv1_7;
    }
    else
    {
        uv_9 = uv0_2;
    }
    return apply_uv_transform_0(prim_0.mr_uv_row0_0, prim_0.mr_uv_row1_0, uv_9);
}

fn normal_uv_0( uv0_3 : vec2<f32>,  uv1_8 : vec2<f32>) -> vec2<f32>
{
    var uv_10 : vec2<f32>;
    if(((u32(prim_0.material_flags_0.y) & (u32(4)))) != u32(0))
    {
        uv_10 = uv1_8;
    }
    else
    {
        uv_10 = uv0_3;
    }
    return apply_uv_transform_0(prim_0.normal_uv_row0_0, prim_0.normal_uv_row1_0, uv_10);
}

fn emissive_uv_0( uv0_4 : vec2<f32>,  uv1_9 : vec2<f32>) -> vec2<f32>
{
    var uv_11 : vec2<f32>;
    if(((u32(prim_0.material_flags_0.y) & (u32(8)))) != u32(0))
    {
        uv_11 = uv1_9;
    }
    else
    {
        uv_11 = uv0_4;
    }
    return apply_uv_transform_0(prim_0.emissive_uv_row0_0, prim_0.emissive_uv_row1_0, uv_11);
}

fn occlusion_uv_0( uv0_5 : vec2<f32>,  uv1_10 : vec2<f32>) -> vec2<f32>
{
    var uv_12 : vec2<f32>;
    if(((u32(prim_0.material_flags_0.y) & (u32(16)))) != u32(0))
    {
        uv_12 = uv1_10;
    }
    else
    {
        uv_12 = uv0_5;
    }
    return apply_uv_transform_0(prim_0.occlusion_uv_row0_0, prim_0.occlusion_uv_row1_0, uv_12);
}

struct SurfacePoint_0
{
     position_4 : vec3<f32>,
     n_0 : vec3<f32>,
     albedo_0 : vec3<f32>,
     metallic_0 : f32,
     roughness_0 : f32,
     occlusion_0 : f32,
     emissive_0 : vec3<f32>,
};

struct MaterialSample_0
{
     surface_0 : SurfacePoint_0,
     alpha_2 : f32,
};

fn sample_material_0( In_2 : VsOut_0,  isFrontFace_0 : bool) -> MaterialSample_0
{
    var mrSample_0 : vec4<f32> = (textureSample((metalRoughTex_0), (metalRoughSampler_0), (metallic_roughness_uv_0(In_2.uv_1, In_2.uv1_1))));
    var normalSample_0 : vec4<f32> = (textureSample((normalTex_0), (normalSampler_0), (normal_uv_0(In_2.uv_1, In_2.uv1_1))));
    var emissiveSample_0 : vec4<f32> = (textureSample((emissiveTex_0), (emissiveSampler_0), (emissive_uv_0(In_2.uv_1, In_2.uv1_1))));
    var occlusionSample_0 : vec4<f32> = (textureSample((occlusionTex_0), (occlusionSampler_0), (occlusion_uv_0(In_2.uv_1, In_2.uv1_1))));
    var albedo_1 : vec4<f32> = prim_0.base_color_0 * (textureSample((baseColorTex_0), (baseColorSampler_0), (base_color_uv_0(In_2.uv_1, In_2.uv1_1)))) * In_2.vertexColor_0;
    var m_2 : MaterialSample_0;
    m_2.alpha_2 = albedo_1.w;
    m_2.surface_0.position_4 = In_2.worldPosition_0;
    m_2.surface_0.albedo_0 = albedo_1.xyz;
    m_2.surface_0.metallic_0 = clamp(prim_0.material_factors_0.x * mrSample_0.z, 0.0f, 1.0f);
    m_2.surface_0.roughness_0 = clamp(prim_0.material_factors_0.y * mrSample_0.y, 0.04500000178813934f, 1.0f);
    m_2.surface_0.occlusion_0 = mix(1.0f, occlusionSample_0.x, prim_0.material_factors_0.z);
    m_2.surface_0.emissive_0 = prim_0.emissive_factor_0.xyz * emissiveSample_0.xyz;
    var nGeom_0 : vec3<f32> = normalize_or_0(In_2.worldNormal_0, normalize(frame_0.camera_position_0.xyz - In_2.worldPosition_0));
    var _S33 : vec3<f32> = In_2.worldTangent_0.xyz;
    var t_0 : vec3<f32> = normalize_or_0(_S33 - nGeom_0 * vec3<f32>(dot(nGeom_0, _S33)), vec3<f32>(0.0f));
    var b_0 : vec3<f32> = cross(nGeom_0, t_0) * vec3<f32>(In_2.worldTangent_0.w);
    var nTs_0 : vec3<f32> = normalSample_0.xyz * vec3<f32>(2.0f) - vec3<f32>(1.0f);
    if((normalSample_0.z) == 0.0f)
    {
        nTs_0[i32(2)] = sqrt(saturate(1.0f - dot(nTs_0.xy, nTs_0.xy)));
    }
    var _S34 : vec3<f32> = vec3<f32>(nTs_0.xy * vec2<f32>(prim_0.material_factors_0.w), nTs_0.z);
    nTs_0 = _S34;
    var n_1 : vec3<f32> = normalize_or_0((((mat3x3<f32>(t_0, b_0, nGeom_0)) * (_S34))), nGeom_0);
    var n_2 : vec3<f32>;
    if(!isFrontFace_0)
    {
        n_2 = (vec3<f32>(0) - n_1);
    }
    else
    {
        n_2 = n_1;
    }
    m_2.surface_0.n_0 = n_2;
    return m_2;
}

fn distribution_ggx_0( n_dot_h_0 : f32,  roughness_1 : f32) -> f32
{
    var _S35 : f32 = max(roughness_1, 0.04500000178813934f);
    var a_0 : f32 = _S35 * _S35;
    var a2_0 : f32 = a_0 * a_0;
    var denom_0 : f32 = n_dot_h_0 * n_dot_h_0 * (a2_0 - 1.0f) + 1.0f;
    return a2_0 / (3.14159274101257324f * max(denom_0 * denom_0, 9.99999968265522539e-21f));
}

fn geometry_smith_0( n_dot_v_0 : f32,  n_dot_l_0 : f32,  roughness_2 : f32) -> f32
{
    var r_0 : f32 = roughness_2 + 1.0f;
    var k_0 : f32 = r_0 * r_0 / 8.0f;
    var _S36 : f32 = 1.0f - k_0;
    return n_dot_v_0 / max(n_dot_v_0 * _S36 + k_0, 9.99999997475242708e-07f) * (n_dot_l_0 / max(n_dot_l_0 * _S36 + k_0, 9.99999997475242708e-07f));
}

fn fresnel_schlick_0( cos_theta_0 : f32,  f0_0 : vec3<f32>) -> vec3<f32>
{
    return f0_0 + (vec3<f32>(1.0f) - f0_0) * vec3<f32>(pow(clamp(1.0f - cos_theta_0, 0.0f, 1.0f), 5.0f));
}

fn lambert_diffuse_0( albedo_2 : vec3<f32>) -> vec3<f32>
{
    return albedo_2 / vec3<f32>(3.14159274101257324f);
}

fn brdf_direct_0( n_3 : vec3<f32>,  v_1 : vec3<f32>,  l_0 : vec3<f32>,  albedo_3 : vec3<f32>,  metallic_1 : f32,  roughness_3 : f32,  f0_1 : vec3<f32>,  radiance_0 : vec3<f32>) -> vec3<f32>
{
    var h_0 : vec3<f32> = normalize(l_0 + v_1);
    var _S37 : f32 = max(dot(n_3, l_0), 0.0f);
    var _S38 : f32 = max(dot(n_3, v_1), 0.00009999999747379f);
    var f_0 : vec3<f32> = fresnel_schlick_0(max(dot(h_0, v_1), 0.0f), f0_1);
    return ((vec3<f32>(1.0f) - f_0) * vec3<f32>((1.0f - metallic_1)) * lambert_diffuse_0(albedo_3) + vec3<f32>((distribution_ggx_0(max(dot(n_3, h_0), 0.0f), roughness_3) * geometry_smith_0(_S38, _S37, roughness_3))) * f_0 / vec3<f32>(max(4.0f * _S38 * _S37, 9.99999997475242708e-07f))) * radiance_0 * vec3<f32>(_S37);
}

fn select_cascade_0( viewDepth_0 : f32,  count_0 : i32) -> i32
{
    var selected_0 : i32;
    if(viewDepth_0 >= (frame_0.cascade_splits_0.x))
    {
        selected_0 = i32(1);
    }
    else
    {
        selected_0 = i32(0);
    }
    if(viewDepth_0 >= (frame_0.cascade_splits_0.y))
    {
        selected_0 = i32(2);
    }
    return min(selected_0, count_0 - i32(1));
}

fn light_space_for_0( cascade_2 : i32) -> mat4x4<f32>
{
    if(cascade_2 == i32(1))
    {
        return mat4x4<f32>(frame_0.light_space_1_0.data_0[i32(0)][i32(0)], frame_0.light_space_1_0.data_0[i32(1)][i32(0)], frame_0.light_space_1_0.data_0[i32(2)][i32(0)], frame_0.light_space_1_0.data_0[i32(3)][i32(0)], frame_0.light_space_1_0.data_0[i32(0)][i32(1)], frame_0.light_space_1_0.data_0[i32(1)][i32(1)], frame_0.light_space_1_0.data_0[i32(2)][i32(1)], frame_0.light_space_1_0.data_0[i32(3)][i32(1)], frame_0.light_space_1_0.data_0[i32(0)][i32(2)], frame_0.light_space_1_0.data_0[i32(1)][i32(2)], frame_0.light_space_1_0.data_0[i32(2)][i32(2)], frame_0.light_space_1_0.data_0[i32(3)][i32(2)], frame_0.light_space_1_0.data_0[i32(0)][i32(3)], frame_0.light_space_1_0.data_0[i32(1)][i32(3)], frame_0.light_space_1_0.data_0[i32(2)][i32(3)], frame_0.light_space_1_0.data_0[i32(3)][i32(3)]);
    }
    if(cascade_2 == i32(2))
    {
        return mat4x4<f32>(frame_0.light_space_2_0.data_0[i32(0)][i32(0)], frame_0.light_space_2_0.data_0[i32(1)][i32(0)], frame_0.light_space_2_0.data_0[i32(2)][i32(0)], frame_0.light_space_2_0.data_0[i32(3)][i32(0)], frame_0.light_space_2_0.data_0[i32(0)][i32(1)], frame_0.light_space_2_0.data_0[i32(1)][i32(1)], frame_0.light_space_2_0.data_0[i32(2)][i32(1)], frame_0.light_space_2_0.data_0[i32(3)][i32(1)], frame_0.light_space_2_0.data_0[i32(0)][i32(2)], frame_0.light_space_2_0.data_0[i32(1)][i32(2)], frame_0.light_space_2_0.data_0[i32(2)][i32(2)], frame_0.light_space_2_0.data_0[i32(3)][i32(2)], frame_0.light_space_2_0.data_0[i32(0)][i32(3)], frame_0.light_space_2_0.data_0[i32(1)][i32(3)], frame_0.light_space_2_0.data_0[i32(2)][i32(3)], frame_0.light_space_2_0.data_0[i32(3)][i32(3)]);
    }
    return mat4x4<f32>(frame_0.light_space_0.data_0[i32(0)][i32(0)], frame_0.light_space_0.data_0[i32(1)][i32(0)], frame_0.light_space_0.data_0[i32(2)][i32(0)], frame_0.light_space_0.data_0[i32(3)][i32(0)], frame_0.light_space_0.data_0[i32(0)][i32(1)], frame_0.light_space_0.data_0[i32(1)][i32(1)], frame_0.light_space_0.data_0[i32(2)][i32(1)], frame_0.light_space_0.data_0[i32(3)][i32(1)], frame_0.light_space_0.data_0[i32(0)][i32(2)], frame_0.light_space_0.data_0[i32(1)][i32(2)], frame_0.light_space_0.data_0[i32(2)][i32(2)], frame_0.light_space_0.data_0[i32(3)][i32(2)], frame_0.light_space_0.data_0[i32(0)][i32(3)], frame_0.light_space_0.data_0[i32(1)][i32(3)], frame_0.light_space_0.data_0[i32(2)][i32(3)], frame_0.light_space_0.data_0[i32(3)][i32(3)]);
}

fn bias_scale_for_0( cascade_3 : i32) -> f32
{
    if(cascade_3 == i32(1))
    {
        return frame_0.shadow_bias_scale_0.y;
    }
    if(cascade_3 == i32(2))
    {
        return frame_0.shadow_bias_scale_0.z;
    }
    return frame_0.shadow_bias_scale_0.x;
}

fn shadow_visibility_0( worldPos_1 : vec3<f32>,  nDotL_0 : f32) -> f32
{
    var inBounds_0 : bool;
    var _S39 : bool;
    var _S40 : bool;
    var count_1 : i32 = clamp(i32(frame_0.shadow_params_0.x), i32(0), i32(3));
    if(count_1 == i32(0))
    {
        return 1.0f;
    }
    const _S41 : vec3<f32> = vec3<f32>(0.0f, 0.0f, 0.0f);
    var _S42 : i32 = select_cascade_0(dot(worldPos_1 - frame_0.camera_position_0.xyz, frame_0.shadow_view_dir_0.xyz), count_1);
    var uv_13 : vec2<f32> = vec2<f32>(0.0f, 0.0f);
    var proj_0 : vec3<f32> = _S41;
    var cascade_4 : i32 = _S42;
    for(;;)
    {
        if(cascade_4 < count_1)
        {
        }
        else
        {
            inBounds_0 = false;
            break;
        }
        var lightSpacePos_1 : vec4<f32> = (((vec4<f32>(worldPos_1, 1.0f)) * (light_space_for_0(cascade_4))));
        var proj_1 : vec3<f32> = lightSpacePos_1.xyz / vec3<f32>(lightSpacePos_1.w);
        var _S43 : f32 = proj_1.x * 0.5f + 0.5f;
        var _S44 : f32 = 0.5f - proj_1.y * 0.5f;
        var uv_14 : vec2<f32> = vec2<f32>(_S43, _S44);
        if(_S43 >= 0.0f)
        {
            inBounds_0 = _S43 <= 1.0f;
        }
        else
        {
            inBounds_0 = false;
        }
        if(inBounds_0)
        {
            _S39 = _S44 >= 0.0f;
        }
        else
        {
            _S39 = false;
        }
        if(_S39)
        {
            _S40 = _S44 <= 1.0f;
        }
        else
        {
            _S40 = false;
        }
        var _S45 : bool;
        if(_S40)
        {
            _S45 = (proj_1.z) >= 0.0f;
        }
        else
        {
            _S45 = false;
        }
        var _S46 : bool;
        if(_S45)
        {
            _S46 = (proj_1.z) <= 1.0f;
        }
        else
        {
            _S46 = false;
        }
        if(_S46)
        {
            inBounds_0 = true;
            uv_13 = uv_14;
            proj_0 = proj_1;
            break;
        }
        var _S47 : i32 = cascade_4 + i32(1);
        uv_13 = uv_14;
        proj_0 = proj_1;
        cascade_4 = _S47;
    }
    if(!inBounds_0)
    {
        return 1.0f;
    }
    var _S48 : f32 = max(0.0020000000949949f * (1.0f - nDotL_0), 0.00050000002374873f) * bias_scale_for_0(cascade_4);
    var radius_0 : i32 = clamp(i32(frame_0.shadow_params_0.y), i32(0), i32(5));
    var shadowWidth_0 : u32;
    var shadowHeight_0 : u32;
    var shadowLayers_0 : u32;
    {var dim = textureDimensions((shadowMap_0));((shadowWidth_0)) = dim.x;((shadowHeight_0)) = dim.y;((shadowLayers_0)) = textureNumLayers((shadowMap_0));};
    var _S49 : f32 = 1.0f / f32(shadowWidth_0);
    var _S50 : i32 = - radius_0;
    var y_0 : i32 = _S50;
    var sum_0 : f32 = 0.0f;
    for(;;)
    {
        if(y_0 <= radius_0)
        {
        }
        else
        {
            break;
        }
        var x_0 : i32 = _S50;
        for(;;)
        {
            if(x_0 <= radius_0)
            {
            }
            else
            {
                break;
            }
            var tapUV_0 : vec2<f32> = uv_13 + vec2<f32>(f32(x_0), f32(y_0)) * vec2<f32>(_S49);
            var _S51 : f32 = tapUV_0.x;
            if(_S51 >= 0.0f)
            {
                inBounds_0 = _S51 <= 1.0f;
            }
            else
            {
                inBounds_0 = false;
            }
            if(inBounds_0)
            {
                _S39 = (tapUV_0.y) >= 0.0f;
            }
            else
            {
                _S39 = false;
            }
            if(_S39)
            {
                _S40 = (tapUV_0.y) <= 1.0f;
            }
            else
            {
                _S40 = false;
            }
            var _S52 : f32;
            if(_S40)
            {
                var _S53 : vec3<f32> = vec3<f32>(tapUV_0, f32(cascade_4));
                _S52 = (textureSampleCompareLevel((shadowMap_0), (shadowSampler_0), ((_S53)).xy, i32(((_S53)).z), (proj_0.z - _S48)));
            }
            else
            {
                _S52 = 1.0f;
            }
            var sum_1 : f32 = sum_0 + _S52;
            x_0 = x_0 + i32(1);
            sum_0 = sum_1;
        }
        y_0 = y_0 + i32(1);
    }
    var _S54 : i32 = i32(2) * radius_0 + i32(1);
    return sum_0 / f32(_S54 * _S54);
}

fn punctual_lighting_0( worldPos_2 : vec3<f32>,  n_4 : vec3<f32>,  v_2 : vec3<f32>,  albedo_4 : vec3<f32>,  metallic_2 : f32,  roughness_4 : f32,  f0_2 : vec3<f32>,  fragCoord_0 : vec4<f32>) -> vec3<f32>
{
    var _S55 : vec3<f32> = vec3<f32>(0.0f);
    var tileX_0 : u32 = u32(fragCoord_0.x) / u32(16);
    var tileY_0 : u32 = u32(fragCoord_0.y) / u32(16);
    var tileW_0 : u32 = u32(frame_0.cascade_splits_0.z);
    var tileH_0 : u32 = u32(frame_0.cascade_splits_0.w);
    var _S56 : u32 = u32(frame_0.camera_position_0.w);
    var tileCount_0 : u32;
    var tileOffset_0 : u32;
    var listed_0 : bool;
    if(tileW_0 != u32(0))
    {
        var gridEntry_0 : vec2<u32> = tileLightGrid_0[min(tileY_0, max(tileH_0, u32(1)) - u32(1)) * tileW_0 + min(tileX_0, max(tileW_0, u32(1)) - u32(1))];
        var _S57 : u32 = gridEntry_0.x;
        var listed_1 : bool = _S57 <= u32(32);
        if(listed_1)
        {
            var _S58 : u32 = gridEntry_0.y;
            tileCount_0 = _S57;
            tileOffset_0 = _S58;
        }
        else
        {
            tileCount_0 = _S56;
            tileOffset_0 = u32(0);
        }
        listed_0 = listed_1;
    }
    else
    {
        tileCount_0 = _S56;
        listed_0 = false;
        tileOffset_0 = u32(0);
    }
    var j_0 : u32 = u32(0);
    var total_1 : vec3<f32> = _S55;
    for(;;)
    {
        if(j_0 < tileCount_0)
        {
        }
        else
        {
            break;
        }
        var lightIdx_0 : u32;
        if(listed_0)
        {
            lightIdx_0 = tileLightIndices_0[tileOffset_0 + j_0];
        }
        else
        {
            lightIdx_0 = j_0;
        }
        var _S59 : u32 = lightIdx_0 * u32(4);
        var a_1 : vec4<f32> = punctualLights_0[_S59];
        var b_1 : vec4<f32> = punctualLights_0[_S59 + u32(1)];
        var cvec_0 : vec4<f32> = punctualLights_0[_S59 + u32(2)];
        var dvec_0 : vec4<f32> = punctualLights_0[_S59 + u32(3)];
        var kind_0 : f32 = a_1.w;
        var l_1 : vec3<f32>;
        var attenuation_0 : f32;
        if(kind_0 > 2.5f)
        {
            l_1 = normalize((vec3<f32>(0) - cvec_0.xyz));
            attenuation_0 = 1.0f;
        }
        else
        {
            var toLight_0 : vec3<f32> = a_1.xyz - worldPos_2;
            var _S60 : f32 = max(length(toLight_0), 0.00009999999747379f);
            var l_2 : vec3<f32> = toLight_0 / vec3<f32>(_S60);
            var attenuation_1 : f32 = 1.0f / (_S60 * _S60);
            var range_0 : f32 = b_1.w;
            if(range_0 > 0.0f)
            {
                var k_1 : f32 = clamp(1.0f - pow(_S60 / range_0, 4.0f), 0.0f, 1.0f);
                attenuation_0 = attenuation_1 * (k_1 * k_1);
            }
            else
            {
                attenuation_0 = attenuation_1;
            }
            var attenuation_2 : f32;
            if(kind_0 > 1.5f)
            {
                attenuation_2 = attenuation_0 * smoothstep(dvec_0.x, cvec_0.w, dot(normalize(cvec_0.xyz), (vec3<f32>(0) - l_2)));
            }
            else
            {
                attenuation_2 = attenuation_0;
            }
            l_1 = l_2;
            attenuation_0 = attenuation_2;
        }
        if(attenuation_0 <= 0.0f)
        {
            j_0 = j_0 + u32(1);
            continue;
        }
        total_1 = total_1 + brdf_direct_0(n_4, v_2, l_1, albedo_4, metallic_2, roughness_4, f0_2, b_1.xyz * vec3<f32>(attenuation_0));
        j_0 = j_0 + u32(1);
    }
    return total_1;
}

fn hemisphere_irradiance_0( n_5 : vec3<f32>) -> vec3<f32>
{
    var _S61 : f32 = n_5.y;
    return mix(vec3<f32>(0.18000000715255737f, 0.15999999642372131f, 0.15000000596046448f) * vec3<f32>(0.69999998807907104f), mix(vec3<f32>(0.55000001192092896f, 0.62000000476837158f, 0.72000002861022949f), vec3<f32>(0.09000000357627869f, 0.15999999642372131f, 0.34999999403953552f), vec3<f32>(pow(clamp(_S61, 0.0f, 1.0f), 0.69999998807907104f))), vec3<f32>(clamp(_S61 * 0.5f + 0.5f, 0.0f, 1.0f)));
}

fn sky_gradient_0( dir_0 : vec3<f32>) -> vec3<f32>
{
    var _S62 : f32 = dir_0.y;
    if(_S62 >= 0.0f)
    {
        return mix(vec3<f32>(0.55000001192092896f, 0.62000000476837158f, 0.72000002861022949f), vec3<f32>(0.09000000357627869f, 0.15999999642372131f, 0.34999999403953552f), vec3<f32>(pow(clamp(_S62, 0.0f, 1.0f), 0.69999998807907104f)));
    }
    else
    {
        return mix(vec3<f32>(0.55000001192092896f, 0.62000000476837158f, 0.72000002861022949f), vec3<f32>(0.18000000715255737f, 0.15999999642372131f, 0.15000000596046448f), vec3<f32>(clamp(- _S62 * 3.0f, 0.0f, 1.0f)));
    }
}

fn sun_disk_0( dir_1 : vec3<f32>,  lightDir_0 : vec3<f32>,  intensity_0 : f32) -> vec3<f32>
{
    var _S63 : f32 = max(dot(dir_1, normalize(lightDir_0)), 0.0f);
    return vec3<f32>(1.0f, 0.94999998807907104f, 0.85000002384185791f) * vec3<f32>((pow(_S63, 1200.0f) * 24.0f + pow(_S63, 48.0f) * 0.5f)) * vec3<f32>((intensity_0 * 0.40000000596046448f));
}

fn sky_radiance_0( dir_2 : vec3<f32>,  withSun_0 : bool) -> vec3<f32>
{
    var color_4 : vec3<f32> = sky_gradient_0(dir_2);
    var color_5 : vec3<f32>;
    if(withSun_0)
    {
        color_5 = color_4 + sun_disk_0(dir_2, frame_0.light_dir_ambient_0.xyz, frame_0.light_color_intensity_0.w);
    }
    else
    {
        color_5 = color_4;
    }
    return color_5;
}

fn env_brdf_approx_0( f0_3 : vec3<f32>,  roughness_5 : f32,  nDotV_0 : f32) -> vec3<f32>
{
    var r_1 : vec4<f32> = vec4<f32>(roughness_5) * vec4<f32>(-1.0f, -0.02749999985098839f, -0.57200002670288086f, 0.02199999988079071f) + vec4<f32>(1.0f, 0.04250000044703484f, 1.03999996185302734f, -0.03999999910593033f);
    var _S64 : f32 = r_1.x;
    var ab_0 : vec2<f32> = vec2<f32>(-1.03999996185302734f, 1.03999996185302734f) * vec2<f32>((min(_S64 * _S64, exp2(-9.27999973297119141f * nDotV_0)) * _S64 + r_1.y)) + r_1.zw;
    return f0_3 * vec3<f32>(ab_0.x) + vec3<f32>(ab_0.y);
}

fn ibl_ambient_0( s_0 : SurfacePoint_0,  v_3 : vec3<f32>,  f0_4 : vec3<f32>) -> vec3<f32>
{
    var reflected_0 : vec3<f32> = reflect((vec3<f32>(0) - v_3), s_0.n_0);
    var _S65 : f32 = dot(s_0.n_0, v_3);
    var _S66 : f32 = max(_S65, 0.00009999999747379f);
    var kSIbl_0 : vec3<f32> = fresnel_schlick_0(_S66, f0_4);
    var iblStrength_0 : f32 = frame_0.light_dir_ambient_0.w;
    var envEnabled_0 : bool = (iblParams_0.enabled_maxmip_intensity_0.x) > 0.5f;
    var irradianceAnalytic_0 : vec3<f32> = hemisphere_irradiance_0(s_0.n_0);
    var _S67 : vec3<f32> = vec3<f32>(iblParams_0.enabled_maxmip_intensity_0.z);
    var irradianceEnv_0 : vec3<f32> = (textureSampleLevel((irradianceMap_0), (iblSampler_0), (s_0.n_0), (0.0f))).xyz * _S67;
    var irradiance_0 : vec3<f32>;
    if(envEnabled_0)
    {
        irradiance_0 = irradianceEnv_0;
    }
    else
    {
        irradiance_0 = irradianceAnalytic_0;
    }
    var diffuseIbl_0 : vec3<f32> = (vec3<f32>(1.0f) - kSIbl_0) * vec3<f32>((1.0f - s_0.metallic_0)) * s_0.albedo_0 * irradiance_0;
    var brdfLutSample_0 : vec2<f32> = (textureSampleLevel((brdfLut_0), (iblSampler_0), (vec2<f32>(max(_S65, 0.0f), s_0.roughness_0)), (0.0f))).xy;
    var specularSplitSum_0 : vec3<f32> = (textureSampleLevel((prefilteredMap_0), (iblSampler_0), (reflected_0), (s_0.roughness_0 * iblParams_0.enabled_maxmip_intensity_0.y))).xyz * _S67 * (f0_4 * vec3<f32>(brdfLutSample_0.x) + vec3<f32>(brdfLutSample_0.y));
    var specularAnalytic_0 : vec3<f32> = mix(sky_radiance_0(reflected_0, true), irradianceAnalytic_0, vec3<f32>((s_0.roughness_0 * s_0.roughness_0))) * env_brdf_approx_0(f0_4, s_0.roughness_0, _S66);
    var specularIbl_0 : vec3<f32>;
    if(envEnabled_0)
    {
        specularIbl_0 = specularSplitSum_0;
    }
    else
    {
        specularIbl_0 = specularAnalytic_0;
    }
    return vec3<f32>((iblStrength_0 * s_0.occlusion_0)) * (diffuseIbl_0 + specularIbl_0);
}

fn shade_surface_0( s_1 : SurfacePoint_0,  fragCoord_1 : vec4<f32>) -> vec3<f32>
{
    var v_4 : vec3<f32> = normalize(frame_0.camera_position_0.xyz - s_1.position_4);
    var l_3 : vec3<f32> = normalize(frame_0.light_dir_ambient_0.xyz);
    var f0_5 : vec3<f32> = mix(vec3<f32>(0.03999999910593033f), s_1.albedo_0, vec3<f32>(s_1.metallic_0));
    return clamp(brdf_direct_0(s_1.n_0, v_4, l_3, s_1.albedo_0, s_1.metallic_0, s_1.roughness_0, f0_5, frame_0.light_color_intensity_0.xyz * vec3<f32>(frame_0.light_color_intensity_0.w)) * vec3<f32>(mix(1.0f, shadow_visibility_0(s_1.position_4, max(dot(s_1.n_0, l_3), 0.0f)), frame_0.shadow_params_0.z)) + punctual_lighting_0(s_1.position_4, s_1.n_0, v_4, s_1.albedo_0, s_1.metallic_0, s_1.roughness_0, f0_5, fragCoord_1) + ibl_ambient_0(s_1, v_4, f0_5) + s_1.emissive_0, vec3<f32>(0.0f), vec3<f32>(65000.0f));
}

struct pixelOutput_0
{
    @location(0) output_1 : vec4<f32>,
};

struct pixelInput_1
{
    @location(0) worldNormal_1 : vec3<f32>,
    @location(1) uv_15 : vec2<f32>,
    @location(2) lightSpacePos_2 : vec4<f32>,
    @location(3) worldTangent_1 : vec4<f32>,
    @location(4) worldPosition_1 : vec3<f32>,
    @location(6) vertexColor_1 : vec4<f32>,
    @location(7) uv1_11 : vec2<f32>,
};

@fragment
fn fs_main( _S68 : pixelInput_1, @builtin(front_facing) isFrontFace_1 : bool, @builtin(position) svPosition_1 : vec4<f32>) -> pixelOutput_0
{
    var _S69 : VsOut_0 = VsOut_0( svPosition_1, _S68.worldNormal_1, _S68.uv_15, _S68.lightSpacePos_2, _S68.worldTangent_1, _S68.worldPosition_1, _S68.vertexColor_1, _S68.uv1_11 );
    var m_3 : MaterialSample_0 = sample_material_0(_S69, isFrontFace_1);
    if((m_3.alpha_2) < (prim_0.emissive_factor_0.w))
    {
        discard;
    }
    if((prim_0.material_flags_0.x) > 0.5f)
    {
        var _S70 : pixelOutput_0 = pixelOutput_0( vec4<f32>(m_3.surface_0.albedo_0, m_3.alpha_2) );
        return _S70;
    }
    var _S71 : pixelOutput_0 = pixelOutput_0( vec4<f32>(shade_surface_0(m_3.surface_0, svPosition_1), m_3.alpha_2) );
    return _S71;
}

struct GBufferOut_0
{
    @location(0) normalMetallic_0 : vec4<f32>,
    @location(1) albedoOcclusion_0 : vec4<f32>,
    @location(2) roughnessEmissive_0 : vec4<f32>,
};

struct pixelInput_2
{
    @location(0) worldNormal_2 : vec3<f32>,
    @location(1) uv_16 : vec2<f32>,
    @location(2) lightSpacePos_3 : vec4<f32>,
    @location(3) worldTangent_2 : vec4<f32>,
    @location(4) worldPosition_2 : vec3<f32>,
    @location(6) vertexColor_2 : vec4<f32>,
    @location(7) uv1_12 : vec2<f32>,
};

@fragment
fn fs_gbuffer( _S72 : pixelInput_2, @builtin(front_facing) isFrontFace_2 : bool, @builtin(position) svPosition_2 : vec4<f32>) -> GBufferOut_0
{
    var _S73 : VsOut_0 = VsOut_0( svPosition_2, _S72.worldNormal_2, _S72.uv_16, _S72.lightSpacePos_3, _S72.worldTangent_2, _S72.worldPosition_2, _S72.vertexColor_2, _S72.uv1_12 );
    var m_4 : MaterialSample_0 = sample_material_0(_S73, isFrontFace_2);
    if((m_4.alpha_2) < (prim_0.emissive_factor_0.w))
    {
        discard;
    }
    var g_0 : GBufferOut_0;
    var _S74 : f32;
    if((prim_0.material_flags_0.x) > 0.5f)
    {
        _S74 = -1.0f;
    }
    else
    {
        _S74 = m_4.surface_0.metallic_0;
    }
    g_0.normalMetallic_0 = vec4<f32>(m_4.surface_0.n_0, _S74);
    g_0.albedoOcclusion_0 = vec4<f32>(m_4.surface_0.albedo_0, m_4.surface_0.occlusion_0);
    g_0.roughnessEmissive_0 = vec4<f32>(m_4.surface_0.roughness_0, m_4.surface_0.emissive_0);
    return g_0;
}

struct FullscreenVsOut_0
{
    @builtin(position) svPosition_3 : vec4<f32>,
    @location(0) uv_17 : vec2<f32>,
};

fn fullscreen_vs_0( vid_0 : u32) -> FullscreenVsOut_0
{
    var x_1 : f32 = f32(vid_0 / u32(2)) * 4.0f - 1.0f;
    var y_1 : f32 = f32(vid_0 % u32(2)) * 4.0f - 1.0f;
    var o_2 : FullscreenVsOut_0;
    o_2.svPosition_3 = vec4<f32>(x_1, y_1, 0.0f, 1.0f);
    o_2.uv_17 = vec2<f32>((x_1 + 1.0f) * 0.5f, 1.0f - (y_1 + 1.0f) * 0.5f);
    return o_2;
}

@vertex
fn vs_deferred_lighting(@builtin(vertex_index) vid_1 : u32) -> FullscreenVsOut_0
{
    return fullscreen_vs_0(vid_1);
}

fn fullscreen_uv_to_ndc_0( uv_18 : vec2<f32>) -> vec2<f32>
{
    return vec2<f32>(uv_18.x * 2.0f - 1.0f, 1.0f - uv_18.y * 2.0f);
}

struct pixelOutput_1
{
    @location(0) output_2 : vec4<f32>,
};

struct pixelInput_3
{
    @location(0) uv_19 : vec2<f32>,
};

@fragment
fn fs_deferred_lighting( _S75 : pixelInput_3, @builtin(position) svPosition_4 : vec4<f32>) -> pixelOutput_1
{
    var texel_0 : vec3<i32> = vec3<i32>(vec2<i32>(svPosition_4.xy), i32(0));
    var depth_0 : f32 = (textureLoad((gbufferDepth_0), ((texel_0)).xy, ((texel_0)).z));
    if(depth_0 >= 1.0f)
    {
        discard;
    }
    var normalMetallic_1 : vec4<f32> = (textureLoad((gbufferNormal_0), ((texel_0)).xy, ((texel_0)).z));
    var albedoOcclusion_1 : vec4<f32> = (textureLoad((gbufferAlbedo_0), ((texel_0)).xy, ((texel_0)).z));
    var _S76 : f32 = normalMetallic_1.w;
    if(_S76 < -0.5f)
    {
        var _S77 : pixelOutput_1 = pixelOutput_1( vec4<f32>(albedoOcclusion_1.xyz, 1.0f) );
        return _S77;
    }
    var roughnessEmissive_1 : vec4<f32> = (textureLoad((gbufferMaterial_0), ((texel_0)).xy, ((texel_0)).z));
    var world_2 : vec4<f32> = (((vec4<f32>(fullscreen_uv_to_ndc_0(_S75.uv_19), depth_0, 1.0f)) * (mat4x4<f32>(deferredParams_0.inv_view_proj_0.data_0[i32(0)][i32(0)], deferredParams_0.inv_view_proj_0.data_0[i32(1)][i32(0)], deferredParams_0.inv_view_proj_0.data_0[i32(2)][i32(0)], deferredParams_0.inv_view_proj_0.data_0[i32(3)][i32(0)], deferredParams_0.inv_view_proj_0.data_0[i32(0)][i32(1)], deferredParams_0.inv_view_proj_0.data_0[i32(1)][i32(1)], deferredParams_0.inv_view_proj_0.data_0[i32(2)][i32(1)], deferredParams_0.inv_view_proj_0.data_0[i32(3)][i32(1)], deferredParams_0.inv_view_proj_0.data_0[i32(0)][i32(2)], deferredParams_0.inv_view_proj_0.data_0[i32(1)][i32(2)], deferredParams_0.inv_view_proj_0.data_0[i32(2)][i32(2)], deferredParams_0.inv_view_proj_0.data_0[i32(3)][i32(2)], deferredParams_0.inv_view_proj_0.data_0[i32(0)][i32(3)], deferredParams_0.inv_view_proj_0.data_0[i32(1)][i32(3)], deferredParams_0.inv_view_proj_0.data_0[i32(2)][i32(3)], deferredParams_0.inv_view_proj_0.data_0[i32(3)][i32(3)]))));
    var s_2 : SurfacePoint_0;
    s_2.position_4 = world_2.xyz / vec3<f32>(world_2.w);
    s_2.n_0 = normalize_or_0(normalMetallic_1.xyz, vec3<f32>(0.0f, 1.0f, 0.0f));
    s_2.albedo_0 = albedoOcclusion_1.xyz;
    s_2.metallic_0 = _S76;
    s_2.roughness_0 = roughnessEmissive_1.x;
    s_2.occlusion_0 = albedoOcclusion_1.w;
    s_2.emissive_0 = roughnessEmissive_1.yzw;
    var _S78 : pixelOutput_1 = pixelOutput_1( vec4<f32>(shade_surface_0(s_2, svPosition_4), 1.0f) );
    return _S78;
}

