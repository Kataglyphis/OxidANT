import constants;

// The glTF material rules both renderers sample with: KHR_texture_transform, normal mapping and the factor rules.

// The alpha modes, which C++ includes too.
__exported import alpha_mode;

// KHR_texture_transform: the T*R*S matrix's two rows applied to a UV.
float2 transform_uv(float2 uv, float3 row0, float3 row1)
{
    return float2(dot(float3(uv, 1.0), row0), dot(float3(uv, 1.0), row1));
}

// normalize() of a zero vector is NaN: a zero-scale joint, a flattening instance or a missing tangent gives one.
float3 normalize_or(float3 v, float3 fallback)
{
    return dot(v, v) > 1e-30 ? normalize(v) : fallback;
}

// A normal texel in tangent space; a two-channel map (BC5) samples blue as 0, so z is rebuilt from the unit length.
float3 tangent_space_normal(float3 texel, float scale)
{
    float3 n = texel * 2.0 - 1.0;
    if (texel.z == 0.0)
        n.z = sqrt(saturate(1.0 - dot(n.xy, n.xy)));
    return float3(n.xy * scale, n.z);
}

// glTF 3.9.3 on the authored frame around a unit normal; a doubleSided back face reverses t, b and n together (3.9.2).
float3 apply_normal_map(float3 normal, float4 tangent, float3 tangent_normal, bool front_face)
{
    float3 t = normalize_or(tangent.xyz - normal * dot(normal, tangent.xyz), float3(0.0));
    float3 b = cross(normal, t) * tangent.w;
    // Row-multiply mul(v, M): the matrix is built from the rows (t, b, n).
    float3 mapped = normalize_or(mul(tangent_normal, float3x3(t, b, normal)), normal);
    return front_face ? mapped : -mapped;
}

// -1 where m mirrors space, which reverses a tangent frame's handedness (glTF 3.7.4).
float mirror_sign(float3x3 m)
{
    return dot(m[0], cross(m[1], m[2])) < 0.0 ? -1.0 : 1.0;
}

// m's cofactor times mirror_sign, |det| m^-T: the normal transform up to scale, without the inverse SPIR-V lacks.
float3x3 normal_transform(float3x3 m)
{
    return float3x3(cross(m[1], m[2]), cross(m[2], m[0]), cross(m[0], m[1])) * mirror_sign(m);
}

// glTF 3.9.2: metallic is the texel's B times its factor, held in [0, 1].
float material_metallic(float factor, float sampled)
{
    return clamp(factor * sampled, 0.0, 1.0);
}

// glTF 3.9.2: roughness is the texel's G times its factor; a G of 0 must not undo the floor.
float material_roughness(float factor, float sampled)
{
    return clamp(factor * sampled, MIN_ROUGHNESS, 1.0);
}

// glTF MASK: an alpha below the cutoff is cut out; OPAQUE and BLEND never are, whatever cutoff they carry.
bool alpha_masked_out(int alpha_mode, float alpha, float cutoff)
{
    return alpha_mode == ALPHA_MASK && alpha < cutoff;
}

// glTF 3.9.3: occlusion is 1 + strength * (texel.r - 1); it darkens the ambient term only.
float material_occlusion(float strength, float sampled)
{
    return lerp(1.0, sampled, strength);
}
