@binding(0) @group(0) var<storage, read> vectorInputs_0 : array<u32>;

@binding(1) @group(0) var<storage, read_write> vectorOutputs_0 : array<u32>;

struct VectorInputs_0
{
     at_0 : u32,
};

fn vector_inputs_0( case_index_0 : u32) -> VectorInputs_0
{
    var inputs_0 : VectorInputs_0;
    inputs_0.at_0 = u32(2) + case_index_0 * vectorInputs_0[i32(0)];
    return inputs_0;
}

fn VectorInputs_u32_0( this_0 : ptr<function, VectorInputs_0>) -> u32
{
    var word_0 : u32 = vectorInputs_0[(*this_0).at_0];
    (*this_0).at_0 = (*this_0).at_0 + u32(1);
    return word_0;
}

fn VectorInputs_f32_0( this_1 : ptr<function, VectorInputs_0>) -> f32
{
    var _S1 : u32 = VectorInputs_u32_0(&((*this_1)));
    return (bitcast<f32>((_S1)));
}

fn VectorInputs_f32_1( this_2 : ptr<function, VectorInputs_0>) -> f32
{
    var _S2 : u32 = VectorInputs_u32_0(&((*this_2)));
    return (bitcast<f32>((_S2)));
}

fn VectorInputs_f32x3_0( this_3 : ptr<function, VectorInputs_0>) -> vec3<f32>
{
    var x_0 : f32 = VectorInputs_f32_1(&((*this_3)));
    var y_0 : f32 = VectorInputs_f32_1(&((*this_3)));
    var z_0 : f32 = VectorInputs_f32_1(&((*this_3)));
    return vec3<f32>(x_0, y_0, z_0);
}

fn VectorInputs_f32x3_1( this_4 : ptr<function, VectorInputs_0>) -> vec3<f32>
{
    var x_1 : f32 = VectorInputs_f32_1(&((*this_4)));
    var y_1 : f32 = VectorInputs_f32_1(&((*this_4)));
    var z_1 : f32 = VectorInputs_f32_1(&((*this_4)));
    return vec3<f32>(x_1, y_1, z_1);
}

fn VectorInputs_f32x4_0( this_5 : ptr<function, VectorInputs_0>) -> vec4<f32>
{
    var xyz_0 : vec3<f32> = VectorInputs_f32x3_1(&((*this_5)));
    var w_0 : f32 = VectorInputs_f32_1(&((*this_5)));
    return vec4<f32>(xyz_0, w_0);
}

fn VectorInputs_i32_0( this_6 : ptr<function, VectorInputs_0>) -> i32
{
    var _S3 : u32 = VectorInputs_u32_0(&((*this_6)));
    return (bitcast<i32>((_S3)));
}

struct VectorOutputs_0
{
     at_1 : u32,
};

fn vector_outputs_0( case_index_1 : u32) -> VectorOutputs_0
{
    var outputs_0 : VectorOutputs_0;
    outputs_0.at_1 = case_index_1 * vectorInputs_0[i32(1)];
    return outputs_0;
}

fn mod289_0( x_2 : vec3<f32>) -> vec3<f32>
{
    return x_2 - floor(x_2 * vec3<f32>(0.00346020772121847f)) * vec3<f32>(289.0f);
}

fn VectorOutputs_put_0( this_7 : ptr<function, VectorOutputs_0>,  word_1 : u32)
{
    vectorOutputs_0[(*this_7).at_1] = word_1;
    (*this_7).at_1 = (*this_7).at_1 + u32(1);
    return;
}

fn VectorOutputs_put_1( this_8 : ptr<function, VectorOutputs_0>,  value_0 : f32)
{
    VectorOutputs_put_0(&((*this_8)), (bitcast<u32>((value_0))));
    return;
}

fn VectorOutputs_put_2( this_9 : ptr<function, VectorOutputs_0>,  value_1 : f32)
{
    VectorOutputs_put_0(&((*this_9)), (bitcast<u32>((value_1))));
    return;
}

fn VectorOutputs_put_3( this_10 : ptr<function, VectorOutputs_0>,  value_2 : vec3<f32>)
{
    VectorOutputs_put_2(&((*this_10)), value_2.x);
    VectorOutputs_put_2(&((*this_10)), value_2.y);
    VectorOutputs_put_2(&((*this_10)), value_2.z);
    return;
}

fn VectorOutputs_put_4( this_11 : ptr<function, VectorOutputs_0>,  value_3 : vec3<f32>)
{
    VectorOutputs_put_2(&((*this_11)), value_3.x);
    VectorOutputs_put_2(&((*this_11)), value_3.y);
    VectorOutputs_put_2(&((*this_11)), value_3.z);
    return;
}

fn mod289_1( x_3 : vec4<f32>) -> vec4<f32>
{
    return x_3 - floor(x_3 * vec4<f32>(0.00346020772121847f)) * vec4<f32>(289.0f);
}

fn VectorOutputs_put_5( this_12 : ptr<function, VectorOutputs_0>,  value_4 : vec4<f32>)
{
    VectorOutputs_put_4(&((*this_12)), value_4.xyz);
    VectorOutputs_put_2(&((*this_12)), value_4.w);
    return;
}

fn permute_0( x_4 : vec4<f32>) -> vec4<f32>
{
    return mod289_1((x_4 * vec4<f32>(34.0f) + vec4<f32>(10.0f)) * x_4);
}

fn taylorInvSqrt_0( r_0 : vec4<f32>) -> vec4<f32>
{
    return vec4<f32>(1.79284286499023438f) - vec4<f32>(0.85373473167419434f) * r_0;
}

fn snoise_0( v_0 : vec3<f32>) -> f32
{
    const _S4 : vec3<f32> = vec3<f32>(0.3333333432674408f, 0.3333333432674408f, 0.3333333432674408f);
    var i_0 : vec3<f32> = floor(v_0 + vec3<f32>(dot(v_0, _S4)));
    const _S5 : vec3<f32> = vec3<f32>(0.1666666716337204f, 0.1666666716337204f, 0.1666666716337204f);
    var x0_0 : vec3<f32> = v_0 - i_0 + vec3<f32>(dot(i_0, _S5));
    var g_0 : vec3<f32> = step(x0_0.yzx, x0_0.xyz);
    var _S6 : vec3<f32> = g_0.xyz;
    var _S7 : vec3<f32> = (vec3<f32>(1.0f) - g_0).zxy;
    var i1_0 : vec3<f32> = min(_S6, _S7);
    var i2_0 : vec3<f32> = max(_S6, _S7);
    var x1_0 : vec3<f32> = x0_0 - i1_0 + _S5;
    var x2_0 : vec3<f32> = x0_0 - i2_0 + _S4;
    var x3_0 : vec3<f32> = x0_0 - vec3<f32>(0.5f, 0.5f, 0.5f);
    var i_1 : vec3<f32> = mod289_0(i_0);
    var p_0 : vec4<f32> = permute_0(permute_0(permute_0(vec4<f32>(i_1.z) + vec4<f32>(0.0f, i1_0.z, i2_0.z, 1.0f)) + vec4<f32>(i_1.y) + vec4<f32>(0.0f, i1_0.y, i2_0.y, 1.0f)) + vec4<f32>(i_1.x) + vec4<f32>(0.0f, i1_0.x, i2_0.x, 1.0f));
    var ns_0 : vec3<f32> = vec3<f32>(0.1428571492433548f) * vec3<f32>(2.0f, 0.5f, 1.0f) - vec3<f32>(0.0f, 1.0f, 0.0f);
    var _S8 : vec4<f32> = vec4<f32>(ns_0.z);
    var j_0 : vec4<f32> = p_0 - vec4<f32>(49.0f) * floor(p_0 * _S8 * _S8);
    var x_5 : vec4<f32> = floor(j_0 * _S8);
    var _S9 : vec4<f32> = vec4<f32>(ns_0.x);
    var _S10 : vec4<f32> = ns_0.yyyy;
    var x_6 : vec4<f32> = x_5 * _S9 + _S10;
    var y_2 : vec4<f32> = floor(j_0 - vec4<f32>(7.0f) * x_5) * _S9 + _S10;
    var _S11 : vec4<f32> = vec4<f32>(1.0f);
    var h_0 : vec4<f32> = _S11 - abs(x_6) - abs(y_2);
    var b0_0 : vec4<f32> = vec4<f32>(x_6.xy, y_2.xy);
    var b1_0 : vec4<f32> = vec4<f32>(x_6.zw, y_2.zw);
    var _S12 : vec4<f32> = vec4<f32>(2.0f);
    var _S13 : vec4<f32> = vec4<f32>(0.0f);
    var sh_0 : vec4<f32> = (vec4<f32>(0) - step(h_0, _S13));
    var a0_0 : vec4<f32> = b0_0.xzyw + (floor(b0_0) * _S12 + _S11).xzyw * sh_0.xxyy;
    var a1_0 : vec4<f32> = b1_0.xzyw + (floor(b1_0) * _S12 + _S11).xzyw * sh_0.zzww;
    var p0_0 : vec3<f32> = vec3<f32>(a0_0.xy, h_0.x);
    var p1_0 : vec3<f32> = vec3<f32>(a0_0.zw, h_0.y);
    var p2_0 : vec3<f32> = vec3<f32>(a1_0.xy, h_0.z);
    var p3_0 : vec3<f32> = vec3<f32>(a1_0.zw, h_0.w);
    var norm_0 : vec4<f32> = taylorInvSqrt_0(vec4<f32>(dot(p0_0, p0_0), dot(p1_0, p1_0), dot(p2_0, p2_0), dot(p3_0, p3_0)));
    var m_0 : vec4<f32> = max(vec4<f32>(0.5f) - vec4<f32>(dot(x0_0, x0_0), dot(x1_0, x1_0), dot(x2_0, x2_0), dot(x3_0, x3_0)), _S13);
    var m_1 : vec4<f32> = m_0 * m_0;
    return 105.0f * dot(m_1 * m_1, vec4<f32>(dot(p0_0 * vec3<f32>(norm_0.x), x0_0), dot(p1_0 * vec3<f32>(norm_0.y), x1_0), dot(p2_0 * vec3<f32>(norm_0.z), x2_0), dot(p3_0 * vec3<f32>(norm_0.w), x3_0)));
}

fn fbm_0( p_1 : vec3<f32>,  freq_0 : f32,  octaves_0 : i32) -> f32
{
    var _S14 : f32 = exp2(-0.5f);
    var i_2 : i32 = i32(0);
    var amp_0 : f32 = 1.0f;
    var _S15 : f32 = freq_0;
    var noise_0 : f32 = 0.0f;
    for(;;)
    {
        if(i_2 < octaves_0)
        {
        }
        else
        {
            break;
        }
        var noise_1 : f32 = noise_0 + amp_0 * snoise_0(p_1 * vec3<f32>(_S15));
        var _S16 : f32 = _S15 * 2.0f;
        var amp_1 : f32 = amp_0 * _S14;
        i_2 = i_2 + i32(1);
        amp_0 = amp_1;
        _S15 = _S16;
        noise_0 = noise_1;
    }
    return noise_0;
}

@compute
@workgroup_size(1, 1, 1)
fn noise_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S17 : u32 = tid_0.x;
    var inputs_1 : VectorInputs_0 = vector_inputs_0(_S17);
    var p_2 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var x_7 : vec4<f32> = VectorInputs_f32x4_0(&(inputs_1));
    var freq_1 : f32 = VectorInputs_f32_0(&(inputs_1));
    var octaves_1 : i32 = VectorInputs_i32_0(&(inputs_1));
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S17);
    VectorOutputs_put_3(&(outputs_1), mod289_0(x_7.xyz));
    VectorOutputs_put_5(&(outputs_1), mod289_1(x_7));
    VectorOutputs_put_5(&(outputs_1), permute_0(x_7));
    VectorOutputs_put_5(&(outputs_1), taylorInvSqrt_0(x_7));
    VectorOutputs_put_1(&(outputs_1), snoise_0(p_2));
    VectorOutputs_put_1(&(outputs_1), fbm_0(p_2, freq_1, octaves_1));
    return;
}

