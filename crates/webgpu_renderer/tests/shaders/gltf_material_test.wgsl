@binding(0) @group(0) var<storage, read> vectorInputs_0 : array<u32>;

@binding(1) @group(0) var<storage, read_write> vectorOutputs_0 : array<u32>;

struct VectorInputs_0
{
     at_0 : u32,
};

fn vector_inputs_0( case_index_0 : u32) -> VectorInputs_0
{
    var inputs_0 : VectorInputs_0;
    inputs_0.at_0 = u32(2) + case_index_0 * vectorInputs_0[i32(0)];
    return inputs_0;
}

fn VectorInputs_u32_0( this_0 : ptr<function, VectorInputs_0>) -> u32
{
    var word_0 : u32 = vectorInputs_0[(*this_0).at_0];
    (*this_0).at_0 = (*this_0).at_0 + u32(1);
    return word_0;
}

fn VectorInputs_u32_1( this_1 : ptr<function, VectorInputs_0>) -> u32
{
    var word_1 : u32 = vectorInputs_0[(*this_1).at_0];
    (*this_1).at_0 = (*this_1).at_0 + u32(1);
    return word_1;
}

fn VectorInputs_f32_0( this_2 : ptr<function, VectorInputs_0>) -> f32
{
    var _S1 : u32 = VectorInputs_u32_1(&((*this_2)));
    return (bitcast<f32>((_S1)));
}

fn VectorInputs_f32_1( this_3 : ptr<function, VectorInputs_0>) -> f32
{
    var _S2 : u32 = VectorInputs_u32_1(&((*this_3)));
    return (bitcast<f32>((_S2)));
}

fn VectorInputs_f32x2_0( this_4 : ptr<function, VectorInputs_0>) -> vec2<f32>
{
    var x_0 : f32 = VectorInputs_f32_1(&((*this_4)));
    var y_0 : f32 = VectorInputs_f32_1(&((*this_4)));
    return vec2<f32>(x_0, y_0);
}

fn VectorInputs_f32x3_0( this_5 : ptr<function, VectorInputs_0>) -> vec3<f32>
{
    var x_1 : f32 = VectorInputs_f32_1(&((*this_5)));
    var y_1 : f32 = VectorInputs_f32_1(&((*this_5)));
    var z_0 : f32 = VectorInputs_f32_1(&((*this_5)));
    return vec3<f32>(x_1, y_1, z_0);
}

fn VectorInputs_f32x3_1( this_6 : ptr<function, VectorInputs_0>) -> vec3<f32>
{
    var x_2 : f32 = VectorInputs_f32_1(&((*this_6)));
    var y_2 : f32 = VectorInputs_f32_1(&((*this_6)));
    var z_1 : f32 = VectorInputs_f32_1(&((*this_6)));
    return vec3<f32>(x_2, y_2, z_1);
}

fn VectorInputs_f32x4_0( this_7 : ptr<function, VectorInputs_0>) -> vec4<f32>
{
    var xyz_0 : vec3<f32> = VectorInputs_f32x3_1(&((*this_7)));
    var w_0 : f32 = VectorInputs_f32_1(&((*this_7)));
    return vec4<f32>(xyz_0, w_0);
}

fn VectorInputs_i32_0( this_8 : ptr<function, VectorInputs_0>) -> i32
{
    var _S3 : u32 = VectorInputs_u32_1(&((*this_8)));
    return (bitcast<i32>((_S3)));
}

fn mirror_sign_0( m_0 : mat3x3<f32>) -> f32
{
    var _S4 : f32;
    if((dot(m_0[i32(0)], cross(m_0[i32(1)], m_0[i32(2)]))) < 0.0f)
    {
        _S4 = -1.0f;
    }
    else
    {
        _S4 = 1.0f;
    }
    return _S4;
}

fn normal_transform_0( m_1 : mat3x3<f32>) -> mat3x3<f32>
{
    var _S5 : mat3x3<f32> = mat3x3<f32>(cross(m_1[i32(1)], m_1[i32(2)]), cross(m_1[i32(2)], m_1[i32(0)]), cross(m_1[i32(0)], m_1[i32(1)]));
    var _S6 : mat3x3<f32> = mat3x3<f32>(mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1));
    return mat3x3<f32>(_S5[0] * _S6[0], _S5[1] * _S6[1], _S5[2] * _S6[2]);
}

struct VectorOutputs_0
{
     at_1 : u32,
};

fn vector_outputs_0( case_index_1 : u32) -> VectorOutputs_0
{
    var outputs_0 : VectorOutputs_0;
    outputs_0.at_1 = case_index_1 * vectorInputs_0[i32(1)];
    return outputs_0;
}

fn transform_uv_0( uv_0 : vec2<f32>,  row0_0 : vec3<f32>,  row1_0 : vec3<f32>) -> vec2<f32>
{
    var _S7 : vec3<f32> = vec3<f32>(uv_0, 1.0f);
    return vec2<f32>(dot(_S7, row0_0), dot(_S7, row1_0));
}

fn VectorOutputs_put_0( this_9 : ptr<function, VectorOutputs_0>,  word_2 : u32)
{
    vectorOutputs_0[(*this_9).at_1] = word_2;
    (*this_9).at_1 = (*this_9).at_1 + u32(1);
    return;
}

fn VectorOutputs_put_1( this_10 : ptr<function, VectorOutputs_0>,  word_3 : u32)
{
    vectorOutputs_0[(*this_10).at_1] = word_3;
    (*this_10).at_1 = (*this_10).at_1 + u32(1);
    return;
}

fn VectorOutputs_put_2( this_11 : ptr<function, VectorOutputs_0>,  value_0 : f32)
{
    VectorOutputs_put_1(&((*this_11)), (bitcast<u32>((value_0))));
    return;
}

fn VectorOutputs_put_3( this_12 : ptr<function, VectorOutputs_0>,  value_1 : f32)
{
    VectorOutputs_put_1(&((*this_12)), (bitcast<u32>((value_1))));
    return;
}

fn VectorOutputs_put_4( this_13 : ptr<function, VectorOutputs_0>,  value_2 : vec2<f32>)
{
    VectorOutputs_put_3(&((*this_13)), value_2.x);
    VectorOutputs_put_3(&((*this_13)), value_2.y);
    return;
}

fn normalize_or_0( v_0 : vec3<f32>,  fallback_0 : vec3<f32>) -> vec3<f32>
{
    var _S8 : vec3<f32>;
    if((dot(v_0, v_0)) > 1.00000000317107685e-30f)
    {
        _S8 = normalize(v_0);
    }
    else
    {
        _S8 = fallback_0;
    }
    return _S8;
}

fn VectorOutputs_put_5( this_14 : ptr<function, VectorOutputs_0>,  value_3 : vec3<f32>)
{
    VectorOutputs_put_3(&((*this_14)), value_3.x);
    VectorOutputs_put_3(&((*this_14)), value_3.y);
    VectorOutputs_put_3(&((*this_14)), value_3.z);
    return;
}

fn tangent_space_normal_0( texel_0 : vec3<f32>,  scale_0 : f32) -> vec3<f32>
{
    var n_0 : vec3<f32> = texel_0 * vec3<f32>(2.0f) - vec3<f32>(1.0f);
    if((texel_0.z) == 0.0f)
    {
        n_0[i32(2)] = sqrt(saturate(1.0f - dot(n_0.xy, n_0.xy)));
    }
    return vec3<f32>(n_0.xy * vec2<f32>(scale_0), n_0.z);
}

fn apply_normal_map_0( normal_0 : vec3<f32>,  tangent_0 : vec4<f32>,  tangent_normal_0 : vec3<f32>,  front_face_0 : bool) -> vec3<f32>
{
    var _S9 : vec3<f32> = tangent_0.xyz;
    var t_0 : vec3<f32> = normalize_or_0(_S9 - normal_0 * vec3<f32>(dot(normal_0, _S9)), vec3<f32>(0.0f));
    var mapped_0 : vec3<f32> = normalize_or_0((((mat3x3<f32>(t_0, cross(normal_0, t_0) * vec3<f32>(tangent_0.w), normal_0)) * (tangent_normal_0))), normal_0);
    var _S10 : vec3<f32>;
    if(front_face_0)
    {
        _S10 = mapped_0;
    }
    else
    {
        _S10 = (vec3<f32>(0) - mapped_0);
    }
    return _S10;
}

fn material_metallic_0( factor_0 : f32,  sampled_0 : f32) -> f32
{
    return clamp(factor_0 * sampled_0, 0.0f, 1.0f);
}

fn material_roughness_0( factor_1 : f32,  sampled_1 : f32) -> f32
{
    return clamp(factor_1 * sampled_1, 0.04500000178813934f, 1.0f);
}

fn alpha_masked_out_0( alpha_mode_0 : i32,  alpha_0 : f32,  cutoff_0 : f32) -> bool
{
    var _S11 : bool;
    if(alpha_mode_0 == i32(1))
    {
        _S11 = alpha_0 < cutoff_0;
    }
    else
    {
        _S11 = false;
    }
    return _S11;
}

@compute
@workgroup_size(1, 1, 1)
fn gltf_material_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S12 : u32 = tid_0.x;
    var inputs_1 : VectorInputs_0 = vector_inputs_0(_S12);
    var uv_1 : vec2<f32> = VectorInputs_f32x2_0(&(inputs_1));
    var row0_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var row1_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var v_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var fallback_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var texel_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var scale_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var normal_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var tangent_1 : vec4<f32> = VectorInputs_f32x4_0(&(inputs_1));
    var tangent_normal_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var _S13 : u32 = VectorInputs_u32_0(&(inputs_1));
    var front_face_1 : bool = _S13 != u32(0);
    var m0_0 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var m1_0 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var m2_0 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var factor_2 : f32 = VectorInputs_f32_0(&(inputs_1));
    var sampled_2 : f32 = VectorInputs_f32_0(&(inputs_1));
    var mode_0 : i32 = VectorInputs_i32_0(&(inputs_1));
    var alpha_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var cutoff_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var m_2 : mat3x3<f32> = mat3x3<f32>(m0_0, m1_0, m2_0);
    var normal_m_0 : mat3x3<f32> = normal_transform_0(m_2);
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S12);
    VectorOutputs_put_4(&(outputs_1), transform_uv_0(uv_1, row0_1, row1_1));
    VectorOutputs_put_5(&(outputs_1), normalize_or_0(v_1, fallback_1));
    VectorOutputs_put_5(&(outputs_1), tangent_space_normal_0(texel_1, scale_1));
    VectorOutputs_put_5(&(outputs_1), apply_normal_map_0(normal_1, tangent_1, tangent_normal_1, front_face_1));
    VectorOutputs_put_2(&(outputs_1), mirror_sign_0(m_2));
    VectorOutputs_put_5(&(outputs_1), normal_m_0[i32(0)]);
    VectorOutputs_put_5(&(outputs_1), normal_m_0[i32(1)]);
    VectorOutputs_put_5(&(outputs_1), normal_m_0[i32(2)]);
    VectorOutputs_put_2(&(outputs_1), material_metallic_0(factor_2, sampled_2));
    VectorOutputs_put_2(&(outputs_1), material_roughness_0(factor_2, sampled_2));
    var _S14 : u32;
    if(alpha_masked_out_0(mode_0, alpha_1, cutoff_1))
    {
        _S14 = u32(1);
    }
    else
    {
        _S14 = u32(0);
    }
    VectorOutputs_put_0(&(outputs_1), _S14);
    return;
}

