"""Tablero y salud del enjambre (dominio `swarm`, F1-06).

Cada caso describe el tablero con operaciones (`swarm_board`) y guarda el texto
tal cual lo arma `view`, con el id del enjambre normalizado.
"""

from __future__ import annotations

from typing import Any

import swarm_board as sb
from swarm_board import TEAM

# --------------------------------------------------------------------- view
VIEW_BOARDS: list[dict[str, Any]] = [
    {"id": "vacio", "board": {"goal": "Nada aún"}},
    {"id": "miembros", "board": {"goal": "Sin tareas", "ops": list(TEAM)}},
    {
        "id": "pendiente",
        "board": {"goal": "Migrar", "ops": [*TEAM, {"op": "task", "title": "Portar el núcleo", "role": "dev"}]},
    },
    {
        "id": "reclamada",
        "board": {
            "goal": "Migrar",
            "ops": [*TEAM, {"op": "task", "title": "Portar", "role": "dev"},
                    {"op": "claim", "member": "bo", "task": 1}],
        },
    },
    {
        "id": "hecha",
        "board": {
            "goal": "Migrar",
            "ops": [*TEAM, {"op": "task", "title": "Portar", "role": "dev"},
                    {"op": "claim", "member": "bo", "task": 1},
                    {"op": "finish", "member": "bo", "task": 1, "result": "listo en una línea"}],
        },
    },
    {
        "id": "fallida",
        "board": {
            "goal": "Migrar",
            "ops": [*TEAM, {"op": "task", "title": "Portar", "role": "dev"},
                    {"op": "claim", "member": "bo", "task": 1},
                    {"op": "finish", "member": "bo", "task": 1, "result": "falta el token", "ok": False}],
        },
    },
    {
        "id": "partes",
        "board": {
            "goal": "Migrar",
            "ops": [
                *TEAM,
                {"op": "task", "title": "Portar el núcleo", "role": "dev"},
                {"op": "task", "title": "Lexer", "role": "dev", "parent": 1},
                {"op": "task", "title": "Parser", "role": "dev", "parent": 1},
                {"op": "claim", "member": "bo", "task": 2},
                {"op": "finish", "member": "bo", "task": 2, "result": "hecho"},
            ],
        },
    },
    {
        "id": "reservas",
        "board": {
            "goal": "Migrar",
            "ops": [*TEAM,
                    {"op": "reserve", "member": "bo", "paths": ["src/a.rs", "src/b.rs"]},
                    {"op": "reserve", "member": "ana", "paths": ["docs/plan.md"]}],
        },
    },
    {
        "id": "mensajes",
        "board": {
            "goal": "Migrar",
            "ops": [*TEAM] + [
                {"op": "post", "sender": "ana", "body": f"aviso {index}", "to": "*"} for index in range(1, 7)
            ],
        },
    },
    {"id": "mensajes-1", "board": {"goal": "Migrar", "ops": list(TEAM)}, "recent": 1},
    {"id": "mensajes-0", "board": {"goal": "Migrar", "ops": list(TEAM)}, "recent": 0},
    {
        "id": "sin-leer",
        "board": {
            "goal": "Migrar",
            "ops": [*TEAM,
                    {"op": "post", "sender": "ana", "body": "para bo", "to": "bo"},
                    {"op": "post", "sender": "cy", "body": "a todos", "to": "*"}],
        },
        "member": "bo",
    },
    {
        "id": "unicode",
        "board": {
            "goal": "Revisar la «ñ» y el emoji 🙂",
            "ops": [*TEAM,
                    {"op": "task", "title": "Portar función año ← día", "role": "dev"},
                    {"op": "post", "sender": "ana", "body": "empezamos con el módulo «voz» 🙂", "to": "*"}],
        },
    },
    {"id": "en-pausa", "board": {"goal": "Migrar", "status": "paused", "ops": list(TEAM)}},
    {
        "id": "todo-hecho",
        "board": {
            "goal": "Migrar",
            "ops": [*TEAM, {"op": "task", "title": "Portar", "role": "dev"},
                    {"op": "claim", "member": "bo", "task": 1},
                    {"op": "finish", "member": "bo", "task": 1, "result": "todo listo"}],
        },
    },
    {"id": "sin-miembros", "board": {"goal": "Sin equipo",
                                     "ops": [{"op": "task", "title": "Portar", "role": "dev"}]}},
    {
        "id": "cinco-miembros",
        "board": {
            "goal": "Migrar",
            "ops": [*TEAM,
                    {"op": "join", "name": "di", "role": "docs", "agent": "pi"},
                    {"op": "join", "name": "el", "role": "ops", "agent": "qwen"}],
        },
    },
    {
        "id": "varias-tareas",
        "board": {
            "goal": "Migrar",
            "ops": [*TEAM,
                    {"op": "task", "title": "Lexer", "role": "dev"},
                    {"op": "task", "title": "Parser", "role": "dev", "deps": [1]},
                    {"op": "task", "title": "Probar", "role": "qa", "deps": [1, 2]},
                    {"op": "task", "title": "Guía", "role": "docs"},
                    {"op": "task", "title": "Publicar", "role": "ops"}],
        },
    },
    {
        "id": "mensajes-2",
        "board": {
            "goal": "Migrar",
            "ops": [*TEAM] + [
                {"op": "post", "sender": "ana", "body": f"aviso {index}", "to": "*"} for index in (1, 2)
            ],
        },
        "recent": 2,
    },
]


def _view_cases(out: Any, home: Any) -> None:
    from ezagent.swarm.view import board as board_view

    for case in VIEW_BOARDS:
        store, swarm = sb.board(home, f"view-{case['id']}", case["board"])
        text = board_view(store, swarm, member=case.get("member", ""), recent=case.get("recent", 4))
        out.add(
            f"swarm/view/board/{case['id']}",
            "view.board",
            {
                "board": case["board"],
                "member": case.get("member", ""),
                "recent": case.get("recent", 4),
            },
            {"text": sb.normalize_text(text, swarm, home)},
        )

    store, swarm = sb.board(home, "view-ajeno", {"goal": "X"})
    try:
        board_view(store, "no-existe")
    except Exception as error:  # noqa: BLE001 - se registra el texto real
        out.add_error(
            "swarm/view/board/ajeno",
            "view.board",
            {"board": {"goal": "X"}, "member": "", "recent": 4},
            sb.normalize_text(str(error), swarm, home),
        )
    else:  # pragma: no cover - el oráculo debe registrar el fallo
        raise AssertionError("board aceptó un enjambre inexistente")


HEALTH_BOARDS: list[dict[str, Any]] = [
    {"id": "vacio", "board": {"goal": "Nada aún"}},
    {
        "id": "todo-hecho",
        "board": {"goal": "Migrar", "ops": [*TEAM, {"op": "task", "title": "Portar", "role": "dev"},
                                            {"op": "claim", "member": "bo", "task": 1},
                                            {"op": "finish", "member": "bo", "task": 1, "result": "ok"}]},
    },
    {"id": "en-orden", "board": {"goal": "Migrar", "ops": [*TEAM,
                                                          {"op": "task", "title": "Portar", "role": "dev"}]}},
    {"id": "fallida", "board": {"goal": "Migrar", "ops": [*TEAM,
                                                         {"op": "task", "title": "Portar", "role": "dev"},
                                                         {"op": "claim", "member": "bo", "task": 1},
                                                         {"op": "finish", "member": "bo", "task": 1,
                                                          "result": "sin tiempo", "ok": False}]}},
    {"id": "estancada", "board": {"goal": "Migrar", "ops": [*TEAM,
                                                           {"op": "task", "title": "Portar", "role": "dev"},
                                                           {"op": "claim", "member": "bo", "task": 1}]},
     "now": 1000.0},
    {"id": "rota", "board": {"goal": "Migrar", "ops": [*TEAM,
                                                      {"op": "task", "title": "Lexer", "role": "dev"},
                                                      {"op": "claim", "member": "bo", "task": 1},
                                                      {"op": "finish", "member": "bo", "task": 1,
                                                       "result": "roto", "ok": False},
                                                      {"op": "task", "title": "Parser", "role": "dev",
                                                       "deps": [1]}]}},
    {"id": "rol-huerfano", "board": {"goal": "Migrar", "ops": [*TEAM,
                                                              {"op": "task", "title": "Desplegar", "role": "ops"}]}},
    {"id": "bloqueado", "board": {"goal": "Migrar", "ops": [*TEAM,
                                                            {"op": "task", "title": "Portar", "role": "dev"},
                                                            {"op": "claim", "member": "bo", "task": 1},
                                                            {"op": "member_state", "member": "bo",
                                                             "state": "blocked"}]}},
    {"id": "bloqueado-ocioso", "board": {"goal": "Migrar", "ops": [*TEAM,
                                                                   {"op": "member_state", "member": "bo",
                                                                    "state": "blocked"}]}},
    {"id": "sin-tomar", "board": {"goal": "Migrar", "ops": [*TEAM,
                                                            {"op": "task", "title": "Portar", "role": "dev"}]},
     "now": 500.0},
    {"id": "pregunta", "board": {"goal": "Migrar", "ops": [*TEAM,
                                                           {"op": "post", "sender": "ana", "body": "¿qué API?",
                                                            "to": "bo", "kind": "ask"}]}, "now": 700.0},
    {"id": "respondida", "board": {"goal": "Migrar", "ops": [*TEAM,
                                                             {"op": "post", "sender": "ana", "body": "¿qué API?",
                                                              "to": "bo", "kind": "ask"},
                                                             {"op": "post", "sender": "bo", "body": "esta",
                                                              "to": "ana", "kind": "answer"}]}, "now": 700.0},
    {"id": "pregunta-general", "board": {"goal": "Migrar", "ops": [*TEAM,
                                                                   {"op": "post", "sender": "ana",
                                                                    "body": "¿alguien puede?", "to": "*",
                                                                    "kind": "ask"}]}, "now": 700.0},
    {"id": "incidente", "board": {"goal": "Migrar", "ops": [*TEAM,
                                                            {"op": "task", "title": "Portar", "role": "dev"},
                                                            {"op": "claim", "member": "bo", "task": 1},
                                                            {"op": "incident", "member": "bo", "kind": "quota",
                                                             "detail": "resets 7:10pm",
                                                             "resets": {"day": 1, "hour": 21, "minute": 0}}]},
     "now": 700.0},
    {"id": "incidente-pausa", "board": {"goal": "Migrar", "status": "paused",
                                        "ops": [*TEAM,
                                                {"op": "incident", "member": "cy", "kind": "closed",
                                                 "detail": "su ventana ya no está"}]}, "now": 700.0},
    {"id": "partes", "board": {"goal": "Migrar", "ops": [*TEAM,
                                                         {"op": "task", "title": "Portar", "role": "dev"},
                                                         {"op": "task", "title": "Lexer", "role": "dev",
                                                          "parent": 1},
                                                         {"op": "task", "title": "Parser", "role": "dev",
                                                          "parent": 1}]}},
    {"id": "en-curso", "board": {"goal": "Migrar", "ops": [*TEAM,
                                                           {"op": "task", "title": "Portar", "role": "dev"},
                                                           {"op": "claim", "member": "bo", "task": 1}]}},
    {"id": "siguiente-lista", "board": {"goal": "Migrar", "ops": [*TEAM,
                                                                  {"op": "task", "title": "Lexer", "role": "dev"},
                                                                  {"op": "claim", "member": "bo", "task": 1},
                                                                  {"op": "task", "title": "Parser", "role": "dev",
                                                                   "deps": [1]}]}},
    {"id": "incidente-tapa", "board": {"goal": "Migrar", "ops": [*TEAM,
                                                                 {"op": "task", "title": "Portar", "role": "dev"},
                                                                 {"op": "claim", "member": "bo", "task": 1},
                                                                 {"op": "finish", "member": "bo", "task": 1,
                                                                  "result": "roto", "ok": False},
                                                                 {"op": "incident", "member": "bo",
                                                                  "kind": "closed"}]}, "now": 700.0},
    {"id": "pausado", "board": {"goal": "Migrar", "status": "paused",
                                "ops": [*TEAM, {"op": "task", "title": "Portar", "role": "dev"}]},
     "now": 500.0},
]


def _health_cases(out: Any, home: Any) -> None:
    from ezagent.swarm.view import health, summary

    for case in HEALTH_BOARDS:
        store, swarm = sb.board(home, f"health-{case['id']}", case["board"])
        found = health(store, swarm, now=sb.BASE + case.get("now", 0.0))
        out.add(
            f"swarm/view/health/{case['id']}",
            "view.health",
            {"board": case["board"], "now": case.get("now", 0.0)},
            {
                "state": found.state,
                "done": found.done,
                "total": found.total,
                "working": list(found.working),
                "issues": list(found.issues),
                "keys": [issue.key for issue in found.found],
                "spoken": found.spoken(),
            },
        )
        out.add(
            f"swarm/view/summary/{case['id']}",
            "view.summary",
            {"board": case["board"], "now": case.get("now", 0.0)},
            {"text": summary(store, swarm, now=sb.BASE + case.get("now", 0.0))},
        )

    store, swarm = sb.board(home, "health-ajeno", {"goal": "X"})
    try:
        health(store, "no-existe", now=sb.BASE)
    except Exception as error:  # noqa: BLE001
        out.add_error(
            "swarm/view/health/ajeno",
            "view.health",
            {"board": {"goal": "X"}, "now": 0.0},
            sb.normalize_text(str(error), swarm, home),
        )
    else:  # pragma: no cover
        raise AssertionError("health aceptó un enjambre inexistente")




def cases(out: Any, home: Any) -> None:
    _view_cases(out, home)
    _health_cases(out, home)
