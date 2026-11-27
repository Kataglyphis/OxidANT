@binding(0) @group(0) var<storage, read> vectorInputs_0 : array<u32>;

@binding(1) @group(0) var<storage, read_write> vectorOutputs_0 : array<u32>;

var<private> BLOOM_WEIGHTS_0 : array<f32, i32(5)> = array<f32, i32(5)>( 0.22702699899673462f, 0.194594606757164f, 0.12162160128355026f, 0.05405399948358536f, 0.01621600054204464f );
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

fn VectorInputs_f32x3_0( this_2 : ptr<function, VectorInputs_0>) -> vec3<f32>
{
    var x_0 : f32 = VectorInputs_f32_0(&((*this_2)));
    var y_0 : f32 = VectorInputs_f32_0(&((*this_2)));
    var z_0 : f32 = VectorInputs_f32_0(&((*this_2)));
    return vec3<f32>(x_0, y_0, z_0);
}

fn VectorInputs_i32_0( this_3 : ptr<function, VectorInputs_0>) -> i32
{
    var _S2 : u32 = VectorInputs_u32_0(&((*this_3)));
    return (bitcast<i32>((_S2)));
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
    var _S3 : vec3<f32> = vec3<f32>(0.0f);
    return clamp(select(radiance_0, _S3, (((bitcast<vec3<u32>>((radiance_0))) & (vec3<u32>(u32(2147483647))))) > vec3<u32>(u32(2139095040))), _S3, vec3<f32>(65000.0f));
}

fn bloom_bright_0( exposed_0 : vec3<f32>) -> vec3<f32>
{
    return hdr_safe_0(exposed_0 - vec3<f32>(1.0f));
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

fn bloom_tap_weight_0( offset_0 : i32) -> f32
{
    var distance_0 : i32 = abs(offset_0);
    var _S4 : f32;
    if(distance_0 <= i32(4))
    {
        _S4 = BLOOM_WEIGHTS_0[distance_0];
    }
    else
    {
        _S4 = 0.0f;
    }
    return _S4;
}

@compute
@workgroup_size(1, 1, 1)
fn bloom_filter_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S5 : u32 = tid_0.x;
    var inputs_1 : VectorInputs_0 = vector_inputs_0(_S5);
    var exposed_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var offset_1 : i32 = VectorInputs_i32_0(&(inputs_1));
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S5);
    VectorOutputs_put_3(&(outputs_1), bloom_bright_0(exposed_1));
    VectorOutputs_put_1(&(outputs_1), bloom_tap_weight_0(offset_1));
    var i_0 : i32 = i32(-5);
    var sum_0 : f32 = 0.0f;
    for(;;)
    {
        if(i_0 <= i32(5))
        {
        }
        else
        {
            break;
        }
        var sum_1 : f32 = sum_0 + bloom_tap_weight_0(i_0);
        i_0 = i_0 + i32(1);
        sum_0 = sum_1;
    }
    VectorOutputs_put_1(&(outputs_1), sum_0);
    return;
}

