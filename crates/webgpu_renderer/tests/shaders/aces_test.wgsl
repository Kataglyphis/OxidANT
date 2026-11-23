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

fn VectorInputs_f32x3_0( this_2 : ptr<function, VectorInputs_0>) -> vec3<f32>
{
    var x_0 : f32 = VectorInputs_f32_0(&((*this_2)));
    var y_0 : f32 = VectorInputs_f32_0(&((*this_2)));
    var z_0 : f32 = VectorInputs_f32_0(&((*this_2)));
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

fn aces_tonemap_0( x_1 : vec3<f32>) -> vec3<f32>
{
    return clamp(x_1 * (vec3<f32>(2.50999999046325684f) * x_1 + vec3<f32>(0.02999999932944775f)) / (x_1 * (vec3<f32>(2.43000006675720215f) * x_1 + vec3<f32>(0.5899999737739563f)) + vec3<f32>(0.14000000059604645f)), vec3<f32>(0.0f), vec3<f32>(1.0f));
}

fn VectorOutputs_put_0( this_3 : ptr<function, VectorOutputs_0>,  word_1 : u32)
{
    vectorOutputs_0[(*this_3).at_1] = word_1;
    (*this_3).at_1 = (*this_3).at_1 + u32(1);
    return;
}

fn VectorOutputs_put_1( this_4 : ptr<function, VectorOutputs_0>,  value_0 : f32)
{
    VectorOutputs_put_0(&((*this_4)), (bitcast<u32>((value_0))));
    return;
}

fn VectorOutputs_put_2( this_5 : ptr<function, VectorOutputs_0>,  value_1 : vec3<f32>)
{
    VectorOutputs_put_1(&((*this_5)), value_1.x);
    VectorOutputs_put_1(&((*this_5)), value_1.y);
    VectorOutputs_put_1(&((*this_5)), value_1.z);
    return;
}

fn aces_tonemap_inverse_0( display_0 : vec3<f32>) -> vec3<f32>
{
    var y_1 : vec3<f32> = clamp(display_0, vec3<f32>(0.0f), vec3<f32>(1.0f));
    var qa_0 : vec3<f32> = vec3<f32>(2.50999999046325684f) - vec3<f32>(2.43000006675720215f) * y_1;
    var qb_0 : vec3<f32> = vec3<f32>(0.02999999932944775f) - vec3<f32>(0.5899999737739563f) * y_1;
    return ((vec3<f32>(0) - qb_0) + sqrt(qb_0 * qb_0 + vec3<f32>(4.0f) * qa_0 * vec3<f32>(0.14000000059604645f) * y_1)) / (vec3<f32>(2.0f) * qa_0);
}

fn linear_to_srgb_0( c_0 : vec3<f32>) -> vec3<f32>
{
    return select(vec3<f32>(1.0549999475479126f) * pow(max(c_0, vec3<f32>(0.0f)), vec3<f32>(0.4166666567325592f)) - vec3<f32>(0.05499999970197678f), c_0 * vec3<f32>(12.92000007629394531f), c_0 < vec3<f32>(0.00313080009073019f));
}

@compute
@workgroup_size(1, 1, 1)
fn aces_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S2 : u32 = tid_0.x;
    var inputs_1 : VectorInputs_0 = vector_inputs_0(_S2);
    var radiance_0 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var display_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var encoded_0 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S2);
    VectorOutputs_put_2(&(outputs_1), aces_tonemap_0(radiance_0));
    var _S3 : vec3<f32> = aces_tonemap_inverse_0(display_1);
    VectorOutputs_put_2(&(outputs_1), _S3);
    VectorOutputs_put_2(&(outputs_1), aces_tonemap_0(_S3));
    VectorOutputs_put_2(&(outputs_1), linear_to_srgb_0(encoded_0));
    return;
}

