"""Servidor del enjambre de Python para `run_compat.py` (F7-03).

Siembra el mismo enjambre que el servidor de Rust, publica ``endpoint.json`` y
atiende hasta que se cierra la entrada estándar. No imprime nada: el arnés lee
la dirección y el token del archivo que indica ``--out``.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def _root() -> Path:
    return Path(__file__).resolve().parents[2]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args(argv)
    sys.path.insert(0, str(_root() / "src"))
    out = Path(args.out)
    try:
        from ezagent.swarm.server import SwarmServer
        from ezagent.swarm.service import SwarmService
        from ezagent.swarm.store import SwarmStore

        base = Path(args.base)
        base.mkdir(parents=True, exist_ok=True)
        service = SwarmService(SwarmStore(base / "board.db"))
        swarm, seats = service.start(
            "Compatibilidad F7-03",
            [("ana", "dev", "opencode"), ("bo", "qa", "codex")],
            tasks=[
                ("Primera tarea de compatibilidad", "dev"),
                ("Segunda tarea de compatibilidad", "dev"),
            ],
        )
        token = next(seat.token for seat in seats if seat.name == "ana")
        server = SwarmServer(service, base / "endpoint.json")
        url = server.start()
        out.write_text(
            json.dumps({"url": url, "swarm": swarm, "token": token}),
            encoding="utf-8",
        )
    except Exception as exc:  # noqa: BLE001 - el arnés necesita el texto exacto
        out.write_text(json.dumps({"error": f"{type(exc).__name__}: {exc}"}), encoding="utf-8")
        return 1
    try:
        sys.stdin.read()
    finally:
        server.stop()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
