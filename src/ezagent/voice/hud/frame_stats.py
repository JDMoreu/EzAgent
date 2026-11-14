"""Tiempos de dibujo por ventana: la base para pulir la fluidez con datos.

Each layered window records how long ``render()`` plus the copy to the screen
took. Every ``REPORT_SECS`` a one-line summary goes to the log (engine.log):
frames, p50, p95 and max in ms, plus how many frames missed a 60 fps budget.
Nothing is logged while a window is idle.
"""

from __future__ import annotations

import time
from collections.abc import Callable

from loguru import logger

REPORT_SECS = 60.0
BUDGET_MS = 1000.0 / 60.0
_LIMIT = 4096  # muestras como máximo entre informes


class FrameStats:
    def __init__(self, name: str, clock: Callable[[], float] = time.monotonic) -> None:
        self.name = name
        self._clock = clock
        self._samples: list[float] = []
        self._since = clock()

    def add(self, ms: float) -> str | None:
        """Anota un cuadro; devuelve el resumen si tocaba informar (ya registrado)."""
        if len(self._samples) < _LIMIT:
            self._samples.append(float(ms))
        now = self._clock()
        if now - self._since < REPORT_SECS:
            return None
        line = self.summary()
        self._samples.clear()
        self._since = now
        if line:
            logger.info(line)
        return line

    def summary(self) -> str:
        if not self._samples:
            return ""
        data = sorted(self._samples)
        n = len(data)
        p50 = data[n // 2]
        p95 = data[min(n - 1, int(n * 0.95))]
        slow = sum(1 for v in data if v > BUDGET_MS)
        return (
            f"Cuadros {self.name}: {n} · p50 {p50:.1f} ms · p95 {p95:.1f} ms · "
            f"máx {data[-1]:.1f} ms · {slow} sobre {BUDGET_MS:.1f} ms"
        )


__all__ = ["BUDGET_MS", "REPORT_SECS", "FrameStats"]
