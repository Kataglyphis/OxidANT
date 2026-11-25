struct IblParams_std140_0
{
    @align(16) face_roughness_samples_mip_0 : vec4<f32>,
    @align(16) source_resolution_0 : vec4<f32>,
};

@binding(0) @group(0) var<uniform> params_0 : IblParams_std140_0;
@binding(1) @group(0) var srcEquirect_0 : texture_2d<f32>;

@binding(2) @group(0) var srcCube_0 : texture_cube<f32>;

@binding(3) @group(0) var srcSampler_0 : sampler;

struct FullscreenVsOut_0
{
    @builtin(position) svPosition_0 : vec4<f32>,
    @location(0) uv_0 : vec2<f32>,
};

fn fullscreen_vs_0( vid_0 : u32) -> FullscreenVsOut_0
{
    var x_0 : f32 = f32(vid_0 / u32(2)) * 4.0f - 1.0f;
    var y_0 : f32 = f32(vid_0 % u32(2)) * 4.0f - 1.0f;
    var o_0 : FullscreenVsOut_0;
    o_0.svPosition_0 = vec4<f32>(x_0, y_0, 0.0f, 1.0f);
    o_0.uv_0 = vec2<f32>((x_0 + 1.0f) * 0.5f, 1.0f - (y_0 + 1.0f) * 0.5f);
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

fn cube_direction_0( face_0 : u32,  uv_1 : vec2<f32>) -> vec3<f32>
{
    var a_0 : f32 = 2.0f * uv_1.x - 1.0f;
    var b_0 : f32 = 1.0f - 2.0f * uv_1.y;
    var dir_0 : vec3<f32>;
    switch(face_0)
    {
    case u32(0):
        {
            dir_0 = vec3<f32>(1.0f, b_0, - a_0);
        }
    case u32(1):
        {
            dir_0 = vec3<f32>(-1.0f, b_0, a_0);
        }
    case u32(2):
        {
            dir_0 = vec3<f32>(a_0, 1.0f, - b_0);
        }
    case u32(3):
        {
            dir_0 = vec3<f32>(a_0, -1.0f, b_0);
        }
    case u32(4):
        {
            dir_0 = vec3<f32>(a_0, b_0, 1.0f);
        }
    default :
        {
            dir_0 = vec3<f32>(- a_0, b_0, -1.0f);
        }
    }
    return normalize(dir_0);
}

fn equirect_uv_0( dir_1 : vec3<f32>) -> vec2<f32>
{
    return vec2<f32>(atan2(dir_1.z, dir_1.x) / 6.28318548202514648f + 0.5f, 0.5f - asin(clamp(dir_1.y, -1.0f, 1.0f)) / 3.14159274101257324f);
}

fn sample_equirect_0( dir_2 : vec3<f32>) -> vec3<f32>
{
    var dimsX_0 : u32;
    var dimsY_0 : u32;
    {var dim = textureDimensions((srcEquirect_0));((dimsX_0)) = dim.x;((dimsY_0)) = dim.y;};
    var _S1 : i32 = i32(dimsX_0);
    var _S2 : i32 = i32(dimsY_0);
    var texel_0 : vec2<f32> = equirect_uv_0(dir_2) * vec2<f32>(vec2<i32>(_S1, _S2)) - vec2<f32>(0.5f);
    var base_0 : vec2<f32> = floor(texel_0);
    var frac_0 : vec2<f32> = texel_0 - base_0;
    var x0_0 : i32 = i32(base_0.x);
    var y0_0 : i32 = i32(base_0.y);
    var _S3 : i32 = x0_0 % _S1;
    var xa_0 : i32 = (_S3 + _S1) % _S1;
    var _S4 : i32 = (x0_0 + i32(1)) % _S1;
    var xb_0 : i32 = (_S4 + _S1) % _S1;
    var _S5 : i32 = _S2 - i32(1);
    var ya_0 : i32 = clamp(y0_0, i32(0), _S5);
    var yb_0 : i32 = clamp(y0_0 + i32(1), i32(0), _S5);
    var _S6 : vec3<i32> = vec3<i32>(xa_0, ya_0, i32(0));
    var _S7 : vec3<i32> = vec3<i32>(xb_0, ya_0, i32(0));
    var _S8 : vec3<i32> = vec3<i32>(xa_0, yb_0, i32(0));
    var _S9 : vec3<i32> = vec3<i32>(xb_0, yb_0, i32(0));
    var _S10 : vec3<f32> = vec3<f32>(frac_0.x);
    return mix(mix((textureLoad((srcEquirect_0), ((_S6)).xy, ((_S6)).z)).xyz, (textureLoad((srcEquirect_0), ((_S7)).xy, ((_S7)).z)).xyz, _S10), mix((textureLoad((srcEquirect_0), ((_S8)).xy, ((_S8)).z)).xyz, (textureLoad((srcEquirect_0), ((_S9)).xy, ((_S9)).z)).xyz, _S10), vec3<f32>(frac_0.y));
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
fn fs_equirect_to_cube( _S11 : pixelInput_0, @builtin(position) svPosition_1 : vec4<f32>) -> pixelOutput_0
{
    var _S12 : u32 = u32(params_0.face_roughness_samples_mip_0.x);
    var texel_1 : vec2<f32> = (fwidth((_S11.uv_2)));
    var taps_0 : u32 = clamp(u32(ceil(0.5f * params_0.source_resolution_0.x * texel_1.x)), u32(1), u32(16));
    var _S13 : vec3<f32> = vec3<f32>(0.0f);
    var y_1 : u32 = u32(0);
    var sum_0 : vec3<f32> = _S13;
    for(;;)
    {
        if(y_1 < taps_0)
        {
        }
        else
        {
            break;
        }
        var x_1 : u32 = u32(0);
        for(;;)
        {
            if(x_1 < taps_0)
            {
            }
            else
            {
                break;
            }
            var _S14 : vec2<f32> = vec2<f32>(0.5f);
            var _S15 : vec3<f32> = sample_equirect_0(cube_direction_0(_S12, _S11.uv_2 + ((vec2<f32>(f32(x_1), f32(y_1)) + _S14) / vec2<f32>(f32(taps_0)) - _S14) * texel_1));
            var sum_1 : vec3<f32> = sum_0 + _S15;
            x_1 = x_1 + u32(1);
            sum_0 = sum_1;
        }
        y_1 = y_1 + u32(1);
    }
    var _S16 : pixelOutput_0 = pixelOutput_0( vec4<f32>(sum_0 / vec3<f32>(f32(taps_0 * taps_0)), 1.0f) );
    return _S16;
}

fn aces_tonemap_inverse_0( display_0 : vec3<f32>) -> vec3<f32>
{
    var y_2 : vec3<f32> = clamp(display_0, vec3<f32>(0.0f), vec3<f32>(1.0f));
    var qa_0 : vec3<f32> = vec3<f32>(2.50999999046325684f) - vec3<f32>(2.43000006675720215f) * y_2;
    var qb_0 : vec3<f32> = vec3<f32>(0.02999999932944775f) - vec3<f32>(0.5899999737739563f) * y_2;
    return ((vec3<f32>(0) - qb_0) + sqrt(qb_0 * qb_0 + vec3<f32>(4.0f) * qa_0 * vec3<f32>(0.14000000059604645f) * y_2)) / (vec3<f32>(2.0f) * qa_0);
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
fn fs_skybox_to_cube( _S17 : pixelInput_1, @builtin(position) svPosition_2 : vec4<f32>) -> pixelOutput_1
{
    var _S18 : u32 = u32(params_0.face_roughness_samples_mip_0.x);
    var texel_2 : vec2<f32> = (fwidth((_S17.uv_3)));
    var taps_1 : u32 = clamp(u32(ceil(params_0.source_resolution_0.x * texel_2.x)), u32(1), u32(16));
    var _S19 : vec3<f32> = vec3<f32>(0.0f);
    var y_3 : u32 = u32(0);
    var sum_2 : vec3<f32> = _S19;
    for(;;)
    {
        if(y_3 < taps_1)
        {
        }
        else
        {
            break;
        }
        var x_2 : u32 = u32(0);
        for(;;)
        {
            if(x_2 < taps_1)
            {
            }
            else
            {
                break;
            }
            var _S20 : vec2<f32> = vec2<f32>(0.5f);
            var sum_3 : vec3<f32> = sum_2 + aces_tonemap_inverse_0((textureSampleLevel((srcCube_0), (srcSampler_0), (cube_direction_0(_S18, _S17.uv_3 + ((vec2<f32>(f32(x_2), f32(y_3)) + _S20) / vec2<f32>(f32(taps_1)) - _S20) * texel_2)), (0.0f))).xyz);
            x_2 = x_2 + u32(1);
            sum_2 = sum_3;
        }
        y_3 = y_3 + u32(1);
    }
    var _S21 : pixelOutput_1 = pixelOutput_1( vec4<f32>(sum_2 / vec3<f32>(f32(taps_1 * taps_1)), 1.0f) );
    return _S21;
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
fn fs_downsample_cube( _S22 : pixelInput_2, @builtin(position) svPosition_3 : vec4<f32>) -> pixelOutput_2
{
    var _S23 : pixelOutput_2 = pixelOutput_2( (textureSampleLevel((srcCube_0), (srcSampler_0), (cube_direction_0(u32(params_0.face_roughness_samples_mip_0.x), _S22.uv_4)), (params_0.face_roughness_samples_mip_0.w))) );
    return _S23;
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
fn fs_irradiance( _S24 : pixelInput_3, @builtin(position) svPosition_4 : vec4<f32>) -> pixelOutput_3
{
    var dir_3 : vec3<f32> = cube_direction_0(u32(params_0.face_roughness_samples_mip_0.x), _S24.uv_5);
    var up_0 : vec3<f32>;
    if((abs(dir_3.y)) < 0.99900001287460327f)
    {
        up_0 = vec3<f32>(0.0f, 1.0f, 0.0f);
    }
    else
    {
        up_0 = vec3<f32>(1.0f, 0.0f, 0.0f);
    }
    var right_0 : vec3<f32> = normalize(cross(up_0, dir_3));
    var _S25 : vec3<f32> = cross(dir_3, right_0);
    var _S26 : vec3<f32> = vec3<f32>(0.0f);
    var _S27 : f32 = 12.56637096405029297f / (6.0f * params_0.source_resolution_0.x * params_0.source_resolution_0.x);
    var phi_i_0 : u32 = u32(0);
    var irradiance_0 : vec3<f32> = _S26;
    for(;;)
    {
        if(phi_i_0 < u32(128))
        {
        }
        else
        {
            break;
        }
        var _S28 : f32 = 6.28318548202514648f * (f32(phi_i_0) + 0.5f) / 128.0f;
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
            var _S29 : f32 = sin(theta_0);
            var _S30 : vec3<f32> = vec3<f32>(cos(theta_0));
            var irradiance_1 : vec3<f32> = irradiance_0 + (textureSampleLevel((srcCube_0), (srcSampler_0), (vec3<f32>((_S29 * cos(_S28))) * right_0 + vec3<f32>((_S29 * sin(_S28))) * _S25 + _S30 * dir_3), (max(0.5f * log2(0.00120478577446193f * _S29 / _S27) + 1.0f, 0.0f)))).xyz * _S30 * vec3<f32>(_S29);
            theta_i_0 = theta_i_0 + u32(1);
            irradiance_0 = irradiance_1;
        }
        phi_i_0 = phi_i_0 + u32(1);
    }
    var _S31 : pixelOutput_3 = pixelOutput_3( vec4<f32>(vec3<f32>(3.14159274101257324f) * irradiance_0 / vec3<f32>(8192.0f), 1.0f) );
    return _S31;
}

fn radical_inverse_vdc_0( bits_in_0 : u32) -> f32
{
    var bits_0 : u32 = (((bits_in_0 << (u32(16)))) | (((bits_in_0 >> (u32(16))))));
    var bits_1 : u32 = (((((bits_0 & (u32(1431655765)))) << (u32(1)))) | (((((bits_0 & (u32(2863311530)))) >> (u32(1))))));
    var bits_2 : u32 = (((((bits_1 & (u32(858993459)))) << (u32(2)))) | (((((bits_1 & (u32(3435973836)))) >> (u32(2))))));
    var bits_3 : u32 = (((((bits_2 & (u32(252645135)))) << (u32(4)))) | (((((bits_2 & (u32(4042322160)))) >> (u32(4))))));
    return f32((((((bits_3 & (u32(16711935)))) << (u32(8)))) | (((((bits_3 & (u32(4278255360)))) >> (u32(8))))))) * 2.32830643653869629e-10f;
}

fn hammersley_0( i_0 : u32,  count_0 : u32) -> vec2<f32>
{
    return vec2<f32>(f32(i_0) / f32(count_0), radical_inverse_vdc_0(i_0));
}

fn importance_sample_ggx_0( xi_0 : vec2<f32>,  normal_0 : vec3<f32>,  roughness_0 : f32) -> vec3<f32>
{
    var a_1 : f32 = roughness_0 * roughness_0;
    var phi_0 : f32 = 6.28318548202514648f * xi_0.x;
    var _S32 : f32 = xi_0.y;
    var cosTheta_0 : f32 = sqrt(clamp((1.0f - _S32) / (1.0f + (a_1 * a_1 - 1.0f) * _S32), 0.0f, 1.0f));
    var sinTheta_0 : f32 = sqrt(max(1.0f - cosTheta_0 * cosTheta_0, 0.0f));
    var _S33 : f32 = sinTheta_0 * cos(phi_0);
    var _S34 : f32 = sinTheta_0 * sin(phi_0);
    var up_1 : vec3<f32>;
    if((abs(normal_0.z)) < 0.99900001287460327f)
    {
        up_1 = vec3<f32>(0.0f, 0.0f, 1.0f);
    }
    else
    {
        up_1 = vec3<f32>(0.0f, 1.0f, 0.0f);
    }
    var tangent_0 : vec3<f32> = normalize(cross(up_1, normal_0));
    return normalize(vec3<f32>(_S33) * tangent_0 + vec3<f32>(_S34) * cross(normal_0, tangent_0) + vec3<f32>(cosTheta_0) * normal_0);
}

fn distribution_ggx_0( nDotH_0 : f32,  roughness_1 : f32) -> f32
{
    var a_2 : f32 = roughness_1 * roughness_1;
    var a2_0 : f32 = a_2 * a_2;
    var denom_0 : f32 = nDotH_0 * nDotH_0 * (a2_0 - 1.0f) + 1.0f;
    return a2_0 / max(3.14159274101257324f * denom_0 * denom_0, 9.99999997475242708e-07f);
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
fn fs_prefilter( _S35 : pixelInput_4, @builtin(position) svPosition_5 : vec4<f32>) -> pixelOutput_4
{
    var normal_1 : vec3<f32> = cube_direction_0(u32(params_0.face_roughness_samples_mip_0.x), _S35.uv_6);
    var roughness_2 : f32 = params_0.face_roughness_samples_mip_0.y;
    var _S36 : u32 = u32(params_0.face_roughness_samples_mip_0.z);
    var _S37 : f32 = params_0.source_resolution_0.x;
    if(roughness_2 <= 0.0f)
    {
        var _S38 : pixelOutput_4 = pixelOutput_4( (textureSampleLevel((srcCube_0), (srcSampler_0), (normal_1), (0.0f))) );
        return _S38;
    }
    var _S39 : vec3<f32> = vec3<f32>(0.0f);
    var i_1 : u32 = u32(0);
    var color_0 : vec3<f32> = _S39;
    var totalWeight_0 : f32 = 0.0f;
    for(;;)
    {
        if(i_1 < _S36)
        {
        }
        else
        {
            break;
        }
        var halfVector_0 : vec3<f32> = importance_sample_ggx_0(hammersley_0(i_1, _S36), normal_1, roughness_2);
        var _S40 : f32 = dot(normal_1, halfVector_0);
        var sampleDir_0 : vec3<f32> = normalize(vec3<f32>((2.0f * _S40)) * halfVector_0 - normal_1);
        var nDotL_0 : f32 = dot(normal_1, sampleDir_0);
        if(nDotL_0 > 0.0f)
        {
            var totalWeight_1 : f32 = totalWeight_0 + nDotL_0;
            color_0 = color_0 + (textureSampleLevel((srcCube_0), (srcSampler_0), (sampleDir_0), (max(0.5f * log2(1.0f / (f32(_S36) * (distribution_ggx_0(max(_S40, 0.0f), roughness_2) * 0.25f + 0.00009999999747379f)) / (12.56637096405029297f / (6.0f * _S37 * _S37))), 0.0f)))).xyz * vec3<f32>(nDotL_0);
            totalWeight_0 = totalWeight_1;
        }
        i_1 = i_1 + u32(1);
    }
    var _S41 : pixelOutput_4 = pixelOutput_4( vec4<f32>(color_0 / vec3<f32>(max(totalWeight_0, 0.00009999999747379f)), 1.0f) );
    return _S41;
}

fn geometry_smith_ibl_0( nDotV_0 : f32,  nDotL_1 : f32,  roughness_3 : f32) -> f32
{
    var k_0 : f32 = roughness_3 * roughness_3 / 2.0f;
    var _S42 : f32 = 1.0f - k_0;
    return nDotV_0 / max(nDotV_0 * _S42 + k_0, 0.00009999999747379f) * (nDotL_1 / max(nDotL_1 * _S42 + k_0, 0.00009999999747379f));
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
fn fs_brdf_lut( _S43 : pixelInput_5, @builtin(position) svPosition_6 : vec4<f32>) -> pixelOutput_5
{
    var nDotV_1 : f32 = _S43.uv_7.x;
    var _S44 : f32 = _S43.uv_7.y;
    var _S45 : vec3<f32> = vec3<f32>(sqrt(1.0f - nDotV_1 * nDotV_1), 0.0f, nDotV_1);
    const _S46 : vec3<f32> = vec3<f32>(0.0f, 0.0f, 1.0f);
    var _S47 : vec2<f32> = vec2<f32>(0.0f);
    var i_2 : u32 = u32(0);
    var result_0 : vec2<f32> = _S47;
    for(;;)
    {
        if(i_2 < u32(1024))
        {
        }
        else
        {
            break;
        }
        var halfVector_1 : vec3<f32> = importance_sample_ggx_0(hammersley_0(i_2, u32(1024)), _S46, _S44);
        var _S48 : f32 = dot(_S45, halfVector_1);
        var _S49 : f32 = max(normalize(vec3<f32>((2.0f * _S48)) * halfVector_1 - _S45).z, 0.0f);
        if(_S49 > 0.0f)
        {
            var _S50 : f32 = max(_S48, 0.0f);
            var _S51 : f32 = max(nDotV_1, 0.00009999999747379f);
            var gVis_0 : f32 = geometry_smith_ibl_0(_S51, _S49, _S44) * _S50 / max(max(halfVector_1.z, 0.0f) * _S51, 9.99999997475242708e-07f);
            var fc_0 : f32 = pow(1.0f - _S50, 5.0f);
            result_0 = result_0 + vec2<f32>((1.0f - fc_0) * gVis_0, fc_0 * gVis_0);
        }
        i_2 = i_2 + u32(1);
    }
    var _S52 : pixelOutput_5 = pixelOutput_5( vec4<f32>(result_0 / vec2<f32>(1024.0f), 0.0f, 1.0f) );
    return _S52;
}

