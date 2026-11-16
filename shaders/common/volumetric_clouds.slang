// Volumetric clouds for both renderers: the noise volume's texels, the axis-aligned cloud box and the ray march.

import sky_model;

static const float CLOUD_PI = 3.14159265358979323846;

// Each renderer allocates the volume at this extent per axis; its own constants are pinned to it by a test.
static const float NOISE_VOLUME_EXTENT = 128.0;

// A combined sampler in the C++ engine, a texture and a sampler in WGSL; Slang specializes each use, never dispatches.
interface ICloudNoise
{
    float4 sample_noise(float3 uvw);
};

// The nine cloud controls as the C++ SceneUBO packs them, field for field, so both hosts fill one layout.
struct CloudParams
{
    float4 cloudMeshScale;
    float4 cloudMeshOffset;
    float4 cloudParameters;
    float4 cloudLightMarch;
};

struct Clouds
{
    int num_march_steps;
    int num_march_steps_to_light;
    // Half extents of the axis-aligned cloud box.
    float3 radius;
    // Centre of the cloud box.
    float3 offset;
    float threshold;
    // Density: multiplies every noise sample.
    float scale;
    float pillowness;
    float cirrus_effect;
    bool powder_effect;
    // Wind velocity times time, wrapped by the host to the noise's period; it moves the noise, never the box.
    float3 wind_offset;
};

// Mean sky radiance over the upper and the lower hemisphere: the light a cloud scatters from everywhere but the sun.
struct CloudAmbient
{
    float3 top;
    float3 bottom;
};

// sky_model's gradient averaged per hemisphere over solid angle: the integrals of pow(y, 0.7) and clamp(3y, 0, 1).
CloudAmbient cloud_ambient_analytic_sky()
{
    CloudAmbient ambient;
    ambient.top = lerp(SKY_HORIZON, SKY_ZENITH, 1.0 / 1.7);
    ambient.bottom = lerp(SKY_HORIZON, SKY_GROUND, 5.0 / 6.0);
    return ambient;
}

// The longest path through the box takes at most this many times the slider's steps.
static const int CLOUD_MAX_STEP_FACTOR = 4;
// Scattering octaves of Wrenninge et al.'s multiple-scattering approximation; each halves energy, depth and g.
static const int CLOUD_SCATTER_OCTAVES = 3;

// A shader must not trust a uniform: both step counts are clamped again, to the hosts' bounds.
Clouds cloud_unpack(CloudParams scene)
{
    Clouds cloud;
    cloud.num_march_steps = int(clamp(scene.cloudParameters.w, 4.0, 128.0));
    cloud.num_march_steps_to_light = int(clamp(scene.cloudLightMarch.x, 1.0, 128.0));
    cloud.scale = scene.cloudMeshScale.w;
    cloud.threshold = scene.cloudMeshOffset.w;
    cloud.pillowness = scene.cloudParameters.x;
    cloud.cirrus_effect = scene.cloudParameters.y;
    cloud.powder_effect = scene.cloudParameters.z > 0.5;
    // Scale alone sizes the box, so Density thins the cloud without moving its edges; the hosts keep Scale above zero.
    cloud.radius = scene.cloudMeshScale.xyz * 10.0;
    cloud.offset = scene.cloudMeshOffset.xyz;
    cloud.wind_offset = scene.cloudLightMarch.yzw;
    return cloud;
}

// Worley cell jitter.
float3 hash33(float3 p3)
{
    p3 = frac(p3 * float3(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, p3.yxz + 33.33);
    return frac((p3.xxy + p3.yxx) * p3.zyx);
}

// A lattice cell in [0, period): fmod keeps the dividend's sign, which would make cell -1 differ from period - 1.
float3 wrap_cell(float3 cell, float period)
{
    return cell - period * floor(cell / period);
}

// Worley noise in [0, 1]: minDist starts at 1.0 and only decreases.
float worley(float3 p, float numCells)
{
    p *= numCells;
    float3 i = floor(p);
    float3 f = frac(p);

    float minDist = 1.0;
    for (int z = -1; z <= 1; z++)
    {
        for (int y = -1; y <= 1; y++)
        {
            for (int x = -1; x <= 1; x++)
            {
                float3 neighbor = float3(float(x), float(y), float(z));
                float3 neighborWorld = wrap_cell(i + neighbor, numCells);
                float3 point = hash33(neighborWorld);
                float3 diff = neighbor + point - f;
                float dist = length(diff);
                minDist = min(minDist, dist);
            }
        }
    }
    return minDist;
}

float hash13(float3 p3)
{
    p3 = frac(p3 * 0.1031);
    p3 += dot(p3, p3.zyx + 31.32);
    return frac((p3.x + p3.y) * p3.z);
}

// Value noise whose lattice repeats every `period` cells, so a volume of whole periods tiles; `seed` sets an octave apart.
float valueNoise(float3 x, float period, float seed)
{
    float3 i = floor(x);
    float3 f = frac(x);
    f = f * f * (3.0 - 2.0 * f);
    float3 c0 = wrap_cell(i, period) + seed;
    float3 c1 = wrap_cell(i + 1.0, period) + seed;
    return lerp(lerp(lerp(hash13(c0), hash13(float3(c1.x, c0.y, c0.z)), f.x),
                     lerp(hash13(float3(c0.x, c1.y, c0.z)), hash13(float3(c1.x, c1.y, c0.z)), f.x), f.y),
                 lerp(lerp(hash13(float3(c0.x, c0.y, c1.z)), hash13(float3(c1.x, c0.y, c1.z)), f.x),
                     lerp(hash13(float3(c0.x, c1.y, c1.z)), hash13(c1), f.x), f.y), f.z);
}

// Already in [0, 1]: the geometric weights 0.5, 0.25, ... sum below 1. x repeats every `period`, a whole number.
float fbm_value(float3 x, float period, int octaves)
{
    float v = 0.0;
    float a = 0.5;
    // The cells the unwrapped x * 2 + 100 recurrence hashed, so only each octave's last cell changes.
    float seed = 0.0;
    for (int i = 0; i < octaves; ++i)
    {
        v += a * valueNoise(x, period, seed);
        x *= 2.0;
        period *= 2.0;
        seed = seed * 2.0 + 100.0;
        a *= 0.5;
    }
    return v;
}

// One texel of the volume, which covers [0, 1) once per axis; sample_density weights every channel.
float4 noise_volume_texel(uint3 texel)
{
    float3 uvw = float3(texel) / NOISE_VOLUME_EXTENT;

    float worleyVal = worley(uvw, 4.0);
    float fbmVal = fbm_value(uvw * 3.0, 3.0, 4);
    float worleyFine = worley(uvw, 8.0);
    float cirrusVal = fbm_value(uvw * 6.0, 6.0, 5);

    return float4(worleyVal, fbmVal, worleyFine, cirrusVal);
}

float sample_density(ICloudNoise noise, float3 position, Clouds cloud)
{
    // Box-local, so Offset carries the box's clouds; its centre reads the volume's middle, which tiling left as it was.
    float3 local = position - cloud.offset - cloud.wind_offset;
    float3 uvwCoarse = frac(local / 256.0 + 0.5);
    float4 noiseCoarse = noise.sample_noise(uvwCoarse);

    float baseDensity = ((cloud.pillowness * 0.25)) * max(0.0, noiseCoarse.r - cloud.threshold) * cloud.scale
                      + ((1.0 - cloud.pillowness) * 0.125) * max(0.0, noiseCoarse.g - cloud.threshold) * cloud.scale
                      + ((1.0 - cloud.pillowness) * 0.125) * max(0.0, noiseCoarse.b - cloud.threshold) * cloud.scale;

    baseDensity = baseDensity * (1.0 - cloud.cirrus_effect)
                + max(0.0, noiseCoarse.a - cloud.threshold) * cloud.scale * cloud.cirrus_effect;

    float3 uvwFine = frac(local / 64.0 + 0.5);
    float4 noiseFine = noise.sample_noise(uvwFine);

    float fineDensity = 0.6 * max(0.0, noiseFine.r - cloud.threshold) * cloud.scale
                      + 0.15 * max(0.0, noiseFine.g - cloud.threshold) * cloud.scale
                      + 0.15 * max(0.0, noiseFine.b - cloud.threshold) * cloud.scale;

    return baseDensity + fineDensity * 0.3;
}

// cosTheta is between the view ray and the vector toward the sun; positive g peaks at +1 (forward scattering).
float phase_HG(float cosTheta, float g)
{
    float denom = 1.0 + g * g - 2.0 * g * cosTheta;
    return (1.0 / (4.0 * CLOUD_PI)) * ((1.0 - g * g) / (denom * sqrt(denom)));
}

// Slab test (iquilezles) against the box around cloud.offset with half extents cloud.radius; entry/exit in oT.
bool box_intersect(float3 ro, float3 rd, Clouds cloud, out float2 oT)
{
    oT = float2(0.0, 0.0);
    // A zero component (a sun straight overhead) made inf - inf = NaN below; 1e-30 keeps that slab unbounded.
    float3 m = 1.0 / select(rd == float3(0.0), float3(1e-30), rd);
    float3 n = m * (ro - cloud.offset);
    float3 k = abs(m) * cloud.radius;
    float3 t1 = -n - k;
    float3 t2 = -n + k;

    float tN = max(max(t1.x, t1.y), t1.z);
    float tF = min(min(t2.x, t2.y), t2.z);

    if (tN > tF || tF < 0.0) return false;

    oT = float2(tN, tF);
    return true;
}

// Distance from the eye to the surface at this pixel; depth 1 is a cleared buffer or a traced miss, so open sky.
float cloud_scene_distance(float4x4 ndcToSpace, float2 ndc, float depth, float3 eye)
{
    if (depth == 1.0) return 3.0e38;
    float4 position = mul(ndcToSpace, float4(ndc, depth, 1.0));
    return length(position.xyz / position.w - eye);
}

// The box's shortest full extent.
float cloud_thinnest_extent(Clouds cloud)
{
    return 2.0 * min(min(cloud.radius.x, cloud.radius.y), cloud.radius.z);
}

// The slider's steps span the box's thinnest extent; a longer path takes more, up to CLOUD_MAX_STEP_FACTOR times.
int cloud_march_steps(Clouds cloud, float marchLength)
{
    float wanted = ceil(float(cloud.num_march_steps) * marchLength / cloud_thinnest_extent(cloud));
    float most = min(float(cloud.num_march_steps * CLOUD_MAX_STEP_FACTOR), 128.0);
    return int(clamp(wanted, float(cloud.num_march_steps), most));
}

// Optical depth from samplePos to the box's edge toward the light; directionToLight must be normalized.
float light_optical_depth(ICloudNoise noise, float3 samplePos, float3 directionToLight, Clouds cloud)
{
    float totalDensity = 0.0;
    float2 oT;
    bool intersection = box_intersect(samplePos, directionToLight, cloud, oT);

    if (!intersection) return 0.0;

    // samplePos is inside the box, so the near intersection can be behind it.
    float t0 = max(oT.x, 0.0);
    float dtL = (oT.y - t0) / float(cloud.num_march_steps_to_light);

    for (int i = 0; i < cloud.num_march_steps_to_light; i++)
    {
        float3 p = samplePos + directionToLight * (t0 + (float(i) + 0.5) * dtL);
        totalDensity += sample_density(noise, p, cloud) * dtL;
    }
    return totalDensity;
}

// Sun light scattered toward the eye per unit sun radiance: single scattering plus the halved octaves.
float cloud_sun_scattering(float cosTheta, float lightDepth)
{
    float energy = 0.0;
    float weight = 1.0;
    float depthScale = 1.0;
    float g = 0.5;
    for (int octave = 0; octave < CLOUD_SCATTER_OCTAVES; octave++)
    {
        energy += weight * phase_HG(cosTheta, g) * exp(-lightDepth * depthScale);
        weight *= 0.5;
        depthScale *= 0.5;
        g *= 0.5;
    }
    return energy;
}

// The ambient a sample sees: the sky above near the box's top, the lower hemisphere near its bottom.
float3 cloud_ambient_at(CloudAmbient ambient, Clouds cloud, float3 position)
{
    float height = saturate((position.y - cloud.offset.y) / (2.0 * cloud.radius.y) + 0.5);
    return lerp(ambient.bottom, ambient.top, height);
}

// Premultiplied cloud colour and 1 - transmittance along one ray, ending at the scene surface sceneDistance away.
float4 march_clouds(ICloudNoise noise, float3 eyePosition, float3 rayDirection, float sceneDistance, Clouds settings,
                    float3 directionToLight, float3 sunRadiance, CloudAmbient ambient)
{
    float3 toLight = normalize(directionToLight);
    Clouds cloud = settings;
    float2 oT;
    bool intersection = box_intersect(eyePosition, rayDirection, cloud, oT);
    // The march ends at the first surface the main pass drew, so geometry in front of the box hides it.
    oT.y = min(oT.y, sceneDistance);

    float3 radiance = float3(0.0);
    float transmittance = 1.0;
    float cosTheta = dot(normalize(rayDirection), toLight);

    if (intersection && oT.y > max(oT.x, 0.0))
    {
        oT.x = max(0.0, oT.x);
        float marchLength = oT.y - oT.x;
        // Powder reads the slider's step through the thinnest extent, so the adaptive count does not change it.
        float powderLength = cloud_thinnest_extent(cloud) / float(cloud.num_march_steps);
        cloud.num_march_steps = cloud_march_steps(settings, marchLength);
        float dt = marchLength / float(cloud.num_march_steps);
        for (int i = 0; i < cloud.num_march_steps; i++)
        {
            float3 samplePos = eyePosition + rayDirection * (oT.x + (float(i) + 0.5) * dt);

            float densityOfSample = sample_density(noise, samplePos, cloud);
            float lightDepth = light_optical_depth(noise, samplePos, toLight, cloud);
            // Powder only dims the sun's in-scattering; transmittance changes solely through the `*=` below.
            float powder = cloud.powder_effect ? (1.0 - exp(-densityOfSample * powderLength * 2.0)) : 1.0;
            float3 source = sunRadiance * (cloud_sun_scattering(cosTheta, lightDepth) * powder)
                          + cloud_ambient_at(ambient, cloud, samplePos);

            // The step's in-scattering integrated exactly for albedo 1, so a long step cannot overshoot.
            radiance += transmittance * (1.0 - exp(-densityOfSample * dt)) * source;

            transmittance *= exp(-densityOfSample * dt);

            if (transmittance < 0.01) break;
        }
    }

    return float4(radiance, 1.0 - transmittance);
}

// The C++ engine's entry, lit by the analytic sky until it passes its own sky's ambient.
float4 march_clouds(ICloudNoise noise, float3 eyePosition, float3 rayDirection, float sceneDistance, Clouds cloud,
                    float3 directionToLight, float3 sunRadiance)
{
    return march_clouds(noise, eyePosition, rayDirection, sceneDistance, cloud, directionToLight, sunRadiance,
                        cloud_ambient_analytic_sky());
}
