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
