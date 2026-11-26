"""sun_mask's cases and evaluation for generate-shader-vectors.py, over sun_mask_test.slang's DiskSky."""
import math

import shader_model as sm
import shader_model_sun as ss
from shader_vector_format import (
    CHAINED,
    SMOOTH,
    Draw,
    Module,
    Values,
    flat,
    merged,
    one,
    out,
    ports,
    unit,
)

PHOTO_SUN = unit(0.6248, 0.6769, 0.3891)


def sky_of(i: Values) -> ss.DiskSky:
    return ss.DiskSky(sm.vec3(i["sky"]), sm.vec3(i["disk"]), sm.vec3(i["disk_dir"]), one(i, "disk_cos"))


def evaluate_sun_mask(i: Values) -> dict[str, list[float]]:
    mask_dir, ring = ss.find_sun(sky_of(i))
    radiance, d = sm.vec3(i["radiance"]), sm.vec3(i["dir"])
    return {
        "find_sun.dir": flat(mask_dir),
        "find_sun.ring": flat(ring),
        "sun_masked": flat(ss.sun_masked(radiance, d, mask_dir, ring)),
        "sun_mask_level": flat(ss.sun_mask_level(radiance)),
        "sun_search_tangent": flat(ss.sun_search_tangent(d)),
    }


def turned(d: list[float], degrees: float) -> list[float]:
    """`d` turned by `degrees` towards the horizontal axis across it."""
    across = unit(-d[2], 0.0, d[0])
    a = math.radians(degrees)
    return unit(*(math.cos(a) * x + math.sin(a) * y for x, y in zip(d, across, strict=True)))


def sun_mask_cases() -> list[tuple[str, Values]]:
    # 8.02 degrees keeps every search tap 3e-5 or more off the disk's edge, where a device's rounding could flip it.
    disk_cos = [math.cos(math.radians(8.02))]
    base: Values = {"sky": [0.5, 0.7, 1.0], "disk": [7.242, 7.242, 7.242], "disk_dir": PHOTO_SUN,
                    "disk_cos": disk_cos, "radiance": [7.242, 7.242, 7.242], "dir": PHOTO_SUN}
    rows: list[tuple[str, Values]] = [
        ("a photo sky's saturated sun is found, and a tap at its centre takes the ring", {}),
        ("a tap 20 degrees off the sun keeps its radiance", {"dir": turned(PHOTO_SUN, 20.0)}),
        ("a tap 9.5 degrees off the sun's centre is inside the disk", {"dir": turned(PHOTO_SUN, 9.5)}),
        ("a tap 10.5 degrees off the sun's centre is outside it", {"dir": turned(PHOTO_SUN, 10.5)}),
        ("an HDR sun far brighter than a photo's", {"sky": [1.0, 1.0, 1.0], "disk": [5.0e4, 5.0e4, 5.0e4],
                                                   "radiance": [5.0e4, 5.0e4, 5.0e4]}),
        ("a tap inside the disk: each channel comes down to the ring's", {"radiance": [9.0, 0.3, 2.0]}),
        ("a sun dim in one channel is no sun: the blue glow around a photo's", {"disk": [0.9, 7.242, 7.242]}),
        ("a tap darker than the ring inside the disk keeps its radiance", {"radiance": [0.1, 0.2, 0.3]}),
        ("a disk under three times its ring in its dimmest channel is no sun", {"disk": [1.4, 1.4, 1.4]}),
        ("a disk over three times its ring in its dimmest channel is a sun", {"disk": [1.6, 1.6, 1.6]}),
        ("a sky of one radiance has no sun", {"disk": [0.5, 0.7, 1.0]}),
        ("a black sky's sun takes a black ring", {"sky": [0.0, 0.0, 0.0], "disk": [3.0, 3.0, 3.0]}),
        ("a sun at the zenith: the search's tangent turns at the pole", {"disk_dir": [0.0, 1.0, 0.0],
                                                                         "dir": [0.0, 1.0, 0.0]}),
        ("a sun on the horizon", {"disk_dir": [1.0, 0.0, 0.0], "dir": unit(1.0, 0.05, 0.02)}),
        ("a sun below the horizon", {"disk_dir": unit(0.28, -0.8, 0.5), "dir": unit(0.28, -0.8, 0.5)}),
        ("a bright patch of 25 degrees is no sun: its ring lies inside it", {"disk_cos": [math.cos(math.radians(25.0))]}),
    ]
    draw = Draw(59)
    for index in range(4):
        sun = draw.direction()
        rows.append((f"seeded sun {index}", {
            "sky": draw.nums(0.1, 1.5), "disk": draw.nums(5.0, 20.0), "disk_dir": sun,
            "disk_cos": [math.cos(math.radians(draw.num(7.5, 9.0)))], "radiance": draw.nums(0.0, 10.0),
            "dir": turned(sun, draw.num(0.0, 15.0))}))
    return merged(base, rows)


MODULES = [
    Module("sun_mask", ports("sky:3 disk:3 disk_dir:3 disk_cos radiance:3 dir:3"), [
        out("find_sun.dir", 3, (1e-4, 1e-3)), out("find_sun.ring", 3, CHAINED), out("sun_masked", 3, CHAINED),
        out("sun_mask_level", 1, SMOOTH), out("sun_search_tangent", 3, SMOOTH),
    ], evaluate_sun_mask, sun_mask_cases(), {
        "sun found", "no sun", "inside the mask", "outside the mask", "at the pole", "disk tap"}),
]
