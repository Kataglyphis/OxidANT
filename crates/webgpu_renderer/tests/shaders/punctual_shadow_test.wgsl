@binding(0) @group(0) var<storage, read> vectorInputs_0 : array<u32>;

@binding(1) @group(0) var<storage, read_write> vectorOutputs_0 : array<u32>;

struct PunctualShadow_0
{
     slot_0 : i32,
     nearPlane_0 : f32,
     farPlane_0 : f32,
};

fn punctual_shadow_depth_0( majorAxis_0 : f32,  nearPlane_1 : f32,  farPlane_1 : f32) -> f32
{
    return farPlane_1 * (majorAxis_0 - nearPlane_1) / ((farPlane_1 - nearPlane_1) * majorAxis_0);
}

fn RampTaps_lit_0( d_0 : vec3<f32>,  reference_0 : f32) -> f32
{
    return saturate(0.5f + 0.10000000149011612f * (d_0.x + 0.5f * d_0.y - 0.25f * d_0.z) - 4.0f * (reference_0 - 0.89999997615814209f));
}

fn punctual_shadow_visibility_0( shadow_0 : PunctualShadow_0,  lightPos_0 : vec3<f32>,  worldPos_0 : vec3<f32>,  n_0 : vec3<f32>,  l_0 : vec3<f32>,  width_0 : f32) -> f32
{
    var texel_0 : f32 = 2.0f * length(worldPos_0 - lightPos_0) / width_0;
    var d_1 : vec3<f32> = worldPos_0 + n_0 * vec3<f32>((1.5f * texel_0)) + l_0 * vec3<f32>(texel_0) - lightPos_0;
    var _S1 : f32 = max(abs(d_1.x), max(abs(d_1.y), abs(d_1.z)));
    if(_S1 >= (shadow_0.farPlane_0))
    {
        return 1.0f;
    }
    var reference_1 : f32 = punctual_shadow_depth_0(max(_S1, shadow_0.nearPlane_0), shadow_0.nearPlane_0, shadow_0.farPlane_0);
    var axis_0 : vec3<f32> = normalize(d_1);
    var _S2 : vec3<f32>;
    if((abs(axis_0.y)) < 0.99000000953674316f)
    {
        _S2 = vec3<f32>(0.0f, 1.0f, 0.0f);
    }
    else
    {
        _S2 = vec3<f32>(1.0f, 0.0f, 0.0f);
    }
    var tangent_0 : vec3<f32> = normalize(cross(_S2, axis_0));
    var _S3 : vec3<f32> = vec3<f32>((3.0f * _S1 / width_0));
    var _S4 : vec3<f32> = tangent_0 * _S3;
    var _S5 : vec3<f32> = cross(axis_0, tangent_0) * _S3;
    return (RampTaps_lit_0(d_1, reference_1) + RampTaps_lit_0(d_1 + _S4, reference_1) + RampTaps_lit_0(d_1 - _S4, reference_1) + RampTaps_lit_0(d_1 + _S5, reference_1) + RampTaps_lit_0(d_1 - _S5, reference_1)) * 0.20000000298023224f;
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
    var _S6 : u32 = VectorInputs_u32_0(&((*this_1)));
    return (bitcast<f32>((_S6)));
}

fn VectorInputs_f32_1( this_2 : ptr<function, VectorInputs_0>) -> f32
{
    var _S7 : u32 = VectorInputs_u32_0(&((*this_2)));
    return (bitcast<f32>((_S7)));
}

fn VectorInputs_f32x3_0( this_3 : ptr<function, VectorInputs_0>) -> vec3<f32>
{
    var x_0 : f32 = VectorInputs_f32_1(&((*this_3)));
    var y_0 : f32 = VectorInputs_f32_1(&((*this_3)));
    var z_0 : f32 = VectorInputs_f32_1(&((*this_3)));
    return vec3<f32>(x_0, y_0, z_0);
}

fn VectorInputs_f32x3_1( this_4 : ptr<function, VectorInputs_0>) -> vec3<f32>
{
    var x_1 : f32 = VectorInputs_f32_1(&((*this_4)));
    var y_1 : f32 = VectorInputs_f32_1(&((*this_4)));
    var z_1 : f32 = VectorInputs_f32_1(&((*this_4)));
    return vec3<f32>(x_1, y_1, z_1);
}

fn VectorInputs_f32x4_0( this_5 : ptr<function, VectorInputs_0>) -> vec4<f32>
{
    var xyz_0 : vec3<f32> = VectorInputs_f32x3_1(&((*this_5)));
    var w_0 : f32 = VectorInputs_f32_1(&((*this_5)));
    return vec4<f32>(xyz_0, w_0);
}

fn unpack_punctual_shadow_0( d_2 : vec4<f32>) -> PunctualShadow_0
{
    var shadow_1 : PunctualShadow_0;
    shadow_1.slot_0 = i32(d_2.y + 0.5f) - i32(1);
    shadow_1.nearPlane_0 = d_2.z;
    shadow_1.farPlane_0 = d_2.w;
    return shadow_1;
}

struct PunctualShadowFace_0
{
     face_0 : u32,
     ndc_0 : vec2<f32>,
};

fn punctual_shadow_face_0( r_0 : vec3<f32>) -> PunctualShadowFace_0
{
    var a_0 : vec3<f32> = abs(r_0);
    var o_0 : PunctualShadowFace_0;
    var _S8 : f32 = a_0.x;
    var _S9 : f32 = a_0.y;
    var _S10 : bool;
    if(_S8 >= _S9)
    {
        _S10 = _S8 >= (a_0.z);
    }
    else
    {
        _S10 = false;
    }
    var _S11 : u32;
    var ma_0 : f32;
    if(_S10)
    {
        var _S12 : bool = (r_0.x) > 0.0f;
        if(_S12)
        {
            _S11 = u32(0);
        }
        else
        {
            _S11 = u32(1);
        }
        o_0.face_0 = _S11;
        if(_S12)
        {
            ma_0 = - r_0.z;
        }
        else
        {
            ma_0 = r_0.z;
        }
        o_0.ndc_0 = vec2<f32>(ma_0, - r_0.y);
        ma_0 = _S8;
    }
    else
    {
        var _S13 : f32 = a_0.z;
        if(_S9 >= _S13)
        {
            var _S14 : bool = (r_0.y) > 0.0f;
            if(_S14)
            {
                _S11 = u32(2);
            }
            else
            {
                _S11 = u32(3);
            }
            o_0.face_0 = _S11;
            var _S15 : f32 = r_0.x;
            if(_S14)
            {
                ma_0 = r_0.z;
            }
            else
            {
                ma_0 = - r_0.z;
            }
            o_0.ndc_0 = vec2<f32>(_S15, ma_0);
            ma_0 = _S9;
        }
        else
        {
            var _S16 : bool = (r_0.z) > 0.0f;
            if(_S16)
            {
                _S11 = u32(4);
            }
            else
            {
                _S11 = u32(5);
            }
            o_0.face_0 = _S11;
            if(_S16)
            {
                ma_0 = r_0.x;
            }
            else
            {
                ma_0 = - r_0.x;
            }
            o_0.ndc_0 = vec2<f32>(ma_0, - r_0.y);
            ma_0 = _S13;
        }
    }
    o_0.ndc_0 = o_0.ndc_0 / vec2<f32>(max(ma_0, 9.99999968265522539e-21f));
    return o_0;
}

fn fullscreen_ndc_to_uv_0( ndc_1 : vec2<f32>) -> vec2<f32>
{
    return vec2<f32>(ndc_1.x * 0.5f + 0.5f, 0.5f - ndc_1.y * 0.5f);
}

fn punctual_shadow_face_coord_0( d_3 : vec3<f32>,  slot_1 : i32) -> vec3<f32>
{
    var f_0 : PunctualShadowFace_0 = punctual_shadow_face_0(d_3);
    return vec3<f32>(fullscreen_ndc_to_uv_0(f_0.ndc_0), f32(u32(slot_1) * u32(6) + f_0.face_0));
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

fn VectorOutputs_put_1( this_7 : ptr<function, VectorOutputs_0>,  word_2 : u32)
{
    vectorOutputs_0[(*this_7).at_1] = word_2;
    (*this_7).at_1 = (*this_7).at_1 + u32(1);
    return;
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

fn VectorOutputs_put_4( this_10 : ptr<function, VectorOutputs_0>,  value_2 : vec2<f32>)
{
    VectorOutputs_put_3(&((*this_10)), value_2.x);
    VectorOutputs_put_3(&((*this_10)), value_2.y);
    return;
}

fn fullscreen_uv_to_ndc_0( uv_0 : vec2<f32>) -> vec2<f32>
{
    return vec2<f32>(uv_0.x * 2.0f - 1.0f, 1.0f - uv_0.y * 2.0f);
}

@compute
@workgroup_size(1, 1, 1)
fn punctual_shadow_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S17 : u32 = tid_0.x;
    var inputs_1 : VectorInputs_0 = vector_inputs_0(_S17);
    var row_0 : vec4<f32> = VectorInputs_f32x4_0(&(inputs_1));
    var majorAxis_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var r_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var lightPos_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var worldPos_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var n_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var l_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var width_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var shadow_2 : PunctualShadow_0 = unpack_punctual_shadow_0(row_0);
    var face_1 : PunctualShadowFace_0 = punctual_shadow_face_0(r_1);
    var coord_0 : vec3<f32> = punctual_shadow_face_coord_0(r_1, max(shadow_2.slot_0, i32(0)));
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S17);
    VectorOutputs_put_0(&(outputs_1), u32(shadow_2.slot_0 + i32(1)));
    VectorOutputs_put_4(&(outputs_1), vec2<f32>(shadow_2.nearPlane_0, shadow_2.farPlane_0));
    VectorOutputs_put_2(&(outputs_1), punctual_shadow_depth_0(majorAxis_1, shadow_2.nearPlane_0, shadow_2.farPlane_0));
    VectorOutputs_put_0(&(outputs_1), face_1.face_0);
    VectorOutputs_put_4(&(outputs_1), face_1.ndc_0);
    VectorOutputs_put_4(&(outputs_1), fullscreen_uv_to_ndc_0(coord_0.xy));
    VectorOutputs_put_2(&(outputs_1), coord_0.z);
    VectorOutputs_put_2(&(outputs_1), punctual_shadow_visibility_0(shadow_2, lightPos_1, worldPos_1, n_1, l_1, width_1));
    return;
}

