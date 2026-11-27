@binding(0) @group(0) var srcTex_0 : texture_2d<f32>;

@binding(1) @group(0) var srcSampler_0 : sampler;

struct BloomUniforms_std140_0
{
    @align(16) params_0 : vec4<f32>,
};

@binding(4) @group(0) var<uniform> bloomUniforms_0 : BloomUniforms_std140_0;
@binding(3) @group(0) var cloudTex_0 : texture_2d<f32>;

@binding(2) @group(0) var<storage, read> exposureState_0 : array<f32>;

var<private> BLOOM_WEIGHTS_0 : array<f32, i32(5)> = array<f32, i32(5)>( 0.22702699899673462f, 0.194594606757164f, 0.12162160128355026f, 0.05405399948358536f, 0.01621600054204464f );
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

fn hdr_safe_0( radiance_0 : vec3<f32>) -> vec3<f32>
{
    var _S1 : vec3<f32> = vec3<f32>(0.0f);
    return clamp(select(radiance_0, _S1, (((bitcast<vec3<u32>>((radiance_0))) & (vec3<u32>(u32(2147483647))))) > vec3<u32>(u32(2139095040))), _S1, vec3<f32>(65000.0f));
}

fn exposure_scale_0( ev_0 : f32) -> f32
{
    return exp2(ev_0);
}

fn bloom_bright_0( exposed_0 : vec3<f32>) -> vec3<f32>
{
    return hdr_safe_0(exposed_0 - vec3<f32>(1.0f));
}

struct pixelOutput_0
{
    @location(0) output_0 : vec4<f32>,
};

struct pixelInput_0
{
    @location(0) uv_1 : vec2<f32>,
};

@fragment
fn fs_brightpass( _S2 : pixelInput_0, @builtin(position) svPosition_1 : vec4<f32>) -> pixelOutput_0
{
    var scene_0 : vec3<f32> = hdr_safe_0((textureSample((srcTex_0), (srcSampler_0), (_S2.uv_1))).xyz);
    var scene_1 : vec3<f32>;
    if((bloomUniforms_0.params_0.x) > 0.5f)
    {
        var cloud_0 : vec4<f32> = (textureSample((cloudTex_0), (srcSampler_0), (_S2.uv_1)));
        scene_1 = cloud_0.xyz + scene_0 * vec3<f32>((1.0f - cloud_0.w));
    }
    else
    {
        scene_1 = scene_0;
    }
    var _S3 : pixelOutput_0 = pixelOutput_0( vec4<f32>(bloom_bright_0(hdr_safe_0(scene_1) * vec3<f32>(exposure_scale_0(exposureState_0[i32(0)]))), 1.0f) );
    return _S3;
}

fn bloom_tap_weight_0( offset_0 : i32) -> f32
{
    var distance_0 : i32 = abs(offset_0);
    var _S4 : f32;
    if(distance_0 <= i32(4))
    {
        _S4 = BLOOM_WEIGHTS_0[distance_0];
    }
    else
    {
        _S4 = 0.0f;
    }
    return _S4;
}

fn blur_0( uv_2 : vec2<f32>,  dir_0 : vec2<f32>) -> vec3<f32>
{
    var w_0 : u32;
    var h_0 : u32;
    {var dim = textureDimensions((srcTex_0));((w_0)) = dim.x;((h_0)) = dim.y;};
    var _S5 : vec2<f32> = dir_0 / vec2<f32>(f32(w_0), f32(h_0));
    var _S6 : vec2<f32> = _S5 * vec2<f32>(2.0f);
    var _S7 : vec2<f32> = _S5 * vec2<f32>(3.0f);
    var _S8 : vec2<f32> = _S5 * vec2<f32>(4.0f);
    return (textureSample((srcTex_0), (srcSampler_0), (uv_2))).xyz * vec3<f32>(bloom_tap_weight_0(i32(0))) + (textureSample((srcTex_0), (srcSampler_0), (uv_2 + _S5))).xyz * vec3<f32>(bloom_tap_weight_0(i32(1))) + (textureSample((srcTex_0), (srcSampler_0), (uv_2 - _S5))).xyz * vec3<f32>(bloom_tap_weight_0(i32(-1))) + (textureSample((srcTex_0), (srcSampler_0), (uv_2 + _S6))).xyz * vec3<f32>(bloom_tap_weight_0(i32(2))) + (textureSample((srcTex_0), (srcSampler_0), (uv_2 - _S6))).xyz * vec3<f32>(bloom_tap_weight_0(i32(-2))) + (textureSample((srcTex_0), (srcSampler_0), (uv_2 + _S7))).xyz * vec3<f32>(bloom_tap_weight_0(i32(3))) + (textureSample((srcTex_0), (srcSampler_0), (uv_2 - _S7))).xyz * vec3<f32>(bloom_tap_weight_0(i32(-3))) + (textureSample((srcTex_0), (srcSampler_0), (uv_2 + _S8))).xyz * vec3<f32>(bloom_tap_weight_0(i32(4))) + (textureSample((srcTex_0), (srcSampler_0), (uv_2 - _S8))).xyz * vec3<f32>(bloom_tap_weight_0(i32(-4)));
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
fn fs_blur_h( _S9 : pixelInput_1, @builtin(position) svPosition_2 : vec4<f32>) -> pixelOutput_1
{
    var _S10 : pixelOutput_1 = pixelOutput_1( vec4<f32>(blur_0(_S9.uv_3, vec2<f32>(1.0f, 0.0f)), 1.0f) );
    return _S10;
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
fn fs_blur_v( _S11 : pixelInput_2, @builtin(position) svPosition_3 : vec4<f32>) -> pixelOutput_2
{
    var _S12 : pixelOutput_2 = pixelOutput_2( vec4<f32>(blur_0(_S11.uv_4, vec2<f32>(0.0f, 1.0f)), 1.0f) );
    return _S12;
}

