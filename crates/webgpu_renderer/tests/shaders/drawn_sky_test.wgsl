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

fn aces_tonemap_inverse_0( display_0 : vec3<f32>) -> vec3<f32>
{
    var y_1 : vec3<f32> = clamp(display_0, vec3<f32>(0.0f), vec3<f32>(1.0f));
    var qa_0 : vec3<f32> = vec3<f32>(2.50999999046325684f) - vec3<f32>(2.43000006675720215f) * y_1;
    var qb_0 : vec3<f32> = vec3<f32>(0.02999999932944775f) - vec3<f32>(0.5899999737739563f) * y_1;
    return ((vec3<f32>(0) - qb_0) + sqrt(qb_0 * qb_0 + vec3<f32>(4.0f) * qa_0 * vec3<f32>(0.14000000059604645f) * y_1)) / (vec3<f32>(2.0f) * qa_0);
}

fn sky_gradient_0( dir_0 : vec3<f32>) -> vec3<f32>
{
    var _S3 : f32 = dir_0.y;
    if(_S3 >= 0.0f)
    {
        return mix(vec3<f32>(0.55000001192092896f, 0.62000000476837158f, 0.72000002861022949f), vec3<f32>(0.09000000357627869f, 0.15999999642372131f, 0.34999999403953552f), vec3<f32>(pow(clamp(_S3, 0.0f, 1.0f), 0.69999998807907104f)));
    }
    else
    {
        return mix(vec3<f32>(0.55000001192092896f, 0.62000000476837158f, 0.72000002861022949f), vec3<f32>(0.18000000715255737f, 0.15999999642372131f, 0.15000000596046448f), vec3<f32>(clamp(- _S3 * 3.0f, 0.0f, 1.0f)));
    }
}

fn sun_disk_0( dir_1 : vec3<f32>,  lightDir_0 : vec3<f32>,  intensity_0 : f32) -> vec3<f32>
{
    var _S4 : f32 = max(dot(dir_1, normalize(lightDir_0)), 0.0f);
    return vec3<f32>(1.0f, 0.94999998807907104f, 0.85000002384185791f) * vec3<f32>((pow(_S4, 1200.0f) * 24.0f + pow(_S4, 48.0f) * 0.5f)) * vec3<f32>((intensity_0 * 0.40000000596046448f));
}

fn hdr_safe_0( radiance_0 : vec3<f32>) -> vec3<f32>
{
    var _S5 : vec3<f32> = vec3<f32>(0.0f);
    return clamp(select(radiance_0, _S5, (((bitcast<vec3<u32>>((radiance_0))) & (vec3<u32>(u32(2147483647))))) > vec3<u32>(u32(2139095040))), _S5, vec3<f32>(65000.0f));
}

fn drawn_sky_0( mode_0 : f32,  cube_0 : vec3<f32>,  dir_2 : vec3<f32>,  light_dir_0 : vec3<f32>,  sun_intensity_0 : f32) -> vec3<f32>
{
    if(mode_0 > 1.5f)
    {
        return aces_tonemap_inverse_0(vec3<f32>(0.0f, 0.0f, 0.5f));
    }
    if(mode_0 > 0.5f)
    {
        return aces_tonemap_inverse_0(cube_0);
    }
    return hdr_safe_0(sky_gradient_0(dir_2) + sun_disk_0(dir_2, light_dir_0, sun_intensity_0));
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

@compute
@workgroup_size(1, 1, 1)
fn drawn_sky_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S6 : u32 = tid_0.x;
    var inputs_1 : VectorInputs_0 = vector_inputs_0(_S6);
    var mode_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var cube_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var dir_3 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var light_dir_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var sun_intensity_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S6);
    VectorOutputs_put_2(&(outputs_1), drawn_sky_0(mode_1, cube_1, dir_3, light_dir_1, sun_intensity_1));
    return;
}

