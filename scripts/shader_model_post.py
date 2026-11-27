"""The f64 model of the post chain's shared modules: exposure.slang's histogram and adaptation, bloom_filter.slang."""
import math

from shader_model import Model, Real, Vec, clamp, dot, f32, real, visit

# ---- common/exposure.slang ----

HISTOGRAM_BINS = 64
MIN_LOG_LUMINANCE = -10.0
MAX_LOG_LUMINANCE = 4.0
BLACK_THRESHOLD = 1e-6
EXPOSURE_KEY = 0.18


def log2(x: Real) -> Real:
    return Real(math.log2(x.v))


def exp2(x: Real) -> Real:
    return Real(math.pow(2.0, x.v))


def exp(x: Real) -> Real:
    return Real(math.exp(x.v))


def exposure_luminance(color: Vec) -> Real:
    return dot(color, Vec(0.2126, 0.7152, 0.0722))


def histogram_bin(lum: Real) -> int:
    if not lum.v > f32(BLACK_THRESHOLD):
        visit("black bin")
        return 0
    span = MAX_LOG_LUMINANCE - MIN_LOG_LUMINANCE
    raw = (log2(lum) - MIN_LOG_LUMINANCE) / span
    if raw.v < 0.0:
        visit("below the range")
    elif raw.v > 1.0:
        visit("above the range")
    scaled = clamp(raw, 0.0, 1.0) * float(HISTOGRAM_BINS - 2)
    # uint() truncates, so a device's log2 must not carry the value across an integer.
    fraction = scaled.v - math.floor(scaled.v)
    if 0.0 < scaled.v < float(HISTOGRAM_BINS - 2):
        Model.margins.append(("bin edge", min(fraction, 1.0 - fraction) / max(1.0, scaled.v)))
    return 1 + min(int(scaled.v), HISTOGRAM_BINS - 2)


def bin_luminance(bin_index: int) -> Real:
    if bin_index == 0:
        visit("bin 0")
        return Real(0.0)
    normalized = (Real(float(bin_index - 1)) + 0.5) / float(HISTOGRAM_BINS - 2)
    return exp2(MIN_LOG_LUMINANCE + normalized * (MAX_LOG_LUMINANCE - MIN_LOG_LUMINANCE))


def exposure_target_ev(log2_mean: Real) -> Real:
    return log2(real(EXPOSURE_KEY) / exp2(log2_mean))


def adapt_exposure_ev(current_ev: Real, target_ev: Real, dt: Real, speed: Real) -> Real:
    if math.isfinite(current_ev.v) and abs(current_ev.v) < f32(1e30):
        current = current_ev
    else:
        visit("current replaced")
        current = target_ev
    if dt.v <= 0.0:
        visit("held")
        return current
    if speed.v <= 0.0:
        visit("snapped")
        return target_ev
    blend = clamp(1.0 - exp(-dt * speed), 0.0, 1.0)
    if blend.v >= 1.0:
        visit("blend saturated")
    return current + (target_ev - current) * blend


def exposure_scale(ev: Real) -> Real:
    return exp2(ev)


# ---- common/bloom_filter.slang ----

BLOOM_THRESHOLD = 1.0
BLOOM_TAP_RADIUS = 4
BLOOM_WEIGHTS = (0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216)
HDR_MAX = 65000.0


def bloom_bright(exposed: Vec) -> Vec:
    out: list[Real] = []
    for x in exposed.c:
        kept = x - BLOOM_THRESHOLD
        if kept.v <= 0.0:
            visit("below the threshold")
        elif kept.v > f32(HDR_MAX):
            visit("capped at HDR_MAX")
        out.append(clamp(kept, 0.0, HDR_MAX))
    return Vec(*out)


def bloom_tap_weight(offset: int) -> Real:
    if abs(offset) > BLOOM_TAP_RADIUS:
        visit("past the kernel")
        return Real(0.0)
    return real(BLOOM_WEIGHTS[abs(offset)])


def bloom_tap_weight_sum() -> Real:
    total = Real(0.0)
    for offset in range(-BLOOM_TAP_RADIUS - 1, BLOOM_TAP_RADIUS + 2):
        total = total + bloom_tap_weight(offset)
    return total
