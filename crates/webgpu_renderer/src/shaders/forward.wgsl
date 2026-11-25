struct _MatrixStorage_float4x4_ColMajorstd140_0
{
    @align(16) data_0 : array<vec4<f32>, i32(4)>,
};

struct PrimUniforms_std140_0
{
    @align(16) model_0 : _MatrixStorage_float4x4_ColMajorstd140_0,
    @align(16) normal_matrix_0 : _MatrixStorage_float4x4_ColMajorstd140_0,
    @align(16) base_color_0 : vec4<f32>,
    @align(16) material_factors_0 : vec4<f32>,
    @align(16) emissive_factor_0 : vec4<f32>,
    @align(16) base_uv_row0_0 : vec4<f32>,
    @align(16) base_uv_row1_0 : vec4<f32>,
    @align(16) mr_uv_row0_0 : vec4<f32>,
    @align(16) mr_uv_row1_0 : vec4<f32>,
    @align(16) normal_uv_row0_0 : vec4<f32>,
    @align(16) normal_uv_row1_0 : vec4<f32>,
    @align(16) emissive_uv_row0_0 : vec4<f32>,
    @align(16) emissive_uv_row1_0 : vec4<f32>,
    @align(16) occlusion_uv_row0_0 : vec4<f32>,
    @align(16) occlusion_uv_row1_0 : vec4<f32>,
    @align(16) material_flags_0 : vec4<f32>,
};

@binding(0) @group(0) var<uniform> prim_0 : PrimUniforms_std140_0;
struct _MatrixStorage_float4x4_ColMajorstd430_0
{
    @align(16) data_1 : array<vec4<f32>, i32(4)>,
};

@binding(13) @group(0) var<storage, read> jointMatrices_0 : array<_MatrixStorage_float4x4_ColMajorstd430_0>;

struct FrameUniforms_std140_0
{
    @align(16) view_proj_0 : _MatrixStorage_float4x4_ColMajorstd140_0,
    @align(16) light_space_0 : _MatrixStorage_float4x4_ColMajorstd140_0,
    @align(16) light_space_1_0 : _MatrixStorage_float4x4_ColMajorstd140_0,
    @align(16) light_space_2_0 : _MatrixStorage_float4x4_ColMajorstd140_0,
    @align(16) light_dir_ambient_0 : vec4<f32>,
    @align(16) light_color_intensity_0 : vec4<f32>,
    @align(16) camera_position_0 : vec4<f32>,
    @align(16) cascade_splits_0 : vec4<f32>,
    @align(16) shadow_params_0 : vec4<f32>,
    @align(16) shadow_view_dir_0 : vec4<f32>,
    @align(16) shadow_bias_scale_0 : vec4<f32>,
};

@binding(0) @group(2) var<uniform> frame_0 : FrameUniforms_std140_0;
struct ShadowView_std140_0
{
    @align(16) index_0 : vec4<u32>,
    @align(16) face_0 : _MatrixStorage_float4x4_ColMajorstd140_0,
};

@binding(0) @group(1) var<uniform> shadowView_0 : ShadowView_std140_0;
@binding(3) @group(0) var baseColorTex_0 : texture_2d<f32>;

@binding(4) @group(0) var baseColorSampler_0 : sampler;

@binding(5) @group(0) var metalRoughTex_0 : texture_2d<f32>;

@binding(6) @group(0) var metalRoughSampler_0 : sampler;

@binding(7) @group(0) var normalTex_0 : texture_2d<f32>;

@binding(8) @group(0) var normalSampler_0 : sampler;

@binding(9) @group(0) var emissiveTex_0 : texture_2d<f32>;

@binding(10) @group(0) var emissiveSampler_0 : sampler;

@binding(11) @group(0) var occlusionTex_0 : texture_2d<f32>;

@binding(12) @group(0) var occlusionSampler_0 : sampler;

@binding(1) @group(0) var shadowMap_0 : texture_depth_2d_array;

@binding(1) @group(3) var<storage, read> tileLightGrid_0 : array<vec2<u32>>;

@binding(2) @group(3) var<storage, read> tileLightIndices_0 : array<u32>;

@binding(0) @group(3) var<storage, read> punctualLights_0 : array<vec4<f32>>;

@binding(3) @group(3) var punctualShadowMap_0 : texture_depth_2d_array;

struct IblParams_std140_0
{
    @align(16) enabled_maxmip_intensity_0 : vec4<f32>,
};

@binding(0) @group(1) var<uniform> iblParams_0 : IblParams_std140_0;
@binding(1) @group(1) var irradianceMap_0 : texture_cube<f32>;

@binding(4) @group(1) var iblSampler_0 : sampler;

@binding(2) @group(1) var prefilteredMap_0 : texture_cube<f32>;

@binding(3) @group(1) var brdfLut_0 : texture_2d<f32>;

@binding(17) @group(0) var gbufferDepth_0 : texture_depth_2d;

@binding(14) @group(0) var gbufferNormal_0 : texture_2d<f32>;

@binding(15) @group(0) var gbufferAlbedo_0 : texture_2d<f32>;

@binding(16) @group(0) var gbufferMaterial_0 : texture_2d<f32>;

struct DeferredUniforms_std140_0
{
    @align(16) inv_view_proj_0 : _MatrixStorage_float4x4_ColMajorstd140_0,
};

@binding(18) @group(0) var<uniform> deferredParams_0 : DeferredUniforms_std140_0;
@binding(2) @group(0) var shadowSampler_0 : sampler_comparison;

struct CascadeTaps_0
{
     layer_0 : f32,
};

fn CascadeTaps_lit_0( this_0 : CascadeTaps_0,  uv_0 : vec2<f32>,  reference_0 : f32) -> f32
{
    var _S1 : f32 = uv_0.x;
    var _S2 : bool;
    if(_S1 >= 0.0f)
    {
        _S2 = _S1 <= 1.0f;
    }
    else
    {
        _S2 = false;
    }
    if(_S2)
    {
        _S2 = (uv_0.y) >= 0.0f;
    }
    else
    {
        _S2 = false;
    }
    if(_S2)
    {
        _S2 = (uv_0.y) <= 1.0f;
    }
    else
    {
        _S2 = false;
    }
    var _S3 : f32;
    if(_S2)
    {
        var _S4 : vec3<f32> = vec3<f32>(uv_0, this_0.layer_0);
        _S3 = (textureSampleCompareLevel((shadowMap_0), (shadowSampler_0), ((_S4)).xy, i32(((_S4)).z), (reference_0)));
    }
    else
    {
        _S3 = 1.0f;
    }
    return _S3;
}

fn pcf_visibility_0( taps_0 : CascadeTaps_0,  uv_1 : vec2<f32>,  texel_0 : vec2<f32>,  reference_1 : f32,  uv_depth_slope_0 : vec2<f32>,  radius_0 : i32) -> f32
{
    var _S5 : i32 = - radius_0;
    var y_0 : i32 = _S5;
    var sum_0 : f32 = 0.0f;
    for(;;)
    {
        if(y_0 <= radius_0)
        {
        }
        else
        {
            break;
        }
        var x_0 : i32 = _S5;
        for(;;)
        {
            if(x_0 <= radius_0)
            {
            }
            else
            {
                break;
            }
            var offset_0 : vec2<f32> = vec2<f32>(f32(x_0), f32(y_0)) * texel_0;
            var sum_1 : f32 = sum_0 + CascadeTaps_lit_0(taps_0, uv_1 + offset_0, reference_1 + dot(uv_depth_slope_0, offset_0));
            x_0 = x_0 + i32(1);
            sum_0 = sum_1;
        }
        y_0 = y_0 + i32(1);
    }
    var _S6 : i32 = i32(2) * radius_0 + i32(1);
    return sum_0 / f32(_S6 * _S6);
}

struct FaceTaps_0
{
     slot_0 : i32,
};

struct PunctualShadow_0
{
     slot_1 : i32,
     nearPlane_0 : f32,
     farPlane_0 : f32,
};

fn punctual_shadow_depth_0( majorAxis_0 : f32,  nearPlane_1 : f32,  farPlane_1 : f32) -> f32
{
    return farPlane_1 * (majorAxis_0 - nearPlane_1) / ((farPlane_1 - nearPlane_1) * majorAxis_0);
}

struct PunctualShadowFace_0
{
     face_1 : u32,
     ndc_0 : vec2<f32>,
};

fn punctual_shadow_face_0( r_0 : vec3<f32>) -> PunctualShadowFace_0
{
    var a_0 : vec3<f32> = abs(r_0);
    var o_0 : PunctualShadowFace_0;
    var _S7 : f32 = a_0.x;
    var _S8 : f32 = a_0.y;
    var _S9 : bool;
    if(_S7 >= _S8)
    {
        _S9 = _S7 >= (a_0.z);
    }
    else
    {
        _S9 = false;
    }
    var _S10 : u32;
    var ma_0 : f32;
    if(_S9)
    {
        var _S11 : bool = (r_0.x) > 0.0f;
        if(_S11)
        {
            _S10 = u32(0);
        }
        else
        {
            _S10 = u32(1);
        }
        o_0.face_1 = _S10;
        if(_S11)
        {
            ma_0 = - r_0.z;
        }
        else
        {
            ma_0 = r_0.z;
        }
        o_0.ndc_0 = vec2<f32>(ma_0, - r_0.y);
        ma_0 = _S7;
    }
    else
    {
        var _S12 : f32 = a_0.z;
        if(_S8 >= _S12)
        {
            var _S13 : bool = (r_0.y) > 0.0f;
            if(_S13)
            {
                _S10 = u32(2);
            }
            else
            {
                _S10 = u32(3);
            }
            o_0.face_1 = _S10;
            var _S14 : f32 = r_0.x;
            if(_S13)
            {
                ma_0 = r_0.z;
            }
            else
            {
                ma_0 = - r_0.z;
            }
            o_0.ndc_0 = vec2<f32>(_S14, ma_0);
            ma_0 = _S8;
        }
        else
        {
            var _S15 : bool = (r_0.z) > 0.0f;
            if(_S15)
            {
                _S10 = u32(4);
            }
            else
            {
                _S10 = u32(5);
            }
            o_0.face_1 = _S10;
            if(_S15)
            {
                ma_0 = r_0.x;
            }
            else
            {
                ma_0 = - r_0.x;
            }
            o_0.ndc_0 = vec2<f32>(ma_0, - r_0.y);
            ma_0 = _S12;
        }
    }
    o_0.ndc_0 = o_0.ndc_0 / vec2<f32>(max(ma_0, 9.99999968265522539e-21f));
    return o_0;
}

fn fullscreen_ndc_to_uv_0( ndc_1 : vec2<f32>) -> vec2<f32>
{
    return vec2<f32>(ndc_1.x * 0.5f + 0.5f, 0.5f - ndc_1.y * 0.5f);
}

fn punctual_shadow_face_coord_0( d_0 : vec3<f32>,  slot_2 : i32) -> vec3<f32>
{
    var f_0 : PunctualShadowFace_0 = punctual_shadow_face_0(d_0);
    return vec3<f32>(fullscreen_ndc_to_uv_0(f_0.ndc_0), f32(u32(slot_2) * u32(6) + f_0.face_1));
}

fn FaceTaps_lit_0( this_1 : FaceTaps_0,  d_1 : vec3<f32>,  reference_2 : f32) -> f32
{
    var _S16 : vec3<f32> = punctual_shadow_face_coord_0(d_1, this_1.slot_0);
    return (textureSampleCompareLevel((punctualShadowMap_0), (shadowSampler_0), ((_S16)).xy, i32(((_S16)).z), (reference_2)));
}

fn punctual_shadow_visibility_0( taps_1 : FaceTaps_0,  shadow_0 : PunctualShadow_0,  lightPos_0 : vec3<f32>,  worldPos_0 : vec3<f32>,  n_0 : vec3<f32>,  l_0 : vec3<f32>,  width_0 : f32) -> f32
{
    var texel_1 : f32 = 2.0f * length(worldPos_0 - lightPos_0) / width_0;
    var d_2 : vec3<f32> = worldPos_0 + n_0 * vec3<f32>((1.5f * texel_1)) + l_0 * vec3<f32>(texel_1) - lightPos_0;
    var _S17 : f32 = max(abs(d_2.x), max(abs(d_2.y), abs(d_2.z)));
    if(_S17 >= (shadow_0.farPlane_0))
    {
        return 1.0f;
    }
    var reference_3 : f32 = punctual_shadow_depth_0(max(_S17, shadow_0.nearPlane_0), shadow_0.nearPlane_0, shadow_0.farPlane_0);
    var axis_0 : vec3<f32> = normalize(d_2);
    var _S18 : vec3<f32>;
    if((abs(axis_0.y)) < 0.99000000953674316f)
    {
        _S18 = vec3<f32>(0.0f, 1.0f, 0.0f);
    }
    else
    {
        _S18 = vec3<f32>(1.0f, 0.0f, 0.0f);
    }
    var tangent_0 : vec3<f32> = normalize(cross(_S18, axis_0));
    var _S19 : vec3<f32> = vec3<f32>((3.0f * _S17 / width_0));
    var _S20 : vec3<f32> = tangent_0 * _S19;
    var _S21 : vec3<f32> = cross(axis_0, tangent_0) * _S19;
    return (FaceTaps_lit_0(taps_1, d_2, reference_3) + FaceTaps_lit_0(taps_1, d_2 + _S20, reference_3) + FaceTaps_lit_0(taps_1, d_2 - _S20, reference_3) + FaceTaps_lit_0(taps_1, d_2 + _S21, reference_3) + FaceTaps_lit_0(taps_1, d_2 - _S21, reference_3)) * 0.20000000298023224f;
}

struct CascadeHit_0
{
     uv_2 : vec2<f32>,
     depth_0 : f32,
     inside_0 : bool,
};

fn cascade_bias_0( nDotL_0 : f32,  biasScale_0 : f32) -> f32
{
    return max(0.0020000000949949f * (1.0f - nDotL_0), 0.00050000002374873f) * biasScale_0;
}

fn fullscreen_ndc_uv_y_sign_0() -> f32
{
    return -1.0f;
}

fn receiver_plane_depth_slope_0( light_space_1 : mat4x4<f32>,  n_1 : vec3<f32>,  uv_y_sign_0 : f32) -> vec2<f32>
{
    var ax_0 : vec3<f32> = light_space_1[i32(0)].xyz;
    var ay_0 : vec3<f32> = light_space_1[i32(1)].xyz;
    var az_0 : vec3<f32> = light_space_1[i32(2)].xyz;
    var light_axis_0 : vec3<f32> = cross(ax_0, ay_0);
    var mx_0 : f32 = dot(n_1, cross(ay_0, az_0));
    var my_0 : f32 = dot(n_1, cross(az_0, ax_0));
    var mz_0 : f32 = dot(n_1, light_axis_0);
    var floor_z_0 : f32 = 0.10000000149011612f * length(light_axis_0);
    var denom_0 : f32;
    if(mz_0 >= 0.0f)
    {
        denom_0 = max(mz_0, floor_z_0);
    }
    else
    {
        denom_0 = min(mz_0, - floor_z_0);
    }
    return vec2<f32>(-2.0f) * vec2<f32>(mx_0, my_0 * uv_y_sign_0) / vec2<f32>(denom_0);
}

fn cascade_depth_slope_0( lightSpace_0 : mat4x4<f32>,  n_2 : vec3<f32>) -> vec2<f32>
{
    return receiver_plane_depth_slope_0(lightSpace_0, n_2, fullscreen_ndc_uv_y_sign_0());
}

fn cascade_visibility_0( taps_2 : CascadeTaps_0,  lightSpace_1 : mat4x4<f32>,  hit_0 : CascadeHit_0,  n_3 : vec3<f32>,  nDotL_1 : f32,  biasScale_1 : f32,  texel_2 : vec2<f32>,  radius_1 : i32) -> f32
{
    return pcf_visibility_0(taps_2, hit_0.uv_2, texel_2, hit_0.depth_0 - cascade_bias_0(nDotL_1, biasScale_1), cascade_depth_slope_0(lightSpace_1, n_3), radius_1);
}

var<private> GGX_ENERGY_COEFFICIENTS_0 : array<vec2<f32>, i32(196)> = array<vec2<f32>, i32(196)>( vec2<f32>(0.03755360096693039f, 3.05165600776672363f), vec2<f32>(0.03963959962129593f, 0.65487277507781982f), vec2<f32>(0.07296779751777649f, 0.51261270046234131f), vec2<f32>(0.17118540406227112f, 0.62788808345794678f), vec2<f32>(0.30981320142745972f, 0.50604218244552612f), vec2<f32>(0.44191190600395203f, 0.40881091356277466f), vec2<f32>(0.55946820974349976f, 0.28783369064331055f), vec2<f32>(0.65159928798675537f, 0.20042069256305695f), vec2<f32>(0.70950692892074585f, 0.12844389677047729f), vec2<f32>(0.73192507028579712f, 0.08202389627695084f), vec2<f32>(0.7161983847618103f, 0.05005419999361038f), vec2<f32>(0.66943138837814331f, 0.03112700022757053f), vec2<f32>(0.59648841619491577f, 0.01862730085849762f), vec2<f32>(0.50741302967071533f, 0.01281030010432005f), vec2<f32>(-0.00930839963257313f, -0.05096510052680969f), vec2<f32>(0.03267690166831017f, 0.88316977024078369f), vec2<f32>(0.11045809835195541f, 0.81648111343383789f), vec2<f32>(0.20676289498806f, 0.65160852670669556f), vec2<f32>(0.32054820656776428f, 0.53221142292022705f), vec2<f32>(0.4455254077911377f, 0.40326109528541565f), vec2<f32>(0.56171208620071411f, 0.2928445041179657f), vec2<f32>(0.65319478511810303f, 0.19869279861450195f), vec2<f32>(0.71095079183578491f, 0.12985430657863617f), vec2<f32>(0.7316511869430542f, 0.08150120079517365f), vec2<f32>(0.71713531017303467f, 0.05052080005407333f), vec2<f32>(0.66998302936553955f, 0.03087439946830273f), vec2<f32>(0.59695768356323242f, 0.01905369944870472f), vec2<f32>(0.50688481330871582f, 0.01136360038071871f), vec2<f32>(0.010585299693048f, 0.99188041687011719f), vec2<f32>(0.00207319995388389f, 0.40556570887565613f), vec2<f32>(0.05862849950790405f, 0.55259382724761963f), vec2<f32>(0.16572730243206024f, 0.58792608976364136f), vec2<f32>(0.29784908890724182f, 0.50322818756103516f), vec2<f32>(0.4318917989730835f, 0.39728268980979919f), vec2<f32>(0.55288940668106079f, 0.2878929078578949f), vec2<f32>(0.64753270149230957f, 0.19779230654239655f), vec2<f32>(0.70724570751190186f, 0.12865540385246277f), vec2<f32>(0.72979277372360229f, 0.08128180354833603f), vec2<f32>(0.71553927659988403f, 0.05013220012187958f), vec2<f32>(0.66890257596969604f, 0.03081009909510612f), vec2<f32>(0.59605568647384644f, 0.01881019957363605f), vec2<f32>(0.50637918710708618f, 0.01177479978650808f), vec2<f32>(0.04574630036950111f, 1.02595400810241699f), vec2<f32>(0.03743439912796021f, 0.88119810819625854f), vec2<f32>(0.04426249861717224f, 0.52496278285980225f), vec2<f32>(0.11310549825429916f, 0.46600010991096497f), vec2<f32>(0.23485749959945679f, 0.4375700056552887f), vec2<f32>(0.37963598966598511f, 0.36149460077285767f), vec2<f32>(0.51556599140167236f, 0.27323558926582336f), vec2<f32>(0.622580885887146f, 0.18986819684505463f), vec2<f32>(0.69136160612106323f, 0.12518900632858276f), vec2<f32>(0.71929609775543213f, 0.07907170057296753f), vec2<f32>(0.70917361974716187f, 0.04900079965591431f), vec2<f32>(0.66434282064437866f, 0.02995469979941845f), vec2<f32>(0.59224581718444824f, 0.01837039925158024f), vec2<f32>(0.5022997260093689f, 0.0110470000654459f), vec2<f32>(0.1206125020980835f, 0.77379542589187622f), vec2<f32>(0.12222660332918167f, 0.83616697788238525f), vec2<f32>(0.11106769740581512f, 0.74211102724075317f), vec2<f32>(0.12635010480880737f, 0.50614690780639648f), vec2<f32>(0.20402370393276215f, 0.39524701237678528f), vec2<f32>(0.32609659433364868f, 0.32315808534622192f), vec2<f32>(0.46191799640655518f, 0.24740250408649445f), vec2<f32>(0.57957160472869873f, 0.17592360079288483f), vec2<f32>(0.66025441884994507f, 0.11711639910936356f), vec2<f32>(0.69881141185760498f, 0.07462230324745178f), vec2<f32>(0.69453942775726318f, 0.04620540142059326f), vec2<f32>(0.65352827310562134f, 0.02824320085346699f), vec2<f32>(0.582955002784729f, 0.01713629998266697f), vec2<f32>(0.49382159113883972f, 0.01042309962213039f), vec2<f32>(0.26457729935646057f, 0.79617220163345337f), vec2<f32>(0.25447279214859009f, 0.71848297119140625f), vec2<f32>(0.24372699856758118f, 0.67536389827728271f), vec2<f32>(0.22911170125007629f, 0.58072292804718018f), vec2<f32>(0.24631629884243011f, 0.4128555953502655f), vec2<f32>(0.32152768969535828f, 0.3022313117980957f), vec2<f32>(0.42885839939117432f, 0.22238540649414062f), vec2<f32>(0.53810352087020874f, 0.157231405377388f), vec2<f32>(0.6227080225944519f, 0.10510919988155365f), vec2<f32>(0.66763401031494141f, 0.0672312006354332f), vec2<f32>(0.67200499773025513f, 0.04161379858851433f), vec2<f32>(0.63588422536849976f, 0.02530450001358986f), vec2<f32>(0.56762188673019409f, 0.01518450025469065f), vec2<f32>(0.47794249653816223f, 0.00909390021115541f), vec2<f32>(0.43767940998077393f, 0.52778768539428711f), vec2<f32>(0.43497830629348755f, 0.55565857887268066f), vec2<f32>(0.42141589522361755f, 0.53836488723754883f), vec2<f32>(0.4008432924747467f, 0.49193951487541199f), vec2<f32>(0.37870550155639648f, 0.40763860940933228f), vec2<f32>(0.38813981413841248f, 0.28910300135612488f), vec2<f32>(0.44539439678192139f, 0.20043960213661194f), vec2<f32>(0.52257758378982544f, 0.13623130321502686f), vec2<f32>(0.59163397550582886f, 0.08995489776134491f), vec2<f32>(0.63798898458480835f, 0.05706769973039627f), vec2<f32>(0.64467430114746094f, 0.03520869836211205f), vec2<f32>(0.61179417371749878f, 0.021161999553442f), vec2<f32>(0.5443912148475647f, 0.01255400013178587f), vec2<f32>(0.45556539297103882f, 0.00719430018216372f), vec2<f32>(0.63681560754776001f, 0.37943580746650696f), vec2<f32>(0.63195270299911499f, 0.36018449068069458f), vec2<f32>(0.62122827768325806f, 0.35363009572029114f), vec2<f32>(0.59902858734130859f, 0.34027791023254395f), vec2<f32>(0.56643378734588623f, 0.30496558547019958f), vec2<f32>(0.5314096212387085f, 0.24438419938087463f), vec2<f32>(0.52357661724090576f, 0.16980840265750885f), vec2<f32>(0.5474010705947876f, 0.11221110075712204f), vec2<f32>(0.58856910467147827f, 0.07183969765901566f), vec2<f32>(0.61636549234390259f, 0.04495130106806755f), vec2<f32>(0.61752068996429443f, 0.02730320021510124f), vec2<f32>(0.58241552114486694f, 0.01623369939625263f), vec2<f32>(0.51425069570541382f, 0.00938300043344498f), vec2<f32>(0.42317068576812744f, 0.00540499994531274f), vec2<f32>(0.81267541646957397f, 0.17737489938735962f), vec2<f32>(0.81089460849761963f, 0.18592619895935059f), vec2<f32>(0.80203372240066528f, 0.18450069427490234f), vec2<f32>(0.78451091051101685f, 0.17923450469970703f), vec2<f32>(0.75236481428146362f, 0.17097190022468567f), vec2<f32>(0.70702672004699707f, 0.15113990008831024f), vec2<f32>(0.65327489376068115f, 0.11831910163164139f), vec2<f32>(0.62291878461837769f, 0.08054299652576447f), vec2<f32>(0.61463671922683716f, 0.05135539919137955f), vec2<f32>(0.61291992664337158f, 0.03151309862732887f), vec2<f32>(0.59584552049636841f, 0.01889899931848049f), vec2<f32>(0.5510483980178833f, 0.01100130006670952f), vec2<f32>(0.4772191047668457f, 0.00626480020582676f), vec2<f32>(0.38401719927787781f, 0.0033825000282377f), vec2<f32>(0.93769299983978271f, 0.06798700243234634f), vec2<f32>(0.93442130088806152f, 0.06284449994564056f), vec2<f32>(0.92932981252670288f, 0.06249019876122475f), vec2<f32>(0.91649138927459717f, 0.06285469979047775f), vec2<f32>(0.89388298988342285f, 0.06178810074925423f), vec2<f32>(0.85349130630493164f, 0.06024239957332611f), vec2<f32>(0.79553717374801636f, 0.05434289947152138f), vec2<f32>(0.72840338945388794f, 0.04295980185270309f), vec2<f32>(0.67323261499404907f, 0.02926450036466122f), vec2<f32>(0.63030177354812622f, 0.01836870051920414f), vec2<f32>(0.58399122953414917f, 0.01094940025359392f), vec2<f32>(0.5202292799949646f, 0.00631939992308617f), vec2<f32>(0.43539279699325562f, 0.0035077000502497f), vec2<f32>(0.33793559670448303f, 0.00192960002459586f), vec2<f32>(0.99163192510604858f, 0.00484689977020025f), vec2<f32>(0.99116522073745728f, 0.00796679966151714f), vec2<f32>(0.98792862892150879f, 0.00806930009275675f), vec2<f32>(0.98114508390426636f, 0.0078320000320673f), vec2<f32>(0.96755552291870117f, 0.00866509974002838f), vec2<f32>(0.94221091270446777f, 0.00961260031908751f), vec2<f32>(0.89904522895812988f, 0.0111905001103878f), vec2<f32>(0.83300900459289551f, 0.01175749953836203f), vec2<f32>(0.75132989883422852f, 0.01031319983303547f), vec2<f32>(0.66817468404769897f, 0.00739389983937144f), vec2<f32>(0.58449500799179077f, 0.00469669979065657f), vec2<f32>(0.49239379167556763f, 0.00274799996986985f), vec2<f32>(0.39069679379463196f, 0.00153979996684939f), vec2<f32>(0.28739228844642639f, 0.00080069998512045f), vec2<f32>(1.00030839443206787f, 0.00132489996030927f), vec2<f32>(1.00059950351715088f, -0.00104150001425296f), vec2<f32>(0.99918842315673828f, -0.00109509995672852f), vec2<f32>(0.99662727117538452f, -0.00078689999645576f), vec2<f32>(0.99067902565002441f, -0.00099460000637919f), vec2<f32>(0.9789891242980957f, -0.00078649999340996f), vec2<f32>(0.95506691932678223f, -0.00063089997274801f), vec2<f32>(0.90853261947631836f, -0.00004849999822909f), vec2<f32>(0.83005660772323608f, 0.00064739998197183f), vec2<f32>(0.72199207544326782f, 0.00106369995046407f), vec2<f32>(0.59814202785491943f, 0.00094429997261614f), vec2<f32>(0.46943360567092896f, 0.0006571999983862f), vec2<f32>(0.34491020441055298f, 0.00038939999649301f), vec2<f32>(0.23603999614715576f, 0.00023839999630582f), vec2<f32>(0.99982082843780518f, -0.00226259999908507f), vec2<f32>(0.99921691417694092f, 0.00083259999519214f), vec2<f32>(0.99951702356338501f, 0.00091000000247732f), vec2<f32>(0.99938511848449707f, 0.00050560000818223f), vec2<f32>(0.9986388087272644f, 0.00080109998816624f), vec2<f32>(0.99591022729873657f, 0.00060789997223765f), vec2<f32>(0.98590642213821411f, 0.00062870001420379f), vec2<f32>(0.95896798372268677f, 0.00030819998937659f), vec2<f32>(0.89731162786483765f, -0.00005109999983688f), vec2<f32>(0.78470510244369507f, -0.00027940000290982f), vec2<f32>(0.62571001052856445f, -0.00019700000120793f), vec2<f32>(0.45251798629760742f, -0.00010209999891231f), vec2<f32>(0.29973480105400085f, -0.00003329999890411f), vec2<f32>(0.18639770150184631f, -0.000034600001527f), vec2<f32>(1.00496220588684082f, 0.00913080014288425f), vec2<f32>(1.00138688087463379f, -0.00241709989495575f), vec2<f32>(1.0036776065826416f, -0.0027135000564158f), vec2<f32>(1.00481629371643066f, -0.00120069994591177f), vec2<f32>(1.008644700050354f, -0.00232520001009107f), vec2<f32>(1.01138985157012939f, -0.00167589995544404f), vec2<f32>(1.01234376430511475f, -0.00196720007807016f), vec2<f32>(0.99929207563400269f, -0.00119440001435578f), vec2<f32>(0.95524948835372925f, -0.00035449999268167f), vec2<f32>(0.85058838129043579f, 0.00027970000519417f), vec2<f32>(0.6671786904335022f, 0.00011629999789875f), vec2<f32>(0.44304949045181274f, 0.00001530000008643f), vec2<f32>(0.25579911470413208f, -0.00005640000017593f), vec2<f32>(0.14276920258998871f, 0.00004659999831347f) );
struct VsIn_0
{
     position_0 : vec3<f32>,
     normal_0 : vec3<f32>,
     uv_3 : vec2<f32>,
     tangent_1 : vec4<f32>,
     joints_0 : vec4<f32>,
     weights_0 : vec4<f32>,
     color_0 : vec4<f32>,
     uv1_0 : vec2<f32>,
     instance0_0 : vec4<f32>,
     instance1_0 : vec4<f32>,
     instance2_0 : vec4<f32>,
     instance3_0 : vec4<f32>,
};

fn instance_matrix_0( In_0 : VsIn_0) -> mat4x4<f32>
{
    return transpose(mat4x4<f32>(In_0.instance0_0, In_0.instance1_0, In_0.instance2_0, In_0.instance3_0));
}

fn skin_matrix_0( In_1 : VsIn_0) -> mat4x4<f32>
{
    var _S22 : f32 = In_1.weights_0.x;
    var _S23 : f32 = In_1.weights_0.y;
    var _S24 : f32 = In_1.weights_0.z;
    var _S25 : f32 = In_1.weights_0.w;
    var total_0 : f32 = _S22 + _S23 + _S24 + _S25;
    if(total_0 <= 0.00009999999747379f)
    {
        return mat4x4<f32>(prim_0.model_0.data_0[i32(0)][i32(0)], prim_0.model_0.data_0[i32(1)][i32(0)], prim_0.model_0.data_0[i32(2)][i32(0)], prim_0.model_0.data_0[i32(3)][i32(0)], prim_0.model_0.data_0[i32(0)][i32(1)], prim_0.model_0.data_0[i32(1)][i32(1)], prim_0.model_0.data_0[i32(2)][i32(1)], prim_0.model_0.data_0[i32(3)][i32(1)], prim_0.model_0.data_0[i32(0)][i32(2)], prim_0.model_0.data_0[i32(1)][i32(2)], prim_0.model_0.data_0[i32(2)][i32(2)], prim_0.model_0.data_0[i32(3)][i32(2)], prim_0.model_0.data_0[i32(0)][i32(3)], prim_0.model_0.data_0[i32(1)][i32(3)], prim_0.model_0.data_0[i32(2)][i32(3)], prim_0.model_0.data_0[i32(3)][i32(3)]);
    }
    var _S26 : mat4x4<f32> = mat4x4<f32>(jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(0)][i32(0)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(1)][i32(0)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(2)][i32(0)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(3)][i32(0)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(0)][i32(1)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(1)][i32(1)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(2)][i32(1)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(3)][i32(1)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(0)][i32(2)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(1)][i32(2)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(2)][i32(2)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(3)][i32(2)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(0)][i32(3)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(1)][i32(3)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(2)][i32(3)], jointMatrices_0[u32(In_1.joints_0.x)].data_1[i32(3)][i32(3)]);
    var _S27 : mat4x4<f32> = mat4x4<f32>(_S22, _S22, _S22, _S22, _S22, _S22, _S22, _S22, _S22, _S22, _S22, _S22, _S22, _S22, _S22, _S22);
    var _S28 : mat4x4<f32> = mat4x4<f32>(jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(0)][i32(0)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(1)][i32(0)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(2)][i32(0)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(3)][i32(0)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(0)][i32(1)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(1)][i32(1)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(2)][i32(1)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(3)][i32(1)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(0)][i32(2)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(1)][i32(2)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(2)][i32(2)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(3)][i32(2)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(0)][i32(3)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(1)][i32(3)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(2)][i32(3)], jointMatrices_0[u32(In_1.joints_0.y)].data_1[i32(3)][i32(3)]);
    var _S29 : mat4x4<f32> = mat4x4<f32>(_S23, _S23, _S23, _S23, _S23, _S23, _S23, _S23, _S23, _S23, _S23, _S23, _S23, _S23, _S23, _S23);
    var _S30 : mat4x4<f32> = mat4x4<f32>(jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(0)][i32(0)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(1)][i32(0)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(2)][i32(0)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(3)][i32(0)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(0)][i32(1)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(1)][i32(1)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(2)][i32(1)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(3)][i32(1)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(0)][i32(2)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(1)][i32(2)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(2)][i32(2)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(3)][i32(2)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(0)][i32(3)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(1)][i32(3)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(2)][i32(3)], jointMatrices_0[u32(In_1.joints_0.z)].data_1[i32(3)][i32(3)]);
    var _S31 : mat4x4<f32> = mat4x4<f32>(_S24, _S24, _S24, _S24, _S24, _S24, _S24, _S24, _S24, _S24, _S24, _S24, _S24, _S24, _S24, _S24);
    var _S32 : mat4x4<f32> = mat4x4<f32>(jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(0)][i32(0)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(1)][i32(0)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(2)][i32(0)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(3)][i32(0)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(0)][i32(1)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(1)][i32(1)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(2)][i32(1)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(3)][i32(1)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(0)][i32(2)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(1)][i32(2)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(2)][i32(2)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(3)][i32(2)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(0)][i32(3)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(1)][i32(3)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(2)][i32(3)], jointMatrices_0[u32(In_1.joints_0.w)].data_1[i32(3)][i32(3)]);
    var _S33 : mat4x4<f32> = mat4x4<f32>(_S25, _S25, _S25, _S25, _S25, _S25, _S25, _S25, _S25, _S25, _S25, _S25, _S25, _S25, _S25, _S25);
    var _S34 : mat4x4<f32> = mat4x4<f32>(_S26[0] * _S27[0], _S26[1] * _S27[1], _S26[2] * _S27[2], _S26[3] * _S27[3]) + mat4x4<f32>(_S28[0] * _S29[0], _S28[1] * _S29[1], _S28[2] * _S29[2], _S28[3] * _S29[3]) + mat4x4<f32>(_S30[0] * _S31[0], _S30[1] * _S31[1], _S30[2] * _S31[2], _S30[3] * _S31[3]) + mat4x4<f32>(_S32[0] * _S33[0], _S32[1] * _S33[1], _S32[2] * _S33[2], _S32[3] * _S33[3]);
    var _S35 : mat4x4<f32> = mat4x4<f32>(1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0, 1.0f / total_0);
    return mat4x4<f32>(_S34[0] * _S35[0], _S34[1] * _S35[1], _S34[2] * _S35[2], _S34[3] * _S35[3]);
}

fn mirror_sign_0( m_0 : mat3x3<f32>) -> f32
{
    var _S36 : f32;
    if((dot(m_0[i32(0)], cross(m_0[i32(1)], m_0[i32(2)]))) < 0.0f)
    {
        _S36 = -1.0f;
    }
    else
    {
        _S36 = 1.0f;
    }
    return _S36;
}

fn normal_transform_0( m_1 : mat3x3<f32>) -> mat3x3<f32>
{
    var _S37 : mat3x3<f32> = mat3x3<f32>(cross(m_1[i32(1)], m_1[i32(2)]), cross(m_1[i32(2)], m_1[i32(0)]), cross(m_1[i32(0)], m_1[i32(1)]));
    var _S38 : mat3x3<f32> = mat3x3<f32>(mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1), mirror_sign_0(m_1));
    return mat3x3<f32>(_S37[0] * _S38[0], _S37[1] * _S38[1], _S37[2] * _S38[2]);
}

fn normalize_or_0( v_0 : vec3<f32>,  fallback_0 : vec3<f32>) -> vec3<f32>
{
    var _S39 : vec3<f32>;
    if((dot(v_0, v_0)) > 1.00000000317107685e-30f)
    {
        _S39 = normalize(v_0);
    }
    else
    {
        _S39 = fallback_0;
    }
    return _S39;
}

struct VsOut_0
{
    @builtin(position) svPosition_0 : vec4<f32>,
    @location(0) worldNormal_0 : vec3<f32>,
    @location(1) uv_4 : vec2<f32>,
    @location(2) lightSpacePos_0 : vec4<f32>,
    @location(3) worldTangent_0 : vec4<f32>,
    @location(4) worldPosition_0 : vec3<f32>,
    @location(6) vertexColor_0 : vec4<f32>,
    @location(7) uv1_1 : vec2<f32>,
};

struct vertexInput_0
{
    @location(0) position_1 : vec3<f32>,
    @location(4) normal_1 : vec3<f32>,
    @location(5) uv_5 : vec2<f32>,
    @location(6) tangent_2 : vec4<f32>,
    @location(7) joints_1 : vec4<f32>,
    @location(8) weights_1 : vec4<f32>,
    @location(9) color_1 : vec4<f32>,
    @location(1) uv1_2 : vec2<f32>,
    @location(10) instance0_1 : vec4<f32>,
    @location(11) instance1_1 : vec4<f32>,
    @location(2) instance2_1 : vec4<f32>,
    @location(3) instance3_1 : vec4<f32>,
};

@vertex
fn vs_main( _S40 : vertexInput_0) -> VsOut_0
{
    var _S41 : VsIn_0 = VsIn_0( _S40.position_1, _S40.normal_1, _S40.uv_5, _S40.tangent_2, _S40.joints_1, _S40.weights_1, _S40.color_1, _S40.uv1_2, _S40.instance0_1, _S40.instance1_1, _S40.instance2_1, _S40.instance3_1 );
    var _S42 : mat4x4<f32> = instance_matrix_0(_S41);
    var model_1 : mat4x4<f32> = (((skin_matrix_0(_S41)) * (_S42)));
    var worldPos_1 : vec4<f32> = (((vec4<f32>(_S40.position_1, 1.0f)) * (model_1)));
    var o_1 : VsOut_0;
    o_1.svPosition_0 = (((worldPos_1) * (mat4x4<f32>(frame_0.view_proj_0.data_0[i32(0)][i32(0)], frame_0.view_proj_0.data_0[i32(1)][i32(0)], frame_0.view_proj_0.data_0[i32(2)][i32(0)], frame_0.view_proj_0.data_0[i32(3)][i32(0)], frame_0.view_proj_0.data_0[i32(0)][i32(1)], frame_0.view_proj_0.data_0[i32(1)][i32(1)], frame_0.view_proj_0.data_0[i32(2)][i32(1)], frame_0.view_proj_0.data_0[i32(3)][i32(1)], frame_0.view_proj_0.data_0[i32(0)][i32(2)], frame_0.view_proj_0.data_0[i32(1)][i32(2)], frame_0.view_proj_0.data_0[i32(2)][i32(2)], frame_0.view_proj_0.data_0[i32(3)][i32(2)], frame_0.view_proj_0.data_0[i32(0)][i32(3)], frame_0.view_proj_0.data_0[i32(1)][i32(3)], frame_0.view_proj_0.data_0[i32(2)][i32(3)], frame_0.view_proj_0.data_0[i32(3)][i32(3)]))));
    if((_S40.weights_1.x + _S40.weights_1.y + _S40.weights_1.z + _S40.weights_1.w) > 0.00009999999747379f)
    {
        o_1.worldNormal_0 = normalize_or_0((((_S40.normal_1) * (normal_transform_0(mat3x3<f32>(model_1[i32(0)].xyz, model_1[i32(1)].xyz, model_1[i32(2)].xyz))))), vec3<f32>(0.0f));
    }
    else
    {
        o_1.worldNormal_0 = normalize_or_0(((((((vec4<f32>(_S40.normal_1, 0.0f)) * (mat4x4<f32>(prim_0.normal_matrix_0.data_0[i32(0)][i32(0)], prim_0.normal_matrix_0.data_0[i32(1)][i32(0)], prim_0.normal_matrix_0.data_0[i32(2)][i32(0)], prim_0.normal_matrix_0.data_0[i32(3)][i32(0)], prim_0.normal_matrix_0.data_0[i32(0)][i32(1)], prim_0.normal_matrix_0.data_0[i32(1)][i32(1)], prim_0.normal_matrix_0.data_0[i32(2)][i32(1)], prim_0.normal_matrix_0.data_0[i32(3)][i32(1)], prim_0.normal_matrix_0.data_0[i32(0)][i32(2)], prim_0.normal_matrix_0.data_0[i32(1)][i32(2)], prim_0.normal_matrix_0.data_0[i32(2)][i32(2)], prim_0.normal_matrix_0.data_0[i32(3)][i32(2)], prim_0.normal_matrix_0.data_0[i32(0)][i32(3)], prim_0.normal_matrix_0.data_0[i32(1)][i32(3)], prim_0.normal_matrix_0.data_0[i32(2)][i32(3)], prim_0.normal_matrix_0.data_0[i32(3)][i32(3)])))).xyz) * (normal_transform_0(mat3x3<f32>(_S42[i32(0)].xyz, _S42[i32(1)].xyz, _S42[i32(2)].xyz))))), vec3<f32>(0.0f));
    }
    o_1.worldTangent_0 = vec4<f32>(normalize_or_0((((vec4<f32>(_S40.tangent_2.xyz, 0.0f)) * (model_1))).xyz, vec3<f32>(0.0f)), _S40.tangent_2.w * mirror_sign_0(mat3x3<f32>(model_1[i32(0)].xyz, model_1[i32(1)].xyz, model_1[i32(2)].xyz)));
    o_1.uv_4 = _S40.uv_5;
    o_1.lightSpacePos_0 = (((worldPos_1) * (mat4x4<f32>(frame_0.light_space_0.data_0[i32(0)][i32(0)], frame_0.light_space_0.data_0[i32(1)][i32(0)], frame_0.light_space_0.data_0[i32(2)][i32(0)], frame_0.light_space_0.data_0[i32(3)][i32(0)], frame_0.light_space_0.data_0[i32(0)][i32(1)], frame_0.light_space_0.data_0[i32(1)][i32(1)], frame_0.light_space_0.data_0[i32(2)][i32(1)], frame_0.light_space_0.data_0[i32(3)][i32(1)], frame_0.light_space_0.data_0[i32(0)][i32(2)], frame_0.light_space_0.data_0[i32(1)][i32(2)], frame_0.light_space_0.data_0[i32(2)][i32(2)], frame_0.light_space_0.data_0[i32(3)][i32(2)], frame_0.light_space_0.data_0[i32(0)][i32(3)], frame_0.light_space_0.data_0[i32(1)][i32(3)], frame_0.light_space_0.data_0[i32(2)][i32(3)], frame_0.light_space_0.data_0[i32(3)][i32(3)]))));
    o_1.worldPosition_0 = worldPos_1.xyz;
    o_1.vertexColor_0 = _S40.color_1;
    o_1.uv1_1 = _S40.uv1_2;
    return o_1;
}

fn shadow_view_proj_0() -> mat4x4<f32>
{
    var view_0 : u32 = shadowView_0.index_0.x;
    if(view_0 == u32(1))
    {
        return mat4x4<f32>(frame_0.light_space_1_0.data_0[i32(0)][i32(0)], frame_0.light_space_1_0.data_0[i32(1)][i32(0)], frame_0.light_space_1_0.data_0[i32(2)][i32(0)], frame_0.light_space_1_0.data_0[i32(3)][i32(0)], frame_0.light_space_1_0.data_0[i32(0)][i32(1)], frame_0.light_space_1_0.data_0[i32(1)][i32(1)], frame_0.light_space_1_0.data_0[i32(2)][i32(1)], frame_0.light_space_1_0.data_0[i32(3)][i32(1)], frame_0.light_space_1_0.data_0[i32(0)][i32(2)], frame_0.light_space_1_0.data_0[i32(1)][i32(2)], frame_0.light_space_1_0.data_0[i32(2)][i32(2)], frame_0.light_space_1_0.data_0[i32(3)][i32(2)], frame_0.light_space_1_0.data_0[i32(0)][i32(3)], frame_0.light_space_1_0.data_0[i32(1)][i32(3)], frame_0.light_space_1_0.data_0[i32(2)][i32(3)], frame_0.light_space_1_0.data_0[i32(3)][i32(3)]);
    }
    if(view_0 == u32(2))
    {
        return mat4x4<f32>(frame_0.light_space_2_0.data_0[i32(0)][i32(0)], frame_0.light_space_2_0.data_0[i32(1)][i32(0)], frame_0.light_space_2_0.data_0[i32(2)][i32(0)], frame_0.light_space_2_0.data_0[i32(3)][i32(0)], frame_0.light_space_2_0.data_0[i32(0)][i32(1)], frame_0.light_space_2_0.data_0[i32(1)][i32(1)], frame_0.light_space_2_0.data_0[i32(2)][i32(1)], frame_0.light_space_2_0.data_0[i32(3)][i32(1)], frame_0.light_space_2_0.data_0[i32(0)][i32(2)], frame_0.light_space_2_0.data_0[i32(1)][i32(2)], frame_0.light_space_2_0.data_0[i32(2)][i32(2)], frame_0.light_space_2_0.data_0[i32(3)][i32(2)], frame_0.light_space_2_0.data_0[i32(0)][i32(3)], frame_0.light_space_2_0.data_0[i32(1)][i32(3)], frame_0.light_space_2_0.data_0[i32(2)][i32(3)], frame_0.light_space_2_0.data_0[i32(3)][i32(3)]);
    }
    if(view_0 == u32(3))
    {
        return mat4x4<f32>(shadowView_0.face_0.data_0[i32(0)][i32(0)], shadowView_0.face_0.data_0[i32(1)][i32(0)], shadowView_0.face_0.data_0[i32(2)][i32(0)], shadowView_0.face_0.data_0[i32(3)][i32(0)], shadowView_0.face_0.data_0[i32(0)][i32(1)], shadowView_0.face_0.data_0[i32(1)][i32(1)], shadowView_0.face_0.data_0[i32(2)][i32(1)], shadowView_0.face_0.data_0[i32(3)][i32(1)], shadowView_0.face_0.data_0[i32(0)][i32(2)], shadowView_0.face_0.data_0[i32(1)][i32(2)], shadowView_0.face_0.data_0[i32(2)][i32(2)], shadowView_0.face_0.data_0[i32(3)][i32(2)], shadowView_0.face_0.data_0[i32(0)][i32(3)], shadowView_0.face_0.data_0[i32(1)][i32(3)], shadowView_0.face_0.data_0[i32(2)][i32(3)], shadowView_0.face_0.data_0[i32(3)][i32(3)]);
    }
    return mat4x4<f32>(frame_0.light_space_0.data_0[i32(0)][i32(0)], frame_0.light_space_0.data_0[i32(1)][i32(0)], frame_0.light_space_0.data_0[i32(2)][i32(0)], frame_0.light_space_0.data_0[i32(3)][i32(0)], frame_0.light_space_0.data_0[i32(0)][i32(1)], frame_0.light_space_0.data_0[i32(1)][i32(1)], frame_0.light_space_0.data_0[i32(2)][i32(1)], frame_0.light_space_0.data_0[i32(3)][i32(1)], frame_0.light_space_0.data_0[i32(0)][i32(2)], frame_0.light_space_0.data_0[i32(1)][i32(2)], frame_0.light_space_0.data_0[i32(2)][i32(2)], frame_0.light_space_0.data_0[i32(3)][i32(2)], frame_0.light_space_0.data_0[i32(0)][i32(3)], frame_0.light_space_0.data_0[i32(1)][i32(3)], frame_0.light_space_0.data_0[i32(2)][i32(3)], frame_0.light_space_0.data_0[i32(3)][i32(3)]);
}

struct vertexOutput_0
{
    @builtin(position) output_0 : vec4<f32>,
};

struct vertexInput_1
{
    @location(0) position_2 : vec3<f32>,
    @location(4) normal_2 : vec3<f32>,
    @location(5) uv_6 : vec2<f32>,
    @location(6) tangent_3 : vec4<f32>,
    @location(7) joints_2 : vec4<f32>,
    @location(8) weights_2 : vec4<f32>,
    @location(9) color_2 : vec4<f32>,
    @location(1) uv1_3 : vec2<f32>,
    @location(10) instance0_2 : vec4<f32>,
    @location(11) instance1_2 : vec4<f32>,
    @location(2) instance2_2 : vec4<f32>,
    @location(3) instance3_2 : vec4<f32>,
};

@vertex
fn vs_shadow( _S43 : vertexInput_1) -> vertexOutput_0
{
    var _S44 : VsIn_0 = VsIn_0( _S43.position_2, _S43.normal_2, _S43.uv_6, _S43.tangent_3, _S43.joints_2, _S43.weights_2, _S43.color_2, _S43.uv1_3, _S43.instance0_2, _S43.instance1_2, _S43.instance2_2, _S43.instance3_2 );
    var _S45 : vertexOutput_0 = vertexOutput_0( ((((((vec4<f32>(_S43.position_2, 1.0f)) * ((((skin_matrix_0(_S44)) * (instance_matrix_0(_S44)))))))) * (shadow_view_proj_0()))) );
    return _S45;
}

fn base_color_uv_select_0( uv0_0 : vec2<f32>,  uv1_4 : vec2<f32>) -> vec2<f32>
{
    var _S46 : vec2<f32>;
    if(((u32(prim_0.material_flags_0.y) & (u32(1)))) != u32(0))
    {
        _S46 = uv1_4;
    }
    else
    {
        _S46 = uv0_0;
    }
    return _S46;
}

struct VsShadowMaskedOut_0
{
    @builtin(position) pos_0 : vec4<f32>,
    @location(0) uv_7 : vec2<f32>,
    @location(1) alpha_0 : f32,
};

struct vertexInput_2
{
    @location(0) position_3 : vec3<f32>,
    @location(4) normal_3 : vec3<f32>,
    @location(5) uv_8 : vec2<f32>,
    @location(6) tangent_4 : vec4<f32>,
    @location(7) joints_3 : vec4<f32>,
    @location(8) weights_3 : vec4<f32>,
    @location(9) color_3 : vec4<f32>,
    @location(1) uv1_5 : vec2<f32>,
    @location(10) instance0_3 : vec4<f32>,
    @location(11) instance1_3 : vec4<f32>,
    @location(2) instance2_3 : vec4<f32>,
    @location(3) instance3_3 : vec4<f32>,
};

@vertex
fn vs_shadow_masked( _S47 : vertexInput_2) -> VsShadowMaskedOut_0
{
    var _S48 : VsIn_0 = VsIn_0( _S47.position_3, _S47.normal_3, _S47.uv_8, _S47.tangent_4, _S47.joints_3, _S47.weights_3, _S47.color_3, _S47.uv1_5, _S47.instance0_3, _S47.instance1_3, _S47.instance2_3, _S47.instance3_3 );
    var o_2 : VsShadowMaskedOut_0;
    o_2.pos_0 = ((((((vec4<f32>(_S47.position_3, 1.0f)) * ((((skin_matrix_0(_S48)) * (instance_matrix_0(_S48)))))))) * (shadow_view_proj_0())));
    o_2.uv_7 = base_color_uv_select_0(_S47.uv_8, _S47.uv1_5);
    o_2.alpha_0 = _S47.color_3.w;
    return o_2;
}

fn transform_uv_0( uv_9 : vec2<f32>,  row0_0 : vec3<f32>,  row1_0 : vec3<f32>) -> vec2<f32>
{
    var _S49 : vec3<f32> = vec3<f32>(uv_9, 1.0f);
    return vec2<f32>(dot(_S49, row0_0), dot(_S49, row1_0));
}

fn base_color_uv_rows_0( uv_10 : vec2<f32>) -> vec2<f32>
{
    return transform_uv_0(uv_10, prim_0.base_uv_row0_0.xyz, prim_0.base_uv_row1_0.xyz);
}

fn prim_alpha_mode_0() -> i32
{
    return i32(prim_0.material_flags_0.z);
}

fn alpha_masked_out_0( alpha_mode_0 : i32,  alpha_1 : f32,  cutoff_0 : f32) -> bool
{
    var _S50 : bool;
    if(alpha_mode_0 == i32(1))
    {
        _S50 = alpha_1 < cutoff_0;
    }
    else
    {
        _S50 = false;
    }
    return _S50;
}

struct pixelInput_0
{
    @location(0) uv_11 : vec2<f32>,
    @location(1) alpha_2 : f32,
};

@fragment
fn fs_shadow_masked( _S51 : pixelInput_0, @builtin(position) pos_1 : vec4<f32>)
{
    if(alpha_masked_out_0(prim_alpha_mode_0(), prim_0.base_color_0.w * (textureSample((baseColorTex_0), (baseColorSampler_0), (base_color_uv_rows_0(_S51.uv_11)), (vec2<i32>(i32(0))))).w * _S51.alpha_2, prim_0.emissive_factor_0.w))
    {
        discard;
    }
    return;
}

fn base_color_uv_0( uv0_1 : vec2<f32>,  uv1_6 : vec2<f32>) -> vec2<f32>
{
    return base_color_uv_rows_0(base_color_uv_select_0(uv0_1, uv1_6));
}

fn metallic_roughness_uv_0( uv0_2 : vec2<f32>,  uv1_7 : vec2<f32>) -> vec2<f32>
{
    var uv_12 : vec2<f32>;
    if(((u32(prim_0.material_flags_0.y) & (u32(2)))) != u32(0))
    {
        uv_12 = uv1_7;
    }
    else
    {
        uv_12 = uv0_2;
    }
    return transform_uv_0(uv_12, prim_0.mr_uv_row0_0.xyz, prim_0.mr_uv_row1_0.xyz);
}

fn normal_uv_0( uv0_3 : vec2<f32>,  uv1_8 : vec2<f32>) -> vec2<f32>
{
    var uv_13 : vec2<f32>;
    if(((u32(prim_0.material_flags_0.y) & (u32(4)))) != u32(0))
    {
        uv_13 = uv1_8;
    }
    else
    {
        uv_13 = uv0_3;
    }
    return transform_uv_0(uv_13, prim_0.normal_uv_row0_0.xyz, prim_0.normal_uv_row1_0.xyz);
}

fn emissive_uv_0( uv0_4 : vec2<f32>,  uv1_9 : vec2<f32>) -> vec2<f32>
{
    var uv_14 : vec2<f32>;
    if(((u32(prim_0.material_flags_0.y) & (u32(8)))) != u32(0))
    {
        uv_14 = uv1_9;
    }
    else
    {
        uv_14 = uv0_4;
    }
    return transform_uv_0(uv_14, prim_0.emissive_uv_row0_0.xyz, prim_0.emissive_uv_row1_0.xyz);
}

fn occlusion_uv_0( uv0_5 : vec2<f32>,  uv1_10 : vec2<f32>) -> vec2<f32>
{
    var uv_15 : vec2<f32>;
    if(((u32(prim_0.material_flags_0.y) & (u32(16)))) != u32(0))
    {
        uv_15 = uv1_10;
    }
    else
    {
        uv_15 = uv0_5;
    }
    return transform_uv_0(uv_15, prim_0.occlusion_uv_row0_0.xyz, prim_0.occlusion_uv_row1_0.xyz);
}

fn material_metallic_0( factor_0 : f32,  sampled_0 : f32) -> f32
{
    return clamp(factor_0 * sampled_0, 0.0f, 1.0f);
}

fn material_roughness_0( factor_1 : f32,  sampled_1 : f32) -> f32
{
    return clamp(factor_1 * sampled_1, 0.04500000178813934f, 1.0f);
}

fn tangent_space_normal_0( texel_3 : vec3<f32>,  scale_0 : f32) -> vec3<f32>
{
    var n_4 : vec3<f32> = texel_3 * vec3<f32>(2.0f) - vec3<f32>(1.0f);
    if((texel_3.z) == 0.0f)
    {
        n_4[i32(2)] = sqrt(saturate(1.0f - dot(n_4.xy, n_4.xy)));
    }
    return vec3<f32>(n_4.xy * vec2<f32>(scale_0), n_4.z);
}

fn apply_normal_map_0( normal_4 : vec3<f32>,  tangent_5 : vec4<f32>,  tangent_normal_0 : vec3<f32>,  front_face_0 : bool) -> vec3<f32>
{
    var _S52 : vec3<f32> = tangent_5.xyz;
    var t_0 : vec3<f32> = normalize_or_0(_S52 - normal_4 * vec3<f32>(dot(normal_4, _S52)), vec3<f32>(0.0f));
    var mapped_0 : vec3<f32> = normalize_or_0((((mat3x3<f32>(t_0, cross(normal_4, t_0) * vec3<f32>(tangent_5.w), normal_4)) * (tangent_normal_0))), normal_4);
    var _S53 : vec3<f32>;
    if(front_face_0)
    {
        _S53 = mapped_0;
    }
    else
    {
        _S53 = (vec3<f32>(0) - mapped_0);
    }
    return _S53;
}

struct SurfacePoint_0
{
     position_4 : vec3<f32>,
     n_5 : vec3<f32>,
     albedo_0 : vec3<f32>,
     metallic_0 : f32,
     roughness_0 : f32,
     occlusion_0 : f32,
     emissive_0 : vec3<f32>,
};

struct MaterialSample_0
{
     surface_0 : SurfacePoint_0,
     alpha_3 : f32,
};

fn sample_material_0( In_2 : VsOut_0,  isFrontFace_0 : bool) -> MaterialSample_0
{
    var mrSample_0 : vec4<f32> = (textureSample((metalRoughTex_0), (metalRoughSampler_0), (metallic_roughness_uv_0(In_2.uv_4, In_2.uv1_1))));
    var normalSample_0 : vec4<f32> = (textureSample((normalTex_0), (normalSampler_0), (normal_uv_0(In_2.uv_4, In_2.uv1_1))));
    var emissiveSample_0 : vec4<f32> = (textureSample((emissiveTex_0), (emissiveSampler_0), (emissive_uv_0(In_2.uv_4, In_2.uv1_1))));
    var occlusionSample_0 : vec4<f32> = (textureSample((occlusionTex_0), (occlusionSampler_0), (occlusion_uv_0(In_2.uv_4, In_2.uv1_1))));
    var albedo_1 : vec4<f32> = prim_0.base_color_0 * (textureSample((baseColorTex_0), (baseColorSampler_0), (base_color_uv_0(In_2.uv_4, In_2.uv1_1)))) * In_2.vertexColor_0;
    var m_2 : MaterialSample_0;
    m_2.alpha_3 = albedo_1.w;
    m_2.surface_0.position_4 = In_2.worldPosition_0;
    m_2.surface_0.albedo_0 = albedo_1.xyz;
    m_2.surface_0.metallic_0 = material_metallic_0(prim_0.material_factors_0.x, mrSample_0.z);
    m_2.surface_0.roughness_0 = material_roughness_0(prim_0.material_factors_0.y, mrSample_0.y);
    m_2.surface_0.occlusion_0 = mix(1.0f, occlusionSample_0.x, prim_0.material_factors_0.z);
    m_2.surface_0.emissive_0 = prim_0.emissive_factor_0.xyz * emissiveSample_0.xyz;
    m_2.surface_0.n_5 = apply_normal_map_0(normalize_or_0(In_2.worldNormal_0, normalize(frame_0.camera_position_0.xyz - In_2.worldPosition_0)), In_2.worldTangent_0, tangent_space_normal_0(normalSample_0.xyz, prim_0.material_factors_0.w), isFrontFace_0);
    return m_2;
}

fn surface_f0_0( albedo_2 : vec3<f32>,  metallic_1 : f32) -> vec3<f32>
{
    return mix(vec3<f32>(0.03999999910593033f), albedo_2, vec3<f32>(metallic_1));
}

fn ggx_energy_coordinates_0( n_dot_v_0 : f32,  roughness_1 : f32) -> vec2<f32>
{
    var low_0 : vec2<f32> = vec2<f32>(pow(0.00009999999747379f, 0.3333333432674408f), sqrt(0.04500000178813934f));
    return (vec2<f32>(pow(clamp(n_dot_v_0, 0.00009999999747379f, 1.0f), 0.3333333432674408f), sqrt(clamp(roughness_1, 0.04500000178813934f, 1.0f))) - low_0) / (vec2<f32>(1.0f) - low_0);
}

fn ggx_energy_weights_0( f_1 : f32) -> vec4<f32>
{
    var g_0 : f32 = 1.0f - f_1;
    return vec4<f32>(g_0 * g_0 * g_0, 4.0f - 6.0f * f_1 * f_1 + 3.0f * f_1 * f_1 * f_1, 4.0f - 6.0f * g_0 * g_0 + 3.0f * g_0 * g_0 * g_0, f_1 * f_1 * f_1) / vec4<f32>(6.0f);
}

fn ggx_energy_fit_0( t_1 : vec2<f32>) -> vec2<f32>
{
    var x_1 : vec2<f32> = t_1 * vec2<f32>(11.0f) + vec2<f32>(1.0f);
    var cell_0 : vec2<f32> = clamp(floor(x_1), vec2<f32>(1.0f), vec2<f32>(11.0f));
    var _S54 : f32 = cell_0.x;
    var _S55 : vec4<f32> = ggx_energy_weights_0(x_1.x - _S54);
    var _S56 : f32 = cell_0.y;
    var _S57 : vec4<f32> = ggx_energy_weights_0(x_1.y - _S56);
    var _S58 : i32 = i32(_S54 - 1.0f) * i32(14) + i32(_S56 - 1.0f);
    var _S59 : vec2<f32> = vec2<f32>(0.0f);
    var i_0 : i32 = i32(0);
    var e_0 : vec2<f32> = _S59;
    for(;;)
    {
        if(i_0 < i32(4))
        {
        }
        else
        {
            break;
        }
        var j_0 : i32 = i32(0);
        var row_0 : vec2<f32> = _S59;
        for(;;)
        {
            if(j_0 < i32(4))
            {
            }
            else
            {
                break;
            }
            var row_1 : vec2<f32> = row_0 + GGX_ENERGY_COEFFICIENTS_0[_S58 + i_0 * i32(14) + j_0] * vec2<f32>(_S57[j_0]);
            j_0 = j_0 + i32(1);
            row_0 = row_1;
        }
        var e_1 : vec2<f32> = e_0 + row_0 * vec2<f32>(_S55[i_0]);
        i_0 = i_0 + i32(1);
        e_0 = e_1;
    }
    return e_0;
}

fn ggx_energy_0( n_dot_v_1 : f32,  roughness_2 : f32) -> vec2<f32>
{
    return ggx_energy_fit_0(ggx_energy_coordinates_0(n_dot_v_1, roughness_2));
}

fn smith_k_analytic_0( roughness_3 : f32) -> f32
{
    var r_1 : f32 = max(roughness_3, 0.04500000178813934f) + 1.0f;
    return r_1 * r_1 / 8.0f;
}

fn distribution_ggx_0( n_dot_h_0 : f32,  roughness_4 : f32) -> f32
{
    var _S60 : f32 = max(roughness_4, 0.04500000178813934f);
    var a_1 : f32 = _S60 * _S60;
    var a2_0 : f32 = a_1 * a_1;
    var denom_1 : f32 = n_dot_h_0 * n_dot_h_0 * (a2_0 - 1.0f) + 1.0f;
    return a2_0 / (3.14159274101257324f * max(denom_1 * denom_1, 9.99999968265522539e-21f));
}

fn geometry_smith_k_0( n_dot_v_2 : f32,  n_dot_l_0 : f32,  k_0 : f32) -> f32
{
    var _S61 : f32 = 1.0f - k_0;
    return n_dot_v_2 / max(n_dot_v_2 * _S61 + k_0, 9.99999997475242708e-07f) * (n_dot_l_0 / max(n_dot_l_0 * _S61 + k_0, 9.99999997475242708e-07f));
}

fn fresnel_schlick_0( cos_theta_0 : f32,  f0_0 : vec3<f32>) -> vec3<f32>
{
    return f0_0 + (vec3<f32>(1.0f) - f0_0) * vec3<f32>(pow(clamp(1.0f - cos_theta_0, 0.0f, 1.0f), 5.0f));
}

fn ggx_single_scatter_0( n_dot_v_3 : f32,  n_dot_l_1 : f32,  n_dot_h_1 : f32,  h_dot_v_0 : f32,  roughness_5 : f32,  f0_1 : vec3<f32>,  k_1 : f32) -> vec3<f32>
{
    return vec3<f32>((distribution_ggx_0(n_dot_h_1, roughness_5) * geometry_smith_k_0(n_dot_v_3, n_dot_l_1, k_1))) * fresnel_schlick_0(h_dot_v_0, f0_1) / vec3<f32>(max(4.0f * n_dot_v_3 * n_dot_l_1, 9.99999997475242708e-07f));
}

fn multiscatter_gain_0( energy_0 : vec2<f32>,  f0_2 : vec3<f32>) -> vec3<f32>
{
    return vec3<f32>(1.0f) + f0_2 * vec3<f32>((1.0f / max(energy_0.x + energy_0.y, 0.00100000004749745f) - 1.0f));
}

fn specular_albedo_0( energy_1 : vec2<f32>,  f0_3 : vec3<f32>) -> vec3<f32>
{
    return multiscatter_gain_0(energy_1, f0_3) * (f0_3 * vec3<f32>(energy_1.x) + vec3<f32>(energy_1.y));
}

fn lambert_diffuse_0( albedo_3 : vec3<f32>) -> vec3<f32>
{
    return albedo_3 / vec3<f32>(3.14159274101257324f);
}

fn brdf_smith_k_0( n_6 : vec3<f32>,  v_1 : vec3<f32>,  l_1 : vec3<f32>,  albedo_4 : vec3<f32>,  metallic_2 : f32,  roughness_6 : f32,  f0_4 : vec3<f32>,  radiance_0 : vec3<f32>,  k_2 : f32,  energy_2 : vec2<f32>) -> vec3<f32>
{
    var h_0 : vec3<f32> = normalize(l_1 + v_1);
    var _S62 : f32 = max(dot(n_6, l_1), 0.0f);
    return (saturate(vec3<f32>(1.0f) - specular_albedo_0(energy_2, f0_4)) * vec3<f32>((1.0f - metallic_2)) * lambert_diffuse_0(albedo_4) + multiscatter_gain_0(energy_2, f0_4) * ggx_single_scatter_0(max(dot(n_6, v_1), 0.00009999999747379f), _S62, max(dot(n_6, h_0), 0.0f), max(dot(h_0, v_1), 0.0f), max(roughness_6, 0.04500000178813934f), f0_4, k_2)) * radiance_0 * vec3<f32>(_S62);
}

fn brdf_direct_0( n_7 : vec3<f32>,  v_2 : vec3<f32>,  l_2 : vec3<f32>,  albedo_5 : vec3<f32>,  metallic_3 : f32,  roughness_7 : f32,  f0_5 : vec3<f32>,  radiance_1 : vec3<f32>,  energy_3 : vec2<f32>) -> vec3<f32>
{
    return brdf_smith_k_0(n_7, v_2, l_2, albedo_5, metallic_3, roughness_7, f0_5, radiance_1, smith_k_analytic_0(roughness_7), energy_3);
}

fn select_cascade_0( viewDepth_0 : f32,  splits_0 : vec2<f32>,  count_0 : i32) -> i32
{
    var selected_0 : i32;
    if(viewDepth_0 >= (splits_0.x))
    {
        selected_0 = i32(1);
    }
    else
    {
        selected_0 = i32(0);
    }
    if(viewDepth_0 >= (splits_0.y))
    {
        selected_0 = i32(2);
    }
    return min(selected_0, count_0 - i32(1));
}

fn light_space_for_0( cascade_0 : i32) -> mat4x4<f32>
{
    if(cascade_0 == i32(1))
    {
        return mat4x4<f32>(frame_0.light_space_1_0.data_0[i32(0)][i32(0)], frame_0.light_space_1_0.data_0[i32(1)][i32(0)], frame_0.light_space_1_0.data_0[i32(2)][i32(0)], frame_0.light_space_1_0.data_0[i32(3)][i32(0)], frame_0.light_space_1_0.data_0[i32(0)][i32(1)], frame_0.light_space_1_0.data_0[i32(1)][i32(1)], frame_0.light_space_1_0.data_0[i32(2)][i32(1)], frame_0.light_space_1_0.data_0[i32(3)][i32(1)], frame_0.light_space_1_0.data_0[i32(0)][i32(2)], frame_0.light_space_1_0.data_0[i32(1)][i32(2)], frame_0.light_space_1_0.data_0[i32(2)][i32(2)], frame_0.light_space_1_0.data_0[i32(3)][i32(2)], frame_0.light_space_1_0.data_0[i32(0)][i32(3)], frame_0.light_space_1_0.data_0[i32(1)][i32(3)], frame_0.light_space_1_0.data_0[i32(2)][i32(3)], frame_0.light_space_1_0.data_0[i32(3)][i32(3)]);
    }
    if(cascade_0 == i32(2))
    {
        return mat4x4<f32>(frame_0.light_space_2_0.data_0[i32(0)][i32(0)], frame_0.light_space_2_0.data_0[i32(1)][i32(0)], frame_0.light_space_2_0.data_0[i32(2)][i32(0)], frame_0.light_space_2_0.data_0[i32(3)][i32(0)], frame_0.light_space_2_0.data_0[i32(0)][i32(1)], frame_0.light_space_2_0.data_0[i32(1)][i32(1)], frame_0.light_space_2_0.data_0[i32(2)][i32(1)], frame_0.light_space_2_0.data_0[i32(3)][i32(1)], frame_0.light_space_2_0.data_0[i32(0)][i32(2)], frame_0.light_space_2_0.data_0[i32(1)][i32(2)], frame_0.light_space_2_0.data_0[i32(2)][i32(2)], frame_0.light_space_2_0.data_0[i32(3)][i32(2)], frame_0.light_space_2_0.data_0[i32(0)][i32(3)], frame_0.light_space_2_0.data_0[i32(1)][i32(3)], frame_0.light_space_2_0.data_0[i32(2)][i32(3)], frame_0.light_space_2_0.data_0[i32(3)][i32(3)]);
    }
    return mat4x4<f32>(frame_0.light_space_0.data_0[i32(0)][i32(0)], frame_0.light_space_0.data_0[i32(1)][i32(0)], frame_0.light_space_0.data_0[i32(2)][i32(0)], frame_0.light_space_0.data_0[i32(3)][i32(0)], frame_0.light_space_0.data_0[i32(0)][i32(1)], frame_0.light_space_0.data_0[i32(1)][i32(1)], frame_0.light_space_0.data_0[i32(2)][i32(1)], frame_0.light_space_0.data_0[i32(3)][i32(1)], frame_0.light_space_0.data_0[i32(0)][i32(2)], frame_0.light_space_0.data_0[i32(1)][i32(2)], frame_0.light_space_0.data_0[i32(2)][i32(2)], frame_0.light_space_0.data_0[i32(3)][i32(2)], frame_0.light_space_0.data_0[i32(0)][i32(3)], frame_0.light_space_0.data_0[i32(1)][i32(3)], frame_0.light_space_0.data_0[i32(2)][i32(3)], frame_0.light_space_0.data_0[i32(3)][i32(3)]);
}

fn cascade_hit_0( lightSpace_2 : mat4x4<f32>,  worldPos_2 : vec3<f32>) -> CascadeHit_0
{
    var lightSpacePos_1 : vec4<f32> = (((vec4<f32>(worldPos_2, 1.0f)) * (lightSpace_2)));
    var proj_0 : vec3<f32> = lightSpacePos_1.xyz / vec3<f32>(lightSpacePos_1.w);
    var hit_1 : CascadeHit_0;
    var _S63 : vec2<f32> = fullscreen_ndc_to_uv_0(proj_0.xy);
    hit_1.uv_2 = _S63;
    var _S64 : f32 = proj_0.z;
    hit_1.depth_0 = _S64;
    var _S65 : bool;
    if((_S63.x) >= 0.0f)
    {
        _S65 = (hit_1.uv_2.x) <= 1.0f;
    }
    else
    {
        _S65 = false;
    }
    if(_S65)
    {
        _S65 = (hit_1.uv_2.y) >= 0.0f;
    }
    else
    {
        _S65 = false;
    }
    if(_S65)
    {
        _S65 = (hit_1.uv_2.y) <= 1.0f;
    }
    else
    {
        _S65 = false;
    }
    if(_S65)
    {
        _S65 = _S64 >= 0.0f;
    }
    else
    {
        _S65 = false;
    }
    if(_S65)
    {
        _S65 = _S64 <= 1.0f;
    }
    else
    {
        _S65 = false;
    }
    hit_1.inside_0 = _S65;
    return hit_1;
}

fn CascadeTaps_x24init_0( layer_1 : f32) -> CascadeTaps_0
{
    var _S66 : CascadeTaps_0;
    _S66.layer_0 = layer_1;
    return _S66;
}

fn bias_scale_for_0( cascade_1 : i32) -> f32
{
    if(cascade_1 == i32(1))
    {
        return frame_0.shadow_bias_scale_0.y;
    }
    if(cascade_1 == i32(2))
    {
        return frame_0.shadow_bias_scale_0.z;
    }
    return frame_0.shadow_bias_scale_0.x;
}

fn shadow_visibility_0( worldPos_3 : vec3<f32>,  n_8 : vec3<f32>,  nDotL_2 : f32) -> f32
{
    var count_1 : i32 = clamp(i32(frame_0.shadow_params_0.x), i32(0), i32(3));
    if(count_1 == i32(0))
    {
        return 1.0f;
    }
    var _S67 : i32 = select_cascade_0(dot(worldPos_3 - frame_0.camera_position_0.xyz, frame_0.shadow_view_dir_0.xyz), frame_0.cascade_splits_0.xy, count_1);
    var hit_2 : CascadeHit_0;
    hit_2.inside_0 = false;
    var cascade_2 : i32 = _S67;
    for(;;)
    {
        if(cascade_2 < count_1)
        {
        }
        else
        {
            break;
        }
        hit_2 = cascade_hit_0(light_space_for_0(cascade_2), worldPos_3);
        if(hit_2.inside_0)
        {
            break;
        }
        cascade_2 = cascade_2 + i32(1);
    }
    if(!hit_2.inside_0)
    {
        return 1.0f;
    }
    var radius_2 : i32 = clamp(i32(frame_0.shadow_params_0.y), i32(0), i32(5));
    var shadowWidth_0 : u32;
    var shadowHeight_0 : u32;
    var shadowLayers_0 : u32;
    {var dim = textureDimensions((shadowMap_0));((shadowWidth_0)) = dim.x;((shadowHeight_0)) = dim.y;((shadowLayers_0)) = textureNumLayers((shadowMap_0));};
    return cascade_visibility_0(CascadeTaps_x24init_0(f32(cascade_2)), light_space_for_0(cascade_2), hit_2, n_8, nDotL_2, bias_scale_for_0(cascade_2), vec2<f32>(1.0f / f32(shadowWidth_0), 1.0f / f32(shadowHeight_0)), radius_2);
}

struct PunctualLight_0
{
     position_5 : vec3<f32>,
     kind_0 : f32,
     radiance_2 : vec3<f32>,
     range_0 : f32,
     direction_0 : vec3<f32>,
     cos_inner_0 : f32,
     cos_outer_0 : f32,
};

fn unpack_punctual_light_0( a_2 : vec4<f32>,  b_0 : vec4<f32>,  c_0 : vec4<f32>,  d_3 : vec4<f32>) -> PunctualLight_0
{
    var light_0 : PunctualLight_0;
    light_0.position_5 = a_2.xyz;
    light_0.kind_0 = a_2.w;
    light_0.radiance_2 = b_0.xyz;
    light_0.range_0 = b_0.w;
    light_0.direction_0 = c_0.xyz;
    light_0.cos_inner_0 = c_0.w;
    light_0.cos_outer_0 = d_3.x;
    return light_0;
}

struct PunctualIncidence_0
{
     l_3 : vec3<f32>,
     distance_0 : f32,
     attenuation_0 : f32,
};

fn punctual_incidence_0( light_1 : PunctualLight_0,  position_6 : vec3<f32>) -> PunctualIncidence_0
{
    var o_3 : PunctualIncidence_0;
    o_3.attenuation_0 = 1.0f;
    if((light_1.kind_0) > 2.5f)
    {
        o_3.l_3 = normalize((vec3<f32>(0) - light_1.direction_0));
        o_3.distance_0 = 3.00000000549775576e+38f;
        return o_3;
    }
    var toLight_0 : vec3<f32> = light_1.position_5 - position_6;
    var _S68 : f32 = max(length(toLight_0), 0.00009999999747379f);
    o_3.distance_0 = _S68;
    o_3.l_3 = toLight_0 / vec3<f32>(_S68);
    o_3.attenuation_0 = 1.0f / (_S68 * _S68);
    if((light_1.range_0) > 0.0f)
    {
        var k_3 : f32 = clamp(1.0f - pow(o_3.distance_0 / light_1.range_0, 4.0f), 0.0f, 1.0f);
        o_3.attenuation_0 = o_3.attenuation_0 * (k_3 * k_3);
    }
    if((light_1.kind_0) > 1.5f)
    {
        o_3.attenuation_0 = o_3.attenuation_0 * smoothstep(light_1.cos_outer_0, light_1.cos_inner_0, dot(normalize(light_1.direction_0), (vec3<f32>(0) - o_3.l_3)));
    }
    return o_3;
}

fn unpack_punctual_shadow_0( d_4 : vec4<f32>) -> PunctualShadow_0
{
    var shadow_1 : PunctualShadow_0;
    shadow_1.slot_1 = i32(d_4.y + 0.5f) - i32(1);
    shadow_1.nearPlane_0 = d_4.z;
    shadow_1.farPlane_0 = d_4.w;
    return shadow_1;
}

fn FaceTaps_x24init_0( slot_3 : i32) -> FaceTaps_0
{
    var _S69 : FaceTaps_0;
    _S69.slot_0 = slot_3;
    return _S69;
}

fn punctual_lighting_0( worldPos_4 : vec3<f32>,  n_9 : vec3<f32>,  v_3 : vec3<f32>,  albedo_6 : vec3<f32>,  metallic_4 : f32,  roughness_8 : f32,  f0_6 : vec3<f32>,  energy_4 : vec2<f32>,  fragCoord_0 : vec4<f32>) -> vec3<f32>
{
    var _S70 : vec3<f32> = vec3<f32>(0.0f);
    var tileX_0 : u32 = u32(fragCoord_0.x) / u32(16);
    var tileY_0 : u32 = u32(fragCoord_0.y) / u32(16);
    var tileW_0 : u32 = u32(frame_0.cascade_splits_0.z);
    var tileH_0 : u32 = u32(frame_0.cascade_splits_0.w);
    var _S71 : u32 = u32(frame_0.camera_position_0.w);
    var _S72 : u32 = u32(frame_0.shadow_params_0.w);
    var tileCount_0 : u32;
    var tileOffset_0 : u32;
    var listed_0 : bool;
    if(tileW_0 != u32(0))
    {
        var gridEntry_0 : vec2<u32> = tileLightGrid_0[min(tileY_0, max(tileH_0, u32(1)) - u32(1)) * tileW_0 + min(tileX_0, max(tileW_0, u32(1)) - u32(1))];
        var _S73 : u32 = gridEntry_0.x;
        var listed_1 : bool = _S73 <= u32(32);
        if(listed_1)
        {
            var _S74 : u32 = gridEntry_0.y;
            tileCount_0 = _S73;
            tileOffset_0 = _S74;
        }
        else
        {
            tileCount_0 = _S71;
            tileOffset_0 = u32(0);
        }
        listed_0 = listed_1;
    }
    else
    {
        tileCount_0 = _S71;
        listed_0 = false;
        tileOffset_0 = u32(0);
    }
    var j_1 : u32 = u32(0);
    var total_1 : vec3<f32> = _S70;
    for(;;)
    {
        if(j_1 < tileCount_0)
        {
        }
        else
        {
            break;
        }
        var lightIdx_0 : u32;
        if(listed_0)
        {
            lightIdx_0 = tileLightIndices_0[tileOffset_0 + j_1];
        }
        else
        {
            lightIdx_0 = j_1;
        }
        var _S75 : u32 = lightIdx_0 * u32(4);
        var _S76 : u32 = _S75 + u32(3);
        var light_2 : PunctualLight_0 = unpack_punctual_light_0(punctualLights_0[_S75], punctualLights_0[_S75 + u32(1)], punctualLights_0[_S75 + u32(2)], punctualLights_0[_S76]);
        var incidence_0 : PunctualIncidence_0 = punctual_incidence_0(light_2, worldPos_4);
        if((incidence_0.attenuation_0) <= 0.0f)
        {
            j_1 = j_1 + u32(1);
            continue;
        }
        var shadow_2 : PunctualShadow_0 = unpack_punctual_shadow_0(punctualLights_0[_S76]);
        var _S77 : bool;
        if((shadow_2.slot_1) >= i32(0))
        {
            _S77 = u32(shadow_2.slot_1) < _S72;
        }
        else
        {
            _S77 = false;
        }
        var _S78 : bool;
        if(_S77)
        {
            _S78 = (dot(n_9, incidence_0.l_3)) > 0.0f;
        }
        else
        {
            _S78 = false;
        }
        var visibility_0 : f32;
        if(_S78)
        {
            var width_1 : u32;
            var height_0 : u32;
            var layers_0 : u32;
            {var dim = textureDimensions((punctualShadowMap_0));((width_1)) = dim.x;((height_0)) = dim.y;((layers_0)) = textureNumLayers((punctualShadowMap_0));};
            visibility_0 = punctual_shadow_visibility_0(FaceTaps_x24init_0(shadow_2.slot_1), shadow_2, light_2.position_5, worldPos_4, n_9, incidence_0.l_3, f32(width_1));
        }
        else
        {
            visibility_0 = 1.0f;
        }
        total_1 = total_1 + brdf_direct_0(n_9, v_3, incidence_0.l_3, albedo_6, metallic_4, roughness_8, f0_6, light_2.radiance_2 * vec3<f32>(incidence_0.attenuation_0) * vec3<f32>(visibility_0), energy_4);
        j_1 = j_1 + u32(1);
    }
    return total_1;
}

fn ambient_split_sum_0( irradiance_0 : vec3<f32>,  reflection_0 : vec3<f32>,  albedo_7 : vec3<f32>,  metallic_5 : f32,  f0_7 : vec3<f32>,  energy_5 : vec2<f32>) -> vec3<f32>
{
    var specularAlbedo_0 : vec3<f32> = specular_albedo_0(energy_5, f0_7);
    return saturate(vec3<f32>(1.0f) - specularAlbedo_0) * vec3<f32>((1.0f - metallic_5)) * albedo_7 * irradiance_0 + reflection_0 * specularAlbedo_0;
}

fn hemisphere_irradiance_0( n_10 : vec3<f32>) -> vec3<f32>
{
    var _S79 : f32 = n_10.y;
    return mix(vec3<f32>(0.18000000715255737f, 0.15999999642372131f, 0.15000000596046448f) * vec3<f32>(0.69999998807907104f), mix(vec3<f32>(0.55000001192092896f, 0.62000000476837158f, 0.72000002861022949f), vec3<f32>(0.09000000357627869f, 0.15999999642372131f, 0.34999999403953552f), vec3<f32>(pow(clamp(_S79, 0.0f, 1.0f), 0.69999998807907104f))), vec3<f32>(clamp(_S79 * 0.5f + 0.5f, 0.0f, 1.0f)));
}

fn sky_gradient_0( dir_0 : vec3<f32>) -> vec3<f32>
{
    var _S80 : f32 = dir_0.y;
    if(_S80 >= 0.0f)
    {
        return mix(vec3<f32>(0.55000001192092896f, 0.62000000476837158f, 0.72000002861022949f), vec3<f32>(0.09000000357627869f, 0.15999999642372131f, 0.34999999403953552f), vec3<f32>(pow(clamp(_S80, 0.0f, 1.0f), 0.69999998807907104f)));
    }
    else
    {
        return mix(vec3<f32>(0.55000001192092896f, 0.62000000476837158f, 0.72000002861022949f), vec3<f32>(0.18000000715255737f, 0.15999999642372131f, 0.15000000596046448f), vec3<f32>(clamp(- _S80 * 3.0f, 0.0f, 1.0f)));
    }
}

fn analytic_ambient_0( n_11 : vec3<f32>,  v_4 : vec3<f32>,  albedo_8 : vec3<f32>,  metallic_6 : f32,  roughness_9 : f32,  f0_8 : vec3<f32>,  energy_6 : vec2<f32>) -> vec3<f32>
{
    var irradiance_1 : vec3<f32> = hemisphere_irradiance_0(n_11);
    return ambient_split_sum_0(irradiance_1, mix(sky_gradient_0(reflect((vec3<f32>(0) - v_4), n_11)), irradiance_1, vec3<f32>((roughness_9 * roughness_9))), albedo_8, metallic_6, f0_8, energy_6);
}

fn ibl_ambient_0( s_0 : SurfacePoint_0,  v_5 : vec3<f32>,  f0_9 : vec3<f32>,  surfaceEnergy_0 : vec2<f32>) -> vec3<f32>
{
    var ambient_0 : vec3<f32>;
    if((iblParams_0.enabled_maxmip_intensity_0.x) > 0.5f)
    {
        var _S81 : vec3<f32> = vec3<f32>(iblParams_0.enabled_maxmip_intensity_0.z);
        ambient_0 = ambient_split_sum_0((textureSampleLevel((irradianceMap_0), (iblSampler_0), (s_0.n_5), (0.0f))).xyz * _S81, (textureSampleLevel((prefilteredMap_0), (iblSampler_0), (reflect((vec3<f32>(0) - v_5), s_0.n_5)), (s_0.roughness_0 * iblParams_0.enabled_maxmip_intensity_0.y))).xyz * _S81, s_0.albedo_0, s_0.metallic_0, f0_9, (textureSampleLevel((brdfLut_0), (iblSampler_0), (vec2<f32>(max(dot(s_0.n_5, v_5), 0.0f), s_0.roughness_0)), (0.0f))).xy);
    }
    else
    {
        ambient_0 = analytic_ambient_0(s_0.n_5, v_5, s_0.albedo_0, s_0.metallic_0, s_0.roughness_0, f0_9, surfaceEnergy_0);
    }
    return vec3<f32>((frame_0.light_dir_ambient_0.w * s_0.occlusion_0)) * ambient_0;
}

fn hdr_safe_0( radiance_3 : vec3<f32>) -> vec3<f32>
{
    var _S82 : vec3<f32> = vec3<f32>(0.0f);
    return clamp(select(radiance_3, _S82, (((bitcast<vec3<u32>>((radiance_3))) & (vec3<u32>(u32(2147483647))))) > vec3<u32>(u32(2139095040))), _S82, vec3<f32>(65000.0f));
}

fn surface_lighting_0( sunDirect_0 : vec3<f32>,  sunVisibility_0 : f32,  shadowStrength_0 : f32,  punctual_0 : vec3<f32>,  ambient_1 : vec3<f32>,  emissive_1 : vec3<f32>) -> vec3<f32>
{
    return hdr_safe_0(sunDirect_0 * vec3<f32>(mix(1.0f, sunVisibility_0, shadowStrength_0)) + punctual_0 + ambient_1 + emissive_1);
}

fn shade_surface_0( s_1 : SurfacePoint_0,  fragCoord_1 : vec4<f32>) -> vec3<f32>
{
    var v_6 : vec3<f32> = normalize(frame_0.camera_position_0.xyz - s_1.position_4);
    var l_4 : vec3<f32> = normalize(frame_0.light_dir_ambient_0.xyz);
    var f0_10 : vec3<f32> = surface_f0_0(s_1.albedo_0, s_1.metallic_0);
    var energy_7 : vec2<f32> = ggx_energy_0(dot(s_1.n_5, v_6), s_1.roughness_0);
    return surface_lighting_0(brdf_direct_0(s_1.n_5, v_6, l_4, s_1.albedo_0, s_1.metallic_0, s_1.roughness_0, f0_10, frame_0.light_color_intensity_0.xyz * vec3<f32>(frame_0.light_color_intensity_0.w), energy_7), shadow_visibility_0(s_1.position_4, s_1.n_5, max(dot(s_1.n_5, l_4), 0.0f)), frame_0.shadow_params_0.z, punctual_lighting_0(s_1.position_4, s_1.n_5, v_6, s_1.albedo_0, s_1.metallic_0, s_1.roughness_0, f0_10, energy_7, fragCoord_1), ibl_ambient_0(s_1, v_6, f0_10, energy_7), s_1.emissive_0);
}

struct pixelOutput_0
{
    @location(0) output_1 : vec4<f32>,
};

struct pixelInput_1
{
    @location(0) worldNormal_1 : vec3<f32>,
    @location(1) uv_16 : vec2<f32>,
    @location(2) lightSpacePos_2 : vec4<f32>,
    @location(3) worldTangent_1 : vec4<f32>,
    @location(4) worldPosition_1 : vec3<f32>,
    @location(6) vertexColor_1 : vec4<f32>,
    @location(7) uv1_11 : vec2<f32>,
};

@fragment
fn fs_main( _S83 : pixelInput_1, @builtin(front_facing) isFrontFace_1 : bool, @builtin(position) svPosition_1 : vec4<f32>) -> pixelOutput_0
{
    var _S84 : VsOut_0 = VsOut_0( svPosition_1, _S83.worldNormal_1, _S83.uv_16, _S83.lightSpacePos_2, _S83.worldTangent_1, _S83.worldPosition_1, _S83.vertexColor_1, _S83.uv1_11 );
    var m_3 : MaterialSample_0 = sample_material_0(_S84, isFrontFace_1);
    if(alpha_masked_out_0(prim_alpha_mode_0(), m_3.alpha_3, prim_0.emissive_factor_0.w))
    {
        discard;
    }
    if((prim_0.material_flags_0.x) > 0.5f)
    {
        var _S85 : pixelOutput_0 = pixelOutput_0( vec4<f32>(m_3.surface_0.albedo_0, m_3.alpha_3) );
        return _S85;
    }
    var _S86 : pixelOutput_0 = pixelOutput_0( vec4<f32>(shade_surface_0(m_3.surface_0, svPosition_1), m_3.alpha_3) );
    return _S86;
}

struct GBufferTexels_0
{
     normalMetallic_0 : vec4<f32>,
     albedoOcclusion_0 : vec4<f32>,
     roughnessEmissive_0 : vec4<f32>,
};

fn gbuffer_pack_0( s_2 : SurfacePoint_0,  unlit_0 : bool) -> GBufferTexels_0
{
    var t_2 : GBufferTexels_0;
    var _S87 : f32;
    if(unlit_0)
    {
        _S87 = -1.0f;
    }
    else
    {
        _S87 = s_2.metallic_0;
    }
    t_2.normalMetallic_0 = vec4<f32>(s_2.n_5, _S87);
    t_2.albedoOcclusion_0 = vec4<f32>(s_2.albedo_0, s_2.occlusion_0);
    t_2.roughnessEmissive_0 = vec4<f32>(s_2.roughness_0, s_2.emissive_0);
    return t_2;
}

struct GBufferOut_0
{
    @location(0) normalMetallic_1 : vec4<f32>,
    @location(1) albedoOcclusion_1 : vec4<f32>,
    @location(2) roughnessEmissive_1 : vec4<f32>,
};

struct pixelInput_2
{
    @location(0) worldNormal_2 : vec3<f32>,
    @location(1) uv_17 : vec2<f32>,
    @location(2) lightSpacePos_3 : vec4<f32>,
    @location(3) worldTangent_2 : vec4<f32>,
    @location(4) worldPosition_2 : vec3<f32>,
    @location(6) vertexColor_2 : vec4<f32>,
    @location(7) uv1_12 : vec2<f32>,
};

@fragment
fn fs_gbuffer( _S88 : pixelInput_2, @builtin(front_facing) isFrontFace_2 : bool, @builtin(position) svPosition_2 : vec4<f32>) -> GBufferOut_0
{
    var _S89 : VsOut_0 = VsOut_0( svPosition_2, _S88.worldNormal_2, _S88.uv_17, _S88.lightSpacePos_3, _S88.worldTangent_2, _S88.worldPosition_2, _S88.vertexColor_2, _S88.uv1_12 );
    var m_4 : MaterialSample_0 = sample_material_0(_S89, isFrontFace_2);
    if(alpha_masked_out_0(prim_alpha_mode_0(), m_4.alpha_3, prim_0.emissive_factor_0.w))
    {
        discard;
    }
    var t_3 : GBufferTexels_0 = gbuffer_pack_0(m_4.surface_0, (prim_0.material_flags_0.x) > 0.5f);
    var g_1 : GBufferOut_0;
    g_1.normalMetallic_1 = t_3.normalMetallic_0;
    g_1.albedoOcclusion_1 = t_3.albedoOcclusion_0;
    g_1.roughnessEmissive_1 = t_3.roughnessEmissive_0;
    return g_1;
}

struct FullscreenVsOut_0
{
    @builtin(position) svPosition_3 : vec4<f32>,
    @location(0) uv_18 : vec2<f32>,
};

fn fullscreen_vs_0( vid_0 : u32) -> FullscreenVsOut_0
{
    var x_2 : f32 = f32(vid_0 / u32(2)) * 4.0f - 1.0f;
    var y_1 : f32 = f32(vid_0 % u32(2)) * 4.0f - 1.0f;
    var o_4 : FullscreenVsOut_0;
    o_4.svPosition_3 = vec4<f32>(x_2, y_1, 0.0f, 1.0f);
    o_4.uv_18 = vec2<f32>((x_2 + 1.0f) * 0.5f, 1.0f - (y_1 + 1.0f) * 0.5f);
    return o_4;
}

@vertex
fn vs_deferred_lighting(@builtin(vertex_index) vid_1 : u32) -> FullscreenVsOut_0
{
    return fullscreen_vs_0(vid_1);
}

fn gbuffer_unlit_0( normalMetallic_2 : vec4<f32>) -> bool
{
    return (normalMetallic_2.w) < -0.5f;
}

fn fullscreen_uv_to_ndc_0( uv_19 : vec2<f32>) -> vec2<f32>
{
    return vec2<f32>(uv_19.x * 2.0f - 1.0f, 1.0f - uv_19.y * 2.0f);
}

fn gbuffer_unpack_0( t_4 : GBufferTexels_0,  position_7 : vec3<f32>) -> SurfacePoint_0
{
    var s_3 : SurfacePoint_0;
    s_3.position_4 = position_7;
    s_3.n_5 = normalize_or_0(t_4.normalMetallic_0.xyz, vec3<f32>(0.0f, 1.0f, 0.0f));
    s_3.albedo_0 = t_4.albedoOcclusion_0.xyz;
    s_3.metallic_0 = t_4.normalMetallic_0.w;
    s_3.roughness_0 = t_4.roughnessEmissive_0.x;
    s_3.occlusion_0 = t_4.albedoOcclusion_0.w;
    s_3.emissive_0 = t_4.roughnessEmissive_0.yzw;
    return s_3;
}

struct pixelOutput_1
{
    @location(0) output_2 : vec4<f32>,
};

struct pixelInput_3
{
    @location(0) uv_20 : vec2<f32>,
};

@fragment
fn fs_deferred_lighting( _S90 : pixelInput_3, @builtin(position) svPosition_4 : vec4<f32>) -> pixelOutput_1
{
    var texel_4 : vec3<i32> = vec3<i32>(vec2<i32>(svPosition_4.xy), i32(0));
    var depth_1 : f32 = (textureLoad((gbufferDepth_0), ((texel_4)).xy, ((texel_4)).z));
    if(depth_1 >= 1.0f)
    {
        discard;
    }
    var t_5 : GBufferTexels_0;
    var _S91 : vec4<f32> = (textureLoad((gbufferNormal_0), ((texel_4)).xy, ((texel_4)).z));
    t_5.normalMetallic_0 = _S91;
    t_5.albedoOcclusion_0 = (textureLoad((gbufferAlbedo_0), ((texel_4)).xy, ((texel_4)).z));
    if(gbuffer_unlit_0(_S91))
    {
        var _S92 : pixelOutput_1 = pixelOutput_1( vec4<f32>(t_5.albedoOcclusion_0.xyz, 1.0f) );
        return _S92;
    }
    t_5.roughnessEmissive_0 = (textureLoad((gbufferMaterial_0), ((texel_4)).xy, ((texel_4)).z));
    var world_0 : vec4<f32> = (((vec4<f32>(fullscreen_uv_to_ndc_0(_S90.uv_20), depth_1, 1.0f)) * (mat4x4<f32>(deferredParams_0.inv_view_proj_0.data_0[i32(0)][i32(0)], deferredParams_0.inv_view_proj_0.data_0[i32(1)][i32(0)], deferredParams_0.inv_view_proj_0.data_0[i32(2)][i32(0)], deferredParams_0.inv_view_proj_0.data_0[i32(3)][i32(0)], deferredParams_0.inv_view_proj_0.data_0[i32(0)][i32(1)], deferredParams_0.inv_view_proj_0.data_0[i32(1)][i32(1)], deferredParams_0.inv_view_proj_0.data_0[i32(2)][i32(1)], deferredParams_0.inv_view_proj_0.data_0[i32(3)][i32(1)], deferredParams_0.inv_view_proj_0.data_0[i32(0)][i32(2)], deferredParams_0.inv_view_proj_0.data_0[i32(1)][i32(2)], deferredParams_0.inv_view_proj_0.data_0[i32(2)][i32(2)], deferredParams_0.inv_view_proj_0.data_0[i32(3)][i32(2)], deferredParams_0.inv_view_proj_0.data_0[i32(0)][i32(3)], deferredParams_0.inv_view_proj_0.data_0[i32(1)][i32(3)], deferredParams_0.inv_view_proj_0.data_0[i32(2)][i32(3)], deferredParams_0.inv_view_proj_0.data_0[i32(3)][i32(3)]))));
    var _S93 : pixelOutput_1 = pixelOutput_1( vec4<f32>(shade_surface_0(gbuffer_unpack_0(t_5, world_0.xyz / vec3<f32>(world_0.w)), svPosition_4), 1.0f) );
    return _S93;
}

