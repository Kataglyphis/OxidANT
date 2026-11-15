struct TraceTile_std140_0
{
    @align(16) rect_0 : vec4<u32>,
    @align(16) samples_0 : vec4<u32>,
};

@binding(2) @group(0) var<uniform> traceTile_0 : TraceTile_std140_0;
struct _MatrixStorage_float4x4_ColMajorstd140_0
{
    @align(16) data_0 : array<vec4<f32>, i32(4)>,
};

struct TraceFrame_std140_0
{
    @align(16) size_0 : vec4<u32>,
    @align(16) inv_view_proj_0 : _MatrixStorage_float4x4_ColMajorstd140_0,
    @align(16) light_dir_0 : vec4<f32>,
    @align(16) light_radiance_0 : vec4<f32>,
    @align(16) furnace_0 : vec4<f32>,
};

@binding(0) @group(0) var<uniform> traceFrame_0 : TraceFrame_std140_0;
@binding(1) @group(0) var<storage, read_write> traceAccumulation_0 : array<vec4<f32>>;

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

@binding(1) @group(1) var<storage, read> tracerTriangles_0 : array<vec4<u32>>;

@binding(2) @group(1) var<storage, read> tracerPositions_0 : array<vec4<f32>>;

@binding(3) @group(1) var<storage, read> tracerVertices_0 : array<vec4<u32>>;

@binding(5) @group(1) var tracerSrgbLayers_0 : texture_2d_array<f32>;

@binding(8) @group(1) var tracerNearestSampler_0 : sampler;

@binding(7) @group(1) var tracerLinearSampler_0 : sampler;

@binding(6) @group(1) var tracerLinearLayers_0 : texture_2d_array<f32>;

fn tile_pixel_0( thread_0 : vec2<u32>,  pixel_0 : ptr<function, vec2<u32>>) -> bool
{
    (*pixel_0) = traceTile_0.rect_0.xy + thread_0;
    var _S1 : bool;
    if((thread_0.x) < (traceTile_0.rect_0.z))
    {
        _S1 = (thread_0.y) < (traceTile_0.rect_0.w);
    }
    else
    {
        _S1 = false;
    }
    return _S1;
}

fn pixel_index_0( pixel_1 : vec2<u32>) -> u32
{
    return pixel_1.y * traceFrame_0.size_0.x + pixel_1.x;
}

struct TraceRay_0
{
     origin_0 : vec3<f32>,
     tmin_0 : f32,
     direction_0 : vec3<f32>,
     tmax_0 : f32,
};

fn camera_ray_0( pixel_2 : vec2<f32>) -> TraceRay_0
{
    var _S2 : vec2<f32> = vec2<f32>(traceFrame_0.size_0.xy);
    var ndc_0 : vec2<f32> = vec2<f32>(pixel_2.x / _S2.x * 2.0f - 1.0f, 1.0f - pixel_2.y / _S2.y * 2.0f);
    var near_0 : vec4<f32> = (((vec4<f32>(ndc_0, 0.0f, 1.0f)) * (mat4x4<f32>(traceFrame_0.inv_view_proj_0.data_0[i32(0)][i32(0)], traceFrame_0.inv_view_proj_0.data_0[i32(1)][i32(0)], traceFrame_0.inv_view_proj_0.data_0[i32(2)][i32(0)], traceFrame_0.inv_view_proj_0.data_0[i32(3)][i32(0)], traceFrame_0.inv_view_proj_0.data_0[i32(0)][i32(1)], traceFrame_0.inv_view_proj_0.data_0[i32(1)][i32(1)], traceFrame_0.inv_view_proj_0.data_0[i32(2)][i32(1)], traceFrame_0.inv_view_proj_0.data_0[i32(3)][i32(1)], traceFrame_0.inv_view_proj_0.data_0[i32(0)][i32(2)], traceFrame_0.inv_view_proj_0.data_0[i32(1)][i32(2)], traceFrame_0.inv_view_proj_0.data_0[i32(2)][i32(2)], traceFrame_0.inv_view_proj_0.data_0[i32(3)][i32(2)], traceFrame_0.inv_view_proj_0.data_0[i32(0)][i32(3)], traceFrame_0.inv_view_proj_0.data_0[i32(1)][i32(3)], traceFrame_0.inv_view_proj_0.data_0[i32(2)][i32(3)], traceFrame_0.inv_view_proj_0.data_0[i32(3)][i32(3)]))));
    var far_0 : vec4<f32> = (((vec4<f32>(ndc_0, 1.0f, 1.0f)) * (mat4x4<f32>(traceFrame_0.inv_view_proj_0.data_0[i32(0)][i32(0)], traceFrame_0.inv_view_proj_0.data_0[i32(1)][i32(0)], traceFrame_0.inv_view_proj_0.data_0[i32(2)][i32(0)], traceFrame_0.inv_view_proj_0.data_0[i32(3)][i32(0)], traceFrame_0.inv_view_proj_0.data_0[i32(0)][i32(1)], traceFrame_0.inv_view_proj_0.data_0[i32(1)][i32(1)], traceFrame_0.inv_view_proj_0.data_0[i32(2)][i32(1)], traceFrame_0.inv_view_proj_0.data_0[i32(3)][i32(1)], traceFrame_0.inv_view_proj_0.data_0[i32(0)][i32(2)], traceFrame_0.inv_view_proj_0.data_0[i32(1)][i32(2)], traceFrame_0.inv_view_proj_0.data_0[i32(2)][i32(2)], traceFrame_0.inv_view_proj_0.data_0[i32(3)][i32(2)], traceFrame_0.inv_view_proj_0.data_0[i32(0)][i32(3)], traceFrame_0.inv_view_proj_0.data_0[i32(1)][i32(3)], traceFrame_0.inv_view_proj_0.data_0[i32(2)][i32(3)], traceFrame_0.inv_view_proj_0.data_0[i32(3)][i32(3)]))));
    var origin_1 : vec3<f32> = near_0.xyz / vec3<f32>(near_0.w);
    var span_0 : vec3<f32> = far_0.xyz / vec3<f32>(far_0.w) - origin_1;
    var ray_0 : TraceRay_0;
    ray_0.origin_0 = origin_1;
    ray_0.tmin_0 = 0.0f;
    ray_0.direction_0 = normalize(span_0);
    ray_0.tmax_0 = length(span_0);
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
    var _S3 : f32;
    if((abs(c_0)) > 1.00000000317107685e-30f)
    {
        _S3 = c_0;
    }
    else
    {
        if(c_0 < 0.0f)
        {
            _S3 = -1.00000000317107685e-30f;
        }
        else
        {
            _S3 = 1.00000000317107685e-30f;
        }
    }
    return _S3;
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

fn load_node_0( index_0 : u32) -> BvhNode_0
{
    var _S4 : u32 = index_0 * u32(2);
    var a_0 : vec4<f32> = tracerRecords_0[_S4];
    var b_0 : vec4<f32> = tracerRecords_0[_S4 + u32(1)];
    var node_0 : BvhNode_0;
    node_0.lo_0 = a_0.xyz;
    node_0.first_0 = (bitcast<u32>((a_0.w)));
    node_0.hi_0 = b_0.xyz;
    node_0.count_0 = (bitcast<u32>((b_0.w)));
    return node_0;
}

fn box_entry_0( node_1 : BvhNode_0,  origin_2 : vec3<f32>,  inv_0 : vec3<f32>,  tmin_1 : f32,  tmax_2 : f32) -> f32
{
    var t0_0 : vec3<f32> = (node_1.lo_0 - origin_2) * inv_0;
    var t1_0 : vec3<f32> = (node_1.hi_0 - origin_2) * inv_0;
    var lo_1 : vec3<f32> = min(t0_0, t1_0);
    var hi_1 : vec3<f32> = max(t0_0, t1_0);
    var _S5 : f32 = max(max(lo_1.x, lo_1.y), max(lo_1.z, tmin_1));
    var _S6 : f32;
    if(_S5 <= (min(min(hi_1.x, hi_1.y), min(hi_1.z, tmax_2)) * 1.00000035762786865f))
    {
        _S6 = _S5;
    }
    else
    {
        _S6 = 3.00000000549775576e+38f;
    }
    return _S6;
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

fn load_instance_0( index_1 : u32) -> TraceInstance_0
{
    var base_0 : u32 = tracerInfo_0.instance_base_0 + index_1 * u32(7);
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
    var _S7 : bool;
    if(u_0 < 0.0f)
    {
        _S7 = true;
    }
    else
    {
        _S7 = u_0 > 1.0f;
    }
    if(_S7)
    {
        return miss_0;
    }
    var q_0 : vec3<f32> = cross(s_0, e1_0);
    var v_1 : f32 = dot(d_1, q_0) * inv_1;
    if(v_1 < 0.0f)
    {
        _S7 = true;
    }
    else
    {
        _S7 = (u_0 + v_1) > 1.0f;
    }
    if(_S7)
    {
        return miss_0;
    }
    return vec3<f32>(dot(e2_0, q_0) * inv_1, u_0, v_1);
}

fn trace_closer_0( t_1 : f32,  instance_1 : u32,  triangle_1 : u32,  best_0 : TraceHit_0) -> bool
{
    var _S8 : bool;
    if(t_1 < (best_0.t_0))
    {
        _S8 = true;
    }
    else
    {
        if(t_1 == (best_0.t_0))
        {
            if(instance_1 < (best_0.instance_0))
            {
                _S8 = true;
            }
            else
            {
                if(instance_1 == (best_0.instance_0))
                {
                    _S8 = triangle_1 < (best_0.triangle_0);
                }
                else
                {
                    _S8 = false;
                }
            }
        }
        else
        {
            _S8 = false;
        }
    }
    return _S8;
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

fn load_material_0( index_2 : u32) -> TraceMaterial_0
{
    var base_1 : u32 = tracerInfo_0.material_base_0 + index_2 * u32(14);
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
    var _S9 : f32 = e_0.x;
    var _S10 : f32 = e_0.y;
    var _S11 : f32 = 1.0f - abs(_S9) - abs(_S10);
    var v_2 : vec3<f32> = vec3<f32>(_S9, _S10, _S11);
    var _S12 : f32 = max(- _S11, 0.0f);
    var _S13 : f32;
    if(_S9 >= 0.0f)
    {
        _S13 = - _S12;
    }
    else
    {
        _S13 = _S12;
    }
    v_2[i32(0)] = v_2[i32(0)] + _S13;
    if((v_2.y) >= 0.0f)
    {
        _S13 = - _S12;
    }
    else
    {
        _S13 = _S12;
    }
    v_2[i32(1)] = v_2[i32(1)] + _S13;
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

fn load_vertex_0( index_3 : u32) -> TraceVertex_0
{
    var _S14 : u32 = index_3 * u32(2);
    var a_1 : vec4<u32> = tracerVertices_0[_S14];
    var b_1 : vec4<u32> = tracerVertices_0[_S14 + u32(1)];
    var v_3 : TraceVertex_0;
    v_3.normal_0 = oct_decode_0(unpack_snorm2_0(a_1.x));
    var _S15 : u32 = a_1.y;
    var handedness_0 : f32;
    if(((_S15 & (u32(65536)))) != u32(0))
    {
        handedness_0 = -1.0f;
    }
    else
    {
        handedness_0 = 1.0f;
    }
    v_3.tangent_0 = vec4<f32>(oct_decode_0(unpack_snorm2_0((_S15 & (u32(4294901759))))), handedness_0);
    v_3.uv0_0 = (bitcast<vec2<f32>>((a_1.zw)));
    v_3.uv1_0 = (bitcast<vec2<f32>>((b_1.xy)));
    v_3.color_0 = vec4<f32>(unpack_unorm2_0(b_1.z), unpack_unorm2_0(b_1.w));
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
    var _S16 : f32 = uv_0.x;
    var _S17 : f32 = uv_0.y;
    return vec2<f32>(r0_0.x * _S16 + r0_0.y * _S17 + r0_0.z, r1_0.x * _S16 + r1_0.y * _S17 + r1_0.z);
}

fn wrap_coordinate_0( c_1 : f32,  mode_0 : u32,  half_texel_0 : f32) -> f32
{
    var _S18 : bool = mode_0 == u32(1);
    var _S19 : f32;
    if(_S18)
    {
        _S19 = 1.0f - abs(fract(c_1 * 0.5f) * 2.0f - 1.0f);
    }
    else
    {
        _S19 = c_1;
    }
    var _S20 : bool;
    if(_S18)
    {
        _S20 = true;
    }
    else
    {
        _S20 = mode_0 == u32(2);
    }
    if(_S20)
    {
        _S19 = clamp(_S19, half_texel_0, 1.0f - half_texel_0);
    }
    return _S19;
}

fn sample_material_slot_0( material_1 : u32,  m_3 : TraceMaterial_0,  slot_2 : u32,  uv0_2 : vec2<f32>,  uv1_2 : vec2<f32>,  lod_0 : f32) -> vec4<f32>
{
    var layer_0 : u32 = material_layer_0(m_3, slot_2);
    var _S21 : vec4<f32>;
    if(layer_0 == u32(65535))
    {
        if(slot_2 == u32(2))
        {
            _S21 = vec4<f32>(0.5f, 0.5f, 1.0f, 1.0f);
        }
        else
        {
            _S21 = vec4<f32>(1.0f, 1.0f, 1.0f, 1.0f);
        }
        return _S21;
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
    var _S22 : u32;
    if(srgb_0)
    {
        _S22 = tracerInfo_0.srgb_layer_size_0;
    }
    else
    {
        _S22 = tracerInfo_0.linear_layer_size_0;
    }
    var half_texel_1 : f32 = 0.5f * exp2(max(lod_0, 0.0f)) / f32(_S22);
    var wrap_0 : u32 = ((m_3.flags_2) >> ((u32(9) + u32(4) * slot_2)));
    var uv_1 : vec2<f32> = material_slot_uv_0(material_1, m_3, slot_2, uv0_2, uv1_2);
    var coord_0 : vec3<f32> = vec3<f32>(wrap_coordinate_0(uv_1.x, (wrap_0 & (u32(3))), half_texel_1), wrap_coordinate_0(uv_1.y, (((wrap_0 >> (u32(2)))) & (u32(3))), half_texel_1), f32(layer_0));
    var nearest_0 : bool = (((((m_3.layers_0.z) >> ((u32(16) + slot_2)))) & (u32(1)))) != u32(0);
    if(srgb_0)
    {
        if(nearest_0)
        {
            _S21 = (textureSampleLevel((tracerSrgbLayers_0), (tracerNearestSampler_0), ((coord_0)).xy, i32(((coord_0)).z), (lod_0)));
        }
        else
        {
            _S21 = (textureSampleLevel((tracerSrgbLayers_0), (tracerLinearSampler_0), ((coord_0)).xy, i32(((coord_0)).z), (lod_0)));
        }
        return _S21;
    }
    if(nearest_0)
    {
        _S21 = (textureSampleLevel((tracerLinearLayers_0), (tracerNearestSampler_0), ((coord_0)).xy, i32(((coord_0)).z), (lod_0)));
    }
    else
    {
        _S21 = (textureSampleLevel((tracerLinearLayers_0), (tracerLinearSampler_0), ((coord_0)).xy, i32(((coord_0)).z), (lod_0)));
    }
    return _S21;
}

fn alpha_masked_out_0( tri_0 : vec4<u32>,  barycentrics_1 : vec2<f32>) -> bool
{
    var material_2 : u32 = ((tri_0.w) & (u32(16777215)));
    var m_4 : TraceMaterial_0 = load_material_0(material_2);
    if((((m_4.flags_2) & (u32(3)))) != u32(1))
    {
        return false;
    }
    var _S23 : f32 = barycentrics_1.x;
    var _S24 : f32 = barycentrics_1.y;
    var _S25 : f32 = 1.0f - _S23 - _S24;
    var a_2 : TraceVertex_0 = load_vertex_0(tri_0.x);
    var b_2 : TraceVertex_0 = load_vertex_0(tri_0.y);
    var c_2 : TraceVertex_0 = load_vertex_0(tri_0.z);
    var _S26 : vec2<f32> = vec2<f32>(_S25);
    var _S27 : vec2<f32> = vec2<f32>(_S23);
    var _S28 : vec2<f32> = vec2<f32>(_S24);
    return (m_4.base_color_0.w * (a_2.color_0.w * _S25 + b_2.color_0.w * _S23 + c_2.color_0.w * _S24) * sample_material_slot_0(material_2, m_4, u32(0), a_2.uv0_0 * _S26 + b_2.uv0_0 * _S27 + c_2.uv0_0 * _S28, a_2.uv1_0 * _S26 + b_2.uv1_0 * _S27 + c_2.uv1_0 * _S28, 0.0f).w) < (m_4.alpha_cutoff_0);
}

fn trace_scene_0( ray_1 : TraceRay_0,  any_hit_0 : bool) -> TraceHit_0
{
    var origin_3 : vec3<f32>;
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
    var origin_4 : vec3<f32> = ray_1.origin_0;
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
        var _S29 : BvhNode_0 = node_3;
        var _S30 : bool;
        var node_4 : BvhNode_0;
        var origin_5 : vec3<f32>;
        var inv_5 : vec3<f32>;
        var direction_3 : vec3<f32>;
        var blas_floor_2 : u32;
        var blas_floor_3 : u32;
        if((node_3.count_0) == u32(0))
        {
            var _S31 : BvhNode_0 = node_3;
            var a_3 : BvhNode_0 = load_node_0(node_3.first_0);
            var _S32 : u32 = node_3.first_0 + u32(1);
            var b_3 : BvhNode_0 = load_node_0(_S32);
            var ta_0 : f32 = box_entry_0(a_3, origin_4, inv_4, ray_1.tmin_0, best_1.t_0);
            var tb_0 : f32 = box_entry_0(b_3, origin_4, inv_4, ray_1.tmin_0, best_1.t_0);
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
                    _S30 = sp_0 < u32(96);
                }
                else
                {
                    _S30 = false;
                }
                if(_S30)
                {
                    if(flip_0)
                    {
                        blas_floor_2 = _S31.first_0;
                    }
                    else
                    {
                        blas_floor_2 = _S32;
                    }
                    stack_0[sp_0] = blas_floor_2;
                    sp_0 = sp_0 + u32(1);
                }
                if(flip_0)
                {
                    node_3 = b_3;
                }
                else
                {
                    node_3 = a_3;
                }
                continue;
            }
            blas_floor_2 = blas_floor_1;
            origin_5 = origin_4;
            inv_5 = inv_4;
            node_4 = node_3;
            direction_3 = direction_2;
        }
        else
        {
            if(blas_floor_1 == u32(4294967295))
            {
                var _S33 : BvhNode_0 = node_3;
                var inst_1 : TraceInstance_0 = load_instance_0(node_3.first_0);
                var origin_6 : vec3<f32> = transform_point_0(inst_1.world_to_object_0, ray_1.origin_0);
                var direction_4 : vec3<f32> = transform_vector_0(inst_1.world_to_object_0, ray_1.direction_0);
                var inv_6 : vec3<f32> = safe_inverse_0(direction_4);
                var node_5 : BvhNode_0 = load_node_0(inst_1.blas_root_0);
                if((box_entry_0(node_5, origin_6, inv_6, ray_1.tmin_0, best_1.t_0)) < 3.00000000549775576e+38f)
                {
                    node_3 = node_5;
                    origin_4 = origin_6;
                    inv_4 = inv_6;
                    blas_floor_1 = sp_0;
                    direction_2 = direction_4;
                    instance_2 = _S33.first_0;
                    continue;
                }
                blas_floor_2 = sp_0;
                origin_5 = origin_6;
                inv_5 = inv_6;
                node_4 = node_5;
                direction_3 = direction_4;
                blas_floor_3 = _S33.first_0;
            }
            else
            {
                blas_floor_2 = u32(0);
                for(;;)
                {
                    if(blas_floor_2 < (_S29.count_0))
                    {
                    }
                    else
                    {
                        break;
                    }
                    var triangle_2 : u32 = node_3.first_0 + blas_floor_2;
                    var tri_1 : vec4<u32> = tracerTriangles_0[triangle_2];
                    var hit_1 : vec3<f32> = intersect_triangle_0(origin_4, direction_2, trace_position_0(tri_1.x), trace_position_0(tri_1.y), trace_position_0(tri_1.z));
                    var _S34 : f32 = hit_1.x;
                    if(_S34 < (ray_1.tmin_0))
                    {
                        _S30 = true;
                    }
                    else
                    {
                        _S30 = !trace_closer_0(_S34, instance_2, triangle_2, best_1);
                    }
                    if(_S30)
                    {
                        blas_floor_2 = blas_floor_2 + u32(1);
                        continue;
                    }
                    var _S35 : bool;
                    if((((tri_1.w) & (u32(2147483648)))) != u32(0))
                    {
                        _S35 = alpha_masked_out_0(tri_1, hit_1.yz);
                    }
                    else
                    {
                        _S35 = false;
                    }
                    if(_S35)
                    {
                        blas_floor_2 = blas_floor_2 + u32(1);
                        continue;
                    }
                    best_1.t_0 = _S34;
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
                origin_5 = origin_4;
                inv_5 = inv_4;
                node_4 = node_3;
                direction_3 = direction_2;
                blas_floor_3 = instance_2;
            }
            instance_2 = blas_floor_3;
        }
        blas_floor_3 = blas_floor_2;
        var origin_7 : vec3<f32> = origin_5;
        var inv_7 : vec3<f32> = inv_5;
        var node_6 : BvhNode_0 = node_4;
        var direction_5 : vec3<f32> = direction_3;
        var sp_1 : u32 = sp_0;
        for(;;)
        {
            if(sp_1 == blas_floor_3)
            {
                origin_3 = ray_1.origin_0;
                inv_2 = inv_3;
                blas_floor_0 = u32(4294967295);
                direction_1 = ray_1.direction_0;
            }
            else
            {
                origin_3 = origin_7;
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
            if((box_entry_0(node_7, origin_3, inv_2, ray_1.tmin_0, best_1.t_0)) < 3.00000000549775576e+38f)
            {
                live_0 = true;
                node_3 = node_7;
                sp_0 = sp_2;
                break;
            }
            blas_floor_3 = blas_floor_0;
            origin_7 = origin_3;
            inv_7 = inv_2;
            node_6 = node_7;
            direction_5 = direction_1;
            sp_1 = sp_2;
        }
        origin_4 = origin_3;
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

fn sky_gradient_0( dir_0 : vec3<f32>) -> vec3<f32>
{
    var _S36 : f32 = dir_0.y;
    if(_S36 >= 0.0f)
    {
        return mix(vec3<f32>(0.55000001192092896f, 0.62000000476837158f, 0.72000002861022949f), vec3<f32>(0.09000000357627869f, 0.15999999642372131f, 0.34999999403953552f), vec3<f32>(pow(clamp(_S36, 0.0f, 1.0f), 0.69999998807907104f)));
    }
    else
    {
        return mix(vec3<f32>(0.55000001192092896f, 0.62000000476837158f, 0.72000002861022949f), vec3<f32>(0.18000000715255737f, 0.15999999642372131f, 0.15000000596046448f), vec3<f32>(clamp(- _S36 * 3.0f, 0.0f, 1.0f)));
    }
}

fn sun_disk_0( dir_1 : vec3<f32>,  lightDir_0 : vec3<f32>,  intensity_0 : f32) -> vec3<f32>
{
    var _S37 : f32 = max(dot(dir_1, normalize(lightDir_0)), 0.0f);
    return vec3<f32>(1.0f, 0.94999998807907104f, 0.85000002384185791f) * vec3<f32>((pow(_S37, 1200.0f) * 24.0f + pow(_S37, 48.0f) * 0.5f)) * vec3<f32>((intensity_0 * 0.40000000596046448f));
}

fn sky_with_sun_0( dir_2 : vec3<f32>) -> vec3<f32>
{
    return sky_gradient_0(dir_2) + sun_disk_0(dir_2, traceFrame_0.light_dir_0.xyz, traceFrame_0.light_dir_0.w);
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
    var _S38 : f32 = hit_2.barycentrics_0.x;
    var _S39 : f32 = hit_2.barycentrics_0.y;
    var _S40 : f32 = 1.0f - _S38 - _S39;
    var _S41 : u32 = tri_2.x;
    var p0_1 : vec3<f32> = trace_position_0(_S41);
    var _S42 : u32 = tri_2.y;
    var p1_1 : vec3<f32> = trace_position_0(_S42);
    var _S43 : u32 = tri_2.z;
    var p2_1 : vec3<f32> = trace_position_0(_S43);
    var a_4 : TraceVertex_0 = load_vertex_0(_S41);
    var b_4 : TraceVertex_0 = load_vertex_0(_S42);
    var c_3 : TraceVertex_0 = load_vertex_0(_S43);
    var s_1 : TraceSurface_0;
    s_1.position_0 = ray_3.origin_0 + ray_3.direction_0 * vec3<f32>(hit_2.t_0);
    s_1.geometric_normal_0 = normalize(transform_normal_0(inst_2.world_to_object_0, cross(p1_1 - p0_1, p2_1 - p0_1)));
    var n_1 : vec3<f32> = transform_normal_0(inst_2.world_to_object_0, a_4.normal_0 * vec3<f32>(_S40) + b_4.normal_0 * vec3<f32>(_S38) + c_3.normal_0 * vec3<f32>(_S39));
    var _S44 : vec3<f32>;
    if((dot(n_1, n_1)) > 1.00000000317107685e-30f)
    {
        _S44 = normalize(n_1);
    }
    else
    {
        _S44 = s_1.geometric_normal_0;
    }
    s_1.normal_1 = _S44;
    var _S45 : vec4<f32> = vec4<f32>(_S40);
    var _S46 : vec4<f32> = vec4<f32>(_S38);
    var _S47 : vec4<f32> = vec4<f32>(_S39);
    var t_2 : vec4<f32> = a_4.tangent_0 * _S45 + b_4.tangent_0 * _S46 + c_3.tangent_0 * _S47;
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
        _S44 = normalize(tw_0);
    }
    else
    {
        _S44 = vec3<f32>(0.0f, 0.0f, 0.0f);
    }
    var _S48 : f32;
    if((t_2.w) < 0.0f)
    {
        _S48 = -1.0f;
    }
    else
    {
        _S48 = 1.0f;
    }
    s_1.tangent_1 = vec4<f32>(_S44, _S48 * mirror_0);
    var _S49 : vec2<f32> = vec2<f32>(_S40);
    var _S50 : vec2<f32> = vec2<f32>(_S38);
    var _S51 : vec2<f32> = vec2<f32>(_S39);
    s_1.uv0_3 = a_4.uv0_0 * _S49 + b_4.uv0_0 * _S50 + c_3.uv0_0 * _S51;
    s_1.uv1_3 = a_4.uv1_0 * _S49 + b_4.uv1_0 * _S50 + c_3.uv1_0 * _S51;
    s_1.color_1 = a_4.color_0 * _S45 + b_4.color_0 * _S46 + c_3.color_0 * _S47;
    s_1.material_3 = ((tri_2.w) & (u32(16777215)));
    s_1.front_face_0 = (dot(s_1.geometric_normal_0, ray_3.direction_0)) < 0.0f;
    return s_1;
}

fn normalize_or_0( v_4 : vec3<f32>,  fallback_0 : vec3<f32>) -> vec3<f32>
{
    var _S52 : vec3<f32>;
    if((dot(v_4, v_4)) > 1.00000000317107685e-30f)
    {
        _S52 = normalize(v_4);
    }
    else
    {
        _S52 = fallback_0;
    }
    return _S52;
}

fn mapped_normal_0( s_2 : TraceSurface_0,  texel_0 : vec4<f32>,  scale_0 : f32) -> vec3<f32>
{
    var _S53 : vec3<f32> = s_2.tangent_1.xyz;
    var t_3 : vec3<f32> = normalize_or_0(_S53 - s_2.normal_1 * vec3<f32>(dot(s_2.normal_1, _S53)), vec3<f32>(0.0f));
    var b_5 : vec3<f32> = cross(s_2.normal_1, t_3) * vec3<f32>(s_2.tangent_1.w);
    var n_2 : vec3<f32> = texel_0.xyz * vec3<f32>(2.0f) - vec3<f32>(1.0f);
    if((texel_0.z) == 0.0f)
    {
        n_2[i32(2)] = sqrt(saturate(1.0f - dot(n_2.xy, n_2.xy)));
    }
    var _S54 : vec3<f32> = vec3<f32>(n_2.xy * vec2<f32>(scale_0), n_2.z);
    n_2 = _S54;
    var mapped_0 : vec3<f32> = normalize_or_0((((mat3x3<f32>(t_3, b_5, s_2.normal_1)) * (_S54))), s_2.normal_1);
    var _S55 : vec3<f32>;
    if(s_2.front_face_0)
    {
        _S55 = mapped_0;
    }
    else
    {
        _S55 = (vec3<f32>(0) - mapped_0);
    }
    return _S55;
}

struct TracedSurface_0
{
     position_1 : vec3<f32>,
     face_normal_0 : vec3<f32>,
     normal_2 : vec3<f32>,
     albedo_0 : vec3<f32>,
     emissive_2 : vec3<f32>,
     metallic_1 : f32,
     roughness_1 : f32,
     unlit_0 : bool,
};

fn shade_hit_0( ray_4 : TraceRay_0,  hit_3 : TraceHit_0) -> TracedSurface_0
{
    var s_3 : TraceSurface_0 = trace_surface_0(ray_4, hit_3);
    var m_5 : TraceMaterial_0 = load_material_0(s_3.material_3);
    var base_3 : vec4<f32> = sample_material_slot_0(s_3.material_3, m_5, u32(0), s_3.uv0_3, s_3.uv1_3, 0.0f);
    var mr_0 : vec4<f32> = sample_material_slot_0(s_3.material_3, m_5, u32(1), s_3.uv0_3, s_3.uv1_3, 0.0f);
    var normal_3 : vec4<f32> = sample_material_slot_0(s_3.material_3, m_5, u32(2), s_3.uv0_3, s_3.uv1_3, 0.0f);
    var emission_0 : vec4<f32> = sample_material_slot_0(s_3.material_3, m_5, u32(3), s_3.uv0_3, s_3.uv1_3, 0.0f);
    var o_1 : TracedSurface_0;
    o_1.position_1 = s_3.position_0;
    var _S56 : vec3<f32>;
    if(s_3.front_face_0)
    {
        _S56 = s_3.geometric_normal_0;
    }
    else
    {
        _S56 = (vec3<f32>(0) - s_3.geometric_normal_0);
    }
    o_1.face_normal_0 = _S56;
    var n_3 : vec3<f32> = mapped_normal_0(s_3, normal_3, m_5.normal_scale_0);
    if((dot(n_3, (vec3<f32>(0) - ray_4.direction_0))) < 0.00009999999747379f)
    {
        _S56 = o_1.face_normal_0;
    }
    else
    {
        _S56 = n_3;
    }
    o_1.normal_2 = _S56;
    o_1.albedo_0 = m_5.base_color_0.xyz * base_3.xyz * s_3.color_1.xyz;
    o_1.emissive_2 = m_5.emissive_0 * emission_0.xyz;
    o_1.metallic_1 = clamp(m_5.metallic_0 * mr_0.z, 0.0f, 1.0f);
    o_1.roughness_1 = clamp(m_5.roughness_0 * mr_0.y, 0.04500000178813934f, 1.0f);
    o_1.unlit_0 = (((m_5.flags_2) & (u32(8)))) != u32(0);
    return o_1;
}

fn trace_any_0( ray_5 : TraceRay_0) -> bool
{
    return (trace_scene_0(ray_5, true).instance_0) != u32(4294967295);
}

fn sun_visible_0( s_4 : TracedSurface_0) -> bool
{
    var ray_6 : TraceRay_0;
    ray_6.origin_0 = s_4.position_1 + s_4.face_normal_0 * vec3<f32>(0.00009999999747379f);
    ray_6.tmin_0 = 0.00100000004749745f;
    ray_6.direction_0 = traceFrame_0.light_dir_0.xyz;
    ray_6.tmax_0 = 10000.0f;
    return !trace_any_0(ray_6);
}

fn distribution_ggx_0( n_dot_h_0 : f32,  roughness_2 : f32) -> f32
{
    var _S57 : f32 = max(roughness_2, 0.04500000178813934f);
    var a_5 : f32 = _S57 * _S57;
    var a2_0 : f32 = a_5 * a_5;
    var denom_0 : f32 = n_dot_h_0 * n_dot_h_0 * (a2_0 - 1.0f) + 1.0f;
    return a2_0 / (3.14159274101257324f * max(denom_0 * denom_0, 9.99999968265522539e-21f));
}

fn geometry_smith_0( n_dot_v_0 : f32,  n_dot_l_0 : f32,  roughness_3 : f32) -> f32
{
    var r_0 : f32 = roughness_3 + 1.0f;
    var k_0 : f32 = r_0 * r_0 / 8.0f;
    var _S58 : f32 = 1.0f - k_0;
    return n_dot_v_0 / max(n_dot_v_0 * _S58 + k_0, 9.99999997475242708e-07f) * (n_dot_l_0 / max(n_dot_l_0 * _S58 + k_0, 9.99999997475242708e-07f));
}

fn fresnel_schlick_0( cos_theta_0 : f32,  f0_0 : vec3<f32>) -> vec3<f32>
{
    return f0_0 + (vec3<f32>(1.0f) - f0_0) * vec3<f32>(pow(clamp(1.0f - cos_theta_0, 0.0f, 1.0f), 5.0f));
}

fn lambert_diffuse_0( albedo_1 : vec3<f32>) -> vec3<f32>
{
    return albedo_1 / vec3<f32>(3.14159274101257324f);
}

fn brdf_direct_0( n_4 : vec3<f32>,  v_5 : vec3<f32>,  l_0 : vec3<f32>,  albedo_2 : vec3<f32>,  metallic_2 : f32,  roughness_4 : f32,  f0_1 : vec3<f32>,  radiance_0 : vec3<f32>) -> vec3<f32>
{
    var h_1 : vec3<f32> = normalize(l_0 + v_5);
    var _S59 : f32 = max(dot(n_4, l_0), 0.0f);
    var _S60 : f32 = max(dot(n_4, v_5), 0.00009999999747379f);
    var f_0 : vec3<f32> = fresnel_schlick_0(max(dot(h_1, v_5), 0.0f), f0_1);
    return ((vec3<f32>(1.0f) - f_0) * vec3<f32>((1.0f - metallic_2)) * lambert_diffuse_0(albedo_2) + vec3<f32>((distribution_ggx_0(max(dot(n_4, h_1), 0.0f), roughness_4) * geometry_smith_0(_S60, _S59, roughness_4))) * f_0 / vec3<f32>(max(4.0f * _S60 * _S59, 9.99999997475242708e-07f))) * radiance_0 * vec3<f32>(_S59);
}

fn sun_light_0( s_5 : TracedSurface_0,  v_6 : vec3<f32>) -> vec3<f32>
{
    var l_1 : vec3<f32> = traceFrame_0.light_dir_0.xyz;
    var _S61 : bool;
    if((dot(s_5.normal_2, l_1)) <= 0.0f)
    {
        _S61 = true;
    }
    else
    {
        _S61 = !sun_visible_0(s_5);
    }
    if(_S61)
    {
        return vec3<f32>(0.0f);
    }
    return brdf_direct_0(s_5.normal_2, v_6, l_1, s_5.albedo_0, s_5.metallic_1, s_5.roughness_1, mix(vec3<f32>(0.03999999910593033f), s_5.albedo_0, vec3<f32>(s_5.metallic_1)), traceFrame_0.light_radiance_0.xyz);
}

fn ray_trace_pixel_0( pixel_3 : vec2<u32>) -> vec3<f32>
{
    var ray_7 : TraceRay_0 = camera_ray_0(vec2<f32>(pixel_3) + vec2<f32>(0.5f));
    var hit_4 : TraceHit_0 = trace_closest_0(ray_7);
    if((hit_4.instance_0) == u32(4294967295))
    {
        return sky_with_sun_0(ray_7.direction_0);
    }
    var s_6 : TracedSurface_0 = shade_hit_0(ray_7, hit_4);
    if(s_6.unlit_0)
    {
        return s_6.albedo_0;
    }
    return sun_light_0(s_6, (vec3<f32>(0) - ray_7.direction_0)) + s_6.emissive_2;
}

fn hdr_safe_0( radiance_1 : vec3<f32>) -> vec3<f32>
{
    var _S62 : vec3<f32> = vec3<f32>(0.0f);
    return clamp(select(radiance_1, _S62, (((bitcast<vec3<u32>>((radiance_1))) & (vec3<u32>(u32(2147483647))))) > vec3<u32>(u32(2139095040))), _S62, vec3<f32>(65000.0f));
}

@compute
@workgroup_size(8, 8, 1)
fn cs_ray_trace(@builtin(global_invocation_id) id_0 : vec3<u32>)
{
    var pixel_4 : vec2<u32>;
    var _S63 : bool = tile_pixel_0(id_0.xy, &(pixel_4));
    if(!_S63)
    {
        return;
    }
    traceAccumulation_0[pixel_index_0(pixel_4)] = vec4<f32>(hdr_safe_0(ray_trace_pixel_0(pixel_4)), 1.0f);
    return;
}

fn rng_next_0( state_0 : ptr<function, u32>) -> f32
{
    var _S64 : u32 = (*state_0) * u32(747796405) + u32(1);
    (*state_0) = _S64;
    var word_1 : u32 = ((((_S64 >> ((((_S64 >> (u32(28)))) + u32(4))))) ^ (_S64))) * u32(277803737);
    return f32((((word_1 >> (u32(22)))) ^ (word_1))) / 4.294967296e+09f;
}

fn rng_next_1( state_1 : ptr<function, u32>) -> f32
{
    var _S65 : u32 = (*state_1) * u32(747796405) + u32(1);
    (*state_1) = _S65;
    var word_2 : u32 = ((((_S65 >> ((((_S65 >> (u32(28)))) + u32(4))))) ^ (_S65))) * u32(277803737);
    return f32((((word_2 >> (u32(22)))) ^ (word_2))) / 4.294967296e+09f;
}

fn survives_0( bounce_0 : u32,  throughput_0 : ptr<function, vec3<f32>>,  rng_0 : ptr<function, u32>) -> bool
{
    if(bounce_0 < u32(3))
    {
        return true;
    }
    var survival_0 : f32 = clamp(max((*throughput_0).x, max((*throughput_0).y, (*throughput_0).z)), 0.05000000074505806f, 0.94999998807907104f);
    var _S66 : f32 = rng_next_1(&((*rng_0)));
    if(_S66 > survival_0)
    {
        return false;
    }
    (*throughput_0) = (*throughput_0) / vec3<f32>(survival_0);
    return true;
}

fn furnace_on_0() -> bool
{
    return (traceFrame_0.furnace_0.w) > 0.5f;
}

fn miss_radiance_0( dir_3 : vec3<f32>,  bounce_1 : u32) -> vec3<f32>
{
    if(furnace_on_0())
    {
        return vec3<f32>(traceFrame_0.furnace_0.x);
    }
    var _S67 : vec3<f32>;
    if(bounce_1 == u32(0))
    {
        _S67 = sky_with_sun_0(dir_3);
    }
    else
    {
        _S67 = sky_gradient_0(dir_3) * vec3<f32>(traceFrame_0.light_radiance_0.w);
    }
    return _S67;
}

fn path_surface_0( ray_8 : TraceRay_0,  hit_5 : TraceHit_0) -> TracedSurface_0
{
    var s_7 : TracedSurface_0 = shade_hit_0(ray_8, hit_5);
    if(furnace_on_0())
    {
        s_7.albedo_0 = vec3<f32>(1.0f);
        s_7.emissive_2 = vec3<f32>(0.0f);
        s_7.unlit_0 = false;
        s_7.metallic_1 = traceFrame_0.furnace_0.y;
        s_7.roughness_1 = clamp(traceFrame_0.furnace_0.z, 0.04500000178813934f, 1.0f);
    }
    return s_7;
}

fn next_event_0( s_8 : TracedSurface_0,  v_7 : vec3<f32>) -> vec3<f32>
{
    if(furnace_on_0())
    {
        return vec3<f32>(0.0f);
    }
    return sun_light_0(s_8, v_7);
}

fn specular_lobe_probability_0( albedo_3 : vec3<f32>,  metallic_3 : f32,  f0_2 : vec3<f32>,  n_dot_v_1 : f32) -> f32
{
    const LUMA_0 : vec3<f32> = vec3<f32>(0.2125999927520752f, 0.71520000696182251f, 0.07220000028610229f);
    var fresnel_0 : vec3<f32> = fresnel_schlick_0(n_dot_v_1, f0_2);
    var specular_0 : f32 = dot(fresnel_0, LUMA_0);
    return clamp(specular_0 / max(specular_0 + dot((vec3<f32>(1.0f) - fresnel_0) * vec3<f32>((1.0f - metallic_3)) * albedo_3, LUMA_0), 9.99999997475242708e-07f), 0.10000000149011612f, 1.0f);
}

fn orthonormal_basis_0( n_5 : vec3<f32>,  t_4 : ptr<function, vec3<f32>>,  b_6 : ptr<function, vec3<f32>>)
{
    var _S68 : f32 = n_5.z;
    var s_9 : f32;
    if(_S68 >= 0.0f)
    {
        s_9 = 1.0f;
    }
    else
    {
        s_9 = -1.0f;
    }
    var a_6 : f32 = -1.0f / (s_9 + _S68);
    var _S69 : f32 = n_5.x;
    var _S70 : f32 = n_5.y;
    var c_4 : f32 = _S69 * _S70 * a_6;
    (*t_4) = vec3<f32>(1.0f + s_9 * _S69 * _S69 * a_6, s_9 * c_4, - s_9 * _S69);
    (*b_6) = vec3<f32>(c_4, s_9 + _S70 * _S70 * a_6, - _S70);
    return;
}

fn sample_ggx_visible_normal_0( v_8 : vec3<f32>,  alpha_0 : f32,  u_1 : vec2<f32>) -> vec3<f32>
{
    var vh_0 : vec3<f32> = normalize(vec3<f32>(alpha_0 * v_8.x, alpha_0 * v_8.y, v_8.z));
    var _S71 : f32 = vh_0.x;
    var _S72 : f32 = vh_0.y;
    var length_squared_0 : f32 = _S71 * _S71 + _S72 * _S72;
    var t1_1 : vec3<f32>;
    if(length_squared_0 > 0.0f)
    {
        t1_1 = vec3<f32>(- _S72, _S71, 0.0f) / vec3<f32>(sqrt(length_squared_0));
    }
    else
    {
        t1_1 = vec3<f32>(1.0f, 0.0f, 0.0f);
    }
    var r_1 : f32 = sqrt(u_1.x);
    var phi_0 : f32 = 6.28318548202514648f * u_1.y;
    var p1_2 : f32 = r_1 * cos(phi_0);
    var s_10 : f32 = 0.5f * (1.0f + vh_0.z);
    var _S73 : f32 = 1.0f - p1_2 * p1_2;
    var p2_2 : f32 = (1.0f - s_10) * sqrt(max(_S73, 0.0f)) + s_10 * (r_1 * sin(phi_0));
    var nh_0 : vec3<f32> = vec3<f32>(p1_2) * t1_1 + vec3<f32>(p2_2) * cross(vh_0, t1_1) + vec3<f32>(sqrt(max(_S73 - p2_2 * p2_2, 0.0f))) * vh_0;
    return normalize(vec3<f32>(alpha_0 * nh_0.x, alpha_0 * nh_0.y, max(nh_0.z, 9.99999997475242708e-07f)));
}

fn ggx_visible_normal_pdf_0( n_dot_v_2 : f32,  n_dot_h_1 : f32,  alpha_1 : f32) -> f32
{
    var a2_1 : f32 = alpha_1 * alpha_1;
    var d_2 : f32 = n_dot_h_1 * n_dot_h_1 * (a2_1 - 1.0f) + 1.0f;
    return a2_1 / (3.14159274101257324f * d_2 * d_2) * (2.0f * n_dot_v_2 / (n_dot_v_2 + sqrt(a2_1 + (1.0f - a2_1) * n_dot_v_2 * n_dot_v_2))) / (4.0f * n_dot_v_2);
}

struct Bounce_0
{
     direction_6 : vec3<f32>,
     weight_0 : vec3<f32>,
};

fn sample_bounce_0( n_6 : vec3<f32>,  face_normal_1 : vec3<f32>,  v_9 : vec3<f32>,  albedo_4 : vec3<f32>,  metallic_4 : f32,  roughness_5 : f32,  rng_1 : ptr<function, u32>) -> Bounce_0
{
    var f0_3 : vec3<f32> = mix(vec3<f32>(0.03999999910593033f), albedo_4, vec3<f32>(metallic_4));
    var _S74 : f32 = max(dot(n_6, v_9), 0.00009999999747379f);
    var specular_1 : f32 = specular_lobe_probability_0(albedo_4, metallic_4, f0_3, _S74);
    var alpha_2 : f32 = roughness_5 * roughness_5;
    var t_5 : vec3<f32>;
    var b_7 : vec3<f32>;
    orthonormal_basis_0(n_6, &(t_5), &(b_7));
    var _S75 : f32 = rng_next_0(&((*rng_1)));
    var direction_7 : vec3<f32>;
    if(_S75 < specular_1)
    {
        var local_0 : vec3<f32> = vec3<f32>(dot(v_9, t_5), dot(v_9, b_7), dot(v_9, n_6));
        var _S76 : f32 = rng_next_0(&((*rng_1)));
        var _S77 : f32 = rng_next_0(&((*rng_1)));
        var h_2 : vec3<f32> = sample_ggx_visible_normal_0(local_0, alpha_2, vec2<f32>(_S76, _S77));
        direction_7 = reflect((vec3<f32>(0) - v_9), vec3<f32>(h_2.x) * t_5 + vec3<f32>(h_2.y) * b_7 + vec3<f32>(h_2.z) * n_6);
    }
    else
    {
        var _S78 : f32 = rng_next_0(&((*rng_1)));
        var theta_0 : f32 = 6.28318548202514648f * _S78;
        var _S79 : f32 = rng_next_0(&((*rng_1)));
        var z_0 : f32 = 2.0f * _S79 - 1.0f;
        var r_2 : f32 = sqrt(max(1.0f - z_0 * z_0, 0.0f));
        var direction_8 : vec3<f32> = n_6 + vec3<f32>(r_2 * cos(theta_0), r_2 * sin(theta_0), z_0);
        if((dot(direction_8, direction_8)) < 9.99999993922529029e-09f)
        {
            direction_7 = n_6;
        }
        else
        {
            direction_7 = direction_8;
        }
    }
    var o_2 : Bounce_0;
    var _S80 : vec3<f32> = normalize(direction_7);
    o_2.direction_6 = _S80;
    o_2.weight_0 = vec3<f32>(0.0f);
    var n_dot_l_1 : f32 = dot(n_6, _S80);
    var _S81 : bool;
    if(n_dot_l_1 <= 0.0f)
    {
        _S81 = true;
    }
    else
    {
        _S81 = (dot(face_normal_1, o_2.direction_6)) <= 0.0f;
    }
    if(_S81)
    {
        return o_2;
    }
    o_2.weight_0 = brdf_direct_0(n_6, v_9, o_2.direction_6, albedo_4, metallic_4, roughness_5, f0_3, vec3<f32>(1.0f)) / vec3<f32>(max(specular_1 * ggx_visible_normal_pdf_0(_S74, max(dot(n_6, normalize(v_9 + o_2.direction_6)), 0.0f), alpha_2) + (1.0f - specular_1) * n_dot_l_1 / 3.14159274101257324f, 9.99999997475242708e-07f));
    return o_2;
}

fn trace_path_0( pixel_5 : vec2<u32>,  rng_2 : ptr<function, u32>) -> vec3<f32>
{
    var _S82 : f32 = rng_next_0(&((*rng_2)));
    var _S83 : f32 = rng_next_0(&((*rng_2)));
    var ray_9 : TraceRay_0 = camera_ray_0(vec2<f32>(pixel_5) + vec2<f32>(_S82, _S83));
    var throughput_1 : vec3<f32> = vec3<f32>(1.0f);
    var _S84 : vec3<f32> = vec3<f32>(0.0f);
    var bounce_2 : u32 = u32(0);
    var radiance_2 : vec3<f32> = _S84;
    for(;;)
    {
        if(bounce_2 < (traceFrame_0.size_0.z))
        {
        }
        else
        {
            break;
        }
        var _S85 : bool = survives_0(bounce_2, &(throughput_1), &((*rng_2)));
        if(!_S85)
        {
            break;
        }
        var hit_6 : TraceHit_0 = trace_closest_0(ray_9);
        if((hit_6.instance_0) == u32(4294967295))
        {
            radiance_2 = radiance_2 + throughput_1 * miss_radiance_0(ray_9.direction_0, bounce_2);
            break;
        }
        var s_11 : TracedSurface_0 = path_surface_0(ray_9, hit_6);
        if(s_11.unlit_0)
        {
            radiance_2 = radiance_2 + throughput_1 * s_11.albedo_0;
            break;
        }
        var v_10 : vec3<f32> = (vec3<f32>(0) - ray_9.direction_0);
        var radiance_3 : vec3<f32> = radiance_2 + throughput_1 * (s_11.emissive_2 + next_event_0(s_11, v_10));
        var next_0 : Bounce_0 = sample_bounce_0(s_11.normal_2, s_11.face_normal_0, v_10, s_11.albedo_0, s_11.metallic_1, s_11.roughness_1, &((*rng_2)));
        if((all(((next_0.weight_0) == _S84))))
        {
            radiance_2 = radiance_3;
            break;
        }
        throughput_1 = throughput_1 * next_0.weight_0;
        ray_9.origin_0 = s_11.position_1 + s_11.face_normal_0 * vec3<f32>(0.00009999999747379f);
        ray_9.tmin_0 = 0.00100000004749745f;
        ray_9.direction_0 = next_0.direction_6;
        ray_9.tmax_0 = 10000.0f;
        bounce_2 = bounce_2 + u32(1);
        radiance_2 = radiance_3;
    }
    return radiance_2;
}

@compute
@workgroup_size(8, 8, 1)
fn cs_path_trace(@builtin(global_invocation_id) id_1 : vec3<u32>)
{
    var pixel_6 : vec2<u32>;
    var _S86 : bool = tile_pixel_0(id_1.xy, &(pixel_6));
    if(!_S86)
    {
        return;
    }
    var index_4 : u32 = pixel_index_0(pixel_6);
    var rng_3 : u32 = index_4 + traceTile_0.samples_0.z * u32(2654435769);
    var _S87 : vec3<f32> = vec3<f32>(0.0f);
    var i_0 : u32 = u32(0);
    var sum_0 : vec3<f32> = _S87;
    for(;;)
    {
        if(i_0 < (traceTile_0.samples_0.y))
        {
        }
        else
        {
            break;
        }
        var _S88 : vec3<f32> = trace_path_0(pixel_6, &(rng_3));
        var sum_1 : vec3<f32> = sum_0 + hdr_safe_0(_S88);
        i_0 = i_0 + u32(1);
        sum_0 = sum_1;
    }
    var before_0 : f32 = f32(traceTile_0.samples_0.z);
    var previous_0 : vec3<f32>;
    if((traceTile_0.samples_0.z) > u32(0))
    {
        previous_0 = traceAccumulation_0[index_4].xyz;
    }
    else
    {
        previous_0 = _S87;
    }
    traceAccumulation_0[index_4] = vec4<f32>((previous_0 * vec3<f32>(before_0) + sum_0) / vec3<f32>((before_0 + f32(traceTile_0.samples_0.y))), 1.0f);
    return;
}

