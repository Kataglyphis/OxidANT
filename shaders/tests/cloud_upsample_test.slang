import cloud_upsample;
import kernel_vectors;

// Every public cloud_upsample.slang function, one thread per case of vectors/cloud_upsample.json.

// A scene at one depth but for a single pixel, a half march that ramps with its texel, and distance proportional to depth.
struct SpotTexels : ICloudUpsampleTexels
{
    int2 spot;
    float spotDepth;
    float restDepth;
    float scale;

    float scene_depth(int2 pixel)
    {
        return all(pixel == spot) ? spotDepth : restDepth;
    }

    float4 half_cloud(int2 texel)
    {
        return float4(float(texel.x) * 0.1, float(texel.y) * 0.1, float(texel.x + texel.y) * 0.05, 1.0);
    }

    float scene_distance(float depth)
    {
        return depth * scale;
    }
};

// Two words in order: a constructor's arguments have no evaluation order to rely on.
int2 i32x2(inout VectorInputs inputs)
{
    int x = inputs.i32();
    int y = inputs.i32();
    return int2(x, y);
}

[shader("compute")]
[numthreads(1, 1, 1)]
void cloud_upsample_test_main(uint3 tid : SV_DispatchThreadID)
{
    VectorInputs inputs = vector_inputs(tid.x);
    int2 pixel = i32x2(inputs);
    int2 texel = i32x2(inputs);
    int2 halfSize = i32x2(inputs);
    int2 depthSize = i32x2(inputs);
    SpotTexels texels;
    texels.spot = i32x2(inputs);
    texels.spotDepth = inputs.f32();
    texels.restDepth = inputs.f32();
    texels.scale = inputs.f32();

    VectorOutputs outputs = vector_outputs(tid.x);
    outputs.put(half_texel_depth(texels, texel, depthSize));
    outputs.put(cloud_upsample(texels, pixel, halfSize, depthSize));
}
