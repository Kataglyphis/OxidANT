import camera_ray;
import clip_y;
import kernel_vectors;

// Every public camera_ray.slang function, one thread per case of vectors/camera_ray.json; both renderers' runners check it.

[shader("compute")]
[numthreads(1, 1, 1)]
void camera_ray_test_main(uint3 tid : SV_DispatchThreadID)
{
    VectorInputs inputs = vector_inputs(tid.x);
    float4 row0 = inputs.f32x4();
    float4 row1 = inputs.f32x4();
    float4 row2 = inputs.f32x4();
    float4 row3 = inputs.f32x4();
    float2 ndc = inputs.f32x2();
    float2 pixel = inputs.f32x2();
    float2 size = inputs.f32x2();
    float4x4 clip_to_world = float4x4(row0, row1, row2, row3);

    VectorOutputs outputs = vector_outputs(tid.x);
    CameraRay ray = camera_ray(clip_to_world, ndc);
    outputs.put(ray.origin);
    outputs.put(ray.direction);
    outputs.put(ray.length);
    // The pixel's NDC y differs per target, so the pixel ray is held to the ray through clip_y.slang's own NDC.
    CameraRay through = camera_ray_through_pixel(clip_to_world, pixel, size);
    float2 uv = pixel / size;
    CameraRay expected = camera_ray(clip_to_world, float2(2.0 * uv.x - 1.0, top_row_clip_y() * (1.0 - 2.0 * uv.y)));
    outputs.put(through.origin - expected.origin);
    outputs.put(through.direction - expected.direction);
}
