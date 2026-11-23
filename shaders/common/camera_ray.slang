// The camera's ray through a pixel as both renderers trace it: from the near plane to the far plane.

import fullscreen;

struct CameraRay
{
    float3 origin;     // on the near plane
    float3 direction;  // unit
    float length;      // to the far plane, the ray's tmax
};

// clip_to_world is the inverse view-projection; a camera ray starts at the near plane, as raster clips there.
CameraRay camera_ray(float4x4 clip_to_world, float2 ndc)
{
    float4 near = mul(clip_to_world, float4(ndc, 0.0, 1.0));
    float4 far = mul(clip_to_world, float4(ndc, 1.0, 1.0));
    CameraRay ray;
    ray.origin = near.xyz / near.w;
    float3 span = far.xyz / far.w - ray.origin;
    ray.direction = normalize(span);
    ray.length = length(span);
    return ray;
}

// Through `pixel` of a `size` target, in pixels from its top-left corner; a texel's centre is at + 0.5.
CameraRay camera_ray_through_pixel(float4x4 clip_to_world, float2 pixel, float2 size)
{
    return camera_ray(clip_to_world, fullscreen_uv_to_ndc(pixel / size));
}
