"""The vector files' shapes and the case-writing helpers generate-shader-vectors.py and its module files share."""
import math
import random
from collections.abc import Callable, Sequence
from dataclasses import dataclass, field

import shader_model as sm

Values = dict[str, list[float]]


@dataclass
class Port:
    name: str
    kind: str
    width: int
    abs: float = 0.0
    rel: float = 0.0


@dataclass
class Module:
    name: str
    inputs: list[Port]
    outputs: list[Port]
    evaluate: Callable[[Values], dict[str, list[float]]]
    cases: list[tuple[str, Values]]
    visits: set[str] = field(default_factory=set)


def flat(*items: sm.Real | sm.Vec | int | bool) -> list[float]:
    words: list[float] = []
    for item in items:
        if isinstance(item, sm.Vec):
            words += [c.v for c in item.c]
        elif isinstance(item, sm.Real):
            words.append(item.v)
        else:
            words.append(int(item))
    return words


def one(values: Values, name: str) -> sm.Real:
    return sm.real(values[name][0])


def word(values: Values, name: str) -> int:
    return int(values[name][0])


def unit(*c: float) -> list[float]:
    length = math.sqrt(sum(x * x for x in c))
    return [x / length for x in c]


def ports(spec: str) -> list[Port]:
    """`name:width/type` words, width 1 and type f32 by default."""
    found = []
    for item in spec.split():
        head, _, kind = item.partition("/")
        name, _, width = head.partition(":")
        found.append(Port(name, kind or "f32", int(width or 1)))
    return found


def out(name: str, width: int, tolerance: tuple[float, float] | None = None) -> Port:
    """An f32 output with its (abs, rel) tolerance, or an exact u32 one without."""
    if tolerance is None:
        return Port(name, "u32", width)
    return Port(name, "f32", width, *tolerance)


def merged(base: Values, rows: Sequence[tuple[str, Values]]) -> list[tuple[str, Values]]:
    return [(name, base | row) for name, row in rows]


class Draw:
    """Seeded inputs from arithmetic and sqrt alone, which round alike on every platform."""

    def __init__(self, seed: int) -> None:
        self.rng = random.Random(seed)

    def num(self, lo: float, hi: float) -> float:
        return lo + (hi - lo) * self.rng.random()

    def nums(self, lo: float, hi: float, count: int = 3) -> list[float]:
        return [self.num(lo, hi) for _ in range(count)]

    def whole(self, lo: int, hi: int) -> int:
        return self.rng.randrange(lo, hi)

    def direction(self, around: Sequence[float] | None = None) -> list[float]:
        while True:
            c = self.nums(-1.0, 1.0)
            length = math.sqrt(sum(x * x for x in c))
            if 0.2 < length <= 1.0 and (around is None or sum(a * x for a, x in zip(around, c, strict=True)) > 0.1 * length):
                return unit(*c)


SMOOTH = (1e-6, 1e-5)
CHAINED = (1e-5, 1e-4)
