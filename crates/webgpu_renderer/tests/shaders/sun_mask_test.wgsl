@binding(0) @group(0) var<storage, read> vectorInputs_0 : array<u32>;

@binding(1) @group(0) var<storage, read_write> vectorOutputs_0 : array<u32>;

struct SunMask_0
{
     dir_0 : vec3<f32>,
     ring_0 : vec3<f32>,
};

struct DiskSky_0
{
     sky_0 : vec3<f32>,
     disk_0 : vec3<f32>,
     disk_dir_0 : vec3<f32>,
     disk_cos_0 : f32,
};

fn DiskSky_radiance_0( this_0 : DiskSky_0,  dir_1 : vec3<f32>,  lod_0 : f32) -> vec3<f32>
{
    var _S1 : vec3<f32>;
    if((dot(dir_1, this_0.disk_dir_0)) > (this_0.disk_cos_0))
    {
        _S1 = this_0.disk_0;
    }
    else
    {
        _S1 = this_0.sky_0;
    }
    return _S1;
}

fn sun_mask_level_0( c_0 : vec3<f32>) -> f32
{
    return min(c_0.x, min(c_0.y, c_0.z));
}

fn sun_search_tangent_0( n_0 : vec3<f32>) -> vec3<f32>
{
    var up_0 : vec3<f32>;
    if((abs(n_0.y)) < 0.99900001287460327f)
    {
        up_0 = vec3<f32>(0.0f, 1.0f, 0.0f);
    }
    else
    {
        up_0 = vec3<f32>(1.0f, 0.0f, 0.0f);
    }
    return normalize(cross(up_0, n_0));
}

fn find_sun_0( sky_1 : DiskSky_0) -> SunMask_0
{
    var coarsePeak_0 : f32;
    var centre_0 : vec3<f32>;
    var i_0 : u32;
    const _S2 : vec3<f32> = vec3<f32>(0.0f, 1.0f, 0.0f);
    var coarsePeak_1 : f32 = 0.0f;
    var centre_1 : vec3<f32> = _S2;
    var j_0 : u32 = u32(0);
    for(;;)
    {
        if(j_0 < u32(18))
        {
        }
        else
        {
            break;
        }
        var _S3 : f32 = (10.0f * f32(j_0) - 85.0f) * 0.01745329238474369f;
        coarsePeak_0 = coarsePeak_1;
        centre_0 = centre_1;
        i_0 = u32(0);
        for(;;)
        {
            if(i_0 < u32(36))
            {
            }
            else
            {
                break;
            }
            var lon_0 : f32 = 10.0f * f32(i_0) * 0.01745329238474369f;
            var _S4 : f32 = cos(_S3);
            var dir_2 : vec3<f32> = vec3<f32>(_S4 * cos(lon_0), sin(_S3), _S4 * sin(lon_0));
            var l_0 : f32 = sun_mask_level_0(DiskSky_radiance_0(sky_1, dir_2, 4.0f));
            if(l_0 > coarsePeak_0)
            {
                coarsePeak_0 = l_0;
                centre_0 = dir_2;
            }
            i_0 = i_0 + u32(1);
        }
        var _S5 : u32 = j_0 + u32(1);
        coarsePeak_1 = coarsePeak_0;
        centre_1 = centre_0;
        j_0 = _S5;
    }
    coarsePeak_0 = 0.0f;
    i_0 = u32(0);
    for(;;)
    {
        if(i_0 < u32(2))
        {
        }
        else
        {
            break;
        }
        var t_0 : vec3<f32> = sun_search_tangent_0(centre_1);
        var _S6 : vec3<f32> = cross(centre_1, t_0);
        var _S7 : vec3<f32> = vec3<f32>(0.0f);
        var y_0 : i32 = i32(-6);
        centre_0 = _S7;
        for(;;)
        {
            if(y_0 <= i32(6))
            {
            }
            else
            {
                break;
            }
            var peak_0 : f32 = coarsePeak_0;
            var x_0 : i32 = i32(-6);
            for(;;)
            {
                if(x_0 <= i32(6))
                {
                }
                else
                {
                    break;
                }
                var dir_3 : vec3<f32> = normalize(centre_1 + (vec3<f32>(f32(x_0)) * t_0 + vec3<f32>(f32(y_0)) * _S6) * vec3<f32>(0.02618592046201229f));
                var l_1 : f32 = sun_mask_level_0(DiskSky_radiance_0(sky_1, dir_3, 2.0f));
                var _S8 : f32 = max(peak_0, l_1);
                var weighted_0 : vec3<f32> = centre_0 + vec3<f32>(max(l_1 - 0.5f * coarsePeak_1, 0.0f)) * dir_3;
                var _S9 : i32 = x_0 + i32(1);
                peak_0 = _S8;
                x_0 = _S9;
                centre_0 = weighted_0;
            }
            var _S10 : i32 = y_0 + i32(1);
            coarsePeak_0 = peak_0;
            y_0 = _S10;
        }
        if((dot(centre_0, centre_0)) > 0.0f)
        {
            centre_1 = normalize(centre_0);
        }
        i_0 = i_0 + u32(1);
    }
    var t_1 : vec3<f32> = sun_search_tangent_0(centre_1);
    var _S11 : vec3<f32> = cross(centre_1, t_1);
    var _S12 : vec3<f32> = vec3<f32>(0.0f);
    var k_0 : u32 = u32(0);
    centre_0 = _S12;
    for(;;)
    {
        if(k_0 < u32(16))
        {
        }
        else
        {
            break;
        }
        var phi_0 : f32 = 6.28318548202514648f * (f32(k_0) + 0.5f) / 16.0f;
        var ring_1 : vec3<f32> = centre_0 + DiskSky_radiance_0(sky_1, centre_1 * vec3<f32>(0.97814762592315674f) + (vec3<f32>(cos(phi_0)) * t_1 + vec3<f32>(sin(phi_0)) * _S11) * vec3<f32>(0.20791168510913849f), 2.0f);
        k_0 = k_0 + u32(1);
        centre_0 = ring_1;
    }
    var mask_0 : SunMask_0;
    var _S13 : vec3<f32> = centre_0 / vec3<f32>(16.0f);
    mask_0.ring_0 = _S13;
    if(coarsePeak_0 > (3.0f * sun_mask_level_0(_S13)))
    {
    }
    else
    {
        centre_1 = _S12;
    }
    mask_0.dir_0 = centre_1;
    return mask_0;
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

fn VectorInputs_u32_0( this_1 : ptr<function, VectorInputs_0>) -> u32
{
    var word_0 : u32 = vectorInputs_0[(*this_1).at_0];
    (*this_1).at_0 = (*this_1).at_0 + u32(1);
    return word_0;
}

fn VectorInputs_f32_0( this_2 : ptr<function, VectorInputs_0>) -> f32
{
    var _S14 : u32 = VectorInputs_u32_0(&((*this_2)));
    return (bitcast<f32>((_S14)));
}

fn VectorInputs_f32_1( this_3 : ptr<function, VectorInputs_0>) -> f32
{
    var _S15 : u32 = VectorInputs_u32_0(&((*this_3)));
    return (bitcast<f32>((_S15)));
}

fn VectorInputs_f32x3_0( this_4 : ptr<function, VectorInputs_0>) -> vec3<f32>
{
    var x_1 : f32 = VectorInputs_f32_1(&((*this_4)));
    var y_1 : f32 = VectorInputs_f32_1(&((*this_4)));
    var z_0 : f32 = VectorInputs_f32_1(&((*this_4)));
    return vec3<f32>(x_1, y_1, z_0);
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

fn sun_masked_0( radiance_0 : vec3<f32>,  dir_4 : vec3<f32>,  mask_1 : SunMask_0) -> vec3<f32>
{
    var _S16 : vec3<f32>;
    if((dot(dir_4, mask_1.dir_0)) > 0.98480772972106934f)
    {
        _S16 = min(radiance_0, mask_1.ring_0);
    }
    else
    {
        _S16 = radiance_0;
    }
    return _S16;
}

@compute
@workgroup_size(1, 1, 1)
fn sun_mask_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S17 : u32 = tid_0.x;
    var inputs_1 : VectorInputs_0 = vector_inputs_0(_S17);
    var sky_2 : DiskSky_0;
    var _S18 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    sky_2.sky_0 = _S18;
    var _S19 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    sky_2.disk_0 = _S19;
    var _S20 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    sky_2.disk_dir_0 = _S20;
    var _S21 : f32 = VectorInputs_f32_0(&(inputs_1));
    sky_2.disk_cos_0 = _S21;
    var radiance_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var dir_5 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S17);
    var mask_2 : SunMask_0 = find_sun_0(sky_2);
    VectorOutputs_put_3(&(outputs_1), mask_2.dir_0);
    VectorOutputs_put_3(&(outputs_1), mask_2.ring_0);
    VectorOutputs_put_3(&(outputs_1), sun_masked_0(radiance_1, dir_5, mask_2));
    VectorOutputs_put_1(&(outputs_1), sun_mask_level_0(radiance_1));
    VectorOutputs_put_3(&(outputs_1), sun_search_tangent_0(dir_5));
    return;
}

