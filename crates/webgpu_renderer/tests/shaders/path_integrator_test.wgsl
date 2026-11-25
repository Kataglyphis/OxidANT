@binding(0) @group(0) var<storage, read> vectorInputs_0 : array<u32>;

@binding(1) @group(0) var<storage, read_write> vectorOutputs_0 : array<u32>;

struct TracedSurface_0
{
     position_0 : vec3<f32>,
     face_normal_0 : vec3<f32>,
     normal_0 : vec3<f32>,
     albedo_0 : vec3<f32>,
     emissive_0 : vec3<f32>,
     metallic_0 : f32,
     roughness_0 : f32,
     occlusion_0 : f32,
     unlit_0 : bool,
};

struct PunctualLight_0
{
     position_1 : vec3<f32>,
     kind_0 : f32,
     radiance_0 : vec3<f32>,
     range_0 : f32,
     direction_0 : vec3<f32>,
     cos_inner_0 : f32,
     cos_outer_0 : f32,
};

struct FloorScene_0
{
     floor_0 : TracedSurface_0,
     sun_l_0 : vec3<f32>,
     sun_rad_0 : vec3<f32>,
     light_0 : PunctualLight_0,
     lights_0 : u32,
     furnace_0 : vec4<f32>,
};

struct TracedSegment_0
{
     origin_0 : vec3<f32>,
     tmin_0 : f32,
     direction_1 : vec3<f32>,
     tmax_0 : f32,
};

struct PathDepth_0
{
     wanted_0 : bool,
     ray_0 : TracedSegment_0,
     hit_0 : bool,
     position_2 : vec3<f32>,
};

fn rng_next_0( state_0 : ptr<function, u32>) -> f32
{
    var _S1 : u32 = (*state_0) * u32(747796405) + u32(1);
    (*state_0) = _S1;
    var word_0 : u32 = ((((_S1 >> ((((_S1 >> (u32(28)))) + u32(4))))) ^ (_S1))) * u32(277803737);
    return f32((((word_0 >> (u32(22)))) ^ (word_0))) / 4.294967296e+09f;
}

fn russian_roulette_survives_0( bounce_0 : u32,  throughput_0 : ptr<function, vec3<f32>>,  rng_0 : ptr<function, u32>) -> bool
{
    if(bounce_0 < u32(3))
    {
        return true;
    }
    var survival_0 : f32 = clamp(max((*throughput_0).x, max((*throughput_0).y, (*throughput_0).z)), 0.05000000074505806f, 0.94999998807907104f);
    var _S2 : f32 = rng_next_0(&((*rng_0)));
    if(_S2 > survival_0)
    {
        return false;
    }
    (*throughput_0) = (*throughput_0) / vec3<f32>(survival_0);
    return true;
}

fn FloorScene_closest_0( this_0 : FloorScene_0,  ray_1 : TracedSegment_0,  s_0 : ptr<function, TracedSurface_0>) -> bool
{
    var _S3 : f32 = ray_1.direction_1.y;
    var t_0 : f32 = - ray_1.origin_0.y / _S3;
    var found_0 : bool;
    if(_S3 < 0.0f)
    {
        found_0 = t_0 >= (ray_1.tmin_0);
    }
    else
    {
        found_0 = false;
    }
    if(found_0)
    {
        found_0 = t_0 <= (ray_1.tmax_0);
    }
    else
    {
        found_0 = false;
    }
    (*s_0) = this_0.floor_0;
    var _S4 : vec3<f32>;
    if(found_0)
    {
        _S4 = ray_1.origin_0 + ray_1.direction_1 * vec3<f32>(t_0);
    }
    else
    {
        _S4 = vec3<f32>(0.0f);
    }
    (*s_0).position_0 = _S4;
    return found_0;
}

fn FloorScene_furnace_on_0( this_1 : FloorScene_0) -> bool
{
    return (this_1.furnace_0.w) > 0.5f;
}

fn FloorScene_furnace_sky_0( this_2 : FloorScene_0) -> vec3<f32>
{
    return vec3<f32>(this_2.furnace_0.x);
}

fn FloorScene_camera_sky_0( this_3 : FloorScene_0,  dir_0 : vec3<f32>) -> vec3<f32>
{
    var _S5 : vec3<f32> = vec3<f32>(0.5f);
    return dir_0 * _S5 + _S5;
}

fn FloorScene_bounce_sky_0( this_4 : FloorScene_0,  dir_1 : vec3<f32>) -> vec3<f32>
{
    var _S6 : vec3<f32> = vec3<f32>(0.5f);
    return (dir_1 * _S6 + _S6) * vec3<f32>(0.25f);
}

fn path_miss_0( scene_0 : FloorScene_0,  dir_2 : vec3<f32>,  bounce_1 : u32) -> vec3<f32>
{
    var sky_0 : vec3<f32>;
    if(FloorScene_furnace_on_0(scene_0))
    {
        sky_0 = FloorScene_furnace_sky_0(scene_0);
    }
    else
    {
        if(bounce_1 == u32(0))
        {
            sky_0 = FloorScene_camera_sky_0(scene_0, dir_2);
        }
        else
        {
            sky_0 = FloorScene_bounce_sky_0(scene_0, dir_2);
        }
    }
    return sky_0;
}

fn FloorScene_furnace_material_0( this_5 : FloorScene_0) -> vec2<f32>
{
    return this_5.furnace_0.yz;
}

fn furnace_surface_0( scene_1 : FloorScene_0,  s_1 : TracedSurface_0) -> TracedSurface_0
{
    var _S7 : TracedSurface_0 = s_1;
    var material_0 : vec2<f32> = FloorScene_furnace_material_0(scene_1);
    _S7.albedo_0 = vec3<f32>(1.0f);
    _S7.emissive_0 = vec3<f32>(0.0f);
    _S7.unlit_0 = false;
    _S7.metallic_0 = material_0.x;
    _S7.roughness_0 = clamp(material_0.y, 0.04500000178813934f, 1.0f);
    return _S7;
}

fn ggx_energy_coordinates_0( n_dot_v_0 : f32,  roughness_1 : f32) -> vec2<f32>
{
    var low_0 : vec2<f32> = vec2<f32>(pow(0.00009999999747379f, 0.3333333432674408f), sqrt(0.04500000178813934f));
    return (vec2<f32>(pow(clamp(n_dot_v_0, 0.00009999999747379f, 1.0f), 0.3333333432674408f), sqrt(clamp(roughness_1, 0.04500000178813934f, 1.0f))) - low_0) / (vec2<f32>(1.0f) - low_0);
}

fn ggx_energy_weights_0( f_0 : f32) -> vec4<f32>
{
    var g_0 : f32 = 1.0f - f_0;
    return vec4<f32>(g_0 * g_0 * g_0, 4.0f - 6.0f * f_0 * f_0 + 3.0f * f_0 * f_0 * f_0, 4.0f - 6.0f * g_0 * g_0 + 3.0f * g_0 * g_0 * g_0, f_0 * f_0 * f_0) / vec4<f32>(6.0f);
}

var<private> GGX_ENERGY_COEFFICIENTS_0 : array<vec2<f32>, i32(196)> = array<vec2<f32>, i32(196)>( vec2<f32>(0.03755360096693039f, 3.05165600776672363f), vec2<f32>(0.03963959962129593f, 0.65487277507781982f), vec2<f32>(0.07296779751777649f, 0.51261270046234131f), vec2<f32>(0.17118540406227112f, 0.62788808345794678f), vec2<f32>(0.30981320142745972f, 0.50604218244552612f), vec2<f32>(0.44191190600395203f, 0.40881091356277466f), vec2<f32>(0.55946820974349976f, 0.28783369064331055f), vec2<f32>(0.65159928798675537f, 0.20042069256305695f), vec2<f32>(0.70950692892074585f, 0.12844389677047729f), vec2<f32>(0.73192507028579712f, 0.08202389627695084f), vec2<f32>(0.7161983847618103f, 0.05005419999361038f), vec2<f32>(0.66943138837814331f, 0.03112700022757053f), vec2<f32>(0.59648841619491577f, 0.01862730085849762f), vec2<f32>(0.50741302967071533f, 0.01281030010432005f), vec2<f32>(-0.00930839963257313f, -0.05096510052680969f), vec2<f32>(0.03267690166831017f, 0.88316977024078369f), vec2<f32>(0.11045809835195541f, 0.81648111343383789f), vec2<f32>(0.20676289498806f, 0.65160852670669556f), vec2<f32>(0.32054820656776428f, 0.53221142292022705f), vec2<f32>(0.4455254077911377f, 0.40326109528541565f), vec2<f32>(0.56171208620071411f, 0.2928445041179657f), vec2<f32>(0.65319478511810303f, 0.19869279861450195f), vec2<f32>(0.71095079183578491f, 0.12985430657863617f), vec2<f32>(0.7316511869430542f, 0.08150120079517365f), vec2<f32>(0.71713531017303467f, 0.05052080005407333f), vec2<f32>(0.66998302936553955f, 0.03087439946830273f), vec2<f32>(0.59695768356323242f, 0.01905369944870472f), vec2<f32>(0.50688481330871582f, 0.01136360038071871f), vec2<f32>(0.010585299693048f, 0.99188041687011719f), vec2<f32>(0.00207319995388389f, 0.40556570887565613f), vec2<f32>(0.05862849950790405f, 0.55259382724761963f), vec2<f32>(0.16572730243206024f, 0.58792608976364136f), vec2<f32>(0.29784908890724182f, 0.50322818756103516f), vec2<f32>(0.4318917989730835f, 0.39728268980979919f), vec2<f32>(0.55288940668106079f, 0.2878929078578949f), vec2<f32>(0.64753270149230957f, 0.19779230654239655f), vec2<f32>(0.70724570751190186f, 0.12865540385246277f), vec2<f32>(0.72979277372360229f, 0.08128180354833603f), vec2<f32>(0.71553927659988403f, 0.05013220012187958f), vec2<f32>(0.66890257596969604f, 0.03081009909510612f), vec2<f32>(0.59605568647384644f, 0.01881019957363605f), vec2<f32>(0.50637918710708618f, 0.01177479978650808f), vec2<f32>(0.04574630036950111f, 1.02595400810241699f), vec2<f32>(0.03743439912796021f, 0.88119810819625854f), vec2<f32>(0.04426249861717224f, 0.52496278285980225f), vec2<f32>(0.11310549825429916f, 0.46600010991096497f), vec2<f32>(0.23485749959945679f, 0.4375700056552887f), vec2<f32>(0.37963598966598511f, 0.36149460077285767f), vec2<f32>(0.51556599140167236f, 0.27323558926582336f), vec2<f32>(0.622580885887146f, 0.18986819684505463f), vec2<f32>(0.69136160612106323f, 0.12518900632858276f), vec2<f32>(0.71929609775543213f, 0.07907170057296753f), vec2<f32>(0.70917361974716187f, 0.04900079965591431f), vec2<f32>(0.66434282064437866f, 0.02995469979941845f), vec2<f32>(0.59224581718444824f, 0.01837039925158024f), vec2<f32>(0.5022997260093689f, 0.0110470000654459f), vec2<f32>(0.1206125020980835f, 0.77379542589187622f), vec2<f32>(0.12222660332918167f, 0.83616697788238525f), vec2<f32>(0.11106769740581512f, 0.74211102724075317f), vec2<f32>(0.12635010480880737f, 0.50614690780639648f), vec2<f32>(0.20402370393276215f, 0.39524701237678528f), vec2<f32>(0.32609659433364868f, 0.32315808534622192f), vec2<f32>(0.46191799640655518f, 0.24740250408649445f), vec2<f32>(0.57957160472869873f, 0.17592360079288483f), vec2<f32>(0.66025441884994507f, 0.11711639910936356f), vec2<f32>(0.69881141185760498f, 0.07462230324745178f), vec2<f32>(0.69453942775726318f, 0.04620540142059326f), vec2<f32>(0.65352827310562134f, 0.02824320085346699f), vec2<f32>(0.582955002784729f, 0.01713629998266697f), vec2<f32>(0.49382159113883972f, 0.01042309962213039f), vec2<f32>(0.26457729935646057f, 0.79617220163345337f), vec2<f32>(0.25447279214859009f, 0.71848297119140625f), vec2<f32>(0.24372699856758118f, 0.67536389827728271f), vec2<f32>(0.22911170125007629f, 0.58072292804718018f), vec2<f32>(0.24631629884243011f, 0.4128555953502655f), vec2<f32>(0.32152768969535828f, 0.3022313117980957f), vec2<f32>(0.42885839939117432f, 0.22238540649414062f), vec2<f32>(0.53810352087020874f, 0.157231405377388f), vec2<f32>(0.6227080225944519f, 0.10510919988155365f), vec2<f32>(0.66763401031494141f, 0.0672312006354332f), vec2<f32>(0.67200499773025513f, 0.04161379858851433f), vec2<f32>(0.63588422536849976f, 0.02530450001358986f), vec2<f32>(0.56762188673019409f, 0.01518450025469065f), vec2<f32>(0.47794249653816223f, 0.00909390021115541f), vec2<f32>(0.43767940998077393f, 0.52778768539428711f), vec2<f32>(0.43497830629348755f, 0.55565857887268066f), vec2<f32>(0.42141589522361755f, 0.53836488723754883f), vec2<f32>(0.4008432924747467f, 0.49193951487541199f), vec2<f32>(0.37870550155639648f, 0.40763860940933228f), vec2<f32>(0.38813981413841248f, 0.28910300135612488f), vec2<f32>(0.44539439678192139f, 0.20043960213661194f), vec2<f32>(0.52257758378982544f, 0.13623130321502686f), vec2<f32>(0.59163397550582886f, 0.08995489776134491f), vec2<f32>(0.63798898458480835f, 0.05706769973039627f), vec2<f32>(0.64467430114746094f, 0.03520869836211205f), vec2<f32>(0.61179417371749878f, 0.021161999553442f), vec2<f32>(0.5443912148475647f, 0.01255400013178587f), vec2<f32>(0.45556539297103882f, 0.00719430018216372f), vec2<f32>(0.63681560754776001f, 0.37943580746650696f), vec2<f32>(0.63195270299911499f, 0.36018449068069458f), vec2<f32>(0.62122827768325806f, 0.35363009572029114f), vec2<f32>(0.59902858734130859f, 0.34027791023254395f), vec2<f32>(0.56643378734588623f, 0.30496558547019958f), vec2<f32>(0.5314096212387085f, 0.24438419938087463f), vec2<f32>(0.52357661724090576f, 0.16980840265750885f), vec2<f32>(0.5474010705947876f, 0.11221110075712204f), vec2<f32>(0.58856910467147827f, 0.07183969765901566f), vec2<f32>(0.61636549234390259f, 0.04495130106806755f), vec2<f32>(0.61752068996429443f, 0.02730320021510124f), vec2<f32>(0.58241552114486694f, 0.01623369939625263f), vec2<f32>(0.51425069570541382f, 0.00938300043344498f), vec2<f32>(0.42317068576812744f, 0.00540499994531274f), vec2<f32>(0.81267541646957397f, 0.17737489938735962f), vec2<f32>(0.81089460849761963f, 0.18592619895935059f), vec2<f32>(0.80203372240066528f, 0.18450069427490234f), vec2<f32>(0.78451091051101685f, 0.17923450469970703f), vec2<f32>(0.75236481428146362f, 0.17097190022468567f), vec2<f32>(0.70702672004699707f, 0.15113990008831024f), vec2<f32>(0.65327489376068115f, 0.11831910163164139f), vec2<f32>(0.62291878461837769f, 0.08054299652576447f), vec2<f32>(0.61463671922683716f, 0.05135539919137955f), vec2<f32>(0.61291992664337158f, 0.03151309862732887f), vec2<f32>(0.59584552049636841f, 0.01889899931848049f), vec2<f32>(0.5510483980178833f, 0.01100130006670952f), vec2<f32>(0.4772191047668457f, 0.00626480020582676f), vec2<f32>(0.38401719927787781f, 0.0033825000282377f), vec2<f32>(0.93769299983978271f, 0.06798700243234634f), vec2<f32>(0.93442130088806152f, 0.06284449994564056f), vec2<f32>(0.92932981252670288f, 0.06249019876122475f), vec2<f32>(0.91649138927459717f, 0.06285469979047775f), vec2<f32>(0.89388298988342285f, 0.06178810074925423f), vec2<f32>(0.85349130630493164f, 0.06024239957332611f), vec2<f32>(0.79553717374801636f, 0.05434289947152138f), vec2<f32>(0.72840338945388794f, 0.04295980185270309f), vec2<f32>(0.67323261499404907f, 0.02926450036466122f), vec2<f32>(0.63030177354812622f, 0.01836870051920414f), vec2<f32>(0.58399122953414917f, 0.01094940025359392f), vec2<f32>(0.5202292799949646f, 0.00631939992308617f), vec2<f32>(0.43539279699325562f, 0.0035077000502497f), vec2<f32>(0.33793559670448303f, 0.00192960002459586f), vec2<f32>(0.99163192510604858f, 0.00484689977020025f), vec2<f32>(0.99116522073745728f, 0.00796679966151714f), vec2<f32>(0.98792862892150879f, 0.00806930009275675f), vec2<f32>(0.98114508390426636f, 0.0078320000320673f), vec2<f32>(0.96755552291870117f, 0.00866509974002838f), vec2<f32>(0.94221091270446777f, 0.00961260031908751f), vec2<f32>(0.89904522895812988f, 0.0111905001103878f), vec2<f32>(0.83300900459289551f, 0.01175749953836203f), vec2<f32>(0.75132989883422852f, 0.01031319983303547f), vec2<f32>(0.66817468404769897f, 0.00739389983937144f), vec2<f32>(0.58449500799179077f, 0.00469669979065657f), vec2<f32>(0.49239379167556763f, 0.00274799996986985f), vec2<f32>(0.39069679379463196f, 0.00153979996684939f), vec2<f32>(0.28739228844642639f, 0.00080069998512045f), vec2<f32>(1.00030839443206787f, 0.00132489996030927f), vec2<f32>(1.00059950351715088f, -0.00104150001425296f), vec2<f32>(0.99918842315673828f, -0.00109509995672852f), vec2<f32>(0.99662727117538452f, -0.00078689999645576f), vec2<f32>(0.99067902565002441f, -0.00099460000637919f), vec2<f32>(0.9789891242980957f, -0.00078649999340996f), vec2<f32>(0.95506691932678223f, -0.00063089997274801f), vec2<f32>(0.90853261947631836f, -0.00004849999822909f), vec2<f32>(0.83005660772323608f, 0.00064739998197183f), vec2<f32>(0.72199207544326782f, 0.00106369995046407f), vec2<f32>(0.59814202785491943f, 0.00094429997261614f), vec2<f32>(0.46943360567092896f, 0.0006571999983862f), vec2<f32>(0.34491020441055298f, 0.00038939999649301f), vec2<f32>(0.23603999614715576f, 0.00023839999630582f), vec2<f32>(0.99982082843780518f, -0.00226259999908507f), vec2<f32>(0.99921691417694092f, 0.00083259999519214f), vec2<f32>(0.99951702356338501f, 0.00091000000247732f), vec2<f32>(0.99938511848449707f, 0.00050560000818223f), vec2<f32>(0.9986388087272644f, 0.00080109998816624f), vec2<f32>(0.99591022729873657f, 0.00060789997223765f), vec2<f32>(0.98590642213821411f, 0.00062870001420379f), vec2<f32>(0.95896798372268677f, 0.00030819998937659f), vec2<f32>(0.89731162786483765f, -0.00005109999983688f), vec2<f32>(0.78470510244369507f, -0.00027940000290982f), vec2<f32>(0.62571001052856445f, -0.00019700000120793f), vec2<f32>(0.45251798629760742f, -0.00010209999891231f), vec2<f32>(0.29973480105400085f, -0.00003329999890411f), vec2<f32>(0.18639770150184631f, -0.000034600001527f), vec2<f32>(1.00496220588684082f, 0.00913080014288425f), vec2<f32>(1.00138688087463379f, -0.00241709989495575f), vec2<f32>(1.0036776065826416f, -0.0027135000564158f), vec2<f32>(1.00481629371643066f, -0.00120069994591177f), vec2<f32>(1.008644700050354f, -0.00232520001009107f), vec2<f32>(1.01138985157012939f, -0.00167589995544404f), vec2<f32>(1.01234376430511475f, -0.00196720007807016f), vec2<f32>(0.99929207563400269f, -0.00119440001435578f), vec2<f32>(0.95524948835372925f, -0.00035449999268167f), vec2<f32>(0.85058838129043579f, 0.00027970000519417f), vec2<f32>(0.6671786904335022f, 0.00011629999789875f), vec2<f32>(0.44304949045181274f, 0.00001530000008643f), vec2<f32>(0.25579911470413208f, -0.00005640000017593f), vec2<f32>(0.14276920258998871f, 0.00004659999831347f) );
fn ggx_energy_fit_0( t_1 : vec2<f32>) -> vec2<f32>
{
    var x_0 : vec2<f32> = t_1 * vec2<f32>(11.0f) + vec2<f32>(1.0f);
    var cell_0 : vec2<f32> = clamp(floor(x_0), vec2<f32>(1.0f), vec2<f32>(11.0f));
    var _S8 : f32 = cell_0.x;
    var _S9 : vec4<f32> = ggx_energy_weights_0(x_0.x - _S8);
    var _S10 : f32 = cell_0.y;
    var _S11 : vec4<f32> = ggx_energy_weights_0(x_0.y - _S10);
    var _S12 : i32 = i32(_S8 - 1.0f) * i32(14) + i32(_S10 - 1.0f);
    var _S13 : vec2<f32> = vec2<f32>(0.0f);
    var i_0 : i32 = i32(0);
    var e_0 : vec2<f32> = _S13;
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
        var row_0 : vec2<f32> = _S13;
        for(;;)
        {
            if(j_0 < i32(4))
            {
            }
            else
            {
                break;
            }
            var row_1 : vec2<f32> = row_0 + GGX_ENERGY_COEFFICIENTS_0[_S12 + i_0 * i32(14) + j_0] * vec2<f32>(_S11[j_0]);
            j_0 = j_0 + i32(1);
            row_0 = row_1;
        }
        var e_1 : vec2<f32> = e_0 + row_0 * vec2<f32>(_S9[i_0]);
        i_0 = i_0 + i32(1);
        e_0 = e_1;
    }
    return e_0;
}

fn ggx_energy_0( n_dot_v_1 : f32,  roughness_2 : f32) -> vec2<f32>
{
    return ggx_energy_fit_0(ggx_energy_coordinates_0(n_dot_v_1, roughness_2));
}

fn FloorScene_sun_direction_0( this_6 : FloorScene_0) -> vec3<f32>
{
    return this_6.sun_l_0;
}

fn FloorScene_sun_radiance_0( this_7 : FloorScene_0) -> vec3<f32>
{
    return this_7.sun_rad_0;
}

struct LightSample_0
{
     radiance_1 : vec3<f32>,
     l_0 : vec3<f32>,
     distance_0 : f32,
};

fn surface_f0_0( albedo_1 : vec3<f32>,  metallic_1 : f32) -> vec3<f32>
{
    return mix(vec3<f32>(0.03999999910593033f), albedo_1, vec3<f32>(metallic_1));
}

fn smith_k_analytic_0( roughness_3 : f32) -> f32
{
    var r_0 : f32 = max(roughness_3, 0.04500000178813934f) + 1.0f;
    return r_0 * r_0 / 8.0f;
}

fn distribution_ggx_0( n_dot_h_0 : f32,  roughness_4 : f32) -> f32
{
    var _S14 : f32 = max(roughness_4, 0.04500000178813934f);
    var a_0 : f32 = _S14 * _S14;
    var a2_0 : f32 = a_0 * a_0;
    var denom_0 : f32 = n_dot_h_0 * n_dot_h_0 * (a2_0 - 1.0f) + 1.0f;
    return a2_0 / (3.14159274101257324f * max(denom_0 * denom_0, 9.99999968265522539e-21f));
}

fn geometry_smith_k_0( n_dot_v_2 : f32,  n_dot_l_0 : f32,  k_0 : f32) -> f32
{
    var _S15 : f32 = 1.0f - k_0;
    return n_dot_v_2 / max(n_dot_v_2 * _S15 + k_0, 9.99999997475242708e-07f) * (n_dot_l_0 / max(n_dot_l_0 * _S15 + k_0, 9.99999997475242708e-07f));
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

fn lambert_diffuse_0( albedo_2 : vec3<f32>) -> vec3<f32>
{
    return albedo_2 / vec3<f32>(3.14159274101257324f);
}

fn brdf_smith_k_0( n_0 : vec3<f32>,  v_0 : vec3<f32>,  l_1 : vec3<f32>,  albedo_3 : vec3<f32>,  metallic_2 : f32,  roughness_6 : f32,  f0_4 : vec3<f32>,  radiance_2 : vec3<f32>,  k_2 : f32,  energy_2 : vec2<f32>) -> vec3<f32>
{
    var h_0 : vec3<f32> = normalize(l_1 + v_0);
    var _S16 : f32 = max(dot(n_0, l_1), 0.0f);
    return (saturate(vec3<f32>(1.0f) - specular_albedo_0(energy_2, f0_4)) * vec3<f32>((1.0f - metallic_2)) * lambert_diffuse_0(albedo_3) + multiscatter_gain_0(energy_2, f0_4) * ggx_single_scatter_0(max(dot(n_0, v_0), 0.00009999999747379f), _S16, max(dot(n_0, h_0), 0.0f), max(dot(h_0, v_0), 0.0f), max(roughness_6, 0.04500000178813934f), f0_4, k_2)) * radiance_2 * vec3<f32>(_S16);
}

fn brdf_direct_0( n_1 : vec3<f32>,  v_1 : vec3<f32>,  l_2 : vec3<f32>,  albedo_4 : vec3<f32>,  metallic_3 : f32,  roughness_7 : f32,  f0_5 : vec3<f32>,  radiance_3 : vec3<f32>,  energy_3 : vec2<f32>) -> vec3<f32>
{
    return brdf_smith_k_0(n_1, v_1, l_2, albedo_4, metallic_3, roughness_7, f0_5, radiance_3, smith_k_analytic_0(roughness_7), energy_3);
}

fn sun_sample_0( s_2 : TracedSurface_0,  v_2 : vec3<f32>,  energy_4 : vec2<f32>,  l_3 : vec3<f32>,  radiance_4 : vec3<f32>) -> LightSample_0
{
    var o_0 : LightSample_0;
    o_0.radiance_1 = vec3<f32>(0.0f);
    o_0.l_0 = vec3<f32>(0.0f, 1.0f, 0.0f);
    o_0.distance_0 = 0.0f;
    var o_1 : LightSample_0 = o_0;
    o_1.l_0 = l_3;
    o_1.distance_0 = 10000.0f;
    if((dot(s_2.normal_0, l_3)) <= 0.0f)
    {
        return o_1;
    }
    o_1.radiance_1 = brdf_direct_0(s_2.normal_0, v_2, l_3, s_2.albedo_0, s_2.metallic_0, s_2.roughness_0, surface_f0_0(s_2.albedo_0, s_2.metallic_0), radiance_4, energy_4);
    return o_1;
}

fn FloorScene_punctual_count_0( this_8 : FloorScene_0) -> u32
{
    return this_8.lights_0;
}

fn rng_next_1( state_1 : ptr<function, u32>) -> f32
{
    var _S17 : u32 = (*state_1) * u32(747796405) + u32(1);
    (*state_1) = _S17;
    var word_1 : u32 = ((((_S17 >> ((((_S17 >> (u32(28)))) + u32(4))))) ^ (_S17))) * u32(277803737);
    return f32((((word_1 >> (u32(22)))) ^ (word_1))) / 4.294967296e+09f;
}

fn FloorScene_punctual_0( this_9 : FloorScene_0,  index_0 : u32) -> PunctualLight_0
{
    return this_9.light_0;
}

struct PunctualIncidence_0
{
     l_4 : vec3<f32>,
     distance_1 : f32,
     attenuation_0 : f32,
};

fn punctual_incidence_0( light_1 : PunctualLight_0,  position_3 : vec3<f32>) -> PunctualIncidence_0
{
    var o_2 : PunctualIncidence_0;
    o_2.attenuation_0 = 1.0f;
    if((light_1.kind_0) > 2.5f)
    {
        o_2.l_4 = normalize((vec3<f32>(0) - light_1.direction_0));
        o_2.distance_1 = 3.00000000549775576e+38f;
        return o_2;
    }
    var toLight_0 : vec3<f32> = light_1.position_1 - position_3;
    var _S18 : f32 = max(length(toLight_0), 0.00009999999747379f);
    o_2.distance_1 = _S18;
    o_2.l_4 = toLight_0 / vec3<f32>(_S18);
    o_2.attenuation_0 = 1.0f / (_S18 * _S18);
    if((light_1.range_0) > 0.0f)
    {
        var k_3 : f32 = clamp(1.0f - pow(o_2.distance_1 / light_1.range_0, 4.0f), 0.0f, 1.0f);
        o_2.attenuation_0 = o_2.attenuation_0 * (k_3 * k_3);
    }
    if((light_1.kind_0) > 1.5f)
    {
        o_2.attenuation_0 = o_2.attenuation_0 * smoothstep(light_1.cos_outer_0, light_1.cos_inner_0, dot(normalize(light_1.direction_0), (vec3<f32>(0) - o_2.l_4)));
    }
    return o_2;
}

fn punctual_sample_0( s_3 : TracedSurface_0,  v_3 : vec3<f32>,  energy_5 : vec2<f32>,  light_2 : PunctualLight_0) -> LightSample_0
{
    var o_3 : LightSample_0;
    o_3.radiance_1 = vec3<f32>(0.0f);
    o_3.l_0 = vec3<f32>(0.0f, 1.0f, 0.0f);
    o_3.distance_0 = 0.0f;
    var o_4 : LightSample_0 = o_3;
    var incidence_0 : PunctualIncidence_0 = punctual_incidence_0(light_2, s_3.position_0);
    o_4.l_0 = incidence_0.l_4;
    o_4.distance_0 = incidence_0.distance_1 * 0.99989998340606689f;
    var _S19 : bool;
    if((incidence_0.attenuation_0) <= 0.0f)
    {
        _S19 = true;
    }
    else
    {
        _S19 = (dot(s_3.normal_0, incidence_0.l_4)) <= 0.0f;
    }
    if(_S19)
    {
        return o_4;
    }
    o_4.radiance_1 = brdf_direct_0(s_3.normal_0, v_3, incidence_0.l_4, s_3.albedo_0, s_3.metallic_0, s_3.roughness_0, surface_f0_0(s_3.albedo_0, s_3.metallic_0), light_2.radiance_0 * vec3<f32>(incidence_0.attenuation_0), energy_5);
    return o_4;
}

fn picked_punctual_sample_0( scene_2 : FloorScene_0,  s_4 : TracedSurface_0,  v_4 : vec3<f32>,  energy_6 : vec2<f32>,  rng_1 : ptr<function, u32>) -> LightSample_0
{
    var count_0 : u32 = FloorScene_punctual_count_0(scene_2);
    var o_5 : LightSample_0;
    o_5.radiance_1 = vec3<f32>(0.0f);
    o_5.l_0 = vec3<f32>(0.0f, 1.0f, 0.0f);
    o_5.distance_0 = 0.0f;
    var o_6 : LightSample_0 = o_5;
    if(count_0 > u32(0))
    {
        var _S20 : f32 = rng_next_1(&((*rng_1)));
        var _S21 : f32 = f32(count_0);
        o_6 = punctual_sample_0(s_4, v_4, energy_6, FloorScene_punctual_0(scene_2, min(u32(_S20 * _S21), count_0 - u32(1))));
        o_6.radiance_1 = o_6.radiance_1 * vec3<f32>(_S21);
    }
    return o_6;
}

fn FloorScene_visible_0( this_10 : FloorScene_0,  s_5 : TracedSurface_0,  l_5 : vec3<f32>,  distance_2 : f32) -> bool
{
    return (l_5.x) < 0.5f;
}

fn shadowed_0( scene_3 : FloorScene_0,  s_6 : TracedSurface_0,  light_3 : LightSample_0) -> vec3<f32>
{
    var _S22 : bool;
    if((any(((light_3.radiance_1) > vec3<f32>(0.0f)))))
    {
        _S22 = FloorScene_visible_0(scene_3, s_6, light_3.l_0, light_3.distance_0);
    }
    else
    {
        _S22 = false;
    }
    var _S23 : vec3<f32>;
    if(_S22)
    {
        _S23 = light_3.radiance_1;
    }
    else
    {
        _S23 = vec3<f32>(0.0f);
    }
    return _S23;
}

fn next_event_0( scene_4 : FloorScene_0,  s_7 : TracedSurface_0,  v_5 : vec3<f32>,  energy_7 : vec2<f32>,  rng_2 : ptr<function, u32>) -> vec3<f32>
{
    if(FloorScene_furnace_on_0(scene_4))
    {
        return vec3<f32>(0.0f);
    }
    var _S24 : LightSample_0 = sun_sample_0(s_7, v_5, energy_7, FloorScene_sun_direction_0(scene_4), FloorScene_sun_radiance_0(scene_4));
    var _S25 : LightSample_0 = picked_punctual_sample_0(scene_4, s_7, v_5, energy_7, &((*rng_2)));
    var _S26 : array<LightSample_0, i32(2)> = array<LightSample_0, i32(2)>( _S24, _S25 );
    var _S27 : vec3<f32> = vec3<f32>(0.0f);
    var i_1 : u32 = u32(0);
    var lit_0 : vec3<f32> = _S27;
    for(;;)
    {
        if(i_1 < u32(2))
        {
        }
        else
        {
            break;
        }
        var lit_1 : vec3<f32> = lit_0 + shadowed_0(scene_4, s_7, _S26[i_1]);
        i_1 = i_1 + u32(1);
        lit_0 = lit_1;
    }
    return lit_0;
}

struct Bounce_0
{
     direction_2 : vec3<f32>,
     weight_0 : vec3<f32>,
};

fn specular_lobe_probability_0( albedo_5 : vec3<f32>,  metallic_4 : f32,  f0_6 : vec3<f32>,  n_dot_v_4 : f32) -> f32
{
    const LUMA_0 : vec3<f32> = vec3<f32>(0.2125999927520752f, 0.71520000696182251f, 0.07220000028610229f);
    var fresnel_0 : vec3<f32> = fresnel_schlick_0(n_dot_v_4, f0_6);
    var specular_0 : f32 = dot(fresnel_0, LUMA_0);
    return clamp(specular_0 / max(specular_0 + dot((vec3<f32>(1.0f) - fresnel_0) * vec3<f32>((1.0f - metallic_4)) * albedo_5, LUMA_0), 9.99999997475242708e-07f), 0.10000000149011612f, 1.0f);
}

fn orthonormal_basis_0( n_2 : vec3<f32>,  t_2 : ptr<function, vec3<f32>>,  b_0 : ptr<function, vec3<f32>>)
{
    var _S28 : f32 = n_2.z;
    var s_8 : f32;
    if(_S28 >= 0.0f)
    {
        s_8 = 1.0f;
    }
    else
    {
        s_8 = -1.0f;
    }
    var a_1 : f32 = -1.0f / (s_8 + _S28);
    var _S29 : f32 = n_2.x;
    var _S30 : f32 = n_2.y;
    var c_0 : f32 = _S29 * _S30 * a_1;
    (*t_2) = vec3<f32>(1.0f + s_8 * _S29 * _S29 * a_1, s_8 * c_0, - s_8 * _S29);
    (*b_0) = vec3<f32>(c_0, s_8 + _S30 * _S30 * a_1, - _S30);
    return;
}

fn sample_ggx_visible_normal_0( v_6 : vec3<f32>,  alpha_0 : f32,  u_0 : vec2<f32>) -> vec3<f32>
{
    var vh_0 : vec3<f32> = normalize(vec3<f32>(alpha_0 * v_6.x, alpha_0 * v_6.y, v_6.z));
    var _S31 : f32 = vh_0.x;
    var _S32 : f32 = vh_0.y;
    var length_squared_0 : f32 = _S31 * _S31 + _S32 * _S32;
    var t1_0 : vec3<f32>;
    if(length_squared_0 > 0.0f)
    {
        t1_0 = vec3<f32>(- _S32, _S31, 0.0f) / vec3<f32>(sqrt(length_squared_0));
    }
    else
    {
        t1_0 = vec3<f32>(1.0f, 0.0f, 0.0f);
    }
    var r_1 : f32 = sqrt(u_0.x);
    var phi_0 : f32 = 6.28318548202514648f * u_0.y;
    var p1_0 : f32 = r_1 * cos(phi_0);
    var s_9 : f32 = 0.5f * (1.0f + vh_0.z);
    var _S33 : f32 = 1.0f - p1_0 * p1_0;
    var p2_0 : f32 = (1.0f - s_9) * sqrt(max(_S33, 0.0f)) + s_9 * (r_1 * sin(phi_0));
    var nh_0 : vec3<f32> = vec3<f32>(p1_0) * t1_0 + vec3<f32>(p2_0) * cross(vh_0, t1_0) + vec3<f32>(sqrt(max(_S33 - p2_0 * p2_0, 0.0f))) * vh_0;
    return normalize(vec3<f32>(alpha_0 * nh_0.x, alpha_0 * nh_0.y, max(nh_0.z, 9.99999997475242708e-07f)));
}

fn ggx_visible_normal_pdf_0( n_dot_v_5 : f32,  n_dot_h_2 : f32,  alpha_1 : f32) -> f32
{
    var a2_1 : f32 = alpha_1 * alpha_1;
    var d_0 : f32 = n_dot_h_2 * n_dot_h_2 * (a2_1 - 1.0f) + 1.0f;
    return a2_1 / (3.14159274101257324f * d_0 * d_0) * (2.0f * n_dot_v_5 / (n_dot_v_5 + sqrt(a2_1 + (1.0f - a2_1) * n_dot_v_5 * n_dot_v_5))) / (4.0f * n_dot_v_5);
}

fn smith_k_sampled_0( roughness_8 : f32) -> f32
{
    var _S34 : f32 = max(roughness_8, 0.04500000178813934f);
    return _S34 * _S34 / 2.0f;
}

fn brdf_sampled_0( n_3 : vec3<f32>,  v_7 : vec3<f32>,  l_6 : vec3<f32>,  albedo_6 : vec3<f32>,  metallic_5 : f32,  roughness_9 : f32,  f0_7 : vec3<f32>,  energy_8 : vec2<f32>) -> vec3<f32>
{
    return brdf_smith_k_0(n_3, v_7, l_6, albedo_6, metallic_5, roughness_9, f0_7, vec3<f32>(1.0f), smith_k_sampled_0(roughness_9), energy_8);
}

fn sample_bounce_0( n_4 : vec3<f32>,  face_normal_1 : vec3<f32>,  v_8 : vec3<f32>,  albedo_7 : vec3<f32>,  metallic_6 : f32,  roughness_10 : f32,  energy_9 : vec2<f32>,  rng_3 : ptr<function, u32>) -> Bounce_0
{
    var f0_8 : vec3<f32> = surface_f0_0(albedo_7, metallic_6);
    var _S35 : f32 = max(dot(n_4, v_8), 0.00009999999747379f);
    var specular_1 : f32 = specular_lobe_probability_0(albedo_7, metallic_6, f0_8, _S35);
    var alpha_2 : f32 = roughness_10 * roughness_10;
    var t_3 : vec3<f32>;
    var b_1 : vec3<f32>;
    orthonormal_basis_0(n_4, &(t_3), &(b_1));
    var _S36 : f32 = rng_next_1(&((*rng_3)));
    var direction_3 : vec3<f32>;
    if(_S36 < specular_1)
    {
        var local_0 : vec3<f32> = vec3<f32>(dot(v_8, t_3), dot(v_8, b_1), dot(v_8, n_4));
        var _S37 : f32 = rng_next_1(&((*rng_3)));
        var _S38 : f32 = rng_next_1(&((*rng_3)));
        var h_1 : vec3<f32> = sample_ggx_visible_normal_0(local_0, alpha_2, vec2<f32>(_S37, _S38));
        direction_3 = reflect((vec3<f32>(0) - v_8), vec3<f32>(h_1.x) * t_3 + vec3<f32>(h_1.y) * b_1 + vec3<f32>(h_1.z) * n_4);
    }
    else
    {
        var _S39 : f32 = rng_next_1(&((*rng_3)));
        var theta_0 : f32 = 6.28318548202514648f * _S39;
        var _S40 : f32 = rng_next_1(&((*rng_3)));
        var z_0 : f32 = 2.0f * _S40 - 1.0f;
        var r_2 : f32 = sqrt(max(1.0f - z_0 * z_0, 0.0f));
        var direction_4 : vec3<f32> = n_4 + vec3<f32>(r_2 * cos(theta_0), r_2 * sin(theta_0), z_0);
        if((dot(direction_4, direction_4)) < 9.99999993922529029e-09f)
        {
            direction_3 = n_4;
        }
        else
        {
            direction_3 = direction_4;
        }
    }
    var o_7 : Bounce_0;
    var _S41 : vec3<f32> = normalize(direction_3);
    o_7.direction_2 = _S41;
    o_7.weight_0 = vec3<f32>(0.0f);
    var n_dot_l_2 : f32 = dot(n_4, _S41);
    var _S42 : bool;
    if(n_dot_l_2 <= 0.0f)
    {
        _S42 = true;
    }
    else
    {
        _S42 = (dot(face_normal_1, o_7.direction_2)) <= 0.0f;
    }
    if(_S42)
    {
        return o_7;
    }
    o_7.weight_0 = brdf_sampled_0(n_4, v_8, o_7.direction_2, albedo_7, metallic_6, roughness_10, f0_8, energy_9) / vec3<f32>(max(specular_1 * ggx_visible_normal_pdf_0(_S35, max(dot(n_4, normalize(v_8 + o_7.direction_2)), 0.0f), alpha_2) + (1.0f - specular_1) * n_dot_l_2 / 3.14159274101257324f, 9.99999997475242708e-07f));
    return o_7;
}

fn trace_path_0( scene_5 : FloorScene_0,  camera_0 : TracedSegment_0,  max_bounces_0 : u32,  rng_4 : ptr<function, u32>,  depth_0 : ptr<function, PathDepth_0>) -> vec3<f32>
{
    var probing_0 : bool = (*depth_0).wanted_0;
    var ray_2 : TracedSegment_0;
    var _S43 : TracedSegment_0;
    if((*depth_0).wanted_0)
    {
        _S43 = (*depth_0).ray_0;
    }
    else
    {
        _S43 = camera_0;
    }
    ray_2 = _S43;
    var throughput_1 : vec3<f32> = vec3<f32>(1.0f);
    var _S44 : vec3<f32> = vec3<f32>(0.0f);
    var probing_1 : bool = probing_0;
    var bounce_2 : u32 = u32(0);
    var radiance_5 : vec3<f32> = _S44;
    for(;;)
    {
        var _S45 : bool;
        if(probing_1)
        {
            _S45 = true;
        }
        else
        {
            _S45 = bounce_2 < max_bounces_0;
        }
        if(_S45)
        {
        }
        else
        {
            break;
        }
        var _S46 : bool;
        if(!probing_1)
        {
            var _S47 : bool = russian_roulette_survives_0(bounce_2, &(throughput_1), &((*rng_4)));
            _S46 = !_S47;
        }
        else
        {
            _S46 = false;
        }
        if(_S46)
        {
            break;
        }
        var s_10 : TracedSurface_0;
        var hit_1 : bool = FloorScene_closest_0(scene_5, ray_2, &(s_10));
        if(probing_1)
        {
            (*depth_0).hit_0 = hit_1;
            (*depth_0).position_2 = s_10.position_0;
            ray_2 = camera_0;
            probing_1 = false;
            continue;
        }
        if(!hit_1)
        {
            radiance_5 = radiance_5 + throughput_1 * path_miss_0(scene_5, ray_2.direction_1, bounce_2);
            break;
        }
        if(FloorScene_furnace_on_0(scene_5))
        {
            s_10 = furnace_surface_0(scene_5, s_10);
        }
        if(s_10.unlit_0)
        {
            radiance_5 = radiance_5 + throughput_1 * s_10.albedo_0;
            break;
        }
        var v_9 : vec3<f32> = (vec3<f32>(0) - ray_2.direction_1);
        if((dot(s_10.normal_0, v_9)) < 0.00009999999747379f)
        {
            s_10.normal_0 = s_10.face_normal_0;
        }
        var energy_10 : vec2<f32> = ggx_energy_0(dot(s_10.normal_0, v_9), s_10.roughness_0);
        var _S48 : vec3<f32> = next_event_0(scene_5, s_10, v_9, energy_10, &((*rng_4)));
        var radiance_6 : vec3<f32> = radiance_5 + throughput_1 * (s_10.emissive_0 + _S48);
        var next_0 : Bounce_0 = sample_bounce_0(s_10.normal_0, s_10.face_normal_0, v_9, s_10.albedo_0, s_10.metallic_0, s_10.roughness_0, energy_10, &((*rng_4)));
        if((all(((next_0.weight_0) == _S44))))
        {
            radiance_5 = radiance_6;
            break;
        }
        throughput_1 = throughput_1 * next_0.weight_0;
        ray_2.origin_0 = s_10.position_0 + s_10.face_normal_0 * vec3<f32>(0.00009999999747379f);
        ray_2.tmin_0 = 0.00100000004749745f;
        ray_2.direction_1 = next_0.direction_2;
        ray_2.tmax_0 = 10000.0f;
        bounce_2 = bounce_2 + u32(1);
        radiance_5 = radiance_6;
    }
    return radiance_5;
}

fn picked_punctual_sample_1( scene_6 : FloorScene_0,  s_11 : TracedSurface_0,  v_10 : vec3<f32>,  energy_11 : vec2<f32>,  rng_5 : ptr<function, u32>) -> LightSample_0
{
    var count_1 : u32 = FloorScene_punctual_count_0(scene_6);
    var o_8 : LightSample_0;
    o_8.radiance_1 = vec3<f32>(0.0f);
    o_8.l_0 = vec3<f32>(0.0f, 1.0f, 0.0f);
    o_8.distance_0 = 0.0f;
    var o_9 : LightSample_0 = o_8;
    if(count_1 > u32(0))
    {
        var _S49 : f32 = rng_next_1(&((*rng_5)));
        var _S50 : f32 = f32(count_1);
        o_9 = punctual_sample_0(s_11, v_10, energy_11, FloorScene_punctual_0(scene_6, min(u32(_S49 * _S50), count_1 - u32(1))));
        o_9.radiance_1 = o_9.radiance_1 * vec3<f32>(_S50);
    }
    return o_9;
}

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

fn TracedSurface_x24init_0( position_4 : vec3<f32>,  face_normal_2 : vec3<f32>,  normal_1 : vec3<f32>,  albedo_8 : vec3<f32>,  emissive_1 : vec3<f32>,  metallic_7 : f32,  roughness_11 : f32,  occlusion_1 : f32,  unlit_1 : bool) -> TracedSurface_0
{
    var _S51 : TracedSurface_0;
    _S51.position_0 = position_4;
    _S51.face_normal_0 = face_normal_2;
    _S51.normal_0 = normal_1;
    _S51.albedo_0 = albedo_8;
    _S51.emissive_0 = emissive_1;
    _S51.metallic_0 = metallic_7;
    _S51.roughness_0 = roughness_11;
    _S51.occlusion_0 = occlusion_1;
    _S51.unlit_0 = unlit_1;
    return _S51;
}

fn VectorInputs_u32_0( this_11 : ptr<function, VectorInputs_0>) -> u32
{
    var word_2 : u32 = vectorInputs_0[(*this_11).at_0];
    (*this_11).at_0 = (*this_11).at_0 + u32(1);
    return word_2;
}

fn VectorInputs_u32_1( this_12 : ptr<function, VectorInputs_0>) -> u32
{
    var word_3 : u32 = vectorInputs_0[(*this_12).at_0];
    (*this_12).at_0 = (*this_12).at_0 + u32(1);
    return word_3;
}

fn VectorInputs_f32_0( this_13 : ptr<function, VectorInputs_0>) -> f32
{
    var _S52 : u32 = VectorInputs_u32_1(&((*this_13)));
    return (bitcast<f32>((_S52)));
}

fn VectorInputs_f32_1( this_14 : ptr<function, VectorInputs_0>) -> f32
{
    var _S53 : u32 = VectorInputs_u32_1(&((*this_14)));
    return (bitcast<f32>((_S53)));
}

fn VectorInputs_f32x3_0( this_15 : ptr<function, VectorInputs_0>) -> vec3<f32>
{
    var x_1 : f32 = VectorInputs_f32_1(&((*this_15)));
    var y_0 : f32 = VectorInputs_f32_1(&((*this_15)));
    var z_1 : f32 = VectorInputs_f32_1(&((*this_15)));
    return vec3<f32>(x_1, y_0, z_1);
}

fn VectorInputs_f32x3_1( this_16 : ptr<function, VectorInputs_0>) -> vec3<f32>
{
    var x_2 : f32 = VectorInputs_f32_1(&((*this_16)));
    var y_1 : f32 = VectorInputs_f32_1(&((*this_16)));
    var z_2 : f32 = VectorInputs_f32_1(&((*this_16)));
    return vec3<f32>(x_2, y_1, z_2);
}

fn VectorInputs_f32x4_0( this_17 : ptr<function, VectorInputs_0>) -> vec4<f32>
{
    var xyz_0 : vec3<f32> = VectorInputs_f32x3_1(&((*this_17)));
    var w_0 : f32 = VectorInputs_f32_1(&((*this_17)));
    return vec4<f32>(xyz_0, w_0);
}

fn unpack_punctual_light_0( a_2 : vec4<f32>,  b_2 : vec4<f32>,  c_1 : vec4<f32>,  d_1 : vec4<f32>) -> PunctualLight_0
{
    var light_4 : PunctualLight_0;
    light_4.position_1 = a_2.xyz;
    light_4.kind_0 = a_2.w;
    light_4.radiance_0 = b_2.xyz;
    light_4.range_0 = b_2.w;
    light_4.direction_0 = c_1.xyz;
    light_4.cos_inner_0 = c_1.w;
    light_4.cos_outer_0 = d_1.x;
    return light_4;
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

fn VectorOutputs_put_0( this_18 : ptr<function, VectorOutputs_0>,  word_4 : u32)
{
    vectorOutputs_0[(*this_18).at_1] = word_4;
    (*this_18).at_1 = (*this_18).at_1 + u32(1);
    return;
}

fn VectorOutputs_put_1( this_19 : ptr<function, VectorOutputs_0>,  word_5 : u32)
{
    vectorOutputs_0[(*this_19).at_1] = word_5;
    (*this_19).at_1 = (*this_19).at_1 + u32(1);
    return;
}

fn VectorOutputs_put_2( this_20 : ptr<function, VectorOutputs_0>,  value_0 : f32)
{
    VectorOutputs_put_1(&((*this_20)), (bitcast<u32>((value_0))));
    return;
}

fn VectorOutputs_put_3( this_21 : ptr<function, VectorOutputs_0>,  value_1 : f32)
{
    VectorOutputs_put_1(&((*this_21)), (bitcast<u32>((value_1))));
    return;
}

fn VectorOutputs_put_4( this_22 : ptr<function, VectorOutputs_0>,  value_2 : vec3<f32>)
{
    VectorOutputs_put_3(&((*this_22)), value_2.x);
    VectorOutputs_put_3(&((*this_22)), value_2.y);
    VectorOutputs_put_3(&((*this_22)), value_2.z);
    return;
}

fn VectorOutputs_put_5( this_23 : ptr<function, VectorOutputs_0>,  value_3 : vec3<f32>)
{
    VectorOutputs_put_3(&((*this_23)), value_3.x);
    VectorOutputs_put_3(&((*this_23)), value_3.y);
    VectorOutputs_put_3(&((*this_23)), value_3.z);
    return;
}

fn VectorOutputs_put_6( this_24 : ptr<function, VectorOutputs_0>,  value_4 : vec4<f32>)
{
    VectorOutputs_put_5(&((*this_24)), value_4.xyz);
    VectorOutputs_put_3(&((*this_24)), value_4.w);
    return;
}

fn no_depth_0() -> PathDepth_0
{
    var d_2 : PathDepth_0;
    d_2.wanted_0 = false;
    var _S54 : vec3<f32> = vec3<f32>(0.0f);
    d_2.ray_0.origin_0 = _S54;
    d_2.ray_0.tmin_0 = 0.0f;
    d_2.ray_0.direction_1 = vec3<f32>(0.0f, 0.0f, 1.0f);
    d_2.ray_0.tmax_0 = 0.0f;
    d_2.hit_0 = false;
    d_2.position_2 = _S54;
    return d_2;
}

@compute
@workgroup_size(1, 1, 1)
fn path_integrator_test_main(@builtin(global_invocation_id) tid_0 : vec3<u32>)
{
    var _S55 : u32 = tid_0.x;
    var inputs_1 : VectorInputs_0 = vector_inputs_0(_S55);
    var scene_7 : FloorScene_0;
    var _S56 : vec3<f32> = vec3<f32>(0.0f);
    var s_12 : TracedSurface_0 = TracedSurface_x24init_0(_S56, _S56, _S56, _S56, _S56, 0.0f, 0.0f, 0.0f, false);
    scene_7.floor_0 = s_12;
    const _S57 : vec3<f32> = vec3<f32>(0.0f, 1.0f, 0.0f);
    scene_7.floor_0.face_normal_0 = _S57;
    var _S58 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    scene_7.floor_0.normal_0 = _S58;
    var _S59 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    scene_7.floor_0.albedo_0 = _S59;
    var _S60 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    scene_7.floor_0.emissive_0 = _S60;
    var _S61 : f32 = VectorInputs_f32_0(&(inputs_1));
    scene_7.floor_0.metallic_0 = _S61;
    var _S62 : f32 = VectorInputs_f32_0(&(inputs_1));
    scene_7.floor_0.roughness_0 = _S62;
    scene_7.floor_0.occlusion_0 = 1.0f;
    var _S63 : u32 = VectorInputs_u32_0(&(inputs_1));
    scene_7.floor_0.unlit_0 = _S63 != u32(0);
    var _S64 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    scene_7.sun_l_0 = _S64;
    var _S65 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    scene_7.sun_rad_0 = _S65;
    var a_3 : vec4<f32> = VectorInputs_f32x4_0(&(inputs_1));
    var b_3 : vec4<f32> = VectorInputs_f32x4_0(&(inputs_1));
    var c_2 : vec4<f32> = VectorInputs_f32x4_0(&(inputs_1));
    var d_3 : vec4<f32> = VectorInputs_f32x4_0(&(inputs_1));
    var _S66 : PunctualLight_0 = unpack_punctual_light_0(a_3, b_3, c_2, d_3);
    scene_7.light_0 = _S66;
    var _S67 : u32 = VectorInputs_u32_0(&(inputs_1));
    scene_7.lights_0 = _S67;
    var _S68 : vec4<f32> = VectorInputs_f32x4_0(&(inputs_1));
    scene_7.furnace_0 = _S68;
    var camera_1 : TracedSegment_0;
    var _S69 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    camera_1.origin_0 = _S69;
    camera_1.tmin_0 = 0.0f;
    var _S70 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    camera_1.direction_1 = _S70;
    var _S71 : f32 = VectorInputs_f32_0(&(inputs_1));
    camera_1.tmax_0 = _S71;
    var max_bounces_1 : u32 = VectorInputs_u32_0(&(inputs_1));
    var rng_6 : u32 = VectorInputs_u32_0(&(inputs_1));
    var _S72 : u32 = VectorInputs_u32_0(&(inputs_1));
    var probe_0 : bool = _S72 != u32(0);
    var s_13 : TracedSurface_0 = scene_7.floor_0;
    var _S73 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    s_13.position_0 = _S73;
    var v_11 : vec3<f32> = VectorInputs_f32x3_0(&(inputs_1));
    var energy_12 : vec2<f32> = ggx_energy_0(dot(s_13.normal_0, v_11), s_13.roughness_0);
    var outputs_1 : VectorOutputs_0 = vector_outputs_0(_S55);
    VectorOutputs_put_6(&(outputs_1), vec4<f32>(s_12.position_0.x + s_12.normal_0.y + s_12.face_normal_0.z, s_12.albedo_0.x + s_12.emissive_0.y, s_12.roughness_0 + s_12.metallic_0 + s_12.occlusion_0, f32(s_12.unlit_0)));
    var o_10 : LightSample_0;
    o_10.radiance_1 = _S56;
    o_10.l_0 = _S57;
    o_10.distance_0 = 0.0f;
    VectorOutputs_put_6(&(outputs_1), vec4<f32>(o_10.radiance_1, o_10.distance_0));
    VectorOutputs_put_4(&(outputs_1), o_10.l_0);
    var sun_0 : LightSample_0 = sun_sample_0(s_13, v_11, energy_12, _S64, _S65);
    VectorOutputs_put_4(&(outputs_1), sun_0.radiance_1);
    VectorOutputs_put_4(&(outputs_1), sun_0.l_0);
    VectorOutputs_put_2(&(outputs_1), sun_0.distance_0);
    var punctual_0 : LightSample_0 = punctual_sample_0(s_13, v_11, energy_12, _S66);
    VectorOutputs_put_4(&(outputs_1), punctual_0.radiance_1);
    VectorOutputs_put_4(&(outputs_1), punctual_0.l_0);
    VectorOutputs_put_2(&(outputs_1), punctual_0.distance_0);
    VectorOutputs_put_4(&(outputs_1), shadowed_0(scene_7, s_13, sun_0));
    VectorOutputs_put_4(&(outputs_1), shadowed_0(scene_7, s_13, punctual_0));
    var picked_rng_0 : u32 = rng_6;
    var _S74 : LightSample_0 = picked_punctual_sample_1(scene_7, s_13, v_11, energy_12, &(picked_rng_0));
    VectorOutputs_put_4(&(outputs_1), _S74.radiance_1);
    VectorOutputs_put_0(&(outputs_1), picked_rng_0);
    var event_rng_0 : u32 = rng_6;
    var _S75 : vec3<f32> = next_event_0(scene_7, s_13, v_11, energy_12, &(event_rng_0));
    VectorOutputs_put_4(&(outputs_1), _S75);
    VectorOutputs_put_0(&(outputs_1), event_rng_0);
    var white_0 : TracedSurface_0 = furnace_surface_0(scene_7, s_13);
    VectorOutputs_put_6(&(outputs_1), vec4<f32>(white_0.albedo_0.x, white_0.emissive_0.x, white_0.metallic_0, white_0.roughness_0));
    VectorOutputs_put_2(&(outputs_1), f32(white_0.unlit_0));
    VectorOutputs_put_4(&(outputs_1), path_miss_0(scene_7, _S70, u32(0)));
    VectorOutputs_put_4(&(outputs_1), path_miss_0(scene_7, _S70, u32(1)));
    var depth_1 : PathDepth_0 = no_depth_0();
    VectorOutputs_put_0(&(outputs_1), select(u32(0), u32(1), depth_1.wanted_0) + u32(2) * select(u32(0), u32(1), depth_1.hit_0));
    depth_1.wanted_0 = probe_0;
    depth_1.ray_0 = camera_1;
    var path_rng_0 : u32 = rng_6;
    var _S76 : vec3<f32> = trace_path_0(scene_7, camera_1, max_bounces_1, &(path_rng_0), &(depth_1));
    VectorOutputs_put_4(&(outputs_1), _S76);
    VectorOutputs_put_0(&(outputs_1), path_rng_0);
    VectorOutputs_put_0(&(outputs_1), select(u32(0), u32(1), depth_1.hit_0));
    VectorOutputs_put_4(&(outputs_1), depth_1.position_2);
    return;
}

