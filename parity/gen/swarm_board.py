"""Tablero de prueba, volcado SQLite y transcripciones de `ezs` (dominio `swarm`).

Los ids de enjambre y los tokens son aleatorios por diseño y el perfil temporal
cambia en cada corrida: aquí se normalizan a «<swarm>», «<token>» y «<tmp>». Las
columnas de tiempo se guardan como segundos respecto a `BASE` (el reloj fijo con
el que se abren las bases), así el volcado no depende de la zona horaria.
"""

from __future__ import annotations

import json
import re
import shutil
import sqlite3
from datetime import datetime
from pathlib import Path
from typing import Any

BASE = datetime(2025, 1, 1, 20, 0, 0).timestamp()
TABLES = ("cursor", "incident", "lease", "member", "message", "swarm", "task")
TIME_COLUMNS = frozenset({"created", "updated", "ts", "seen", "expires", "resets"})
STAMP = re.compile(r"\d{8}-\d{6}")


def at(day: int = 1, hour: int = 20, minute: int = 0) -> float:
    """Instante local del día fijo del tablero (época)."""
    return datetime(2025, 1, day, hour, minute).timestamp()


TEAM = (
    {"op": "join", "name": "ana", "role": "plan", "agent": "claude", "model": "opus"},
    {"op": "join", "name": "bo", "role": "dev", "agent": "opencode"},
    {"op": "join", "name": "cy", "role": "qa", "agent": "codex", "effort": "high"},
)


def info(goal: str = "Migrar el núcleo a Rust", status: str = "active") -> Any:
    from ezagent.swarm.models import SwarmInfo

    return SwarmInfo("abc12345", goal, "C:/proyecto", status, BASE, BASE)


def member(
    name: str, role: str = "", agent: str = "claude", state: str = "idle", model: str = ""
) -> Any:
    from ezagent.swarm.models import Member

    return Member(name, role, agent, "seat-1", state, BASE, model)


def task(
    task_id: int,
    title: str,
    *,
    role: str = "",
    owner: str = "",
    state: str = "pending",
    deps: tuple[int, ...] = (),
    parent: int = 0,
    result: str = "",
    rounds: int = 0,
) -> Any:
    from ezagent.swarm.models import Task

    return Task(task_id, title, role, owner, state, deps, parent, result, rounds, BASE)


def open_store(path: Path) -> Any:
    from ezagent.swarm.store import SwarmStore

    path.parent.mkdir(parents=True, exist_ok=True)
    return SwarmStore(path, clock=lambda: BASE)


def board(home: Path, name: str, spec: dict[str, Any] | None = None) -> tuple[Any, str]:
    """Tablero nuevo en `home/<name>` sembrado con `spec`; devuelve (store, id)."""
    store = open_store(home / "cases" / name / "board.db")
    return store, seed(store, spec or {})


def seed(store: Any, spec: dict[str, Any]) -> str:
    swarm = store.create(
        spec.get("goal", "Objetivo de prueba"),
        cwd=spec.get("cwd", ""),
        status=spec.get("status", "active"),
    )
    for op in spec.get("ops", []):
        apply(store, swarm, op)
    return swarm


def apply(store: Any, swarm: str, op: dict[str, Any]) -> None:
    """Aplica una operación del lenguaje del tablero (una línea por efecto)."""
    kind = op["op"]
    if kind == "join":
        store.join(
            swarm,
            op["name"],
            role=op.get("role", ""),
            agent=op.get("agent", ""),
            address=op.get("address", ""),
            model=op.get("model", ""),
            effort=op.get("effort", ""),
        )
    elif kind == "task":
        store.add_task(
            swarm,
            op["title"],
            role=op.get("role", ""),
            owner=op.get("owner", ""),
            deps=op.get("deps", ()),
            parent=op.get("parent", 0),
        )
    elif kind == "claim":
        store.claim(swarm, op["member"], op.get("task"))
    elif kind == "finish":
        store.finish(
            swarm,
            op["member"],
            op["task"],
            op.get("result", ""),
            ok=op.get("ok", True),
            ref=op.get("ref", ""),
        )
    elif kind == "reopen":
        store.reopen(
            swarm, op["task"], by=op.get("by", "ez"), note=op.get("note", ""), ref=op.get("ref", "")
        )
    elif kind == "fill":
        for index in range(op["count"]):
            store.add_task(swarm, f"{op.get('prefix', 'Tarea')} {index + 1}", role=op.get("role", ""))
    elif kind == "incident":
        store.open_incident(
            swarm,
            op["member"],
            op["kind"],
            op.get("detail", ""),
            resets=at(**op["resets"]) if op.get("resets") else 0.0,
        )
    else:
        _apply_mail(store, swarm, op)


def _apply_mail(store: Any, swarm: str, op: dict[str, Any]) -> None:
    kind = op["op"]
    if kind == "post":
        store.post(
            swarm,
            op.get("sender", "ez"),
            op["body"],
            to=op.get("to", "*"),
            kind=op.get("kind", "info"),
            ref=op.get("ref", ""),
        )
    elif kind == "reserve":
        store.reserve(swarm, op["member"], op["paths"], ttl=op.get("ttl", 900.0))
    elif kind == "release":
        store.release(swarm, op["member"], op.get("paths"))
    elif kind == "incident_state":
        store.set_incident(op["id"], op["state"], op.get("action", ""))
    elif kind == "member_state":
        store.set_member_state(swarm, op["member"], op["state"])
    elif kind == "address":
        store.set_address(swarm, op["member"], op["address"])
    elif kind == "status":
        store.set_status(swarm, op["status"])
    else:  # pragma: no cover - el oráculo debe registrar el fallo
        raise ValueError(f"operación desconocida: {kind}")


def normalize_text(value: str, swarm: str, home: Path) -> str:
    text = value.replace(swarm, "<swarm>") if swarm else value
    for prefix in (str(home), home.as_posix()):
        text = text.replace(prefix, "<tmp>")
    text = STAMP.sub("<stamp>", text)
    return text.replace("\\", "/")


def transcript(case: dict[str, Any], home: Path) -> dict[str, Any]:
    """Corre un comando de `ezs` sobre un tablero y devuelve salida y volcado."""
    from ezagent.swarm.service import SwarmService

    work = home / "cases" / case["id"]
    work.mkdir(parents=True, exist_ok=True)
    store = open_store(work / "board.db")
    swarm = seed(store, case.get("board", {}))
    service = SwarmService(store, clock=lambda: BASE)
    tokens = {member.name: service.issue_token(swarm, member.name) for member in store.members(swarm)}
    for argv in case.get("before", []):
        service.handle(tokens[case["token_of"]], argv)
    if case.get("closed"):
        service.close()
    token = "token-de-otro-enjambre" if case.get("bad_token") else tokens[case["token_of"]]
    reply = service.handle(token, case["argv"])
    text = normalize_text(reply.text, swarm, home)
    payload = {
        "ok": reply.ok,
        "exit": 0 if reply.ok else 1,
        "stdout": text if reply.ok else "",
        "stderr": "" if reply.ok else text,
    }
    if case.get("dump", True):
        payload["board"] = dump_board(work / "board.db", swarm, home)
    return payload


def dump_board(path: Path, swarm: str, home: Path) -> dict[str, list[dict[str, Any]]]:
    """Copia la base (con su WAL) y devuelve sus filas normalizadas y ordenadas."""
    copy = path.parent / "copied"
    copy.mkdir(exist_ok=True)
    target = copy / "board.db"
    for suffix in ("", "-wal", "-shm"):
        source = path.with_name(path.name + suffix)
        if source.exists():
            shutil.copyfile(source, target.with_name(target.name + suffix))
    connection = sqlite3.connect(str(target))
    connection.row_factory = sqlite3.Row
    try:
        return {table: _rows(connection, table, swarm, home) for table in TABLES}
    finally:
        connection.close()


def _rows(connection: Any, table: str, swarm: str, home: Path) -> list[dict[str, Any]]:
    items: list[dict[str, Any]] = []
    for row in connection.execute(f"SELECT * FROM {table}").fetchall():
        data: dict[str, Any] = {}
        for key in row.keys():
            value = row[key]
            if isinstance(value, str):
                value = normalize_text(value, swarm, home)
                if key == "token" and value:
                    value = "<token>"
                elif key == "swarm" or (table == "swarm" and key == "id"):
                    value = "<swarm>"
            elif isinstance(value, float) and key in TIME_COLUMNS:
                value = round(value - BASE, 3) if value else 0.0
            data[key] = value
        items.append(data)
    items.sort(key=lambda item: json.dumps(item, ensure_ascii=True, sort_keys=True))
    return items
