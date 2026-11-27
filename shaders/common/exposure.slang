import hdr_safe;

// Exposure in both renderers: the histogram's bins, the EV they reduce to, its adaptation and the tonemap's scale.

static const uint HISTOGRAM_BINS = 64u;
static const float MIN_LOG_LUMINANCE = -10.0;
static const float MAX_LOG_LUMINANCE = 4.0;
static const float BLACK_THRESHOLD = 1e-6;
static const float EXPOSURE_KEY = 0.18;

// Rec. 709 luma, as the golden tests' CPU luminance.
float exposure_luminance(float3 color)
{
    return dot(color, float3(0.2126, 0.7152, 0.0722));
}

// Bin 0 is black, which would drag the log-average down; !(lum > threshold) puts NaN there too.
uint histogram_bin(float lum)
{
    if (!(lum > BLACK_THRESHOLD))
        return 0u;
    float normalized = clamp((log2(lum) - MIN_LOG_LUMINANCE) / (MAX_LOG_LUMINANCE - MIN_LOG_LUMINANCE), 0.0, 1.0);
    return 1u + min(uint(normalized * float(HISTOGRAM_BINS - 2u)), HISTOGRAM_BINS - 2u);
}

// histogram_bin's inverse at the bin's centre.
float bin_luminance(uint bin)
{
    if (bin == 0u)
        return 0.0;
    float normalized = (float(bin - 1u) + 0.5) / float(HISTOGRAM_BINS - 2u);
    return exp2(MIN_LOG_LUMINANCE + normalized * (MAX_LOG_LUMINANCE - MIN_LOG_LUMINANCE));
}

// The EV that exposes a geometric-mean luminance of exp2(log2Mean) onto middle grey.
float exposure_target_ev(float log2Mean)
{
    return log2(EXPOSURE_KEY / exp2(log2Mean));
}

// Exponential, so 60 and 120 Hz settle alike; a non-finite EV takes the target, dt <= 0 holds, speed <= 0 snaps.
float adapt_exposure_ev(float currentEv, float targetEv, float dt, float speed)
{
    float current = is_finite(currentEv) && abs(currentEv) < 1e30 ? currentEv : targetEv;
    if (dt <= 0.0)
        return current;
    if (speed <= 0.0)
        return targetEv;
    float blend = clamp(1.0 - exp(-dt * speed), 0.0, 1.0);
    return current + (targetEv - current) * blend;
}

// What an EV multiplies linear radiance by before the tonemap.
float exposure_scale(float ev)
{
    return exp2(ev);
}
