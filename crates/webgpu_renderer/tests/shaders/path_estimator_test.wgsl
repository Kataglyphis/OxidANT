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

fn VectorInputs_u32_1( this_1 : ptr<function, VectorInputs_0>) -> u32
{
    var word_1 : u32 = vectorInputs_0[(*this_1).at_0];
    (*this_1).at_0 = (*this_1).at_0 + u32(1);
    return word_1;
}

fn VectorInputs_f32_0( this_2 : ptr<function, VectorInputs_0>) -> f32
{
    var _S1 : u32 = VectorInputs_u32_1(&((*this_2)));
    return (bitcast<f32>((_S1)));
}

fn VectorInputs_f32_1( this_3 : ptr<function, VectorInputs_0>) -> f32
{
    var _S2 : u32 = VectorInputs_u32_1(&((*this_3)));
    return (bitcast<f32>((_S2)));
}

fn VectorInputs_f32x3_0( this_4 : ptr<function, VectorInputs_0>) -> vec3<f32>
{
    var x_0 : f32 = VectorInputs_f32_1(&((*this_4)));
    var y_0 : f32 = VectorInputs_f32_1(&((*this_4)));
    var z_0 : f32 = VectorInputs_f32_1(&((*this_4)));
    return vec3<f32>(x_0, y_0, z_0);
}

fn VectorInputs_f32x2_0( this_5 : ptr<function, VectorInputs_0>) -> vec2<f32>
{
    var x_1 : f32 = VectorInputs_f32_1(&((*this_5)));
    var y_1 : f32 = VectorInputs_f32_1(&((*this_5)));
    return vec2<f32>(x_1, y_1);
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

fn path_rng_seed_0( pixel_index_0 : u32,  stream_0 : u32) -> u32
{
    return pixel_index_0 + stream_0 * u32(2654435769);
}

fn VectorOutputs_put_0( this_6 : ptr<function, VectorOutputs_0>,  word_2 : u32)
{
    vectorOutputs_0[(*this_6).at_1] = word_2;
    (*this_6).at_1 = (*this_6).at_1 + u32(1);
    return;
}

fn VectorOutputs_put_1( this_7 : ptr<function, VectorOutputs_0>,  word_3 : u32)
{
    vectorOutputs_0[(*this_7).at_1] = word_3;
    (*this_7).at_1 = (*this_7).at_1 + u32(1);
    return;
}

fn rng_next_0( state_0 : ptr<function, u32>) -> f32
{
    var _S3 : u32 = (*state_0) * u32(747796405) + u32(1);
    (*state_0) = _S3;
    var word_4 : u32 = ((((_S3 >> ((((_S3 >> (u32(28)))) + u32(4))))) ^ (_S3))) * u32(277803737);
    return f32((((word_4 >> (u32(22)))) ^ (word_4))) / 4.294967296e+09f;
}

fn rng_next_1( state_1 : ptr<function, u32>) -> f32
{
    var _S4 : u32 = (*state_1) * u32(747796405) + u32(1);
    (*state_1) = _S4;
    var word_5 : u32 = ((((_S4 >> ((((_S4 >> (u32(28)))) + u32(4))))) ^ (_S4))) * u32(277803737);
    return f32((((word_5 >> (u32(22)))) ^ (word_5))) / 4.294967296e+09f;
}

fn VectorOutputs_put_2( this_8 : ptr<function, VectorOutputs_0>,  value_0 : f32)
{
    VectorOutputs_put_1(&((*this_8)), (bitcast<u32>((value_0))));
    return;
}

fn VectorOutputs_put_3( this_9 : ptr<function, VectorOutputs_0>,  value_1 : f32)
{
    VectorOutputs_put_1(&((*this_9)), (bitcast<u32>((value_1))));
    return;
}

fn russian_roulette_survives_0( bounce_0 : u32,  throughput_0 : ptr<function, vec3<f32>>,  rng_0 : ptr<function, u32>) -> bool
{
    if(bounce_0 < u32(3))
    {
        return true;
    }
    var survival_0 : f32 = clamp(max((*throughput_0).x, max((*throughput_0).y, (*throughput_0).z)), 0.05000000074505806f, 0.94999998807907104f);
    var _S5 : f32 = rng_next_1(&((*rng_0)));
    if(_S5 > survival_0)
    {
        return false;
    }
    (*throughput_0) = (*throughput_0) / vec3<f32>(survival_0);
    return true;
}

fn VectorOutputs_put_4( this_10 : ptr<function, VectorOutputs_0>,  value_2 : vec3<f32>)
{
    VectorOutputs_put_3(&((*this_10)), value_2.x);
    VectorOutputs_put_3(&((*this_10)), value_2.y);
    VectorOutputs_put_3(&((*this_10)), value_2.z);
    return;
}

fn orthonormal_basis_0( n_0 : vec3<f32>,  t_0 : ptr<function, vec3<f32>>,  b_0 : ptr<function, vec3<f32>>)
{
    var _S6 : f32 = n_0.z;
    var s_0 : f32;
    if(_S6 >= 0.0f)
    {
        s_0 = 1.0f;
    }
    else
    {
        s_0 = -1.0f;
    }
    var a_0 : f32 = -1.0f / (s_0 + _S6);
    var _S7 : f32 = n_0.x;
    var _S8 : f32 = n_0.y;
    var c_0 : f32 = _S7 * _S8 * a_0;
    (*t_0) = vec3<f32>(1.0f + s_0 * _S7 * _S7 * a_0, s_0 * c_0, - s_0 * _S7);
    (*b_0) = vec3<f32>(c_0, s_0 + _S8 * _S8 * a_0, - _S8);
    return;
}

fn sample_ggx_visible_normal_0( v_0 : vec3<f32>,  alpha_0 : f32,  u_0 : vec2<f32>) -> vec3<f32>
{
    var vh_0 : vec3<f32> = normalize(vec3<f32>(alpha_0 * v_0.x, alpha_0 * v_0.y, v_0.z));
    var _S9 : f32 = vh_0.x;
    var _S10 : f32 = vh_0.y;
    var length_squared_0 : f32 = _S9 * _S9 + _S10 * _S10;
    var t1_0 : vec3<f32>;
    if(length_squared_0 > 0.0f)
    {
        t1_0 = vec3<f32>(- _S10, _S9, 0.0f) / vec3<f32>(sqrt(length_squared_0));
    }
    else
    {
        t1_0 = vec3<f32>(1.0f, 0.0f, 0.0f);
    }
    var r_0 : f32 = sqrt(u_0.x);
    var phi_0 : f32 = 6.28318548202514648f * u_0.y;
    var p1_0 : f32 = r_0 * cos(phi_0);
    var s_1 : f32 = 0.5f * (1.0f + vh_0.z);
    var _S11 : f32 = 1.0f - p1_0 * p1_0;
    var p2_0 : f32 = (1.0f - s_1) * sqrt(max(_S11, 0.0f)) + s_1 * (r_0 * sin(phi_0));
    var nh_0 : vec3<f32> = vec3<f32>(p1_0) * t1_0 + vec3<f32>(p2_0) * cross(vh_0, t1_0) + vec3<f32>(sqrt(max(_S11 - p2_0 * p2_0, 0.0f))) * vh_0;
    return normalize(vec3<f32>(alpha_0 * nh_0.x, alpha_0 * nh_0.y, max(nh_0.z, 9.99999997475242708e-07f)));
}

fn ggx_visible_normal_pdf_0( n_dot_v_0 : f32,  n_dot_h_0 : f32,  alpha_1 : f32) -> f32
{
    var a2_0 : f32 = alpha_1 * alpha_1;
    var d_0 : f32 = n_dot_h_0 * n_dot_h_0 * (a2_0 - 1.0f) + 1.0f;
    return a2_0 / (3.14159274101257324f * d_0 * d_0) * (2.0f * n_dot_v_0 / (n_dot_v_0 + sqrt(a2_0 + (1.0f - a2_0) * n_dot_v_0 * n_dot_v_0))) / (4.0f * n_dot_v_0);
}

fn fresnel_schlick_0( cos_theta_0 : f32,  f0_0 : vec3<f32>) -> vec3<f32>
{
    return f0_0 + (vec3<f32>(1.0f) - f0_0) * vec3<f32>(pow(clamp(1.0f - cos_theta_0, 0.0f, 1.0f), 5.0f));
}

fn specular_lobe_probability_0( albedo_0 : vec3<f32>,  metallic_0 : f32,  f0_1 : vec3<f32>,  n_dot_v_1 : f32) -> f32
{
    const LUMA_0 : vec3<f32> = vec3<f32>(0.2125999927520752f, 0.71520000696182251f, 0.07220000028610229f);
    var fresnel_0 : vec3<f32> = fresnel_schlick_0(n_dot_v_1, f0_1);
    var specular_0 : f32 = dot(fresnel_0, LUMA_0);
    return clamp(specular_0 / max(specular_0 + dot((vec3<f32>(1.0f) - fresnel_0) * vec3<f32>((1.0f - metallic_0)) * albedo_0, LUMA_0), 9.99999997475242708e-07f), 0.10000000149011612f, 1.0f);
}

fn smith_k_sampled_0( roughness_0 : f32) -> f32
{
    var _S12 : f32 = max(roughness_0, 0.04500000178813934f);
    return _S12 * _S12 / 2.0f;
}

fn ggx_energy_coordinates_0( n_dot_v_2 : f32,  roughness_1 : f32) -> vec2<f32>
{
    var low_0 : vec2<f32> = vec2<f32>(pow(0.00009999999747379f, 0.3333333432674408f), sqrt(0.04500000178813934f));
    return (vec2<f32>(pow(clamp(n_dot_v_2, 0.00009999999747379f, 1.0f), 0.3333333432674408f), sqrt(clamp(roughness_1, 0.04500000178813934f, 1.0f))) - low_0) / (vec2<f32>(1.0f) - low_0);
}

fn ggx_energy_basis_0( x_2 : f32) -> f32
{
    var t_1 : f32 = abs(x_2);
    var inner_0 : f32 = 0.66666668653488159f - t_1 * t_1 + 0.5f * t_1 * t_1 * t_1;
    var _S13 : f32 = 2.0f - t_1;
    var outer_0 : f32 = _S13 * _S13 * _S13 / 6.0f;
    var _S14 : f32;
    if(t_1 < 1.0f)
    {
        _S14 = inner_0;
    }
    else
    {
        if(t_1 < 2.0f)
        {
            _S14 = outer_0;
        }
        else
        {
            _S14 = 0.0f;
        }
    }
    return _S14;
}

fn ggx_energy_fit_0( t_2 : vec2<f32>) -> vec2<f32>
{
    var x_3 : vec2<f32> = t_2 * vec2<f32>(11.0f) + vec2<f32>(1.0f);
    var _S15 : f32 = x_3.y;
    var _S16 : f32 = x_3.x;
    var _S17 : vec2<f32> = vec2<f32>(ggx_energy_basis_0(_S15));
    var _S18 : vec2<f32> = vec2<f32>(ggx_energy_basis_0(_S15 - 1.0f));
    var _S19 : vec2<f32> = vec2<f32>(ggx_energy_basis_0(_S15 - 2.0f));
    var _S20 : vec2<f32> = vec2<f32>(ggx_energy_basis_0(_S15 - 3.0f));
    var _S21 : vec2<f32> = vec2<f32>(ggx_energy_basis_0(_S15 - 4.0f));
    var _S22 : vec2<f32> = vec2<f32>(ggx_energy_basis_0(_S15 - 5.0f));
    var _S23 : vec2<f32> = vec2<f32>(ggx_energy_basis_0(_S15 - 6.0f));
    var _S24 : vec2<f32> = vec2<f32>(ggx_energy_basis_0(_S15 - 7.0f));
    var _S25 : vec2<f32> = vec2<f32>(ggx_energy_basis_0(_S15 - 8.0f));
    var _S26 : vec2<f32> = vec2<f32>(ggx_energy_basis_0(_S15 - 9.0f));
    var _S27 : vec2<f32> = vec2<f32>(ggx_energy_basis_0(_S15 - 10.0f));
    var _S28 : vec2<f32> = vec2<f32>(ggx_energy_basis_0(_S15 - 11.0f));
    var _S29 : vec2<f32> = vec2<f32>(ggx_energy_basis_0(_S15 - 12.0f));
    var _S30 : vec2<f32> = vec2<f32>(ggx_energy_basis_0(_S15 - 13.0f));
    return vec2<f32>(ggx_energy_basis_0(_S16)) * (vec2<f32>(0.03755360096693039f, 3.05165600776672363f) * _S17 + vec2<f32>(0.03963959962129593f, 0.65487277507781982f) * _S18 + vec2<f32>(0.07296779751777649f, 0.51261270046234131f) * _S19 + vec2<f32>(0.17118540406227112f, 0.62788808345794678f) * _S20 + vec2<f32>(0.30981320142745972f, 0.50604218244552612f) * _S21 + vec2<f32>(0.44191190600395203f, 0.40881091356277466f) * _S22 + vec2<f32>(0.55946820974349976f, 0.28783369064331055f) * _S23 + vec2<f32>(0.65159928798675537f, 0.20042069256305695f) * _S24 + vec2<f32>(0.70950692892074585f, 0.12844389677047729f) * _S25 + vec2<f32>(0.73192507028579712f, 0.08202389627695084f) * _S26 + vec2<f32>(0.7161983847618103f, 0.05005419999361038f) * _S27 + vec2<f32>(0.66943138837814331f, 0.03112700022757053f) * _S28 + vec2<f32>(0.59648841619491577f, 0.01862730085849762f) * _S29 + vec2<f32>(0.50741302967071533f, 0.01281030010432005f) * _S30) + vec2<f32>(ggx_energy_basis_0(_S16 - 1.0f)) * (vec2<f32>(-0.00930839963257313f, -0.05096510052680969f) * _S17 + vec2<f32>(0.03267690166831017f, 0.88316977024078369f) * _S18 + vec2<f32>(0.11045809835195541f, 0.81648111343383789f) * _S19 + vec2<f32>(0.20676289498806f, 0.65160852670669556f) * _S20 + vec2<f32>(0.32054820656776428f, 0.53221142292022705f) * _S21 + vec2<f32>(0.4455254077911377f, 0.40326109528541565f) * _S22 + vec2<f32>(0.56171208620071411f, 0.2928445041179657f) * _S23 + vec2<f32>(0.65319478511810303f, 0.19869279861450195f) * _S24 + vec2<f32>(0.71095079183578491f, 0.12985430657863617f) * _S25 + vec2<f32>(0.7316511869430542f, 0.08150120079517365f) * _S26 + vec2<f32>(0.71713531017303467f, 0.05052080005407333f) * _S27 + vec2<f32>(0.66998302936553955f, 0.03087439946830273f) * _S28 + vec2<f32>(0.59695768356323242f, 0.01905369944870472f) * _S29 + vec2<f32>(0.50688481330871582f, 0.01136360038071871f) * _S30) + vec2<f32>(ggx_energy_basis_0(_S16 - 2.0f)) * (vec2<f32>(0.010585299693048f, 0.99188041687011719f) * _S17 + vec2<f32>(0.00207319995388389f, 0.40556570887565613f) * _S18 + vec2<f32>(0.05862849950790405f, 0.55259382724761963f) * _S19 + vec2<f32>(0.16572730243206024f, 0.58792608976364136f) * _S20 + vec2<f32>(0.29784908890724182f, 0.50322818756103516f) * _S21 + vec2<f32>(0.4318917989730835f, 0.39728268980979919f) * _S22 + vec2<f32>(0.55288940668106079f, 0.2878929078578949f) * _S23 + vec2<f32>(0.64753270149230957f, 0.19779230654239655f) * _S24 + vec2<f32>(0.70724570751190186f, 0.12865540385246277f) * _S25 + vec2<f32>(0.72979277372360229f, 0.08128180354833603f) * _S26 + vec2<f32>(0.71553927659988403f, 0.05013220012187958f) * _S27 + vec2<f32>(0.66890257596969604f, 0.03081009909510612f) * _S28 + vec2<f32>(0.59605568647384644f, 0.01881019957363605f) * _S29 + vec2<f32>(0.50637918710708618f, 0.01177479978650808f) * _S30) + vec2<f32>(ggx_energy_basis_0(_S16 - 3.0f)) * (vec2<f32>(0.04574630036950111f, 1.02595400810241699f) * _S17 + vec2<f32>(0.03743439912796021f, 0.88119810819625854f) * _S18 + vec2<f32>(0.04426249861717224f, 0.52496278285980225f) * _S19 + vec2<f32>(0.11310549825429916f, 0.46600010991096497f) * _S20 + vec2<f32>(0.23485749959945679f, 0.4375700056552887f) * _S21 + vec2<f32>(0.37963598966598511f, 0.36149460077285767f) * _S22 + vec2<f32>(0.51556599140167236f, 0.27323558926582336f) * _S23 + vec2<f32>(0.622580885887146f, 0.18986819684505463f) * _S24 + vec2<f32>(0.69136160612106323f, 0.12518900632858276f) * _S25 + vec2<f32>(0.71929609775543213f, 0.07907170057296753f) * _S26 + vec2<f32>(0.70917361974716187f, 0.04900079965591431f) * _S27 + vec2<f32>(0.66434282064437866f, 0.02995469979941845f) * _S28 + vec2<f32>(0.59224581718444824f, 0.01837039925158024f) * _S29 + vec2<f32>(0.5022997260093689f, 0.0110470000654459f) * _S30) + vec2<f32>(ggx_energy_basis_0(_S16 - 4.0f)) * (vec2<f32>(0.1206125020980835f, 0.77379542589187622f) * _S17 + vec2<f32>(0.12222660332918167f, 0.83616697788238525f) * _S18 + vec2<f32>(0.11106769740581512f, 0.74211102724075317f) * _S19 + vec2<f32>(0.12635010480880737f, 0.50614690780639648f) * _S20 + vec2<f32>(0.20402370393276215f, 0.39524701237678528f) * _S21 + vec2<f32>(0.32609659433364868f, 0.32315808534622192f) * _S22 + vec2<f32>(0.46191799640655518f, 0.24740250408649445f) * _S23 + vec2<f32>(0.57957160472869873f, 0.17592360079288483f) * _S24 + vec2<f32>(0.66025441884994507f, 0.11711639910936356f) * _S25 + vec2<f32>(0.69881141185760498f, 0.07462230324745178f) * _S26 + vec2<f32>(0.69453942775726318f, 0.04620540142059326f) * _S27 + vec2<f32>(0.65352827310562134f, 0.02824320085346699f) * _S28 + vec2<f32>(0.582955002784729f, 0.01713629998266697f) * _S29 + vec2<f32>(0.49382159113883972f, 0.01042309962213039f) * _S30) + vec2<f32>(ggx_energy_basis_0(_S16 - 5.0f)) * (vec2<f32>(0.26457729935646057f, 0.79617220163345337f) * _S17 + vec2<f32>(0.25447279214859009f, 0.71848297119140625f) * _S18 + vec2<f32>(0.24372699856758118f, 0.67536389827728271f) * _S19 + vec2<f32>(0.22911170125007629f, 0.58072292804718018f) * _S20 + vec2<f32>(0.24631629884243011f, 0.4128555953502655f) * _S21 + vec2<f32>(0.32152768969535828f, 0.3022313117980957f) * _S22 + vec2<f32>(0.42885839939117432f, 0.22238540649414062f) * _S23 + vec2<f32>(0.53810352087020874f, 0.157231405377388f) * _S24 + vec2<f32>(0.6227080225944519f, 0.10510919988155365f) * _S25 + vec2<f32>(0.66763401031494141f, 0.0672312006354332f) * _S26 + vec2<f32>(0.67200499773025513f, 0.04161379858851433f) * _S27 + vec2<f32>(0.63588422536849976f, 0.02530450001358986f) * _S28 + vec2<f32>(0.56762188673019409f, 0.01518450025469065f) * _S29 + vec2<f32>(0.47794249653816223f, 0.00909390021115541f) * _S30) + vec2<f32>(ggx_energy_basis_0(_S16 - 6.0f)) * (vec2<f32>(0.43767940998077393f, 0.52778768539428711f) * _S17 + vec2<f32>(0.43497830629348755f, 0.55565857887268066f) * _S18 + vec2<f32>(0.42141589522361755f, 0.53836488723754883f) * _S19 + vec2<f32>(0.4008432924747467f, 0.49193951487541199f) * _S20 + vec2<f32>(0.37870550155639648f, 0.40763860940933228f) * _S21 + vec2<f32>(0.38813981413841248f, 0.28910300135612488f) * _S22 + vec2<f32>(0.44539439678192139f, 0.20043960213661194f) * _S23 + vec2<f32>(0.52257758378982544f, 0.13623130321502686f) * _S24 + vec2<f32>(0.59163397550582886f, 0.08995489776134491f) * _S25 + vec2<f32>(0.63798898458480835f, 0.05706769973039627f) * _S26 + vec2<f32>(0.64467430114746094f, 0.03520869836211205f) * _S27 + vec2<f32>(0.61179417371749878f, 0.021161999553442f) * _S28 + vec2<f32>(0.5443912148475647f, 0.01255400013178587f) * _S29 + vec2<f32>(0.45556539297103882f, 0.00719430018216372f) * _S30) + vec2<f32>(ggx_energy_basis_0(_S16 - 7.0f)) * (vec2<f32>(0.63681560754776001f, 0.37943580746650696f) * _S17 + vec2<f32>(0.63195270299911499f, 0.36018449068069458f) * _S18 + vec2<f32>(0.62122827768325806f, 0.35363009572029114f) * _S19 + vec2<f32>(0.59902858734130859f, 0.34027791023254395f) * _S20 + vec2<f32>(0.56643378734588623f, 0.30496558547019958f) * _S21 + vec2<f32>(0.5314096212387085f, 0.24438419938087463f) * _S22 + vec2<f32>(0.52357661724090576f, 0.16980840265750885f) * _S23 + vec2<f32>(0.5474010705947876f, 0.11221110075712204f) * _S24 + vec2<f32>(0.58856910467147827f, 0.07183969765901566f) * _S25 + vec2<f32>(0.61636549234390259f, 0.04495130106806755f) * _S26 + vec2<f32>(0.61752068996429443f, 0.02730320021510124f) * _S27 + vec2<f32>(0.58241552114486694f, 0.01623369939625263f) * _S28 + vec2<f32>(0.51425069570541382f, 0.00938300043344498f) * _S29 + vec2<f32>(0.42317068576812744f, 0.00540499994531274f) * _S30) + vec2<f32>(ggx_energy_basis_0(_S16 - 8.0f)) * (vec2<f32>(0.81267541646957397f, 0.17737489938735962f) * _S17 + vec2<f32>(0.81089460849761963f, 0.18592619895935059f) * _S18 + vec2<f32>(0.80203372240066528f, 0.18450069427490234f) * _S19 + vec2<f32>(0.78451091051101685f, 0.17923450469970703f) * _S20 + vec2<f32>(0.75236481428146362f, 0.17097190022468567f) * _S21 + vec2<f32>(0.70702672004699707f, 0.15113990008831024f) * _S22 + vec2<f32>(0.65327489376068115f, 0.11831910163164139f) * _S23 + vec2<f32>(0.62291878461837769f, 0.08054299652576447f) * _S24 + vec2<f32>(0.61463671922683716f, 0.05135539919137955f) * _S25 + vec2<f32>(0.61291992664337158f, 0.03151309862732887f) * _S26 + vec2<f32>(0.59584552049636841f, 0.01889899931848049f) * _S27 + vec2<f32>(0.5510483980178833f, 0.01100130006670952f) * _S28 + vec2<f32>(0.4772191047668457f, 0.00626480020582676f) * _S29 + vec2<f32>(0.38401719927787781f, 0.0033825000282377f) * _S30) + vec2<f32>(ggx_energy_basis_0(_S16 - 9.0f)) * (vec2<f32>(0.93769299983978271f, 0.06798700243234634f) * _S17 + vec2<f32>(0.93442130088806152f, 0.06284449994564056f) * _S18 + vec2<f32>(0.92932981252670288f, 0.06249019876122475f) * _S19 + vec2<f32>(0.91649138927459717f, 0.06285469979047775f) * _S20 + vec2<f32>(0.89388298988342285f, 0.06178810074925423f) * _S21 + vec2<f32>(0.85349130630493164f, 0.06024239957332611f) * _S22 + vec2<f32>(0.79553717374801636f, 0.05434289947152138f) * _S23 + vec2<f32>(0.72840338945388794f, 0.04295980185270309f) * _S24 + vec2<f32>(0.67323261499404907f, 0.02926450036466122f) * _S25 + vec2<f32>(0.63030177354812622f, 0.01836870051920414f) * _S26 + vec2<f32>(0.58399122953414917f, 0.01094940025359392f) * _S27 + vec2<f32>(0.5202292799949646f, 0.00631939992308617f) * _S28 + vec2<f32>(0.43539279699325562f, 0.0035077000502497f) * _S29 + vec2<f32>(0.33793559670448303f, 0.00192960002459586f) * _S30) + vec2<f32>(ggx_energy_basis_0(_S16 - 10.0f)) * (vec2<f32>(0.99163192510604858f, 0.00484689977020025f) * _S17 + vec2<f32>(0.99116522073745728f, 0.00796679966151714f) * _S18 + vec2<f32>(0.98792862892150879f, 0.00806930009275675f) * _S19 + vec2<f32>(0.98114508390426636f, 0.0078320000320673f) * _S20 + vec2<f32>(0.96755552291870117f, 0.00866509974002838f) * _S21 + vec2<f32>(0.94221091270446777f, 0.00961260031908751f) * _S22 + vec2<f32>(0.89904522895812988f, 0.0111905001103878f) * _S23 + vec2<f32>(0.83300900459289551f, 0.01175749953836203f) * _S24 + vec2<f32>(0.75132989883422852f, 0.01031319983303547f) * _S25 + vec2<f32>(0.66817468404769897f, 0.00739389983937144f) * _S26 + vec2<f32>(0.58449500799179077f, 0.00469669979065657f) * _S27 + vec2<f32>(0.49239379167556763f, 0.00274799996986985f) * _S28 + vec2<f32>(0.39069679379463196f, 0.00153979996684939f) * _S29 + vec2<f32>(0.28739228844642639f, 0.00080069998512045f) * _S30) + vec2<f32>(ggx_energy_basis_0(_S16 - 11.0f)) * (vec2<f32>(1.00030839443206787f, 0.00132489996030927f) * _S17 + vec2<f32>(1.00059950351715088f, -0.00104150001425296f) * _S18 + vec2<f32>(0.99918842315673828f, -0.00109509995672852f) * _S19 + vec2<f32>(0.99662727117538452f, -0.00078689999645576f) * _S20 + vec2<f32>(0.99067902565002441f, -0.00099460000637919f) * _S21 + vec2<f32>(0.9789891242980957f, -0.00078649999340996f) * _S22 + vec2<f32>(0.95506691932678223f, -0.00063089997274801f) * _S23 + vec2<f32>(0.90853261947631836f, -0.00004849999822909f) * _S24 + vec2<f32>(0.83005660772323608f, 0.00064739998197183f) * _S25 + vec2<f32>(0.72199207544326782f, 0.00106369995046407f) * _S26 + vec2<f32>(0.59814202785491943f, 0.00094429997261614f) * _S27 + vec2<f32>(0.46943360567092896f, 0.0006571999983862f) * _S28 + vec2<f32>(0.34491020441055298f, 0.00038939999649301f) * _S29 + vec2<f32>(0.23603999614715576f, 0.00023839999630582f) * _S30) + vec2<f32>(ggx_energy_basis_0(_S16 - 12.0f)) * (vec2<f32>(0.99982082843780518f, -0.00226259999908507f) * _S17 + vec2<f32>(0.99921691417694092f, 0.00083259999519214f) * _S18 + vec2<f32>(0.99951702356338501f, 0.00091000000247732f) * _S19 + vec2<f32>(0.99938511848449707f, 0.00050560000818223f) * _S20 + vec2<f32>(0.9986388087272644f, 0.00080109998816624f) * _S21 + vec2<f32>(0.99591022729873657f, 0.00060789997223765f) * _S22 + vec2<f32>(0.98590642213821411f, 0.00062870001420379f) * _S23 + vec2<f32>(0.95896798372268677f, 0.00030819998937659f) * _S24 + vec2<f32>(0.89731162786483765f, -0.00005109999983688f) * _S25 + vec2<f32>(0.78470510244369507f, -0.00027940000290982f) * _S26 + vec2<f32>(0.62571001052856445f, -0.00019700000120793f) * _S27 + vec2<f32>(0.45251798629760742f, -0.00010209999891231f) * _S28 + vec2<f32>(0.29973480105400085f, -0.00003329999890411f) * _S29 + vec2<f32>(0.18639770150184631f, -0.000034600001527f) * _S30) + vec2<f32>(ggx_energy_basis_0(_S16 - 13.0f)) * (vec2<f32>(1.00496220588684082f, 0.00913080014288425f) * _S17 + vec2<f32>(1.00138688087463379f, -0.00241709989495575f) * _S18 + vec2<f32>(1.0036776065826416f, -0.0027135000564158f) * _S19 + vec2<f32>(1.00481629371643066f, -0.00120069994591177f) * _S20 + vec2<f32>(1.008644700050354f, -0.00232520001009107f) * _S21 + vec2<f32>(1.01138985157012939f, -0.00167589995544404f) * _S22 + vec2<f32>(1.01234376430511475f, -0.00196720007807016f) * _S23 + vec2<f32>(0.99929207563400269f, -0.00119440001435578f) * _S24 + vec2<f32>(0.95524948835372925f, -0.00035449999268167f) * _S25 + vec2<f32>(0.85058838129043579f, 0.00027970000519417f) * _S26 + vec2<f32>(0.6671786904335022f, 0.00011629999789875f) * _S27 + vec2<f32>(0.44304949045181274f, 0.00001530000008643f) * _S28 + vec2<f32>(0.25579911470413208f, -0.00005640000017593f) * _S29 + vec2<f32>(0.14276920258998871f, 0.00004659999831347f) * _S30);
}

fn ggx_energy_0( n_dot_v_3 : f32,  roughness_2 : f32) -> vec2<f32>
{
    return ggx_energy_fit_0(ggx_energy_coordinates_0(n_dot_v_3, roughness_2));
}

fn distribution_ggx_0( n_dot_h_1 : f32,  roughness_3 : f32) -> f32
{
    var _S31 : f32 = max(roughness_3, 0.04500000178813934f);
    var a_1 : f32 = _S31 * _S31;
    var a2_1 : f32 = a_1 * a_1;
    var denom_0 : f32 = n_dot_h_1 * n_dot_h_1 * (a2_1 - 1.0f) + 1.0f;
    return a2_1 / (3.14159274101257324f * max(denom_0 * denom_0, 9.99999968265522539e-21f));
}

fn geometry_smith_k_0( n_dot_v_4 : f32,  n_dot_l_0 : f32,  k_0 : f32) -> f32
{
    var _S32 : f32 = 1.0f - k_0;
    return n_dot_v_4 / max(n_dot_v_4 * _S32 + k_0, 9.99999997475242708e-07f) * (n_dot_l_0 / max(n_dot_l_0 * _S32 + k_0, 9.99999997475242708e-07f));
}

fn ggx_single_scatter_0( n_dot_v_5 : f32,  n_dot_l_1 : f32,  n_dot_h_2 : f32,  h_dot_v_0 : f32,  roughness_4 : f32,  f0_2 : vec3<f32>,  k_1 : f32) -> vec3<f32>
{
    return vec3<f32>((distribution_ggx_0(n_dot_h_2, roughness_4) * geometry_smith_k_0(n_dot_v_5, n_dot_l_1, k_1))) * fresnel_schlick_0(h_dot_v_0, f0_2) / vec3<f32>(max(4.0f * n_dot_v_5 * n_dot_l_1, 9.99999997475242708e-07f));
}

fn multiscatter_gain_0( energy_0 : vec2<f32>,  f0_3 : vec3<f32>) -> vec3<f32>
{
    return vec3<f32>(1.0f) + f0_3 * vec3<f32>((1.0f / max(energy_0.x + energy_0.y, 0.00100000004749745f) - 1.0f));
}

fn specular_albedo_0( energy_1 : vec2<f32>,  f0_4 : vec3<f32>) -> vec3<f32>
{
    return multiscatter_gain_0(energy_1, f0_4) * (f0_4 * vec3<f32>(energy_1.x) + vec3<f32>(energy_1.y));
}

fn lambert_diffuse_0( albedo_1 : vec3<f32>) -> vec3<f32>
{
    return albedo_1 / vec3<f32>(3.14159274101257324f);
}

fn brdf_smith_k_0( n_1 : vec3<f32>,  v_1 : vec3<f32>,  l_0 : vec3<f32>,  albedo_2 : vec3<f32>,  metallic_1 : f32,  roughness_5 : f32,  f0_5 : vec3<f32>,  radiance_0 : vec3<f32>,  k_2 : f32) -> vec3<f32>
{
    var h_0 : vec3<f32> = normalize(l_0 + v_1);
    var _S33 : f32 = max(dot(n_1, l_0), 0.0f);
    var _S34 : f32 = max(dot(n_1, v_1), 0.00009999999747379f);
    var _S35 : f32 = max(roughness_5, 0.04500000178813934f);
    var energy_2 : vec2<f32> = ggx_energy_0(_S34, _S35);
    return (saturate(vec3<f32>(1.0f) - specular_albedo_0(energy_2, f0_5)) * vec3<f32>((1.0f - metallic_1)) * lambert_diffuse_0(albedo_2) + multiscatter_gain_0(energy_2, f0_5) * ggx_single_scatter_0(_S34, _S33, max(dot(n_1, h_0), 0.0f), max(dot(h_0, v_1), 0.0f), _S35, f0_5, k_2)) * radiance_0 * vec3<f32>(_S33);
}

fn brdf_sampled_0( n_2 : vec3<f32>,  v_2 : vec3<f32>,  l_1 : vec3<f32>,  albedo_3 : vec3<f32>,  metallic_2 : f32,  roughness_6 : f32,  f0_6 : vec3<f32>) -> vec3<f32>
{
    return brdf_smith_k_0(n_2, v_2, l_1, albedo_3, metallic_2, roughness_6, f0_6, vec3<f32>(1.0f), smith_k_sampled_0(roughness_6));
}

struct Bounce_0
{
     direction_0 : vec3<f32>,
     weight_0 : vec3<f32>,
};

fn sample_bounce_0( n_3 : vec3<f32>,  face_normal_0 : vec3<f32>,  v_3 : vec3<f32>,  albedo_4 : vec3<f32>,  metallic_3 : f32,  roughness_7 : f32,  rng_1 : ptr<function, u32>) -> Bounce_0
{
    var f0_7 : vec3<f32> = mix(vec3<f32>(0.03999999910593033f), albedo_4, vec3<f32>(metallic_3));
    var _S36 : f32 = max(dot(n_3, v_3), 0.00009999999747379f);
    var specular_1 : f32 = specular_lobe_probability_0(albedo_4, metallic_3, f0_7, _S36);
    var alpha_2 : f32 = roughness_7 * roughness_7;
    var t_3 : vec3<f32>;
    var b_1 : vec3<f32>;
    orthonormal_basis_0(n_3, &(t_3), &(b_1));
    var _S37 : f32 = rng_next_0(&((*rng_1)));
    var direction_1 : vec3<f32>;
    if(_S37 < specular_1)
    {
        var local_0 : vec3<f32> = vec3<f32>(dot(v_3, t_3), dot(v_3, b_1), dot(v_3, n_3));
        var _S38 : f32 = rng_next_0(&((*rng_1)));
        var _S39 : f32 = rng_next_0(&((*rng_1)));
        var h_1 : vec3<f32> = sample_ggx_visible_normal_0(local_0, alpha_2, vec2<f32>(_S38, _S39));
        direction_1 = reflect((vec3<f32>(0) - v_3), vec3<f32>(h_1.x) * t_3 + vec3<f32>(h_1.y) * b_1 + vec3<f32>(h_1.z) * n_3);
    }
    else
    {
        var _S40 : f32 = rng_next_0(&((*rng_1)));
        var theta_0 : f32 = 6.28318548202514648f * _S40;
        var _S41 : f32 = rng_next_0(&((*rng_1)));
        var z_1 : f32 = 2.0f * _S41 - 1.0f;
        var r_1 : f32 = sqrt(max(1.0f - z_1 * z_1, 0.0f));
        var direction_2 : vec3<f32> = n_3 + vec3<f32>(r_1 * cos(theta_0), r_1 * sin(theta_0), z_1);
        if((dot(direction_2, direction_2)) < 9.99999993922529029e-09f)
        {
            direction_1 = n_3;
        }
        else
        {
            direction_1 = direction_2;
        }
    }
    var o_0 : Bounce_0;
    var _S42 : vec3<f32> = normalize(direction_1);
    o_0.direction_0 = _S42;
    o_0.weight_0 = vec3<f32>(0.0f);
    var n_dot_l_2 : f32 = dot(n_3, _S42);
    var _S43 : bool;
    if(n_dot_l_2 <= 0.0f)
    {
        _S43 = true;
    }
    else
    {
        _S43 = (dot(face_normal_0, o_0.direction_0)) <= 0.0f;
    }
    if(_S43)
    {
        return o_0;
    }
    o_0.weight_0 = brdf_sampled_0(n_3, v_3, o_0.direction_0, albedo_4, metallic_3, roughness_7, f0_7) / vec3<f32>(max(specular_1 * ggx_visible_normal_pdf_0(_S36, max(dot(n_3, normalize(v_3 + o_0.direction_0)), 0.0f), alpha_2) + (1.0f - specular_1) * n_dot_l_2 / 3.14159274101257324f, 9.99999997475242708e-07f));
    return o_0;
}

@compute
@workgroup_size(1, 1, 1)
fn path_estimator_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S44 : u32 = tid_0.x;
    var inputs_1 : VectorInputs_0 = vector_inputs_0(_S44);
    var pixel_index_1 : u32 = VectorInputs_u32_0(&(inputs_1));
    var stream_1 : u32 = VectorInputs_u32_0(&(inputs_1));
    var rng_2 : u32 = VectorInputs_u32_0(&(inputs_1));
    var bounce_1 : u32 = VectorInputs_u32_0(&(inputs_1));
    var throughput_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var n_4 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var face_normal_1 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var v_4 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var albedo_5 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var metallic_4 : f32 = VectorInputs_f32_0(&(inputs_1));
    var roughness_8 : f32 = VectorInputs_f32_0(&(inputs_1));
    var f0_8 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var local_v_0 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var alpha_3 : f32 = VectorInputs_f32_0(&(inputs_1));
    var u_1 : vec2<f32> = VectorInputs_f32x2_0(&(inputs_1));
    var n_dot_v_6 : f32 = VectorInputs_f32_0(&(inputs_1));
    var n_dot_h_3 : f32 = VectorInputs_f32_0(&(inputs_1));
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S44);
    VectorOutputs_put_0(&(outputs_1), path_rng_seed_0(pixel_index_1, stream_1));
    var drawn_rng_0 : u32 = rng_2;
    var drawn_0 : f32 = rng_next_0(&(drawn_rng_0));
    VectorOutputs_put_2(&(outputs_1), drawn_0);
    VectorOutputs_put_0(&(outputs_1), drawn_rng_0);
    var roulette_rng_0 : u32 = rng_2;
    var kept_0 : vec3<f32> = throughput_1;
    var alive_0 : bool = russian_roulette_survives_0(bounce_1, &(kept_0), &(roulette_rng_0));
    var _S45 : u32;
    if(alive_0)
    {
        _S45 = u32(1);
    }
    else
    {
        _S45 = u32(0);
    }
    VectorOutputs_put_0(&(outputs_1), _S45);
    VectorOutputs_put_4(&(outputs_1), kept_0);
    VectorOutputs_put_0(&(outputs_1), roulette_rng_0);
    var t_4 : vec3<f32>;
    var b_2 : vec3<f32>;
    orthonormal_basis_0(n_4, &(t_4), &(b_2));
    VectorOutputs_put_4(&(outputs_1), t_4);
    VectorOutputs_put_4(&(outputs_1), b_2);
    VectorOutputs_put_4(&(outputs_1), sample_ggx_visible_normal_0(local_v_0, alpha_3, u_1));
    VectorOutputs_put_2(&(outputs_1), ggx_visible_normal_pdf_0(n_dot_v_6, n_dot_h_3, alpha_3));
    VectorOutputs_put_2(&(outputs_1), specular_lobe_probability_0(albedo_5, metallic_4, f0_8, n_dot_v_6));
    var bounce_rng_0 : u32 = rng_2;
    var next_0 : Bounce_0 = sample_bounce_0(n_4, face_normal_1, v_4, albedo_5, metallic_4, roughness_8, &(bounce_rng_0));
    VectorOutputs_put_4(&(outputs_1), next_0.direction_0);
    VectorOutputs_put_4(&(outputs_1), next_0.weight_0);
    VectorOutputs_put_0(&(outputs_1), bounce_rng_0);
    return;
}

