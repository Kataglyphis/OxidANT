struct FxaaUniforms_std140_0
{
    @align(16) params_0 : vec4<f32>,
};

@binding(2) @group(0) var<uniform> fxaaUniforms_0 : FxaaUniforms_std140_0;
@binding(0) @group(0) var ldrTex_0 : texture_2d<f32>;

@binding(1) @group(0) var ldrSampler_0 : sampler;

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

fn luma_of_0( rgb_0 : vec3<f32>) -> f32
{
    var luma_0 : f32 = dot(rgb_0, vec3<f32>(0.29899999499320984f, 0.58700001239776611f, 0.11400000005960464f));
    var _S1 : f32;
    if((fxaaUniforms_0.params_0.z) > 0.5f)
    {
        _S1 = sqrt(luma_0);
    }
    else
    {
        _S1 = luma_0;
    }
    return _S1;
}

fn luma_at_0( uv_1 : vec2<f32>) -> f32
{
    return luma_of_0((textureSampleLevel((ldrTex_0), (ldrSampler_0), (uv_1), (0.0f))).xyz);
}

fn search_step_0( i_0 : i32) -> f32
{
    if(i_0 < i32(5))
    {
        return 1.0f;
    }
    if(i_0 == i32(5))
    {
        return 1.5f;
    }
    if(i_0 < i32(10))
    {
        return 2.0f;
    }
    if(i_0 == i32(10))
    {
        return 4.0f;
    }
    return 8.0f;
}

struct EdgeSpan_0
{
     uv_2 : vec2<f32>,
     offset_0 : vec2<f32>,
     stepLength_0 : f32,
     lumaAverage_0 : f32,
     gradientScaled_0 : f32,
};

fn edge_offset_0( uv_3 : vec2<f32>,  span_0 : EdgeSpan_0,  lumaCenter_0 : f32,  horizontal_0 : bool) -> f32
{
    var uv1_0 : vec2<f32> = span_0.uv_2 - span_0.offset_0;
    var uv2_0 : vec2<f32> = span_0.uv_2 + span_0.offset_0;
    var end1_0 : f32 = luma_at_0(uv1_0) - span_0.lumaAverage_0;
    var end2_0 : f32 = luma_at_0(uv2_0) - span_0.lumaAverage_0;
    var _S2 : bool = (abs(end2_0)) >= (span_0.gradientScaled_0);
    var reached1_0 : bool = (abs(end1_0)) >= (span_0.gradientScaled_0);
    var reached2_0 : bool = _S2;
    var end1_1 : f32 = end1_0;
    var end2_1 : f32 = end2_0;
    var i_1 : i32 = i32(1);
    var uv1_1 : vec2<f32> = uv1_0;
    var uv2_1 : vec2<f32> = uv2_0;
    for(;;)
    {
        var _S3 : bool;
        if(i_1 < i32(12))
        {
            if(reached1_0)
            {
                _S3 = reached2_0;
            }
            else
            {
                _S3 = false;
            }
            _S3 = !_S3;
        }
        else
        {
            _S3 = false;
        }
        if(_S3)
        {
        }
        else
        {
            break;
        }
        if(!reached1_0)
        {
            var uv1_2 : vec2<f32> = uv1_1 - span_0.offset_0 * vec2<f32>(search_step_0(i_1));
            var end1_2 : f32 = luma_at_0(uv1_2) - span_0.lumaAverage_0;
            reached1_0 = (abs(end1_2)) >= (span_0.gradientScaled_0);
            end1_1 = end1_2;
            uv1_1 = uv1_2;
        }
        if(!reached2_0)
        {
            var uv2_2 : vec2<f32> = uv2_1 + span_0.offset_0 * vec2<f32>(search_step_0(i_1));
            var end2_2 : f32 = luma_at_0(uv2_2) - span_0.lumaAverage_0;
            reached2_0 = (abs(end2_2)) >= (span_0.gradientScaled_0);
            end2_1 = end2_2;
            uv2_1 = uv2_2;
        }
        i_1 = i_1 + i32(1);
    }
    var distance1_0 : f32;
    if(horizontal_0)
    {
        distance1_0 = uv_3.x - uv1_1.x;
    }
    else
    {
        distance1_0 = uv_3.y - uv1_1.y;
    }
    var distance2_0 : f32;
    if(horizontal_0)
    {
        distance2_0 = uv2_1.x - uv_3.x;
    }
    else
    {
        distance2_0 = uv2_1.y - uv_3.y;
    }
    var pixelOffset_0 : f32 = 0.5f - min(distance1_0, distance2_0) / (distance1_0 + distance2_0);
    var centerSmaller_0 : bool = lumaCenter_0 < (span_0.lumaAverage_0);
    if(distance1_0 < distance2_0)
    {
    }
    else
    {
        end1_1 = end2_1;
    }
    if((end1_1 < 0.0f) != centerSmaller_0)
    {
        end1_1 = pixelOffset_0;
    }
    else
    {
        end1_1 = 0.0f;
    }
    return end1_1;
}

struct pixelOutput_0
{
    @location(0) output_0 : vec4<f32>,
};

struct pixelInput_0
{
    @location(0) uv_4 : vec2<f32>,
};

@fragment
fn fs_main( _S4 : pixelInput_0, @builtin(position) svPosition_1 : vec4<f32>) -> pixelOutput_0
{
    var rcp_0 : vec2<f32> = fxaaUniforms_0.params_0.xy;
    var _S5 : vec3<i32> = vec3<i32>(vec2<i32>(svPosition_1.xy), i32(0));
    var _S6 : vec3<f32> = (textureLoad((ldrTex_0), ((_S5)).xy, ((_S5)).z)).xyz;
    var lumaCenter_1 : f32 = luma_of_0(_S6);
    var _S7 : f32 = rcp_0.y;
    var _S8 : vec2<f32> = vec2<f32>(0.0f, _S7);
    var lumaDown_0 : f32 = luma_at_0(_S4.uv_4 + _S8);
    var lumaUp_0 : f32 = luma_at_0(_S4.uv_4 - _S8);
    var _S9 : f32 = rcp_0.x;
    var _S10 : vec2<f32> = vec2<f32>(_S9, 0.0f);
    var lumaLeft_0 : f32 = luma_at_0(_S4.uv_4 - _S10);
    var lumaRight_0 : f32 = luma_at_0(_S4.uv_4 + _S10);
    var _S11 : f32 = max(lumaCenter_1, max(max(lumaDown_0, lumaUp_0), max(lumaLeft_0, lumaRight_0)));
    var range_0 : f32 = _S11 - min(lumaCenter_1, min(min(lumaDown_0, lumaUp_0), min(lumaLeft_0, lumaRight_0)));
    if(range_0 < (max(0.03119999915361404f, _S11 * 0.125f)))
    {
        var _S12 : pixelOutput_0 = pixelOutput_0( vec4<f32>(_S6, 1.0f) );
        return _S12;
    }
    var lumaDownLeft_0 : f32 = luma_at_0(_S4.uv_4 + vec2<f32>(- _S9, _S7));
    var lumaUpRight_0 : f32 = luma_at_0(_S4.uv_4 + vec2<f32>(_S9, - _S7));
    var lumaUpLeft_0 : f32 = luma_at_0(_S4.uv_4 - rcp_0);
    var lumaDownRight_0 : f32 = luma_at_0(_S4.uv_4 + rcp_0);
    var downUp_0 : f32 = lumaDown_0 + lumaUp_0;
    var leftRight_0 : f32 = lumaLeft_0 + lumaRight_0;
    var leftCorners_0 : f32 = lumaDownLeft_0 + lumaUpLeft_0;
    var rightCorners_0 : f32 = lumaDownRight_0 + lumaUpRight_0;
    var _S13 : f32 = -2.0f * lumaCenter_1;
    var horizontal_1 : bool = (abs(-2.0f * lumaLeft_0 + leftCorners_0) + abs(_S13 + downUp_0) * 2.0f + abs(-2.0f * lumaRight_0 + rightCorners_0)) >= (abs(-2.0f * lumaUp_0 + (lumaUpRight_0 + lumaUpLeft_0)) + abs(_S13 + leftRight_0) * 2.0f + abs(-2.0f * lumaDown_0 + (lumaDownLeft_0 + lumaDownRight_0)));
    var luma1_0 : f32;
    if(horizontal_1)
    {
        luma1_0 = lumaUp_0;
    }
    else
    {
        luma1_0 = lumaLeft_0;
    }
    var luma2_0 : f32;
    if(horizontal_1)
    {
        luma2_0 = lumaDown_0;
    }
    else
    {
        luma2_0 = lumaRight_0;
    }
    var _S14 : f32 = abs(luma1_0 - lumaCenter_1);
    var _S15 : f32 = abs(luma2_0 - lumaCenter_1);
    var steepest1_0 : bool = _S14 >= _S15;
    var span_1 : EdgeSpan_0;
    span_1.gradientScaled_0 = 0.25f * max(_S14, _S15);
    var _S16 : f32;
    if(horizontal_1)
    {
        _S16 = _S7;
    }
    else
    {
        _S16 = _S9;
    }
    span_1.stepLength_0 = _S16;
    if(steepest1_0)
    {
    }
    else
    {
        luma1_0 = luma2_0;
    }
    span_1.lumaAverage_0 = 0.5f * (luma1_0 + lumaCenter_1);
    if(steepest1_0)
    {
        span_1.stepLength_0 = - span_1.stepLength_0;
    }
    span_1.uv_2 = _S4.uv_4;
    if(horizontal_1)
    {
        span_1.uv_2[i32(1)] = span_1.uv_2[i32(1)] + span_1.stepLength_0 * 0.5f;
    }
    else
    {
        span_1.uv_2[i32(0)] = span_1.uv_2[i32(0)] + span_1.stepLength_0 * 0.5f;
    }
    var _S17 : vec2<f32>;
    if(horizontal_1)
    {
        _S17 = _S10;
    }
    else
    {
        _S17 = _S8;
    }
    span_1.offset_0 = _S17;
    var subpixel_0 : f32 = saturate(abs((2.0f * (downUp_0 + leftRight_0) + leftCorners_0 + rightCorners_0) / 12.0f - lumaCenter_1) / range_0);
    var subpixel_1 : f32 = (-2.0f * subpixel_0 + 3.0f) * subpixel_0 * subpixel_0;
    var _S18 : f32 = max(edge_offset_0(_S4.uv_4, span_1, lumaCenter_1, horizontal_1), subpixel_1 * subpixel_1 * 0.75f);
    var finalUv_0 : vec2<f32> = _S4.uv_4;
    if(horizontal_1)
    {
        finalUv_0[i32(1)] = finalUv_0[i32(1)] + _S18 * span_1.stepLength_0;
    }
    else
    {
        finalUv_0[i32(0)] = finalUv_0[i32(0)] + _S18 * span_1.stepLength_0;
    }
    var _S19 : pixelOutput_0 = pixelOutput_0( vec4<f32>((textureSampleLevel((ldrTex_0), (ldrSampler_0), (finalUv_0), (0.0f))).xyz, 1.0f) );
    return _S19;
}

