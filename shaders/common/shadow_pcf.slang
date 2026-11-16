// PCF over a cascaded shadow map with a receiver-plane depth bias, shared by both renderers' directional shadows.

// Smallest |cos| between the receiver normal and the light axis the plane follows; past it the slope stops growing.
static const float PCF_MIN_RECEIVER_COS = 0.1;

// Depth per unit of map uv on the receiver's plane, from the affine light matrix, not derivatives; uv_y_sign: 2 d(uv.y)/d(ndc.y).
float2 receiver_plane_depth_slope(float4x4 light_space, float3 n, float uv_y_sign)
{
    float3 ax = light_space[0].xyz;
    float3 ay = light_space[1].xyz;
    float3 az = light_space[2].xyz;
    // The cofactor rows map a world normal to the plane's NDC normal; z = n . (ax x ay) is cos to the light axis.
    float3 light_axis = cross(ax, ay);
    float mx = dot(n, cross(ay, az));
    float my = dot(n, cross(az, ax));
    float mz = dot(n, light_axis);
    float floor_z = PCF_MIN_RECEIVER_COS * length(light_axis);
    float denom = mz >= 0.0 ? max(mz, floor_z) : min(mz, -floor_z);
    return -2.0 * float2(mx, my * uv_y_sign) / denom;
}

// One comparison tap of a renderer's own shadow map and sampler.
interface IShadowTaps
{
    // The comparison sampler's lit fraction at uv against reference; 1 outside [0, 1]^2, where neither border reads lit.
    float lit(float2 uv, float reference);
}

// Lit fraction over (2 radius + 1)^2 taps a texel apart, each compared at the receiver plane's depth beneath it.
float pcf_visibility<T : IShadowTaps>(T taps, float2 uv, float2 texel, float reference, float2 uv_depth_slope, int radius)
{
    float sum = 0.0;
    for (int y = -radius; y <= radius; y++)
    {
        for (int x = -radius; x <= radius; x++)
        {
            float2 offset = float2(float(x), float(y)) * texel;
            sum += taps.lit(uv + offset, reference + dot(uv_depth_slope, offset));
        }
    }
    return sum / float((2 * radius + 1) * (2 * radius + 1));
}
