// Color math shared by both renderers (SPIR-V for C++/Vulkan, WGSL for Rust/WebGPU).

// ACES filmic tonemap (Narkowicz 2015): x (a x + b) / (x (c x + d) + e), clamped to [0, 1].
static const float ACES_A = 2.51;
static const float ACES_B = 0.03;
static const float ACES_C = 2.43;
static const float ACES_D = 0.59;
static const float ACES_E = 0.14;

float3 aces_tonemap(float3 x)
{
    return clamp((x * (ACES_A * x + ACES_B)) / (x * (ACES_C * x + ACES_D) + ACES_E), float3(0.0), float3(1.0));
}

// The radiance aces_tonemap shows as `display`: its quadratic solved for x, display clamped to [0, 1].
float3 aces_tonemap_inverse(float3 display)
{
    float3 y = clamp(display, float3(0.0), float3(1.0));
    float3 qa = ACES_A - ACES_C * y;
    float3 qb = ACES_B - ACES_D * y;
    return (-qb + sqrt(qb * qb + 4.0 * qa * ACES_E * y)) / (2.0 * qa);
}

// Exact IEC 61966-2-1 sRGB encode; pow(x, 1/2.2) is visibly wrong in the darks (docs/shaders.md#color-space).
float3 linear_to_srgb(float3 c)
{
    float3 low = c * 12.92;
    float3 high = 1.055 * pow(max(c, float3(0.0)), float3(1.0 / 2.4)) - 0.055;
    return select(c < float3(0.0031308), low, high);
}
