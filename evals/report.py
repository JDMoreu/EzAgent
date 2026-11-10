"""Resumen legible de ``evals/output/<suite>.json`` (la salida de promptfoo).

    uv run python evals/report.py live
    uv run python evals/report.py offline
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent


def main(argv: list[str]) -> int:
    suite = argv[1] if len(argv) > 1 else "live"
    path = HERE / "output" / f"{suite}.json"
    if not path.is_file():
        print(f"No existe {path}. Ejecuta primero: uv run python evals/run_evals.py {suite}")
        return 2
    rows = json.loads(path.read_text("utf-8"))["results"]["results"]
    failed = 0
    for row in rows:
        description = (row.get("testCase") or {}).get("description", "?")
        if row.get("success"):
            print(f"PASA   {description}")
            continue
        failed += 1
        grading = row.get("gradingResult") or {}
        reasons = [c.get("reason", "") for c in grading.get("componentResults", []) if not c.get("pass")]
        reason = " || ".join(r for r in reasons if r) or str(row.get("error") or grading.get("reason") or "")
        print(f"FALLA  {description}\n         -> {reason[:400]}")
    print(f"\n{len(rows) - failed}/{len(rows)} pasan, {failed} fallan")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
