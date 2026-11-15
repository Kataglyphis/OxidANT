struct TraceResolveParams_std140_0
{
    @align(16) size_0 : vec4<u32>,
};

@binding(0) @group(0) var<uniform> resolveParams_0 : TraceResolveParams_std140_0;
@binding(1) @group(0) var<storage, read> resolveAccumulation_0 : array<vec4<f32>>;

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

struct pixelOutput_0
{
    @location(0) output_0 : vec4<f32>,
};

struct pixelInput_0
{
    @location(0) uv_1 : vec2<f32>,
};

@fragment
fn fs_main( _S1 : pixelInput_0, @builtin(position) svPosition_1 : vec4<f32>) -> pixelOutput_0
{
    var _S2 : vec2<u32> = vec2<u32>(svPosition_1.xy);
    var _S3 : pixelOutput_0 = pixelOutput_0( vec4<f32>(resolveAccumulation_0[_S2.y * resolveParams_0.size_0.x + _S2.x].xyz, 1.0f) );
    return _S3;
}

