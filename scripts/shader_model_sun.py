"""The f64 model of sun_mask.slang over sun_mask_test.slang's DiskSky: the IBL bake's sun search and its mask."""
from dataclasses import dataclass

from shader_model import (
    PI,
    Real,
    Vec,
    below,
    cos,
    cross,
    dot,
    normalize,
    real,
    rmax,
    rmin,
    sin,
    visit,
)

SUN_MASK_COS_RADIUS = 0.98480775
SUN_MASK_RING_COS = 0.9781476
SUN_MASK_RING_SIN = 0.20791169
SUN_MASK_RING_TAPS = 16
SUN_MASK_CONTRAST = 3.0
SUN_SEARCH_STEP = 0.026185921
SUN_SEARCH_HALF = 6


@dataclass
class DiskSky:
    sky: Vec
    disk: Vec
    disk_dir: Vec
    disk_cos: Real

    def radiance(self, d: Vec) -> Vec:
        if below(self.disk_cos, dot(d, self.disk_dir), "the disk's edge"):
            visit("disk tap")
            return self.disk
        return self.sky


def sun_mask_level(c: Vec) -> Real:
    return rmin(c.x, rmin(c.y, c.z))


def sun_search_tangent(n: Vec) -> Vec:
    if below(Real(abs(n.y.v)), 0.999, "the tangent's pole"):
        up = Vec(0.0, 1.0, 0.0)
    else:
        visit("at the pole")
        up = Vec(1.0, 0.0, 0.0)
    return normalize(cross(up, n))


def find_sun(sky: DiskSky) -> tuple[Vec, Vec]:
    """The mask's direction (zero for no sun) and its ring."""
    centre = Vec(0.0, 1.0, 0.0)
    coarse_peak = Real(0.0)
    degree = real(PI) / 180.0
    for j in range(18):
        lat = (Real(float(j)) * 10.0 - 85.0) * degree
        for i in range(36):
            lon = Real(float(i)) * 10.0 * degree
            d = Vec(cos(lat) * cos(lon), sin(lat), cos(lat) * sin(lon))
            l = sun_mask_level(sky.radiance(d))
            # The sky and the disk each give one exact value, so a tie is a tie on every device.
            if l.v > coarse_peak.v:
                coarse_peak, centre = l, d
    peak = Real(0.0)
    for _ in range(2):
        t = sun_search_tangent(centre)
        b = cross(centre, t)
        weighted = Vec(0.0, 0.0, 0.0)
        for y in range(-SUN_SEARCH_HALF, SUN_SEARCH_HALF + 1):
            for x in range(-SUN_SEARCH_HALF, SUN_SEARCH_HALF + 1):
                d = normalize(centre + (t * Real(float(x)) + b * Real(float(y))) * SUN_SEARCH_STEP)
                l = sun_mask_level(sky.radiance(d))
                peak = rmax(peak, l)
                weighted = weighted + d * rmax(l - coarse_peak * 0.5, 0.0)
        if dot(weighted, weighted).v > 0.0:
            centre = normalize(weighted)
    t = sun_search_tangent(centre)
    b = cross(centre, t)
    ring = Vec(0.0, 0.0, 0.0)
    for k in range(SUN_MASK_RING_TAPS):
        phi = real(2.0) * PI * (Real(float(k)) + 0.5) / float(SUN_MASK_RING_TAPS)
        ring = ring + sky.radiance(centre * SUN_MASK_RING_COS + (t * cos(phi) + b * sin(phi)) * SUN_MASK_RING_SIN)
    ring = ring / float(SUN_MASK_RING_TAPS)
    if below(sun_mask_level(ring) * SUN_MASK_CONTRAST, peak, "the sun's contrast"):
        visit("sun found")
        return centre, ring
    visit("no sun")
    return Vec(0.0, 0.0, 0.0), ring


def sun_masked(radiance: Vec, d: Vec, mask_dir: Vec, ring: Vec) -> Vec:
    if below(SUN_MASK_COS_RADIUS, dot(d, mask_dir), "the mask's edge"):
        visit("inside the mask")
        return Vec(*(rmin(a, b) for a, b in zip(radiance.c, ring.c, strict=True)))
    visit("outside the mask")
    return radiance
