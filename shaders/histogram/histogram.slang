// Both renderers' auto exposure (histogram.wgsl and the C++ SPIR-V); common/exposure.slang holds its maths.

import exposure;

struct ExposureParams
{
    // x: seconds since the previous frame, y: adaptation rate (0 snaps), z: 1 when auto is on, w: the manual EV
    float4 values;
};

[vk::binding(0, 0)] Texture2D<float4> hdrTex;
// Atomic<uint> emits WGSL's atomic<u32>; InterlockedAdd does not compile for WGSL.
[vk::binding(1, 0)] RWStructuredBuffer<Atomic<uint>> histogram;
// [adapted EV, target EV]: the tonemap and bloom read it, so no frame reads back to the CPU.
[vk::binding(2, 0)] RWStructuredBuffer<float> exposureState;
[vk::binding(3, 0)] ConstantBuffer<ExposureParams> exposureParams;

// One sky-dominated bin would serialise every pixel's global atomic, so each workgroup counts here first.
groupshared Atomic<uint> localHist[HISTOGRAM_BINS];

// One thread per pixel; 256 threads cover the 64 bins, so thread i zeroes and flushes bin i.
[shader("compute")]
[numthreads(16, 16, 1)]
void cs_build_histogram(uint3 id : SV_DispatchThreadID, uint localIndex : SV_GroupIndex)
{
    if (localIndex < HISTOGRAM_BINS)
        localHist[localIndex].store(0u);
    GroupMemoryBarrierWithGroupSync();

    uint width, height;
    hdrTex.GetDimensions(width, height);
    // A branch, not an early return: WGSL forbids a barrier after non-uniform control flow.
    if (id.x < width && id.y < height)
    {
        float3 color = hdrTex.Load(int3(int2(id.xy), 0)).rgb;
        localHist[histogram_bin(exposure_luminance(color))].add(1u);
    }
    GroupMemoryBarrierWithGroupSync();

    if (localIndex < HISTOGRAM_BINS)
    {
        uint count = localHist[localIndex].load();
        if (count > 0u)
            histogram[localIndex].add(count);
    }
}

// A kernel, not a queue write, so the reset stays on the GPU timeline behind the previous frame's reduction.
[shader("compute")]
[numthreads(64, 1, 1)]
void cs_clear_histogram(uint3 id : SV_DispatchThreadID)
{
    if (id.x < HISTOGRAM_BINS)
        histogram[id.x].store(0u);
}

// One invocation: 64 bins are too little work for a parallel reduction.
[shader("compute")]
[numthreads(1, 1, 1)]
void cs_reduce_exposure()
{
    float dt = exposureParams.values.x;
    float speed = exposureParams.values.y;
    if (exposureParams.values.z < 0.5)
    {
        // Manual mode writes the buffer too, so switching modes leaves no stale auto value.
        exposureState[0] = exposureParams.values.w;
        exposureState[1] = exposureParams.values.w;
        return;
    }

    // The geometric mean over the non-black bins.
    float weightedLogSum = 0.0;
    float counted = 0.0;
    for (uint bin = 1u; bin < HISTOGRAM_BINS; bin++)
    {
        float count = float(histogram[bin].load());
        if (count > 0.0)
        {
            weightedLogSum += log2(bin_luminance(bin)) * count;
            counted += count;
        }
    }

    float currentEv = exposureState[0];
    if (counted <= 0.0)
    {
        // Nothing but black: hold, or the first frame before a scene loads drives the exposure to infinity.
        exposureState[1] = currentEv;
        return;
    }

    float targetEv = exposure_target_ev(weightedLogSum / counted);
    exposureState[0] = adapt_exposure_ev(currentEv, targetEv, dt, speed);
    exposureState[1] = targetEv;
}
