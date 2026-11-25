// Fullscreen triangle from the vertex index alone, shared so both renderers get identical UV math.

struct FullscreenVsOut
{
    float4 svPosition : SV_Position;
    float2 uv : TEXCOORD0;
};

// uv (0,0) is the target's top-left texel: clip y = -1 is the top row in Vulkan, the bottom row in WebGPU.
FullscreenVsOut fullscreen_vs(uint vid)
{
    FullscreenVsOut o;
    float x = float(vid / 2) * 4.0 - 1.0;
    float y = float(vid % 2) * 4.0 - 1.0;
    o.svPosition = float4(x, y, 0.0, 1.0);
    __target_switch
    {
    case spirv:
        o.uv = float2((x + 1.0) * 0.5, (y + 1.0) * 0.5);
    default:
        o.uv = float2((x + 1.0) * 0.5, 1.0 - (y + 1.0) * 0.5);
    }
    return o;
}

// The exact inverse of fullscreen_vs's uv; edit the four target switches together.
float2 fullscreen_uv_to_ndc(float2 uv)
{
    __target_switch
    {
    case spirv:
        return uv * 2.0 - 1.0;
    default:
        return float2(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    }
}

// fullscreen_uv_to_ndc's inverse: an NDC position's uv, (0, 0) at the target's top-left texel.
float2 fullscreen_ndc_to_uv(float2 ndc)
{
    __target_switch
    {
    case spirv:
        return ndc * 0.5 + 0.5;
    default:
        return float2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    }
}

// 2 d(uv.y)/d(ndc.y) of fullscreen_ndc_to_uv: the sign a map's uv-space depth slope takes in y on this target.
float fullscreen_ndc_uv_y_sign()
{
    __target_switch
    {
    case spirv:
        return 1.0;
    default:
        return -1.0;
    }
}
