import brdf;

// Bakes the samples common/brdf_energy_fit.slang is fitted to, from ggx_single_scatter, and reads the fit back, for the tests.

// BeschleunigerBallett's furnace quadrature (GoldenMetrics.hpp): CDF rows over the floored lobe, uniform azimuth columns.
static const uint BAKE_ROWS = 2048u;
static const uint BAKE_COLUMNS = 128u;
// Invocations per probe, each summing BAKE_ROWS / BAKE_CHUNKS rows.
static const uint BAKE_CHUNKS = 32u;

// Fit coordinates in [0, 1], as ggx_energy_node takes them.
[vk::binding(0, 0)] StructuredBuffer<float2> energyProbes;
// cs_bake: one chunk's (scale, bias) sums and its probe's n.v and roughness; cs_lookup: ggx_energy there.
[vk::binding(1, 0)] RWStructuredBuffer<float4> energyResults;

// The weighted sums of the lobe's Fresnel scale and bias over one chunk of rows at node.
float2 bake_chunk(float2 node, uint chunk)
{
    float mu = node.x;
    float roughness = node.y;
    float r = max(roughness, MIN_ROUGHNESS);
    float a = r * r;
    float a2 = a * a;
    float k = smith_k_sampled(roughness);
    float3 v = float3(sqrt(max(1.0 - mu * mu, 0.0)), 0.0, mu);
    float n_dot_v = max(mu, 1e-4);
    uint rows = BAKE_ROWS / BAKE_CHUNKS;
    float2 sum = float2(0.0);
    for (uint i = chunk * rows; i < (chunk + 1u) * rows; i++)
    {
        float xi = (float(i) + 0.5) / float(BAKE_ROWS);
        float n_dot_h = sqrt((1.0 - xi) / (1.0 + (a2 - 1.0) * xi));
        float sin_h = sqrt(max(1.0 - n_dot_h * n_dot_h, 0.0));
        float denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
        // Each cell holds the same share of D(h) n.h, and dl = 4 (v.h) dh: a sample weighs f n.l 4 (v.h) / (D n.h).
        float density = a2 / (PI * denom * denom) * n_dot_h;
        float2 row = float2(0.0);
        for (uint j = 0u; j < BAKE_COLUMNS; j++)
        {
            float phi = (float(j) + 0.5) / float(BAKE_COLUMNS) * 2.0 * PI;
            float3 h = float3(sin_h * cos(phi), sin_h * sin(phi), n_dot_h);
            float v_dot_h = dot(v, h);
            float n_dot_l = 2.0 * v_dot_h * n_dot_h - v.z;
            if (v_dot_h <= 0.0 || n_dot_l <= 0.0)
                continue;
            float weight = n_dot_l * 4.0 * v_dot_h / density;
            float whole = ggx_single_scatter(n_dot_v, n_dot_l, n_dot_h, v_dot_h, roughness, float3(1.0), k).x;
            float bias = ggx_single_scatter(n_dot_v, n_dot_l, n_dot_h, v_dot_h, roughness, float3(0.0), k).x;
            row += float2(whole - bias, bias) * weight;
        }
        sum += row;
    }
    return sum;
}

uint probe_count()
{
    uint count;
    uint stride;
    energyProbes.GetDimensions(count, stride);
    return count;
}

[shader("compute")]
[numthreads(64, 1, 1)]
void cs_bake(uint3 id : SV_DispatchThreadID)
{
    uint probe = id.x / BAKE_CHUNKS;
    if (probe >= probe_count())
        return;
    float2 node = ggx_energy_node(energyProbes[probe]);
    energyResults[id.x] = float4(bake_chunk(node, id.x % BAKE_CHUNKS), node);
}

[shader("compute")]
[numthreads(64, 1, 1)]
void cs_lookup(uint3 id : SV_DispatchThreadID)
{
    if (id.x >= probe_count())
        return;
    float2 node = ggx_energy_node(energyProbes[id.x]);
    energyResults[id.x] = float4(ggx_energy(node.x, node.y), node);
}
