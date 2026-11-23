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

fn VectorInputs_f32x2_0( this_3 : ptr<function, VectorInputs_0>) -> vec2<f32>
{
    var x_0 : f32 = VectorInputs_f32_0(&((*this_3)));
    var y_0 : f32 = VectorInputs_f32_0(&((*this_3)));
    return vec2<f32>(x_0, y_0);
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

struct FullscreenVsOut_0
{
    @location(0) svPosition_0 : vec4<f32>,
    @location(0) uv_0 : vec2<f32>,
};

fn fullscreen_vs_0( vid_0 : u32) -> FullscreenVsOut_0
{
    var x_1 : f32 = f32(vid_0 / u32(2)) * 4.0f - 1.0f;
    var y_1 : f32 = f32(vid_0 % u32(2)) * 4.0f - 1.0f;
    var o_0 : FullscreenVsOut_0;
    o_0.svPosition_0 = vec4<f32>(x_1, y_1, 0.0f, 1.0f);
    o_0.uv_0 = vec2<f32>((x_1 + 1.0f) * 0.5f, 1.0f - (y_1 + 1.0f) * 0.5f);
    return o_0;
}

fn VectorOutputs_put_0( this_4 : ptr<function, VectorOutputs_0>,  word_2 : u32)
{
    vectorOutputs_0[(*this_4).at_1] = word_2;
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

fn VectorOutputs_put_4( this_8 : ptr<function, VectorOutputs_0>,  value_3 : vec4<f32>)
{
    VectorOutputs_put_3(&((*this_8)), value_3.xyz);
    VectorOutputs_put_2(&((*this_8)), value_3.w);
    return;
}

fn fullscreen_uv_to_ndc_0( uv_1 : vec2<f32>) -> vec2<f32>
{
    return vec2<f32>(uv_1.x * 2.0f - 1.0f, 1.0f - uv_1.y * 2.0f);
}

fn VectorOutputs_put_5( this_9 : ptr<function, VectorOutputs_0>,  value_4 : vec2<f32>)
{
    VectorOutputs_put_2(&((*this_9)), value_4.x);
    VectorOutputs_put_2(&((*this_9)), value_4.y);
    return;
}

fn fullscreen_ndc_to_uv_0( ndc_0 : vec2<f32>) -> vec2<f32>
{
    return vec2<f32>(ndc_0.x * 0.5f + 0.5f, 0.5f - ndc_0.y * 0.5f);
}

fn top_row_clip_y_0() -> f32
{
    return 1.0f;
}

@compute
@workgroup_size(1, 1, 1)
fn fullscreen_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S2 : u32 = tid_0.x;
    var inputs_1 : VectorInputs_0 = vector_inputs_0(_S2);
    var vid_1 : u32 = VectorInputs_u32_0(&(inputs_1));
    var uv_2 : vec2<f32> = VectorInputs_f32x2_0(&(inputs_1));
    var ndc_1 : vec2<f32> = VectorInputs_f32x2_0(&(inputs_1));
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S2);
    var triangle_0 : FullscreenVsOut_0 = fullscreen_vs_0(vid_1);
    VectorOutputs_put_4(&(outputs_1), triangle_0.svPosition_0);
    VectorOutputs_put_5(&(outputs_1), fullscreen_uv_to_ndc_0(triangle_0.uv_0));
    VectorOutputs_put_5(&(outputs_1), fullscreen_ndc_to_uv_0(triangle_0.svPosition_0.xy) - triangle_0.uv_0);
    var _S3 : vec2<f32> = fullscreen_uv_to_ndc_0(uv_2);
    VectorOutputs_put_1(&(outputs_1), _S3.x);
    VectorOutputs_put_5(&(outputs_1), fullscreen_ndc_to_uv_0(_S3));
    var _S4 : vec2<f32> = fullscreen_ndc_to_uv_0(ndc_1);
    VectorOutputs_put_1(&(outputs_1), _S4.x);
    VectorOutputs_put_5(&(outputs_1), fullscreen_uv_to_ndc_0(_S4));
    var top_0 : f32 = top_row_clip_y_0();
    VectorOutputs_put_1(&(outputs_1), triangle_0.uv_0.y - 0.5f * (1.0f - triangle_0.svPosition_0.y * top_0));
    VectorOutputs_put_1(&(outputs_1), fullscreen_uv_to_ndc_0(vec2<f32>(uv_2.x, 0.0f)).y - top_0);
    VectorOutputs_put_1(&(outputs_1), fullscreen_ndc_to_uv_0(vec2<f32>(ndc_1.x, top_0)).y);
    return;
}

