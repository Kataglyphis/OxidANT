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

fn exposure_luminance_0( color_0 : vec3<f32>) -> f32
{
    return dot(color_0, vec3<f32>(0.2125999927520752f, 0.71520000696182251f, 0.07220000028610229f));
}

fn VectorOutputs_put_0( this_5 : ptr<function, VectorOutputs_0>,  word_2 : u32)
{
    vectorOutputs_0[(*this_5).at_1] = word_2;
    (*this_5).at_1 = (*this_5).at_1 + u32(1);
    return;
}

fn VectorOutputs_put_1( this_6 : ptr<function, VectorOutputs_0>,  word_3 : u32)
{
    vectorOutputs_0[(*this_6).at_1] = word_3;
    (*this_6).at_1 = (*this_6).at_1 + u32(1);
    return;
}

fn VectorOutputs_put_2( this_7 : ptr<function, VectorOutputs_0>,  value_0 : f32)
{
    VectorOutputs_put_1(&((*this_7)), (bitcast<u32>((value_0))));
    return;
}

fn histogram_bin_0( lum_0 : f32) -> u32
{
    if(!(lum_0 > 9.99999997475242708e-07f))
    {
        return u32(0);
    }
    return u32(1) + min(u32(clamp((log2(lum_0) - -10.0f) / 14.0f, 0.0f, 1.0f) * 62.0f), u32(62));
}

fn bin_luminance_0( bin_0 : u32) -> f32
{
    if(bin_0 == u32(0))
    {
        return 0.0f;
    }
    return exp2(-10.0f + (f32(bin_0 - u32(1)) + 0.5f) / 62.0f * 14.0f);
}

fn exposure_target_ev_0( log2Mean_0 : f32) -> f32
{
    return log2(0.18000000715255737f / exp2(log2Mean_0));
}

fn is_finite_0( x_1 : f32) -> bool
{
    return (((bitcast<u32>((x_1))) & (u32(2139095040)))) != u32(2139095040);
}

fn adapt_exposure_ev_0( currentEv_0 : f32,  targetEv_0 : f32,  dt_0 : f32,  speed_0 : f32) -> f32
{
    var _S3 : bool;
    if(is_finite_0(currentEv_0))
    {
        _S3 = (abs(currentEv_0)) < 1.00000001504746622e+30f;
    }
    else
    {
        _S3 = false;
    }
    var current_0 : f32;
    if(_S3)
    {
        current_0 = currentEv_0;
    }
    else
    {
        current_0 = targetEv_0;
    }
    if(dt_0 <= 0.0f)
    {
        return current_0;
    }
    if(speed_0 <= 0.0f)
    {
        return targetEv_0;
    }
    return current_0 + (targetEv_0 - current_0) * clamp(1.0f - exp(- dt_0 * speed_0), 0.0f, 1.0f);
}

fn exposure_scale_0( ev_0 : f32) -> f32
{
    return exp2(ev_0);
}

@compute
@workgroup_size(1, 1, 1)
fn exposure_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S4 : u32 = tid_0.x;
    var inputs_1 : VectorInputs_0 = vector_inputs_0(_S4);
    var color_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var lum_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var bin_1 : u32 = VectorInputs_u32_0(&(inputs_1));
    var log2Mean_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var currentEv_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var targetEv_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var dt_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var speed_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var ev_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S4);
    VectorOutputs_put_2(&(outputs_1), exposure_luminance_0(color_1));
    VectorOutputs_put_0(&(outputs_1), histogram_bin_0(lum_1));
    VectorOutputs_put_2(&(outputs_1), bin_luminance_0(bin_1));
    VectorOutputs_put_2(&(outputs_1), exposure_target_ev_0(log2Mean_1));
    VectorOutputs_put_2(&(outputs_1), adapt_exposure_ev_0(currentEv_1, targetEv_1, dt_1, speed_1));
    VectorOutputs_put_2(&(outputs_1), exposure_scale_0(ev_1));
    return;
}

