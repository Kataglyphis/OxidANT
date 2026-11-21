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

fn VectorInputs_f32x3_0( this_4 : ptr<function, VectorInputs_0>) -> vec3<f32>
{
    var x_0 : f32 = VectorInputs_f32_1(&((*this_4)));
    var y_0 : f32 = VectorInputs_f32_1(&((*this_4)));
    var z_0 : f32 = VectorInputs_f32_1(&((*this_4)));
    return vec3<f32>(x_0, y_0, z_0);
}

fn VectorInputs_f32x2_0( this_5 : ptr<function, VectorInputs_0>) -> vec2<f32>
{
    var x_1 : f32 = VectorInputs_f32_1(&((*this_5)));
    var y_1 : f32 = VectorInputs_f32_1(&((*this_5)));
    return vec2<f32>(x_1, y_1);
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

fn path_rng_seed_0( pixel_index_0 : u32,  stream_0 : u32) -> u32
{
    return pixel_index_0 + stream_0 * u32(2654435769);
}

fn VectorOutputs_put_0( this_6 : ptr<function, VectorOutputs_0>,  word_2 : u32)
{
    vectorOutputs_0[(*this_6).at_1] = word_2;
    (*this_6).at_1 = (*this_6).at_1 + u32(1);
    return;
}

fn VectorOutputs_put_1( this_7 : ptr<function, VectorOutputs_0>,  word_3 : u32)
{
    vectorOutputs_0[(*this_7).at_1] = word_3;
    (*this_7).at_1 = (*this_7).at_1 + u32(1);
    return;
}

fn rng_next_0( state_0 : ptr<function, u32>) -> f32
{
    var _S3 : u32 = (*state_0) * u32(747796405) + u32(1);
    (*state_0) = _S3;
    var word_4 : u32 = ((((_S3 >> ((((_S3 >> (u32(28)))) + u32(4))))) ^ (_S3))) * u32(277803737);
    return f32((((word_4 >> (u32(22)))) ^ (word_4))) / 4.294967296e+09f;
}

fn rng_next_1( state_1 : ptr<function, u32>) -> f32
{
    var _S4 : u32 = (*state_1) * u32(747796405) + u32(1);
    (*state_1) = _S4;
    var word_5 : u32 = ((((_S4 >> ((((_S4 >> (u32(28)))) + u32(4))))) ^ (_S4))) * u32(277803737);
    return f32((((word_5 >> (u32(22)))) ^ (word_5))) / 4.294967296e+09f;
}

fn VectorOutputs_put_2( this_8 : ptr<function, VectorOutputs_0>,  value_0 : f32)
{
    VectorOutputs_put_1(&((*this_8)), (bitcast<u32>((value_0))));
    return;
}

fn VectorOutputs_put_3( this_9 : ptr<function, VectorOutputs_0>,  value_1 : f32)
{
    VectorOutputs_put_1(&((*this_9)), (bitcast<u32>((value_1))));
    return;
}

fn russian_roulette_survives_0( bounce_0 : u32,  throughput_0 : ptr<function, vec3<f32>>,  rng_0 : ptr<function, u32>) -> bool
{
    if(bounce_0 < u32(3))
    {
        return true;
    }
    var survival_0 : f32 = clamp(max((*throughput_0).x, max((*throughput_0).y, (*throughput_0).z)), 0.05000000074505806f, 0.94999998807907104f);
    var _S5 : f32 = rng_next_1(&((*rng_0)));
    if(_S5 > survival_0)
    {
        return false;
    }
    (*throughput_0) = (*throughput_0) / vec3<f32>(survival_0);
    return true;
}

fn VectorOutputs_put_4( this_10 : ptr<function, VectorOutputs_0>,  value_2 : vec3<f32>)
{
    VectorOutputs_put_3(&((*this_10)), value_2.x);
    VectorOutputs_put_3(&((*this_10)), value_2.y);
    VectorOutputs_put_3(&((*this_10)), value_2.z);
    return;
}

fn orthonormal_basis_0( n_0 : vec3<f32>,  t_0 : ptr<function, vec3<f32>>,  b_0 : ptr<function, vec3<f32>>)
{
    var _S6 : f32 = n_0.z;
    var s_0 : f32;
    if(_S6 >= 0.0f)
    {
        s_0 = 1.0f;
    }
    else
    {
        s_0 = -1.0f;
    }
    var a_0 : f32 = -1.0f / (s_0 + _S6);
    var _S7 : f32 = n_0.x;
    var _S8 : f32 = n_0.y;
    var c_0 : f32 = _S7 * _S8 * a_0;
    (*t_0) = vec3<f32>(1.0f + s_0 * _S7 * _S7 * a_0, s_0 * c_0, - s_0 * _S7);
    (*b_0) = vec3<f32>(c_0, s_0 + _S8 * _S8 * a_0, - _S8);
    return;
}

fn sample_ggx_visible_normal_0( v_0 : vec3<f32>,  alpha_0 : f32,  u_0 : vec2<f32>) -> vec3<f32>
{
    var vh_0 : vec3<f32> = normalize(vec3<f32>(alpha_0 * v_0.x, alpha_0 * v_0.y, v_0.z));
    var _S9 : f32 = vh_0.x;
    var _S10 : f32 = vh_0.y;
    var length_squared_0 : f32 = _S9 * _S9 + _S10 * _S10;
    var t1_0 : vec3<f32>;
    if(length_squared_0 > 0.0f)
    {
        t1_0 = vec3<f32>(- _S10, _S9, 0.0f) / vec3<f32>(sqrt(length_squared_0));
    }
    else
    {
        t1_0 = vec3<f32>(1.0f, 0.0f, 0.0f);
    }
    var r_0 : f32 = sqrt(u_0.x);
    var phi_0 : f32 = 6.28318548202514648f * u_0.y;
    var p1_0 : f32 = r_0 * cos(phi_0);
    var s_1 : f32 = 0.5f * (1.0f + vh_0.z);
    var _S11 : f32 = 1.0f - p1_0 * p1_0;
    var p2_0 : f32 = (1.0f - s_1) * sqrt(max(_S11, 0.0f)) + s_1 * (r_0 * sin(phi_0));
    var nh_0 : vec3<f32> = vec3<f32>(p1_0) * t1_0 + vec3<f32>(p2_0) * cross(vh_0, t1_0) + vec3<f32>(sqrt(max(_S11 - p2_0 * p2_0, 0.0f))) * vh_0;
    return normalize(vec3<f32>(alpha_0 * nh_0.x, alpha_0 * nh_0.y, max(nh_0.z, 9.99999997475242708e-07f)));
}

fn ggx_visible_normal_pdf_0( n_dot_v_0 : f32,  n_dot_h_0 : f32,  alpha_1 : f32) -> f32
{
    var a2_0 : f32 = alpha_1 * alpha_1;
    var d_0 : f32 = n_dot_h_0 * n_dot_h_0 * (a2_0 - 1.0f) + 1.0f;
    return a2_0 / (3.14159274101257324f * d_0 * d_0) * (2.0f * n_dot_v_0 / (n_dot_v_0 + sqrt(a2_0 + (1.0f - a2_0) * n_dot_v_0 * n_dot_v_0))) / (4.0f * n_dot_v_0);
}

fn fresnel_schlick_0( cos_theta_0 : f32,  f0_0 : vec3<f32>) -> vec3<f32>
{
    return f0_0 + (vec3<f32>(1.0f) - f0_0) * vec3<f32>(pow(clamp(1.0f - cos_theta_0, 0.0f, 1.0f), 5.0f));
}

fn specular_lobe_probability_0( albedo_0 : vec3<f32>,  metallic_0 : f32,  f0_1 : vec3<f32>,  n_dot_v_1 : f32) -> f32
{
    const LUMA_0 : vec3<f32> = vec3<f32>(0.2125999927520752f, 0.71520000696182251f, 0.07220000028610229f);
    var fresnel_0 : vec3<f32> = fresnel_schlick_0(n_dot_v_1, f0_1);
    var specular_0 : f32 = dot(fresnel_0, LUMA_0);
    return clamp(specular_0 / max(specular_0 + dot((vec3<f32>(1.0f) - fresnel_0) * vec3<f32>((1.0f - metallic_0)) * albedo_0, LUMA_0), 9.99999997475242708e-07f), 0.10000000149011612f, 1.0f);
}

fn smith_k_sampled_0( roughness_0 : f32) -> f32
{
    return roughness_0 * roughness_0 / 2.0f;
}

fn distribution_ggx_0( n_dot_h_1 : f32,  roughness_1 : f32) -> f32
{
    var _S12 : f32 = max(roughness_1, 0.04500000178813934f);
    var a_1 : f32 = _S12 * _S12;
    var a2_1 : f32 = a_1 * a_1;
    var denom_0 : f32 = n_dot_h_1 * n_dot_h_1 * (a2_1 - 1.0f) + 1.0f;
    return a2_1 / (3.14159274101257324f * max(denom_0 * denom_0, 9.99999968265522539e-21f));
}

fn geometry_smith_k_0( n_dot_v_2 : f32,  n_dot_l_0 : f32,  k_0 : f32) -> f32
{
    var _S13 : f32 = 1.0f - k_0;
    return n_dot_v_2 / max(n_dot_v_2 * _S13 + k_0, 9.99999997475242708e-07f) * (n_dot_l_0 / max(n_dot_l_0 * _S13 + k_0, 9.99999997475242708e-07f));
}

fn lambert_diffuse_0( albedo_1 : vec3<f32>) -> vec3<f32>
{
    return albedo_1 / vec3<f32>(3.14159274101257324f);
}

fn brdf_smith_k_0( n_1 : vec3<f32>,  v_1 : vec3<f32>,  l_0 : vec3<f32>,  albedo_2 : vec3<f32>,  metallic_1 : f32,  roughness_2 : f32,  f0_2 : vec3<f32>,  radiance_0 : vec3<f32>,  k_1 : f32) -> vec3<f32>
{
    var h_0 : vec3<f32> = normalize(l_0 + v_1);
    var _S14 : f32 = max(dot(n_1, l_0), 0.0f);
    var _S15 : f32 = max(dot(n_1, v_1), 0.00009999999747379f);
    var f_0 : vec3<f32> = fresnel_schlick_0(max(dot(h_0, v_1), 0.0f), f0_2);
    return ((vec3<f32>(1.0f) - f_0) * vec3<f32>((1.0f - metallic_1)) * lambert_diffuse_0(albedo_2) + vec3<f32>((distribution_ggx_0(max(dot(n_1, h_0), 0.0f), roughness_2) * geometry_smith_k_0(_S15, _S14, k_1))) * f_0 / vec3<f32>(max(4.0f * _S15 * _S14, 9.99999997475242708e-07f))) * radiance_0 * vec3<f32>(_S14);
}

fn brdf_sampled_0( n_2 : vec3<f32>,  v_2 : vec3<f32>,  l_1 : vec3<f32>,  albedo_3 : vec3<f32>,  metallic_2 : f32,  roughness_3 : f32,  f0_3 : vec3<f32>) -> vec3<f32>
{
    return brdf_smith_k_0(n_2, v_2, l_1, albedo_3, metallic_2, roughness_3, f0_3, vec3<f32>(1.0f), smith_k_sampled_0(roughness_3));
}

struct Bounce_0
{
     direction_0 : vec3<f32>,
     weight_0 : vec3<f32>,
};

fn sample_bounce_0( n_3 : vec3<f32>,  face_normal_0 : vec3<f32>,  v_3 : vec3<f32>,  albedo_4 : vec3<f32>,  metallic_3 : f32,  roughness_4 : f32,  rng_1 : ptr<function, u32>) -> Bounce_0
{
    var f0_4 : vec3<f32> = mix(vec3<f32>(0.03999999910593033f), albedo_4, vec3<f32>(metallic_3));
    var _S16 : f32 = max(dot(n_3, v_3), 0.00009999999747379f);
    var specular_1 : f32 = specular_lobe_probability_0(albedo_4, metallic_3, f0_4, _S16);
    var alpha_2 : f32 = roughness_4 * roughness_4;
    var t_1 : vec3<f32>;
    var b_1 : vec3<f32>;
    orthonormal_basis_0(n_3, &(t_1), &(b_1));
    var _S17 : f32 = rng_next_0(&((*rng_1)));
    var direction_1 : vec3<f32>;
    if(_S17 < specular_1)
    {
        var local_0 : vec3<f32> = vec3<f32>(dot(v_3, t_1), dot(v_3, b_1), dot(v_3, n_3));
        var _S18 : f32 = rng_next_0(&((*rng_1)));
        var _S19 : f32 = rng_next_0(&((*rng_1)));
        var h_1 : vec3<f32> = sample_ggx_visible_normal_0(local_0, alpha_2, vec2<f32>(_S18, _S19));
        direction_1 = reflect((vec3<f32>(0) - v_3), vec3<f32>(h_1.x) * t_1 + vec3<f32>(h_1.y) * b_1 + vec3<f32>(h_1.z) * n_3);
    }
    else
    {
        var _S20 : f32 = rng_next_0(&((*rng_1)));
        var theta_0 : f32 = 6.28318548202514648f * _S20;
        var _S21 : f32 = rng_next_0(&((*rng_1)));
        var z_1 : f32 = 2.0f * _S21 - 1.0f;
        var r_1 : f32 = sqrt(max(1.0f - z_1 * z_1, 0.0f));
        var direction_2 : vec3<f32> = n_3 + vec3<f32>(r_1 * cos(theta_0), r_1 * sin(theta_0), z_1);
        if((dot(direction_2, direction_2)) < 9.99999993922529029e-09f)
        {
            direction_1 = n_3;
        }
        else
        {
            direction_1 = direction_2;
        }
    }
    var o_0 : Bounce_0;
    var _S22 : vec3<f32> = normalize(direction_1);
    o_0.direction_0 = _S22;
    o_0.weight_0 = vec3<f32>(0.0f);
    var n_dot_l_1 : f32 = dot(n_3, _S22);
    var _S23 : bool;
    if(n_dot_l_1 <= 0.0f)
    {
        _S23 = true;
    }
    else
    {
        _S23 = (dot(face_normal_0, o_0.direction_0)) <= 0.0f;
    }
    if(_S23)
    {
        return o_0;
    }
    o_0.weight_0 = brdf_sampled_0(n_3, v_3, o_0.direction_0, albedo_4, metallic_3, roughness_4, f0_4) / vec3<f32>(max(specular_1 * ggx_visible_normal_pdf_0(_S16, max(dot(n_3, normalize(v_3 + o_0.direction_0)), 0.0f), alpha_2) + (1.0f - specular_1) * n_dot_l_1 / 3.14159274101257324f, 9.99999997475242708e-07f));
    return o_0;
}

@compute
@workgroup_size(1, 1, 1)
fn path_estimator_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S24 : u32 = tid_0.x;
    var inputs_1 : VectorInputs_0 = vector_inputs_0(_S24);
    var pixel_index_1 : u32 = VectorInputs_u32_0(&(inputs_1));
    var stream_1 : u32 = VectorInputs_u32_0(&(inputs_1));
    var rng_2 : u32 = VectorInputs_u32_0(&(inputs_1));
    var bounce_1 : u32 = VectorInputs_u32_0(&(inputs_1));
    var throughput_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var n_4 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var face_normal_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var v_4 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var albedo_5 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var metallic_4 : f32 = VectorInputs_f32_0(&(inputs_1));
    var roughness_5 : f32 = VectorInputs_f32_0(&(inputs_1));
    var f0_5 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var local_v_0 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var alpha_3 : f32 = VectorInputs_f32_0(&(inputs_1));
    var u_1 : vec2<f32> = VectorInputs_f32x2_0(&(inputs_1));
    var n_dot_v_3 : f32 = VectorInputs_f32_0(&(inputs_1));
    var n_dot_h_2 : f32 = VectorInputs_f32_0(&(inputs_1));
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S24);
    VectorOutputs_put_0(&(outputs_1), path_rng_seed_0(pixel_index_1, stream_1));
    var drawn_rng_0 : u32 = rng_2;
    var drawn_0 : f32 = rng_next_0(&(drawn_rng_0));
    VectorOutputs_put_2(&(outputs_1), drawn_0);
    VectorOutputs_put_0(&(outputs_1), drawn_rng_0);
    var roulette_rng_0 : u32 = rng_2;
    var kept_0 : vec3<f32> = throughput_1;
    var alive_0 : bool = russian_roulette_survives_0(bounce_1, &(kept_0), &(roulette_rng_0));
    var _S25 : u32;
    if(alive_0)
    {
        _S25 = u32(1);
    }
    else
    {
        _S25 = u32(0);
    }
    VectorOutputs_put_0(&(outputs_1), _S25);
    VectorOutputs_put_4(&(outputs_1), kept_0);
    VectorOutputs_put_0(&(outputs_1), roulette_rng_0);
    var t_2 : vec3<f32>;
    var b_2 : vec3<f32>;
    orthonormal_basis_0(n_4, &(t_2), &(b_2));
    VectorOutputs_put_4(&(outputs_1), t_2);
    VectorOutputs_put_4(&(outputs_1), b_2);
    VectorOutputs_put_4(&(outputs_1), sample_ggx_visible_normal_0(local_v_0, alpha_3, u_1));
    VectorOutputs_put_2(&(outputs_1), ggx_visible_normal_pdf_0(n_dot_v_3, n_dot_h_2, alpha_3));
    VectorOutputs_put_2(&(outputs_1), specular_lobe_probability_0(albedo_5, metallic_4, f0_5, n_dot_v_3));
    var bounce_rng_0 : u32 = rng_2;
    var next_0 : Bounce_0 = sample_bounce_0(n_4, face_normal_1, v_4, albedo_5, metallic_4, roughness_5, &(bounce_rng_0));
    VectorOutputs_put_4(&(outputs_1), next_0.direction_0);
    VectorOutputs_put_4(&(outputs_1), next_0.weight_0);
    VectorOutputs_put_0(&(outputs_1), bounce_rng_0);
    return;
}

