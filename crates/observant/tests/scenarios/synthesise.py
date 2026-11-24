#!/usr/bin/env python3
"""Write the synthesised ObservANT scenarios beside this file, in the format `--record-scenario` records."""

import json
import random
import sys
from dataclasses import dataclass, field
from pathlib import Path

WIDTH, HEIGHT, BLOCK = 640, 480, 8
COLS, ROWS = WIDTH // BLOCK, HEIGHT // BLOCK
# motion.rs's MOVED_LATELY: a block counts as moving this long after it changed.
MOVED_LATELY = 10.0
CAT = 15
WHOLE = (0.0, 0.0, float(WIDTH), float(HEIGHT))
SOURCE = (
    "synthesised, not recorded: crates/observant/tests/scenarios/synthesise.py from the hand "
    "observations in docs/observant.md and CHANGELOG.md"
)

Rect = tuple[float, float, float, float]


@dataclass
class Scene:
    """One scenario's model runs and the motion the pacer saw."""

    name: str
    story: str
    ignore: list[list[float]] = field(default_factory=list)
    rng: random.Random = field(default_factory=lambda: random.Random(7))
    motion: list[tuple[float, Rect]] = field(default_factory=list)
    frames: list[dict[str, object]] = field(default_factory=list)

    def move(self, t: float, rect: Rect) -> None:
        self.motion.append((t, rect))

    def run(self, t: float, boxes: list[tuple[Rect, float]]) -> None:
        self.frames.append({
            "t": round(t, 3),
            "boxes": [sighting(rect, score) for rect, score in boxes],
            "moving": moving_runs(t, self.motion),
        })

    def jitter(self, rect: Rect, px: float) -> Rect:
        x1, y1, x2, y2 = (v + self.rng.uniform(-px, px) for v in rect)
        return (max(x1, 0.0), max(y1, 0.0), min(x2, WIDTH), min(y2, HEIGHT))

    def score(self, low: float, high: float) -> float:
        return round(self.rng.uniform(low, high), 3)

    def write(self, out: Path) -> None:
        header = {
            "observant_scenario": 1,
            "provenance": f"{SOURCE}: {self.story}",
            "width": WIDTH,
            "height": HEIGHT,
            "inference_fps": 2.0,
            "floor": 0.1,
            "ignore": self.ignore,
        }
        lines = [header, *self.frames]
        text = "".join(json.dumps(line, separators=(",", ":")) + "\n" for line in lines)
        (out / f"{self.name}.jsonl").write_text(text, encoding="utf-8")


def sighting(rect: Rect, score: float) -> dict[str, object]:
    x1, y1, x2, y2 = (round(v, 1) for v in rect)
    return {"x1": x1, "y1": y1, "x2": x2, "y2": y2, "score": score, "class": CAT}


def moving_runs(t: float, motion: list[tuple[float, Rect]]) -> dict[str, object]:
    """The blocks that changed in the last MOVED_LATELY seconds, as motion.rs's Moving runs."""
    blocks = [False] * (COLS * ROWS)
    for at, (x1, y1, x2, y2) in motion:
        if not 0.0 <= t - at <= MOVED_LATELY:
            continue
        for row in range(int(y1) // BLOCK, min(int(y2 - 1) // BLOCK + 1, ROWS)):
            for col in range(int(x1) // BLOCK, min(int(x2 - 1) // BLOCK + 1, COLS)):
                blocks[row * COLS + col] = True
    runs: list[list[int]] = []
    for i, moved in enumerate(blocks):
        if moved and runs and runs[-1][0] + runs[-1][1] == i:
            runs[-1][1] += 1
        elif moved:
            runs.append([i, 1])
    return {"cols": COLS, "rows": ROWS, "runs": runs}


def steps(start: float, end: float, period: float) -> list[float]:
    """Run times from start up to but not including end."""
    count = round((end - start) / period)
    return [start + i * period for i in range(count)]


def empty(scene: Scene, start: float, end: float) -> None:
    """Nobody in view: three still-period runs after a sighting, then the idle rate's every 5 s."""
    for t in steps(start, min(start + 3.0, end), 1.0) + steps(start + 3.0, end, 5.0):
        scene.run(t, [])


def along(path: tuple[Rect, Rect], i: int, count: int) -> Rect:
    f = i / max(count - 1, 1)
    x1, y1, x2, y2 = (a + (b - a) * f for a, b in zip(path[0], path[1], strict=True))
    return (x1, y1, x2, y2)


Still = list[tuple[Rect, tuple[float, float]]]


def walk(scene: Scene, start: float, end: float, path: tuple[Rect, Rect], scores: tuple[float, float],
         beside: Still | None = None) -> None:
    """A cat walking from one box to the other at the full rate, the blocks under it moving."""
    times = steps(start, end, 0.5)
    for i, t in enumerate(times):
        box = along(path, i, len(times))
        scene.move(t, box)
        scene.run(t, [(box, scene.score(*scores)), *still(scene, beside or [])])


def still(scene: Scene, things: Still) -> list[tuple[Rect, float]]:
    return [(scene.jitter(rect, 2.0), scene.score(*scores)) for rect, scores in things]


def sit(scene: Scene, start: float, end: float, things: Still) -> None:
    """Cats, or what the model takes for one, still in view: looked at once a still period."""
    for t in steps(start, end, 1.0):
        scene.run(t, still(scene, things))


RACK: Rect = (165.0, 362.0, 478.0, 479.0)
RACK_ZONE = [0.25, 0.75, 0.75, 1.0]
RACK_SCORES = (0.27, 0.34)


def drying_rack_still() -> Scene:
    scene = Scene(
        "drying-rack-still",
        "the test Pi's drying rack, a false cat at about 0.34 that never moves (2026-10-13), "
        "before any spot was marked not a cat; the motion gate alone must keep it from arriving",
    )
    scene.move(0.0, WHOLE)
    empty(scene, 0.0, 60.0)
    sit(scene, 60.0, 300.0, [(RACK, RACK_SCORES)])
    return scene


def drying_rack_cat_in_front() -> Scene:
    scene = Scene(
        "drying-rack-cat-in-front",
        "the drying rack marked not a cat (docs/observant.md's ignore example), scored all along, "
        "and a cat that walks in, sits in front of it for half a minute and leaves",
        ignore=[RACK_ZONE],
    )
    rack = [(RACK, RACK_SCORES)]
    seat: Rect = (280.0, 380.0, 360.0, 470.0)
    scene.move(0.0, WHOLE)
    sit(scene, 0.0, 30.0, rack)
    walk(scene, 30.0, 40.0, ((20.0, 380.0, 110.0, 460.0), seat), (0.6, 0.85), rack)
    sit(scene, 40.0, 70.0, [*rack, (seat, (0.7, 0.85))])
    walk(scene, 70.0, 75.0, (seat, (540.0, 380.0, 630.0, 460.0)), (0.6, 0.85), rack)
    sit(scene, 75.0, 200.0, rack)
    return scene


SOFA: Rect = (380.0, 220.0, 470.0, 300.0)
BEHIND: Rect = (500.0, 220.0, 590.0, 300.0)
LEFT: Rect = (0.0, 240.0, 90.0, 320.0)


def behind_the_sofa() -> Scene:
    scene = Scene(
        "cat-behind-the-sofa",
        "a cat sits by the sofa, goes behind it for 50 s and comes back: one visit "
        "(the minute's return, 2026-10-14)",
    )
    scene.move(0.0, WHOLE)
    empty(scene, 0.0, 20.0)
    walk(scene, 20.0, 25.0, (LEFT, SOFA), (0.55, 0.8))
    sit(scene, 25.0, 45.0, [(SOFA, (0.6, 0.8))])
    walk(scene, 45.0, 47.0, (SOFA, BEHIND), (0.45, 0.6))
    empty(scene, 47.0, 97.0)
    walk(scene, 97.0, 100.0, (BEHIND, SOFA), (0.45, 0.7))
    sit(scene, 100.0, 130.0, [(SOFA, (0.6, 0.8))])
    walk(scene, 130.0, 133.0, (SOFA, LEFT), (0.55, 0.8))
    empty(scene, 133.0, 220.0)
    return scene


BOWL_A: Rect = (250.0, 300.0, 340.0, 380.0)
BOWL_B: Rect = (360.0, 300.0, 450.0, 380.0)
DOOR: Rect = (560.0, 280.0, 640.0, 370.0)
EATS_A = (0.65, 0.9)
EATS_B = (0.4, 0.7)
WALKS = (0.5, 0.85)


def feeding_hour() -> Scene:
    scene = Scene(
        "feeding-hour",
        "two cats at the feeding trough, walking off and back 30 to 40 s later: one visit; "
        "then a 98 s break and a second (one cat had made 21 visits an hour, 10 to 90 s apart, 2026-10-14)",
    )
    scene.move(0.0, WHOLE)
    empty(scene, 0.0, 20.0)
    walk(scene, 20.0, 23.0, (DOOR, BOWL_A), WALKS)
    sit(scene, 23.0, 80.0, [(BOWL_A, EATS_A)])
    walk(scene, 80.0, 82.0, (BOWL_A, DOOR), WALKS)
    empty(scene, 82.0, 122.0)
    walk(scene, 122.0, 124.0, (DOOR, BOWL_A), WALKS)
    sit(scene, 124.0, 150.0, [(BOWL_A, EATS_A)])
    walk(scene, 150.0, 153.0, (DOOR, BOWL_B), WALKS, [(BOWL_A, EATS_A)])
    sit(scene, 153.0, 170.0, [(BOWL_A, EATS_A), (BOWL_B, EATS_B)])
    walk(scene, 170.0, 172.0, (BOWL_A, DOOR), WALKS, [(BOWL_B, EATS_B)])
    sit(scene, 172.0, 230.0, [(BOWL_B, EATS_B)])
    walk(scene, 230.0, 232.0, (BOWL_B, DOOR), WALKS)
    empty(scene, 232.0, 262.0)
    walk(scene, 262.0, 265.0, (DOOR, BOWL_B), WALKS)
    sit(scene, 265.0, 300.0, [(BOWL_B, EATS_B)])
    walk(scene, 300.0, 302.0, (BOWL_B, DOOR), WALKS)
    empty(scene, 302.0, 400.0)
    walk(scene, 400.0, 403.0, (DOOR, BOWL_A), WALKS)
    sit(scene, 403.0, 450.0, [(BOWL_A, EATS_A)])
    walk(scene, 450.0, 452.0, (BOWL_A, DOOR), WALKS)
    empty(scene, 452.0, 540.0)
    return scene


CUSHION: Rect = (200.0, 250.0, 300.0, 330.0)
FULL_FRAME: Rect = (0.0, 5.0, 640.0, 476.0)
SPOT: Rect = (300.0, 330.0, 390.0, 400.0)
DIM = (0.25, 0.29)


def dark_room() -> Scene:
    scene = Scene(
        "dark-room",
        "dusk in the test Pi's room: lone hits at 0.2x where the sensor's noise flickers (52 of 72 "
        "old arrivals were one hit, 42 scored 0.2x, 2026-10-09 to -11), black, a lamp switched on "
        "(boxes over 0.95 of the frame at 0.31, 2026-10-16/17, and 0.2x on a cushion), then a "
        "cat in the dim light at 0.25 to 0.31",
    )
    scene.move(0.0, WHOLE)
    for t in steps(0.0, 100.0, 5.0):
        if t in (15.0, 35.0, 55.0, 80.0):
            scene.move(t, CUSHION)
            scene.run(t, still(scene, [(CUSHION, (0.26, 0.29))]))
            scene.run(t + 1.0, [])
        else:
            scene.run(t, [])
    lamp(scene)
    dim_walk_in(scene, 240.0)
    for t in steps(245.0, 290.0, 1.0):
        scene.run(t, [] if int(t) % 7 == 0 else still(scene, [(SPOT, DIM)]))
    walk(scene, 290.0, 292.0, (SPOT, (0.0, 330.0, 90.0, 400.0)), DIM)
    empty(scene, 292.0, 360.0)
    return scene


def lamp(scene: Scene) -> None:
    """Black from 100 s, so no run; the lamp at 150 s moves every block."""
    scene.move(150.0, WHOLE)
    for i, t in enumerate(steps(150.0, 156.0, 0.5)):
        boxes = still(scene, [(CUSHION, (0.21, 0.24))])
        if i < 3:
            boxes.append((scene.jitter(FULL_FRAME, 3.0), 0.31))
        scene.run(t, boxes)
    for t in steps(156.0, 240.0, 5.0):
        scene.run(t, [])


def dim_walk_in(scene: Scene, start: float) -> None:
    """0.26 then 0.31 arrive at 0.25, and no two runs in a row reach 0.30, so 0.30 sees no cat."""
    path = ((0.0, 330.0, 90.0, 400.0), SPOT)
    times = steps(start, start + 5.0, 0.5)
    for i, t in enumerate(times):
        box = along(path, i, len(times))
        scene.move(t, box)
        score = 0.31 if i % 2 else (0.26 if i == 0 else scene.score(*DIM))
        scene.run(t, [(box, score)])


def main() -> int:
    out = Path(__file__).resolve().parent
    for make in (drying_rack_still, drying_rack_cat_in_front, behind_the_sofa, feeding_hour, dark_room):
        make().write(out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
