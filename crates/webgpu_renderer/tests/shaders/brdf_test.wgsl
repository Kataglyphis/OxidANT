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

fn VectorInputs_f32_0( this_1 : ptr<function, VectorInputs_0>) -> f32
{
    var _S1 : u32 = VectorInputs_u32_0(&((*this_1)));
    return (bitcast<f32>((_S1)));
}

fn VectorInputs_f32_1( this_2 : ptr<function, VectorInputs_0>) -> f32
{
    var _S2 : u32 = VectorInputs_u32_0(&((*this_2)));
    return (bitcast<f32>((_S2)));
}

fn VectorInputs_f32x3_0( this_3 : ptr<function, VectorInputs_0>) -> vec3<f32>
{
    var x_0 : f32 = VectorInputs_f32_1(&((*this_3)));
    var y_0 : f32 = VectorInputs_f32_1(&((*this_3)));
    var z_0 : f32 = VectorInputs_f32_1(&((*this_3)));
    return vec3<f32>(x_0, y_0, z_0);
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

fn lambert_diffuse_0( albedo_0 : vec3<f32>) -> vec3<f32>
{
    return albedo_0 / vec3<f32>(3.14159274101257324f);
}

fn VectorOutputs_put_0( this_4 : ptr<function, VectorOutputs_0>,  word_1 : u32)
{
    vectorOutputs_0[(*this_4).at_1] = word_1;
    (*this_4).at_1 = (*this_4).at_1 + u32(1);
    return;
}

fn VectorOutputs_put_1( this_5 : ptr<function, VectorOutputs_0>,  value_0 : f32)
{
    VectorOutputs_put_0(&((*this_5)), (bitcast<u32>((value_0))));
    return;
}

fn VectorOutputs_put_2( this_6 : ptr<function, VectorOutputs_0>,  value_1 : f32)
{
    VectorOutputs_put_0(&((*this_6)), (bitcast<u32>((value_1))));
    return;
}

fn VectorOutputs_put_3( this_7 : ptr<function, VectorOutputs_0>,  value_2 : vec3<f32>)
{
    VectorOutputs_put_2(&((*this_7)), value_2.x);
    VectorOutputs_put_2(&((*this_7)), value_2.y);
    VectorOutputs_put_2(&((*this_7)), value_2.z);
    return;
}

fn distribution_ggx_0( n_dot_h_0 : f32,  roughness_0 : f32) -> f32
{
    var _S3 : f32 = max(roughness_0, 0.04500000178813934f);
    var a_0 : f32 = _S3 * _S3;
    var a2_0 : f32 = a_0 * a_0;
    var denom_0 : f32 = n_dot_h_0 * n_dot_h_0 * (a2_0 - 1.0f) + 1.0f;
    return a2_0 / (3.14159274101257324f * max(denom_0 * denom_0, 9.99999968265522539e-21f));
}

fn smith_k_analytic_0( roughness_1 : f32) -> f32
{
    var r_0 : f32 = roughness_1 + 1.0f;
    return r_0 * r_0 / 8.0f;
}

fn smith_k_sampled_0( roughness_2 : f32) -> f32
{
    return roughness_2 * roughness_2 / 2.0f;
}

fn geometry_smith_k_0( n_dot_v_0 : f32,  n_dot_l_0 : f32,  k_0 : f32) -> f32
{
    var _S4 : f32 = 1.0f - k_0;
    return n_dot_v_0 / max(n_dot_v_0 * _S4 + k_0, 9.99999997475242708e-07f) * (n_dot_l_0 / max(n_dot_l_0 * _S4 + k_0, 9.99999997475242708e-07f));
}

fn fresnel_schlick_0( cos_theta_0 : f32,  f0_0 : vec3<f32>) -> vec3<f32>
{
    return f0_0 + (vec3<f32>(1.0f) - f0_0) * vec3<f32>(pow(clamp(1.0f - cos_theta_0, 0.0f, 1.0f), 5.0f));
}

fn brdf_smith_k_0( n_0 : vec3<f32>,  v_0 : vec3<f32>,  l_0 : vec3<f32>,  albedo_1 : vec3<f32>,  metallic_0 : f32,  roughness_3 : f32,  f0_1 : vec3<f32>,  radiance_0 : vec3<f32>,  k_1 : f32) -> vec3<f32>
{
    var h_0 : vec3<f32> = normalize(l_0 + v_0);
    var _S5 : f32 = max(dot(n_0, l_0), 0.0f);
    var _S6 : f32 = max(dot(n_0, v_0), 0.00009999999747379f);
    var f_0 : vec3<f32> = fresnel_schlick_0(max(dot(h_0, v_0), 0.0f), f0_1);
    return ((vec3<f32>(1.0f) - f_0) * vec3<f32>((1.0f - metallic_0)) * lambert_diffuse_0(albedo_1) + vec3<f32>((distribution_ggx_0(max(dot(n_0, h_0), 0.0f), roughness_3) * geometry_smith_k_0(_S6, _S5, k_1))) * f_0 / vec3<f32>(max(4.0f * _S6 * _S5, 9.99999997475242708e-07f))) * radiance_0 * vec3<f32>(_S5);
}

fn brdf_direct_0( n_1 : vec3<f32>,  v_1 : vec3<f32>,  l_1 : vec3<f32>,  albedo_2 : vec3<f32>,  metallic_1 : f32,  roughness_4 : f32,  f0_2 : vec3<f32>,  radiance_1 : vec3<f32>) -> vec3<f32>
{
    return brdf_smith_k_0(n_1, v_1, l_1, albedo_2, metallic_1, roughness_4, f0_2, radiance_1, smith_k_analytic_0(roughness_4));
}

fn brdf_sampled_0( n_2 : vec3<f32>,  v_2 : vec3<f32>,  l_2 : vec3<f32>,  albedo_3 : vec3<f32>,  metallic_2 : f32,  roughness_5 : f32,  f0_3 : vec3<f32>) -> vec3<f32>
{
    return brdf_smith_k_0(n_2, v_2, l_2, albedo_3, metallic_2, roughness_5, f0_3, vec3<f32>(1.0f), smith_k_sampled_0(roughness_5));
}

@compute
@workgroup_size(1, 1, 1)
fn brdf_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S7 : u32 = tid_0.x;
    var inputs_1 : VectorInputs_0 = vector_inputs_0(_S7);
    var n_3 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var v_3 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var l_3 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var albedo_4 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var metallic_3 : f32 = VectorInputs_f32_0(&(inputs_1));
    var roughness_6 : f32 = VectorInputs_f32_0(&(inputs_1));
    var f0_4 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var radiance_2 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var k_2 : f32 = VectorInputs_f32_0(&(inputs_1));
    var n_dot_h_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var n_dot_v_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var n_dot_l_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var cos_theta_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S7);
    VectorOutputs_put_3(&(outputs_1), lambert_diffuse_0(albedo_4));
    VectorOutputs_put_1(&(outputs_1), distribution_ggx_0(n_dot_h_1, roughness_6));
    VectorOutputs_put_1(&(outputs_1), smith_k_analytic_0(roughness_6));
    VectorOutputs_put_1(&(outputs_1), smith_k_sampled_0(roughness_6));
    VectorOutputs_put_1(&(outputs_1), geometry_smith_k_0(n_dot_v_1, n_dot_l_1, k_2));
    VectorOutputs_put_3(&(outputs_1), fresnel_schlick_0(cos_theta_1, f0_4));
    VectorOutputs_put_3(&(outputs_1), brdf_smith_k_0(n_3, v_3, l_3, albedo_4, metallic_3, roughness_6, f0_4, radiance_2, k_2));
    VectorOutputs_put_3(&(outputs_1), brdf_direct_0(n_3, v_3, l_3, albedo_4, metallic_3, roughness_6, f0_4, radiance_2));
    VectorOutputs_put_3(&(outputs_1), brdf_sampled_0(n_3, v_3, l_3, albedo_4, metallic_3, roughness_6, f0_4));
    return;
}

