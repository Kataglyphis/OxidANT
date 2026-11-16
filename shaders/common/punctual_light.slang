// KHR_lights_punctual as the raster passes and the path tracer both shade it; render::lights packs the four rows.

// A directional light's distance: no occluder lies past it.
static const float PUNCTUAL_AT_INFINITY = 3.0e38;

struct PunctualLight
{
    float3 position;
    float kind;          // 1 point, 2 spot, 3 directional
    float3 radiance;     // colour times intensity
    float range;         // 0: unbounded
    float3 direction;    // where a spot or directional light points
    float cos_inner;
    float cos_outer;
};

PunctualLight unpack_punctual_light(float4 a, float4 b, float4 c, float4 d)
{
    PunctualLight light;
    light.position = a.xyz;
    light.kind = a.w;
    light.radiance = b.rgb;
    light.range = b.w;
    light.direction = c.xyz;
    light.cos_inner = c.w;
    light.cos_outer = d.x;
    return light;
}

// What reaches a point from a light: the unit direction to it, how far it is, and the factor on its radiance.
struct PunctualIncidence
{
    float3 l;
    float distance;
    float attenuation;
};

// Inverse square, the KHR range window and the spot cone's smoothstep; a directional light is unattenuated.
PunctualIncidence punctual_incidence(PunctualLight light, float3 position)
{
    PunctualIncidence o;
    o.attenuation = 1.0;
    if (light.kind > 2.5)
    {
        o.l = normalize(-light.direction);
        o.distance = PUNCTUAL_AT_INFINITY;
        return o;
    }
    float3 toLight = light.position - position;
    o.distance = max(length(toLight), 1e-4);
    o.l = toLight / o.distance;
    o.attenuation = 1.0 / (o.distance * o.distance);
    if (light.range > 0.0)
    {
        float k = clamp(1.0 - pow(o.distance / light.range, 4.0), 0.0, 1.0);
        o.attenuation *= k * k;
    }
    if (light.kind > 1.5)
    {
        float cosAngle = dot(normalize(light.direction), -o.l);
        o.attenuation *= smoothstep(light.cos_outer, light.cos_inner, cosAngle);
    }
    return o;
}
