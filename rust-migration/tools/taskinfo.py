"""Devuelve en JSON los datos de una tarea de `rust-migration/tasks.yaml`.

Uso: uv run python rust-migration/tools/taskinfo.py F3-07
"""

from __future__ import annotations

import json
import pathlib
import sys

import yaml

ROOT = pathlib.Path(__file__).resolve().parents[2]


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("uso: taskinfo.py <ID>", file=sys.stderr)
        return 2
    wanted = argv[1]
    data = yaml.safe_load((ROOT / "rust-migration" / "tasks.yaml").read_text(encoding="utf-8"))
    for task in data["tasks"]:
        if task["id"] == wanted:
            crates = sorted(
                {
                    str(item).split("/")[2]
                    for item in task["files"]
                    if isinstance(item, str) and item.startswith("rust/crates/")
                }
            )
            print(json.dumps({"task": task, "crates": crates}, ensure_ascii=True))
            return 0
    print(f"tarea desconocida: {wanted}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
