import hdr_safe;

// Bloom in both renderers: what the bright pass keeps of the exposed radiance and the separable Gaussian's taps.

static const float BLOOM_THRESHOLD = 1.0;
static const int BLOOM_TAP_RADIUS = 4;
static const float BLOOM_WEIGHTS[5] = { 0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216 };

// The exposed radiance above the threshold, per channel: only what the tonemap would show past white glows.
float3 bloom_bright(float3 exposed)
{
    return hdr_safe(exposed - float3(BLOOM_THRESHOLD));
}

// The 9-tap Gaussian's weight `offset` texels from the centre; the nine sum to one, so the blur keeps the energy.
float bloom_tap_weight(int offset)
{
    int distance = abs(offset);
    return distance <= BLOOM_TAP_RADIUS ? BLOOM_WEIGHTS[distance] : 0.0;
}
