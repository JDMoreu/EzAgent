"""Marcas de latencia del turno de voz.

Un turno se mide desde que el VAD deja de oír voz hasta que suena el primer
audio de la respuesta. Las marcas se toman en el hilo de audio, el de STT y el
de TTS; el objeto es de un solo turno y no necesita compartirse entre turnos.
"""

from __future__ import annotations

import json
import threading
import time
from collections import deque
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class TurnTimings:
    """Timestamps monotónicos de un turno."""

    speech_end: float
    vad_end: float
    marks: dict[str, float] = field(default_factory=dict)
    # Tiempo acumulado (s) de etapas que ocurren a trozos durante el turno:
    # ``llm`` (espera de tokens), ``tts_synth`` (síntesis) y ``tts_play`` (voz).
    spans: dict[str, float] = field(default_factory=dict)
    # La cola de voz suma ``tts_play`` desde su propio hilo.
    _lock: threading.Lock = field(
        default_factory=threading.Lock, repr=False, compare=False
    )

    def mark(self, name: str, at: float | None = None) -> None:
        with self._lock:
            self.marks[name] = time.monotonic() if at is None else float(at)

    def add_span(self, name: str, seconds: float) -> None:
        with self._lock:
            self.spans[name] = self.spans.get(name, 0.0) + max(0.0, float(seconds))

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
        # Duración propia de cada etapa, para leerla sin sumar a mano.
        llm_start = self.marks.get("llm_start")
        first_token = self.marks.get("first_token")
        if llm_start is not None and first_token is not None:
            result["llm_ttft_ms"] = (first_token - llm_start) * 1000.0
        for span, key in (
            ("llm", "llm_ms"),
            ("tts_synth", "tts_synth_ms"),
            ("tts_play", "tts_play_ms"),
        ):
            if span in self.spans:
                result[key] = self.spans[span] * 1000.0
        turn_end = self.marks.get("turn_end")
        if turn_end is not None:
            result["turn_total_ms"] = (turn_end - self.speech_end) * 1000.0
        return result


def _ms(value: float) -> str:
    return f"{value:.0f} ms"


def format_stt(report: dict[str, float]) -> str:
    """Línea corta con lo que ya se sabe al terminar de transcribir."""
    parts = [f"VAD {_ms(report['vad_wait_ms'])}"]
    if "stt_ready_ms" in report:
        parts.append(f"STT {_ms(report['stt_ready_ms'])}")
    return " | ".join(parts)


def format_turn(report: dict[str, float]) -> str:
    """Resumen legible de un turno: una etapa por campo, en milisegundos."""
    parts = [f"VAD {_ms(report['vad_wait_ms'])}"]
    if "stt_ready_ms" in report:
        parts.append(f"STT {_ms(report['stt_ready_ms'])}")
    if "llm_ms" in report:
        detail = (
            f" (1.er token {_ms(report['llm_ttft_ms'])})"
            if "llm_ttft_ms" in report
            else ""
        )
        parts.append(f"LLM {_ms(report['llm_ms'])}{detail}")
    if "tts_synth_ms" in report:
        parts.append(f"TTS {_ms(report['tts_synth_ms'])} síntesis")
    if "tts_play_ms" in report:
        parts.append(f"Voz {_ms(report['tts_play_ms'])}")
    if "ttfa_ms" in report:
        parts.append(f"1.er audio a los {_ms(report['ttfa_ms'])}")
    if "turn_total_ms" in report:
        parts.append(f"Turno {_ms(report['turn_total_ms'])}")
    return " | ".join(parts)


class LatencyStats:
    """Acumula los turnos y publica media y P95 cada diez turnos."""

    def __init__(self, maxlen: int = 50, jsonl_path: Path | None = None) -> None:
        self._turns: deque[dict[str, float]] = deque(maxlen=maxlen)
        self._total = 0
        self._jsonl = jsonl_path

    def _append_jsonl(self, record: dict[str, object]) -> None:
        """Escribe una línea JSON por turno; un fallo de disco nunca rompe la voz."""
        if self._jsonl is None:
            return
        try:
            self._jsonl.parent.mkdir(parents=True, exist_ok=True)
            with self._jsonl.open("a", encoding="utf-8") as handle:
                handle.write(json.dumps(record, ensure_ascii=False) + "\n")
        except OSError:
            pass

    def record_command(self, name: str, handled_ms: float) -> None:
        """Registra un comando local (sin LLM): tiempo hasta ejecutarlo."""
        self._append_jsonl(
            {"ts": time.time(), "kind": "command", "name": name,
             "handled_ms": round(handled_ms, 1)}
        )

    def record(self, timings: TurnTimings) -> dict[str, float]:
        report = timings.report()
        self._turns.append(report)
        self._total += 1
        self._append_jsonl(
            {"ts": time.time(), "kind": "turn",
             **{key: round(value, 1) for key, value in report.items()}}
        )
        print(f"[ezagent-voice] Tiempos: {format_turn(report)}", flush=True)
        if self._total >= 10 and self._total % 10 == 0:
            self._log_summary()
        return report

    def _log_summary(self) -> None:
        labels = (
            ("vad_wait_ms", "VAD"),
            ("stt_ready_ms", "STT"),
            ("llm_ms", "LLM"),
            ("llm_ttft_ms", "LLM 1.er token"),
            ("tts_synth_ms", "TTS"),
            ("ttfa_ms", "1.er audio"),
        )
        parts = []
        for key, label in labels:
            values = [t[key] for t in self._turns if key in t]
            if not values:
                continue
            parts.append(
                f"{label} media {sum(values) / len(values):.0f} ms "
                f"P95 {self.percentile(values, 95):.0f} ms"
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


__all__ = ["LatencyStats", "TurnTimings", "format_stt", "format_turn"]
