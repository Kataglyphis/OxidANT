// Both renderers' ray bounds for every ray that leaves a surface.

// A hit nearer than TRACE_TMIN is the surface the ray left.
static const float TRACE_TMIN = 0.001;
static const float TRACE_TMAX = 10000.0;
// Along the face normal, against self-intersection.
static const float TRACE_SURFACE_OFFSET = 0.0001;
// A punctual light's shadow ray stops this fraction of the way, so geometry at the light's position does not shadow it.
static const float TRACE_LIGHT_SPAN = 1.0 - 1e-4;
