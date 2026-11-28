import constants;

// The sun both renderers' IBL bake leaves out of the sky it reads: the directional Sun alone lights with it.

// The disk the bake replaces, 10 degrees: a photo sky's saturated sun, its rays and its glow.
static const float SUN_MASK_COS_RADIUS = 0.98480775;
// The ring at 12 degrees whose mean fills the disk; the cap fades out between the disk's edge and it.
static const float SUN_MASK_RING_COS = 0.9781476;
static const float SUN_MASK_RING_SIN = 0.20791169;
static const uint SUN_MASK_RING_TAPS = 16u;
// A disk that outshines its ring this much is a sun: the photo sky's does 12 times, the gradient's horizon 1.3.
static const float SUN_MASK_CONTRAST = 3.0;
// Mips of the 128-texel environment: on 8 texels a side a sunlit cloud stays below the sun's glow.
static const float SUN_SEARCH_COARSE_LOD = 4.0;
static const float SUN_SEARCH_FINE_LOD = 2.0;
// tan(1.5 degrees), the fine grid's step; 13 taps a side reach past a coarse cell's corner.
static const float SUN_SEARCH_STEP = 0.026185921;
static const int SUN_SEARCH_HALF = 6;

// A sky's radiance along a unit direction at a mip of the bake's environment cube.
interface ISkyRadiance
{
    float3 radiance(float3 dir, float lod);
}

// The sun the bake leaves out: dir is zero when the sky has none, ring is the sky that takes its place.
struct SunMask
{
    float3 dir;
    float3 ring;
};

// A sun outshines its sky in every channel; a photo's blue glow around it only in green and blue.
float sun_mask_level(float3 c)
{
    return min(c.r, min(c.g, c.b));
}

// A unit vector across n; the fine grid and the ring lie in the plane it spans with cross(n, t).
float3 sun_search_tangent(float3 n)
{
    float3 up = abs(n.y) < 0.999 ? float3(0.0, 1.0, 0.0) : float3(1.0, 0.0, 0.0);
    return normalize(cross(up, n));
}

// The brightest coarse cell, twice re-centred on what outshines half of it: a photo's flat-topped disk has no peak.
SunMask find_sun<S : ISkyRadiance>(S sky)
{
    float3 centre = float3(0.0, 1.0, 0.0);
    float coarsePeak = 0.0;
    for (uint j = 0u; j < 18u; j++)
    {
        float lat = (10.0 * float(j) - 85.0) * (PI / 180.0);
        for (uint i = 0u; i < 36u; i++)
        {
            float lon = 10.0 * float(i) * (PI / 180.0);
            float3 dir = float3(cos(lat) * cos(lon), sin(lat), cos(lat) * sin(lon));
            float l = sun_mask_level(sky.radiance(dir, SUN_SEARCH_COARSE_LOD));
            if (l > coarsePeak)
            {
                coarsePeak = l;
                centre = dir;
            }
        }
    }
    float peak = 0.0;
    for (uint pass = 0u; pass < 2u; pass++)
    {
        float3 t = sun_search_tangent(centre);
        float3 b = cross(centre, t);
        float3 weighted = float3(0.0);
        for (int y = -SUN_SEARCH_HALF; y <= SUN_SEARCH_HALF; y++)
        {
            for (int x = -SUN_SEARCH_HALF; x <= SUN_SEARCH_HALF; x++)
            {
                float3 dir = normalize(centre + (float(x) * t + float(y) * b) * SUN_SEARCH_STEP);
                float l = sun_mask_level(sky.radiance(dir, SUN_SEARCH_FINE_LOD));
                peak = max(peak, l);
                weighted += max(l - 0.5 * coarsePeak, 0.0) * dir;
            }
        }
        if (dot(weighted, weighted) > 0.0)
            centre = normalize(weighted);
    }
    float3 t = sun_search_tangent(centre);
    float3 b = cross(centre, t);
    float3 ring = float3(0.0);
    for (uint k = 0u; k < SUN_MASK_RING_TAPS; k++)
    {
        float phi = 2.0 * PI * (float(k) + 0.5) / float(SUN_MASK_RING_TAPS);
        ring += sky.radiance(centre * SUN_MASK_RING_COS + (cos(phi) * t + sin(phi) * b) * SUN_MASK_RING_SIN,
            SUN_SEARCH_FINE_LOD);
    }
    SunMask mask;
    mask.ring = ring / float(SUN_MASK_RING_TAPS);
    mask.dir = peak > SUN_MASK_CONTRAST * sun_mask_level(mask.ring) ? centre : float3(0.0);
    return mask;
}

// A bake tap's radiance: inside the disk no channel outshines the ring; a cap fading out by the ring draws no circle.
float3 sun_masked(float3 radiance, float3 dir, SunMask mask)
{
    float feather = smoothstep(SUN_MASK_RING_COS, SUN_MASK_COS_RADIUS, dot(dir, mask.dir));
    return lerp(radiance, min(radiance, mask.ring), feather);
}
