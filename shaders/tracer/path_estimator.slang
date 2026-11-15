// The C++ path tracer's estimator pieces, ported from BeschleunigerBallett's path_tracing.slang: its RNG and both lobes' sampling.

import brdf;

// PCG RNG: condensed pcg_output_rxs_mxs_32_32.
float rng_next(inout uint state)
{
    state = state * 747796405u + 1u;
    uint word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    word = (word >> 22u) ^ word;
    return float(word) / 4294967295.0;
}

// Unit t and b completing n to a right-handed frame, without a branch on n (Duff et al. 2017).
void orthonormal_basis(float3 n, out float3 t, out float3 b)
{
    float s = n.z >= 0.0 ? 1.0 : -1.0;
    float a = -1.0 / (s + n.z);
    float c = n.x * n.y * a;
    t = float3(1.0 + s * n.x * n.x * a, s * c, -s * n.x);
    b = float3(c, s + n.y * n.y * a, -n.y);
}

// A GGX microfacet normal visible from v (Heitz 2018), in the frame where n is +z; alpha is distribution_ggx's.
float3 sample_ggx_visible_normal(float3 v, float alpha, float2 u)
{
    float3 vh = normalize(float3(alpha * v.x, alpha * v.y, v.z));
    float length_squared = vh.x * vh.x + vh.y * vh.y;
    float3 t1 = length_squared > 0.0 ? float3(-vh.y, vh.x, 0.0) / sqrt(length_squared) : float3(1.0, 0.0, 0.0);
    float3 t2 = cross(vh, t1);
    float r = sqrt(u.x);
    float phi = 2.0 * PI * u.y;
    float p1 = r * cos(phi);
    float p2 = r * sin(phi);
    float s = 0.5 * (1.0 + vh.z);
    p2 = (1.0 - s) * sqrt(max(1.0 - p1 * p1, 0.0)) + s * p2;
    float3 nh = p1 * t1 + p2 * t2 + sqrt(max(1.0 - p1 * p1 - p2 * p2, 0.0)) * vh;
    return normalize(float3(alpha * nh.x, alpha * nh.y, max(nh.z, 1e-6)));
}

// Density of the direction sample_ggx_visible_normal reflects to: D(h) G1(v) / (4 n.v), exact GGX and Smith G1.
float ggx_visible_normal_pdf(float n_dot_v, float n_dot_h, float alpha)
{
    float a2 = alpha * alpha;
    float d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    // Unfloored, unlike distribution_ggx: the density must be the one the sampler draws.
    float ndf = a2 / (PI * d * d);
    float g1 = 2.0 * n_dot_v / (n_dot_v + sqrt(a2 + (1.0 - a2) * n_dot_v * n_dot_v));
    return ndf * g1 / (4.0 * n_dot_v);
}

// Chance to sample the GGX lobe: its Fresnel share at n.v; the floor keeps a white dielectric's highlight sampled.
float specular_lobe_probability(float3 albedo, float metallic, float3 f0, float n_dot_v)
{
    const float3 LUMA = float3(0.2126, 0.7152, 0.0722);
    float3 fresnel = fresnel_schlick(n_dot_v, f0);
    float specular = dot(fresnel, LUMA);
    float diffuse = dot((float3(1.0) - fresnel) * (1.0 - metallic) * albedo, LUMA);
    return clamp(specular / max(specular + diffuse, 1e-6), 0.1, 1.0);
}

// The next segment's direction and its throughput weight f n.l / pdf; a zero weight ends the path.
struct Bounce
{
    float3 direction;
    float3 weight;
};

// One-sample MIS over the GGX and Lambert lobes; brdf_direct under a unit light is f n.l, the BRDF NEE and raster use.
Bounce sample_bounce(float3 n, float3 face_normal, float3 v, float3 albedo, float metallic, float roughness, inout uint rng)
{
    float3 f0 = lerp(float3(0.04), albedo, metallic);
    float n_dot_v = max(dot(n, v), 1e-4);
    float specular = specular_lobe_probability(albedo, metallic, f0, n_dot_v);
    float alpha = roughness * roughness;
    float3 t;
    float3 b;
    orthonormal_basis(n, t, b);
    float3 direction;
    if (rng_next(rng) < specular)
    {
        float3 local = float3(dot(v, t), dot(v, b), dot(v, n));
        float2 u = float2(rng_next(rng), rng_next(rng));
        float3 h = sample_ggx_visible_normal(local, alpha, u);
        direction = reflect(-v, h.x * t + h.y * b + h.z * n);
    }
    else
    {
        // Cosine-weighted: a random unit-sphere point around the normal tip (RTIOW 8.5); landing on -n leaves n.
        float theta = 2.0 * PI * rng_next(rng);
        float z = 2.0 * rng_next(rng) - 1.0;
        float r = sqrt(max(1.0 - z * z, 0.0));
        direction = n + float3(r * cos(theta), r * sin(theta), z);
        direction = dot(direction, direction) < 1e-8 ? n : direction;
    }
    Bounce o;
    o.direction = normalize(direction);
    o.weight = float3(0.0);
    float n_dot_l = dot(n, o.direction);
    // Below either surface ends the path: past the face normal, a tilted map would leak light through it.
    if (n_dot_l <= 0.0 || dot(face_normal, o.direction) <= 0.0)
        return o;
    float n_dot_h = max(dot(n, normalize(v + o.direction)), 0.0);
    float pdf = specular * ggx_visible_normal_pdf(n_dot_v, n_dot_h, alpha) + (1.0 - specular) * n_dot_l / PI;
    o.weight = brdf_direct(n, v, o.direction, albedo, metallic, roughness, f0, float3(1.0)) / max(pdf, 1e-6);
    return o;
}
