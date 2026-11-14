"""Cambia el estado de una tarea en `tasks.yaml` (uso del orquestador).

Uso:
    uv run python rust-migration/tools/task_state.py F3-07 done
    uv run python rust-migration/tools/task_state.py F3-07 rejected --round 2 --note "…"
"""

from __future__ import annotations

import argparse
import pathlib

import yaml

ROOT = pathlib.Path(__file__).resolve().parents[2]
PATH = ROOT / "rust-migration" / "tasks.yaml"
STATES = {"todo", "in_progress", "review", "rejected", "done", "split", "needs_owner"}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("task")
    parser.add_argument("status", choices=sorted(STATES))
    parser.add_argument("--round", type=int, default=None)
    parser.add_argument("--note", default=None)
    args = parser.parse_args()

    data = yaml.safe_load(PATH.read_text(encoding="utf-8"))
    for task in data["tasks"]:
        if task["id"] != args.task:
            continue
        task["status"] = args.status
        if args.round is not None:
            task["round"] = args.round
        if args.note:
            task["note"] = args.note
        PATH.write_text(
            yaml.safe_dump(data, allow_unicode=True, sort_keys=False, width=200),
            encoding="utf-8",
            newline="\n",
        )
        print(f"{args.task}: {args.status}")
        return 0
    print(f"tarea desconocida: {args.task}")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
