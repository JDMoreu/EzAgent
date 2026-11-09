"""Bar choreography: how the buttons are born from the form and go back into it.

Pure module (no Win32, no Pillow) so the timing can be tested in isolation.

* Entering: the form grows 4 % and the buttons slide out from *behind* it,
  scaled to 0.6 and transparent, on a spring with ~8 % overshoot, staggered
  40 ms from the nearest slot to the farthest.
* Leaving: reverse order, 25 ms stagger, critically damped (no bounce).
* Pointer: the button under the pointer grows to 1.08, pressing it 0.94.
* Reduced motion: no slide, no scale, no bounce; 120 ms linear fades.
"""

from __future__ import annotations

import math
from collections.abc import Iterable

STAGGER_IN = 0.040  # s between buttons when they come out
STAGGER_OUT = 0.025  # s between buttons when they go back in
SPRING_IN = (300.0, 22.0)  # k, c -> damping 0.64, ~8 % overshoot
SPRING_OUT = (420.0, 41.0)  # k, c -> critically damped, no overshoot
SPRING_POINTER = (520.0, 34.0)
SPRING_GROW = (260.0, 26.0)
FORM_GROW = 1.04  # form scale while the pointer is near
HOVER_SCALE = 1.08
PRESS_SCALE = 0.94
BORN_SCALE = 0.6  # scale of a button when it starts coming out
REDUCED_FADE = 0.12  # s, linear fade with reduced motion
SETTLE_EPS = 1e-3


class Spring:
    """Damped spring (k, c, unit mass) integrated at a fixed 1/120 s step."""

    def __init__(self, x: float = 0.0, k: float = 300.0, c: float = 18.0) -> None:
        self.x = float(x)
        self.v = 0.0
        self.t = float(x)
        self.k = k
        self.c = c

    def step(self, dt: float, instant: bool = False) -> float:
        if instant:
            self.x, self.v = self.t, 0.0
            return self.x
        n = max(1, math.ceil(dt * 120))
        h = dt / n
        for _ in range(n):
            self.v += (-self.k * (self.x - self.t) - self.c * self.v) * h
            self.x += self.v * h
        return self.x

    def fade(self, dt: float, seconds: float = REDUCED_FADE) -> float:
        """Linear approach to the target (reduced motion)."""
        step = dt / max(1e-6, seconds)
        d = self.t - self.x
        self.x = self.t if abs(d) <= step else self.x + math.copysign(step, d)
        self.v = 0.0
        return self.x

    def settled(self) -> bool:
        return abs(self.x - self.t) < SETTLE_EPS and abs(self.v) < SETTLE_EPS * 10


class Choreo:
    """Per-button appear springs with staggered start times."""

    def __init__(self, names: Iterable[str], shown: bool = True) -> None:
        self.names = tuple(names)
        v = 1.0 if shown else 0.0
        self.appear = {n: Spring(v, *SPRING_IN) for n in self.names}
        self.pointer = {n: Spring(1.0, *SPRING_POINTER) for n in self.names}
        self.grow = Spring(1.0, *SPRING_GROW)
        self._want: dict[str, bool] = {n: shown for n in self.names}
        self._due: dict[str, float] = {n: 0.0 for n in self.names}

    def set_targets(
        self,
        want: dict[str, bool],
        order: Iterable[Iterable[str]],
        now: float,
        reduced: bool = False,
    ) -> None:
        """Schedules each change.

        ``order`` is a sequence of groups from the nearest slot to the farthest;
        buttons in the same group move together. Reduced motion: no stagger.
        """
        groups = [tuple(n for n in g if n in self.appear) for g in order]
        coming = [g for g in groups if any(want.get(n, False) and not self._want[n] for n in g)]
        going = [
            g for g in reversed(groups) if any(not want.get(n, False) and self._want[n] for n in g)
        ]
        for i, g in enumerate(coming):
            for n in g:
                if want.get(n, False) and not self._want[n]:
                    self._want[n] = True
                    self._due[n] = now + (0.0 if reduced else i * STAGGER_IN)
        for i, g in enumerate(going):
            for n in g:
                if not want.get(n, False) and self._want[n]:
                    self._want[n] = False
                    self._due[n] = now + (0.0 if reduced else i * STAGGER_OUT)

    def step(
        self,
        now: float,
        dt: float,
        reduced: bool,
        grow: bool,
        hover_item: str | None = None,
        pressed: str | None = None,
    ) -> None:
        for n, spring in self.appear.items():
            if now >= self._due[n]:
                target = 1.0 if self._want[n] else 0.0
                if target != spring.t:
                    spring.t = target
                    spring.k, spring.c = SPRING_IN if target > spring.x else SPRING_OUT
            if reduced:
                spring.fade(dt)
            else:
                spring.step(dt)
        for n, spring in self.pointer.items():
            if reduced:
                spring.t = 1.0
            elif pressed == n:
                spring.t = PRESS_SCALE
            elif hover_item == n:
                spring.t = HOVER_SCALE
            else:
                spring.t = 1.0
            spring.step(dt, instant=reduced)
        self.grow.t = FORM_GROW if (grow and not reduced) else 1.0
        self.grow.step(dt, instant=reduced)

    def pending(self, now: float) -> bool:
        """True while a staggered change has not started yet."""
        return any(
            now < self._due[n] and (1.0 if self._want[n] else 0.0) != self.appear[n].t
            for n in self.names
        )

    def animating(self, now: float) -> bool:
        springs = (*self.appear.values(), *self.pointer.values(), self.grow)
        return self.pending(now) or not all(s.settled() for s in springs)


def emerge(
    slot: tuple[float, float], origin: tuple[float, float], a: float, reduced: bool
) -> tuple[float, float, float, float]:
    """(cx, cy, scale, alpha) of a button whose appear value is ``a``.

    The button travels from ``origin`` (the form centre) to its ``slot``. ``a``
    may overshoot 1 slightly: the button then goes a little past its slot and
    comes back, which is the bounce.
    """
    if reduced:
        return (slot[0], slot[1], 1.0, max(0.0, min(1.0, a)))
    k = max(0.0, a)
    cx = origin[0] + (slot[0] - origin[0]) * k
    cy = origin[1] + (slot[1] - origin[1]) * k
    scale = BORN_SCALE + (1.0 - BORN_SCALE) * min(1.0, k)
    alpha = max(0.0, min(1.0, k * 1.6))
    return (cx, cy, scale, alpha)
