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

fn VectorInputs_f32x2_0( this_4 : ptr<function, VectorInputs_0>) -> vec2<f32>
{
    var x_1 : f32 = VectorInputs_f32_1(&((*this_4)));
    var y_1 : f32 = VectorInputs_f32_1(&((*this_4)));
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

fn multiscatter_gain_0( energy_0 : vec2<f32>,  f0_0 : vec3<f32>) -> vec3<f32>
{
    return vec3<f32>(1.0f) + f0_0 * vec3<f32>((1.0f / max(energy_0.x + energy_0.y, 0.00100000004749745f) - 1.0f));
}

fn specular_albedo_0( energy_1 : vec2<f32>,  f0_1 : vec3<f32>) -> vec3<f32>
{
    return multiscatter_gain_0(energy_1, f0_1) * (f0_1 * vec3<f32>(energy_1.x) + vec3<f32>(energy_1.y));
}

fn ambient_split_sum_0( irradiance_0 : vec3<f32>,  reflection_0 : vec3<f32>,  albedo_0 : vec3<f32>,  metallic_0 : f32,  f0_2 : vec3<f32>,  energy_2 : vec2<f32>) -> vec3<f32>
{
    var specularAlbedo_0 : vec3<f32> = specular_albedo_0(energy_2, f0_2);
    return saturate(vec3<f32>(1.0f) - specularAlbedo_0) * vec3<f32>((1.0f - metallic_0)) * albedo_0 * irradiance_0 + reflection_0 * specularAlbedo_0;
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

fn VectorOutputs_put_2( this_7 : ptr<function, VectorOutputs_0>,  value_1 : vec3<f32>)
{
    VectorOutputs_put_1(&((*this_7)), value_1.x);
    VectorOutputs_put_1(&((*this_7)), value_1.y);
    VectorOutputs_put_1(&((*this_7)), value_1.z);
    return;
}

fn hemisphere_irradiance_0( n_0 : vec3<f32>) -> vec3<f32>
{
    var _S3 : f32 = n_0.y;
    return mix(vec3<f32>(0.18000000715255737f, 0.15999999642372131f, 0.15000000596046448f) * vec3<f32>(0.69999998807907104f), mix(vec3<f32>(0.55000001192092896f, 0.62000000476837158f, 0.72000002861022949f), vec3<f32>(0.09000000357627869f, 0.15999999642372131f, 0.34999999403953552f), vec3<f32>(pow(clamp(_S3, 0.0f, 1.0f), 0.69999998807907104f))), vec3<f32>(clamp(_S3 * 0.5f + 0.5f, 0.0f, 1.0f)));
}

fn sky_gradient_0( dir_0 : vec3<f32>) -> vec3<f32>
{
    var _S4 : f32 = dir_0.y;
    if(_S4 >= 0.0f)
    {
        return mix(vec3<f32>(0.55000001192092896f, 0.62000000476837158f, 0.72000002861022949f), vec3<f32>(0.09000000357627869f, 0.15999999642372131f, 0.34999999403953552f), vec3<f32>(pow(clamp(_S4, 0.0f, 1.0f), 0.69999998807907104f)));
    }
    else
    {
        return mix(vec3<f32>(0.55000001192092896f, 0.62000000476837158f, 0.72000002861022949f), vec3<f32>(0.18000000715255737f, 0.15999999642372131f, 0.15000000596046448f), vec3<f32>(clamp(- _S4 * 3.0f, 0.0f, 1.0f)));
    }
}

fn sun_disk_0( dir_1 : vec3<f32>,  lightDir_0 : vec3<f32>,  intensity_0 : f32) -> vec3<f32>
{
    var _S5 : f32 = max(dot(dir_1, normalize(lightDir_0)), 0.0f);
    return vec3<f32>(1.0f, 0.94999998807907104f, 0.85000002384185791f) * vec3<f32>((pow(_S5, 1200.0f) * 24.0f + pow(_S5, 48.0f) * 0.5f)) * vec3<f32>((intensity_0 * 0.40000000596046448f));
}

fn analytic_ambient_0( n_1 : vec3<f32>,  v_0 : vec3<f32>,  albedo_1 : vec3<f32>,  metallic_1 : f32,  roughness_0 : f32,  f0_3 : vec3<f32>,  energy_3 : vec2<f32>,  towardsSun_0 : vec3<f32>,  sunIntensity_0 : f32) -> vec3<f32>
{
    var irradiance_1 : vec3<f32> = hemisphere_irradiance_0(n_1);
    var reflected_0 : vec3<f32> = reflect((vec3<f32>(0) - v_0), n_1);
    return ambient_split_sum_0(irradiance_1, mix(sky_gradient_0(reflected_0) + sun_disk_0(reflected_0, towardsSun_0, sunIntensity_0), irradiance_1, vec3<f32>((roughness_0 * roughness_0))), albedo_1, metallic_1, f0_3, energy_3);
}

@compute
@workgroup_size(1, 1, 1)
fn ambient_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S6 : u32 = tid_0.x;
    var inputs_1 : VectorInputs_0 = vector_inputs_0(_S6);
    var n_2 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var v_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var albedo_2 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var metallic_2 : f32 = VectorInputs_f32_0(&(inputs_1));
    var roughness_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var f0_4 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var energy_4 : vec2<f32> = VectorInputs_f32x2_0(&(inputs_1));
    var towards_sun_0 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var sun_intensity_0 : f32 = VectorInputs_f32_0(&(inputs_1));
    var irradiance_2 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var reflection_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S6);
    VectorOutputs_put_2(&(outputs_1), ambient_split_sum_0(irradiance_2, reflection_1, albedo_2, metallic_2, f0_4, energy_4));
    VectorOutputs_put_2(&(outputs_1), analytic_ambient_0(n_2, v_1, albedo_2, metallic_2, roughness_1, f0_4, energy_4, towards_sun_0, sun_intensity_0));
    return;
}

