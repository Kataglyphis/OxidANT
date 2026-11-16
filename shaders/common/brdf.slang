// Epic/UE4 microfacet BRDF shared by both renderers; the 1e-6 guards prevent NaN/inf at grazing angles.

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

// Smith geometry function (Epic Games approximation with k = (r+1)^2/8).
float geometry_smith(float n_dot_v, float n_dot_l, float roughness)
{
    return geometry_smith_k(n_dot_v, n_dot_l, smith_k_analytic(roughness));
}

// Schlick Fresnel. cos_theta: dot(H, V), f0: reflectance at normal incidence.
float3 fresnel_schlick(float cos_theta, float3 f0)
{
    return f0 + (float3(1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

// Cook-Torrance specular with Smith remap k + Lambert diffuse scaled by (1-F)(1-metallic), times radiance and n.l.
float3 brdf_smith_k(float3 n, float3 v, float3 l, float3 albedo, float metallic, float roughness, float3 f0,
    float3 radiance, float k)
{
    float3 h = normalize(l + v);
    float n_dot_l = max(dot(n, l), 0.0);
    float n_dot_v = max(dot(n, v), 1e-4);
    float n_dot_h = max(dot(n, h), 0.0);
    float h_dot_v = max(dot(h, v), 0.0);

    float d = distribution_ggx(n_dot_h, roughness);
    float g = geometry_smith_k(n_dot_v, n_dot_l, k);
    float3 f = fresnel_schlick(h_dot_v, f0);
    float3 specular = (d * g * f) / max(4.0 * n_dot_v * n_dot_l, 1e-6);
    float3 diffuse = (float3(1.0) - f) * (1.0 - metallic) * lambert_diffuse(albedo);
    return (diffuse + specular) * radiance * n_dot_l;
}

// Cook-Torrance specular + Lambert diffuse scaled by (1-F)(1-metallic); v and l point away from the surface.
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
