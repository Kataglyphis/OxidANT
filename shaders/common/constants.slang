// The numbers more than one shared module shades with, declared once for both renderers.

static const float PI = 3.14159265358979323846;

// Smallest perceptual roughness the NDF evaluates; below it the GGX peak 1/(PI r^4) outgrows HDR targets.
static const float MIN_ROUGHNESS = 0.045;
