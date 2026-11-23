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

fn hdr_safe_0( radiance_0 : vec3<f32>) -> vec3<f32>
{
    var _S1 : vec3<f32> = vec3<f32>(0.0f);
    return clamp(select(radiance_0, _S1, (((bitcast<vec3<u32>>((radiance_0))) & (vec3<u32>(u32(2147483647))))) > vec3<u32>(u32(2139095040))), _S1, vec3<f32>(65000.0f));
}

fn VectorOutputs_put_0( this_1 : ptr<function, VectorOutputs_0>,  word_1 : u32)
{
    vectorOutputs_0[(*this_1).at_1] = word_1;
    (*this_1).at_1 = (*this_1).at_1 + u32(1);
    return;
}

fn VectorOutputs_put_1( this_2 : ptr<function, VectorOutputs_0>,  word_2 : u32)
{
    vectorOutputs_0[(*this_2).at_1] = word_2;
    (*this_2).at_1 = (*this_2).at_1 + u32(1);
    return;
}

fn VectorOutputs_put_2( this_3 : ptr<function, VectorOutputs_0>,  value_0 : f32)
{
    VectorOutputs_put_1(&((*this_3)), (bitcast<u32>((value_0))));
    return;
}

fn VectorOutputs_put_3( this_4 : ptr<function, VectorOutputs_0>,  value_1 : vec3<f32>)
{
    VectorOutputs_put_2(&((*this_4)), value_1.x);
    VectorOutputs_put_2(&((*this_4)), value_1.y);
    VectorOutputs_put_2(&((*this_4)), value_1.z);
    return;
}

fn is_finite_0( x_0 : f32) -> bool
{
    return (((bitcast<u32>((x_0))) & (u32(2139095040)))) != u32(2139095040);
}

@compute
@workgroup_size(1, 1, 1)
fn hdr_safe_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S2 : u32 = tid_0.x;
    var inputs_1 : VectorInputs_0 = vector_inputs_0(_S2);
    var x_1 : u32 = VectorInputs_u32_0(&(inputs_1));
    var y_0 : u32 = VectorInputs_u32_0(&(inputs_1));
    var z_0 : u32 = VectorInputs_u32_0(&(inputs_1));
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S2);
    VectorOutputs_put_3(&(outputs_1), hdr_safe_0((bitcast<vec3<f32>>((vec3<u32>(x_1, y_0, z_0))))));
    var _S3 : u32;
    if(is_finite_0((bitcast<f32>((x_1)))))
    {
        _S3 = u32(1);
    }
    else
    {
        _S3 = u32(0);
    }
    VectorOutputs_put_0(&(outputs_1), _S3);
    if(is_finite_0((bitcast<f32>((y_0)))))
    {
        _S3 = u32(1);
    }
    else
    {
        _S3 = u32(0);
    }
    VectorOutputs_put_0(&(outputs_1), _S3);
    if(is_finite_0((bitcast<f32>((z_0)))))
    {
        _S3 = u32(1);
    }
    else
    {
        _S3 = u32(0);
    }
    VectorOutputs_put_0(&(outputs_1), _S3);
    return;
}

