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
    @align(16) view_proj_0 : _MatrixStorage_float4x4_ColMajorstd140_0,
    @align(16) lights_0 : vec4<u32>,
};

@binding(0) @group(0) var<uniform> traceFrame_0 : TraceFrame_std140_0;
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

@binding(0) @group(2) var<storage, read> tracerLights_0 : array<vec4<f32>>;

@binding(1) @group(0) var<storage, read_write> traceAccumulation_0 : array<vec4<f32>>;

@binding(1) @group(2) var tracerEnvironment_0 : texture_cube<f32>;

@binding(2) @group(2) var tracerIrradiance_0 : texture_cube<f32>;

@binding(3) @group(2) var tracerPrefiltered_0 : texture_cube<f32>;

@binding(4) @group(2) var tracerBrdfLut_0 : texture_2d<f32>;

@binding(5) @group(2) var tracerIblSampler_0 : sampler;

fn TracerAmbientMaps_energy_0( nDotV_0 : f32,  roughness_0 : f32) -> vec2<f32>
{
    return (textureSampleLevel((tracerBrdfLut_0), (tracerIblSampler_0), (vec2<f32>(nDotV_0, roughness_0)), (0.0f))).xy;
}

fn TracerAmbientMaps_irradiance_0( n_0 : vec3<f32>) -> vec3<f32>
{
    return (textureSampleLevel((tracerIrradiance_0), (tracerIblSampler_0), (n_0), (0.0f))).xyz;
}

fn TracerAmbientMaps_reflection_0( r_0 : vec3<f32>,  roughness_1 : f32) -> vec3<f32>
{
    return (textureSampleLevel((tracerPrefiltered_0), (tracerIblSampler_0), (r_0), (roughness_1 * f32(traceFrame_0.lights_0.z)))).xyz;
}

fn multiscatter_gain_0( energy_0 : vec2<f32>,  f0_0 : vec3<f32>) -> vec3<f32>
{
    return vec3<f32>(1.0f) + f0_0 * vec3<f32>((1.0f / max(energy_0.x + energy_0.y, 0.00100000004749745f) - 1.0f));
}

fn specular_albedo_0( energy_1 : vec2<f32>,  f0_1 : vec3<f32>) -> vec3<f32>
{
    return multiscatter_gain_0(energy_1, f0_1) * (f0_1 * vec3<f32>(energy_1.x) + vec3<f32>(energy_1.y));
}

fn ambient_split_sum_0( irradiance_0 : vec3<f32>,  reflection_0 : vec3<f32>,  albedo_0 : vec3<f32>,  metallic_0 : f32,  f0_2 : vec3<f32>,  energy_2 : vec2<f32>) -> vec3<f32>
{
    var specularAlbedo_0 : vec3<f32> = specular_albedo_0(energy_2, f0_2);
    return saturate(vec3<f32>(1.0f) - specularAlbedo_0) * vec3<f32>((1.0f - metallic_0)) * albedo_0 * irradiance_0 + reflection_0 * specularAlbedo_0;
}

fn hemisphere_irradiance_0( n_1 : vec3<f32>) -> vec3<f32>
{
    var _S1 : f32 = n_1.y;
    return mix(vec3<f32>(0.18000000715255737f, 0.15999999642372131f, 0.15000000596046448f) * vec3<f32>(0.69999998807907104f), mix(vec3<f32>(0.55000001192092896f, 0.62000000476837158f, 0.72000002861022949f), vec3<f32>(0.09000000357627869f, 0.15999999642372131f, 0.34999999403953552f), vec3<f32>(pow(clamp(_S1, 0.0f, 1.0f), 0.69999998807907104f))), vec3<f32>(clamp(_S1 * 0.5f + 0.5f, 0.0f, 1.0f)));
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

fn analytic_ambient_0( n_2 : vec3<f32>,  v_0 : vec3<f32>,  albedo_1 : vec3<f32>,  metallic_1 : f32,  roughness_2 : f32,  f0_3 : vec3<f32>,  energy_3 : vec2<f32>) -> vec3<f32>
{
    var irradiance_1 : vec3<f32> = hemisphere_irradiance_0(n_2);
    return ambient_split_sum_0(irradiance_1, mix(sky_gradient_0(reflect((vec3<f32>(0) - v_0), n_2)), irradiance_1, vec3<f32>((roughness_2 * roughness_2))), albedo_1, metallic_1, f0_3, energy_3);
}

fn sky_ambient_0( baked_0 : bool,  n_3 : vec3<f32>,  v_1 : vec3<f32>,  albedo_2 : vec3<f32>,  metallic_2 : f32,  roughness_3 : f32,  f0_4 : vec3<f32>,  surfaceEnergy_0 : vec2<f32>) -> vec3<f32>
{
    if(baked_0)
    {
        return ambient_split_sum_0(TracerAmbientMaps_irradiance_0(n_3), TracerAmbientMaps_reflection_0(reflect((vec3<f32>(0) - v_1), n_3), roughness_3), albedo_2, metallic_2, f0_4, TracerAmbientMaps_energy_0(max(dot(n_3, v_1), 0.0f), roughness_3));
    }
    return analytic_ambient_0(n_3, v_1, albedo_2, metallic_2, roughness_3, f0_4, surfaceEnergy_0);
}

var<private> GGX_ENERGY_COEFFICIENTS_0 : array<vec2<f32>, i32(196)> = array<vec2<f32>, i32(196)>( vec2<f32>(0.03755360096693039f, 3.05165600776672363f), vec2<f32>(0.03963959962129593f, 0.65487277507781982f), vec2<f32>(0.07296779751777649f, 0.51261270046234131f), vec2<f32>(0.17118540406227112f, 0.62788808345794678f), vec2<f32>(0.30981320142745972f, 0.50604218244552612f), vec2<f32>(0.44191190600395203f, 0.40881091356277466f), vec2<f32>(0.55946820974349976f, 0.28783369064331055f), vec2<f32>(0.65159928798675537f, 0.20042069256305695f), vec2<f32>(0.70950692892074585f, 0.12844389677047729f), vec2<f32>(0.73192507028579712f, 0.08202389627695084f), vec2<f32>(0.7161983847618103f, 0.05005419999361038f), vec2<f32>(0.66943138837814331f, 0.03112700022757053f), vec2<f32>(0.59648841619491577f, 0.01862730085849762f), vec2<f32>(0.50741302967071533f, 0.01281030010432005f), vec2<f32>(-0.00930839963257313f, -0.05096510052680969f), vec2<f32>(0.03267690166831017f, 0.88316977024078369f), vec2<f32>(0.11045809835195541f, 0.81648111343383789f), vec2<f32>(0.20676289498806f, 0.65160852670669556f), vec2<f32>(0.32054820656776428f, 0.53221142292022705f), vec2<f32>(0.4455254077911377f, 0.40326109528541565f), vec2<f32>(0.56171208620071411f, 0.2928445041179657f), vec2<f32>(0.65319478511810303f, 0.19869279861450195f), vec2<f32>(0.71095079183578491f, 0.12985430657863617f), vec2<f32>(0.7316511869430542f, 0.08150120079517365f), vec2<f32>(0.71713531017303467f, 0.05052080005407333f), vec2<f32>(0.66998302936553955f, 0.03087439946830273f), vec2<f32>(0.59695768356323242f, 0.01905369944870472f), vec2<f32>(0.50688481330871582f, 0.01136360038071871f), vec2<f32>(0.010585299693048f, 0.99188041687011719f), vec2<f32>(0.00207319995388389f, 0.40556570887565613f), vec2<f32>(0.05862849950790405f, 0.55259382724761963f), vec2<f32>(0.16572730243206024f, 0.58792608976364136f), vec2<f32>(0.29784908890724182f, 0.50322818756103516f), vec2<f32>(0.4318917989730835f, 0.39728268980979919f), vec2<f32>(0.55288940668106079f, 0.2878929078578949f), vec2<f32>(0.64753270149230957f, 0.19779230654239655f), vec2<f32>(0.70724570751190186f, 0.12865540385246277f), vec2<f32>(0.72979277372360229f, 0.08128180354833603f), vec2<f32>(0.71553927659988403f, 0.05013220012187958f), vec2<f32>(0.66890257596969604f, 0.03081009909510612f), vec2<f32>(0.59605568647384644f, 0.01881019957363605f), vec2<f32>(0.50637918710708618f, 0.01177479978650808f), vec2<f32>(0.04574630036950111f, 1.02595400810241699f), vec2<f32>(0.03743439912796021f, 0.88119810819625854f), vec2<f32>(0.04426249861717224f, 0.52496278285980225f), vec2<f32>(0.11310549825429916f, 0.46600010991096497f), vec2<f32>(0.23485749959945679f, 0.4375700056552887f), vec2<f32>(0.37963598966598511f, 0.36149460077285767f), vec2<f32>(0.51556599140167236f, 0.27323558926582336f), vec2<f32>(0.622580885887146f, 0.18986819684505463f), vec2<f32>(0.69136160612106323f, 0.12518900632858276f), vec2<f32>(0.71929609775543213f, 0.07907170057296753f), vec2<f32>(0.70917361974716187f, 0.04900079965591431f), vec2<f32>(0.66434282064437866f, 0.02995469979941845f), vec2<f32>(0.59224581718444824f, 0.01837039925158024f), vec2<f32>(0.5022997260093689f, 0.0110470000654459f), vec2<f32>(0.1206125020980835f, 0.77379542589187622f), vec2<f32>(0.12222660332918167f, 0.83616697788238525f), vec2<f32>(0.11106769740581512f, 0.74211102724075317f), vec2<f32>(0.12635010480880737f, 0.50614690780639648f), vec2<f32>(0.20402370393276215f, 0.39524701237678528f), vec2<f32>(0.32609659433364868f, 0.32315808534622192f), vec2<f32>(0.46191799640655518f, 0.24740250408649445f), vec2<f32>(0.57957160472869873f, 0.17592360079288483f), vec2<f32>(0.66025441884994507f, 0.11711639910936356f), vec2<f32>(0.69881141185760498f, 0.07462230324745178f), vec2<f32>(0.69453942775726318f, 0.04620540142059326f), vec2<f32>(0.65352827310562134f, 0.02824320085346699f), vec2<f32>(0.582955002784729f, 0.01713629998266697f), vec2<f32>(0.49382159113883972f, 0.01042309962213039f), vec2<f32>(0.26457729935646057f, 0.79617220163345337f), vec2<f32>(0.25447279214859009f, 0.71848297119140625f), vec2<f32>(0.24372699856758118f, 0.67536389827728271f), vec2<f32>(0.22911170125007629f, 0.58072292804718018f), vec2<f32>(0.24631629884243011f, 0.4128555953502655f), vec2<f32>(0.32152768969535828f, 0.3022313117980957f), vec2<f32>(0.42885839939117432f, 0.22238540649414062f), vec2<f32>(0.53810352087020874f, 0.157231405377388f), vec2<f32>(0.6227080225944519f, 0.10510919988155365f), vec2<f32>(0.66763401031494141f, 0.0672312006354332f), vec2<f32>(0.67200499773025513f, 0.04161379858851433f), vec2<f32>(0.63588422536849976f, 0.02530450001358986f), vec2<f32>(0.56762188673019409f, 0.01518450025469065f), vec2<f32>(0.47794249653816223f, 0.00909390021115541f), vec2<f32>(0.43767940998077393f, 0.52778768539428711f), vec2<f32>(0.43497830629348755f, 0.55565857887268066f), vec2<f32>(0.42141589522361755f, 0.53836488723754883f), vec2<f32>(0.4008432924747467f, 0.49193951487541199f), vec2<f32>(0.37870550155639648f, 0.40763860940933228f), vec2<f32>(0.38813981413841248f, 0.28910300135612488f), vec2<f32>(0.44539439678192139f, 0.20043960213661194f), vec2<f32>(0.52257758378982544f, 0.13623130321502686f), vec2<f32>(0.59163397550582886f, 0.08995489776134491f), vec2<f32>(0.63798898458480835f, 0.05706769973039627f), vec2<f32>(0.64467430114746094f, 0.03520869836211205f), vec2<f32>(0.61179417371749878f, 0.021161999553442f), vec2<f32>(0.5443912148475647f, 0.01255400013178587f), vec2<f32>(0.45556539297103882f, 0.00719430018216372f), vec2<f32>(0.63681560754776001f, 0.37943580746650696f), vec2<f32>(0.63195270299911499f, 0.36018449068069458f), vec2<f32>(0.62122827768325806f, 0.35363009572029114f), vec2<f32>(0.59902858734130859f, 0.34027791023254395f), vec2<f32>(0.56643378734588623f, 0.30496558547019958f), vec2<f32>(0.5314096212387085f, 0.24438419938087463f), vec2<f32>(0.52357661724090576f, 0.16980840265750885f), vec2<f32>(0.5474010705947876f, 0.11221110075712204f), vec2<f32>(0.58856910467147827f, 0.07183969765901566f), vec2<f32>(0.61636549234390259f, 0.04495130106806755f), vec2<f32>(0.61752068996429443f, 0.02730320021510124f), vec2<f32>(0.58241552114486694f, 0.01623369939625263f), vec2<f32>(0.51425069570541382f, 0.00938300043344498f), vec2<f32>(0.42317068576812744f, 0.00540499994531274f), vec2<f32>(0.81267541646957397f, 0.17737489938735962f), vec2<f32>(0.81089460849761963f, 0.18592619895935059f), vec2<f32>(0.80203372240066528f, 0.18450069427490234f), vec2<f32>(0.78451091051101685f, 0.17923450469970703f), vec2<f32>(0.75236481428146362f, 0.17097190022468567f), vec2<f32>(0.70702672004699707f, 0.15113990008831024f), vec2<f32>(0.65327489376068115f, 0.11831910163164139f), vec2<f32>(0.62291878461837769f, 0.08054299652576447f), vec2<f32>(0.61463671922683716f, 0.05135539919137955f), vec2<f32>(0.61291992664337158f, 0.03151309862732887f), vec2<f32>(0.59584552049636841f, 0.01889899931848049f), vec2<f32>(0.5510483980178833f, 0.01100130006670952f), vec2<f32>(0.4772191047668457f, 0.00626480020582676f), vec2<f32>(0.38401719927787781f, 0.0033825000282377f), vec2<f32>(0.93769299983978271f, 0.06798700243234634f), vec2<f32>(0.93442130088806152f, 0.06284449994564056f), vec2<f32>(0.92932981252670288f, 0.06249019876122475f), vec2<f32>(0.91649138927459717f, 0.06285469979047775f), vec2<f32>(0.89388298988342285f, 0.06178810074925423f), vec2<f32>(0.85349130630493164f, 0.06024239957332611f), vec2<f32>(0.79553717374801636f, 0.05434289947152138f), vec2<f32>(0.72840338945388794f, 0.04295980185270309f), vec2<f32>(0.67323261499404907f, 0.02926450036466122f), vec2<f32>(0.63030177354812622f, 0.01836870051920414f), vec2<f32>(0.58399122953414917f, 0.01094940025359392f), vec2<f32>(0.5202292799949646f, 0.00631939992308617f), vec2<f32>(0.43539279699325562f, 0.0035077000502497f), vec2<f32>(0.33793559670448303f, 0.00192960002459586f), vec2<f32>(0.99163192510604858f, 0.00484689977020025f), vec2<f32>(0.99116522073745728f, 0.00796679966151714f), vec2<f32>(0.98792862892150879f, 0.00806930009275675f), vec2<f32>(0.98114508390426636f, 0.0078320000320673f), vec2<f32>(0.96755552291870117f, 0.00866509974002838f), vec2<f32>(0.94221091270446777f, 0.00961260031908751f), vec2<f32>(0.89904522895812988f, 0.0111905001103878f), vec2<f32>(0.83300900459289551f, 0.01175749953836203f), vec2<f32>(0.75132989883422852f, 0.01031319983303547f), vec2<f32>(0.66817468404769897f, 0.00739389983937144f), vec2<f32>(0.58449500799179077f, 0.00469669979065657f), vec2<f32>(0.49239379167556763f, 0.00274799996986985f), vec2<f32>(0.39069679379463196f, 0.00153979996684939f), vec2<f32>(0.28739228844642639f, 0.00080069998512045f), vec2<f32>(1.00030839443206787f, 0.00132489996030927f), vec2<f32>(1.00059950351715088f, -0.00104150001425296f), vec2<f32>(0.99918842315673828f, -0.00109509995672852f), vec2<f32>(0.99662727117538452f, -0.00078689999645576f), vec2<f32>(0.99067902565002441f, -0.00099460000637919f), vec2<f32>(0.9789891242980957f, -0.00078649999340996f), vec2<f32>(0.95506691932678223f, -0.00063089997274801f), vec2<f32>(0.90853261947631836f, -0.00004849999822909f), vec2<f32>(0.83005660772323608f, 0.00064739998197183f), vec2<f32>(0.72199207544326782f, 0.00106369995046407f), vec2<f32>(0.59814202785491943f, 0.00094429997261614f), vec2<f32>(0.46943360567092896f, 0.0006571999983862f), vec2<f32>(0.34491020441055298f, 0.00038939999649301f), vec2<f32>(0.23603999614715576f, 0.00023839999630582f), vec2<f32>(0.99982082843780518f, -0.00226259999908507f), vec2<f32>(0.99921691417694092f, 0.00083259999519214f), vec2<f32>(0.99951702356338501f, 0.00091000000247732f), vec2<f32>(0.99938511848449707f, 0.00050560000818223f), vec2<f32>(0.9986388087272644f, 0.00080109998816624f), vec2<f32>(0.99591022729873657f, 0.00060789997223765f), vec2<f32>(0.98590642213821411f, 0.00062870001420379f), vec2<f32>(0.95896798372268677f, 0.00030819998937659f), vec2<f32>(0.89731162786483765f, -0.00005109999983688f), vec2<f32>(0.78470510244369507f, -0.00027940000290982f), vec2<f32>(0.62571001052856445f, -0.00019700000120793f), vec2<f32>(0.45251798629760742f, -0.00010209999891231f), vec2<f32>(0.29973480105400085f, -0.00003329999890411f), vec2<f32>(0.18639770150184631f, -0.000034600001527f), vec2<f32>(1.00496220588684082f, 0.00913080014288425f), vec2<f32>(1.00138688087463379f, -0.00241709989495575f), vec2<f32>(1.0036776065826416f, -0.0027135000564158f), vec2<f32>(1.00481629371643066f, -0.00120069994591177f), vec2<f32>(1.008644700050354f, -0.00232520001009107f), vec2<f32>(1.01138985157012939f, -0.00167589995544404f), vec2<f32>(1.01234376430511475f, -0.00196720007807016f), vec2<f32>(0.99929207563400269f, -0.00119440001435578f), vec2<f32>(0.95524948835372925f, -0.00035449999268167f), vec2<f32>(0.85058838129043579f, 0.00027970000519417f), vec2<f32>(0.6671786904335022f, 0.00011629999789875f), vec2<f32>(0.44304949045181274f, 0.00001530000008643f), vec2<f32>(0.25579911470413208f, -0.00005640000017593f), vec2<f32>(0.14276920258998871f, 0.00004659999831347f) );
fn tile_pixel_0( thread_0 : vec2<u32>,  pixel_0 : ptr<function, vec2<u32>>) -> bool
{
    (*pixel_0) = traceTile_0.rect_0.xy + thread_0;
    var _S3 : bool;
    if((thread_0.x) < (traceTile_0.rect_0.z))
    {
        _S3 = (thread_0.y) < (traceTile_0.rect_0.w);
    }
    else
    {
        _S3 = false;
    }
    return _S3;
}

fn fullscreen_uv_to_ndc_0( uv_0 : vec2<f32>) -> vec2<f32>
{
    return vec2<f32>(uv_0.x * 2.0f - 1.0f, 1.0f - uv_0.y * 2.0f);
}

struct CameraRay_0
{
     origin_0 : vec3<f32>,
     direction_0 : vec3<f32>,
     length_0 : f32,
};

fn camera_ray_0( clip_to_world_0 : mat4x4<f32>,  ndc_0 : vec2<f32>) -> CameraRay_0
{
    var near_0 : vec4<f32> = (((vec4<f32>(ndc_0, 0.0f, 1.0f)) * (clip_to_world_0)));
    var far_0 : vec4<f32> = (((vec4<f32>(ndc_0, 1.0f, 1.0f)) * (clip_to_world_0)));
    var ray_0 : CameraRay_0;
    var _S4 : vec3<f32> = near_0.xyz / vec3<f32>(near_0.w);
    ray_0.origin_0 = _S4;
    var span_0 : vec3<f32> = far_0.xyz / vec3<f32>(far_0.w) - _S4;
    ray_0.direction_0 = normalize(span_0);
    ray_0.length_0 = length(span_0);
    return ray_0;
}

fn camera_ray_through_pixel_0( clip_to_world_1 : mat4x4<f32>,  pixel_1 : vec2<f32>,  size_1 : vec2<f32>) -> CameraRay_0
{
    return camera_ray_0(clip_to_world_1, fullscreen_uv_to_ndc_0(pixel_1 / size_1));
}

struct TraceRay_0
{
     origin_1 : vec3<f32>,
     tmin_0 : f32,
     direction_1 : vec3<f32>,
     tmax_0 : f32,
};

fn pixel_ray_0( pixel_2 : vec2<f32>) -> TraceRay_0
{
    var camera_0 : CameraRay_0 = camera_ray_through_pixel_0(mat4x4<f32>(traceFrame_0.inv_view_proj_0.data_0[i32(0)][i32(0)], traceFrame_0.inv_view_proj_0.data_0[i32(1)][i32(0)], traceFrame_0.inv_view_proj_0.data_0[i32(2)][i32(0)], traceFrame_0.inv_view_proj_0.data_0[i32(3)][i32(0)], traceFrame_0.inv_view_proj_0.data_0[i32(0)][i32(1)], traceFrame_0.inv_view_proj_0.data_0[i32(1)][i32(1)], traceFrame_0.inv_view_proj_0.data_0[i32(2)][i32(1)], traceFrame_0.inv_view_proj_0.data_0[i32(3)][i32(1)], traceFrame_0.inv_view_proj_0.data_0[i32(0)][i32(2)], traceFrame_0.inv_view_proj_0.data_0[i32(1)][i32(2)], traceFrame_0.inv_view_proj_0.data_0[i32(2)][i32(2)], traceFrame_0.inv_view_proj_0.data_0[i32(3)][i32(2)], traceFrame_0.inv_view_proj_0.data_0[i32(0)][i32(3)], traceFrame_0.inv_view_proj_0.data_0[i32(1)][i32(3)], traceFrame_0.inv_view_proj_0.data_0[i32(2)][i32(3)], traceFrame_0.inv_view_proj_0.data_0[i32(3)][i32(3)]), pixel_2, vec2<f32>(traceFrame_0.size_0.xy));
    var ray_1 : TraceRay_0;
    ray_1.origin_1 = camera_0.origin_0;
    ray_1.tmin_0 = 0.0f;
    ray_1.direction_1 = camera_0.direction_0;
    ray_1.tmax_0 = camera_0.length_0;
    return ray_1;
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
    var _S5 : f32;
    if((abs(c_0)) > 1.00000000317107685e-30f)
    {
        _S5 = c_0;
    }
    else
    {
        if(c_0 < 0.0f)
        {
            _S5 = -1.00000000317107685e-30f;
        }
        else
        {
            _S5 = 1.00000000317107685e-30f;
        }
    }
    return _S5;
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
    var a_0 : vec4<f32> = tracerRecords_0[index_0];
    var b_0 : vec4<f32> = tracerNodeMax_0[index_0];
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
    var _S6 : f32 = max(max(lo_1.x, lo_1.y), max(lo_1.z, tmin_1));
    var _S7 : f32;
    if(_S6 <= (min(min(hi_1.x, hi_1.y), min(hi_1.z, tmax_2)) * 1.00000035762786865f))
    {
        _S7 = _S6;
    }
    else
    {
        _S7 = 3.00000000549775576e+38f;
    }
    return _S7;
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
    inst_0.world_to_object_0[i32(0)] = tracerRecords_0[base_0];
    inst_0.world_to_object_0[i32(1)] = tracerRecords_0[base_0 + u32(1)];
    inst_0.world_to_object_0[i32(2)] = tracerRecords_0[base_0 + u32(2)];
    inst_0.object_to_world_0[i32(0)] = tracerRecords_0[base_0 + u32(3)];
    inst_0.object_to_world_0[i32(1)] = tracerRecords_0[base_0 + u32(4)];
    inst_0.object_to_world_0[i32(2)] = tracerRecords_0[base_0 + u32(5)];
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

fn transform_vector_0( rows_1 : array<vec4<f32>, i32(3)>,  v_2 : vec3<f32>) -> vec3<f32>
{
    return vec3<f32>(dot(rows_1[i32(0)].xyz, v_2), dot(rows_1[i32(1)].xyz, v_2), dot(rows_1[i32(2)].xyz, v_2));
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
    var _S8 : bool;
    if(u_0 < 0.0f)
    {
        _S8 = true;
    }
    else
    {
        _S8 = u_0 > 1.0f;
    }
    if(_S8)
    {
        return miss_0;
    }
    var q_0 : vec3<f32> = cross(s_0, e1_0);
    var v_3 : f32 = dot(d_1, q_0) * inv_1;
    if(v_3 < 0.0f)
    {
        _S8 = true;
    }
    else
    {
        _S8 = (u_0 + v_3) > 1.0f;
    }
    if(_S8)
    {
        return miss_0;
    }
    return vec3<f32>(dot(e2_0, q_0) * inv_1, u_0, v_3);
}

fn trace_closer_0( t_1 : f32,  instance_1 : u32,  triangle_1 : u32,  best_0 : TraceHit_0) -> bool
{
    var _S9 : bool;
    if(t_1 < (best_0.t_0))
    {
        _S9 = true;
    }
    else
    {
        if(t_1 == (best_0.t_0))
        {
            if(instance_1 < (best_0.instance_0))
            {
                _S9 = true;
            }
            else
            {
                if(instance_1 == (best_0.instance_0))
                {
                    _S9 = triangle_1 < (best_0.triangle_0);
                }
                else
                {
                    _S9 = false;
                }
            }
        }
        else
        {
            _S9 = false;
        }
    }
    return _S9;
}

struct TraceMaterial_0
{
     base_color_0 : vec4<f32>,
     emissive_0 : vec3<f32>,
     alpha_cutoff_0 : f32,
     metallic_3 : f32,
     roughness_4 : f32,
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
    m_0.metallic_3 = factors_0.x;
    m_0.roughness_4 = factors_0.y;
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
    var _S10 : f32 = e_0.x;
    var _S11 : f32 = e_0.y;
    var _S12 : f32 = 1.0f - abs(_S10) - abs(_S11);
    var v_4 : vec3<f32> = vec3<f32>(_S10, _S11, _S12);
    var _S13 : f32 = max(- _S12, 0.0f);
    var _S14 : f32;
    if(_S10 >= 0.0f)
    {
        _S14 = - _S13;
    }
    else
    {
        _S14 = _S13;
    }
    v_4[i32(0)] = v_4[i32(0)] + _S14;
    if((v_4.y) >= 0.0f)
    {
        _S14 = - _S13;
    }
    else
    {
        _S14 = _S13;
    }
    v_4[i32(1)] = v_4[i32(1)] + _S14;
    return normalize(v_4);
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
    var a_1 : vec4<u32> = tracerVertices_0[index_3];
    var b_1 : vec4<u32> = tracerVertexExtras_0[index_3];
    var v_5 : TraceVertex_0;
    v_5.normal_0 = oct_decode_0(unpack_snorm2_0(a_1.x));
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
    v_5.tangent_0 = vec4<f32>(oct_decode_0(unpack_snorm2_0((_S15 & (u32(4294901759))))), handedness_0);
    v_5.uv0_0 = (bitcast<vec2<f32>>((a_1.zw)));
    v_5.uv1_0 = (bitcast<vec2<f32>>((b_1.xy)));
    v_5.color_0 = vec4<f32>(unpack_unorm2_0(b_1.z), unpack_unorm2_0(b_1.w));
    return v_5;
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

fn transform_uv_0( uv_1 : vec2<f32>,  row0_0 : vec3<f32>,  row1_0 : vec3<f32>) -> vec2<f32>
{
    var _S16 : vec3<f32> = vec3<f32>(uv_1, 1.0f);
    return vec2<f32>(dot(_S16, row0_0), dot(_S16, row1_0));
}

fn material_slot_uv_0( material_0 : u32,  m_2 : TraceMaterial_0,  slot_1 : u32,  uv0_1 : vec2<f32>,  uv1_1 : vec2<f32>) -> vec2<f32>
{
    var uv_2 : vec2<f32>;
    if((((((m_2.flags_2) >> ((u32(4) + slot_1)))) & (u32(1)))) != u32(0))
    {
        uv_2 = uv1_1;
    }
    else
    {
        uv_2 = uv0_1;
    }
    var base_2 : u32 = tracerInfo_0.material_base_0 + material_0 * u32(14) + u32(4) + u32(2) * slot_1;
    return transform_uv_0(uv_2, tracerRecords_0[base_2].xyz, tracerRecords_0[base_2 + u32(1)].xyz);
}

fn wrap_coordinate_0( c_1 : f32,  mode_0 : u32,  half_texel_0 : f32) -> f32
{
    var _S17 : bool = mode_0 == u32(1);
    var _S18 : f32;
    if(_S17)
    {
        _S18 = 1.0f - abs(fract(c_1 * 0.5f) * 2.0f - 1.0f);
    }
    else
    {
        _S18 = c_1;
    }
    var _S19 : bool;
    if(_S17)
    {
        _S19 = true;
    }
    else
    {
        _S19 = mode_0 == u32(2);
    }
    if(_S19)
    {
        _S18 = clamp(_S18, half_texel_0, 1.0f - half_texel_0);
    }
    return _S18;
}

fn sample_material_slot_0( material_1 : u32,  m_3 : TraceMaterial_0,  slot_2 : u32,  uv0_2 : vec2<f32>,  uv1_2 : vec2<f32>,  lod_0 : f32) -> vec4<f32>
{
    var layer_0 : u32 = material_layer_0(m_3, slot_2);
    var _S20 : vec4<f32>;
    if(layer_0 == u32(65535))
    {
        if(slot_2 == u32(2))
        {
            _S20 = vec4<f32>(0.5f, 0.5f, 1.0f, 1.0f);
        }
        else
        {
            _S20 = vec4<f32>(1.0f, 1.0f, 1.0f, 1.0f);
        }
        return _S20;
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
    var _S21 : u32;
    if(srgb_0)
    {
        _S21 = tracerInfo_0.srgb_layer_size_0;
    }
    else
    {
        _S21 = tracerInfo_0.linear_layer_size_0;
    }
    var half_texel_1 : f32 = 0.5f * exp2(max(lod_0, 0.0f)) / f32(_S21);
    var wrap_0 : u32 = ((m_3.flags_2) >> ((u32(9) + u32(4) * slot_2)));
    var uv_3 : vec2<f32> = material_slot_uv_0(material_1, m_3, slot_2, uv0_2, uv1_2);
    var coord_0 : vec3<f32> = vec3<f32>(wrap_coordinate_0(uv_3.x, (wrap_0 & (u32(3))), half_texel_1), wrap_coordinate_0(uv_3.y, (((wrap_0 >> (u32(2)))) & (u32(3))), half_texel_1), f32(layer_0));
    var nearest_0 : bool = (((((m_3.layers_0.z) >> ((u32(16) + slot_2)))) & (u32(1)))) != u32(0);
    if(srgb_0)
    {
        if(nearest_0)
        {
            _S20 = (textureSampleLevel((tracerSrgbLayers_0), (tracerNearestSampler_0), ((coord_0)).xy, i32(((coord_0)).z), (lod_0)));
        }
        else
        {
            _S20 = (textureSampleLevel((tracerSrgbLayers_0), (tracerLinearSampler_0), ((coord_0)).xy, i32(((coord_0)).z), (lod_0)));
        }
        return _S20;
    }
    if(nearest_0)
    {
        _S20 = (textureSampleLevel((tracerLinearLayers_0), (tracerNearestSampler_0), ((coord_0)).xy, i32(((coord_0)).z), (lod_0)));
    }
    else
    {
        _S20 = (textureSampleLevel((tracerLinearLayers_0), (tracerLinearSampler_0), ((coord_0)).xy, i32(((coord_0)).z), (lod_0)));
    }
    return _S20;
}

fn alpha_masked_out_0( alpha_mode_0 : i32,  alpha_0 : f32,  cutoff_0 : f32) -> bool
{
    var _S22 : bool;
    if(alpha_mode_0 == i32(1))
    {
        _S22 = alpha_0 < cutoff_0;
    }
    else
    {
        _S22 = false;
    }
    return _S22;
}

fn hit_masked_out_0( tri_0 : vec4<u32>,  barycentrics_1 : vec2<f32>) -> bool
{
    var material_2 : u32 = ((tri_0.w) & (u32(16777215)));
    var m_4 : TraceMaterial_0 = load_material_0(material_2);
    var mode_1 : i32 = i32(((m_4.flags_2) & (u32(3))));
    if(mode_1 != i32(1))
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
    return alpha_masked_out_0(mode_1, m_4.base_color_0.w * (a_2.color_0.w * _S25 + b_2.color_0.w * _S23 + c_2.color_0.w * _S24) * sample_material_slot_0(material_2, m_4, u32(0), a_2.uv0_0 * _S26 + b_2.uv0_0 * _S27 + c_2.uv0_0 * _S28, a_2.uv1_0 * _S26 + b_2.uv1_0 * _S27 + c_2.uv1_0 * _S28, 0.0f).w, m_4.alpha_cutoff_0);
}

fn trace_scene_0( ray_2 : TraceRay_0,  any_hit_0 : bool) -> TraceHit_0
{
    var origin_3 : vec3<f32>;
    var inv_2 : vec3<f32>;
    var direction_2 : vec3<f32>;
    var blas_floor_0 : u32;
    var best_1 : TraceHit_0 = trace_miss_0(ray_2.tmax_0);
    if((tracerInfo_0.instance_count_0) == u32(0))
    {
        return best_1;
    }
    var inv_3 : vec3<f32> = safe_inverse_0(ray_2.direction_1);
    var stack_0 : array<u32, i32(96)>;
    var node_2 : BvhNode_0 = load_node_0(tracerInfo_0.tlas_root_0);
    var live_0 : bool = (box_entry_0(node_2, ray_2.origin_1, inv_3, ray_2.tmin_0, best_1.t_0)) < 3.00000000549775576e+38f;
    var node_3 : BvhNode_0 = node_2;
    var origin_4 : vec3<f32> = ray_2.origin_1;
    var inv_4 : vec3<f32> = inv_3;
    var blas_floor_1 : u32 = u32(4294967295);
    var direction_3 : vec3<f32> = ray_2.direction_1;
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
        var direction_4 : vec3<f32>;
        var blas_floor_2 : u32;
        var blas_floor_3 : u32;
        if((node_3.count_0) == u32(0))
        {
            var _S31 : BvhNode_0 = node_3;
            var a_3 : BvhNode_0 = load_node_0(node_3.first_0);
            var _S32 : u32 = node_3.first_0 + u32(1);
            var b_3 : BvhNode_0 = load_node_0(_S32);
            var ta_0 : f32 = box_entry_0(a_3, origin_4, inv_4, ray_2.tmin_0, best_1.t_0);
            var tb_0 : f32 = box_entry_0(b_3, origin_4, inv_4, ray_2.tmin_0, best_1.t_0);
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
            direction_4 = direction_3;
        }
        else
        {
            if(blas_floor_1 == u32(4294967295))
            {
                var _S33 : BvhNode_0 = node_3;
                var inst_1 : TraceInstance_0 = load_instance_0(node_3.first_0);
                var origin_6 : vec3<f32> = transform_point_0(inst_1.world_to_object_0, ray_2.origin_1);
                var direction_5 : vec3<f32> = transform_vector_0(inst_1.world_to_object_0, ray_2.direction_1);
                var inv_6 : vec3<f32> = safe_inverse_0(direction_5);
                var node_5 : BvhNode_0 = load_node_0(inst_1.blas_root_0);
                if((box_entry_0(node_5, origin_6, inv_6, ray_2.tmin_0, best_1.t_0)) < 3.00000000549775576e+38f)
                {
                    node_3 = node_5;
                    origin_4 = origin_6;
                    inv_4 = inv_6;
                    blas_floor_1 = sp_0;
                    direction_3 = direction_5;
                    instance_2 = _S33.first_0;
                    continue;
                }
                blas_floor_2 = sp_0;
                origin_5 = origin_6;
                inv_5 = inv_6;
                node_4 = node_5;
                direction_4 = direction_5;
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
                    var hit_1 : vec3<f32> = intersect_triangle_0(origin_4, direction_3, trace_position_0(tri_1.x), trace_position_0(tri_1.y), trace_position_0(tri_1.z));
                    var _S34 : f32 = hit_1.x;
                    if(_S34 < (ray_2.tmin_0))
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
                        _S35 = hit_masked_out_0(tri_1, hit_1.yz);
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
                direction_4 = direction_3;
                blas_floor_3 = instance_2;
            }
            instance_2 = blas_floor_3;
        }
        blas_floor_3 = blas_floor_2;
        var origin_7 : vec3<f32> = origin_5;
        var inv_7 : vec3<f32> = inv_5;
        var node_6 : BvhNode_0 = node_4;
        var direction_6 : vec3<f32> = direction_4;
        var sp_1 : u32 = sp_0;
        for(;;)
        {
            if(sp_1 == blas_floor_3)
            {
                origin_3 = ray_2.origin_1;
                inv_2 = inv_3;
                blas_floor_0 = u32(4294967295);
                direction_2 = ray_2.direction_1;
            }
            else
            {
                origin_3 = origin_7;
                inv_2 = inv_7;
                blas_floor_0 = blas_floor_3;
                direction_2 = direction_6;
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
            if((box_entry_0(node_7, origin_3, inv_2, ray_2.tmin_0, best_1.t_0)) < 3.00000000549775576e+38f)
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
            direction_6 = direction_2;
            sp_1 = sp_2;
        }
        origin_4 = origin_3;
        inv_4 = inv_2;
        blas_floor_1 = blas_floor_0;
        direction_3 = direction_2;
    }
    return best_1;
}

fn trace_closest_0( ray_3 : TraceRay_0) -> TraceHit_0
{
    return trace_scene_0(ray_3, false);
}

fn sun_disk_0( dir_1 : vec3<f32>,  lightDir_0 : vec3<f32>,  intensity_0 : f32) -> vec3<f32>
{
    var _S36 : f32 = max(dot(dir_1, normalize(lightDir_0)), 0.0f);
    return vec3<f32>(1.0f, 0.94999998807907104f, 0.85000002384185791f) * vec3<f32>((pow(_S36, 1200.0f) * 24.0f + pow(_S36, 48.0f) * 0.5f)) * vec3<f32>((intensity_0 * 0.40000000596046448f));
}

fn sky_with_sun_0( dir_2 : vec3<f32>) -> vec3<f32>
{
    return sky_gradient_0(dir_2) + sun_disk_0(dir_2, traceFrame_0.light_dir_0.xyz, traceFrame_0.light_dir_0.w);
}

fn transform_normal_0( world_to_object_1 : array<vec4<f32>, i32(3)>,  n_4 : vec3<f32>) -> vec3<f32>
{
    return world_to_object_1[i32(0)].xyz * vec3<f32>(n_4.x) + world_to_object_1[i32(1)].xyz * vec3<f32>(n_4.y) + world_to_object_1[i32(2)].xyz * vec3<f32>(n_4.z);
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

fn trace_surface_0( ray_4 : TraceRay_0,  hit_2 : TraceHit_0) -> TraceSurface_0
{
    var tri_2 : vec4<u32> = tracerTriangles_0[hit_2.triangle_0];
    var inst_2 : TraceInstance_0 = load_instance_0(hit_2.instance_0);
    var _S37 : f32 = hit_2.barycentrics_0.x;
    var _S38 : f32 = hit_2.barycentrics_0.y;
    var _S39 : f32 = 1.0f - _S37 - _S38;
    var _S40 : u32 = tri_2.x;
    var p0_1 : vec3<f32> = trace_position_0(_S40);
    var _S41 : u32 = tri_2.y;
    var p1_1 : vec3<f32> = trace_position_0(_S41);
    var _S42 : u32 = tri_2.z;
    var p2_1 : vec3<f32> = trace_position_0(_S42);
    var a_4 : TraceVertex_0 = load_vertex_0(_S40);
    var b_4 : TraceVertex_0 = load_vertex_0(_S41);
    var c_3 : TraceVertex_0 = load_vertex_0(_S42);
    var s_1 : TraceSurface_0;
    s_1.position_0 = ray_4.origin_1 + ray_4.direction_1 * vec3<f32>(hit_2.t_0);
    s_1.geometric_normal_0 = normalize(transform_normal_0(inst_2.world_to_object_0, cross(p1_1 - p0_1, p2_1 - p0_1)));
    var n_5 : vec3<f32> = transform_normal_0(inst_2.world_to_object_0, a_4.normal_0 * vec3<f32>(_S39) + b_4.normal_0 * vec3<f32>(_S37) + c_3.normal_0 * vec3<f32>(_S38));
    var _S43 : vec3<f32>;
    if((dot(n_5, n_5)) > 1.00000000317107685e-30f)
    {
        _S43 = normalize(n_5);
    }
    else
    {
        _S43 = s_1.geometric_normal_0;
    }
    s_1.normal_1 = _S43;
    var _S44 : vec4<f32> = vec4<f32>(_S39);
    var _S45 : vec4<f32> = vec4<f32>(_S37);
    var _S46 : vec4<f32> = vec4<f32>(_S38);
    var t_2 : vec4<f32> = a_4.tangent_0 * _S44 + b_4.tangent_0 * _S45 + c_3.tangent_0 * _S46;
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
        _S43 = normalize(tw_0);
    }
    else
    {
        _S43 = vec3<f32>(0.0f, 0.0f, 0.0f);
    }
    var _S47 : f32;
    if((t_2.w) < 0.0f)
    {
        _S47 = -1.0f;
    }
    else
    {
        _S47 = 1.0f;
    }
    s_1.tangent_1 = vec4<f32>(_S43, _S47 * mirror_0);
    var _S48 : vec2<f32> = vec2<f32>(_S39);
    var _S49 : vec2<f32> = vec2<f32>(_S37);
    var _S50 : vec2<f32> = vec2<f32>(_S38);
    s_1.uv0_3 = a_4.uv0_0 * _S48 + b_4.uv0_0 * _S49 + c_3.uv0_0 * _S50;
    s_1.uv1_3 = a_4.uv1_0 * _S48 + b_4.uv1_0 * _S49 + c_3.uv1_0 * _S50;
    s_1.color_1 = a_4.color_0 * _S44 + b_4.color_0 * _S45 + c_3.color_0 * _S46;
    s_1.material_3 = ((tri_2.w) & (u32(16777215)));
    s_1.front_face_0 = (dot(s_1.geometric_normal_0, ray_4.direction_1)) < 0.0f;
    return s_1;
}

fn tangent_space_normal_0( texel_0 : vec3<f32>,  scale_0 : f32) -> vec3<f32>
{
    var n_6 : vec3<f32> = texel_0 * vec3<f32>(2.0f) - vec3<f32>(1.0f);
    if((texel_0.z) == 0.0f)
    {
        n_6[i32(2)] = sqrt(saturate(1.0f - dot(n_6.xy, n_6.xy)));
    }
    return vec3<f32>(n_6.xy * vec2<f32>(scale_0), n_6.z);
}

fn normalize_or_0( v_6 : vec3<f32>,  fallback_0 : vec3<f32>) -> vec3<f32>
{
    var _S51 : vec3<f32>;
    if((dot(v_6, v_6)) > 1.00000000317107685e-30f)
    {
        _S51 = normalize(v_6);
    }
    else
    {
        _S51 = fallback_0;
    }
    return _S51;
}

fn apply_normal_map_0( normal_2 : vec3<f32>,  tangent_2 : vec4<f32>,  tangent_normal_0 : vec3<f32>,  front_face_1 : bool) -> vec3<f32>
{
    var _S52 : vec3<f32> = tangent_2.xyz;
    var t_3 : vec3<f32> = normalize_or_0(_S52 - normal_2 * vec3<f32>(dot(normal_2, _S52)), vec3<f32>(0.0f));
    var mapped_0 : vec3<f32> = normalize_or_0((((mat3x3<f32>(t_3, cross(normal_2, t_3) * vec3<f32>(tangent_2.w), normal_2)) * (tangent_normal_0))), normal_2);
    var _S53 : vec3<f32>;
    if(front_face_1)
    {
        _S53 = mapped_0;
    }
    else
    {
        _S53 = (vec3<f32>(0) - mapped_0);
    }
    return _S53;
}

fn material_metallic_0( factor_0 : f32,  sampled_0 : f32) -> f32
{
    return clamp(factor_0 * sampled_0, 0.0f, 1.0f);
}

fn material_roughness_0( factor_1 : f32,  sampled_1 : f32) -> f32
{
    return clamp(factor_1 * sampled_1, 0.04500000178813934f, 1.0f);
}

fn material_occlusion_0( strength_0 : f32,  sampled_2 : f32) -> f32
{
    return mix(1.0f, sampled_2, strength_0);
}

struct TracedSurface_0
{
     position_1 : vec3<f32>,
     face_normal_0 : vec3<f32>,
     normal_3 : vec3<f32>,
     albedo_3 : vec3<f32>,
     emissive_2 : vec3<f32>,
     metallic_4 : f32,
     roughness_5 : f32,
     occlusion_0 : f32,
     unlit_0 : bool,
};

fn shade_hit_0( ray_5 : TraceRay_0,  hit_3 : TraceHit_0) -> TracedSurface_0
{
    var s_2 : TraceSurface_0 = trace_surface_0(ray_5, hit_3);
    var m_5 : TraceMaterial_0 = load_material_0(s_2.material_3);
    var base_3 : vec4<f32> = sample_material_slot_0(s_2.material_3, m_5, u32(0), s_2.uv0_3, s_2.uv1_3, 0.0f);
    var mr_0 : vec4<f32> = sample_material_slot_0(s_2.material_3, m_5, u32(1), s_2.uv0_3, s_2.uv1_3, 0.0f);
    var normal_4 : vec4<f32> = sample_material_slot_0(s_2.material_3, m_5, u32(2), s_2.uv0_3, s_2.uv1_3, 0.0f);
    var emission_0 : vec4<f32> = sample_material_slot_0(s_2.material_3, m_5, u32(3), s_2.uv0_3, s_2.uv1_3, 0.0f);
    var occlusion_1 : vec4<f32> = sample_material_slot_0(s_2.material_3, m_5, u32(4), s_2.uv0_3, s_2.uv1_3, 0.0f);
    var o_1 : TracedSurface_0;
    o_1.position_1 = s_2.position_0;
    var _S54 : vec3<f32>;
    if(s_2.front_face_0)
    {
        _S54 = s_2.geometric_normal_0;
    }
    else
    {
        _S54 = (vec3<f32>(0) - s_2.geometric_normal_0);
    }
    o_1.face_normal_0 = _S54;
    var n_7 : vec3<f32> = apply_normal_map_0(s_2.normal_1, s_2.tangent_1, tangent_space_normal_0(normal_4.xyz, m_5.normal_scale_0), s_2.front_face_0);
    if((dot(n_7, (vec3<f32>(0) - ray_5.direction_1))) < 0.00009999999747379f)
    {
        _S54 = o_1.face_normal_0;
    }
    else
    {
        _S54 = n_7;
    }
    o_1.normal_3 = _S54;
    o_1.albedo_3 = m_5.base_color_0.xyz * base_3.xyz * s_2.color_1.xyz;
    o_1.emissive_2 = m_5.emissive_0 * emission_0.xyz;
    o_1.metallic_4 = material_metallic_0(m_5.metallic_3, mr_0.z);
    o_1.roughness_5 = material_roughness_0(m_5.roughness_4, mr_0.y);
    o_1.occlusion_0 = material_occlusion_0(m_5.occlusion_strength_0, occlusion_1.x);
    o_1.unlit_0 = (((m_5.flags_2) & (u32(8)))) != u32(0);
    return o_1;
}

fn ndc_depth_0( ray_6 : TraceRay_0,  hit_4 : TraceHit_0) -> f32
{
    if((hit_4.instance_0) == u32(4294967295))
    {
        return 1.0f;
    }
    var clip_0 : vec4<f32> = (((vec4<f32>(ray_6.origin_1 + ray_6.direction_1 * vec3<f32>(hit_4.t_0), 1.0f)) * (mat4x4<f32>(traceFrame_0.view_proj_0.data_0[i32(0)][i32(0)], traceFrame_0.view_proj_0.data_0[i32(1)][i32(0)], traceFrame_0.view_proj_0.data_0[i32(2)][i32(0)], traceFrame_0.view_proj_0.data_0[i32(3)][i32(0)], traceFrame_0.view_proj_0.data_0[i32(0)][i32(1)], traceFrame_0.view_proj_0.data_0[i32(1)][i32(1)], traceFrame_0.view_proj_0.data_0[i32(2)][i32(1)], traceFrame_0.view_proj_0.data_0[i32(3)][i32(1)], traceFrame_0.view_proj_0.data_0[i32(0)][i32(2)], traceFrame_0.view_proj_0.data_0[i32(1)][i32(2)], traceFrame_0.view_proj_0.data_0[i32(2)][i32(2)], traceFrame_0.view_proj_0.data_0[i32(3)][i32(2)], traceFrame_0.view_proj_0.data_0[i32(0)][i32(3)], traceFrame_0.view_proj_0.data_0[i32(1)][i32(3)], traceFrame_0.view_proj_0.data_0[i32(2)][i32(3)], traceFrame_0.view_proj_0.data_0[i32(3)][i32(3)]))));
    return saturate(clip_0.z / clip_0.w);
}

fn ggx_energy_coordinates_0( n_dot_v_0 : f32,  roughness_6 : f32) -> vec2<f32>
{
    var low_0 : vec2<f32> = vec2<f32>(pow(0.00009999999747379f, 0.3333333432674408f), sqrt(0.04500000178813934f));
    return (vec2<f32>(pow(clamp(n_dot_v_0, 0.00009999999747379f, 1.0f), 0.3333333432674408f), sqrt(clamp(roughness_6, 0.04500000178813934f, 1.0f))) - low_0) / (vec2<f32>(1.0f) - low_0);
}

fn ggx_energy_weights_0( f_0 : f32) -> vec4<f32>
{
    var g_0 : f32 = 1.0f - f_0;
    return vec4<f32>(g_0 * g_0 * g_0, 4.0f - 6.0f * f_0 * f_0 + 3.0f * f_0 * f_0 * f_0, 4.0f - 6.0f * g_0 * g_0 + 3.0f * g_0 * g_0 * g_0, f_0 * f_0 * f_0) / vec4<f32>(6.0f);
}

fn ggx_energy_fit_0( t_4 : vec2<f32>) -> vec2<f32>
{
    var x_0 : vec2<f32> = t_4 * vec2<f32>(11.0f) + vec2<f32>(1.0f);
    var cell_0 : vec2<f32> = clamp(floor(x_0), vec2<f32>(1.0f), vec2<f32>(11.0f));
    var _S55 : f32 = cell_0.x;
    var _S56 : vec4<f32> = ggx_energy_weights_0(x_0.x - _S55);
    var _S57 : f32 = cell_0.y;
    var _S58 : vec4<f32> = ggx_energy_weights_0(x_0.y - _S57);
    var _S59 : i32 = i32(_S55 - 1.0f) * i32(14) + i32(_S57 - 1.0f);
    var _S60 : vec2<f32> = vec2<f32>(0.0f);
    var i_0 : i32 = i32(0);
    var e_1 : vec2<f32> = _S60;
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
        var row_0 : vec2<f32> = _S60;
        for(;;)
        {
            if(j_0 < i32(4))
            {
            }
            else
            {
                break;
            }
            var row_1 : vec2<f32> = row_0 + GGX_ENERGY_COEFFICIENTS_0[_S59 + i_0 * i32(14) + j_0] * vec2<f32>(_S58[j_0]);
            j_0 = j_0 + i32(1);
            row_0 = row_1;
        }
        var e_2 : vec2<f32> = e_1 + row_0 * vec2<f32>(_S56[i_0]);
        i_0 = i_0 + i32(1);
        e_1 = e_2;
    }
    return e_1;
}

fn ggx_energy_0( n_dot_v_1 : f32,  roughness_7 : f32) -> vec2<f32>
{
    return ggx_energy_fit_0(ggx_energy_coordinates_0(n_dot_v_1, roughness_7));
}

struct LightSample_0
{
     radiance_0 : vec3<f32>,
     l_0 : vec3<f32>,
     distance_0 : f32,
};

fn unlit_sample_0() -> LightSample_0
{
    var o_2 : LightSample_0;
    o_2.radiance_0 = vec3<f32>(0.0f);
    o_2.l_0 = vec3<f32>(0.0f, 1.0f, 0.0f);
    o_2.distance_0 = 0.0f;
    return o_2;
}

fn surface_f0_0( albedo_4 : vec3<f32>,  metallic_5 : f32) -> vec3<f32>
{
    return mix(vec3<f32>(0.03999999910593033f), albedo_4, vec3<f32>(metallic_5));
}

fn smith_k_analytic_0( roughness_8 : f32) -> f32
{
    var r_1 : f32 = max(roughness_8, 0.04500000178813934f) + 1.0f;
    return r_1 * r_1 / 8.0f;
}

fn distribution_ggx_0( n_dot_h_0 : f32,  roughness_9 : f32) -> f32
{
    var _S61 : f32 = max(roughness_9, 0.04500000178813934f);
    var a_5 : f32 = _S61 * _S61;
    var a2_0 : f32 = a_5 * a_5;
    var denom_0 : f32 = n_dot_h_0 * n_dot_h_0 * (a2_0 - 1.0f) + 1.0f;
    return a2_0 / (3.14159274101257324f * max(denom_0 * denom_0, 9.99999968265522539e-21f));
}

fn geometry_smith_k_0( n_dot_v_2 : f32,  n_dot_l_0 : f32,  k_0 : f32) -> f32
{
    var _S62 : f32 = 1.0f - k_0;
    return n_dot_v_2 / max(n_dot_v_2 * _S62 + k_0, 9.99999997475242708e-07f) * (n_dot_l_0 / max(n_dot_l_0 * _S62 + k_0, 9.99999997475242708e-07f));
}

fn fresnel_schlick_0( cos_theta_0 : f32,  f0_5 : vec3<f32>) -> vec3<f32>
{
    return f0_5 + (vec3<f32>(1.0f) - f0_5) * vec3<f32>(pow(clamp(1.0f - cos_theta_0, 0.0f, 1.0f), 5.0f));
}

fn ggx_single_scatter_0( n_dot_v_3 : f32,  n_dot_l_1 : f32,  n_dot_h_1 : f32,  h_dot_v_0 : f32,  roughness_10 : f32,  f0_6 : vec3<f32>,  k_1 : f32) -> vec3<f32>
{
    return vec3<f32>((distribution_ggx_0(n_dot_h_1, roughness_10) * geometry_smith_k_0(n_dot_v_3, n_dot_l_1, k_1))) * fresnel_schlick_0(h_dot_v_0, f0_6) / vec3<f32>(max(4.0f * n_dot_v_3 * n_dot_l_1, 9.99999997475242708e-07f));
}

fn lambert_diffuse_0( albedo_5 : vec3<f32>) -> vec3<f32>
{
    return albedo_5 / vec3<f32>(3.14159274101257324f);
}

fn brdf_smith_k_0( n_8 : vec3<f32>,  v_7 : vec3<f32>,  l_1 : vec3<f32>,  albedo_6 : vec3<f32>,  metallic_6 : f32,  roughness_11 : f32,  f0_7 : vec3<f32>,  radiance_1 : vec3<f32>,  k_2 : f32,  energy_4 : vec2<f32>) -> vec3<f32>
{
    var h_1 : vec3<f32> = normalize(l_1 + v_7);
    var _S63 : f32 = max(dot(n_8, l_1), 0.0f);
    return (saturate(vec3<f32>(1.0f) - specular_albedo_0(energy_4, f0_7)) * vec3<f32>((1.0f - metallic_6)) * lambert_diffuse_0(albedo_6) + multiscatter_gain_0(energy_4, f0_7) * ggx_single_scatter_0(max(dot(n_8, v_7), 0.00009999999747379f), _S63, max(dot(n_8, h_1), 0.0f), max(dot(h_1, v_7), 0.0f), max(roughness_11, 0.04500000178813934f), f0_7, k_2)) * radiance_1 * vec3<f32>(_S63);
}

fn brdf_direct_0( n_9 : vec3<f32>,  v_8 : vec3<f32>,  l_2 : vec3<f32>,  albedo_7 : vec3<f32>,  metallic_7 : f32,  roughness_12 : f32,  f0_8 : vec3<f32>,  radiance_2 : vec3<f32>,  energy_5 : vec2<f32>) -> vec3<f32>
{
    return brdf_smith_k_0(n_9, v_8, l_2, albedo_7, metallic_7, roughness_12, f0_8, radiance_2, smith_k_analytic_0(roughness_12), energy_5);
}

fn sun_sample_0( s_3 : TracedSurface_0,  v_9 : vec3<f32>,  energy_6 : vec2<f32>) -> LightSample_0
{
    var o_3 : LightSample_0 = unlit_sample_0();
    var _S64 : vec3<f32> = traceFrame_0.light_dir_0.xyz;
    o_3.l_0 = _S64;
    o_3.distance_0 = 10000.0f;
    if((dot(s_3.normal_3, _S64)) <= 0.0f)
    {
        return o_3;
    }
    o_3.radiance_0 = brdf_direct_0(s_3.normal_3, v_9, o_3.l_0, s_3.albedo_3, s_3.metallic_4, s_3.roughness_5, surface_f0_0(s_3.albedo_3, s_3.metallic_4), traceFrame_0.light_radiance_0.xyz, energy_6);
    return o_3;
}

struct PunctualLight_0
{
     position_2 : vec3<f32>,
     kind_0 : f32,
     radiance_3 : vec3<f32>,
     range_0 : f32,
     direction_7 : vec3<f32>,
     cos_inner_0 : f32,
     cos_outer_0 : f32,
};

fn unpack_punctual_light_0( a_6 : vec4<f32>,  b_5 : vec4<f32>,  c_4 : vec4<f32>,  d_2 : vec4<f32>) -> PunctualLight_0
{
    var light_0 : PunctualLight_0;
    light_0.position_2 = a_6.xyz;
    light_0.kind_0 = a_6.w;
    light_0.radiance_3 = b_5.xyz;
    light_0.range_0 = b_5.w;
    light_0.direction_7 = c_4.xyz;
    light_0.cos_inner_0 = c_4.w;
    light_0.cos_outer_0 = d_2.x;
    return light_0;
}

struct PunctualIncidence_0
{
     l_3 : vec3<f32>,
     distance_1 : f32,
     attenuation_0 : f32,
};

fn punctual_incidence_0( light_1 : PunctualLight_0,  position_3 : vec3<f32>) -> PunctualIncidence_0
{
    var o_4 : PunctualIncidence_0;
    o_4.attenuation_0 = 1.0f;
    if((light_1.kind_0) > 2.5f)
    {
        o_4.l_3 = normalize((vec3<f32>(0) - light_1.direction_7));
        o_4.distance_1 = 3.00000000549775576e+38f;
        return o_4;
    }
    var toLight_0 : vec3<f32> = light_1.position_2 - position_3;
    var _S65 : f32 = max(length(toLight_0), 0.00009999999747379f);
    o_4.distance_1 = _S65;
    o_4.l_3 = toLight_0 / vec3<f32>(_S65);
    o_4.attenuation_0 = 1.0f / (_S65 * _S65);
    if((light_1.range_0) > 0.0f)
    {
        var k_3 : f32 = clamp(1.0f - pow(o_4.distance_1 / light_1.range_0, 4.0f), 0.0f, 1.0f);
        o_4.attenuation_0 = o_4.attenuation_0 * (k_3 * k_3);
    }
    if((light_1.kind_0) > 1.5f)
    {
        o_4.attenuation_0 = o_4.attenuation_0 * smoothstep(light_1.cos_outer_0, light_1.cos_inner_0, dot(normalize(light_1.direction_7), (vec3<f32>(0) - o_4.l_3)));
    }
    return o_4;
}

fn punctual_sample_0( s_4 : TracedSurface_0,  v_10 : vec3<f32>,  energy_7 : vec2<f32>,  index_4 : u32) -> LightSample_0
{
    var o_5 : LightSample_0 = unlit_sample_0();
    var _S66 : u32 = index_4 * u32(4);
    var light_2 : PunctualLight_0 = unpack_punctual_light_0(tracerLights_0[_S66], tracerLights_0[_S66 + u32(1)], tracerLights_0[_S66 + u32(2)], tracerLights_0[_S66 + u32(3)]);
    var incidence_0 : PunctualIncidence_0 = punctual_incidence_0(light_2, s_4.position_1);
    o_5.l_0 = incidence_0.l_3;
    o_5.distance_0 = incidence_0.distance_1 * 0.99989998340606689f;
    var _S67 : bool;
    if((incidence_0.attenuation_0) <= 0.0f)
    {
        _S67 = true;
    }
    else
    {
        _S67 = (dot(s_4.normal_3, incidence_0.l_3)) <= 0.0f;
    }
    if(_S67)
    {
        return o_5;
    }
    o_5.radiance_0 = brdf_direct_0(s_4.normal_3, v_10, incidence_0.l_3, s_4.albedo_3, s_4.metallic_4, s_4.roughness_5, surface_f0_0(s_4.albedo_3, s_4.metallic_4), light_2.radiance_3 * vec3<f32>(incidence_0.attenuation_0), energy_7);
    return o_5;
}

fn direct_sample_0( s_5 : TracedSurface_0,  v_11 : vec3<f32>,  energy_8 : vec2<f32>,  index_5 : u32) -> LightSample_0
{
    if(index_5 == u32(0))
    {
        return sun_sample_0(s_5, v_11, energy_8);
    }
    return punctual_sample_0(s_5, v_11, energy_8, index_5 - u32(1));
}

fn trace_any_0( ray_7 : TraceRay_0) -> bool
{
    return (trace_scene_0(ray_7, true).instance_0) != u32(4294967295);
}

fn light_visible_0( s_6 : TracedSurface_0,  l_4 : vec3<f32>,  distance_2 : f32) -> bool
{
    var ray_8 : TraceRay_0;
    ray_8.origin_1 = s_6.position_1 + s_6.face_normal_0 * vec3<f32>(0.00009999999747379f);
    ray_8.tmin_0 = 0.00100000004749745f;
    ray_8.direction_1 = l_4;
    ray_8.tmax_0 = min(distance_2, 10000.0f);
    return !trace_any_0(ray_8);
}

fn shadowed_0( s_7 : TracedSurface_0,  light_3 : LightSample_0) -> vec3<f32>
{
    var _S68 : bool;
    if((any(((light_3.radiance_0) > vec3<f32>(0.0f)))))
    {
        _S68 = light_visible_0(s_7, light_3.l_0, light_3.distance_0);
    }
    else
    {
        _S68 = false;
    }
    var _S69 : vec3<f32>;
    if(_S68)
    {
        _S69 = light_3.radiance_0;
    }
    else
    {
        _S69 = vec3<f32>(0.0f);
    }
    return _S69;
}

fn direct_light_0( s_8 : TracedSurface_0,  v_12 : vec3<f32>,  energy_9 : vec2<f32>) -> vec3<f32>
{
    var _S70 : vec3<f32> = vec3<f32>(0.0f);
    var i_1 : u32 = u32(0);
    var lit_0 : vec3<f32> = _S70;
    for(;;)
    {
        if(i_1 <= (traceFrame_0.lights_0.x))
        {
        }
        else
        {
            break;
        }
        var lit_1 : vec3<f32> = lit_0 + shadowed_0(s_8, direct_sample_0(s_8, v_12, energy_9, i_1));
        i_1 = i_1 + u32(1);
        lit_0 = lit_1;
    }
    return lit_0;
}

fn traced_ambient_0( s_9 : TracedSurface_0,  v_13 : vec3<f32>,  energy_10 : vec2<f32>) -> vec3<f32>
{
    return vec3<f32>((traceFrame_0.light_radiance_0.w * s_9.occlusion_0)) * sky_ambient_0((traceFrame_0.lights_0.y) != u32(0), s_9.normal_3, v_13, s_9.albedo_3, s_9.metallic_4, s_9.roughness_5, surface_f0_0(s_9.albedo_3, s_9.metallic_4), energy_10);
}

fn ray_trace_pixel_0( pixel_3 : vec2<u32>) -> vec4<f32>
{
    var ray_9 : TraceRay_0 = pixel_ray_0(vec2<f32>(pixel_3) + vec2<f32>(0.5f));
    var hit_5 : TraceHit_0 = trace_closest_0(ray_9);
    if((hit_5.instance_0) == u32(4294967295))
    {
        return vec4<f32>(sky_with_sun_0(ray_9.direction_1), 1.0f);
    }
    var s_10 : TracedSurface_0 = shade_hit_0(ray_9, hit_5);
    if(s_10.unlit_0)
    {
        return vec4<f32>(s_10.albedo_3, ndc_depth_0(ray_9, hit_5));
    }
    var v_14 : vec3<f32> = (vec3<f32>(0) - ray_9.direction_1);
    var energy_11 : vec2<f32> = ggx_energy_0(dot(s_10.normal_3, v_14), s_10.roughness_5);
    return vec4<f32>(direct_light_0(s_10, v_14, energy_11) + traced_ambient_0(s_10, v_14, energy_11) + s_10.emissive_2, ndc_depth_0(ray_9, hit_5));
}

fn pixel_index_0( pixel_4 : vec2<u32>) -> u32
{
    return pixel_4.y * traceFrame_0.size_0.x + pixel_4.x;
}

fn hdr_safe_0( radiance_4 : vec3<f32>) -> vec3<f32>
{
    var _S71 : vec3<f32> = vec3<f32>(0.0f);
    return clamp(select(radiance_4, _S71, (((bitcast<vec3<u32>>((radiance_4))) & (vec3<u32>(u32(2147483647))))) > vec3<u32>(u32(2139095040))), _S71, vec3<f32>(65000.0f));
}

@compute
@workgroup_size(8, 8, 1)
fn cs_ray_trace(@builtin(global_invocation_id) id_0 : vec3<u32>)
{
    var pixel_5 : vec2<u32>;
    var _S72 : bool = tile_pixel_0(id_0.xy, &(pixel_5));
    if(!_S72)
    {
        return;
    }
    var traced_0 : vec4<f32> = ray_trace_pixel_0(pixel_5);
    traceAccumulation_0[pixel_index_0(pixel_5)] = vec4<f32>(hdr_safe_0(traced_0.xyz), traced_0.w);
    return;
}

fn path_rng_seed_0( pixel_index_1 : u32,  stream_0 : u32) -> u32
{
    return pixel_index_1 + stream_0 * u32(2654435769);
}

fn rng_next_0( state_0 : ptr<function, u32>) -> f32
{
    var _S73 : u32 = (*state_0) * u32(747796405) + u32(1);
    (*state_0) = _S73;
    var word_1 : u32 = ((((_S73 >> ((((_S73 >> (u32(28)))) + u32(4))))) ^ (_S73))) * u32(277803737);
    return f32((((word_1 >> (u32(22)))) ^ (word_1))) / 4.294967296e+09f;
}

fn rng_next_1( state_1 : ptr<function, u32>) -> f32
{
    var _S74 : u32 = (*state_1) * u32(747796405) + u32(1);
    (*state_1) = _S74;
    var word_2 : u32 = ((((_S74 >> ((((_S74 >> (u32(28)))) + u32(4))))) ^ (_S74))) * u32(277803737);
    return f32((((word_2 >> (u32(22)))) ^ (word_2))) / 4.294967296e+09f;
}

fn russian_roulette_survives_0( bounce_0 : u32,  throughput_0 : ptr<function, vec3<f32>>,  rng_0 : ptr<function, u32>) -> bool
{
    if(bounce_0 < u32(3))
    {
        return true;
    }
    var survival_0 : f32 = clamp(max((*throughput_0).x, max((*throughput_0).y, (*throughput_0).z)), 0.05000000074505806f, 0.94999998807907104f);
    var _S75 : f32 = rng_next_1(&((*rng_0)));
    if(_S75 > survival_0)
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

fn bounce_sky_0( dir_3 : vec3<f32>) -> vec3<f32>
{
    var sky_0 : vec3<f32>;
    if((traceFrame_0.lights_0.y) != u32(0))
    {
        sky_0 = (textureSampleLevel((tracerEnvironment_0), (tracerLinearSampler_0), (dir_3), (0.0f))).xyz;
    }
    else
    {
        sky_0 = sky_gradient_0(dir_3);
    }
    return sky_0 * vec3<f32>(traceFrame_0.light_radiance_0.w);
}

fn miss_radiance_0( dir_4 : vec3<f32>,  bounce_1 : u32) -> vec3<f32>
{
    if(furnace_on_0())
    {
        return vec3<f32>(traceFrame_0.furnace_0.x);
    }
    var _S76 : vec3<f32>;
    if(bounce_1 == u32(0))
    {
        _S76 = sky_with_sun_0(dir_4);
    }
    else
    {
        _S76 = bounce_sky_0(dir_4);
    }
    return _S76;
}

fn path_surface_0( ray_10 : TraceRay_0,  hit_6 : TraceHit_0) -> TracedSurface_0
{
    var s_11 : TracedSurface_0 = shade_hit_0(ray_10, hit_6);
    if(furnace_on_0())
    {
        s_11.albedo_3 = vec3<f32>(1.0f);
        s_11.emissive_2 = vec3<f32>(0.0f);
        s_11.unlit_0 = false;
        s_11.metallic_4 = traceFrame_0.furnace_0.y;
        s_11.roughness_5 = clamp(traceFrame_0.furnace_0.z, 0.04500000178813934f, 1.0f);
    }
    return s_11;
}

fn punctual_light_sample_0( s_12 : TracedSurface_0,  v_15 : vec3<f32>,  energy_12 : vec2<f32>,  rng_1 : ptr<function, u32>) -> LightSample_0
{
    var count_1 : u32 = traceFrame_0.lights_0.x;
    if(count_1 == u32(0))
    {
        return unlit_sample_0();
    }
    var _S77 : f32 = rng_next_0(&((*rng_1)));
    var _S78 : f32 = f32(count_1);
    var o_6 : LightSample_0 = punctual_sample_0(s_12, v_15, energy_12, min(u32(_S77 * _S78), count_1 - u32(1)));
    o_6.radiance_0 = o_6.radiance_0 * vec3<f32>(_S78);
    return o_6;
}

fn next_event_0( s_13 : TracedSurface_0,  v_16 : vec3<f32>,  energy_13 : vec2<f32>,  rng_2 : ptr<function, u32>) -> vec3<f32>
{
    if(furnace_on_0())
    {
        return vec3<f32>(0.0f);
    }
    var _S79 : LightSample_0 = sun_sample_0(s_13, v_16, energy_13);
    var _S80 : LightSample_0 = punctual_light_sample_0(s_13, v_16, energy_13, &((*rng_2)));
    var _S81 : array<LightSample_0, i32(2)> = array<LightSample_0, i32(2)>( _S79, _S80 );
    var _S82 : vec3<f32> = vec3<f32>(0.0f);
    var i_2 : u32 = u32(0);
    var lit_2 : vec3<f32> = _S82;
    for(;;)
    {
        if(i_2 < u32(2))
        {
        }
        else
        {
            break;
        }
        var lit_3 : vec3<f32> = lit_2 + shadowed_0(s_13, _S81[i_2]);
        i_2 = i_2 + u32(1);
        lit_2 = lit_3;
    }
    return lit_2;
}

fn specular_lobe_probability_0( albedo_8 : vec3<f32>,  metallic_8 : f32,  f0_9 : vec3<f32>,  n_dot_v_4 : f32) -> f32
{
    const LUMA_0 : vec3<f32> = vec3<f32>(0.2125999927520752f, 0.71520000696182251f, 0.07220000028610229f);
    var fresnel_0 : vec3<f32> = fresnel_schlick_0(n_dot_v_4, f0_9);
    var specular_0 : f32 = dot(fresnel_0, LUMA_0);
    return clamp(specular_0 / max(specular_0 + dot((vec3<f32>(1.0f) - fresnel_0) * vec3<f32>((1.0f - metallic_8)) * albedo_8, LUMA_0), 9.99999997475242708e-07f), 0.10000000149011612f, 1.0f);
}

fn orthonormal_basis_0( n_10 : vec3<f32>,  t_5 : ptr<function, vec3<f32>>,  b_6 : ptr<function, vec3<f32>>)
{
    var _S83 : f32 = n_10.z;
    var s_14 : f32;
    if(_S83 >= 0.0f)
    {
        s_14 = 1.0f;
    }
    else
    {
        s_14 = -1.0f;
    }
    var a_7 : f32 = -1.0f / (s_14 + _S83);
    var _S84 : f32 = n_10.x;
    var _S85 : f32 = n_10.y;
    var c_5 : f32 = _S84 * _S85 * a_7;
    (*t_5) = vec3<f32>(1.0f + s_14 * _S84 * _S84 * a_7, s_14 * c_5, - s_14 * _S84);
    (*b_6) = vec3<f32>(c_5, s_14 + _S85 * _S85 * a_7, - _S85);
    return;
}

fn sample_ggx_visible_normal_0( v_17 : vec3<f32>,  alpha_1 : f32,  u_1 : vec2<f32>) -> vec3<f32>
{
    var vh_0 : vec3<f32> = normalize(vec3<f32>(alpha_1 * v_17.x, alpha_1 * v_17.y, v_17.z));
    var _S86 : f32 = vh_0.x;
    var _S87 : f32 = vh_0.y;
    var length_squared_0 : f32 = _S86 * _S86 + _S87 * _S87;
    var t1_1 : vec3<f32>;
    if(length_squared_0 > 0.0f)
    {
        t1_1 = vec3<f32>(- _S87, _S86, 0.0f) / vec3<f32>(sqrt(length_squared_0));
    }
    else
    {
        t1_1 = vec3<f32>(1.0f, 0.0f, 0.0f);
    }
    var r_2 : f32 = sqrt(u_1.x);
    var phi_0 : f32 = 6.28318548202514648f * u_1.y;
    var p1_2 : f32 = r_2 * cos(phi_0);
    var s_15 : f32 = 0.5f * (1.0f + vh_0.z);
    var _S88 : f32 = 1.0f - p1_2 * p1_2;
    var p2_2 : f32 = (1.0f - s_15) * sqrt(max(_S88, 0.0f)) + s_15 * (r_2 * sin(phi_0));
    var nh_0 : vec3<f32> = vec3<f32>(p1_2) * t1_1 + vec3<f32>(p2_2) * cross(vh_0, t1_1) + vec3<f32>(sqrt(max(_S88 - p2_2 * p2_2, 0.0f))) * vh_0;
    return normalize(vec3<f32>(alpha_1 * nh_0.x, alpha_1 * nh_0.y, max(nh_0.z, 9.99999997475242708e-07f)));
}

fn ggx_visible_normal_pdf_0( n_dot_v_5 : f32,  n_dot_h_2 : f32,  alpha_2 : f32) -> f32
{
    var a2_1 : f32 = alpha_2 * alpha_2;
    var d_3 : f32 = n_dot_h_2 * n_dot_h_2 * (a2_1 - 1.0f) + 1.0f;
    return a2_1 / (3.14159274101257324f * d_3 * d_3) * (2.0f * n_dot_v_5 / (n_dot_v_5 + sqrt(a2_1 + (1.0f - a2_1) * n_dot_v_5 * n_dot_v_5))) / (4.0f * n_dot_v_5);
}

fn smith_k_sampled_0( roughness_13 : f32) -> f32
{
    var _S89 : f32 = max(roughness_13, 0.04500000178813934f);
    return _S89 * _S89 / 2.0f;
}

fn brdf_sampled_0( n_11 : vec3<f32>,  v_18 : vec3<f32>,  l_5 : vec3<f32>,  albedo_9 : vec3<f32>,  metallic_9 : f32,  roughness_14 : f32,  f0_10 : vec3<f32>,  energy_14 : vec2<f32>) -> vec3<f32>
{
    return brdf_smith_k_0(n_11, v_18, l_5, albedo_9, metallic_9, roughness_14, f0_10, vec3<f32>(1.0f), smith_k_sampled_0(roughness_14), energy_14);
}

struct Bounce_0
{
     direction_8 : vec3<f32>,
     weight_0 : vec3<f32>,
};

fn sample_bounce_0( n_12 : vec3<f32>,  face_normal_1 : vec3<f32>,  v_19 : vec3<f32>,  albedo_10 : vec3<f32>,  metallic_10 : f32,  roughness_15 : f32,  energy_15 : vec2<f32>,  rng_3 : ptr<function, u32>) -> Bounce_0
{
    var f0_11 : vec3<f32> = surface_f0_0(albedo_10, metallic_10);
    var _S90 : f32 = max(dot(n_12, v_19), 0.00009999999747379f);
    var specular_1 : f32 = specular_lobe_probability_0(albedo_10, metallic_10, f0_11, _S90);
    var alpha_3 : f32 = roughness_15 * roughness_15;
    var t_6 : vec3<f32>;
    var b_7 : vec3<f32>;
    orthonormal_basis_0(n_12, &(t_6), &(b_7));
    var _S91 : f32 = rng_next_0(&((*rng_3)));
    var direction_9 : vec3<f32>;
    if(_S91 < specular_1)
    {
        var local_0 : vec3<f32> = vec3<f32>(dot(v_19, t_6), dot(v_19, b_7), dot(v_19, n_12));
        var _S92 : f32 = rng_next_0(&((*rng_3)));
        var _S93 : f32 = rng_next_0(&((*rng_3)));
        var h_2 : vec3<f32> = sample_ggx_visible_normal_0(local_0, alpha_3, vec2<f32>(_S92, _S93));
        direction_9 = reflect((vec3<f32>(0) - v_19), vec3<f32>(h_2.x) * t_6 + vec3<f32>(h_2.y) * b_7 + vec3<f32>(h_2.z) * n_12);
    }
    else
    {
        var _S94 : f32 = rng_next_0(&((*rng_3)));
        var theta_0 : f32 = 6.28318548202514648f * _S94;
        var _S95 : f32 = rng_next_0(&((*rng_3)));
        var z_0 : f32 = 2.0f * _S95 - 1.0f;
        var r_3 : f32 = sqrt(max(1.0f - z_0 * z_0, 0.0f));
        var direction_10 : vec3<f32> = n_12 + vec3<f32>(r_3 * cos(theta_0), r_3 * sin(theta_0), z_0);
        if((dot(direction_10, direction_10)) < 9.99999993922529029e-09f)
        {
            direction_9 = n_12;
        }
        else
        {
            direction_9 = direction_10;
        }
    }
    var o_7 : Bounce_0;
    var _S96 : vec3<f32> = normalize(direction_9);
    o_7.direction_8 = _S96;
    o_7.weight_0 = vec3<f32>(0.0f);
    var n_dot_l_2 : f32 = dot(n_12, _S96);
    var _S97 : bool;
    if(n_dot_l_2 <= 0.0f)
    {
        _S97 = true;
    }
    else
    {
        _S97 = (dot(face_normal_1, o_7.direction_8)) <= 0.0f;
    }
    if(_S97)
    {
        return o_7;
    }
    o_7.weight_0 = brdf_sampled_0(n_12, v_19, o_7.direction_8, albedo_10, metallic_10, roughness_15, f0_11, energy_15) / vec3<f32>(max(specular_1 * ggx_visible_normal_pdf_0(_S90, max(dot(n_12, normalize(v_19 + o_7.direction_8)), 0.0f), alpha_3) + (1.0f - specular_1) * n_dot_l_2 / 3.14159274101257324f, 9.99999997475242708e-07f));
    return o_7;
}

fn trace_path_0( pixel_6 : vec2<u32>,  rng_4 : ptr<function, u32>) -> vec3<f32>
{
    var _S98 : f32 = rng_next_0(&((*rng_4)));
    var _S99 : f32 = rng_next_0(&((*rng_4)));
    var ray_11 : TraceRay_0 = pixel_ray_0(vec2<f32>(pixel_6) + vec2<f32>(_S98, _S99));
    var throughput_1 : vec3<f32> = vec3<f32>(1.0f);
    var _S100 : vec3<f32> = vec3<f32>(0.0f);
    var bounce_2 : u32 = u32(0);
    var radiance_5 : vec3<f32> = _S100;
    for(;;)
    {
        if(bounce_2 < (traceFrame_0.size_0.z))
        {
        }
        else
        {
            break;
        }
        var _S101 : bool = russian_roulette_survives_0(bounce_2, &(throughput_1), &((*rng_4)));
        if(!_S101)
        {
            break;
        }
        var hit_7 : TraceHit_0 = trace_closest_0(ray_11);
        if((hit_7.instance_0) == u32(4294967295))
        {
            radiance_5 = radiance_5 + throughput_1 * miss_radiance_0(ray_11.direction_1, bounce_2);
            break;
        }
        var s_16 : TracedSurface_0 = path_surface_0(ray_11, hit_7);
        if(s_16.unlit_0)
        {
            radiance_5 = radiance_5 + throughput_1 * s_16.albedo_3;
            break;
        }
        var v_20 : vec3<f32> = (vec3<f32>(0) - ray_11.direction_1);
        var energy_16 : vec2<f32> = ggx_energy_0(dot(s_16.normal_3, v_20), s_16.roughness_5);
        var _S102 : vec3<f32> = next_event_0(s_16, v_20, energy_16, &((*rng_4)));
        var radiance_6 : vec3<f32> = radiance_5 + throughput_1 * (s_16.emissive_2 + _S102);
        var next_0 : Bounce_0 = sample_bounce_0(s_16.normal_3, s_16.face_normal_0, v_20, s_16.albedo_3, s_16.metallic_4, s_16.roughness_5, energy_16, &((*rng_4)));
        if((all(((next_0.weight_0) == _S100))))
        {
            radiance_5 = radiance_6;
            break;
        }
        throughput_1 = throughput_1 * next_0.weight_0;
        ray_11.origin_1 = s_16.position_1 + s_16.face_normal_0 * vec3<f32>(0.00009999999747379f);
        ray_11.tmin_0 = 0.00100000004749745f;
        ray_11.direction_1 = next_0.direction_8;
        ray_11.tmax_0 = 10000.0f;
        bounce_2 = bounce_2 + u32(1);
        radiance_5 = radiance_6;
    }
    return radiance_5;
}

fn primary_hit_depth_0( pixel_7 : vec2<u32>) -> f32
{
    var ray_12 : TraceRay_0 = pixel_ray_0(vec2<f32>(pixel_7) + vec2<f32>(0.5f));
    return ndc_depth_0(ray_12, trace_closest_0(ray_12));
}

@compute
@workgroup_size(8, 8, 1)
fn cs_path_trace(@builtin(global_invocation_id) id_1 : vec3<u32>)
{
    var pixel_8 : vec2<u32>;
    var _S103 : bool = tile_pixel_0(id_1.xy, &(pixel_8));
    if(!_S103)
    {
        return;
    }
    var index_6 : u32 = pixel_index_0(pixel_8);
    var rng_5 : u32 = path_rng_seed_0(index_6, traceTile_0.samples_0.z);
    var _S104 : vec3<f32> = vec3<f32>(0.0f);
    var i_3 : u32 = u32(0);
    var sum_0 : vec3<f32> = _S104;
    for(;;)
    {
        if(i_3 < (traceTile_0.samples_0.y))
        {
        }
        else
        {
            break;
        }
        var _S105 : vec3<f32> = trace_path_0(pixel_8, &(rng_5));
        var sum_1 : vec3<f32> = sum_0 + hdr_safe_0(_S105);
        i_3 = i_3 + u32(1);
        sum_0 = sum_1;
    }
    var before_0 : f32 = f32(traceTile_0.samples_0.z);
    var held_0 : vec4<f32> = traceAccumulation_0[index_6];
    var previous_0 : vec3<f32>;
    if((traceTile_0.samples_0.z) > u32(0))
    {
        previous_0 = held_0.xyz;
    }
    else
    {
        previous_0 = _S104;
    }
    var depth_0 : f32;
    if((traceTile_0.samples_0.x) == u32(0))
    {
        depth_0 = primary_hit_depth_0(pixel_8);
    }
    else
    {
        depth_0 = held_0.w;
    }
    traceAccumulation_0[index_6] = vec4<f32>((previous_0 * vec3<f32>(before_0) + sum_0) / vec3<f32>((before_0 + f32(traceTile_0.samples_0.y))), depth_0);
    return;
}

