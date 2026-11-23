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

fn VectorInputs_f32x4_0( this_3 : ptr<function, VectorInputs_0>) -> vec4<f32>
{
    var xyz_0 : vec3<f32> = VectorInputs_f32x3_0(&((*this_3)));
    var w_0 : f32 = VectorInputs_f32_0(&((*this_3)));
    return vec4<f32>(xyz_0, w_0);
}

fn VectorInputs_f32x2_0( this_4 : ptr<function, VectorInputs_0>) -> vec2<f32>
{
    var x_1 : f32 = VectorInputs_f32_0(&((*this_4)));
    var y_1 : f32 = VectorInputs_f32_0(&((*this_4)));
    return vec2<f32>(x_1, y_1);
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

struct CameraRay_0
{
     origin_0 : vec3<f32>,
     direction_0 : vec3<f32>,
     length_0 : f32,
};

fn camera_ray_0( clip_to_world_0 : mat4x4<f32>,  ndc_0 : vec2<f32>) -> CameraRay_0
{
    var near_0 : vec4<f32> = (((vec4<f32>(ndc_0, 0.0f, 1.0f)) * (clip_to_world_0)));
    var far_0 : vec4<f32> = (((vec4<f32>(ndc_0, 1.0f, 1.0f)) * (clip_to_world_0)));
    var ray_0 : CameraRay_0;
    var _S2 : vec3<f32> = near_0.xyz / vec3<f32>(near_0.w);
    ray_0.origin_0 = _S2;
    var span_0 : vec3<f32> = far_0.xyz / vec3<f32>(far_0.w) - _S2;
    ray_0.direction_0 = normalize(span_0);
    ray_0.length_0 = length(span_0);
    return ray_0;
}

fn VectorOutputs_put_0( this_5 : ptr<function, VectorOutputs_0>,  word_1 : u32)
{
    vectorOutputs_0[(*this_5).at_1] = word_1;
    (*this_5).at_1 = (*this_5).at_1 + u32(1);
    return;
}

fn VectorOutputs_put_1( this_6 : ptr<function, VectorOutputs_0>,  value_0 : f32)
{
    VectorOutputs_put_0(&((*this_6)), (bitcast<u32>((value_0))));
    return;
}

fn VectorOutputs_put_2( this_7 : ptr<function, VectorOutputs_0>,  value_1 : f32)
{
    VectorOutputs_put_0(&((*this_7)), (bitcast<u32>((value_1))));
    return;
}

fn VectorOutputs_put_3( this_8 : ptr<function, VectorOutputs_0>,  value_2 : vec3<f32>)
{
    VectorOutputs_put_2(&((*this_8)), value_2.x);
    VectorOutputs_put_2(&((*this_8)), value_2.y);
    VectorOutputs_put_2(&((*this_8)), value_2.z);
    return;
}

fn fullscreen_uv_to_ndc_0( uv_0 : vec2<f32>) -> vec2<f32>
{
    return vec2<f32>(uv_0.x * 2.0f - 1.0f, 1.0f - uv_0.y * 2.0f);
}

fn camera_ray_through_pixel_0( clip_to_world_1 : mat4x4<f32>,  pixel_0 : vec2<f32>,  size_0 : vec2<f32>) -> CameraRay_0
{
    return camera_ray_0(clip_to_world_1, fullscreen_uv_to_ndc_0(pixel_0 / size_0));
}

fn top_row_clip_y_0() -> f32
{
    return 1.0f;
}

@compute
@workgroup_size(1, 1, 1)
fn camera_ray_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S3 : u32 = tid_0.x;
    var inputs_1 : VectorInputs_0 = vector_inputs_0(_S3);
    var row0_0 : vec4<f32> = VectorInputs_f32x4_0(&(inputs_1));
    var row1_0 : vec4<f32> = VectorInputs_f32x4_0(&(inputs_1));
    var row2_0 : vec4<f32> = VectorInputs_f32x4_0(&(inputs_1));
    var row3_0 : vec4<f32> = VectorInputs_f32x4_0(&(inputs_1));
    var ndc_1 : vec2<f32> = VectorInputs_f32x2_0(&(inputs_1));
    var pixel_1 : vec2<f32> = VectorInputs_f32x2_0(&(inputs_1));
    var size_1 : vec2<f32> = VectorInputs_f32x2_0(&(inputs_1));
    var clip_to_world_2 : mat4x4<f32> = mat4x4<f32>(row0_0, row1_0, row2_0, row3_0);
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S3);
    var ray_1 : CameraRay_0 = camera_ray_0(clip_to_world_2, ndc_1);
    VectorOutputs_put_3(&(outputs_1), ray_1.origin_0);
    VectorOutputs_put_3(&(outputs_1), ray_1.direction_0);
    VectorOutputs_put_1(&(outputs_1), ray_1.length_0);
    var through_0 : CameraRay_0 = camera_ray_through_pixel_0(clip_to_world_2, pixel_1, size_1);
    var uv_1 : vec2<f32> = pixel_1 / size_1;
    var expected_0 : CameraRay_0 = camera_ray_0(clip_to_world_2, vec2<f32>(2.0f * uv_1.x - 1.0f, top_row_clip_y_0() * (1.0f - 2.0f * uv_1.y)));
    VectorOutputs_put_3(&(outputs_1), through_0.origin_0 - expected_0.origin_0);
    VectorOutputs_put_3(&(outputs_1), through_0.direction_0 - expected_0.direction_0);
    return;
}

