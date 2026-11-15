@binding(0) @group(0) var noiseVolume_0 : texture_storage_3d<rgba16float, write>;

fn hash33_0( p3_0 : vec3<f32>) -> vec3<f32>
{
    var _S1 : vec3<f32> = fract(p3_0 * vec3<f32>(0.1031000018119812f, 0.10300000011920929f, 0.09730000048875809f));
    var _S2 : vec3<f32> = _S1 + vec3<f32>(dot(_S1, _S1.yxz + vec3<f32>(33.3300018310546875f)));
    return fract((_S2.xxy + _S2.yxx) * _S2.zyx);
}

fn worley_0( p_0 : vec3<f32>,  numCells_0 : f32) -> f32
{
    var _S3 : vec3<f32>;
    var _S4 : vec3<f32> = p_0 * vec3<f32>(numCells_0);
    var _S5 : vec3<f32> = floor(_S4);
    var _S6 : vec3<f32> = fract(_S4);
    var minDist_0 : f32 = 1.0f;
    var z_0 : i32 = i32(-1);
    for(;;)
    {
        if(z_0 <= i32(1))
        {
        }
        else
        {
            break;
        }
        var y_0 : i32 = i32(-1);
        for(;;)
        {
            if(y_0 <= i32(1))
            {
            }
            else
            {
                break;
            }
            var minDist_1 : f32 = minDist_0;
            var x_0 : i32 = i32(-1);
            for(;;)
            {
                if(x_0 <= i32(1))
                {
                }
                else
                {
                    break;
                }
                var neighbor_0 : vec3<f32> = vec3<f32>(f32(x_0), f32(y_0), f32(z_0));
                var _S7 : vec3<f32> = _S5 + neighbor_0;
                for(;;)
                {
                    var result_0 : vec3<f32>;
                    var i_0 : i32 = i32(0);
                    for(;;)
                    {
                        if(i_0 < i32(3))
                        {
                        }
                        else
                        {
                            break;
                        }
                        result_0[i_0] = ((((_S7[i_0])) % ((numCells_0))));
                        i_0 = i_0 + i32(1);
                    }
                    _S3 = result_0;
                    break;
                }
                var _S8 : f32 = min(minDist_1, length(neighbor_0 + hash33_0(_S3) - _S6));
                var _S9 : i32 = x_0 + i32(1);
                minDist_1 = _S8;
                x_0 = _S9;
            }
            var _S10 : i32 = y_0 + i32(1);
            minDist_0 = minDist_1;
            y_0 = _S10;
        }
        z_0 = z_0 + i32(1);
    }
    return minDist_0;
}

fn hash13_0( p3_1 : vec3<f32>) -> f32
{
    var _S11 : vec3<f32> = fract(p3_1 * vec3<f32>(0.1031000018119812f));
    var _S12 : vec3<f32> = _S11 + vec3<f32>(dot(_S11, _S11.zyx + vec3<f32>(31.31999969482421875f)));
    return fract((_S12.x + _S12.y) * _S12.z);
}

fn valueNoise_0( x_1 : vec3<f32>) -> f32
{
    var i_1 : vec3<f32> = floor(x_1);
    var f_0 : vec3<f32> = fract(x_1);
    var f_1 : vec3<f32> = f_0 * f_0 * (vec3<f32>(3.0f) - vec3<f32>(2.0f) * f_0);
    var _S13 : f32 = f_1.x;
    var _S14 : f32 = f_1.y;
    return mix(mix(mix(hash13_0(i_1), hash13_0(i_1 + vec3<f32>(1.0f, 0.0f, 0.0f)), _S13), mix(hash13_0(i_1 + vec3<f32>(0.0f, 1.0f, 0.0f)), hash13_0(i_1 + vec3<f32>(1.0f, 1.0f, 0.0f)), _S13), _S14), mix(mix(hash13_0(i_1 + vec3<f32>(0.0f, 0.0f, 1.0f)), hash13_0(i_1 + vec3<f32>(1.0f, 0.0f, 1.0f)), _S13), mix(hash13_0(i_1 + vec3<f32>(0.0f, 1.0f, 1.0f)), hash13_0(i_1 + vec3<f32>(1.0f, 1.0f, 1.0f)), _S13), _S14), f_1.z);
}

fn fbm_value_0( x_2 : vec3<f32>,  octaves_0 : i32) -> f32
{
    var _S15 : vec3<f32> = x_2;
    var i_2 : i32 = i32(0);
    var a_0 : f32 = 0.5f;
    var v_0 : f32 = 0.0f;
    for(;;)
    {
        if(i_2 < octaves_0)
        {
        }
        else
        {
            break;
        }
        var v_1 : f32 = v_0 + a_0 * valueNoise_0(_S15);
        var _S16 : vec3<f32> = _S15 * vec3<f32>(2.0f) + vec3<f32>(100.0f);
        var a_1 : f32 = a_0 * 0.5f;
        var i_3 : i32 = i_2 + i32(1);
        _S15 = _S16;
        i_2 = i_3;
        a_0 = a_1;
        v_0 = v_1;
    }
    return v_0;
}

fn noise_volume_texel_0( texel_0 : vec3<u32>) -> vec4<f32>
{
    var uvw_0 : vec3<f32> = vec3<f32>(texel_0) / vec3<f32>(128.0f);
    return vec4<f32>(worley_0(uvw_0, 4.0f), fbm_value_0(uvw_0 * vec3<f32>(3.0f), i32(4)), worley_0(uvw_0, 8.0f), fbm_value_0(uvw_0 * vec3<f32>(6.0f), i32(5)));
}

@compute
@workgroup_size(8, 8, 4)
fn cs_noise(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    textureStore((noiseVolume_0), (tid_0), (noise_volume_texel_0(tid_0)));
    return;
}

