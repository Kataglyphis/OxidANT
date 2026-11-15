// Rust/WebGPU AABB-vs-depth occlusion culling: the checked-in gpu_cull.wgsl is generated from this file.

struct CullParams
{
    float4x4 view_proj;
    float4x4 inv_view_proj;
    float width;
    float height;
    uint primitive_count;
};
[vk::binding(0, 0)] Texture2D<float> depthTex;
[vk::binding(1, 0)] ConstantBuffer<CullParams> params;

struct Aabb
{
    float4 min;
    float4 max;
};
[vk::binding(2, 0)] StructuredBuffer<Aabb> aabbData;
[vk::binding(3, 0)] RWStructuredBuffer<uint> visibility;

// lerp(aabb.min, aabb.max, UNIT_CORNERS[i]) walks all eight AABB corners.
static const float3 UNIT_CORNERS[8] = {
    float3(0.0, 0.0, 0.0), float3(1.0, 0.0, 0.0),
    float3(0.0, 1.0, 0.0), float3(1.0, 1.0, 0.0),
    float3(0.0, 0.0, 1.0), float3(1.0, 0.0, 1.0),
    float3(0.0, 1.0, 1.0), float3(1.0, 1.0, 1.0),
};

// Rects up to this many texels are tested texel by texel: sparse point taps miss gaps between them.
static const uint MAX_TESTED_TEXELS = 4096u;

[shader("compute")]
[numthreads(64, 1, 1)]
void cs_main(uint3 gid : SV_DispatchThreadID)
{
    uint idx = gid.x;
    if (idx >= params.primitive_count) return;

    Aabb aabb = aabbData[idx];

    // A box crossing the near plane has no reliable UV, so it counts as visible.
    const float FAR_SENTINEL = 1e30;
    float aabbNear = FAR_SENTINEL;
    float2 uvMin = float2(FAR_SENTINEL, FAR_SENTINEL);
    float2 uvMax = float2(-FAR_SENTINEL, -FAR_SENTINEL);

    for (uint i = 0u; i < 8u; ++i)
    {
        float3 corner = lerp(aabb.min.xyz, aabb.max.xyz, UNIT_CORNERS[i]);
        float4 clip = mul(params.view_proj, float4(corner, 1.0));
        if (clip.w <= 0.0) { visibility[idx] = 1u; return; }

        float2 ndc = clip.xy / clip.w;
        float depth = clip.z / clip.w;
        // WebGPU convention: ndc.y = +1 is the top row.
        float2 uv = float2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);

        aabbNear = min(aabbNear, depth);
        uvMin = min(uvMin, uv);
        uvMax = max(uvMax, uv);
    }

    // Only a fully off-screen rect is culled here.
    uvMin = clamp(uvMin, float2(0.0, 0.0), float2(1.0, 1.0));
    uvMax = clamp(uvMax, float2(0.0, 0.0), float2(1.0, 1.0));
    if (uvMax.x <= uvMin.x || uvMax.y <= uvMin.y) { visibility[idx] = 0u; return; }

    uint w, h;
    depthTex.GetDimensions(w, h);
    int2 maxCoord = int2(int(w) - 1, int(h) - 1);
    float2 size = float2(float(w), float(h));
    int2 lo = clamp(int2(floor(uvMin * size)), int2(0, 0), maxCoord);
    int2 hi = clamp(int2(ceil(uvMax * size)) - int2(1, 1), lo, maxCoord);
    uint2 extent = uint2(hi - lo) + uint2(1u, 1u);
    // Too many texels to test one by one: drawn, since a sparser test can miss the gap it is seen through.
    if (extent.x * extent.y > MAX_TESTED_TEXELS) { visibility[idx] = 1u; return; }

    // Occluded only when every texel the rect covers is nearer than the box's nearest corner.
    for (int y = lo.y; y <= hi.y; ++y)
    {
        for (int x = lo.x; x <= hi.x; ++x)
        {
            int3 coords = int3(x, y, 0);
            if (depthTex.Load(coords) >= aabbNear) { visibility[idx] = 1u; return; }
        }
    }
    visibility[idx] = 0u;
}