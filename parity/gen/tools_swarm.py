"""Fixtures de la herramienta `swarm` (dominio `tools_swarm`, F1-14).

Cubre `src/ezagent/agent/tools_swarm.py` contra un `SwarmStack` real (tablero
SQLite, servicio y supervisor) con un Herdr falso: no se abre ninguna terminal.
Tras cada llamada se vuelca el tablero completo con `swarm_board.dump_board`,
así que los fixtures fijan también los miembros, tareas, mensajes, reservas e
incidentes que deja cada acción.

Los ids de enjambre se normalizan a `<swarm>`, los tokens a `<token>`, las
rutas del perfil a `<tmp>` y las columnas de tiempo a segundos respecto al
reloj fijo del caso.
"""

from __future__ import annotations

import os
from dataclasses import asdict
from pathlib import Path
from types import SimpleNamespace
from typing import Any

import swarm_board as sb
from _common import FixtureFile, main_guard

DOMAIN = "tools_swarm"

COMMAND_DIR = "C:/ezagent/bin"
TEAM = [
    {"agent": "claude", "role": "plan", "model": "opus", "effort": "high"},
    {"agent": "opencode", "role": "desarrollar", "model": "deepseek-v4.1-flash", "effort": "max"},
    {"agent": "codex", "role": "qa", "model": "gpt-6-sol", "effort": "high"},
]


class FakeHerdr:
    """Herdr de mentira: resuelve agentes, guarda contextos y recuerda pantallas."""

    def __init__(self, context: str = "C:/tmp/encargo.md") -> None:
        self.cfg = SimpleNamespace(
            agents={"claude": {}, "opencode": {}, "codex": {}}, default_cwd="C:/proj"
        )
        self.context = context
        self.saved: list[str] = []
        self.launched: list[tuple[str, str, dict[str, Any]]] = []

    def resolve_agent(self, name: str) -> str | None:
        key = name.replace(" ", "").lower()
        return key if key in self.cfg.agents else None

    def swarm_args(self, agent: str) -> list[str]:
        return []

    def run_agent(self, agent: str, task: str = "", **kwargs: Any) -> dict[str, Any]:
        self.launched.append((agent, task, kwargs))
        return {"ok": True, "address": f"t{len(self.launched)}", "task_sent": False}

    def list_agents(self, timeout: float = 10) -> list[dict[str, Any]]:
        return []

    def read_agent(self, address: str, lines: int | None = None) -> str:
        return ""

    def send_task(self, address: str, text: str) -> dict[str, Any]:
        return {"ok": True}

    def save_context(self, text: str) -> str:
        self.saved.append(text)
        return self.context


class Clock:
    def __init__(self) -> None:
        self.value = sb.BASE

    def __call__(self) -> float:
        current = self.value
        self.value += 1.0
        return current


def _rig(home: Path, label: str, *, command_dir: str, context: str) -> tuple[Any, Any, Clock, FakeHerdr]:
    from ezagent.swarm.stack import SwarmStack

    base = home / "swarm-tools-cases" / label
    herdr = FakeHerdr(context)
    said: list[str] = []
    asked: list[str] = []
    stack = SwarmStack(base, herdr, announce=said.append, ask=asked.append)
    clock = Clock()
    stack.command_dir = command_dir
    stack.supervisor.command_dir = command_dir
    stack.supervisor._clock = clock
    stack.supervisor.recovery._clock = clock
    stack.supervisor.recovery._wall = clock
    stack.store._clock = clock
    stack.service._clock = clock
    return stack, base, clock, herdr


def _seed(store: Any, seeds: list[dict[str, Any]]) -> list[str]:
    created = []
    for spec in seeds:
        swarm = store.create(
            spec.get("goal", "Objetivo de prueba"),
            cwd=spec.get("cwd", ""),
            status=spec.get("status", "active"),
        )
        for op in spec.get("ops", []):
            sb.apply(store, swarm, op)
        created.append(swarm)
    return created


def _norm(value: Any, swarm: str, home: Path) -> Any:
    if isinstance(value, str):
        return sb.normalize_text(value, swarm, home)
    if isinstance(value, dict):
        return {key: _norm(item, swarm, home) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_norm(item, swarm, home) for item in value]
    return value


def _run(
    out: FixtureFile,
    home: Path,
    label: str,
    args: dict[str, Any],
    *,
    seeds: list[dict[str, Any]] | None = None,
    command_dir: str = COMMAND_DIR,
    context: str = "C:/tmp/encargo.md",
) -> None:
    from ezagent.agent.tools_swarm import build_swarm_tools

    stack, base, _clock, _herdr = _rig(home, label, command_dir=command_dir, context=context)
    created = _seed(stack.store, seeds or [])
    tool = build_swarm_tools(stack)[0]
    result = tool.handler(args)
    info = asdict(tool.describe(args))
    swarm = ""
    if result.ok and isinstance(result.data, dict):
        swarm = str(result.data.get("swarm") or "")
    if not swarm:
        swarm = stack.store.latest() or (created[-1] if created else "")
    board = sb.dump_board(base / "board.db", swarm, home) if swarm else None
    out.add(
        f"tools_swarm/call/{label}",
        "swarm",
        {"args": args, "seeds": seeds or [], "command_dir": command_dir, "context": context},
        {
            "result": _norm(result.to_dict(), swarm, home),
            "info": _norm(info, swarm, home),
            "board": board,
            "saved_context": len(_herdr.saved),
        },
    )


START_CASES: list[tuple[str, dict[str, Any], list[dict[str, Any]], str, str]] = [
    ("start-basic", {"action": "start", "goal": "Migrar el núcleo a Rust", "members": TEAM}, [], COMMAND_DIR, "C:/tmp/encargo.md"),
    (
        "start-alias-roles",
        {
            "action": "start",
            "goal": "Preparar el equipo",
            "members": [
                {"agent": "claude", "role": "desarrollar"},
                {"agent": "opencode", "role": "QA"},
                {"agent": "codex", "role": "Diseñador"},
                {"agent": "claude", "role": "backend"},
                {"agent": "opencode", "role": "Dev Ops"},
                {"agent": "codex", "role": "investigador"},
                {"agent": "claude", "role": "planificar"},
            ],
        },
        [],
        COMMAND_DIR,
        "C:/tmp/encargo.md",
    ),
    (
        "start-names",
        {
            "action": "start",
            "goal": "Nombres",
            "members": [
                {"agent": "claude", "role": "dev", "name": "Ana Plan"},
                {"agent": "opencode", "role": "dev", "name": "ana plan"},
                {"agent": "codex", "role": "qa", "name": "ñandú 🙂"},
                {"agent": "claude", "role": "qa", "name": "***"},
                {"agent": "opencode", "role": "dev", "name": "x" * 40},
            ],
        },
        [],
        COMMAND_DIR,
        "C:/tmp/encargo.md",
    ),
    (
        "start-parallel",
        {
            "action": "start",
            "goal": "En paralelo",
            "members": [
                {"agent": "claude", "role": "plan", "parallel": True},
                {"agent": "opencode", "role": "dev", "parallel": True},
                {"agent": "codex", "role": "qa", "parallel": False},
            ],
        },
        [],
        COMMAND_DIR,
        "C:/tmp/encargo.md",
    ),
    ("start-goal-long", {"action": "start", "goal": "G" * 700, "members": TEAM}, [], COMMAND_DIR, "C:/tmp/encargo.md"),
    ("start-goal-long-path", {"action": "start", "goal": "G" * 700, "members": TEAM}, [], COMMAND_DIR, "C:/" + "x" * 300),
    ("start-cwd", {"action": "start", "goal": "Proyecto", "members": TEAM, "cwd": "C:/proyecto ñ"}, [], COMMAND_DIR, "C:/tmp/encargo.md"),
    ("start-no-members", {"action": "start", "goal": "X"}, [], COMMAND_DIR, "C:/tmp/encargo.md"),
    ("start-members-text", {"action": "start", "goal": "X", "members": "texto"}, [], COMMAND_DIR, "C:/tmp/encargo.md"),
    ("start-members-empty", {"action": "start", "goal": "X", "members": []}, [], COMMAND_DIR, "C:/tmp/encargo.md"),
    (
        "start-too-many",
        {"action": "start", "goal": "X", "members": [TEAM[0]] * 9},
        [],
        COMMAND_DIR,
        "C:/tmp/encargo.md",
    ),
    ("start-member-not-object", {"action": "start", "goal": "X", "members": [5]}, [], COMMAND_DIR, "C:/tmp/encargo.md"),
    (
        "start-unknown-agent",
        {"action": "start", "goal": "X", "members": [{"agent": "nope", "role": "dev"}]},
        [],
        COMMAND_DIR,
        "C:/tmp/encargo.md",
    ),
    (
        "start-missing-role",
        {"action": "start", "goal": "X", "members": [{"agent": "claude"}]},
        [],
        COMMAND_DIR,
        "C:/tmp/encargo.md",
    ),
    (
        "start-invalid-role",
        {"action": "start", "goal": "X", "members": [{"agent": "claude", "role": "-"}]},
        [],
        COMMAND_DIR,
        "C:/tmp/encargo.md",
    ),
    ("start-missing-goal", {"action": "start", "members": TEAM}, [], COMMAND_DIR, "C:/tmp/encargo.md"),
    ("start-no-command", {"action": "start", "goal": "X", "members": TEAM}, [], "", "C:/tmp/encargo.md"),
]

SEEDED: list[tuple[str, dict[str, Any], list[dict[str, Any]]]] = [
    (
        "status-basic",
        {"action": "status"},
        [
            {
                "ops": [
                    {"op": "join", "name": "ana", "role": "plan", "agent": "claude", "model": "opus"},
                    {"op": "join", "name": "bo", "role": "dev", "agent": "opencode"},
                    {"op": "join", "name": "cy", "role": "qa", "agent": "codex", "effort": "high"},
                    {"op": "task", "title": "Planificar", "role": "plan"},
                    {"op": "claim", "member": "ana", "task": 1},
                    {"op": "finish", "member": "ana", "task": 1, "result": "Listo"},
                    {"op": "task", "title": "Implementar", "role": "dev"},
                    {"op": "task", "title": "Validar", "role": "qa"},
                    {"op": "post", "sender": "ana", "body": "Empiezo por el plan", "to": "*"},
                    {"op": "post", "sender": "bo", "body": "Voy con el núcleo", "to": "ana", "kind": "info"},
                    {"op": "reserve", "member": "bo", "paths": ["src/nucleo.rs", "src/main.rs"]},
                ]
            }
        ],
    ),
    (
        "status-parts",
        {"action": "status"},
        [
            {
                "ops": [
                    {"op": "join", "name": "ana", "role": "plan", "agent": "claude"},
                    {"op": "task", "title": "Migrar", "role": "plan"},
                    {"op": "task", "title": "Núcleo", "parent": 1, "owner": "ana"},
                    {"op": "claim", "member": "ana", "task": 2},
                    {"op": "finish", "member": "ana", "task": 2, "result": "hecho"},
                    {"op": "task", "title": "Interfaz", "parent": 1},
                ]
            }
        ],
    ),
    (
        "status-attention",
        {"action": "status"},
        [
            {
                "ops": [
                    {"op": "join", "name": "ana", "role": "plan", "agent": "claude"},
                    {"op": "task", "title": "Fallar", "owner": "ana"},
                    {"op": "claim", "member": "ana", "task": 1},
                    {"op": "finish", "member": "ana", "task": 1, "result": "no pude", "ok": False},
                    {"op": "member_state", "member": "ana", "state": "blocked"},
                ]
            }
        ],
    ),
    (
        "status-stalled",
        {"action": "status"},
        [
            {
                "ops": [
                    {"op": "join", "name": "ana", "role": "plan", "agent": "claude"},
                    {"op": "task", "title": "Lenta", "owner": "ana"},
                    {"op": "claim", "member": "ana", "task": 1},
                    {"op": "member_state", "member": "ana", "state": "idle"},
                ]
            }
        ],
    ),
    (
        "status-paused",
        {"action": "status"},
        [{"status": "paused", "ops": [{"op": "join", "name": "ana", "role": "plan", "agent": "claude"}, {"op": "task", "title": "X"}]}],
    ),
    (
        "status-stopped",
        {"action": "status"},
        [{"status": "stopped", "ops": [{"op": "join", "name": "ana", "role": "plan", "agent": "claude"}, {"op": "task", "title": "X"}]}],
    ),
    (
        "status-latest",
        {"action": "status"},
        [
            {"ops": [{"op": "task", "title": "Viejo"}]},
            {"ops": [{"op": "task", "title": "Nuevo"}]},
        ],
    ),
    ("status-no-swarm", {"action": "status"}, []),
    (
        "tell-broadcast",
        {"action": "tell", "text": "  Revisad el plan  "},
        [{"ops": [{"op": "join", "name": "ana", "role": "plan", "agent": "claude"}]}],
    ),
    (
        "tell-member",
        {"action": "tell", "text": "Ve con el núcleo", "to": "ANA"},
        [{"ops": [{"op": "join", "name": "ana", "role": "plan", "agent": "claude"}]}],
    ),
    (
        "tell-long",
        {"action": "tell", "text": "x" * 400},
        [{"ops": [{"op": "join", "name": "ana", "role": "plan", "agent": "claude"}]}],
    ),
    (
        "tell-missing-text",
        {"action": "tell"},
        [{"ops": [{"op": "join", "name": "ana", "role": "plan", "agent": "claude"}]}],
    ),
    (
        "tell-unknown-member",
        {"action": "tell", "text": "hola", "to": "nadie"},
        [{"ops": [{"op": "join", "name": "ana", "role": "plan", "agent": "claude"}]}],
    ),
    ("tell-no-swarm", {"action": "tell", "text": "hola"}, []),
    (
        "pause",
        {"action": "pause"},
        [{"ops": [{"op": "join", "name": "ana", "role": "plan", "agent": "claude"}]}],
    ),
    (
        "resume",
        {"action": "resume"},
        [{"status": "paused", "ops": [{"op": "join", "name": "ana", "role": "plan", "agent": "claude"}]}],
    ),
    (
        "resume-not-paused",
        {"action": "resume"},
        [{"ops": [{"op": "join", "name": "ana", "role": "plan", "agent": "claude"}]}],
    ),
    (
        "stop",
        {"action": "stop"},
        [{"ops": [{"op": "join", "name": "ana", "role": "plan", "agent": "claude"}]}],
    ),
    ("stop-no-swarm", {"action": "stop"}, []),
    ("unknown-action", {"action": "volar"}, []),
    ("empty-action", {}, []),
    (
        "uppercase-action",
        {"action": "STATUS"},
        [{"ops": [{"op": "join", "name": "ana", "role": "plan", "agent": "claude"}]}],
    ),
    # ------------------------------------------------------------- resolve
    ("resolve-no-swarm", {"action": "resolve", "choice": "wait"}, []),
    (
        "resolve-no-incidents",
        {"action": "resolve", "choice": "wait"},
        [{"ops": [{"op": "join", "name": "ana", "role": "plan", "agent": "claude"}]}],
    ),
    (
        "resolve-two-incidents",
        {"action": "resolve", "choice": "wait"},
        [
            {
                "ops": [
                    {"op": "join", "name": "ana", "role": "plan", "agent": "claude"},
                    {"op": "join", "name": "bo", "role": "dev", "agent": "opencode"},
                    {"op": "incident", "member": "ana", "kind": "quota", "detail": "sin uso"},
                    {"op": "incident", "member": "bo", "kind": "error", "detail": "roto"},
                ]
            }
        ],
    ),
    (
        "resolve-wait",
        {"action": "resolve", "incident": "I1", "choice": "wait"},
        [
            {
                "ops": [
                    {"op": "join", "name": "ana", "role": "plan", "agent": "claude"},
                    {"op": "incident", "member": "ana", "kind": "quota", "detail": "sin uso"},
                    {"op": "member_state", "member": "ana", "state": "blocked"},
                ]
            }
        ],
    ),
    (
        "resolve-pause",
        {"action": "resolve", "incident": "i1", "choice": "pause"},
        [
            {
                "ops": [
                    {"op": "join", "name": "ana", "role": "plan", "agent": "claude"},
                    {"op": "incident", "member": "ana", "kind": "quota", "detail": "sin uso"},
                ]
            }
        ],
    ),
    (
        "resolve-release",
        {"action": "resolve", "incident": "1", "choice": "release"},
        [
            {
                "ops": [
                    {"op": "join", "name": "ana", "role": "plan", "agent": "claude"},
                    {"op": "task", "title": "Planificar", "role": "plan", "owner": "ana"},
                    {"op": "claim", "member": "ana", "task": 1},
                    {"op": "incident", "member": "ana", "kind": "closed", "detail": "cerrada"},
                ]
            }
        ],
    ),
    (
        "resolve-reopen",
        {"action": "resolve", "incident": "I1", "choice": "reopen"},
        [
            {
                "ops": [
                    {"op": "join", "name": "ana", "role": "plan", "agent": "claude"},
                    {"op": "task", "title": "Planificar", "owner": "ana"},
                    {"op": "claim", "member": "ana", "task": 1},
                    {"op": "incident", "member": "ana", "kind": "context", "detail": "lleno"},
                ]
            }
        ],
    ),
    (
        "resolve-replace",
        {"action": "resolve", "incident": "I1", "choice": "replace", "agent": "codex", "model": "gpt-6-sol", "effort": "high"},
        [
            {
                "ops": [
                    {"op": "join", "name": "ana", "role": "plan", "agent": "claude"},
                    {"op": "incident", "member": "ana", "kind": "auth", "detail": "sesión"},
                ]
            }
        ],
    ),
    (
        "resolve-replace-suggested",
        {"action": "resolve", "incident": "I1", "choice": "replace"},
        [
            {
                "ops": [
                    {"op": "join", "name": "ana", "role": "plan", "agent": "claude", "model": "opus"},
                    {"op": "join", "name": "bo", "role": "dev", "agent": "opencode", "model": "flash"},
                    {"op": "incident", "member": "ana", "kind": "auth", "detail": "sesión"},
                ]
            }
        ],
    ),
    (
        "resolve-replace-unknown",
        {"action": "resolve", "incident": "I1", "choice": "replace", "agent": "nope"},
        [
            {
                "ops": [
                    {"op": "join", "name": "ana", "role": "plan", "agent": "claude"},
                    {"op": "incident", "member": "ana", "kind": "auth", "detail": "sesión"},
                ]
            }
        ],
    ),
    (
        "resolve-invalid-choice",
        {"action": "resolve", "incident": "I1", "choice": "bailar"},
        [
            {
                "ops": [
                    {"op": "join", "name": "ana", "role": "plan", "agent": "claude"},
                    {"op": "incident", "member": "ana", "kind": "error", "detail": "roto"},
                ]
            }
        ],
    ),
    (
        "resolve-missing-incident",
        {"action": "resolve", "incident": "I99", "choice": "wait"},
        [
            {
                "ops": [
                    {"op": "join", "name": "ana", "role": "plan", "agent": "claude"},
                    {"op": "incident", "member": "ana", "kind": "error", "detail": "roto"},
                ]
            }
        ],
    ),
    (
        "resolve-non-digit",
        {"action": "resolve", "incident": "abc", "choice": "wait"},
        [
            {
                "ops": [
                    {"op": "join", "name": "ana", "role": "plan", "agent": "claude"},
                    {"op": "incident", "member": "ana", "kind": "error", "detail": "roto"},
                ]
            }
        ],
    ),
]


def _describe_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.agent.tools_swarm import build_swarm_tools

    stack, _base, _clock, _herdr = _rig(
        home, "describe", command_dir=COMMAND_DIR, context="C:/tmp/encargo.md"
    )
    tool = build_swarm_tools(stack)[0]
    cases = []
    for args in [
        {"action": "start"},
        {"action": "status"},
        {"action": "tell"},
        {"action": "resolve"},
        {"action": "pause"},
        {"action": "resume"},
        {"action": "stop"},
        {"action": "volar"},
        {"action": "STATUS"},
        {},
    ]:
        cases.append({"args": args, "info": asdict(tool.describe(args))})
    out.add(
        "tools_swarm/describe/0001",
        "swarm.describe",
        {"args": "varios"},
        {"cases": cases},
    )


def _specs(out: FixtureFile, home: Path) -> None:
    from ezagent.agent.tools_swarm import build_swarm_tools

    stack, _base, _clock, _herdr = _rig(
        home, "specs", command_dir=COMMAND_DIR, context="C:/tmp/encargo.md"
    )
    tools = []
    for tool in build_swarm_tools(stack):
        tools.append(
            {
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.parameters,
                "level": tool.level,
                "untrusted_output": tool.untrusted_output,
                "timeout_s": tool.timeout_s,
                "schema": tool.schema(),
                "info": asdict(tool.describe({})),
            }
        )
    out.add(
        "tools_swarm/build_swarm_tools/specs",
        "build_swarm_tools",
        {"stack": "falso"},
        {"tools": tools, "without_stack": build_swarm_tools(None)},
    )


def generate(out: FixtureFile) -> None:
    from loguru import logger

    logger.remove()
    home = Path(os.environ["HOME"])
    _specs(out, home)
    _describe_cases(out, home)
    for label, args, seeds, command_dir, context in START_CASES:
        _run(
            out,
            home,
            label,
            args,
            seeds=seeds,
            command_dir=command_dir,
            context=context,
        )
    for label, args, seeds in SEEDED:
        _run(out, home, label, args, seeds=seeds)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
