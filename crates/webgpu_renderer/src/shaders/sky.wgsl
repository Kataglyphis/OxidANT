struct _MatrixStorage_float4x4_ColMajorstd140_0
{
    @align(16) data_0 : array<vec4<f32>, i32(4)>,
};

struct SkyUniforms_std140_0
{
    @align(16) inv_view_proj_0 : _MatrixStorage_float4x4_ColMajorstd140_0,
    @align(16) light_dir_intensity_0 : vec4<f32>,
    @align(16) mode_0 : vec4<f32>,
};

@binding(0) @group(0) var<uniform> sky_0 : SkyUniforms_std140_0;
@binding(1) @group(0) var skyboxTex_0 : texture_cube<f32>;

@binding(2) @group(0) var skyboxSampler_0 : sampler;

struct SkyVsOut_0
{
    @builtin(position) svPosition_0 : vec4<f32>,
    @location(0) ndc_0 : vec2<f32>,
};

fn sky_vs_0( vid_0 : u32) -> SkyVsOut_0
{
    var x_0 : f32 = f32(vid_0 / u32(2)) * 4.0f - 1.0f;
    var y_0 : f32 = f32(vid_0 % u32(2)) * 4.0f - 1.0f;
    var o_0 : SkyVsOut_0;
    o_0.svPosition_0 = vec4<f32>(x_0, y_0, 1.0f, 1.0f);
    o_0.ndc_0 = vec2<f32>(x_0, y_0);
    return o_0;
}

@vertex
fn vs_main(@builtin(vertex_index) vid_1 : u32) -> SkyVsOut_0
{
    return sky_vs_0(vid_1);
}

@vertex
fn vs_vulkan_main(@builtin(vertex_index) vid_2 : u32) -> SkyVsOut_0
{
    return sky_vs_0(vid_2);
}

struct CameraRay_0
{
     origin_0 : vec3<f32>,
     direction_0 : vec3<f32>,
     length_0 : f32,
};

fn camera_ray_0( clip_to_world_0 : mat4x4<f32>,  ndc_1 : vec2<f32>) -> CameraRay_0
{
    var near_0 : vec4<f32> = (((vec4<f32>(ndc_1, 0.0f, 1.0f)) * (clip_to_world_0)));
    var far_0 : vec4<f32> = (((vec4<f32>(ndc_1, 1.0f, 1.0f)) * (clip_to_world_0)));
    var ray_0 : CameraRay_0;
    var _S1 : vec3<f32> = near_0.xyz / vec3<f32>(near_0.w);
    ray_0.origin_0 = _S1;
    var span_0 : vec3<f32> = far_0.xyz / vec3<f32>(far_0.w) - _S1;
    ray_0.direction_0 = normalize(span_0);
    ray_0.length_0 = length(span_0);
    return ray_0;
}

fn aces_tonemap_inverse_0( display_0 : vec3<f32>) -> vec3<f32>
{
    var y_1 : vec3<f32> = clamp(display_0, vec3<f32>(0.0f), vec3<f32>(1.0f));
    var qa_0 : vec3<f32> = vec3<f32>(2.50999999046325684f) - vec3<f32>(2.43000006675720215f) * y_1;
    var qb_0 : vec3<f32> = vec3<f32>(0.02999999932944775f) - vec3<f32>(0.5899999737739563f) * y_1;
    return ((vec3<f32>(0) - qb_0) + sqrt(qb_0 * qb_0 + vec3<f32>(4.0f) * qa_0 * vec3<f32>(0.14000000059604645f) * y_1)) / (vec3<f32>(2.0f) * qa_0);
}

fn sky_gradient_0( dir_0 : vec3<f32>) -> vec3<f32>
{
    var _S2 : f32 = dir_0.y;
    if(_S2 >= 0.0f)
    {
        return mix(vec3<f32>(0.55000001192092896f, 0.62000000476837158f, 0.72000002861022949f), vec3<f32>(0.09000000357627869f, 0.15999999642372131f, 0.34999999403953552f), vec3<f32>(pow(clamp(_S2, 0.0f, 1.0f), 0.69999998807907104f)));
    }
    else
    {
        return mix(vec3<f32>(0.55000001192092896f, 0.62000000476837158f, 0.72000002861022949f), vec3<f32>(0.18000000715255737f, 0.15999999642372131f, 0.15000000596046448f), vec3<f32>(clamp(- _S2 * 3.0f, 0.0f, 1.0f)));
    }
}

fn sun_disk_0( dir_1 : vec3<f32>,  lightDir_0 : vec3<f32>,  intensity_0 : f32) -> vec3<f32>
{
    var _S3 : f32 = max(dot(dir_1, normalize(lightDir_0)), 0.0f);
    return vec3<f32>(1.0f, 0.94999998807907104f, 0.85000002384185791f) * vec3<f32>((pow(_S3, 1200.0f) * 24.0f + pow(_S3, 48.0f) * 0.5f)) * vec3<f32>((intensity_0 * 0.40000000596046448f));
}

fn hdr_safe_0( radiance_0 : vec3<f32>) -> vec3<f32>
{
    var _S4 : vec3<f32> = vec3<f32>(0.0f);
    return clamp(select(radiance_0, _S4, (((bitcast<vec3<u32>>((radiance_0))) & (vec3<u32>(u32(2147483647))))) > vec3<u32>(u32(2139095040))), _S4, vec3<f32>(65000.0f));
}

fn drawn_sky_0( mode_1 : f32,  cube_0 : vec3<f32>,  dir_2 : vec3<f32>,  light_dir_0 : vec3<f32>,  sun_intensity_0 : f32) -> vec3<f32>
{
    if(mode_1 > 1.5f)
    {
        return aces_tonemap_inverse_0(vec3<f32>(0.0f, 0.0f, 0.5f));
    }
    if(mode_1 > 0.5f)
    {
        return aces_tonemap_inverse_0(cube_0);
    }
    return hdr_safe_0(sky_gradient_0(dir_2) + sun_disk_0(dir_2, light_dir_0, sun_intensity_0));
}

struct pixelOutput_0
{
    @location(0) output_0 : vec4<f32>,
};

struct pixelInput_0
{
    @location(0) ndc_2 : vec2<f32>,
};

@fragment
fn fs_main( _S5 : pixelInput_0, @builtin(position) svPosition_1 : vec4<f32>) -> pixelOutput_0
{
    var _S6 : CameraRay_0 = camera_ray_0(mat4x4<f32>(sky_0.inv_view_proj_0.data_0[i32(0)][i32(0)], sky_0.inv_view_proj_0.data_0[i32(1)][i32(0)], sky_0.inv_view_proj_0.data_0[i32(2)][i32(0)], sky_0.inv_view_proj_0.data_0[i32(3)][i32(0)], sky_0.inv_view_proj_0.data_0[i32(0)][i32(1)], sky_0.inv_view_proj_0.data_0[i32(1)][i32(1)], sky_0.inv_view_proj_0.data_0[i32(2)][i32(1)], sky_0.inv_view_proj_0.data_0[i32(3)][i32(1)], sky_0.inv_view_proj_0.data_0[i32(0)][i32(2)], sky_0.inv_view_proj_0.data_0[i32(1)][i32(2)], sky_0.inv_view_proj_0.data_0[i32(2)][i32(2)], sky_0.inv_view_proj_0.data_0[i32(3)][i32(2)], sky_0.inv_view_proj_0.data_0[i32(0)][i32(3)], sky_0.inv_view_proj_0.data_0[i32(1)][i32(3)], sky_0.inv_view_proj_0.data_0[i32(2)][i32(3)], sky_0.inv_view_proj_0.data_0[i32(3)][i32(3)]), _S5.ndc_2);
    var _S7 : pixelOutput_0 = pixelOutput_0( vec4<f32>(drawn_sky_0(sky_0.mode_0.x, (textureSampleLevel((skyboxTex_0), (skyboxSampler_0), (_S6.direction_0), (0.0f))).xyz, _S6.direction_0, sky_0.light_dir_intensity_0.xyz, sky_0.light_dir_intensity_0.w), 1.0f) );
    return _S7;
}

