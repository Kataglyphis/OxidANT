import fullscreen;

// Punctual-light cube shadows as render::lights and the C++ PunctualLight.ixx lay them out: layer slot * 6 + face.

// A light's fourth row past cos_outer: .y is the cube slot plus one (0 unshadowed), .zw the cube's depth range.
struct PunctualShadow
{
    int slot;
    float nearPlane;
    float farPlane;
};

PunctualShadow unpack_punctual_shadow(float4 d)
{
    PunctualShadow shadow;
    shadow.slot = int(d.y + 0.5) - 1;
    shadow.nearPlane = d.z;
    shadow.farPlane = d.w;
    return shadow;
}

// The depth a face stores at `majorAxis` along its axis: perspectiveRH_ZO's z / w (render::lights' twin).
float punctual_shadow_depth(float majorAxis, float nearPlane, float farPlane)
{
    return farPlane * (majorAxis - nearPlane) / ((farPlane - nearPlane) * majorAxis);
}

// The face a direction from the light picks (+X, -X, +Y, -Y, +Z, -Z), and its NDC xy there.
struct PunctualShadowFace
{
    uint face;
    float2 ndc;
};

// The cube-map selection table, so a 2D array of faces samples where a cube would: NDC xy is (sc, tc) / |ma|.
PunctualShadowFace punctual_shadow_face(float3 r)
{
    float3 a = abs(r);
    PunctualShadowFace o;
    float ma;
    if (a.x >= a.y && a.x >= a.z)
    {
        ma = a.x;
        o.face = r.x > 0.0 ? 0u : 1u;
        o.ndc = float2(r.x > 0.0 ? -r.z : r.z, -r.y);
    }
    else if (a.y >= a.z)
    {
        ma = a.y;
        o.face = r.y > 0.0 ? 2u : 3u;
        o.ndc = float2(r.x, r.y > 0.0 ? r.z : -r.z);
    }
    else
    {
        ma = a.z;
        o.face = r.z > 0.0 ? 4u : 5u;
        o.ndc = float2(r.z > 0.0 ? r.x : -r.x, -r.y);
    }
    o.ndc /= max(ma, 1e-20);
    return o;
}

// A face's map uv and layer along d from the light: a cube tap's 2D-array twin, uv in the target's NDC rows.
float3 punctual_shadow_face_coord(float3 d, int slot)
{
    PunctualShadowFace f = punctual_shadow_face(d);
    return float3(fullscreen_ndc_to_uv(f.ndc), float(uint(slot) * 6u + f.face));
}

// One comparison tap of a renderer's own cube shadows along d from the light.
interface IPunctualShadowTaps
{
    // The lit fraction at d against reference.
    float lit(float3 d, float reference);
}

// Fraction of the light reaching worldPos: a tap at the offset centre and four around it; width is a face's in texels.
float punctual_shadow_visibility<T : IPunctualShadowTaps>(T taps, PunctualShadow shadow, float3 lightPos,
    float3 worldPos, float3 n, float3 l, float width)
{
    // A texel spans 2 * distance / width at the receiver; offsetting by it along n and l keeps acne off.
    float texel = 2.0 * length(worldPos - lightPos) / width;
    float3 d = worldPos + n * (1.5 * texel) + l * texel - lightPos;
    float majorAxis = max(abs(d.x), max(abs(d.y), abs(d.z)));
    if (majorAxis >= shadow.farPlane)
        return 1.0;
    float reference = punctual_shadow_depth(max(majorAxis, shadow.nearPlane), shadow.nearPlane, shadow.farPlane);

    float3 axis = normalize(d);
    float3 tangent = normalize(cross(abs(axis.y) < 0.99 ? float3(0.0, 1.0, 0.0) : float3(1.0, 0.0, 0.0), axis));
    float3 bitangent = cross(axis, tangent);
    float spread = 1.5 * 2.0 * majorAxis / width;
    float lit = taps.lit(d, reference);
    lit += taps.lit(d + tangent * spread, reference);
    lit += taps.lit(d - tangent * spread, reference);
    lit += taps.lit(d + bitangent * spread, reference);
    lit += taps.lit(d - bitangent * spread, reference);
    return lit * 0.2;
}
