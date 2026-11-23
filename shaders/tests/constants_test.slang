import constants;
import trace_constants;

// CI-only entry point: proves constants.slang and trace_constants.slang emit SPIR-V as the C++ shaders import them; nothing loads it.

[vk::binding(0, 0)] RWStructuredBuffer<float> constantsOut;

[shader("compute")]
[numthreads(1, 1, 1)]
void constants_test_main(uint3 tid : SV_DispatchThreadID)
{
    constantsOut[0] = PI;
    constantsOut[1] = MIN_ROUGHNESS;
    constantsOut[2] = TRACE_TMIN;
    constantsOut[3] = TRACE_TMAX;
    constantsOut[4] = TRACE_SURFACE_OFFSET;
    constantsOut[5] = TRACE_LIGHT_SPAN;
}
