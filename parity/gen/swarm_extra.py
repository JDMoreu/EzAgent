"""Fixtures del enjambre: modelos, tablero directo, `recovery` y `ezs` (F1-06).

Complementa a `swarm.py`: aquí van las validaciones de `models`, los métodos del
tablero ejercitados uno a uno (tablero vacío, lleno, duplicados, error), las
decisiones de `recovery` con un Herdr falso y los textos del CLI `ezs`.
"""

from __future__ import annotations

import contextlib
import io
import os
from dataclasses import asdict, is_dataclass
from pathlib import Path
from typing import Any

import swarm_board as sb

CAST_PATHS = [
    ("src/a.rs", 20),
    ("./src/a.rs", 20),
    ("src\\a.rs", 20),
    ("SRC/A.RS", 20),
    ("src//a.rs", 20),
    ("a/./b", 20),
    ("../x", 20),
    ("/", 20),
    ("", 20),
    (None, 20),
    ("a/", 20),
    ("././x", 20),
    (" a ", 20),
    ("a/b/../c", 20),
    ("x" * 261, 280),
    ("C:/x/y", 20),
    ("ñ/á", 20),
    ("a b", 20),
    ("a\x00b", 20),
    ("y" * 40, 40),
]

NAMES = [
    "ana", "Ana", "a", "a" * 32, "a" * 33, "9lives", "-x", "_x", "a b", "a.b",
    "ñ", "", None, "  ANA  ", "a-b_c", "A1", "ez", "todos", "rol largo con espacios", "b",
]

IDS = ["3", "T3", "t3", " 4 ", "T", "", None, "3x", "-1", "0003", "١٢"] + [
    "T 3", "+3", "3.0", "T-1", "T0", "0", "999", " 0x3 ", "TT3",
]


def _models_cases(out: Any) -> None:
    from ezagent.swarm import models

    for index, value in enumerate([None, "", "  hola  ", "a\n b\tc", "nulo\x00aquí", 5, ["x"], "ñ 🙂"], 1):
        out.add(f"swarm/models/flat/{index:02d}", "models.flat", {"value": value},
                {"text": models.flat(value)})

    for index, (value, limit) in enumerate(CAST_PATHS, 1):
        out.add(f"swarm/models/clip/{index:02d}", "models.clip", {"value": value, "limit": limit},
                {"text": models.clip(value, limit)[0], "clipped": models.clip(value, limit)[1]})

    for index, value in enumerate(NAMES, 1):
        _error_case(out, f"swarm/models/member-name/{index:02d}", "models.member_name",
                    {"value": value}, lambda v=value: models.member_name(v))
    for index, value in enumerate(
        ["", "Plan", "  QA  ", "x!", None, "otro rol", "a" * 33,
         *("plan", "arch", "lead", "dev", "back", "front", "ux", "data", "ops", "docs", "qa",
           "test", "sec")],
        1,
    ):
        _error_case(out, f"swarm/models/role-name/{index:02d}", "models.role_name",
                    {"value": value}, lambda v=value: models.role_name(v))
    for index, value in enumerate(IDS, 1):
        _error_case(out, f"swarm/models/task-id/{index:02d}", "models.task_id",
                    {"value": value}, lambda v=value: models.task_id(v))
    for index, (value, limit) in enumerate([("", 10), ("  ", 10), ("x" * 20, 10), ("ok", 10), (None, 10)], 1):
        _error_case(out, f"swarm/models/required/{index:02d}", "models.required",
                    {"value": value, "limit": limit, "what": "el objetivo"},
                    lambda v=value, n=limit: models.required(v, n, "el objetivo"))
    for index, (value, limit) in enumerate([("src/a.rs", 260), ("./src/a.rs", 260), ("src\\a.rs", 260),
                                           ("SRC/A.RS", 260), ("src//a.rs", 260), ("a/./b", 260),
                                           ("../x", 260), ("/", 260), ("", 260), (None, 260), ("a/", 260),
                                           ("././x", 260), (" a ", 260), ("a/b/../c", 260),
                                           ("x" * 261, 260), ("C:/x/y", 260), ("ñ/á", 260),
                                           ("a b", 260), ("a\x00b", 260), ("src/a.rs/", 260)], 1):
        _error_case(out, f"swarm/models/lease-path/{index:02d}", "models.lease_path",
                    {"value": value, "limit": limit}, lambda v=value: models.lease_path(v))
    for index, (first, second) in enumerate([("a", "a"), ("a/b", "a"), ("a", "a/b"), ("a/b", "a/c"),
                                             ("ab", "a"), ("a", ""), ("", ""), ("a/b/c", "a/b"),
                                             ("a/b", "a/b/c"), ("a/b", "ab"), ("a/b/c", "a/b/c"),
                                             ("a/b/c", "a"), ("a", "a/b/c"), ("x/y", "y"),
                                             ("x", "x/y/z"), ("a//b", "a"), ("A", "a"),
                                             ("src/a.rs", "src"), ("src", "src/a.rs"),
                                             ("src/a.rs", "src/b.rs")], 1):
        out.add(f"swarm/models/overlaps/{index:02d}", "models.overlaps",
                {"first": first, "second": second},
                {"overlaps": models.overlaps(first, second)})


def _error_case(out: Any, case: str, fn: str, payload: dict[str, Any], call: Any) -> None:
    try:
        result = call()
    except Exception as error:  # noqa: BLE001 - el oráculo guarda el texto real
        out.add_error(case, fn, payload, str(error))
    else:
        out.add(case, fn, payload, {"result": result})


# ------------------------------------------------------------------- tablero
TEAM = [
    {"op": "join", "name": "ana", "role": "plan", "agent": "claude"},
    {"op": "join", "name": "bo", "role": "dev", "agent": "opencode"},
    {"op": "join", "name": "cy", "role": "qa", "agent": "codex"},
]
BASE_BOARD = {"goal": "Migrar", "ops": list(TEAM)}
DONE_BOARD = {
    "goal": "Migrar",
    "ops": [*TEAM, {"op": "task", "title": "Portar", "role": "dev"},
            {"op": "claim", "member": "bo", "task": 1},
            {"op": "finish", "member": "bo", "task": 1, "result": "ok"}],
}

STORE_CASES: list[dict[str, Any]] = [
    {"id": "create-vacio", "method": "create", "args": [""]},
    {"id": "create-estado", "method": "create", "args": ["Nuevo"], "kwargs": {"status": "raro"}},
    {"id": "create-ok", "method": "create", "args": ["Nuevo enjambre"], "kwargs": {"cwd": "C:/x"},
     "flag": True},
    {"id": "get", "method": "get", "args": ["$swarm"]},
    {"id": "get-ajeno", "method": "get", "args": ["no-existe"]},
    {"id": "latest", "method": "latest", "args": []},
    {"id": "active", "method": "active", "args": []},
    {"id": "live", "method": "live", "args": []},
    {"id": "live-pausa", "method": "live", "args": [], "board": {"goal": "X", "status": "paused",
                                                                 "ops": list(TEAM)}},
    {"id": "set-status", "method": "set_status", "args": ["$swarm", "done"]},
    {"id": "set-status-raro", "method": "set_status", "args": ["$swarm", "raro"]},
    {"id": "join-duplicado", "method": "join", "args": ["$swarm", "ana"], "kwargs": {"role": "qa"}},
    {"id": "join-reservado", "method": "join", "args": ["$swarm", "ez"]},
    {"id": "join-todos", "method": "join", "args": ["$swarm", "todos"]},
    {"id": "join-nombre", "method": "join", "args": ["$swarm", "a b"]},
    {"id": "members", "method": "members", "args": ["$swarm"]},
    {"id": "pending", "method": "pending", "args": ["$swarm", "bo"],
     "board": {"goal": "X", "ops": [*TEAM,
                                    {"op": "post", "sender": "ana", "body": "para bo", "to": "bo"},
                                    {"op": "post", "sender": "cy", "body": "a todos", "to": "*"},
                                    {"op": "task", "title": "Portar", "role": "dev"}]}},
    {"id": "pending-vacio", "method": "pending", "args": ["$swarm", "bo"], "board": BASE_BOARD},
    {"id": "add-task", "method": "add_task", "args": ["$swarm", "Documentar"], "kwargs": {"role": "docs"}},
    {"id": "add-vacia", "method": "add_task", "args": ["$swarm", ""]},
    {"id": "add-ajena", "method": "add_task", "args": ["$swarm", "X"], "kwargs": {"owner": "nadie"}},
    {"id": "add-dependencia", "method": "add_task", "args": ["$swarm", "X"], "kwargs": {"deps": [9]}},
    {"id": "add-parte-de-parte", "method": "add_task", "args": ["$swarm", "Nieta"],
     "kwargs": {"parent": 3},
     "board": {"goal": "X", "ops": [*TEAM, {"op": "task", "title": "Madre", "role": "dev"},
                                    {"op": "task", "title": "Parte", "role": "dev", "parent": 1},
                                    {"op": "task", "title": "Otra", "role": "dev", "parent": 1}]}},
    {"id": "add-parte", "method": "add_task", "args": ["$swarm", "Lexer"], "kwargs": {"parent": 1},
     "board": {"goal": "X", "ops": [*TEAM, {"op": "task", "title": "Madre", "role": "dev"}]}},
    {"id": "claim", "method": "claim", "args": ["$swarm", "bo"]},
    {"id": "claim-todo", "method": "claim", "args": ["$swarm", "bo"], "kwargs": {"every": True},
     "board": {"goal": "X", "ops": [*TEAM, {"op": "task", "title": "Lexer", "role": "dev"},
                                    {"op": "task", "title": "Parser", "role": "dev"}]}},
    {"id": "claim-nada", "method": "claim", "args": ["$swarm", "bo"], "board": BASE_BOARD},
    {"id": "claim-otro", "method": "claim", "args": ["$swarm", "cy", 1], "board": BASE_BOARD},
    {"id": "finish-ajena", "method": "finish", "args": ["$swarm", "ana", 1, "ok"], "board": DONE_BOARD},
    {"id": "finish-reintento", "method": "finish", "args": ["$swarm", "bo", 1, "otra vez"],
     "board": DONE_BOARD},
    {"id": "finish-sistema", "method": "finish", "args": ["$swarm", "ez", 1, "cerrada por ez"],
     "board": DONE_BOARD},
    {"id": "reopen", "method": "reopen", "args": ["$swarm", 1], "kwargs": {"by": "cy", "note": "falta"},
     "board": DONE_BOARD},
    {"id": "reopen-sin-terminar", "method": "reopen", "args": ["$swarm", 1], "kwargs": {"by": "cy"},
     "board": BASE_BOARD},
    {"id": "reopen-agotado", "method": "reopen", "args": ["$swarm", 1],
     "kwargs": {"by": "cy", "note": "otra vez"},
     "board": {"goal": "X", "ops": [
         *TEAM, {"op": "task", "title": "Portar", "role": "dev"},
         {"op": "claim", "member": "bo", "task": 1},
         {"op": "finish", "member": "bo", "task": 1, "result": "ok"},
         {"op": "reopen", "task": 1, "note": "r1"},
         {"op": "claim", "member": "bo", "task": 1},
         {"op": "finish", "member": "bo", "task": 1, "result": "ok"},
         {"op": "reopen", "task": 1, "note": "r2"},
         {"op": "claim", "member": "bo", "task": 1},
         {"op": "finish", "member": "bo", "task": 1, "result": "ok"},
         {"op": "reopen", "task": 1, "note": "r3"}]}}
]


def _store_cases(out: Any, home: Path) -> None:
    for case in STORE_CASES:
        spec = case.get("board", BASE_BOARD)
        store, swarm = sb.board(home, f"store-{case['id']}", spec)
        args = [swarm if item == "$swarm" else item for item in case.get("args", [])]
        kwargs = {key: (swarm if value == "$swarm" else value)
                  for key, value in case.get("kwargs", {}).items()}
        payload = {"board": spec, "method": case["method"], "args": case.get("args", []),
                   "kwargs": case.get("kwargs", {})}
        try:
            result = getattr(store, case["method"])(*args, **kwargs)
        except Exception as error:  # noqa: BLE001
            out.add_error(f"swarm/store/{case['id']}", f"store.{case['method']}", payload,
                          sb.normalize_text(str(error), swarm, home))
            continue
        shown = "<swarm>" if case.get("flag") else _jsonable(result, swarm, home)
        out.add(f"swarm/store/{case['id']}", f"store.{case['method']}", payload,
                {"result": shown, "board": sb.dump_board(store.path, swarm, home)})


def _jsonable(value: Any, swarm: str, home: Path) -> Any:
    if isinstance(value, str):
        return sb.normalize_text(value, swarm, home)
    if value is None or isinstance(value, (bool, int, float)):
        return value
    if isinstance(value, dict):
        shown: dict[str, Any] = {}
        for key, item in value.items():
            if key in sb.TIME_COLUMNS and isinstance(item, float):
                shown[key] = round(item - sb.BASE, 3) if item else 0.0
            else:
                shown[key] = _jsonable(item, swarm, home)
        return shown
    if isinstance(value, (list, tuple, set)):
        return [_jsonable(item, swarm, home) for item in value]
    if is_dataclass(value) and not isinstance(value, type):
        return _jsonable(asdict(value), swarm, home)
    return str(value)


MAIL_CASES: list[dict[str, Any]] = [
    {"id": "post", "method": "post", "args": ["$swarm", "ana", "hola a todos"]},
    {"id": "post-vacio", "method": "post", "args": ["$swarm", "ana", ""]},
    {"id": "post-tipo", "method": "post", "args": ["$swarm", "ana", "x"], "kwargs": {"kind": "raro"}},
    {"id": "post-largo", "method": "post", "args": ["$swarm", "ana", "x" * 300]},
    {"id": "post-ajeno", "method": "post", "args": ["$swarm", "nadie", "x"]},
    {"id": "post-a-ajeno", "method": "post", "args": ["$swarm", "ana", "x"], "kwargs": {"to": "nadie"}},
    {"id": "post-a-invalido", "method": "post", "args": ["$swarm", "ana", "x"], "kwargs": {"to": "a b"}},
    {"id": "post-sistema", "method": "post", "args": ["$swarm", "ez", "aviso del sistema"]},
    {"id": "post-unicode", "method": "post", "args": ["$swarm", "ana", "empezamos con la «ñ» 🙂"]},
    {"id": "inbox", "method": "inbox", "args": ["$swarm", "bo"],
     "board": {"goal": "X", "ops": [*TEAM,
                                    {"op": "post", "sender": "ana", "body": "para bo", "to": "bo"},
                                    {"op": "post", "sender": "cy", "body": "a todos", "to": "*"}]}},
    {"id": "inbox-sin-marcar", "method": "inbox", "args": ["$swarm", "bo"], "kwargs": {"mark": False},
     "board": {"goal": "X", "ops": [*TEAM,
                                    {"op": "post", "sender": "ana", "body": "para bo", "to": "bo"}]}},
    {"id": "inbox-limite", "method": "inbox", "args": ["$swarm", "bo"], "kwargs": {"limit": 1},
     "board": {"goal": "X", "ops": [*TEAM] + [
         {"op": "post", "sender": "ana", "body": f"m{index}", "to": "bo"} for index in range(3)]}},
    {"id": "unread", "method": "unread", "args": ["$swarm", "bo"],
     "board": {"goal": "X", "ops": [*TEAM,
                                    {"op": "post", "sender": "ana", "body": "para bo", "to": "bo"}]}},
    {"id": "history", "method": "history", "args": ["$swarm"], "kwargs": {"limit": 2},
     "board": {"goal": "X", "ops": [*TEAM] + [
         {"op": "post", "sender": "ana", "body": f"m{index}", "to": "*"} for index in range(4)]}},
    {"id": "history-desde", "method": "history", "args": ["$swarm"], "kwargs": {"since": 2},
     "board": {"goal": "X", "ops": [*TEAM] + [
         {"op": "post", "sender": "ana", "body": f"m{index}", "to": "*"} for index in range(4)]}},
    {"id": "reserve", "method": "reserve", "args": ["$swarm", "bo", ["src/a.rs", "src/b.rs"]]},
    {"id": "reserve-vacio", "method": "reserve", "args": ["$swarm", "bo", []]},
    {"id": "reserve-conflicto", "method": "reserve", "args": ["$swarm", "bo", ["src/a.rs"]],
     "board": {"goal": "X", "ops": [*TEAM, {"op": "reserve", "member": "ana", "paths": ["src/a.rs"]}]}},
    {"id": "reserve-directorio", "method": "reserve", "args": ["$swarm", "bo", ["src/a.rs"]],
     "board": {"goal": "X", "ops": [*TEAM, {"op": "reserve", "member": "ana", "paths": ["src"]}]}},
    {"id": "reserve-ttl", "method": "reserve", "args": ["$swarm", "bo", ["src/a.rs"]],
     "kwargs": {"ttl": 5.0}},
    {"id": "release", "method": "release", "args": ["$swarm", "bo"],
     "board": {"goal": "X", "ops": [*TEAM, {"op": "reserve", "member": "bo", "paths": ["src/a.rs"]}]}},
    {"id": "release-ruta", "method": "release", "args": ["$swarm", "bo", ["src/a.rs"]],
     "board": {"goal": "X", "ops": [*TEAM, {"op": "reserve", "member": "bo", "paths": ["src/a.rs"]}]}},
    {"id": "release-nada", "method": "release", "args": ["$swarm", "bo", ["src/z.rs"]]},
    {"id": "leases", "method": "leases", "args": ["$swarm"],
     "board": {"goal": "X", "ops": [*TEAM, {"op": "reserve", "member": "bo", "paths": ["src/b.rs"]},
                                    {"op": "reserve", "member": "ana", "paths": ["src/a.rs"]}]}},
    {"id": "incident-nuevo", "method": "open_incident", "args": ["$swarm", "bo", "quota", "resets 7:10pm"]},
    {"id": "incident-mismo", "method": "open_incident", "args": ["$swarm", "bo", "quota", "otra vez"],
     "board": {"goal": "X", "ops": [*TEAM,
                                    {"op": "incident", "member": "bo", "kind": "quota",
                                     "detail": "uno"}]}},
    {"id": "incident-otro", "method": "open_incident", "args": ["$swarm", "bo", "context"],
     "board": {"goal": "X", "ops": [*TEAM,
                                    {"op": "incident", "member": "bo", "kind": "quota",
                                     "detail": "uno"}]}},
    {"id": "incident-ajeno", "method": "open_incident", "args": ["$swarm", "nadie", "quota"]},
    {"id": "incident-largo", "method": "open_incident", "args": ["$swarm", "bo", "error", "x" * 400]},
    {"id": "incident-get", "method": "incident", "args": [1],
     "board": {"goal": "X", "ops": [*TEAM, {"op": "incident", "member": "bo", "kind": "quota"}]}},
    {"id": "incident-get-ajeno", "method": "incident", "args": [99]},
    {"id": "incidents", "method": "incidents", "args": ["$swarm"],
     "board": {"goal": "X", "ops": [*TEAM, {"op": "incident", "member": "bo", "kind": "quota"},
                                    {"op": "incident", "member": "cy", "kind": "closed"}]}},
    {"id": "incidents-todos", "method": "incidents", "args": ["$swarm"], "kwargs": {"active": False},
     "board": {"goal": "X", "ops": [*TEAM, {"op": "incident", "member": "bo", "kind": "quota"},
                                    {"op": "incident_state", "id": 1, "state": "resolved"}]}},
    {"id": "member-incident", "method": "member_incident", "args": ["$swarm", "bo"],
     "board": {"goal": "X", "ops": [*TEAM, {"op": "incident", "member": "bo", "kind": "quota"}]}},
    {"id": "member-incident-nada", "method": "member_incident", "args": ["$swarm", "bo"]},
    {"id": "set-incident", "method": "set_incident", "args": [1, "waiting", "wait"],
     "board": {"goal": "X", "ops": [*TEAM, {"op": "incident", "member": "bo", "kind": "quota"}]}},
    {"id": "set-incident-raro", "method": "set_incident", "args": [1, "raro"],
     "board": {"goal": "X", "ops": [*TEAM, {"op": "incident", "member": "bo", "kind": "quota"}]}},
    {"id": "set-incident-ajeno", "method": "set_incident", "args": [99, "resolved"]},
]

STORE_CASES.extend(MAIL_CASES)


# ------------------------------------------------------------------ recovery
class _Herdr:
    """Herdr falso: pantalla fija y agentes conocidos, sin red ni ventanas."""

    def __init__(self, screen: str = "pantalla de la sesión anterior",
                 agents: tuple[str, ...] = ("claude", "opencode", "codex")) -> None:
        self.screen = screen
        self.agents = agents

    def read_agent(self, address: str, lines: int) -> str:
        return self.screen

    def send_task(self, address: str, line: str) -> dict[str, Any]:
        return {"ok": True}

    def resolve_agent(self, name: str) -> str | None:
        return name if name in self.agents else None


RECOVERY_CASES: list[dict[str, Any]] = [
    {"id": "wait-hora", "choice": "wait", "kind": "quota",
     "resets": {"day": 2, "hour": 9, "minute": 0}},
    {"id": "wait-sin-hora", "choice": "wait", "kind": "closed"},
    {"id": "pause", "choice": "pause", "kind": "quota"},
    {"id": "release", "choice": "release", "kind": "quota"},
    {"id": "release-sin-tareas", "choice": "release", "kind": "closed", "task": False},
    {"id": "reopen", "choice": "reopen", "kind": "closed", "address": "t1"},
    {"id": "replace-explicito", "choice": "replace", "kind": "quota", "agent": "codex"},
    {"id": "replace-sugerido", "choice": "replace", "kind": "quota"},
    {"id": "replace-modelo", "choice": "replace", "kind": "context", "agent": "opencode",
     "model": "kimi", "effort": "high"},
    {"id": "replace-desconocido", "choice": "replace", "kind": "quota", "agent": "nope"},
    {"id": "replace-sin-alterno", "choice": "replace", "kind": "quota", "solo": True},
    {"id": "opcion", "choice": "xyz", "kind": "quota"},
    {"id": "mayusculas", "choice": " WAIT ", "kind": "quota"},
    {"id": "incidente-ajeno", "choice": "wait", "kind": "quota", "incident": 99},
    {"id": "incidente-resuelto", "choice": "wait", "kind": "quota", "resolved": True},
    {"id": "wait-quota-sin-hora", "choice": "wait", "kind": "quota"},
    {"id": "release-dos-tareas", "choice": "release", "kind": "quota", "tasks": 2},
    {"id": "replace-sugerido-modelo", "choice": "replace", "kind": "quota", "model": "gpt-5"},
    {"id": "reopen-sin-direccion", "choice": "reopen", "kind": "closed"},
    {"id": "mayusculas-pause", "choice": " Pause ", "kind": "quota"},
    {"id": "pause-ya-pausado", "choice": "pause", "kind": "quota", "status": "paused"},
]


def _recovery_board(case: dict[str, Any]) -> dict[str, Any]:
    """Tablero del caso: miembros, una o dos tareas tomadas y el incidente."""
    ops = list(TEAM)
    extra = case.get("tasks", 1) > 1 and not case.get("solo")
    if not case.get("solo"):
        ops += [{"op": "task", "title": "Portar", "role": "dev"}]
    if extra:
        ops += [{"op": "task", "title": "Documentar", "role": "dev"}]
    if case.get("task", True):
        ops += [{"op": "claim", "member": "bo", "task": 1}]
        if extra:
            ops += [{"op": "claim", "member": "bo", "task": 2}]
    if case.get("address"):
        ops += [{"op": "address", "member": "bo", "address": case["address"]}]
    ops += [{"op": "incident", "member": "bo", "kind": case["kind"], "detail": "detalle",
             "resets": case.get("resets")}]
    if case.get("resolved"):
        ops += [{"op": "incident_state", "id": 1, "state": "resolved", "action": "wait"}]
    return {"goal": "Migrar", "status": case.get("status", "active"), "ops": ops}


def _recovery_cases(out: Any, home: Path) -> None:
    from ezagent.swarm.recovery import Recovery
    from ezagent.swarm.service import SwarmService

    for case in RECOVERY_CASES:
        board = _recovery_board(case)
        store, swarm = sb.board(home, f"recovery-{case['id']}", board)
        service = SwarmService(store, clock=lambda: sb.BASE)
        asked: list[str] = []
        announced: list[str] = []
        recovery = Recovery(
            service,
            _Herdr(),
            handoff_dir=home / "handoff" / case["id"],
            announce=announced.append,
            ask=asked.append,
            reset_launches=lambda _, _member: None,
            wall=lambda: sb.BASE,
            clock=lambda: sb.BASE,
        )
        payload = {"board": board, "incident": case.get("incident", 1), "choice": case["choice"],
                   "agent": case.get("agent", ""), "model": case.get("model", ""),
                   "effort": case.get("effort", "")}
        try:
            text = recovery.resolve(
                case.get("incident", 1),
                case["choice"],
                agent=case.get("agent", ""),
                model=case.get("model", ""),
                effort=case.get("effort", ""),
            )
        except Exception as error:  # noqa: BLE001
            out.add_error(f"swarm/recovery/{case['id']}", "recovery.resolve", payload,
                          sb.normalize_text(str(error), swarm, home))
            continue
        out.add(
            f"swarm/recovery/{case['id']}",
            "recovery.resolve",
            payload,
            {
                "text": sb.normalize_text(text, swarm, home),
                "asked": [sb.normalize_text(item, swarm, home) for item in asked],
                "announced": [sb.normalize_text(item, swarm, home) for item in announced],
                "board": sb.dump_board(store.path, swarm, home),
            },
        )


# ----------------------------------------------------------------------- cli
CLI_ENDPOINTS: list[dict[str, Any]] = [
    {"id": "directo", "env": {"EZS_ENDPOINT": "http://mi-host/endpoint.json"}},
    {"id": "asiento", "env": {"EZS_SEAT": "<tmp>/swarm/seats/ana/token"}},
    {"id": "appdata", "env": {}},
    {"id": "xdg", "env": {"APPDATA": None, "XDG_CONFIG_HOME": "<tmp>/config"}},
    {"id": "perfil", "env": {"APPDATA": None}},
]

CLI_PORTS: list[tuple[str, str | None]] = [
    ("valido", '{"url": "http://127.0.0.1:8765"}'),
    ("con-ruta", '{"url": "http://127.0.0.1:8765/v1"}'),
    ("https", '{"url": "https://127.0.0.1:8765"}'),
    ("puerto-texto", '{"url": "http://127.0.0.1:abc"}'),
    ("json-roto", "{no es json"),
    ("sin-url", '{"puerto": 1}'),
    ("url-vacia", '{"url": ""}'),
    ("lista", "[]"),
    ("sin-archivo", None),
]

CLI_MAIN: list[dict[str, Any]] = [
    {"id": "sin-token", "env": {"EZS_TOKEN": None, "EZS_SEAT": None}, "argv": ["board"]},
    {"id": "token-con-espacio", "env": {"EZS_TOKEN": "a b"}, "argv": ["board"]},
    {"id": "token-no-ascii", "env": {"EZS_TOKEN": "tökén"}, "argv": ["next"]},
    {"id": "sin-endpoint", "env": {"EZS_TOKEN": "abc"}, "endpoint": "no-existe.json",
     "argv": ["board"]},
    {"id": "endpoint-malo", "env": {"EZS_TOKEN": "abc"}, "content": '{"url": "https://x"}',
     "argv": ["next"]},
]


@contextlib.contextmanager
def _environment(values: dict[str, str | None], home: Path) -> Any:
    saved = {key: os.environ.get(key) for key in values}
    try:
        for key, value in values.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value.replace("<tmp>", str(home))
        yield
    finally:
        for key, value in saved.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value


def _cli_cases(out: Any, home: Path) -> None:
    from ezagent.swarm import cli

    for case in CLI_ENDPOINTS:
        env = {"EZS_ENDPOINT": None, "EZS_SEAT": None, **case["env"]}
        with _environment(env, home):
            path = cli.endpoint_path()
        out.add(
            f"swarm/cli/endpoint/{case['id']}",
            "cli.endpoint_path",
            {"env": case["env"]},
            {"path": sb.normalize_text(path, "", home)},
        )

    for case_id, content in CLI_PORTS:
        folder = home / "cli" / case_id
        folder.mkdir(parents=True, exist_ok=True)
        path = folder / "endpoint.json"
        if content is not None:
            path.write_text(content, encoding="utf-8")
        with _environment({"EZS_ENDPOINT": str(path)}, home):
            port = cli._read_port()
        out.add(f"swarm/cli/port/{case_id}", "cli._read_port", {"endpoint": content},
                {"port": port})

    for case in CLI_MAIN:
        folder = home / "cli" / f"main-{case['id']}"
        folder.mkdir(parents=True, exist_ok=True)
        endpoint = folder / case.get("endpoint", "endpoint.json")
        if case.get("content") is not None:
            endpoint.write_text(case["content"], encoding="utf-8")
        env = {"EZS_ENDPOINT": str(endpoint), "EZS_SEAT": None, **case["env"]}
        out_stream, err_stream = io.StringIO(), io.StringIO()
        with _environment(env, home):
            with contextlib.redirect_stdout(out_stream), contextlib.redirect_stderr(err_stream):
                code = cli.main(case["argv"])
        out.add(
            f"swarm/cli/main/{case['id']}",
            "cli.main",
            {"argv": case["argv"], "env": case["env"], "endpoint": case.get("content")},
            {
                "exit": code,
                "stdout": sb.normalize_text(out_stream.getvalue(), "", home),
                "stderr": sb.normalize_text(err_stream.getvalue(), "", home),
            },
        )


def generate(out: Any, home: Path) -> None:
    _models_cases(out)
    _store_cases(out, home)
    _recovery_cases(out, home)
    _cli_cases(out, home)



