// The clamp every write to an RGBA16F HDR target ends in, on both renderers.

// Below fp16's 65504: a larger radiance stores as +Inf, and ACES turns Inf into NaN.
static const float HDR_MAX = 65000.0;

// WGSL may assume floats are finite, so a NaN is found by its bits; clamp then caps +Inf.
float3 hdr_safe(float3 radiance)
{
    bool3 nan = (asuint(radiance) & 0x7FFFFFFFu) > 0x7F800000u;
    return clamp(select(nan, float3(0.0), radiance), float3(0.0), float3(HDR_MAX));
}
