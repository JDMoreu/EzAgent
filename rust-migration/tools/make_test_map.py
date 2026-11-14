"""Genera `parity/test-map.csv` desde `pytest --collect-only -q` (F1-13).

Cada prueba de pytest tiene una fila. Al inicio de la migración todas están en
`pendiente`; cada tarea de Rust pasa sus filas a `portada` o
`cubierta_por_fixture:<dominio>` (o `no_aplica:<motivo>`, que el revisor
justifica). F7 cierra el mapa; F9 exige el 100 %.

Uso: uv run python rust-migration/tools/make_test_map.py
"""

from __future__ import annotations

import csv
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[2]

# (fragmento del nombre de la prueba, crate de Rust)
CRATE_HINTS = [
    ("swarm", "ez-swarm"),
    ("board", "ez-swarm"),
    ("ezs", "ez-swarm"),
    ("hud", "ez-hud"),
    ("drop", "ez-hud"),
    ("menu", "ez-hud"),
    ("settings", "ez-hud"),
    ("chat", "ez-hud"),
    ("peek", "ez-hud"),
    ("theme", "ez-hud"),
    ("bar", "ez-hud"),
    ("card", "ez-hud"),
    ("tray", "ez-app"),
    ("hotkey", "ez-app"),
    ("audio", "ez-audio"),
    ("segmenter", "ez-audio"),
    ("turn", "ez-audio"),
    ("wake", "ez-audio"),
    ("stt", "ez-stt"),
    ("whisper", "ez-stt"),
    ("tts", "ez-tts"),
    ("kokoro", "ez-tts"),
    ("speech", "ez-tts"),
    ("llm", "ez-llm"),
    ("tool", "ez-agent"),
    ("agent", "ez-agent"),
    ("memory", "ez-agent"),
    ("skill", "ez-agent"),
    ("policy", "ez-agent"),
    ("audit", "ez-agent"),
    ("confirm", "ez-agent"),
    ("herdr", "ez-herdr"),
    ("watcher", "ez-herdr"),
    ("config", "ez-core"),
    ("path", "ez-core"),
    ("manifest", "ez-core"),
    ("download", "ez-core"),
    ("redact", "ez-core"),
    ("text", "ez-core"),
    ("metrics", "ez-core"),
]


def crate_for(test_id: str) -> str:
    lowered = test_id.lower()
    for fragment, crate in CRATE_HINTS:
        if fragment in lowered:
            return crate
    return "ez-app"


def collect() -> list[str]:
    result = subprocess.run(
        ["uv", "run", "pytest", "--collect-only", "-q"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=900,
    )
    ids = [line.strip() for line in result.stdout.splitlines() if "::" in line]
    if not ids:
        raise SystemExit("pytest no devolvió pruebas")
    return sorted(ids)


def main() -> int:
    ids = collect()
    out = ROOT / "parity" / "test-map.csv"
    with out.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(["python_test_id", "rust_crate", "rust_test_path", "estado"])
        for test_id in ids:
            writer.writerow([test_id, crate_for(test_id), "", "pendiente"])
    print(f"{len(ids)} pruebas -> {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
