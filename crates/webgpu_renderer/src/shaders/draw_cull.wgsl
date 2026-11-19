struct CullViews_std140_0
{
    @align(16) camera_planes_0 : array<vec4<f32>, i32(6)>,
    @align(16) cascade_planes_0 : array<vec4<f32>, i32(18)>,
    @align(16) eye_0 : vec4<f32>,
    @align(16) prev_eye_0 : vec4<f32>,
    @align(16) slot_count_0 : u32,
    @align(4) cascade_count_0 : u32,
    @align(8) flags_0 : u32,
    @align(4) occlusion_count_0 : u32,
    @align(16) slot_stride_0 : u32,
    @align(4) pad0_0 : u32,
    @align(8) pad1_0 : u32,
    @align(4) pad2_0 : u32,
};

@binding(0) @group(0) var<uniform> views_0 : CullViews_std140_0;
struct DrawInput_std430_0
{
    @align(16) aabb_min_0 : vec4<f32>,
    @align(16) aabb_max_0 : vec4<f32>,
    @align(16) primitive_0 : u32,
    @align(4) flags_1 : u32,
    @align(8) camera_index_count_0 : u32,
    @align(4) shadow_index_count_0 : u32,
    @align(16) instance_count_0 : u32,
    @align(4) pad0_1 : u32,
    @align(8) pad1_1 : u32,
    @align(4) pad2_1 : u32,
};

@binding(1) @group(0) var<storage, read> draws_0 : array<DrawInput_std430_0>;

@binding(4) @group(0) var<storage, read_write> stats_0 : array<atomic<u32>>;

@binding(2) @group(0) var<storage, read> occlusion_0 : array<u32>;

@binding(3) @group(0) var<storage, read_write> args_0 : array<u32>;

fn known_0( lo_0 : vec3<f32>,  hi_0 : vec3<f32>) -> bool
{
    return (all((lo_0 <= hi_0)));
}

fn outside_plane_0( plane_0 : vec4<f32>,  lo_1 : vec3<f32>,  hi_1 : vec3<f32>) -> bool
{
    var _S1 : vec3<f32> = plane_0.xyz;
    if((dot(_S1, _S1)) < 1.00000001686238353e-16f)
    {
        return false;
    }
    var _S2 : f32;
    if((plane_0.x) >= 0.0f)
    {
        _S2 = hi_1.x;
    }
    else
    {
        _S2 = lo_1.x;
    }
    var _S3 : f32;
    if((plane_0.y) >= 0.0f)
    {
        _S3 = hi_1.y;
    }
    else
    {
        _S3 = lo_1.y;
    }
    var _S4 : f32;
    if((plane_0.z) >= 0.0f)
    {
        _S4 = hi_1.z;
    }
    else
    {
        _S4 = lo_1.z;
    }
    return (dot(_S1, vec3<f32>(_S2, _S3, _S4)) + plane_0.w) < -0.00009999999747379f;
}

fn in_camera_0( lo_2 : vec3<f32>,  hi_2 : vec3<f32>) -> bool
{
    if(!known_0(lo_2, hi_2))
    {
        return true;
    }
    var i_0 : u32 = u32(0);
    for(;;)
    {
        if(i_0 < u32(6))
        {
        }
        else
        {
            break;
        }
        if(outside_plane_0(views_0.camera_planes_0[i_0], lo_2, hi_2))
        {
            return false;
        }
        i_0 = i_0 + u32(1);
    }
    return true;
}

fn count_0( stat_0 : u32,  counted_0 : bool)
{
    if(counted_0)
    {
        var _S5 : u32 = atomicAdd(&(stats_0[stat_0]), u32(1));
    }
    return;
}

fn contains_0( lo_3 : vec3<f32>,  hi_3 : vec3<f32>,  p_0 : vec3<f32>) -> bool
{
    var expand_0 : vec3<f32> = (hi_3 - lo_3) * vec3<f32>(0.5f) * vec3<f32>(0.01999999955296516f) + vec3<f32>(0.00999999977648258f, 0.00999999977648258f, 0.00999999977648258f);
    var _S6 : bool;
    if((all((p_0 >= (lo_3 - expand_0)))))
    {
        _S6 = (all((p_0 <= (hi_3 + expand_0))));
    }
    else
    {
        _S6 = false;
    }
    return _S6;
}

fn occluded_0( draw_0 : ptr<function, DrawInput_std430_0>) -> bool
{
    var _S7 : u32 = (*draw_0).primitive_0;
    if(((*draw_0).primitive_0) >= (views_0.occlusion_count_0))
    {
        return false;
    }
    var hidden_0 : bool;
    if((((views_0.flags_0) & (u32(2)))) != u32(0))
    {
        hidden_0 = occlusion_0[_S7] == u32(0);
    }
    else
    {
        if((((views_0.flags_0) & (u32(4)))) != u32(0))
        {
            var _S8 : u32 = u32(2) * _S7;
            hidden_0 = ((occlusion_0[_S8] | (occlusion_0[_S8 + u32(1)]))) == u32(0);
        }
        else
        {
            hidden_0 = false;
        }
    }
    if(!hidden_0)
    {
        return false;
    }
    var _S9 : vec3<f32> = (*draw_0).aabb_min_0.xyz;
    var _S10 : vec3<f32> = (*draw_0).aabb_max_0.xyz;
    if(!contains_0(_S9, _S10, views_0.eye_0.xyz))
    {
        hidden_0 = !contains_0(_S9, _S10, views_0.prev_eye_0.xyz);
    }
    else
    {
        hidden_0 = false;
    }
    return hidden_0;
}

fn write_args_0( slot_0 : u32,  index_count_0 : u32,  instance_count_1 : u32)
{
    var base_0 : u32 = slot_0 * u32(5);
    args_0[base_0] = index_count_0;
    args_0[base_0 + u32(1)] = instance_count_1;
    args_0[base_0 + u32(2)] = u32(0);
    args_0[base_0 + u32(3)] = u32(0);
    args_0[base_0 + u32(4)] = u32(0);
    return;
}

fn cull_camera_0( idx_0 : u32,  draw_1 : ptr<function, DrawInput_std430_0>,  counted_1 : bool)
{
    var lo_4 : vec3<f32> = (*draw_1).aabb_min_0.xyz;
    var hi_4 : vec3<f32> = (*draw_1).aabb_max_0.xyz;
    var frustum_0 : bool;
    if((((views_0.flags_0) & (u32(1)))) == u32(0))
    {
        frustum_0 = true;
    }
    else
    {
        frustum_0 = in_camera_0(lo_4, hi_4);
    }
    var visible_0 : bool;
    if(((((*draw_1).flags_1) & (u32(1)))) != u32(0))
    {
        if(counted_1)
        {
            visible_0 = frustum_0;
        }
        else
        {
            visible_0 = false;
        }
        count_0(u32(2), visible_0);
        visible_0 = frustum_0;
    }
    else
    {
        if(counted_1)
        {
            visible_0 = frustum_0;
        }
        else
        {
            visible_0 = false;
        }
        count_0(u32(0), visible_0);
        if(frustum_0)
        {
            var _S11 : bool = occluded_0(&((*draw_1)));
            visible_0 = !_S11;
        }
        else
        {
            visible_0 = false;
        }
        if(counted_1)
        {
            frustum_0 = visible_0;
        }
        else
        {
            frustum_0 = false;
        }
        count_0(u32(1), frustum_0);
    }
    var _S12 : u32 = (*draw_1).camera_index_count_0;
    var _S13 : u32;
    if(visible_0)
    {
        _S13 = (*draw_1).instance_count_0;
    }
    else
    {
        _S13 = u32(0);
    }
    write_args_0(idx_0, _S12, _S13);
    return;
}

fn in_cascade_0( cascade_0 : u32,  lo_5 : vec3<f32>,  hi_5 : vec3<f32>) -> bool
{
    if(!known_0(lo_5, hi_5))
    {
        return true;
    }
    var i_1 : u32 = u32(0);
    for(;;)
    {
        if(i_1 < u32(6))
        {
        }
        else
        {
            break;
        }
        if(i_1 == u32(4))
        {
            i_1 = i_1 + u32(1);
            continue;
        }
        if(outside_plane_0(views_0.cascade_planes_0[cascade_0 * u32(6) + i_1], lo_5, hi_5))
        {
            return false;
        }
        i_1 = i_1 + u32(1);
    }
    return true;
}

fn cull_cascades_0( idx_1 : u32,  draw_2 : ptr<function, DrawInput_std430_0>,  counted_2 : bool)
{
    var _S14 : bool = ((((*draw_2).flags_1) & (u32(2)))) != u32(0);
    var cascade_1 : u32 = u32(0);
    for(;;)
    {
        if(cascade_1 < (views_0.cascade_count_0))
        {
        }
        else
        {
            break;
        }
        var visible_1 : bool;
        if(_S14)
        {
            if((((views_0.flags_0) & (u32(1)))) == u32(0))
            {
                visible_1 = true;
            }
            else
            {
                visible_1 = in_cascade_0(cascade_1, (*draw_2).aabb_min_0.xyz, (*draw_2).aabb_max_0.xyz);
            }
        }
        else
        {
            visible_1 = false;
        }
        var _S15 : bool;
        if(counted_2)
        {
            _S15 = _S14;
        }
        else
        {
            _S15 = false;
        }
        count_0(u32(3), _S15);
        var _S16 : bool;
        if(counted_2)
        {
            _S16 = visible_1;
        }
        else
        {
            _S16 = false;
        }
        count_0(u32(4), _S16);
        var _S17 : u32 = cascade_1 + u32(1);
        var slot_1 : u32 = _S17 * views_0.slot_stride_0 + idx_1;
        var _S18 : u32 = (*draw_2).shadow_index_count_0;
        var _S19 : u32;
        if(visible_1)
        {
            _S19 = (*draw_2).instance_count_0;
        }
        else
        {
            _S19 = u32(0);
        }
        write_args_0(slot_1, _S18, _S19);
        cascade_1 = _S17;
    }
    return;
}

@compute
@workgroup_size(64, 1, 1)
fn cs_main(@builtin(global_invocation_id) gid_0 : vec3<u32>)
{
    var idx_2 : u32 = gid_0.x;
    if(idx_2 >= (views_0.slot_count_0))
    {
        return;
    }
    var _S20 : DrawInput_std430_0 = draws_0[idx_2];
    var counted_3 : bool = (((_S20.flags_1) & (u32(4)))) != u32(0);
    cull_camera_0(idx_2, &(_S20), counted_3);
    cull_cascades_0(idx_2, &(_S20), counted_3);
    return;
}

