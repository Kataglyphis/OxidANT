@binding(0) @group(0) var<storage, read> vectorInputs_0 : array<u32>;

@binding(1) @group(0) var<storage, read_write> vectorOutputs_0 : array<u32>;

struct SpotTexels_0
{
     spot_0 : vec2<i32>,
     spotDepth_0 : f32,
     restDepth_0 : f32,
     scale_0 : f32,
};

fn SpotTexels_scene_depth_0( this_0 : SpotTexels_0,  pixel_0 : vec2<i32>) -> f32
{
    var _S1 : f32;
    if((all((pixel_0 == (this_0.spot_0)))))
    {
        _S1 = this_0.spotDepth_0;
    }
    else
    {
        _S1 = this_0.restDepth_0;
    }
    return _S1;
}

fn SpotTexels_scene_distance_0( this_1 : SpotTexels_0,  depth_0 : f32) -> f32
{
    return depth_0 * this_1.scale_0;
}

fn SpotTexels_half_cloud_0( this_2 : SpotTexels_0,  texel_0 : vec2<i32>) -> vec4<f32>
{
    var _S2 : i32 = texel_0.x;
    var _S3 : i32 = texel_0.y;
    return vec4<f32>(f32(_S2) * 0.10000000149011612f, f32(_S3) * 0.10000000149011612f, f32(_S2 + _S3) * 0.05000000074505806f, 1.0f);
}

fn half_texel_depth_0( texels_0 : SpotTexels_0,  texel_1 : vec2<i32>,  depthSize_0 : vec2<i32>) -> f32
{
    var _S4 : vec2<i32> = depthSize_0 - vec2<i32>(i32(1));
    var nearest_0 : f32 = 1.0f;
    var i_0 : i32 = i32(0);
    for(;;)
    {
        if(i_0 < i32(4))
        {
        }
        else
        {
            break;
        }
        var _S5 : f32 = min(nearest_0, SpotTexels_scene_depth_0(texels_0, min(texel_1 * vec2<i32>(i32(2)) + vec2<i32>((i_0 & (i32(1))), (i_0 >> (u32(1)))), _S4)));
        var _S6 : i32 = i_0 + i32(1);
        nearest_0 = _S5;
        i_0 = _S6;
    }
    return nearest_0;
}

fn cloud_upsample_depth_weight_0( pixelDistance_0 : f32,  texelDistance_0 : f32) -> f32
{
    return exp(- min(abs(pixelDistance_0 - texelDistance_0) / max(min(pixelDistance_0, texelDistance_0), 0.00009999999747379f), 100.0f) * 16.0f);
}

fn cloud_upsample_0( texels_1 : SpotTexels_0,  pixel_1 : vec2<i32>,  halfSize_0 : vec2<i32>,  depthSize_1 : vec2<i32>) -> vec4<f32>
{
    var _S7 : f32 = SpotTexels_scene_distance_0(texels_1, SpotTexels_scene_depth_0(texels_1, pixel_1));
    var _S8 : vec2<i32> = halfSize_0 - vec2<i32>(i32(1));
    var _S9 : vec2<f32> = vec2<f32>(0.5f);
    var position_0 : vec2<f32> = (vec2<f32>(pixel_1) + _S9) * _S9 - _S9;
    var _S10 : vec2<i32> = vec2<i32>(floor(position_0));
    var _S11 : vec2<f32> = position_0 - vec2<f32>(_S10);
    var _S12 : vec4<f32> = vec4<f32>(0.0f);
    var i_1 : i32 = i32(0);
    var weighted_0 : vec4<f32> = _S12;
    var weightSum_0 : f32 = 0.0f;
    var bilinear_0 : vec4<f32> = _S12;
    for(;;)
    {
        if(i_1 < i32(4))
        {
        }
        else
        {
            break;
        }
        var corner_0 : vec2<i32> = vec2<i32>((i_1 & (i32(1))), (i_1 >> (u32(1))));
        var texel_2 : vec2<i32> = clamp(_S10 + corner_0, vec2<i32>(i32(0)), _S8);
        var axis_0 : vec2<f32> = mix(vec2<f32>(1.0f) - _S11, _S11, vec2<f32>(corner_0));
        var w_0 : f32 = axis_0.x * axis_0.y;
        var value_0 : vec4<f32> = SpotTexels_half_cloud_0(texels_1, texel_2);
        var depthWeight_0 : f32 = w_0 * cloud_upsample_depth_weight_0(_S7, SpotTexels_scene_distance_0(texels_1, half_texel_depth_0(texels_1, texel_2, depthSize_1)));
        var weighted_1 : vec4<f32> = weighted_0 + value_0 * vec4<f32>(depthWeight_0);
        var weightSum_1 : f32 = weightSum_0 + depthWeight_0;
        var bilinear_1 : vec4<f32> = bilinear_0 + value_0 * vec4<f32>(w_0);
        i_1 = i_1 + i32(1);
        weighted_0 = weighted_1;
        weightSum_0 = weightSum_1;
        bilinear_0 = bilinear_1;
    }
    if(weightSum_0 > 0.00009999999747379f)
    {
        weighted_0 = weighted_0 / vec4<f32>(weightSum_0);
    }
    else
    {
        weighted_0 = bilinear_0;
    }
    return weighted_0;
}

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

fn VectorInputs_u32_0( this_3 : ptr<function, VectorInputs_0>) -> u32
{
    var word_0 : u32 = vectorInputs_0[(*this_3).at_0];
    (*this_3).at_0 = (*this_3).at_0 + u32(1);
    return word_0;
}

fn VectorInputs_i32_0( this_4 : ptr<function, VectorInputs_0>) -> i32
{
    var _S13 : u32 = VectorInputs_u32_0(&((*this_4)));
    return (bitcast<i32>((_S13)));
}

fn i32x2_0( inputs_1 : ptr<function, VectorInputs_0>) -> vec2<i32>
{
    var x_0 : i32 = VectorInputs_i32_0(&((*inputs_1)));
    var y_0 : i32 = VectorInputs_i32_0(&((*inputs_1)));
    return vec2<i32>(x_0, y_0);
}

fn VectorInputs_f32_0( this_5 : ptr<function, VectorInputs_0>) -> f32
{
    var _S14 : u32 = VectorInputs_u32_0(&((*this_5)));
    return (bitcast<f32>((_S14)));
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

fn VectorOutputs_put_0( this_6 : ptr<function, VectorOutputs_0>,  word_1 : u32)
{
    vectorOutputs_0[(*this_6).at_1] = word_1;
    (*this_6).at_1 = (*this_6).at_1 + u32(1);
    return;
}

fn VectorOutputs_put_1( this_7 : ptr<function, VectorOutputs_0>,  value_1 : f32)
{
    VectorOutputs_put_0(&((*this_7)), (bitcast<u32>((value_1))));
    return;
}

fn VectorOutputs_put_2( this_8 : ptr<function, VectorOutputs_0>,  value_2 : f32)
{
    VectorOutputs_put_0(&((*this_8)), (bitcast<u32>((value_2))));
    return;
}

fn VectorOutputs_put_3( this_9 : ptr<function, VectorOutputs_0>,  value_3 : vec3<f32>)
{
    VectorOutputs_put_2(&((*this_9)), value_3.x);
    VectorOutputs_put_2(&((*this_9)), value_3.y);
    VectorOutputs_put_2(&((*this_9)), value_3.z);
    return;
}

fn VectorOutputs_put_4( this_10 : ptr<function, VectorOutputs_0>,  value_4 : vec4<f32>)
{
    VectorOutputs_put_3(&((*this_10)), value_4.xyz);
    VectorOutputs_put_2(&((*this_10)), value_4.w);
    return;
}

@compute
@workgroup_size(1, 1, 1)
fn cloud_upsample_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S15 : u32 = tid_0.x;
    var inputs_2 : VectorInputs_0 = vector_inputs_0(_S15);
    var pixel_2 : vec2<i32> = i32x2_0(&(inputs_2));
    var texel_3 : vec2<i32> = i32x2_0(&(inputs_2));
    var halfSize_1 : vec2<i32> = i32x2_0(&(inputs_2));
    var depthSize_2 : vec2<i32> = i32x2_0(&(inputs_2));
    var texels_2 : SpotTexels_0;
    var _S16 : vec2<i32> = i32x2_0(&(inputs_2));
    texels_2.spot_0 = _S16;
    var _S17 : f32 = VectorInputs_f32_0(&(inputs_2));
    texels_2.spotDepth_0 = _S17;
    var _S18 : f32 = VectorInputs_f32_0(&(inputs_2));
    texels_2.restDepth_0 = _S18;
    var _S19 : f32 = VectorInputs_f32_0(&(inputs_2));
    texels_2.scale_0 = _S19;
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S15);
    VectorOutputs_put_1(&(outputs_1), half_texel_depth_0(texels_2, texel_3, depthSize_2));
    VectorOutputs_put_4(&(outputs_1), cloud_upsample_0(texels_2, pixel_2, halfSize_1, depthSize_2));
    return;
}

