// The vector kernels' buffers: two header words (input and output words per case), then each case in its vector file's field order.

[vk::binding(0, 0)] StructuredBuffer<uint> vectorInputs;
[vk::binding(1, 0)] RWStructuredBuffer<uint> vectorOutputs;

// One case's input words, read in the order of the vector file's `inputs`.
struct VectorInputs
{
    uint at;

    [mutating] uint u32()
    {
        uint word = vectorInputs[at];
        at += 1u;
        return word;
    }

    [mutating] int i32()
    {
        return asint(u32());
    }

    [mutating] float f32()
    {
        return asfloat(u32());
    }

    [mutating] float2 f32x2()
    {
        float x = f32();
        float y = f32();
        return float2(x, y);
    }

    [mutating] float3 f32x3()
    {
        float x = f32();
        float y = f32();
        float z = f32();
        return float3(x, y, z);
    }

    [mutating] float4 f32x4()
    {
        float3 xyz = f32x3();
        float w = f32();
        return float4(xyz, w);
    }
};

// One case's output words, written in the order of the vector file's `outputs`.
struct VectorOutputs
{
    uint at;

    [mutating] void put(uint word)
    {
        vectorOutputs[at] = word;
        at += 1u;
    }

    [mutating] void put(float value)
    {
        put(asuint(value));
    }

    [mutating] void put(float2 value)
    {
        put(value.x);
        put(value.y);
    }

    [mutating] void put(float3 value)
    {
        put(value.x);
        put(value.y);
        put(value.z);
    }

    [mutating] void put(float4 value)
    {
        put(value.xyz);
        put(value.w);
    }
};

VectorInputs vector_inputs(uint case_index)
{
    VectorInputs inputs;
    inputs.at = 2u + case_index * vectorInputs[0];
    return inputs;
}

VectorOutputs vector_outputs(uint case_index)
{
    VectorOutputs outputs;
    outputs.at = case_index * vectorInputs[1];
    return outputs;
}
