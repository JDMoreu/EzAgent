"""Fixtures de latencia del turno de voz (dominio `metrics`, F1-07).

Cubre `src/ezagent/voice/metrics.py`: `TurnTimings`, `format_stt`,
`format_turn`, `LatencyStats.percentile` y la escritura de `latency.jsonl`.

`TurnTimings.mark` y `LatencyStats` usan `time.monotonic`/`time.time`; aquí se
sustituyen por un reloj fijo (`1234.5` monotónico y el instante congelado del
perfil) para que las marcas implícitas y `ts` sean deterministas.

La salida de cada escenario incluye los informes devueltos, las marcas y
duraciones finales, y el **texto exacto** de `latency.jsonl`.
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "metrics"

MONOTONIC = 1234.5


def _norm(value: Any, home: Path) -> Any:
    if isinstance(value, str):
        value = value.replace("\\\\", "/")
        for prefix in (home.as_posix(), str(home)):
            value = value.replace(prefix, "<tmp>")
        return value.replace("\\", "/")
    if isinstance(value, dict):
        return {key: _norm(item, home) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_norm(item, home) for item in value]
    return value


def _file_text(path: Path) -> Any:
    if path.is_dir():
        return "<dir>"
    if not path.exists():
        return None
    return path.read_text(encoding="utf-8", errors="replace")


def _timings(spec: dict[str, Any]) -> Any:
    from ezagent.voice.metrics import TurnTimings

    timings = TurnTimings(spec.get("speech_end", 0.0), spec.get("vad_end", 0.0))
    for name, at in (spec.get("marks") or {}).items():
        timings.mark(name, at)
    for name, seconds in (spec.get("spans") or {}).items():
        timings.add_span(name, seconds)
    return timings


REPORT_CASES: list[dict[str, Any]] = [
    {"speech_end": 0.0, "vad_end": 0.2},
    {"speech_end": 10.0, "vad_end": 10.25},
    {"speech_end": 20.0, "vad_end": 19.5},
    {
        "speech_end": 0.0,
        "vad_end": 100.0,
        "marks": {
            "stt_ready": 100.5,
            "first_token": 101.25,
            "first_sentence": 102.0,
            "first_sound": 103.5,
            "turn_end": 104.0,
        },
    },
    {
        "speech_end": 1.0,
        "vad_end": 2.0,
        "marks": {"first_token": 2.5, "first_sentence": 3.0, "turn_end": 4.0},
    },
    {
        "speech_end": 1.0,
        "vad_end": 2.0,
        "marks": {"first_sentence": 3.0, "first_sound": 4.0},
    },
    {
        "speech_end": 1.0,
        "vad_end": 2.0,
        "marks": {"first_sound": 3.0},
    },
    {
        "speech_end": 1.0,
        "vad_end": 2.0,
        "marks": {"turn_end": 5.0},
    },
    {
        "speech_end": 1.0,
        "vad_end": 2.0,
        "marks": {"first_ack": 2.25, "turn_end": 5.0},
    },
    {
        "speech_end": 1.0,
        "vad_end": 2.0,
        "marks": {"llm_start": 2.1, "first_token": 2.7},
    },
    {
        "speech_end": 1.0,
        "vad_end": 2.0,
        "marks": {"llm_start": 2.1},
    },
    {
        "speech_end": 1.0,
        "vad_end": 2.0,
        "spans": {"llm": 0.5, "tts_synth": 0.25, "tts_play": 1.75},
    },
    {
        "speech_end": 1.0,
        "vad_end": 2.0,
        "spans": {"llm": 0.0, "tts_synth": -1.0, "tts_play": 0.5},
    },
    {
        "speech_end": 1.0,
        "vad_end": 2.0,
        "spans": {"llm": 0.1, "otro": 9.0},
    },
    {
        "speech_end": 0.0,
        "vad_end": 0.0,
        "marks": {"turn_end": 0.0, "first_ack": 0.0, "first_sound": 0.0},
    },
    {
        "speech_end": 5.0,
        "vad_end": 5.0,
        "marks": {
            "stt_ready": 5.1,
            "first_token": 5.2,
            "first_sentence": 5.3,
            "first_sound": 5.4,
            "turn_end": 5.5,
            "llm_start": 5.05,
            "first_ack": 5.15,
        },
        "spans": {"llm": 0.2, "tts_synth": 0.3, "tts_play": 0.4},
    },
    {
        "speech_end": 5.0,
        "vad_end": 5.0,
        "marks": {"stt_ready": 5.1, "first_sound": 5.2, "first_sentence": 5.3, "turn_end": 5.4},
    },
    {
        "speech_end": 5.0,
        "vad_end": 5.0,
        "marks": {"turn_end": 4.0},
    },
    {
        "speech_end": 5.0,
        "vad_end": 6.0,
        "marks": {"stt_ready": 6.0, "first_token": 6.0005},
    },
    {
        "speech_end": 1.0,
        "vad_end": 2.0,
        "marks": {"ñ": 3.0, "stt_ready": 2.0},
    },
]

MARK_ERRORS: list[tuple[str, Any]] = [
    ("mark", "no-numérico"),
    ("span", "no-numérico"),
]

PERCENTILE_CASES: list[dict[str, Any]] = [
    {"values": [], "pct": 95.0},
    {"values": [5.0], "pct": 95.0},
    {"values": [1.0, 2.0, 3.0, 4.0], "pct": 50.0},
    {"values": [1.0, 2.0, 3.0, 4.0], "pct": 95.0},
    {"values": [10.0, 0.0, 5.0], "pct": 0.0},
    {"values": [10.0, 0.0, 5.0], "pct": 100.0},
    {"values": [10.0, 0.0, 5.0], "pct": 25.5},
    {"values": [1.0, 1.0, 1.0], "pct": 50.0},
    {"values": [-5.0, -1.0, -10.0], "pct": 50.0},
    {"values": [0.0005, 0.001, 0.002], "pct": 95.0},
    {"values": list(range(1, 101)), "pct": 95.0},
]

FORMAT_STT_CASES: list[dict[str, float]] = [
    {"vad_wait_ms": 0.0},
    {"vad_wait_ms": 200.0},
    {"vad_wait_ms": 200.4, "stt_ready_ms": 300.6},
    {"vad_wait_ms": 0.5, "stt_ready_ms": 1.5},
    {"vad_wait_ms": 2.5, "stt_ready_ms": 3.5},
    {"vad_wait_ms": -12.5},
]

FORMAT_TURN_CASES: list[dict[str, float]] = [
    {"vad_wait_ms": 200.0},
    {"vad_wait_ms": 200.0, "stt_ready_ms": 300.0},
    {"vad_wait_ms": 1.0, "stt_ready_ms": 2.0, "llm_ms": 300.0, "llm_ttft_ms": 150.0},
    {"vad_wait_ms": 1.0, "llm_ms": 300.0},
    {"vad_wait_ms": 1.0, "tts_synth_ms": 20.0, "tts_play_ms": 500.0},
    {"vad_wait_ms": 1.0, "ttfa_ms": 700.0},
    {"vad_wait_ms": 1.0, "turn_total_ms": 900.0},
    {
        "vad_wait_ms": 1.0,
        "stt_ready_ms": 2.0,
        "llm_ms": 3.0,
        "llm_ttft_ms": 4.0,
        "tts_synth_ms": 5.0,
        "tts_play_ms": 6.0,
        "ttfa_ms": 7.0,
        "turn_total_ms": 8.0,
    },
    {"vad_wait_ms": 0.4, "stt_ready_ms": 0.5, "llm_ms": 1.5, "ttfa_ms": 2.5},
]


def _report_cases(out: FixtureFile, metrics: Any) -> None:
    for index, spec in enumerate(REPORT_CASES, start=1):
        timings = _timings(spec)
        report = timings.report()
        out.add(
            f"metrics/report/{index:03d}",
            "TurnTimings.report",
            spec,
            {
                "report": report,
                "keys": list(report.keys()),
                "marks": dict(timings.marks),
                "spans": dict(timings.spans),
            },
        )
    timings = _timings({"speech_end": 0.0, "vad_end": 0.0})
    for index, (kind, value) in enumerate(MARK_ERRORS, start=1):
        try:
            if kind == "mark":
                timings.mark("x", value)
            else:
                timings.add_span("x", value)
        except (TypeError, ValueError) as error:
            out.add_error(
                f"metrics/errors/{index:03d}",
                "TurnTimings.mark" if kind == "mark" else "TurnTimings.add_span",
                {"kind": kind, "value": value},
                str(error),
            )
        else:  # pragma: no cover - el oráculo debe registrar el fallo
            raise AssertionError(f"{kind} aceptó {value!r}")
    default_mark = _timings({"speech_end": 0.0, "vad_end": 0.0})
    default_mark.mark("stt_ready")
    out.add(
        "metrics/mark/default",
        "TurnTimings.mark",
        {"name": "stt_ready"},
        {"marks": dict(default_mark.marks)},
    )
    for index, report in enumerate(FORMAT_STT_CASES, start=1):
        out.add(
            f"metrics/format_stt/{index:03d}",
            "format_stt",
            {"report": report},
            {"text": metrics.format_stt(report)},
        )
    for index, report in enumerate(FORMAT_TURN_CASES, start=1):
        out.add(
            f"metrics/format_turn/{index:03d}",
            "format_turn",
            {"report": report},
            {"text": metrics.format_turn(report)},
        )
    for index, case in enumerate(PERCENTILE_CASES, start=1):
        out.add(
            f"metrics/percentile/{index:03d}",
            "LatencyStats.percentile",
            case,
            {"value": metrics.LatencyStats.percentile(case["values"], case["pct"])},
        )
    out.add(
        "metrics/constants",
        "module_constants",
        {},
        {"meta_keys": list(metrics._META_KEYS)},
    )


STATS_CASES: list[dict[str, Any]] = [
    {
        "id": "basic",
        "maxlen": 50,
        "ops": [
            {
                "kind": "turn",
                "timings": {
                    "speech_end": 0.0,
                    "vad_end": 0.1,
                    "marks": {"stt_ready": 0.2, "first_sound": 0.9, "turn_end": 1.0},
                },
                "meta": {"provider": "openai", "model": "gpt", "rounds": 2},
            },
            {"kind": "command", "name": "abrir chat", "handled_ms": 1.25},
        ],
    },
    {
        "id": "meta-whitelist",
        "maxlen": 3,
        "ops": [
            {
                "kind": "turn",
                "timings": {"speech_end": 0.0, "vad_end": 0.0},
                "meta": {
                    "provider": "openai",
                    "model": "gpt",
                    "rounds": 1,
                    "outcome": "completed",
                    "cancelled": False,
                    "first_ack_ms": 12.5,
                    "transcript": "NO GUARDAR",
                    "api_key": "SUPERSECRETO",
                    "rounds_none": None,
                },
            },
            {"kind": "turn", "timings": {"speech_end": 1.0, "vad_end": 2.0}, "meta": {}},
            {"kind": "turn", "timings": {"speech_end": 2.0, "vad_end": 3.0}},
            {
                "kind": "turn",
                "timings": {"speech_end": 3.0, "vad_end": 4.0},
                "meta": {"model": "otro"},
            },
            {"kind": "turn", "timings": {"speech_end": 4.0, "vad_end": 5.0}, "meta": {"rounds": 9}},
        ],
    },
    {
        "id": "rounding",
        "maxlen": 50,
        "ops": [
            {
                "kind": "turn",
                "timings": {
                    "speech_end": 0.0,
                    "vad_end": 0.00025,
                    "marks": {"first_sound": 0.0006},
                },
            },
            {"kind": "command", "name": "a", "handled_ms": 2.55},
            {"kind": "command", "name": "b", "handled_ms": -0.04},
            {"kind": "command", "name": "c", "handled_ms": 12.3456},
            {"kind": "command", "name": "d", "handled_ms": 0.0},
        ],
    },
    {
        "id": "ten-turns",
        "maxlen": 50,
        "ops": [
            {
                "kind": "turn",
                "timings": {
                    "speech_end": 0.0,
                    "vad_end": 0.1,
                    "marks": {"stt_ready": 0.2},
                    "spans": {"llm": 0.1, "tts_synth": 0.2, "tts_play": 0.3},
                },
                "meta": {"provider": "p", "model": "m", "rounds": index, "outcome": "completed"},
            }
            for index in range(10)
        ],
    },
    {
        "id": "maxlen",
        "maxlen": 3,
        "ops": [
            {"kind": "turn", "timings": {"speech_end": float(index), "vad_end": float(index) + 0.5}}
            for index in range(5)
        ],
    },
    {
        "id": "disk-failure",
        "maxlen": 50,
        "jsonl": "<dir>",
        "ops": [
            {"kind": "turn", "timings": {"speech_end": 0.0, "vad_end": 1.0}},
            {"kind": "command", "name": "x", "handled_ms": 1.0},
        ],
    },
]


def _stats_cases(out: FixtureFile, home: Path, metrics: Any) -> None:
    for case in STATS_CASES:
        work = home / "metrics-cases" / case["id"]
        path = work / "latency.jsonl"
        if case.get("jsonl") == "<dir>":
            path.mkdir(parents=True, exist_ok=True)
        stats = metrics.LatencyStats(maxlen=case["maxlen"], jsonl_path=path)
        reports: list[dict[str, float]] = []
        for op in case["ops"]:
            if op["kind"] == "turn":
                timings = _timings(op["timings"])
                reports.append(stats.record(timings, op.get("meta")))
            else:
                stats.record_command(op["name"], op["handled_ms"])
                reports.append({"command": op["name"], "handled_ms": op["handled_ms"]})
        out.add(
            f"metrics/stats/{case['id']}",
            "LatencyStats",
            {
                "maxlen": case["maxlen"],
                "jsonl": case.get("jsonl"),
                "ops": case["ops"],
            },
            _norm(
                {
                    "reports": reports,
                    "turns": [[report, extra] for report, extra in stats._turns],
                    "turns_len": len(stats._turns),
                    "total": stats._total,
                    "file": _file_text(path),
                },
                home,
            ),
        )


def generate(out: FixtureFile) -> None:
    from ezagent.voice import metrics

    home = Path(os.environ["HOME"])
    original_monotonic = metrics.time.monotonic
    metrics.time.monotonic = lambda: MONOTONIC
    try:
        _report_cases(out, metrics)
        _stats_cases(out, home, metrics)
    finally:
        metrics.time.monotonic = original_monotonic


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
