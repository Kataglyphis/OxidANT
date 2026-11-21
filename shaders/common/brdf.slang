// Epic/UE4 microfacet BRDF shared by both renderers; the 1e-6 guards prevent NaN/inf at grazing angles.

import brdf_energy_fit;

static const float PI = 3.14159265358979323846;

// Smallest perceptual roughness the NDF evaluates; below it the GGX peak 1/(PI r^4) outgrows HDR targets.
static const float MIN_ROUGHNESS = 0.045;

// Lambertian diffuse.
float3 lambert_diffuse(float3 albedo)
{
    return albedo / PI;
}

// GGX (Trowbridge-Reitz) normal distribution function.
float distribution_ggx(float n_dot_h, float roughness)
{
    float r = max(roughness, MIN_ROUGHNESS);
    float a = r * r;
    float a2 = a * a;
    float denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    // denom >= a2 > 0, so the guard only catches rounding; a 1e-6 one clipped every peak below r = 0.154.
    return a2 / (PI * max(denom * denom, 1e-20));
}

// Epic's Smith remap k for analytic lights.
float smith_k_analytic(float roughness)
{
    float r = roughness + 1.0;
    return (r * r) / 8.0;
}

// Epic's k = alpha/2 for sampled and image-based light: the analytic remap over-masks a sampled lobe at grazing angles.
float smith_k_sampled(float roughness)
{
    return roughness * roughness / 2.0;
}

// Schlick-GGX Smith masking-shadowing with remap k, uncorrelated.
float geometry_smith_k(float n_dot_v, float n_dot_l, float k)
{
    float g1 = n_dot_v / max(n_dot_v * (1.0 - k) + k, 1e-6);
    float g2 = n_dot_l / max(n_dot_l * (1.0 - k) + k, 1e-6);
    return g1 * g2;
}

// Schlick Fresnel. cos_theta: dot(H, V), f0: reflectance at normal incidence.
float3 fresnel_schlick(float cos_theta, float3 f0)
{
    return f0 + (float3(1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

// The single-scatter GGX lobe D G F / (4 n.v n.l) with Smith remap k, what the multiple-scattering gain completes.
float3 ggx_single_scatter(float n_dot_v, float n_dot_l, float n_dot_h, float h_dot_v, float roughness, float3 f0, float k)
{
    float d = distribution_ggx(n_dot_h, roughness);
    float g = geometry_smith_k(n_dot_v, n_dot_l, k);
    float3 f = fresnel_schlick(h_dot_v, f0);
    return (d * g * f) / max(4.0 * n_dot_v * n_dot_l, 1e-6);
}

// The fit's first coordinate starts at brdf_smith_k's floor on n.v, below which no BRDF here is evaluated.
static const float GGX_ENERGY_MIN_N_DOT_V = 1e-4;

// n.v and roughness at fit coordinates t in [0, 1]: n.v uniform in its cube root, roughness in its square root.
float2 ggx_energy_node(float2 t)
{
    float2 low = float2(pow(GGX_ENERGY_MIN_N_DOT_V, 1.0 / 3.0), sqrt(MIN_ROUGHNESS));
    float2 c = low + clamp(t, float2(0.0), float2(1.0)) * (float2(1.0) - low);
    return float2(c.x * c.x * c.x, c.y * c.y);
}

// ggx_energy_node's inverse: the fit coordinates of an n.v and a roughness.
float2 ggx_energy_coordinates(float n_dot_v, float roughness)
{
    float2 low = float2(pow(GGX_ENERGY_MIN_N_DOT_V, 1.0 / 3.0), sqrt(MIN_ROUGHNESS));
    float2 c = float2(pow(clamp(n_dot_v, GGX_ENERGY_MIN_N_DOT_V, 1.0), 1.0 / 3.0),
        sqrt(clamp(roughness, MIN_ROUGHNESS, 1.0)));
    return (c - low) / (float2(1.0) - low);
}

// brdf_sampled's single-scatter directional albedo split by Schlick's Fresnel: f0 * x + y, x + y at F = 1.
float2 ggx_energy(float n_dot_v, float roughness)
{
    return ggx_energy_fit(ggx_energy_coordinates(n_dot_v, roughness));
}

// Turquin 2019: the lobe scaled so a white metal's albedo is 1; f0 stands in for the average Fresnel of what scatters again.
float3 multiscatter_gain(float2 energy, float3 f0)
{
    return float3(1.0) + f0 * (1.0 / max(energy.x + energy.y, 1e-3) - 1.0);
}

// The compensated specular lobe's directional albedo E_spec(mu); the diffuse lobe takes 1 - E_spec of the light.
float3 specular_albedo(float2 energy, float3 f0)
{
    return multiscatter_gain(energy, f0) * (f0 * energy.x + energy.y);
}

// Compensated Cook-Torrance with remap k + Lambert scaled by (1 - E_spec)(1 - metallic), times radiance and n.l.
float3 brdf_smith_k(float3 n, float3 v, float3 l, float3 albedo, float metallic, float roughness, float3 f0,
    float3 radiance, float k)
{
    float3 h = normalize(l + v);
    float n_dot_l = max(dot(n, l), 0.0);
    float n_dot_v = max(dot(n, v), 1e-4);
    float n_dot_h = max(dot(n, h), 0.0);
    float h_dot_v = max(dot(h, v), 0.0);

    // brdf_sampled's energy for every remap: the analytic remap's extra masking shapes a highlight, it loses no bounce.
    float2 energy = ggx_energy(n_dot_v, roughness);
    float3 single = ggx_single_scatter(n_dot_v, n_dot_l, n_dot_h, h_dot_v, roughness, f0, k);
    float3 specular = multiscatter_gain(energy, f0) * single;
    float3 diffuse = saturate(float3(1.0) - specular_albedo(energy, f0)) * (1.0 - metallic) * lambert_diffuse(albedo);
    return (diffuse + specular) * radiance * n_dot_l;
}

// The compensated Cook-Torrance specular + Lambert diffuse; v and l point away from the surface.
float3 brdf_direct(
    float3 n,
    float3 v,
    float3 l,
    float3 albedo,
    float metallic,
    float roughness,
    float3 f0,
    float3 radiance)
{
    return brdf_smith_k(n, v, l, albedo, metallic, roughness, f0, radiance, smith_k_analytic(roughness));
}

// brdf_direct's f n.l under a unit light for a direction a path sampled, with the sampled remap; NEE keeps brdf_direct.
float3 brdf_sampled(float3 n, float3 v, float3 l, float3 albedo, float metallic, float roughness, float3 f0)
{
    return brdf_smith_k(n, v, l, albedo, metallic, roughness, f0, float3(1.0), smith_k_sampled(roughness));
}
