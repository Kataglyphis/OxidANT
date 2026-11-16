struct TraceDebugParams_std140_0
{
    @align(16) ray_count_0 : u32,
    @align(4) pad0_0 : u32,
    @align(8) pad1_0 : u32,
    @align(4) pad2_0 : u32,
};

@binding(0) @group(0) var<uniform> debugParams_0 : TraceDebugParams_std140_0;
@binding(1) @group(0) var<storage, read> debugRays_0 : array<vec4<f32>>;

struct TraceSceneInfo_std140_0
{
    @align(16) tlas_root_0 : u32,
    @align(4) instance_base_0 : u32,
    @align(8) material_base_0 : u32,
    @align(4) instance_count_0 : u32,
    @align(16) material_count_0 : u32,
    @align(4) srgb_layer_size_0 : u32,
    @align(8) linear_layer_size_0 : u32,
    @align(4) flags_0 : u32,
};

@binding(4) @group(1) var<uniform> tracerInfo_0 : TraceSceneInfo_std140_0;
@binding(0) @group(1) var<storage, read> tracerRecords_0 : array<vec4<f32>>;

@binding(9) @group(1) var<storage, read> tracerNodeMax_0 : array<vec4<f32>>;

@binding(1) @group(1) var<storage, read> tracerTriangles_0 : array<vec4<u32>>;

@binding(2) @group(1) var<storage, read> tracerPositions_0 : array<vec4<f32>>;

@binding(3) @group(1) var<storage, read> tracerVertices_0 : array<vec4<u32>>;

@binding(10) @group(1) var<storage, read> tracerVertexExtras_0 : array<vec4<u32>>;

@binding(5) @group(1) var tracerSrgbLayers_0 : texture_2d_array<f32>;

@binding(8) @group(1) var tracerNearestSampler_0 : sampler;

@binding(7) @group(1) var tracerLinearSampler_0 : sampler;

@binding(6) @group(1) var tracerLinearLayers_0 : texture_2d_array<f32>;

@binding(2) @group(0) var<storage, read_write> debugHits_0 : array<vec4<f32>>;

struct TraceRay_0
{
     origin_0 : vec3<f32>,
     tmin_0 : f32,
     direction_0 : vec3<f32>,
     tmax_0 : f32,
};

fn debug_ray_0( index_0 : u32) -> TraceRay_0
{
    var _S1 : u32 = index_0 * u32(2);
    var a_0 : vec4<f32> = debugRays_0[_S1];
    var b_0 : vec4<f32> = debugRays_0[_S1 + u32(1)];
    var ray_0 : TraceRay_0;
    ray_0.origin_0 = a_0.xyz;
    ray_0.tmin_0 = a_0.w;
    ray_0.direction_0 = b_0.xyz;
    ray_0.tmax_0 = b_0.w;
    return ray_0;
}

struct TraceHit_0
{
     t_0 : f32,
     instance_0 : u32,
     triangle_0 : u32,
     barycentrics_0 : vec2<f32>,
};

fn trace_miss_0( tmax_1 : f32) -> TraceHit_0
{
    var hit_0 : TraceHit_0;
    hit_0.t_0 = tmax_1;
    hit_0.instance_0 = u32(4294967295);
    hit_0.triangle_0 = u32(0);
    hit_0.barycentrics_0 = vec2<f32>(0.0f, 0.0f);
    return hit_0;
}

fn nudge_0( c_0 : f32) -> f32
{
    var _S2 : f32;
    if((abs(c_0)) > 1.00000000317107685e-30f)
    {
        _S2 = c_0;
    }
    else
    {
        if(c_0 < 0.0f)
        {
            _S2 = -1.00000000317107685e-30f;
        }
        else
        {
            _S2 = 1.00000000317107685e-30f;
        }
    }
    return _S2;
}

fn safe_inverse_0( d_0 : vec3<f32>) -> vec3<f32>
{
    return vec3<f32>(1.0f / nudge_0(d_0.x), 1.0f / nudge_0(d_0.y), 1.0f / nudge_0(d_0.z));
}

struct BvhNode_0
{
     lo_0 : vec3<f32>,
     first_0 : u32,
     hi_0 : vec3<f32>,
     count_0 : u32,
};

fn load_node_0( index_1 : u32) -> BvhNode_0
{
    var a_1 : vec4<f32> = tracerRecords_0[index_1];
    var b_1 : vec4<f32> = tracerNodeMax_0[index_1];
    var node_0 : BvhNode_0;
    node_0.lo_0 = a_1.xyz;
    node_0.first_0 = (bitcast<u32>((a_1.w)));
    node_0.hi_0 = b_1.xyz;
    node_0.count_0 = (bitcast<u32>((b_1.w)));
    return node_0;
}

fn box_entry_0( node_1 : BvhNode_0,  origin_1 : vec3<f32>,  inv_0 : vec3<f32>,  tmin_1 : f32,  tmax_2 : f32) -> f32
{
    var t0_0 : vec3<f32> = (node_1.lo_0 - origin_1) * inv_0;
    var t1_0 : vec3<f32> = (node_1.hi_0 - origin_1) * inv_0;
    var lo_1 : vec3<f32> = min(t0_0, t1_0);
    var hi_1 : vec3<f32> = max(t0_0, t1_0);
    var _S3 : f32 = max(max(lo_1.x, lo_1.y), max(lo_1.z, tmin_1));
    var _S4 : f32;
    if(_S3 <= (min(min(hi_1.x, hi_1.y), min(hi_1.z, tmax_2)) * 1.00000035762786865f))
    {
        _S4 = _S3;
    }
    else
    {
        _S4 = 3.00000000549775576e+38f;
    }
    return _S4;
}

struct TraceInstance_0
{
     world_to_object_0 : array<vec4<f32>, i32(3)>,
     object_to_world_0 : array<vec4<f32>, i32(3)>,
     blas_root_0 : u32,
     flags_1 : u32,
     primitive_0 : u32,
     mesh_0 : u32,
};

fn load_instance_0( index_2 : u32) -> TraceInstance_0
{
    var base_0 : u32 = tracerInfo_0.instance_base_0 + index_2 * u32(7);
    var inst_0 : TraceInstance_0;
    var row_0 : u32 = u32(0);
    for(;;)
    {
        if(row_0 < u32(3))
        {
        }
        else
        {
            break;
        }
        inst_0.world_to_object_0[row_0] = tracerRecords_0[base_0 + row_0];
        inst_0.object_to_world_0[row_0] = tracerRecords_0[base_0 + u32(3) + row_0];
        row_0 = row_0 + u32(1);
    }
    var tail_0 : vec4<u32> = (bitcast<vec4<u32>>((tracerRecords_0[base_0 + u32(6)])));
    inst_0.blas_root_0 = tail_0.x;
    inst_0.flags_1 = tail_0.y;
    inst_0.primitive_0 = tail_0.z;
    inst_0.mesh_0 = tail_0.w;
    return inst_0;
}

fn transform_point_0( rows_0 : array<vec4<f32>, i32(3)>,  p_0 : vec3<f32>) -> vec3<f32>
{
    var h_0 : vec4<f32> = vec4<f32>(p_0, 1.0f);
    return vec3<f32>(dot(rows_0[i32(0)], h_0), dot(rows_0[i32(1)], h_0), dot(rows_0[i32(2)], h_0));
}

fn transform_vector_0( rows_1 : array<vec4<f32>, i32(3)>,  v_0 : vec3<f32>) -> vec3<f32>
{
    return vec3<f32>(dot(rows_1[i32(0)].xyz, v_0), dot(rows_1[i32(1)].xyz, v_0), dot(rows_1[i32(2)].xyz, v_0));
}

fn trace_position_0( vertex_0 : u32) -> vec3<f32>
{
    return tracerPositions_0[vertex_0].xyz;
}

fn intersect_triangle_0( o_0 : vec3<f32>,  d_1 : vec3<f32>,  p0_0 : vec3<f32>,  p1_0 : vec3<f32>,  p2_0 : vec3<f32>) -> vec3<f32>
{
    var e1_0 : vec3<f32> = p1_0 - p0_0;
    var e2_0 : vec3<f32> = p2_0 - p0_0;
    var p_1 : vec3<f32> = cross(d_1, e2_0);
    var det_0 : f32 = dot(e1_0, p_1);
    const miss_0 : vec3<f32> = vec3<f32>(-1.0f, 0.0f, 0.0f);
    if(det_0 == 0.0f)
    {
        return miss_0;
    }
    var inv_1 : f32 = 1.0f / det_0;
    var s_0 : vec3<f32> = o_0 - p0_0;
    var u_0 : f32 = dot(s_0, p_1) * inv_1;
    var _S5 : bool;
    if(u_0 < 0.0f)
    {
        _S5 = true;
    }
    else
    {
        _S5 = u_0 > 1.0f;
    }
    if(_S5)
    {
        return miss_0;
    }
    var q_0 : vec3<f32> = cross(s_0, e1_0);
    var v_1 : f32 = dot(d_1, q_0) * inv_1;
    if(v_1 < 0.0f)
    {
        _S5 = true;
    }
    else
    {
        _S5 = (u_0 + v_1) > 1.0f;
    }
    if(_S5)
    {
        return miss_0;
    }
    return vec3<f32>(dot(e2_0, q_0) * inv_1, u_0, v_1);
}

fn trace_closer_0( t_1 : f32,  instance_1 : u32,  triangle_1 : u32,  best_0 : TraceHit_0) -> bool
{
    var _S6 : bool;
    if(t_1 < (best_0.t_0))
    {
        _S6 = true;
    }
    else
    {
        if(t_1 == (best_0.t_0))
        {
            if(instance_1 < (best_0.instance_0))
            {
                _S6 = true;
            }
            else
            {
                if(instance_1 == (best_0.instance_0))
                {
                    _S6 = triangle_1 < (best_0.triangle_0);
                }
                else
                {
                    _S6 = false;
                }
            }
        }
        else
        {
            _S6 = false;
        }
    }
    return _S6;
}

struct TraceMaterial_0
{
     base_color_0 : vec4<f32>,
     emissive_0 : vec3<f32>,
     alpha_cutoff_0 : f32,
     metallic_0 : f32,
     roughness_0 : f32,
     occlusion_strength_0 : f32,
     normal_scale_0 : f32,
     flags_2 : u32,
     layers_0 : vec3<u32>,
};

fn load_material_0( index_3 : u32) -> TraceMaterial_0
{
    var base_1 : u32 = tracerInfo_0.material_base_0 + index_3 * u32(14);
    var emissive_1 : vec4<f32> = tracerRecords_0[base_1 + u32(1)];
    var factors_0 : vec4<f32> = tracerRecords_0[base_1 + u32(2)];
    var words_0 : vec4<u32> = (bitcast<vec4<u32>>((tracerRecords_0[base_1 + u32(3)])));
    var m_0 : TraceMaterial_0;
    m_0.base_color_0 = tracerRecords_0[base_1];
    m_0.emissive_0 = emissive_1.xyz;
    m_0.alpha_cutoff_0 = emissive_1.w;
    m_0.metallic_0 = factors_0.x;
    m_0.roughness_0 = factors_0.y;
    m_0.occlusion_strength_0 = factors_0.z;
    m_0.normal_scale_0 = factors_0.w;
    m_0.flags_2 = words_0.x;
    m_0.layers_0 = words_0.yzw;
    return m_0;
}

fn unpack_snorm2_0( bits_0 : u32) -> vec2<f32>
{
    return max(vec2<f32>(f32((i32((bits_0 << (u32(16)))) >> (u32(16)))), f32((i32(bits_0) >> (u32(16))))) / vec2<f32>(32767.0f), vec2<f32>(-1.0f, -1.0f));
}

fn oct_decode_0( e_0 : vec2<f32>) -> vec3<f32>
{
    var _S7 : f32 = e_0.x;
    var _S8 : f32 = e_0.y;
    var _S9 : f32 = 1.0f - abs(_S7) - abs(_S8);
    var v_2 : vec3<f32> = vec3<f32>(_S7, _S8, _S9);
    var _S10 : f32 = max(- _S9, 0.0f);
    var _S11 : f32;
    if(_S7 >= 0.0f)
    {
        _S11 = - _S10;
    }
    else
    {
        _S11 = _S10;
    }
    v_2[i32(0)] = v_2[i32(0)] + _S11;
    if((v_2.y) >= 0.0f)
    {
        _S11 = - _S10;
    }
    else
    {
        _S11 = _S10;
    }
    v_2[i32(1)] = v_2[i32(1)] + _S11;
    return normalize(v_2);
}

fn unpack_unorm2_0( bits_1 : u32) -> vec2<f32>
{
    return vec2<f32>(f32((bits_1 & (u32(65535)))), f32((bits_1 >> (u32(16))))) / vec2<f32>(65535.0f);
}

struct TraceVertex_0
{
     normal_0 : vec3<f32>,
     tangent_0 : vec4<f32>,
     uv0_0 : vec2<f32>,
     uv1_0 : vec2<f32>,
     color_0 : vec4<f32>,
};

fn load_vertex_0( index_4 : u32) -> TraceVertex_0
{
    var a_2 : vec4<u32> = tracerVertices_0[index_4];
    var b_2 : vec4<u32> = tracerVertexExtras_0[index_4];
    var v_3 : TraceVertex_0;
    v_3.normal_0 = oct_decode_0(unpack_snorm2_0(a_2.x));
    var _S12 : u32 = a_2.y;
    var handedness_0 : f32;
    if(((_S12 & (u32(65536)))) != u32(0))
    {
        handedness_0 = -1.0f;
    }
    else
    {
        handedness_0 = 1.0f;
    }
    v_3.tangent_0 = vec4<f32>(oct_decode_0(unpack_snorm2_0((_S12 & (u32(4294901759))))), handedness_0);
    v_3.uv0_0 = (bitcast<vec2<f32>>((a_2.zw)));
    v_3.uv1_0 = (bitcast<vec2<f32>>((b_2.xy)));
    v_3.color_0 = vec4<f32>(unpack_unorm2_0(b_2.z), unpack_unorm2_0(b_2.w));
    return v_3;
}

fn material_layer_0( m_1 : TraceMaterial_0,  slot_0 : u32) -> u32
{
    var word_0 : u32;
    if(slot_0 < u32(2))
    {
        word_0 = m_1.layers_0.x;
    }
    else
    {
        if(slot_0 < u32(4))
        {
            word_0 = m_1.layers_0.y;
        }
        else
        {
            word_0 = m_1.layers_0.z;
        }
    }
    if(((slot_0 & (u32(1)))) == u32(0))
    {
        word_0 = (word_0 & (u32(65535)));
    }
    else
    {
        word_0 = (word_0 >> (u32(16)));
    }
    return word_0;
}

fn material_slot_uv_0( material_0 : u32,  m_2 : TraceMaterial_0,  slot_1 : u32,  uv0_1 : vec2<f32>,  uv1_1 : vec2<f32>) -> vec2<f32>
{
    var uv_0 : vec2<f32>;
    if((((((m_2.flags_2) >> ((u32(4) + slot_1)))) & (u32(1)))) != u32(0))
    {
        uv_0 = uv1_1;
    }
    else
    {
        uv_0 = uv0_1;
    }
    var base_2 : u32 = tracerInfo_0.material_base_0 + material_0 * u32(14) + u32(4) + u32(2) * slot_1;
    var r0_0 : vec4<f32> = tracerRecords_0[base_2];
    var r1_0 : vec4<f32> = tracerRecords_0[base_2 + u32(1)];
    var _S13 : f32 = uv_0.x;
    var _S14 : f32 = uv_0.y;
    return vec2<f32>(r0_0.x * _S13 + r0_0.y * _S14 + r0_0.z, r1_0.x * _S13 + r1_0.y * _S14 + r1_0.z);
}

fn wrap_coordinate_0( c_1 : f32,  mode_0 : u32,  half_texel_0 : f32) -> f32
{
    var _S15 : bool = mode_0 == u32(1);
    var _S16 : f32;
    if(_S15)
    {
        _S16 = 1.0f - abs(fract(c_1 * 0.5f) * 2.0f - 1.0f);
    }
    else
    {
        _S16 = c_1;
    }
    var _S17 : bool;
    if(_S15)
    {
        _S17 = true;
    }
    else
    {
        _S17 = mode_0 == u32(2);
    }
    if(_S17)
    {
        _S16 = clamp(_S16, half_texel_0, 1.0f - half_texel_0);
    }
    return _S16;
}

fn sample_material_slot_0( material_1 : u32,  m_3 : TraceMaterial_0,  slot_2 : u32,  uv0_2 : vec2<f32>,  uv1_2 : vec2<f32>,  lod_0 : f32) -> vec4<f32>
{
    var layer_0 : u32 = material_layer_0(m_3, slot_2);
    var _S18 : vec4<f32>;
    if(layer_0 == u32(65535))
    {
        if(slot_2 == u32(2))
        {
            _S18 = vec4<f32>(0.5f, 0.5f, 1.0f, 1.0f);
        }
        else
        {
            _S18 = vec4<f32>(1.0f, 1.0f, 1.0f, 1.0f);
        }
        return _S18;
    }
    var srgb_0 : bool;
    if(slot_2 == u32(0))
    {
        srgb_0 = true;
    }
    else
    {
        srgb_0 = slot_2 == u32(3);
    }
    var _S19 : u32;
    if(srgb_0)
    {
        _S19 = tracerInfo_0.srgb_layer_size_0;
    }
    else
    {
        _S19 = tracerInfo_0.linear_layer_size_0;
    }
    var half_texel_1 : f32 = 0.5f * exp2(max(lod_0, 0.0f)) / f32(_S19);
    var wrap_0 : u32 = ((m_3.flags_2) >> ((u32(9) + u32(4) * slot_2)));
    var uv_1 : vec2<f32> = material_slot_uv_0(material_1, m_3, slot_2, uv0_2, uv1_2);
    var coord_0 : vec3<f32> = vec3<f32>(wrap_coordinate_0(uv_1.x, (wrap_0 & (u32(3))), half_texel_1), wrap_coordinate_0(uv_1.y, (((wrap_0 >> (u32(2)))) & (u32(3))), half_texel_1), f32(layer_0));
    var nearest_0 : bool = (((((m_3.layers_0.z) >> ((u32(16) + slot_2)))) & (u32(1)))) != u32(0);
    if(srgb_0)
    {
        if(nearest_0)
        {
            _S18 = (textureSampleLevel((tracerSrgbLayers_0), (tracerNearestSampler_0), ((coord_0)).xy, i32(((coord_0)).z), (lod_0)));
        }
        else
        {
            _S18 = (textureSampleLevel((tracerSrgbLayers_0), (tracerLinearSampler_0), ((coord_0)).xy, i32(((coord_0)).z), (lod_0)));
        }
        return _S18;
    }
    if(nearest_0)
    {
        _S18 = (textureSampleLevel((tracerLinearLayers_0), (tracerNearestSampler_0), ((coord_0)).xy, i32(((coord_0)).z), (lod_0)));
    }
    else
    {
        _S18 = (textureSampleLevel((tracerLinearLayers_0), (tracerLinearSampler_0), ((coord_0)).xy, i32(((coord_0)).z), (lod_0)));
    }
    return _S18;
}

fn alpha_masked_out_0( tri_0 : vec4<u32>,  barycentrics_1 : vec2<f32>) -> bool
{
    var material_2 : u32 = ((tri_0.w) & (u32(16777215)));
    var m_4 : TraceMaterial_0 = load_material_0(material_2);
    if((((m_4.flags_2) & (u32(3)))) != u32(1))
    {
        return false;
    }
    var _S20 : f32 = barycentrics_1.x;
    var _S21 : f32 = barycentrics_1.y;
    var _S22 : f32 = 1.0f - _S20 - _S21;
    var a_3 : TraceVertex_0 = load_vertex_0(tri_0.x);
    var b_3 : TraceVertex_0 = load_vertex_0(tri_0.y);
    var c_2 : TraceVertex_0 = load_vertex_0(tri_0.z);
    var _S23 : vec2<f32> = vec2<f32>(_S22);
    var _S24 : vec2<f32> = vec2<f32>(_S20);
    var _S25 : vec2<f32> = vec2<f32>(_S21);
    return (m_4.base_color_0.w * (a_3.color_0.w * _S22 + b_3.color_0.w * _S20 + c_2.color_0.w * _S21) * sample_material_slot_0(material_2, m_4, u32(0), a_3.uv0_0 * _S23 + b_3.uv0_0 * _S24 + c_2.uv0_0 * _S25, a_3.uv1_0 * _S23 + b_3.uv1_0 * _S24 + c_2.uv1_0 * _S25, 0.0f).w) < (m_4.alpha_cutoff_0);
}

fn trace_scene_0( ray_1 : TraceRay_0,  any_hit_0 : bool) -> TraceHit_0
{
    var origin_2 : vec3<f32>;
    var inv_2 : vec3<f32>;
    var direction_1 : vec3<f32>;
    var blas_floor_0 : u32;
    var best_1 : TraceHit_0 = trace_miss_0(ray_1.tmax_0);
    if((tracerInfo_0.instance_count_0) == u32(0))
    {
        return best_1;
    }
    var inv_3 : vec3<f32> = safe_inverse_0(ray_1.direction_0);
    var stack_0 : array<u32, i32(96)>;
    var node_2 : BvhNode_0 = load_node_0(tracerInfo_0.tlas_root_0);
    var live_0 : bool = (box_entry_0(node_2, ray_1.origin_0, inv_3, ray_1.tmin_0, best_1.t_0)) < 3.00000000549775576e+38f;
    var node_3 : BvhNode_0 = node_2;
    var origin_3 : vec3<f32> = ray_1.origin_0;
    var inv_4 : vec3<f32> = inv_3;
    var blas_floor_1 : u32 = u32(4294967295);
    var direction_2 : vec3<f32> = ray_1.direction_0;
    var instance_2 : u32 = u32(0);
    var sp_0 : u32 = u32(0);
    for(;;)
    {
        if(live_0)
        {
        }
        else
        {
            break;
        }
        var _S26 : BvhNode_0 = node_3;
        var _S27 : bool;
        var node_4 : BvhNode_0;
        var origin_4 : vec3<f32>;
        var inv_5 : vec3<f32>;
        var direction_3 : vec3<f32>;
        var blas_floor_2 : u32;
        var blas_floor_3 : u32;
        if((node_3.count_0) == u32(0))
        {
            var _S28 : BvhNode_0 = node_3;
            var a_4 : BvhNode_0 = load_node_0(node_3.first_0);
            var _S29 : u32 = node_3.first_0 + u32(1);
            var b_4 : BvhNode_0 = load_node_0(_S29);
            var ta_0 : f32 = box_entry_0(a_4, origin_3, inv_4, ray_1.tmin_0, best_1.t_0);
            var tb_0 : f32 = box_entry_0(b_4, origin_3, inv_4, ray_1.tmin_0, best_1.t_0);
            var flip_0 : bool = tb_0 < ta_0;
            var t_near_0 : f32;
            if(flip_0)
            {
                t_near_0 = tb_0;
            }
            else
            {
                t_near_0 = ta_0;
            }
            var t_far_0 : f32;
            if(flip_0)
            {
                t_far_0 = ta_0;
            }
            else
            {
                t_far_0 = tb_0;
            }
            if(t_near_0 < 3.00000000549775576e+38f)
            {
                if(t_far_0 < 3.00000000549775576e+38f)
                {
                    _S27 = sp_0 < u32(96);
                }
                else
                {
                    _S27 = false;
                }
                if(_S27)
                {
                    if(flip_0)
                    {
                        blas_floor_2 = _S28.first_0;
                    }
                    else
                    {
                        blas_floor_2 = _S29;
                    }
                    stack_0[sp_0] = blas_floor_2;
                    sp_0 = sp_0 + u32(1);
                }
                if(flip_0)
                {
                    node_3 = b_4;
                }
                else
                {
                    node_3 = a_4;
                }
                continue;
            }
            blas_floor_2 = blas_floor_1;
            origin_4 = origin_3;
            inv_5 = inv_4;
            node_4 = node_3;
            direction_3 = direction_2;
        }
        else
        {
            if(blas_floor_1 == u32(4294967295))
            {
                var _S30 : BvhNode_0 = node_3;
                var inst_1 : TraceInstance_0 = load_instance_0(node_3.first_0);
                var origin_5 : vec3<f32> = transform_point_0(inst_1.world_to_object_0, ray_1.origin_0);
                var direction_4 : vec3<f32> = transform_vector_0(inst_1.world_to_object_0, ray_1.direction_0);
                var inv_6 : vec3<f32> = safe_inverse_0(direction_4);
                var node_5 : BvhNode_0 = load_node_0(inst_1.blas_root_0);
                if((box_entry_0(node_5, origin_5, inv_6, ray_1.tmin_0, best_1.t_0)) < 3.00000000549775576e+38f)
                {
                    node_3 = node_5;
                    origin_3 = origin_5;
                    inv_4 = inv_6;
                    blas_floor_1 = sp_0;
                    direction_2 = direction_4;
                    instance_2 = _S30.first_0;
                    continue;
                }
                blas_floor_2 = sp_0;
                origin_4 = origin_5;
                inv_5 = inv_6;
                node_4 = node_5;
                direction_3 = direction_4;
                blas_floor_3 = _S30.first_0;
            }
            else
            {
                blas_floor_2 = u32(0);
                for(;;)
                {
                    if(blas_floor_2 < (_S26.count_0))
                    {
                    }
                    else
                    {
                        break;
                    }
                    var triangle_2 : u32 = node_3.first_0 + blas_floor_2;
                    var tri_1 : vec4<u32> = tracerTriangles_0[triangle_2];
                    var hit_1 : vec3<f32> = intersect_triangle_0(origin_3, direction_2, trace_position_0(tri_1.x), trace_position_0(tri_1.y), trace_position_0(tri_1.z));
                    var _S31 : f32 = hit_1.x;
                    if(_S31 < (ray_1.tmin_0))
                    {
                        _S27 = true;
                    }
                    else
                    {
                        _S27 = !trace_closer_0(_S31, instance_2, triangle_2, best_1);
                    }
                    if(_S27)
                    {
                        blas_floor_2 = blas_floor_2 + u32(1);
                        continue;
                    }
                    var _S32 : bool;
                    if((((tri_1.w) & (u32(2147483648)))) != u32(0))
                    {
                        _S32 = alpha_masked_out_0(tri_1, hit_1.yz);
                    }
                    else
                    {
                        _S32 = false;
                    }
                    if(_S32)
                    {
                        blas_floor_2 = blas_floor_2 + u32(1);
                        continue;
                    }
                    best_1.t_0 = _S31;
                    best_1.instance_0 = instance_2;
                    best_1.triangle_0 = triangle_2;
                    best_1.barycentrics_0 = hit_1.yz;
                    if(any_hit_0)
                    {
                        return best_1;
                    }
                    blas_floor_2 = blas_floor_2 + u32(1);
                }
                blas_floor_2 = blas_floor_1;
                origin_4 = origin_3;
                inv_5 = inv_4;
                node_4 = node_3;
                direction_3 = direction_2;
                blas_floor_3 = instance_2;
            }
            instance_2 = blas_floor_3;
        }
        blas_floor_3 = blas_floor_2;
        var origin_6 : vec3<f32> = origin_4;
        var inv_7 : vec3<f32> = inv_5;
        var node_6 : BvhNode_0 = node_4;
        var direction_5 : vec3<f32> = direction_3;
        var sp_1 : u32 = sp_0;
        for(;;)
        {
            if(sp_1 == blas_floor_3)
            {
                origin_2 = ray_1.origin_0;
                inv_2 = inv_3;
                blas_floor_0 = u32(4294967295);
                direction_1 = ray_1.direction_0;
            }
            else
            {
                origin_2 = origin_6;
                inv_2 = inv_7;
                blas_floor_0 = blas_floor_3;
                direction_1 = direction_5;
            }
            if(sp_1 == u32(0))
            {
                live_0 = false;
                node_3 = node_6;
                sp_0 = sp_1;
                break;
            }
            var sp_2 : u32 = sp_1 - u32(1);
            var node_7 : BvhNode_0 = load_node_0(stack_0[sp_2]);
            if((box_entry_0(node_7, origin_2, inv_2, ray_1.tmin_0, best_1.t_0)) < 3.00000000549775576e+38f)
            {
                live_0 = true;
                node_3 = node_7;
                sp_0 = sp_2;
                break;
            }
            blas_floor_3 = blas_floor_0;
            origin_6 = origin_2;
            inv_7 = inv_2;
            node_6 = node_7;
            direction_5 = direction_1;
            sp_1 = sp_2;
        }
        origin_3 = origin_2;
        inv_4 = inv_2;
        blas_floor_1 = blas_floor_0;
        direction_2 = direction_1;
    }
    return best_1;
}

fn trace_closest_0( ray_2 : TraceRay_0) -> TraceHit_0
{
    return trace_scene_0(ray_2, false);
}

fn transform_normal_0( world_to_object_1 : array<vec4<f32>, i32(3)>,  n_0 : vec3<f32>) -> vec3<f32>
{
    return world_to_object_1[i32(0)].xyz * vec3<f32>(n_0.x) + world_to_object_1[i32(1)].xyz * vec3<f32>(n_0.y) + world_to_object_1[i32(2)].xyz * vec3<f32>(n_0.z);
}

struct TraceSurface_0
{
     position_0 : vec3<f32>,
     geometric_normal_0 : vec3<f32>,
     normal_1 : vec3<f32>,
     tangent_1 : vec4<f32>,
     uv0_3 : vec2<f32>,
     uv1_3 : vec2<f32>,
     color_1 : vec4<f32>,
     material_3 : u32,
     front_face_0 : bool,
};

fn trace_surface_0( ray_3 : TraceRay_0,  hit_2 : TraceHit_0) -> TraceSurface_0
{
    var tri_2 : vec4<u32> = tracerTriangles_0[hit_2.triangle_0];
    var inst_2 : TraceInstance_0 = load_instance_0(hit_2.instance_0);
    var _S33 : f32 = hit_2.barycentrics_0.x;
    var _S34 : f32 = hit_2.barycentrics_0.y;
    var _S35 : f32 = 1.0f - _S33 - _S34;
    var _S36 : u32 = tri_2.x;
    var p0_1 : vec3<f32> = trace_position_0(_S36);
    var _S37 : u32 = tri_2.y;
    var p1_1 : vec3<f32> = trace_position_0(_S37);
    var _S38 : u32 = tri_2.z;
    var p2_1 : vec3<f32> = trace_position_0(_S38);
    var a_5 : TraceVertex_0 = load_vertex_0(_S36);
    var b_5 : TraceVertex_0 = load_vertex_0(_S37);
    var c_3 : TraceVertex_0 = load_vertex_0(_S38);
    var s_1 : TraceSurface_0;
    s_1.position_0 = ray_3.origin_0 + ray_3.direction_0 * vec3<f32>(hit_2.t_0);
    s_1.geometric_normal_0 = normalize(transform_normal_0(inst_2.world_to_object_0, cross(p1_1 - p0_1, p2_1 - p0_1)));
    var n_1 : vec3<f32> = transform_normal_0(inst_2.world_to_object_0, a_5.normal_0 * vec3<f32>(_S35) + b_5.normal_0 * vec3<f32>(_S33) + c_3.normal_0 * vec3<f32>(_S34));
    var _S39 : vec3<f32>;
    if((dot(n_1, n_1)) > 1.00000000317107685e-30f)
    {
        _S39 = normalize(n_1);
    }
    else
    {
        _S39 = s_1.geometric_normal_0;
    }
    s_1.normal_1 = _S39;
    var _S40 : vec4<f32> = vec4<f32>(_S35);
    var _S41 : vec4<f32> = vec4<f32>(_S33);
    var _S42 : vec4<f32> = vec4<f32>(_S34);
    var t_2 : vec4<f32> = a_5.tangent_0 * _S40 + b_5.tangent_0 * _S41 + c_3.tangent_0 * _S42;
    var tw_0 : vec3<f32> = transform_vector_0(inst_2.object_to_world_0, t_2.xyz);
    var mirror_0 : f32;
    if((((inst_2.flags_1) & (u32(1)))) != u32(0))
    {
        mirror_0 = -1.0f;
    }
    else
    {
        mirror_0 = 1.0f;
    }
    if((dot(tw_0, tw_0)) > 1.00000000317107685e-30f)
    {
        _S39 = normalize(tw_0);
    }
    else
    {
        _S39 = vec3<f32>(0.0f, 0.0f, 0.0f);
    }
    var _S43 : f32;
    if((t_2.w) < 0.0f)
    {
        _S43 = -1.0f;
    }
    else
    {
        _S43 = 1.0f;
    }
    s_1.tangent_1 = vec4<f32>(_S39, _S43 * mirror_0);
    var _S44 : vec2<f32> = vec2<f32>(_S35);
    var _S45 : vec2<f32> = vec2<f32>(_S33);
    var _S46 : vec2<f32> = vec2<f32>(_S34);
    s_1.uv0_3 = a_5.uv0_0 * _S44 + b_5.uv0_0 * _S45 + c_3.uv0_0 * _S46;
    s_1.uv1_3 = a_5.uv1_0 * _S44 + b_5.uv1_0 * _S45 + c_3.uv1_0 * _S46;
    s_1.color_1 = a_5.color_0 * _S40 + b_5.color_0 * _S41 + c_3.color_0 * _S42;
    s_1.material_3 = ((tri_2.w) & (u32(16777215)));
    s_1.front_face_0 = (dot(s_1.geometric_normal_0, ray_3.direction_0)) < 0.0f;
    return s_1;
}

@compute
@workgroup_size(64, 1, 1)
fn cs_trace_closest(@builtin(global_invocation_id) id_0 : vec3<u32>)
{
    var _S47 : u32 = id_0.x;
    if(_S47 >= (debugParams_0.ray_count_0))
    {
        return;
    }
    var ray_4 : TraceRay_0 = debug_ray_0(_S47);
    var hit_3 : TraceHit_0 = trace_closest_0(ray_4);
    var out_index_0 : u32 = _S47 * u32(4);
    debugHits_0[out_index_0] = vec4<f32>(hit_3.t_0, (bitcast<f32>((hit_3.instance_0))), (bitcast<f32>((hit_3.triangle_0))), hit_3.barycentrics_0.x);
    if((hit_3.instance_0) == u32(4294967295))
    {
        const _S48 : vec4<f32> = vec4<f32>(0.0f, 0.0f, 0.0f, 0.0f);
        debugHits_0[out_index_0 + u32(1)] = _S48;
        debugHits_0[out_index_0 + u32(2)] = _S48;
        debugHits_0[out_index_0 + u32(3)] = _S48;
        return;
    }
    var s_2 : TraceSurface_0 = trace_surface_0(ray_4, hit_3);
    var base_3 : vec4<f32> = sample_material_slot_0(s_2.material_3, load_material_0(s_2.material_3), u32(0), s_2.uv0_3, s_2.uv1_3, 0.0f);
    var _S49 : f32 = hit_3.barycentrics_0.y;
    var _S50 : f32 = (bitcast<f32>((s_2.material_3)));
    var _S51 : f32;
    if(s_2.front_face_0)
    {
        _S51 = 1.0f;
    }
    else
    {
        _S51 = 0.0f;
    }
    debugHits_0[out_index_0 + u32(1)] = vec4<f32>(_S49, _S50, _S51, s_2.tangent_1.w);
    debugHits_0[out_index_0 + u32(2)] = vec4<f32>(s_2.normal_1, s_2.color_1.w);
    debugHits_0[out_index_0 + u32(3)] = vec4<f32>(s_2.uv0_3, base_3.x, base_3.w);
    return;
}

fn trace_any_0( ray_5 : TraceRay_0) -> bool
{
    return (trace_scene_0(ray_5, true).instance_0) != u32(4294967295);
}

@compute
@workgroup_size(64, 1, 1)
fn cs_trace_any(@builtin(global_invocation_id) id_1 : vec3<u32>)
{
    var _S52 : u32 = id_1.x;
    if(_S52 >= (debugParams_0.ray_count_0))
    {
        return;
    }
    var _S53 : f32;
    if(trace_any_0(debug_ray_0(_S52)))
    {
        _S53 = 1.0f;
    }
    else
    {
        _S53 = 0.0f;
    }
    debugHits_0[_S52 * u32(4)] = vec4<f32>(_S53, 0.0f, 0.0f, 0.0f);
    return;
}

