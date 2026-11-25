struct _MatrixStorage_float4x4_ColMajorstd140_0
{
    @align(16) data_0 : array<vec4<f32>, i32(4)>,
};

struct CloudParams_std140_0
{
    @align(16) cloudMeshScale_0 : vec4<f32>,
    @align(16) cloudMeshOffset_0 : vec4<f32>,
    @align(16) cloudParameters_0 : vec4<f32>,
    @align(16) cloudLightMarch_0 : vec4<f32>,
};

struct CloudFrame_std140_0
{
    @align(16) inv_view_proj_0 : _MatrixStorage_float4x4_ColMajorstd140_0,
    @align(16) eye_0 : vec4<f32>,
    @align(16) to_sun_0 : vec4<f32>,
    @align(16) sun_radiance_0 : vec4<f32>,
    @align(16) params_0 : CloudParams_std140_0,
    @align(16) depth_spread_0 : vec4<f32>,
    @align(16) ambient_top_0 : vec4<f32>,
    @align(16) ambient_bottom_0 : vec4<f32>,
};

@binding(0) @group(0) var<uniform> frame_0 : CloudFrame_std140_0;
@binding(3) @group(0) var sceneDepth_0 : texture_2d<f32>;

@binding(4) @group(0) var depthSpread_0 : texture_2d<f32>;

@binding(5) @group(0) var halfClouds_0 : texture_2d<f32>;

@binding(1) @group(0) var noiseTex_0 : texture_3d<f32>;

@binding(2) @group(0) var noiseSampler_0 : sampler;

struct PixelTexels_0
{
     ndc_0 : vec2<f32>,
};

fn PixelTexels_scene_depth_0( this_0 : PixelTexels_0,  pixel_0 : vec2<i32>) -> f32
{
    var _S1 : vec3<i32> = vec3<i32>(pixel_0, i32(0));
    return (textureLoad((sceneDepth_0), ((_S1)).xy, ((_S1)).z)).x;
}

fn cloud_scene_distance_0( ndcToSpace_0 : mat4x4<f32>,  ndc_1 : vec2<f32>,  depth_0 : f32,  eye_1 : vec3<f32>) -> f32
{
    if(depth_0 == 1.0f)
    {
        return 3.00000000549775576e+38f;
    }
    var position_0 : vec4<f32> = (((vec4<f32>(ndc_1, depth_0, 1.0f)) * (ndcToSpace_0)));
    return length(position_0.xyz / vec3<f32>(position_0.w) - eye_1);
}

fn PixelTexels_scene_distance_0( this_1 : PixelTexels_0,  depth_1 : f32) -> f32
{
    return cloud_scene_distance_0(mat4x4<f32>(frame_0.inv_view_proj_0.data_0[i32(0)][i32(0)], frame_0.inv_view_proj_0.data_0[i32(1)][i32(0)], frame_0.inv_view_proj_0.data_0[i32(2)][i32(0)], frame_0.inv_view_proj_0.data_0[i32(3)][i32(0)], frame_0.inv_view_proj_0.data_0[i32(0)][i32(1)], frame_0.inv_view_proj_0.data_0[i32(1)][i32(1)], frame_0.inv_view_proj_0.data_0[i32(2)][i32(1)], frame_0.inv_view_proj_0.data_0[i32(3)][i32(1)], frame_0.inv_view_proj_0.data_0[i32(0)][i32(2)], frame_0.inv_view_proj_0.data_0[i32(1)][i32(2)], frame_0.inv_view_proj_0.data_0[i32(2)][i32(2)], frame_0.inv_view_proj_0.data_0[i32(3)][i32(2)], frame_0.inv_view_proj_0.data_0[i32(0)][i32(3)], frame_0.inv_view_proj_0.data_0[i32(1)][i32(3)], frame_0.inv_view_proj_0.data_0[i32(2)][i32(3)], frame_0.inv_view_proj_0.data_0[i32(3)][i32(3)]), this_1.ndc_0, depth_1, frame_0.eye_0.xyz);
}

fn PixelTexels_half_cloud_0( this_2 : PixelTexels_0,  texel_0 : vec2<i32>) -> vec4<f32>
{
    var _S2 : vec3<i32> = vec3<i32>(texel_0, i32(0));
    return (textureLoad((halfClouds_0), ((_S2)).xy, ((_S2)).z));
}

fn half_texel_depth_0( texels_0 : PixelTexels_0,  texel_1 : vec2<i32>,  depthSize_0 : vec2<i32>) -> f32
{
    var _S3 : vec2<i32> = depthSize_0 - vec2<i32>(i32(1));
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
        var _S4 : f32 = min(nearest_0, PixelTexels_scene_depth_0(texels_0, min(texel_1 * vec2<i32>(i32(2)) + vec2<i32>((i_0 & (i32(1))), (i_0 >> (u32(1)))), _S3)));
        var _S5 : i32 = i_0 + i32(1);
        nearest_0 = _S4;
        i_0 = _S5;
    }
    return nearest_0;
}

fn cloud_upsample_depth_weight_0( pixelDistance_0 : f32,  texelDistance_0 : f32) -> f32
{
    return exp(- min(abs(pixelDistance_0 - texelDistance_0) / max(min(pixelDistance_0, texelDistance_0), 0.00009999999747379f), 100.0f) * 16.0f);
}

fn cloud_upsample_0( texels_1 : PixelTexels_0,  pixel_1 : vec2<i32>,  halfSize_0 : vec2<i32>,  depthSize_1 : vec2<i32>) -> vec4<f32>
{
    var _S6 : f32 = PixelTexels_scene_distance_0(texels_1, PixelTexels_scene_depth_0(texels_1, pixel_1));
    var _S7 : vec2<i32> = halfSize_0 - vec2<i32>(i32(1));
    var _S8 : vec2<f32> = vec2<f32>(0.5f);
    var position_1 : vec2<f32> = (vec2<f32>(pixel_1) + _S8) * _S8 - _S8;
    var _S9 : vec2<i32> = vec2<i32>(floor(position_1));
    var _S10 : vec2<f32> = position_1 - vec2<f32>(_S9);
    var _S11 : vec4<f32> = vec4<f32>(0.0f);
    var i_1 : i32 = i32(0);
    var weighted_0 : vec4<f32> = _S11;
    var weightSum_0 : f32 = 0.0f;
    var bilinear_0 : vec4<f32> = _S11;
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
        var texel_2 : vec2<i32> = clamp(_S9 + corner_0, vec2<i32>(i32(0)), _S7);
        var axis_0 : vec2<f32> = mix(vec2<f32>(1.0f) - _S10, _S10, vec2<f32>(corner_0));
        var w_0 : f32 = axis_0.x * axis_0.y;
        var value_0 : vec4<f32> = PixelTexels_half_cloud_0(texels_1, texel_2);
        var depthWeight_0 : f32 = w_0 * cloud_upsample_depth_weight_0(_S6, PixelTexels_scene_distance_0(texels_1, half_texel_depth_0(texels_1, texel_2, depthSize_1)));
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
fn vs_main(@builtin(vertex_index) vid_1 : u32) -> FullscreenVsOut_0
{
    return fullscreen_vs_0(vid_1);
}

fn fullscreen_uv_to_ndc_0( uv_1 : vec2<f32>) -> vec2<f32>
{
    return vec2<f32>(uv_1.x * 2.0f - 1.0f, 1.0f - uv_1.y * 2.0f);
}

struct CameraRay_0
{
     origin_0 : vec3<f32>,
     direction_0 : vec3<f32>,
     length_0 : f32,
};

fn camera_ray_0( clip_to_world_0 : mat4x4<f32>,  ndc_2 : vec2<f32>) -> CameraRay_0
{
    var near_0 : vec4<f32> = (((vec4<f32>(ndc_2, 0.0f, 1.0f)) * (clip_to_world_0)));
    var far_0 : vec4<f32> = (((vec4<f32>(ndc_2, 1.0f, 1.0f)) * (clip_to_world_0)));
    var ray_0 : CameraRay_0;
    var _S12 : vec3<f32> = near_0.xyz / vec3<f32>(near_0.w);
    ray_0.origin_0 = _S12;
    var span_0 : vec3<f32> = far_0.xyz / vec3<f32>(far_0.w) - _S12;
    ray_0.direction_0 = normalize(span_0);
    ray_0.length_0 = length(span_0);
    return ray_0;
}

fn PixelTexels_x24init_0( ndc_3 : vec2<f32>) -> PixelTexels_0
{
    var _S13 : PixelTexels_0;
    _S13.ndc_0 = ndc_3;
    return _S13;
}

fn scene_depth_size_0() -> vec2<i32>
{
    var width_0 : u32;
    var height_0 : u32;
    {var dim = textureDimensions((sceneDepth_0));((width_0)) = dim.x;((height_0)) = dim.y;};
    return vec2<i32>(i32(width_0), i32(height_0));
}

struct Clouds_0
{
     num_march_steps_0 : i32,
     num_march_steps_to_light_0 : i32,
     radius_0 : vec3<f32>,
     offset_0 : vec3<f32>,
     threshold_0 : f32,
     scale_0 : f32,
     pillowness_0 : f32,
     cirrus_effect_0 : f32,
     powder_effect_0 : bool,
     wind_offset_0 : vec3<f32>,
};

fn cloud_unpack_0( scene_0 : ptr<function, CloudParams_std140_0>) -> Clouds_0
{
    var cloud_0 : Clouds_0;
    cloud_0.num_march_steps_0 = i32(clamp((*scene_0).cloudParameters_0.w, 4.0f, 128.0f));
    cloud_0.num_march_steps_to_light_0 = i32(clamp((*scene_0).cloudLightMarch_0.x, 1.0f, 128.0f));
    cloud_0.scale_0 = (*scene_0).cloudMeshScale_0.w;
    cloud_0.threshold_0 = (*scene_0).cloudMeshOffset_0.w;
    cloud_0.pillowness_0 = (*scene_0).cloudParameters_0.x;
    cloud_0.cirrus_effect_0 = (*scene_0).cloudParameters_0.y;
    cloud_0.powder_effect_0 = ((*scene_0).cloudParameters_0.z) > 0.5f;
    cloud_0.radius_0 = (*scene_0).cloudMeshScale_0.xyz * vec3<f32>(10.0f);
    cloud_0.offset_0 = (*scene_0).cloudMeshOffset_0.xyz;
    cloud_0.wind_offset_0 = (*scene_0).cloudLightMarch_0.yzw;
    return cloud_0;
}

fn box_intersect_0( ro_0 : vec3<f32>,  rd_0 : vec3<f32>,  cloud_1 : Clouds_0,  oT_0 : ptr<function, vec2<f32>>) -> bool
{
    (*oT_0) = vec2<f32>(0.0f, 0.0f);
    var m_0 : vec3<f32> = vec3<f32>(1.0f) / select(rd_0, vec3<f32>(1.00000000317107685e-30f), rd_0 == vec3<f32>(0.0f));
    var k_0 : vec3<f32> = abs(m_0) * cloud_1.radius_0;
    var _S14 : vec3<f32> = (vec3<f32>(0) - m_0 * (ro_0 - cloud_1.offset_0));
    var t1_0 : vec3<f32> = _S14 - k_0;
    var t2_0 : vec3<f32> = _S14 + k_0;
    var _S15 : f32 = max(max(t1_0.x, t1_0.y), t1_0.z);
    var _S16 : f32 = min(min(t2_0.x, t2_0.y), t2_0.z);
    var _S17 : bool;
    if(_S15 > _S16)
    {
        _S17 = true;
    }
    else
    {
        _S17 = _S16 < 0.0f;
    }
    if(_S17)
    {
        return false;
    }
    (*oT_0) = vec2<f32>(_S15, _S16);
    return true;
}

fn box_intersect_1( ro_1 : vec3<f32>,  rd_1 : vec3<f32>,  cloud_2 : Clouds_0,  oT_1 : ptr<function, vec2<f32>>) -> bool
{
    (*oT_1) = vec2<f32>(0.0f, 0.0f);
    var m_1 : vec3<f32> = vec3<f32>(1.0f) / select(rd_1, vec3<f32>(1.00000000317107685e-30f), rd_1 == vec3<f32>(0.0f));
    var k_1 : vec3<f32> = abs(m_1) * cloud_2.radius_0;
    var _S18 : vec3<f32> = (vec3<f32>(0) - m_1 * (ro_1 - cloud_2.offset_0));
    var t1_1 : vec3<f32> = _S18 - k_1;
    var t2_1 : vec3<f32> = _S18 + k_1;
    var _S19 : f32 = max(max(t1_1.x, t1_1.y), t1_1.z);
    var _S20 : f32 = min(min(t2_1.x, t2_1.y), t2_1.z);
    var _S21 : bool;
    if(_S19 > _S20)
    {
        _S21 = true;
    }
    else
    {
        _S21 = _S20 < 0.0f;
    }
    if(_S21)
    {
        return false;
    }
    (*oT_1) = vec2<f32>(_S19, _S20);
    return true;
}

fn cloud_thinnest_extent_0( cloud_3 : Clouds_0) -> f32
{
    return 2.0f * min(min(cloud_3.radius_0.x, cloud_3.radius_0.y), cloud_3.radius_0.z);
}

fn cloud_march_steps_0( cloud_4 : Clouds_0,  marchLength_0 : f32) -> i32
{
    var _S22 : f32 = f32(cloud_4.num_march_steps_0);
    return i32(clamp(ceil(_S22 * marchLength_0 / cloud_thinnest_extent_0(cloud_4)), _S22, min(f32(cloud_4.num_march_steps_0 * i32(4)), 128.0f)));
}

fn SampledNoise_sample_noise_0( uvw_0 : vec3<f32>) -> vec4<f32>
{
    return (textureSampleLevel((noiseTex_0), (noiseSampler_0), (uvw_0), (0.0f)));
}

fn sample_density_0( position_2 : vec3<f32>,  cloud_5 : Clouds_0) -> f32
{
    var local_0 : vec3<f32> = position_2 - cloud_5.offset_0 - cloud_5.wind_offset_0;
    var _S23 : vec3<f32> = vec3<f32>(0.5f);
    var _S24 : vec4<f32> = SampledNoise_sample_noise_0(fract(local_0 / vec3<f32>(256.0f) + _S23));
    var _S25 : f32 = (1.0f - cloud_5.pillowness_0) * 0.125f;
    var _S26 : vec4<f32> = SampledNoise_sample_noise_0(fract(local_0 / vec3<f32>(64.0f) + _S23));
    return (cloud_5.pillowness_0 * 0.25f * max(0.0f, _S24.x - cloud_5.threshold_0) * cloud_5.scale_0 + _S25 * max(0.0f, _S24.y - cloud_5.threshold_0) * cloud_5.scale_0 + _S25 * max(0.0f, _S24.z - cloud_5.threshold_0) * cloud_5.scale_0) * (1.0f - cloud_5.cirrus_effect_0) + max(0.0f, _S24.w - cloud_5.threshold_0) * cloud_5.scale_0 * cloud_5.cirrus_effect_0 + (0.60000002384185791f * max(0.0f, _S26.x - cloud_5.threshold_0) * cloud_5.scale_0 + 0.15000000596046448f * max(0.0f, _S26.y - cloud_5.threshold_0) * cloud_5.scale_0 + 0.15000000596046448f * max(0.0f, _S26.z - cloud_5.threshold_0) * cloud_5.scale_0) * 0.30000001192092896f;
}

fn light_optical_depth_0( samplePos_0 : vec3<f32>,  directionToLight_0 : vec3<f32>,  cloud_6 : Clouds_0) -> f32
{
    var oT_2 : vec2<f32>;
    var intersection_0 : bool = box_intersect_0(samplePos_0, directionToLight_0, cloud_6, &(oT_2));
    if(!intersection_0)
    {
        return 0.0f;
    }
    var _S27 : f32 = max(oT_2.x, 0.0f);
    var _S28 : f32 = (oT_2.y - _S27) / f32(cloud_6.num_march_steps_to_light_0);
    var i_2 : i32 = i32(0);
    var totalDensity_0 : f32 = 0.0f;
    for(;;)
    {
        if(i_2 < (cloud_6.num_march_steps_to_light_0))
        {
        }
        else
        {
            break;
        }
        var totalDensity_1 : f32 = totalDensity_0 + sample_density_0(samplePos_0 + directionToLight_0 * vec3<f32>((_S27 + (f32(i_2) + 0.5f) * _S28)), cloud_6) * _S28;
        i_2 = i_2 + i32(1);
        totalDensity_0 = totalDensity_1;
    }
    return totalDensity_0;
}

fn phase_HG_0( cosTheta_0 : f32,  g_0 : f32) -> f32
{
    var _S29 : f32 = g_0 * g_0;
    var denom_0 : f32 = 1.0f + _S29 - 2.0f * g_0 * cosTheta_0;
    return 0.07957746833562851f * ((1.0f - _S29) / (denom_0 * sqrt(denom_0)));
}

fn cloud_sun_scattering_0( cosTheta_1 : f32,  lightDepth_0 : f32) -> f32
{
    var octave_0 : i32 = i32(0);
    var weight_0 : f32 = 1.0f;
    var g_1 : f32 = 0.5f;
    var depthScale_0 : f32 = 1.0f;
    var energy_0 : f32 = 0.0f;
    for(;;)
    {
        if(octave_0 < i32(3))
        {
        }
        else
        {
            break;
        }
        var energy_1 : f32 = energy_0 + weight_0 * phase_HG_0(cosTheta_1, g_1) * exp(- lightDepth_0 * depthScale_0);
        var weight_1 : f32 = weight_0 * 0.5f;
        var depthScale_1 : f32 = depthScale_0 * 0.5f;
        var g_2 : f32 = g_1 * 0.5f;
        octave_0 = octave_0 + i32(1);
        weight_0 = weight_1;
        g_1 = g_2;
        depthScale_0 = depthScale_1;
        energy_0 = energy_1;
    }
    return energy_0;
}

struct CloudAmbient_0
{
     top_0 : vec3<f32>,
     bottom_0 : vec3<f32>,
};

fn cloud_ambient_at_0( ambient_0 : CloudAmbient_0,  cloud_7 : Clouds_0,  position_3 : vec3<f32>) -> vec3<f32>
{
    return mix(ambient_0.bottom_0, ambient_0.top_0, vec3<f32>(saturate((position_3.y - cloud_7.offset_0.y) / (2.0f * cloud_7.radius_0.y) + 0.5f)));
}

fn march_clouds_0( eyePosition_0 : vec3<f32>,  rayDirection_0 : vec3<f32>,  sceneDistance_0 : f32,  settings_0 : Clouds_0,  directionToLight_1 : vec3<f32>,  sunRadiance_0 : vec3<f32>,  ambient_1 : CloudAmbient_0) -> vec4<f32>
{
    var toLight_0 : vec3<f32> = normalize(directionToLight_1);
    var cloud_8 : Clouds_0 = settings_0;
    var oT_3 : vec2<f32>;
    var intersection_1 : bool = box_intersect_1(eyePosition_0, rayDirection_0, settings_0, &(oT_3));
    oT_3[i32(1)] = min(oT_3.y, sceneDistance_0);
    var _S30 : vec3<f32> = vec3<f32>(0.0f);
    var _S31 : f32 = dot(normalize(rayDirection_0), toLight_0);
    var _S32 : bool;
    if(intersection_1)
    {
        _S32 = (oT_3.y) > (max(oT_3.x, 0.0f));
    }
    else
    {
        _S32 = false;
    }
    var transmittance_0 : f32;
    var radiance_0 : vec3<f32>;
    if(_S32)
    {
        oT_3[i32(0)] = max(0.0f, oT_3.x);
        var marchLength_1 : f32 = oT_3.y - oT_3.x;
        var _S33 : f32 = cloud_thinnest_extent_0(cloud_8) / f32(cloud_8.num_march_steps_0);
        var _S34 : i32 = cloud_march_steps_0(settings_0, marchLength_1);
        cloud_8.num_march_steps_0 = _S34;
        var _S35 : f32 = marchLength_1 / f32(_S34);
        var i_3 : i32 = i32(0);
        transmittance_0 = 1.0f;
        radiance_0 = _S30;
        for(;;)
        {
            if(i_3 < (cloud_8.num_march_steps_0))
            {
            }
            else
            {
                break;
            }
            var samplePos_1 : vec3<f32> = eyePosition_0 + rayDirection_0 * vec3<f32>((oT_3.x + (f32(i_3) + 0.5f) * _S35));
            var _S36 : f32 = sample_density_0(samplePos_1, cloud_8);
            var _S37 : f32 = light_optical_depth_0(samplePos_1, toLight_0, cloud_8);
            var powder_0 : f32;
            if(cloud_8.powder_effect_0)
            {
                powder_0 = 1.0f - exp(- _S36 * _S33 * 2.0f);
            }
            else
            {
                powder_0 = 1.0f;
            }
            var _S38 : f32 = exp(- _S36 * _S35);
            var radiance_1 : vec3<f32> = radiance_0 + vec3<f32>((transmittance_0 * (1.0f - _S38))) * (sunRadiance_0 * vec3<f32>((cloud_sun_scattering_0(_S31, _S37) * powder_0)) + cloud_ambient_at_0(ambient_1, cloud_8, samplePos_1));
            var transmittance_1 : f32 = transmittance_0 * _S38;
            if(transmittance_1 < 0.00999999977648258f)
            {
                radiance_0 = radiance_1;
                transmittance_0 = transmittance_1;
                break;
            }
            i_3 = i_3 + i32(1);
            transmittance_0 = transmittance_1;
            radiance_0 = radiance_1;
        }
    }
    else
    {
        radiance_0 = _S30;
        transmittance_0 = 1.0f;
    }
    return vec4<f32>(radiance_0, 1.0f - transmittance_0);
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
fn fs_main( _S39 : pixelInput_0, @builtin(position) svPosition_1 : vec4<f32>) -> pixelOutput_0
{
    var ndc_4 : vec2<f32> = fullscreen_uv_to_ndc_0(_S39.uv_2);
    var _S40 : CameraRay_0 = camera_ray_0(mat4x4<f32>(frame_0.inv_view_proj_0.data_0[i32(0)][i32(0)], frame_0.inv_view_proj_0.data_0[i32(1)][i32(0)], frame_0.inv_view_proj_0.data_0[i32(2)][i32(0)], frame_0.inv_view_proj_0.data_0[i32(3)][i32(0)], frame_0.inv_view_proj_0.data_0[i32(0)][i32(1)], frame_0.inv_view_proj_0.data_0[i32(1)][i32(1)], frame_0.inv_view_proj_0.data_0[i32(2)][i32(1)], frame_0.inv_view_proj_0.data_0[i32(3)][i32(1)], frame_0.inv_view_proj_0.data_0[i32(0)][i32(2)], frame_0.inv_view_proj_0.data_0[i32(1)][i32(2)], frame_0.inv_view_proj_0.data_0[i32(2)][i32(2)], frame_0.inv_view_proj_0.data_0[i32(3)][i32(2)], frame_0.inv_view_proj_0.data_0[i32(0)][i32(3)], frame_0.inv_view_proj_0.data_0[i32(1)][i32(3)], frame_0.inv_view_proj_0.data_0[i32(2)][i32(3)], frame_0.inv_view_proj_0.data_0[i32(3)][i32(3)]), ndc_4);
    var pixel_2 : vec3<i32> = vec3<i32>(vec2<i32>(svPosition_1.xy), i32(0));
    var texels_2 : PixelTexels_0 = PixelTexels_x24init_0(ndc_4);
    var depth_2 : f32;
    if((frame_0.depth_spread_0.y) > 0.5f)
    {
        depth_2 = half_texel_depth_0(texels_2, pixel_2.xy, scene_depth_size_0());
    }
    else
    {
        depth_2 = (textureLoad((sceneDepth_0), ((pixel_2)).xy, ((pixel_2)).z)).x;
    }
    var sceneDistance_1 : f32 = cloud_scene_distance_0(mat4x4<f32>(frame_0.inv_view_proj_0.data_0[i32(0)][i32(0)], frame_0.inv_view_proj_0.data_0[i32(1)][i32(0)], frame_0.inv_view_proj_0.data_0[i32(2)][i32(0)], frame_0.inv_view_proj_0.data_0[i32(3)][i32(0)], frame_0.inv_view_proj_0.data_0[i32(0)][i32(1)], frame_0.inv_view_proj_0.data_0[i32(1)][i32(1)], frame_0.inv_view_proj_0.data_0[i32(2)][i32(1)], frame_0.inv_view_proj_0.data_0[i32(3)][i32(1)], frame_0.inv_view_proj_0.data_0[i32(0)][i32(2)], frame_0.inv_view_proj_0.data_0[i32(1)][i32(2)], frame_0.inv_view_proj_0.data_0[i32(2)][i32(2)], frame_0.inv_view_proj_0.data_0[i32(3)][i32(2)], frame_0.inv_view_proj_0.data_0[i32(0)][i32(3)], frame_0.inv_view_proj_0.data_0[i32(1)][i32(3)], frame_0.inv_view_proj_0.data_0[i32(2)][i32(3)], frame_0.inv_view_proj_0.data_0[i32(3)][i32(3)]), ndc_4, depth_2, frame_0.eye_0.xyz);
    var _S41 : CloudParams_std140_0 = frame_0.params_0;
    var _S42 : Clouds_0 = cloud_unpack_0(&(_S41));
    var ambient_2 : CloudAmbient_0;
    ambient_2.top_0 = frame_0.ambient_top_0.xyz;
    ambient_2.bottom_0 = frame_0.ambient_bottom_0.xyz;
    var _S43 : vec4<f32> = march_clouds_0(frame_0.eye_0.xyz, _S40.direction_0, sceneDistance_1, _S42, frame_0.to_sun_0.xyz, frame_0.sun_radiance_0.xyz, ambient_2);
    if((frame_0.depth_spread_0.x) < 0.5f)
    {
        var _S44 : pixelOutput_0 = pixelOutput_0( _S43 );
        return _S44;
    }
    var spread_0 : vec2<f32> = (textureLoad((depthSpread_0), ((pixel_2)).xy, ((pixel_2)).z)).xy;
    var _S45 : f32 = spread_0.y;
    var _S46 : bool;
    if(_S45 <= 0.0f)
    {
        _S46 = true;
    }
    else
    {
        _S46 = (spread_0.x) <= depth_2;
    }
    if(_S46)
    {
        var _S47 : pixelOutput_0 = pixelOutput_0( _S43 );
        return _S47;
    }
    var _S48 : pixelOutput_0 = pixelOutput_0( mix(_S43, march_clouds_0(frame_0.eye_0.xyz, _S40.direction_0, cloud_scene_distance_0(mat4x4<f32>(frame_0.inv_view_proj_0.data_0[i32(0)][i32(0)], frame_0.inv_view_proj_0.data_0[i32(1)][i32(0)], frame_0.inv_view_proj_0.data_0[i32(2)][i32(0)], frame_0.inv_view_proj_0.data_0[i32(3)][i32(0)], frame_0.inv_view_proj_0.data_0[i32(0)][i32(1)], frame_0.inv_view_proj_0.data_0[i32(1)][i32(1)], frame_0.inv_view_proj_0.data_0[i32(2)][i32(1)], frame_0.inv_view_proj_0.data_0[i32(3)][i32(1)], frame_0.inv_view_proj_0.data_0[i32(0)][i32(2)], frame_0.inv_view_proj_0.data_0[i32(1)][i32(2)], frame_0.inv_view_proj_0.data_0[i32(2)][i32(2)], frame_0.inv_view_proj_0.data_0[i32(3)][i32(2)], frame_0.inv_view_proj_0.data_0[i32(0)][i32(3)], frame_0.inv_view_proj_0.data_0[i32(1)][i32(3)], frame_0.inv_view_proj_0.data_0[i32(2)][i32(3)], frame_0.inv_view_proj_0.data_0[i32(3)][i32(3)]), ndc_4, spread_0.x, frame_0.eye_0.xyz), _S42, frame_0.to_sun_0.xyz, frame_0.sun_radiance_0.xyz, ambient_2), vec4<f32>(_S45)) );
    return _S48;
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
fn fs_upsample( _S49 : pixelInput_1, @builtin(position) svPosition_2 : vec4<f32>) -> pixelOutput_1
{
    var texels_3 : PixelTexels_0 = PixelTexels_x24init_0(fullscreen_uv_to_ndc_0(_S49.uv_3));
    var width_1 : u32;
    var height_1 : u32;
    {var dim = textureDimensions((halfClouds_0));((width_1)) = dim.x;((height_1)) = dim.y;};
    var _S50 : pixelOutput_1 = pixelOutput_1( cloud_upsample_0(texels_3, vec2<i32>(svPosition_2.xy), vec2<i32>(i32(width_1), i32(height_1)), scene_depth_size_0()) );
    return _S50;
}

