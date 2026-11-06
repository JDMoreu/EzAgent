"""Marcas de latencia del turno de voz.

Un turno se mide desde que el VAD deja de oír voz hasta que suena el primer
audio de la respuesta. Las marcas se toman en el hilo de audio, el de STT y el
de TTS; el objeto es de un solo turno y no necesita compartirse entre turnos.
"""

from __future__ import annotations

import time
from collections import deque
from dataclasses import dataclass, field


@dataclass
class TurnTimings:
    """Timestamps monotónicos de un turno."""

    speech_end: float
    vad_end: float
    marks: dict[str, float] = field(default_factory=dict)

    def mark(self, name: str, at: float | None = None) -> None:
        self.marks[name] = time.monotonic() if at is None else float(at)

    def report(self) -> dict[str, float]:
        """Duraciones en ms entre las marcas disponibles."""
        result: dict[str, float] = {
            "vad_wait_ms": (self.vad_end - self.speech_end) * 1000.0,
        }
        previous = self.vad_end
        for name in ("stt_ready", "first_token", "first_sentence", "first_sound", "turn_end"):
            at = self.marks.get(name)
            if at is None:
                continue
            result[f"{name}_ms"] = (at - previous) * 1000.0
            previous = at
        first_sound = self.marks.get("first_sound")
        if first_sound is not None:
            result["ttfa_ms"] = (first_sound - self.speech_end) * 1000.0
            result["total_ms"] = result["ttfa_ms"]
        return result


class LatencyStats:
    """Acumula los turnos y publica media y P95 cada diez turnos."""

    def __init__(self, maxlen: int = 50) -> None:
        self._turns: deque[dict[str, float]] = deque(maxlen=maxlen)
        self._total = 0

    def record(self, timings: TurnTimings) -> dict[str, float]:
        report = timings.report()
        self._turns.append(report)
        self._total += 1
        print(
            "[ezagent-voice] Latencia: "
            + " ".join(f"{key}={value:.0f}" for key, value in report.items()),
            flush=True,
        )
        if self._total >= 10 and self._total % 10 == 0:
            self._log_summary()
        return report

    def _log_summary(self) -> None:
        keys = ("vad_wait_ms", "stt_ready_ms", "first_token_ms",
                "first_sentence_ms", "first_sound_ms", "ttfa_ms", "total_ms")
        parts = []
        for key in keys:
            values = [t[key] for t in self._turns if key in t]
            if not values:
                continue
            parts.append(
                f"{key}=media {sum(values) / len(values):.0f} "
                f"P95 {self.percentile(values, 95):.0f}"
            )
        if parts:
            print(
                "[ezagent-voice] Latencia acumulada: " + " | ".join(parts),
                flush=True,
            )

    @staticmethod
    def percentile(values: list[float], pct: float) -> float:
        """Percentil con interpolación lineal, sin dependencias externas."""
        if not values:
            return 0.0
        ordered = sorted(values)
        if len(ordered) == 1:
            return ordered[0]
        pos = (len(ordered) - 1) * (pct / 100.0)
        lo = int(pos)
        hi = min(lo + 1, len(ordered) - 1)
        frac = pos - lo
        return ordered[lo] + (ordered[hi] - ordered[lo]) * frac


__all__ = ["LatencyStats", "TurnTimings"]
