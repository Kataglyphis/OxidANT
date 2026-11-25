// The sky both renderers draw, per direction: sky.slang's fragment and both path tracers' camera misses read it.

import aces;
import hdr_safe;
import sky_model;

// The C++ engine's switched-off sky: linear navy.
static const float3 SKYBOX_DISABLED = float3(0.0, 0.0, 0.5);

// mode: 0 procedural, 1 cube map, 2 switched off; cube is the cube map's display colour along dir.
float3 drawn_sky(float mode, float3 cube, float3 dir, float3 light_dir, float sun_intensity)
{
    if (mode > 1.5)
        return aces_tonemap_inverse(SKYBOX_DISABLED);
    // The photo skybox holds display colours, so it enters as the radiance whose ACES at 0 EV reproduces them.
    if (mode > 0.5)
        return aces_tonemap_inverse(cube);
    return hdr_safe(sky_gradient(dir) + sun_disk(dir, light_dir, sun_intensity));
}
