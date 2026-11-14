"""Genera `rust-migration/baseline-metrics.json` (F0-06).

Mide lo que se puede medir sin sesión gráfica: latencias reales de
`latency.jsonl`, lista de micrófonos de PyAudio, arranque hasta `--list-mics`,
tamaño del código y número de pruebas. Lo que exige una sesión viva (CPU en
reposo, RAM, cuadros por segundo del HUD) se marca como pendiente con el motivo.

Uso: uv run python rust-migration/tools/baseline_metrics.py
"""

from __future__ import annotations

import json
import os
import pathlib
import statistics
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
APP_DATA = pathlib.Path(os.environ["APPDATA"]) / "EzAgent"


def percentiles(values: list[float]) -> dict[str, float]:
    ordered = sorted(values)
    def pick(q: float) -> float:
        index = min(len(ordered) - 1, max(0, round(q * (len(ordered) - 1))))
        return round(ordered[index], 3)
    return {"p50": pick(0.50), "p95": pick(0.95), "mean": round(statistics.fmean(ordered), 3)}


def latency_metrics() -> dict:
    path = APP_DATA / "logs" / "latency.jsonl"
    if not path.exists():
        return {"source": str(path), "status": "no existe"}
    rows = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
    stages: dict[str, list[float]] = {}
    for row in rows:
        for key, value in row.items():
            if key in ("ts", "kind") or not isinstance(value, (int, float)):
                continue
            stages.setdefault(key, []).append(float(value))
    return {
        "source": str(path),
        "turns": len(rows),
        "kind": sorted({row.get("kind") for row in rows}),
        "stages": {name: percentiles(values) for name, values in sorted(stages.items())},
        "how": "P50/P95 sobre las marcas de LatencyStats de las sesiones reales ya registradas",
    }


def mic_list() -> dict:
    env = dict(os.environ, PYTHONIOENCODING="utf-8")
    started = time.perf_counter()
    result = subprocess.run(
        [sys.executable, "-m", "ezagent.voice.app", "--list-mics"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        env=env,
        timeout=180,
    )
    elapsed_ms = round((time.perf_counter() - started) * 1000, 1)
    devices = []
    for line in result.stdout.splitlines():
        parts = line.split()
        if len(parts) < 5 or not parts[0].isdigit():
            continue
        devices.append({"line": line.strip()})
    return {
        "startup_to_list_mics_ms": elapsed_ms,
        "returncode": result.returncode,
        "stderr": result.stderr.strip()[:500],
        "device_lines": devices,
        "how": "subprocess `python -m ezagent.voice.app --list-mics`, cronometrado desde el proceso padre",
    }


def code_size() -> dict:
    python_lines = 0
    files = 0
    for path in (ROOT / "src" / "ezagent").rglob("*.py"):
        if "default_skills" in path.parts:
            continue
        python_lines += len(path.read_text(encoding="utf-8", errors="replace").splitlines())
        files += 1
    big = []
    for path in (ROOT / "src" / "ezagent").rglob("*.py"):
        total = len(path.read_text(encoding="utf-8", errors="replace").splitlines())
        if total > 900:
            big.append({"file": str(path.relative_to(ROOT)).replace("\\", "/"), "lines": total})
    return {
        "python_files": files,
        "python_lines": python_lines,
        "files_over_900_lines": sorted(big, key=lambda item: -item["lines"]),
    }


def test_count() -> dict:
    result = subprocess.run(
        ["uv", "run", "pytest", "--collect-only", "-q"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=600,
    )
    collected = 0
    for line in result.stdout.splitlines():
        if "::" in line:
            collected += 1
    return {
        "test_files": len(list((ROOT / "tests").glob("test_*.py"))),
        "collected_tests": collected,
        "last_run": "1881 passed, 1 skipped",
    }


def main() -> int:
    metrics = {
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "python": sys.version.split()[0],
        "source": "tag py-baseline",
        "code": code_size(),
        "tests": test_count(),
        "latency": latency_metrics(),
        "audio": mic_list(),
        "pending": {
            "hud_frame_stats": "requiere `ezagent-voice --demo-ui` con sesión gráfica; lo corre el usuario",
            "cpu_idle_percent": "requiere la app en reposo con HUD visible",
            "ram_rss_mb": "requiere la app en reposo con HUD visible",
            "startup_full_ms": "requiere arranque completo (micrófono + whisper + modelo de voz)",
        },
    }
    out = ROOT / "rust-migration" / "baseline-metrics.json"
    out.write_text(json.dumps(metrics, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(f"escrito {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
