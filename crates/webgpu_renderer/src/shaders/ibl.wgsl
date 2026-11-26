struct IblParams_std140_0
{
    @align(16) face_roughness_samples_mip_0 : vec4<f32>,
    @align(16) source_resolution_0 : vec4<f32>,
};

@binding(0) @group(0) var<uniform> params_0 : IblParams_std140_0;
@binding(4) @group(0) var sunMask_0 : texture_2d<f32>;

@binding(1) @group(0) var srcEquirect_0 : texture_2d<f32>;

@binding(2) @group(0) var srcCube_0 : texture_cube<f32>;

@binding(3) @group(0) var srcSampler_0 : sampler;

struct SunMask_0
{
     dir_0 : vec3<f32>,
     ring_0 : vec3<f32>,
};

fn EnvironmentMips_radiance_0( dir_1 : vec3<f32>,  lod_0 : f32) -> vec3<f32>
{
    return (textureSampleLevel((srcCube_0), (srcSampler_0), (dir_1), (lod_0))).xyz;
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

fn find_sun_0() -> SunMask_0
{
    var coarsePeak_0 : f32;
    var centre_0 : vec3<f32>;
    var i_0 : u32;
    const _S1 : vec3<f32> = vec3<f32>(0.0f, 1.0f, 0.0f);
    var coarsePeak_1 : f32 = 0.0f;
    var centre_1 : vec3<f32> = _S1;
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
        var _S2 : f32 = (10.0f * f32(j_0) - 85.0f) * 0.01745329238474369f;
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
            var _S3 : f32 = cos(_S2);
            var dir_2 : vec3<f32> = vec3<f32>(_S3 * cos(lon_0), sin(_S2), _S3 * sin(lon_0));
            var l_0 : f32 = sun_mask_level_0(EnvironmentMips_radiance_0(dir_2, 4.0f));
            if(l_0 > coarsePeak_0)
            {
                coarsePeak_0 = l_0;
                centre_0 = dir_2;
            }
            i_0 = i_0 + u32(1);
        }
        var _S4 : u32 = j_0 + u32(1);
        coarsePeak_1 = coarsePeak_0;
        centre_1 = centre_0;
        j_0 = _S4;
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
        var _S5 : vec3<f32> = cross(centre_1, t_0);
        var _S6 : vec3<f32> = vec3<f32>(0.0f);
        var y_0 : i32 = i32(-6);
        centre_0 = _S6;
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
                var dir_3 : vec3<f32> = normalize(centre_1 + (vec3<f32>(f32(x_0)) * t_0 + vec3<f32>(f32(y_0)) * _S5) * vec3<f32>(0.02618592046201229f));
                var l_1 : f32 = sun_mask_level_0(EnvironmentMips_radiance_0(dir_3, 2.0f));
                var _S7 : f32 = max(peak_0, l_1);
                var weighted_0 : vec3<f32> = centre_0 + vec3<f32>(max(l_1 - 0.5f * coarsePeak_1, 0.0f)) * dir_3;
                var _S8 : i32 = x_0 + i32(1);
                peak_0 = _S7;
                x_0 = _S8;
                centre_0 = weighted_0;
            }
            var _S9 : i32 = y_0 + i32(1);
            coarsePeak_0 = peak_0;
            y_0 = _S9;
        }
        if((dot(centre_0, centre_0)) > 0.0f)
        {
            centre_1 = normalize(centre_0);
        }
        i_0 = i_0 + u32(1);
    }
    var t_1 : vec3<f32> = sun_search_tangent_0(centre_1);
    var _S10 : vec3<f32> = cross(centre_1, t_1);
    var _S11 : vec3<f32> = vec3<f32>(0.0f);
    var k_0 : u32 = u32(0);
    centre_0 = _S11;
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
        var ring_1 : vec3<f32> = centre_0 + EnvironmentMips_radiance_0(centre_1 * vec3<f32>(0.97814762592315674f) + (vec3<f32>(cos(phi_0)) * t_1 + vec3<f32>(sin(phi_0)) * _S10) * vec3<f32>(0.20791168510913849f), 2.0f);
        k_0 = k_0 + u32(1);
        centre_0 = ring_1;
    }
    var mask_0 : SunMask_0;
    var _S12 : vec3<f32> = centre_0 / vec3<f32>(16.0f);
    mask_0.ring_0 = _S12;
    if(coarsePeak_0 > (3.0f * sun_mask_level_0(_S12)))
    {
    }
    else
    {
        centre_1 = _S11;
    }
    mask_0.dir_0 = centre_1;
    return mask_0;
}

struct FullscreenVsOut_0
{
    @builtin(position) svPosition_0 : vec4<f32>,
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

@vertex
fn vs_fullscreen(@builtin(vertex_index) vid_1 : u32) -> FullscreenVsOut_0
{
    return fullscreen_vs_0(vid_1);
}

@vertex
fn vs_vulkan_fullscreen(@builtin(vertex_index) vid_2 : u32) -> FullscreenVsOut_0
{
    return fullscreen_vs_0(vid_2);
}

fn baked_sun_mask_0() -> SunMask_0
{
    var mask_1 : SunMask_0;
    const _S13 : vec3<i32> = vec3<i32>(i32(0), i32(0), i32(0));
    mask_1.dir_0 = (textureLoad((sunMask_0), ((_S13)).xy, ((_S13)).z)).xyz * vec3<f32>(params_0.source_resolution_0.y);
    const _S14 : vec3<i32> = vec3<i32>(i32(1), i32(0), i32(0));
    mask_1.ring_0 = (textureLoad((sunMask_0), ((_S14)).xy, ((_S14)).z)).xyz;
    return mask_1;
}

fn cube_direction_0( face_0 : u32,  uv_1 : vec2<f32>) -> vec3<f32>
{
    var a_0 : f32 = 2.0f * uv_1.x - 1.0f;
    var b_0 : f32 = 1.0f - 2.0f * uv_1.y;
    var dir_4 : vec3<f32>;
    switch(face_0)
    {
    case u32(0):
        {
            dir_4 = vec3<f32>(1.0f, b_0, - a_0);
        }
    case u32(1):
        {
            dir_4 = vec3<f32>(-1.0f, b_0, a_0);
        }
    case u32(2):
        {
            dir_4 = vec3<f32>(a_0, 1.0f, - b_0);
        }
    case u32(3):
        {
            dir_4 = vec3<f32>(a_0, -1.0f, b_0);
        }
    case u32(4):
        {
            dir_4 = vec3<f32>(a_0, b_0, 1.0f);
        }
    default :
        {
            dir_4 = vec3<f32>(- a_0, b_0, -1.0f);
        }
    }
    return normalize(dir_4);
}

fn equirect_uv_0( dir_5 : vec3<f32>) -> vec2<f32>
{
    return vec2<f32>(atan2(dir_5.z, dir_5.x) / 6.28318548202514648f + 0.5f, 0.5f - asin(clamp(dir_5.y, -1.0f, 1.0f)) / 3.14159274101257324f);
}

fn sample_equirect_0( dir_6 : vec3<f32>) -> vec3<f32>
{
    var dimsX_0 : u32;
    var dimsY_0 : u32;
    {var dim = textureDimensions((srcEquirect_0));((dimsX_0)) = dim.x;((dimsY_0)) = dim.y;};
    var _S15 : i32 = i32(dimsX_0);
    var _S16 : i32 = i32(dimsY_0);
    var texel_0 : vec2<f32> = equirect_uv_0(dir_6) * vec2<f32>(vec2<i32>(_S15, _S16)) - vec2<f32>(0.5f);
    var base_0 : vec2<f32> = floor(texel_0);
    var frac_0 : vec2<f32> = texel_0 - base_0;
    var x0_0 : i32 = i32(base_0.x);
    var y0_0 : i32 = i32(base_0.y);
    var _S17 : i32 = x0_0 % _S15;
    var xa_0 : i32 = (_S17 + _S15) % _S15;
    var _S18 : i32 = (x0_0 + i32(1)) % _S15;
    var xb_0 : i32 = (_S18 + _S15) % _S15;
    var _S19 : i32 = _S16 - i32(1);
    var ya_0 : i32 = clamp(y0_0, i32(0), _S19);
    var yb_0 : i32 = clamp(y0_0 + i32(1), i32(0), _S19);
    var _S20 : vec3<i32> = vec3<i32>(xa_0, ya_0, i32(0));
    var _S21 : vec3<i32> = vec3<i32>(xb_0, ya_0, i32(0));
    var _S22 : vec3<i32> = vec3<i32>(xa_0, yb_0, i32(0));
    var _S23 : vec3<i32> = vec3<i32>(xb_0, yb_0, i32(0));
    var _S24 : vec3<f32> = vec3<f32>(frac_0.x);
    return mix(mix((textureLoad((srcEquirect_0), ((_S20)).xy, ((_S20)).z)).xyz, (textureLoad((srcEquirect_0), ((_S21)).xy, ((_S21)).z)).xyz, _S24), mix((textureLoad((srcEquirect_0), ((_S22)).xy, ((_S22)).z)).xyz, (textureLoad((srcEquirect_0), ((_S23)).xy, ((_S23)).z)).xyz, _S24), vec3<f32>(frac_0.y));
}

fn sun_masked_0( radiance_0 : vec3<f32>,  dir_7 : vec3<f32>,  mask_2 : SunMask_0) -> vec3<f32>
{
    var _S25 : vec3<f32>;
    if((dot(dir_7, mask_2.dir_0)) > 0.98480772972106934f)
    {
        _S25 = min(radiance_0, mask_2.ring_0);
    }
    else
    {
        _S25 = radiance_0;
    }
    return _S25;
}

struct pixelOutput_0
{
    @location(0) output_0 : vec4<f32>,
};

struct pixelInput_0
{
    @location(0) uv_2 : vec2<f32>,
};

@fragment
fn fs_equirect_to_cube( _S26 : pixelInput_0, @builtin(position) svPosition_1 : vec4<f32>) -> pixelOutput_0
{
    var _S27 : u32 = u32(params_0.face_roughness_samples_mip_0.x);
    var _S28 : SunMask_0 = baked_sun_mask_0();
    var texel_1 : vec2<f32> = (fwidth((_S26.uv_2)));
    var taps_0 : u32 = clamp(u32(ceil(0.5f * params_0.source_resolution_0.x * texel_1.x)), u32(1), u32(16));
    var _S29 : vec3<f32> = vec3<f32>(0.0f);
    var y_2 : u32 = u32(0);
    var sum_0 : vec3<f32> = _S29;
    for(;;)
    {
        if(y_2 < taps_0)
        {
        }
        else
        {
            break;
        }
        var x_2 : u32 = u32(0);
        for(;;)
        {
            if(x_2 < taps_0)
            {
            }
            else
            {
                break;
            }
            var _S30 : vec2<f32> = vec2<f32>(0.5f);
            var dir_8 : vec3<f32> = cube_direction_0(_S27, _S26.uv_2 + ((vec2<f32>(f32(x_2), f32(y_2)) + _S30) / vec2<f32>(f32(taps_0)) - _S30) * texel_1);
            var _S31 : vec3<f32> = sample_equirect_0(dir_8);
            var sum_1 : vec3<f32> = sum_0 + sun_masked_0(_S31, dir_8, _S28);
            x_2 = x_2 + u32(1);
            sum_0 = sum_1;
        }
        y_2 = y_2 + u32(1);
    }
    var _S32 : pixelOutput_0 = pixelOutput_0( vec4<f32>(sum_0 / vec3<f32>(f32(taps_0 * taps_0)), 1.0f) );
    return _S32;
}

fn aces_tonemap_inverse_0( display_0 : vec3<f32>) -> vec3<f32>
{
    var y_3 : vec3<f32> = clamp(display_0, vec3<f32>(0.0f), vec3<f32>(1.0f));
    var qa_0 : vec3<f32> = vec3<f32>(2.50999999046325684f) - vec3<f32>(2.43000006675720215f) * y_3;
    var qb_0 : vec3<f32> = vec3<f32>(0.02999999932944775f) - vec3<f32>(0.5899999737739563f) * y_3;
    return ((vec3<f32>(0) - qb_0) + sqrt(qb_0 * qb_0 + vec3<f32>(4.0f) * qa_0 * vec3<f32>(0.14000000059604645f) * y_3)) / (vec3<f32>(2.0f) * qa_0);
}

struct pixelOutput_1
{
    @location(0) output_1 : vec4<f32>,
};

struct pixelInput_1
{
    @location(0) uv_3 : vec2<f32>,
};

@fragment
fn fs_skybox_to_cube( _S33 : pixelInput_1, @builtin(position) svPosition_2 : vec4<f32>) -> pixelOutput_1
{
    var _S34 : u32 = u32(params_0.face_roughness_samples_mip_0.x);
    var _S35 : SunMask_0 = baked_sun_mask_0();
    var texel_2 : vec2<f32> = (fwidth((_S33.uv_3)));
    var taps_1 : u32 = clamp(u32(ceil(params_0.source_resolution_0.x * texel_2.x)), u32(1), u32(16));
    var _S36 : vec3<f32> = vec3<f32>(0.0f);
    var y_4 : u32 = u32(0);
    var sum_2 : vec3<f32> = _S36;
    for(;;)
    {
        if(y_4 < taps_1)
        {
        }
        else
        {
            break;
        }
        var x_3 : u32 = u32(0);
        for(;;)
        {
            if(x_3 < taps_1)
            {
            }
            else
            {
                break;
            }
            var _S37 : vec2<f32> = vec2<f32>(0.5f);
            var dir_9 : vec3<f32> = cube_direction_0(_S34, _S33.uv_3 + ((vec2<f32>(f32(x_3), f32(y_4)) + _S37) / vec2<f32>(f32(taps_1)) - _S37) * texel_2);
            var sum_3 : vec3<f32> = sum_2 + sun_masked_0(aces_tonemap_inverse_0((textureSampleLevel((srcCube_0), (srcSampler_0), (dir_9), (0.0f))).xyz), dir_9, _S35);
            x_3 = x_3 + u32(1);
            sum_2 = sum_3;
        }
        y_4 = y_4 + u32(1);
    }
    var _S38 : pixelOutput_1 = pixelOutput_1( vec4<f32>(sum_2 / vec3<f32>(f32(taps_1 * taps_1)), 1.0f) );
    return _S38;
}

struct pixelOutput_2
{
    @location(0) output_2 : vec4<f32>,
};

struct pixelInput_2
{
    @location(0) uv_4 : vec2<f32>,
};

@fragment
fn fs_find_sun( _S39 : pixelInput_2, @builtin(position) svPosition_3 : vec4<f32>) -> pixelOutput_2
{
    var _S40 : SunMask_0 = find_sun_0();
    var _S41 : vec4<f32>;
    if((_S39.uv_4.x) < 0.5f)
    {
        _S41 = vec4<f32>(_S40.dir_0, 0.0f);
    }
    else
    {
        _S41 = vec4<f32>(_S40.ring_0, 0.0f);
    }
    var _S42 : pixelOutput_2 = pixelOutput_2( _S41 );
    return _S42;
}

struct pixelOutput_3
{
    @location(0) output_3 : vec4<f32>,
};

struct pixelInput_3
{
    @location(0) uv_5 : vec2<f32>,
};

@fragment
fn fs_downsample_cube( _S43 : pixelInput_3, @builtin(position) svPosition_4 : vec4<f32>) -> pixelOutput_3
{
    var _S44 : pixelOutput_3 = pixelOutput_3( (textureSampleLevel((srcCube_0), (srcSampler_0), (cube_direction_0(u32(params_0.face_roughness_samples_mip_0.x), _S43.uv_5)), (params_0.face_roughness_samples_mip_0.w))) );
    return _S44;
}

struct pixelOutput_4
{
    @location(0) output_4 : vec4<f32>,
};

struct pixelInput_4
{
    @location(0) uv_6 : vec2<f32>,
};

@fragment
fn fs_irradiance( _S45 : pixelInput_4, @builtin(position) svPosition_5 : vec4<f32>) -> pixelOutput_4
{
    var dir_10 : vec3<f32> = cube_direction_0(u32(params_0.face_roughness_samples_mip_0.x), _S45.uv_6);
    var up_1 : vec3<f32>;
    if((abs(dir_10.y)) < 0.99900001287460327f)
    {
        up_1 = vec3<f32>(0.0f, 1.0f, 0.0f);
    }
    else
    {
        up_1 = vec3<f32>(1.0f, 0.0f, 0.0f);
    }
    var right_0 : vec3<f32> = normalize(cross(up_1, dir_10));
    var _S46 : vec3<f32> = cross(dir_10, right_0);
    var _S47 : vec3<f32> = vec3<f32>(0.0f);
    var _S48 : f32 = 12.56637096405029297f / (6.0f * params_0.source_resolution_0.x * params_0.source_resolution_0.x);
    var phi_i_0 : u32 = u32(0);
    var irradiance_0 : vec3<f32> = _S47;
    for(;;)
    {
        if(phi_i_0 < u32(128))
        {
        }
        else
        {
            break;
        }
        var _S49 : f32 = 6.28318548202514648f * (f32(phi_i_0) + 0.5f) / 128.0f;
        var theta_i_0 : u32 = u32(0);
        for(;;)
        {
            if(theta_i_0 < u32(64))
            {
            }
            else
            {
                break;
            }
            var theta_0 : f32 = 1.57079637050628662f * (f32(theta_i_0) + 0.5f) / 64.0f;
            var _S50 : f32 = sin(theta_0);
            var _S51 : vec3<f32> = vec3<f32>(cos(theta_0));
            var irradiance_1 : vec3<f32> = irradiance_0 + (textureSampleLevel((srcCube_0), (srcSampler_0), (vec3<f32>((_S50 * cos(_S49))) * right_0 + vec3<f32>((_S50 * sin(_S49))) * _S46 + _S51 * dir_10), (max(0.5f * log2(0.00120478577446193f * _S50 / _S48) + 1.0f, 0.0f)))).xyz * _S51 * vec3<f32>(_S50);
            theta_i_0 = theta_i_0 + u32(1);
            irradiance_0 = irradiance_1;
        }
        phi_i_0 = phi_i_0 + u32(1);
    }
    var _S52 : pixelOutput_4 = pixelOutput_4( vec4<f32>(vec3<f32>(3.14159274101257324f) * irradiance_0 / vec3<f32>(8192.0f), 1.0f) );
    return _S52;
}

fn radical_inverse_vdc_0( bits_in_0 : u32) -> f32
{
    var bits_0 : u32 = (((bits_in_0 << (u32(16)))) | (((bits_in_0 >> (u32(16))))));
    var bits_1 : u32 = (((((bits_0 & (u32(1431655765)))) << (u32(1)))) | (((((bits_0 & (u32(2863311530)))) >> (u32(1))))));
    var bits_2 : u32 = (((((bits_1 & (u32(858993459)))) << (u32(2)))) | (((((bits_1 & (u32(3435973836)))) >> (u32(2))))));
    var bits_3 : u32 = (((((bits_2 & (u32(252645135)))) << (u32(4)))) | (((((bits_2 & (u32(4042322160)))) >> (u32(4))))));
    return f32((((((bits_3 & (u32(16711935)))) << (u32(8)))) | (((((bits_3 & (u32(4278255360)))) >> (u32(8))))))) * 2.32830643653869629e-10f;
}

fn hammersley_0( i_1 : u32,  count_0 : u32) -> vec2<f32>
{
    return vec2<f32>(f32(i_1) / f32(count_0), radical_inverse_vdc_0(i_1));
}

fn importance_sample_ggx_0( xi_0 : vec2<f32>,  normal_0 : vec3<f32>,  roughness_0 : f32) -> vec3<f32>
{
    var a_1 : f32 = roughness_0 * roughness_0;
    var phi_1 : f32 = 6.28318548202514648f * xi_0.x;
    var _S53 : f32 = xi_0.y;
    var cosTheta_0 : f32 = sqrt(clamp((1.0f - _S53) / (1.0f + (a_1 * a_1 - 1.0f) * _S53), 0.0f, 1.0f));
    var sinTheta_0 : f32 = sqrt(max(1.0f - cosTheta_0 * cosTheta_0, 0.0f));
    var _S54 : f32 = sinTheta_0 * cos(phi_1);
    var _S55 : f32 = sinTheta_0 * sin(phi_1);
    var up_2 : vec3<f32>;
    if((abs(normal_0.z)) < 0.99900001287460327f)
    {
        up_2 = vec3<f32>(0.0f, 0.0f, 1.0f);
    }
    else
    {
        up_2 = vec3<f32>(0.0f, 1.0f, 0.0f);
    }
    var tangent_0 : vec3<f32> = normalize(cross(up_2, normal_0));
    return normalize(vec3<f32>(_S54) * tangent_0 + vec3<f32>(_S55) * cross(normal_0, tangent_0) + vec3<f32>(cosTheta_0) * normal_0);
}

fn distribution_ggx_0( nDotH_0 : f32,  roughness_1 : f32) -> f32
{
    var a_2 : f32 = roughness_1 * roughness_1;
    var a2_0 : f32 = a_2 * a_2;
    var denom_0 : f32 = nDotH_0 * nDotH_0 * (a2_0 - 1.0f) + 1.0f;
    return a2_0 / max(3.14159274101257324f * denom_0 * denom_0, 9.99999997475242708e-07f);
}

struct pixelOutput_5
{
    @location(0) output_5 : vec4<f32>,
};

struct pixelInput_5
{
    @location(0) uv_7 : vec2<f32>,
};

@fragment
fn fs_prefilter( _S56 : pixelInput_5, @builtin(position) svPosition_6 : vec4<f32>) -> pixelOutput_5
{
    var normal_1 : vec3<f32> = cube_direction_0(u32(params_0.face_roughness_samples_mip_0.x), _S56.uv_7);
    var roughness_2 : f32 = params_0.face_roughness_samples_mip_0.y;
    var _S57 : u32 = u32(params_0.face_roughness_samples_mip_0.z);
    var _S58 : f32 = params_0.source_resolution_0.x;
    if(roughness_2 <= 0.0f)
    {
        var _S59 : pixelOutput_5 = pixelOutput_5( (textureSampleLevel((srcCube_0), (srcSampler_0), (normal_1), (0.0f))) );
        return _S59;
    }
    var _S60 : vec3<f32> = vec3<f32>(0.0f);
    var i_2 : u32 = u32(0);
    var color_0 : vec3<f32> = _S60;
    var totalWeight_0 : f32 = 0.0f;
    for(;;)
    {
        if(i_2 < _S57)
        {
        }
        else
        {
            break;
        }
        var halfVector_0 : vec3<f32> = importance_sample_ggx_0(hammersley_0(i_2, _S57), normal_1, roughness_2);
        var _S61 : f32 = dot(normal_1, halfVector_0);
        var sampleDir_0 : vec3<f32> = normalize(vec3<f32>((2.0f * _S61)) * halfVector_0 - normal_1);
        var nDotL_0 : f32 = dot(normal_1, sampleDir_0);
        if(nDotL_0 > 0.0f)
        {
            var totalWeight_1 : f32 = totalWeight_0 + nDotL_0;
            color_0 = color_0 + (textureSampleLevel((srcCube_0), (srcSampler_0), (sampleDir_0), (max(0.5f * log2(1.0f / (f32(_S57) * (distribution_ggx_0(max(_S61, 0.0f), roughness_2) * 0.25f + 0.00009999999747379f)) / (12.56637096405029297f / (6.0f * _S58 * _S58))), 0.0f)))).xyz * vec3<f32>(nDotL_0);
            totalWeight_0 = totalWeight_1;
        }
        i_2 = i_2 + u32(1);
    }
    var _S62 : pixelOutput_5 = pixelOutput_5( vec4<f32>(color_0 / vec3<f32>(max(totalWeight_0, 0.00009999999747379f)), 1.0f) );
    return _S62;
}

fn geometry_smith_ibl_0( nDotV_0 : f32,  nDotL_1 : f32,  roughness_3 : f32) -> f32
{
    var k_1 : f32 = roughness_3 * roughness_3 / 2.0f;
    var _S63 : f32 = 1.0f - k_1;
    return nDotV_0 / max(nDotV_0 * _S63 + k_1, 0.00009999999747379f) * (nDotL_1 / max(nDotL_1 * _S63 + k_1, 0.00009999999747379f));
}

struct pixelOutput_6
{
    @location(0) output_6 : vec4<f32>,
};

struct pixelInput_6
{
    @location(0) uv_8 : vec2<f32>,
};

@fragment
fn fs_brdf_lut( _S64 : pixelInput_6, @builtin(position) svPosition_7 : vec4<f32>) -> pixelOutput_6
{
    var nDotV_1 : f32 = _S64.uv_8.x;
    var _S65 : f32 = _S64.uv_8.y;
    var _S66 : vec3<f32> = vec3<f32>(sqrt(1.0f - nDotV_1 * nDotV_1), 0.0f, nDotV_1);
    const _S67 : vec3<f32> = vec3<f32>(0.0f, 0.0f, 1.0f);
    var _S68 : vec2<f32> = vec2<f32>(0.0f);
    var i_3 : u32 = u32(0);
    var result_0 : vec2<f32> = _S68;
    for(;;)
    {
        if(i_3 < u32(1024))
        {
        }
        else
        {
            break;
        }
        var halfVector_1 : vec3<f32> = importance_sample_ggx_0(hammersley_0(i_3, u32(1024)), _S67, _S65);
        var _S69 : f32 = dot(_S66, halfVector_1);
        var _S70 : f32 = max(normalize(vec3<f32>((2.0f * _S69)) * halfVector_1 - _S66).z, 0.0f);
        if(_S70 > 0.0f)
        {
            var _S71 : f32 = max(_S69, 0.0f);
            var _S72 : f32 = max(nDotV_1, 0.00009999999747379f);
            var gVis_0 : f32 = geometry_smith_ibl_0(_S72, _S70, _S65) * _S71 / max(max(halfVector_1.z, 0.0f) * _S72, 9.99999997475242708e-07f);
            var fc_0 : f32 = pow(1.0f - _S71, 5.0f);
            result_0 = result_0 + vec2<f32>((1.0f - fc_0) * gVis_0, fc_0 * gVis_0);
        }
        i_3 = i_3 + u32(1);
    }
    var _S73 : pixelOutput_6 = pixelOutput_6( vec4<f32>(result_0 / vec2<f32>(1024.0f), 0.0f, 1.0f) );
    return _S73;
}

