@binding(0) @group(0) var hdrTex_0 : texture_2d<f32>;

@binding(1) @group(0) var<storage, read_write> histogram_0 : array<atomic<u32>>;

struct ExposureParams_std140_0
{
    @align(16) values_0 : vec4<f32>,
};

@binding(3) @group(0) var<uniform> exposureParams_0 : ExposureParams_std140_0;
@binding(2) @group(0) var<storage, read_write> exposureState_0 : array<f32>;

var<workgroup> localHist_0 : array<atomic<u32>, i32(64)>;

fn exposure_luminance_0( color_0 : vec3<f32>) -> f32
{
    return dot(color_0, vec3<f32>(0.2125999927520752f, 0.71520000696182251f, 0.07220000028610229f));
}

fn histogram_bin_0( lum_0 : f32) -> u32
{
    if(!(lum_0 > 9.99999997475242708e-07f))
    {
        return u32(0);
    }
    return u32(1) + min(u32(clamp((log2(lum_0) - -10.0f) / 14.0f, 0.0f, 1.0f) * 62.0f), u32(62));
}

@compute
@workgroup_size(16, 16, 1)
fn cs_build_histogram(@builtin(global_invocation_id) id_0 : vec3<u32>, @builtin(local_invocation_index) localIndex_0 : u32)
{
    var _S1 : bool = localIndex_0 < u32(64);
    if(_S1)
    {
        atomicStore(&(localHist_0[localIndex_0]), u32(0));
    }
    workgroupBarrier();
    var width_0 : u32;
    var height_0 : u32;
    {var dim = textureDimensions((hdrTex_0));((width_0)) = dim.x;((height_0)) = dim.y;};
    var _S2 : bool;
    if((id_0.x) < width_0)
    {
        _S2 = (id_0.y) < height_0;
    }
    else
    {
        _S2 = false;
    }
    if(_S2)
    {
        var _S3 : vec3<i32> = vec3<i32>(vec2<i32>(id_0.xy), i32(0));
        var _S4 : u32 = atomicAdd(&(localHist_0[histogram_bin_0(exposure_luminance_0((textureLoad((hdrTex_0), ((_S3)).xy, ((_S3)).z)).xyz))]), u32(1));
    }
    workgroupBarrier();
    if(_S1)
    {
        var count_0 : u32 = atomicLoad(&(localHist_0[localIndex_0]));
        if(count_0 > u32(0))
        {
            var _S5 : u32 = atomicAdd(&(histogram_0[localIndex_0]), count_0);
        }
    }
    return;
}

@compute
@workgroup_size(64, 1, 1)
fn cs_clear_histogram(@builtin(global_invocation_id) id_1 : vec3<u32>)
{
    var _S6 : u32 = id_1.x;
    if(_S6 < u32(64))
    {
        atomicStore(&(histogram_0[_S6]), u32(0));
    }
    return;
}

fn bin_luminance_0( bin_0 : u32) -> f32
{
    if(bin_0 == u32(0))
    {
        return 0.0f;
    }
    return exp2(-10.0f + (f32(bin_0 - u32(1)) + 0.5f) / 62.0f * 14.0f);
}

fn exposure_target_ev_0( log2Mean_0 : f32) -> f32
{
    return log2(0.18000000715255737f / exp2(log2Mean_0));
}

fn is_finite_0( x_0 : f32) -> bool
{
    return (((bitcast<u32>((x_0))) & (u32(2139095040)))) != u32(2139095040);
}

fn adapt_exposure_ev_0( currentEv_0 : f32,  targetEv_0 : f32,  dt_0 : f32,  speed_0 : f32) -> f32
{
    var _S7 : bool;
    if(is_finite_0(currentEv_0))
    {
        _S7 = (abs(currentEv_0)) < 1.00000001504746622e+30f;
    }
    else
    {
        _S7 = false;
    }
    var current_0 : f32;
    if(_S7)
    {
        current_0 = currentEv_0;
    }
    else
    {
        current_0 = targetEv_0;
    }
    if(dt_0 <= 0.0f)
    {
        return current_0;
    }
    if(speed_0 <= 0.0f)
    {
        return targetEv_0;
    }
    return current_0 + (targetEv_0 - current_0) * clamp(1.0f - exp(- dt_0 * speed_0), 0.0f, 1.0f);
}

@compute
@workgroup_size(1, 1, 1)
fn cs_reduce_exposure()
{
    var dt_1 : f32 = exposureParams_0.values_0.x;
    var speed_1 : f32 = exposureParams_0.values_0.y;
    if((exposureParams_0.values_0.z) < 0.5f)
    {
        exposureState_0[i32(0)] = exposureParams_0.values_0.w;
        exposureState_0[i32(1)] = exposureParams_0.values_0.w;
        return;
    }
    var bin_1 : u32 = u32(1);
    var weightedLogSum_0 : f32 = 0.0f;
    var counted_0 : f32 = 0.0f;
    for(;;)
    {
        if(bin_1 < u32(64))
        {
        }
        else
        {
            break;
        }
        var _S8 : u32 = atomicLoad(&(histogram_0[bin_1]));
        var count_1 : f32 = f32(_S8);
        if(count_1 > 0.0f)
        {
            var counted_1 : f32 = counted_0 + count_1;
            weightedLogSum_0 = weightedLogSum_0 + log2(bin_luminance_0(bin_1)) * count_1;
            counted_0 = counted_1;
        }
        bin_1 = bin_1 + u32(1);
    }
    var currentEv_1 : f32 = exposureState_0[i32(0)];
    if(counted_0 <= 0.0f)
    {
        exposureState_0[i32(1)] = currentEv_1;
        return;
    }
    var targetEv_1 : f32 = exposure_target_ev_0(weightedLogSum_0 / counted_0);
    exposureState_0[i32(0)] = adapt_exposure_ev_0(currentEv_1, targetEv_1, dt_1, speed_1);
    exposureState_0[i32(1)] = targetEv_1;
    return;
}

