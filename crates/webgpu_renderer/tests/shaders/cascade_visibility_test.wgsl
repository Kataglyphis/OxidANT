@binding(0) @group(0) var<storage, read> vectorInputs_0 : array<u32>;

@binding(1) @group(0) var<storage, read_write> vectorOutputs_0 : array<u32>;

fn RampTaps_lit_0( uv_0 : vec2<f32>,  reference_0 : f32) -> f32
{
    return saturate(0.5f + 2.0f * (uv_0.x - 0.5f) - 4.0f * (reference_0 - 0.5f));
}

fn pcf_visibility_0( uv_1 : vec2<f32>,  texel_0 : vec2<f32>,  reference_1 : f32,  uv_depth_slope_0 : vec2<f32>,  radius_0 : i32) -> f32
{
    var _S1 : i32 = - radius_0;
    var y_0 : i32 = _S1;
    var sum_0 : f32 = 0.0f;
    for(;;)
    {
        if(y_0 <= radius_0)
        {
        }
        else
        {
            break;
        }
        var x_0 : i32 = _S1;
        for(;;)
        {
            if(x_0 <= radius_0)
            {
            }
            else
            {
                break;
            }
            var offset_0 : vec2<f32> = vec2<f32>(f32(x_0), f32(y_0)) * texel_0;
            var sum_1 : f32 = sum_0 + RampTaps_lit_0(uv_1 + offset_0, reference_1 + dot(uv_depth_slope_0, offset_0));
            x_0 = x_0 + i32(1);
            sum_0 = sum_1;
        }
        y_0 = y_0 + i32(1);
    }
    var _S2 : i32 = i32(2) * radius_0 + i32(1);
    return sum_0 / f32(_S2 * _S2);
}

struct CascadeHit_0
{
     uv_2 : vec2<f32>,
     depth_0 : f32,
     inside_0 : bool,
};

fn cascade_bias_0( nDotL_0 : f32,  biasScale_0 : f32) -> f32
{
    return max(0.0020000000949949f * (1.0f - nDotL_0), 0.00050000002374873f) * biasScale_0;
}

fn fullscreen_ndc_uv_y_sign_0() -> f32
{
    return -1.0f;
}

fn receiver_plane_depth_slope_0( light_space_0 : mat4x4<f32>,  n_0 : vec3<f32>,  uv_y_sign_0 : f32) -> vec2<f32>
{
    var ax_0 : vec3<f32> = light_space_0[i32(0)].xyz;
    var ay_0 : vec3<f32> = light_space_0[i32(1)].xyz;
    var az_0 : vec3<f32> = light_space_0[i32(2)].xyz;
    var light_axis_0 : vec3<f32> = cross(ax_0, ay_0);
    var mx_0 : f32 = dot(n_0, cross(ay_0, az_0));
    var my_0 : f32 = dot(n_0, cross(az_0, ax_0));
    var mz_0 : f32 = dot(n_0, light_axis_0);
    var floor_z_0 : f32 = 0.10000000149011612f * length(light_axis_0);
    var denom_0 : f32;
    if(mz_0 >= 0.0f)
    {
        denom_0 = max(mz_0, floor_z_0);
    }
    else
    {
        denom_0 = min(mz_0, - floor_z_0);
    }
    return vec2<f32>(-2.0f) * vec2<f32>(mx_0, my_0 * uv_y_sign_0) / vec2<f32>(denom_0);
}

fn cascade_depth_slope_0( lightSpace_0 : mat4x4<f32>,  n_1 : vec3<f32>) -> vec2<f32>
{
    return receiver_plane_depth_slope_0(lightSpace_0, n_1, fullscreen_ndc_uv_y_sign_0());
}

fn cascade_visibility_0( lightSpace_1 : mat4x4<f32>,  hit_0 : CascadeHit_0,  n_2 : vec3<f32>,  nDotL_1 : f32,  biasScale_1 : f32,  texel_1 : vec2<f32>,  radius_1 : i32) -> f32
{
    return pcf_visibility_0(hit_0.uv_2, texel_1, hit_0.depth_0 - cascade_bias_0(nDotL_1, biasScale_1), cascade_depth_slope_0(lightSpace_1, n_2), radius_1);
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

fn VectorInputs_u32_0( this_0 : ptr<function, VectorInputs_0>) -> u32
{
    var word_0 : u32 = vectorInputs_0[(*this_0).at_0];
    (*this_0).at_0 = (*this_0).at_0 + u32(1);
    return word_0;
}

fn VectorInputs_f32_0( this_1 : ptr<function, VectorInputs_0>) -> f32
{
    var _S3 : u32 = VectorInputs_u32_0(&((*this_1)));
    return (bitcast<f32>((_S3)));
}

fn VectorInputs_f32_1( this_2 : ptr<function, VectorInputs_0>) -> f32
{
    var _S4 : u32 = VectorInputs_u32_0(&((*this_2)));
    return (bitcast<f32>((_S4)));
}

fn VectorInputs_f32x3_0( this_3 : ptr<function, VectorInputs_0>) -> vec3<f32>
{
    var x_1 : f32 = VectorInputs_f32_1(&((*this_3)));
    var y_1 : f32 = VectorInputs_f32_1(&((*this_3)));
    var z_0 : f32 = VectorInputs_f32_1(&((*this_3)));
    return vec3<f32>(x_1, y_1, z_0);
}

fn VectorInputs_f32x3_1( this_4 : ptr<function, VectorInputs_0>) -> vec3<f32>
{
    var x_2 : f32 = VectorInputs_f32_1(&((*this_4)));
    var y_2 : f32 = VectorInputs_f32_1(&((*this_4)));
    var z_1 : f32 = VectorInputs_f32_1(&((*this_4)));
    return vec3<f32>(x_2, y_2, z_1);
}

fn VectorInputs_f32x4_0( this_5 : ptr<function, VectorInputs_0>) -> vec4<f32>
{
    var xyz_0 : vec3<f32> = VectorInputs_f32x3_1(&((*this_5)));
    var w_0 : f32 = VectorInputs_f32_1(&((*this_5)));
    return vec4<f32>(xyz_0, w_0);
}

fn VectorInputs_f32x2_0( this_6 : ptr<function, VectorInputs_0>) -> vec2<f32>
{
    var x_3 : f32 = VectorInputs_f32_1(&((*this_6)));
    var y_3 : f32 = VectorInputs_f32_1(&((*this_6)));
    return vec2<f32>(x_3, y_3);
}

fn VectorInputs_i32_0( this_7 : ptr<function, VectorInputs_0>) -> i32
{
    var _S5 : u32 = VectorInputs_u32_0(&((*this_7)));
    return (bitcast<i32>((_S5)));
}

fn fullscreen_ndc_to_uv_0( ndc_0 : vec2<f32>) -> vec2<f32>
{
    return vec2<f32>(ndc_0.x * 0.5f + 0.5f, 0.5f - ndc_0.y * 0.5f);
}

fn cascade_hit_0( lightSpace_2 : mat4x4<f32>,  worldPos_0 : vec3<f32>) -> CascadeHit_0
{
    var lightSpacePos_0 : vec4<f32> = (((vec4<f32>(worldPos_0, 1.0f)) * (lightSpace_2)));
    var proj_0 : vec3<f32> = lightSpacePos_0.xyz / vec3<f32>(lightSpacePos_0.w);
    var hit_1 : CascadeHit_0;
    var _S6 : vec2<f32> = fullscreen_ndc_to_uv_0(proj_0.xy);
    hit_1.uv_2 = _S6;
    var _S7 : f32 = proj_0.z;
    hit_1.depth_0 = _S7;
    var _S8 : bool;
    if((_S6.x) >= 0.0f)
    {
        _S8 = (hit_1.uv_2.x) <= 1.0f;
    }
    else
    {
        _S8 = false;
    }
    if(_S8)
    {
        _S8 = (hit_1.uv_2.y) >= 0.0f;
    }
    else
    {
        _S8 = false;
    }
    if(_S8)
    {
        _S8 = (hit_1.uv_2.y) <= 1.0f;
    }
    else
    {
        _S8 = false;
    }
    if(_S8)
    {
        _S8 = _S7 >= 0.0f;
    }
    else
    {
        _S8 = false;
    }
    if(_S8)
    {
        _S8 = _S7 <= 1.0f;
    }
    else
    {
        _S8 = false;
    }
    hit_1.inside_0 = _S8;
    return hit_1;
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

fn select_cascade_0( viewDepth_0 : f32,  splits_0 : vec2<f32>,  count_0 : i32) -> i32
{
    var selected_0 : i32;
    if(viewDepth_0 >= (splits_0.x))
    {
        selected_0 = i32(1);
    }
    else
    {
        selected_0 = i32(0);
    }
    if(viewDepth_0 >= (splits_0.y))
    {
        selected_0 = i32(2);
    }
    return min(selected_0, count_0 - i32(1));
}

fn VectorOutputs_put_0( this_8 : ptr<function, VectorOutputs_0>,  word_1 : u32)
{
    vectorOutputs_0[(*this_8).at_1] = word_1;
    (*this_8).at_1 = (*this_8).at_1 + u32(1);
    return;
}

fn VectorOutputs_put_1( this_9 : ptr<function, VectorOutputs_0>,  word_2 : u32)
{
    vectorOutputs_0[(*this_9).at_1] = word_2;
    (*this_9).at_1 = (*this_9).at_1 + u32(1);
    return;
}

fn fullscreen_uv_to_ndc_0( uv_3 : vec2<f32>) -> vec2<f32>
{
    return vec2<f32>(uv_3.x * 2.0f - 1.0f, 1.0f - uv_3.y * 2.0f);
}

fn VectorOutputs_put_2( this_10 : ptr<function, VectorOutputs_0>,  value_0 : f32)
{
    VectorOutputs_put_1(&((*this_10)), (bitcast<u32>((value_0))));
    return;
}

fn VectorOutputs_put_3( this_11 : ptr<function, VectorOutputs_0>,  value_1 : f32)
{
    VectorOutputs_put_1(&((*this_11)), (bitcast<u32>((value_1))));
    return;
}

fn VectorOutputs_put_4( this_12 : ptr<function, VectorOutputs_0>,  value_2 : vec2<f32>)
{
    VectorOutputs_put_3(&((*this_12)), value_2.x);
    VectorOutputs_put_3(&((*this_12)), value_2.y);
    return;
}

@compute
@workgroup_size(1, 1, 1)
fn cascade_visibility_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S9 : u32 = tid_0.x;
    var inputs_1 : VectorInputs_0 = vector_inputs_0(_S9);
    var row0_0 : vec4<f32> = VectorInputs_f32x4_0(&(inputs_1));
    var row1_0 : vec4<f32> = VectorInputs_f32x4_0(&(inputs_1));
    var row2_0 : vec4<f32> = VectorInputs_f32x4_0(&(inputs_1));
    var row3_0 : vec4<f32> = VectorInputs_f32x4_0(&(inputs_1));
    var world_0 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var viewDepth_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var splits_1 : vec2<f32> = VectorInputs_f32x2_0(&(inputs_1));
    var count_1 : i32 = VectorInputs_i32_0(&(inputs_1));
    var n_3 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var nDotL_2 : f32 = VectorInputs_f32_0(&(inputs_1));
    var biasScale_2 : f32 = VectorInputs_f32_0(&(inputs_1));
    var texel_2 : vec2<f32> = VectorInputs_f32x2_0(&(inputs_1));
    var radius_2 : i32 = VectorInputs_i32_0(&(inputs_1));
    var lightSpace_3 : mat4x4<f32> = mat4x4<f32>(row0_0, row1_0, row2_0, row3_0);
    var hit_2 : CascadeHit_0 = cascade_hit_0(lightSpace_3, world_0);
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S9);
    VectorOutputs_put_0(&(outputs_1), u32(select_cascade_0(viewDepth_1, splits_1, count_1)));
    VectorOutputs_put_4(&(outputs_1), fullscreen_uv_to_ndc_0(hit_2.uv_2));
    VectorOutputs_put_2(&(outputs_1), hit_2.depth_0);
    var _S10 : u32;
    if(hit_2.inside_0)
    {
        _S10 = u32(1);
    }
    else
    {
        _S10 = u32(0);
    }
    VectorOutputs_put_0(&(outputs_1), _S10);
    VectorOutputs_put_2(&(outputs_1), cascade_bias_0(nDotL_2, biasScale_2));
    VectorOutputs_put_4(&(outputs_1), cascade_depth_slope_0(lightSpace_3, n_3) * vec2<f32>(1.0f, fullscreen_ndc_uv_y_sign_0()));
    VectorOutputs_put_2(&(outputs_1), cascade_visibility_0(lightSpace_3, hit_2, n_3, nDotL_2, biasScale_2, texel_2, radius_2));
    return;
}

