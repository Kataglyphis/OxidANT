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

fn surface_f0_0( albedo_0 : vec3<f32>,  metallic_0 : f32) -> vec3<f32>
{
    return mix(vec3<f32>(0.03999999910593033f), albedo_0, vec3<f32>(metallic_0));
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

fn VectorOutputs_put_2( this_6 : ptr<function, VectorOutputs_0>,  value_1 : vec3<f32>)
{
    VectorOutputs_put_1(&((*this_6)), value_1.x);
    VectorOutputs_put_1(&((*this_6)), value_1.y);
    VectorOutputs_put_1(&((*this_6)), value_1.z);
    return;
}

fn hdr_safe_0( radiance_0 : vec3<f32>) -> vec3<f32>
{
    var _S3 : vec3<f32> = vec3<f32>(0.0f);
    return clamp(select(radiance_0, _S3, (((bitcast<vec3<u32>>((radiance_0))) & (vec3<u32>(u32(2147483647))))) > vec3<u32>(u32(2139095040))), _S3, vec3<f32>(65000.0f));
}

fn surface_lighting_0( sunDirect_0 : vec3<f32>,  sunVisibility_0 : f32,  shadowStrength_0 : f32,  punctual_0 : vec3<f32>,  ambient_0 : vec3<f32>,  emissive_0 : vec3<f32>) -> vec3<f32>
{
    return hdr_safe_0(sunDirect_0 * vec3<f32>(mix(1.0f, sunVisibility_0, shadowStrength_0)) + punctual_0 + ambient_0 + emissive_0);
}

@compute
@workgroup_size(1, 1, 1)
fn surface_lighting_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S4 : u32 = tid_0.x;
    var inputs_1 : VectorInputs_0 = vector_inputs_0(_S4);
    var albedo_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var metallic_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var sun_0 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var visibility_0 : f32 = VectorInputs_f32_0(&(inputs_1));
    var strength_0 : f32 = VectorInputs_f32_0(&(inputs_1));
    var punctual_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var ambient_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var emissive_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S4);
    VectorOutputs_put_2(&(outputs_1), surface_f0_0(albedo_1, metallic_1));
    VectorOutputs_put_2(&(outputs_1), surface_lighting_0(sun_0, visibility_0, strength_0, punctual_1, ambient_1, emissive_1));
    return;
}

