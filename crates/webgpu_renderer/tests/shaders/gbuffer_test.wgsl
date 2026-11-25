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

fn VectorInputs_f32x3_1( this_5 : ptr<function, VectorInputs_0>) -> vec3<f32>
{
    var x_1 : f32 = VectorInputs_f32_1(&((*this_5)));
    var y_1 : f32 = VectorInputs_f32_1(&((*this_5)));
    var z_1 : f32 = VectorInputs_f32_1(&((*this_5)));
    return vec3<f32>(x_1, y_1, z_1);
}

fn VectorInputs_f32x4_0( this_6 : ptr<function, VectorInputs_0>) -> vec4<f32>
{
    var xyz_0 : vec3<f32> = VectorInputs_f32x3_1(&((*this_6)));
    var w_0 : f32 = VectorInputs_f32_1(&((*this_6)));
    return vec4<f32>(xyz_0, w_0);
}

struct GBufferTexels_0
{
     normalMetallic_0 : vec4<f32>,
     albedoOcclusion_0 : vec4<f32>,
     roughnessEmissive_0 : vec4<f32>,
};

struct SurfacePoint_0
{
     position_0 : vec3<f32>,
     n_0 : vec3<f32>,
     albedo_0 : vec3<f32>,
     metallic_0 : f32,
     roughness_0 : f32,
     occlusion_0 : f32,
     emissive_0 : vec3<f32>,
};

fn gbuffer_pack_0( s_0 : SurfacePoint_0,  unlit_0 : bool) -> GBufferTexels_0
{
    var t_0 : GBufferTexels_0;
    var _S3 : f32;
    if(unlit_0)
    {
        _S3 = -1.0f;
    }
    else
    {
        _S3 = s_0.metallic_0;
    }
    t_0.normalMetallic_0 = vec4<f32>(s_0.n_0, _S3);
    t_0.albedoOcclusion_0 = vec4<f32>(s_0.albedo_0, s_0.occlusion_0);
    t_0.roughnessEmissive_0 = vec4<f32>(s_0.roughness_0, s_0.emissive_0);
    return t_0;
}

fn normalize_or_0( v_0 : vec3<f32>,  fallback_0 : vec3<f32>) -> vec3<f32>
{
    var _S4 : vec3<f32>;
    if((dot(v_0, v_0)) > 1.00000000317107685e-30f)
    {
        _S4 = normalize(v_0);
    }
    else
    {
        _S4 = fallback_0;
    }
    return _S4;
}

fn gbuffer_unpack_0( t_1 : GBufferTexels_0,  position_1 : vec3<f32>) -> SurfacePoint_0
{
    var s_1 : SurfacePoint_0;
    s_1.position_0 = position_1;
    s_1.n_0 = normalize_or_0(t_1.normalMetallic_0.xyz, vec3<f32>(0.0f, 1.0f, 0.0f));
    s_1.albedo_0 = t_1.albedoOcclusion_0.xyz;
    s_1.metallic_0 = t_1.normalMetallic_0.w;
    s_1.roughness_0 = t_1.roughnessEmissive_0.x;
    s_1.occlusion_0 = t_1.albedoOcclusion_0.w;
    s_1.emissive_0 = t_1.roughnessEmissive_0.yzw;
    return s_1;
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

fn VectorOutputs_put_0( this_7 : ptr<function, VectorOutputs_0>,  word_2 : u32)
{
    vectorOutputs_0[(*this_7).at_1] = word_2;
    (*this_7).at_1 = (*this_7).at_1 + u32(1);
    return;
}

fn VectorOutputs_put_1( this_8 : ptr<function, VectorOutputs_0>,  word_3 : u32)
{
    vectorOutputs_0[(*this_8).at_1] = word_3;
    (*this_8).at_1 = (*this_8).at_1 + u32(1);
    return;
}

fn VectorOutputs_put_2( this_9 : ptr<function, VectorOutputs_0>,  value_0 : f32)
{
    VectorOutputs_put_1(&((*this_9)), (bitcast<u32>((value_0))));
    return;
}

fn VectorOutputs_put_3( this_10 : ptr<function, VectorOutputs_0>,  value_1 : vec3<f32>)
{
    VectorOutputs_put_2(&((*this_10)), value_1.x);
    VectorOutputs_put_2(&((*this_10)), value_1.y);
    VectorOutputs_put_2(&((*this_10)), value_1.z);
    return;
}

fn VectorOutputs_put_4( this_11 : ptr<function, VectorOutputs_0>,  value_2 : vec3<f32>)
{
    VectorOutputs_put_2(&((*this_11)), value_2.x);
    VectorOutputs_put_2(&((*this_11)), value_2.y);
    VectorOutputs_put_2(&((*this_11)), value_2.z);
    return;
}

fn VectorOutputs_put_5( this_12 : ptr<function, VectorOutputs_0>,  value_3 : vec4<f32>)
{
    VectorOutputs_put_4(&((*this_12)), value_3.xyz);
    VectorOutputs_put_2(&((*this_12)), value_3.w);
    return;
}

fn gbuffer_unlit_0( normalMetallic_1 : vec4<f32>) -> bool
{
    return (normalMetallic_1.w) < -0.5f;
}

@compute
@workgroup_size(1, 1, 1)
fn gbuffer_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S5 : u32 = tid_0.x;
    var inputs_1 : VectorInputs_0 = vector_inputs_0(_S5);
    var s_2 : SurfacePoint_0;
    var _S6 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    s_2.position_0 = _S6;
    var _S7 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    s_2.n_0 = _S7;
    var _S8 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    s_2.albedo_0 = _S8;
    var _S9 : f32 = VectorInputs_f32_0(&(inputs_1));
    s_2.metallic_0 = _S9;
    var _S10 : f32 = VectorInputs_f32_0(&(inputs_1));
    s_2.roughness_0 = _S10;
    var _S11 : f32 = VectorInputs_f32_0(&(inputs_1));
    s_2.occlusion_0 = _S11;
    var _S12 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    s_2.emissive_0 = _S12;
    var _S13 : u32 = VectorInputs_u32_0(&(inputs_1));
    var unlit_1 : bool = _S13 != u32(0);
    var stored_0 : vec4<f32> = VectorInputs_f32x4_0(&(inputs_1));
    var packed_0 : GBufferTexels_0 = gbuffer_pack_0(s_2, unlit_1);
    var read_0 : GBufferTexels_0 = packed_0;
    read_0.normalMetallic_0 = stored_0;
    var back_0 : SurfacePoint_0 = gbuffer_unpack_0(read_0, _S6);
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S5);
    VectorOutputs_put_5(&(outputs_1), packed_0.normalMetallic_0);
    VectorOutputs_put_5(&(outputs_1), packed_0.albedoOcclusion_0);
    VectorOutputs_put_5(&(outputs_1), packed_0.roughnessEmissive_0);
    var _S14 : u32;
    if(gbuffer_unlit_0(stored_0))
    {
        _S14 = u32(1);
    }
    else
    {
        _S14 = u32(0);
    }
    VectorOutputs_put_0(&(outputs_1), _S14);
    VectorOutputs_put_3(&(outputs_1), back_0.position_0);
    VectorOutputs_put_3(&(outputs_1), back_0.n_0);
    VectorOutputs_put_3(&(outputs_1), back_0.albedo_0);
    VectorOutputs_put_3(&(outputs_1), vec3<f32>(back_0.metallic_0, back_0.roughness_0, back_0.occlusion_0));
    VectorOutputs_put_3(&(outputs_1), back_0.emissive_0);
    return;
}

