@binding(0) @group(0) var msaaDepth_0 : texture_depth_multisampled_2d;

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

struct ResolveOut_0
{
    @location(0) spread_0 : vec2<f32>,
    @builtin(frag_depth) depth_0 : f32,
};

struct pixelInput_0
{
    @location(0) uv_1 : vec2<f32>,
};

@fragment
fn fs_main( _S1 : pixelInput_0, @builtin(position) svPosition_1 : vec4<f32>) -> ResolveOut_0
{
    var w_0 : u32;
    var h_0 : u32;
    var samples_0 : u32;
    {var dim = textureDimensions((msaaDepth_0));((w_0)) = dim.x;((h_0)) = dim.y;((samples_0)) = textureNumSamples((msaaDepth_0));};
    var _S2 : vec2<i32> = vec2<i32>(_S1.uv_1 * vec2<f32>(f32(w_0), f32(h_0)));
    var minDepth_0 : f32 = 1.0f;
    var maxDepth_0 : f32 = 0.0f;
    var i_0 : u32 = u32(0);
    for(;;)
    {
        if(i_0 < samples_0)
        {
        }
        else
        {
            break;
        }
        var d_0 : f32 = (textureLoad((msaaDepth_0), (_S2), (i32(i_0))));
        var _S3 : f32 = min(minDepth_0, d_0);
        var _S4 : f32 = max(maxDepth_0, d_0);
        var _S5 : u32 = i_0 + u32(1);
        minDepth_0 = _S3;
        maxDepth_0 = _S4;
        i_0 = _S5;
    }
    var _S6 : f32 = 0.5f * (minDepth_0 + maxDepth_0);
    var j_0 : u32 = u32(0);
    var far_0 : u32 = u32(0);
    for(;;)
    {
        if(j_0 < samples_0)
        {
        }
        else
        {
            break;
        }
        if((textureLoad((msaaDepth_0), (_S2), (i32(j_0)))) > _S6)
        {
            i_0 = u32(1);
        }
        else
        {
            i_0 = u32(0);
        }
        var far_1 : u32 = far_0 + i_0;
        j_0 = j_0 + u32(1);
        far_0 = far_1;
    }
    var o_1 : ResolveOut_0;
    o_1.depth_0 = minDepth_0;
    o_1.spread_0 = vec2<f32>(maxDepth_0, f32(far_0) / f32(samples_0));
    return o_1;
}

