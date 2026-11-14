"""Avisos de avance y clasificación de incidentes (dominio `swarm`, F1-06).

`Progress` se alimenta con secuencias del mismo enjambre; los instantes de
`incidents` se registran como delta respecto a `now` para no depender de la zona
horaria de la máquina.
"""

from __future__ import annotations

from typing import Any

import swarm_board as sb

# ----------------------------------------------------------------- progress
PROGRESS_CASES: list[dict[str, Any]] = [
    {"id": "primera", "steps": [[{"id": 1, "title": "Portar", "role": "dev"}]]},
    {"id": "hecha-sin-siguiente", "steps": [
        [{"id": 1, "title": "Portar", "role": "dev"}, {"id": 2, "title": "Probar", "role": "qa"}],
        [{"id": 1, "title": "Portar", "role": "dev", "state": "done"},
         {"id": 2, "title": "Probar", "role": "qa"}]]},
    {"id": "hecha-con-siguiente", "steps": [
        [{"id": 1, "title": "Portar", "role": "dev"},
         {"id": 2, "title": "Probar", "role": "qa", "deps": [1]}],
        [{"id": 1, "title": "Portar", "role": "dev", "state": "done"},
         {"id": 2, "title": "Probar", "role": "qa", "deps": [1]}]]},
    {"id": "dos-mismo-rol", "steps": [
        [{"id": 1, "title": "Lexer", "role": "dev"}, {"id": 2, "title": "Parser", "role": "dev"},
         {"id": 3, "title": "Probar", "role": "qa"}],
        [{"id": 1, "title": "Lexer", "role": "dev", "state": "done"},
         {"id": 2, "title": "Parser", "role": "dev", "state": "done"},
         {"id": 3, "title": "Probar", "role": "qa"}]]},
    {"id": "dos-roles", "steps": [
        [{"id": 1, "title": "Lexer", "role": "dev"}, {"id": 2, "title": "Guía", "role": "docs"},
         {"id": 3, "title": "Probar", "role": "qa"}],
        [{"id": 1, "title": "Lexer", "role": "dev", "state": "done"},
         {"id": 2, "title": "Guía", "role": "docs", "state": "done"},
         {"id": 3, "title": "Probar", "role": "qa"}]]},
    {"id": "empiezan-dos", "steps": [
        [{"id": 1, "title": "Lexer", "role": "dev"}, {"id": 2, "title": "Guía", "role": "docs"},
         {"id": 3, "title": "Probar", "role": "qa", "deps": [1]},
         {"id": 4, "title": "Publicar", "role": "ops", "deps": [2]}],
        [{"id": 1, "title": "Lexer", "role": "dev", "state": "done"},
         {"id": 2, "title": "Guía", "role": "docs", "state": "done"},
         {"id": 3, "title": "Probar", "role": "qa", "deps": [1]},
         {"id": 4, "title": "Publicar", "role": "ops", "deps": [2]}]]},
    {"id": "sigue-en-curso", "steps": [
        [{"id": 1, "title": "Portar", "role": "dev"}, {"id": 2, "title": "Probar", "role": "qa"}],
        [{"id": 1, "title": "Portar", "role": "dev", "state": "done"},
         {"id": 2, "title": "Probar", "role": "qa", "state": "claimed"}]]},
    {"id": "siguen-en-curso", "steps": [
        [{"id": 1, "title": "Portar", "role": "dev"}, {"id": 2, "title": "Probar", "role": "qa"},
         {"id": 3, "title": "Documentar", "role": "docs"}],
        [{"id": 1, "title": "Portar", "role": "dev", "state": "done"},
         {"id": 2, "title": "Probar", "role": "qa", "state": "claimed"},
         {"id": 3, "title": "Documentar", "role": "docs", "state": "claimed"}]]},
    {"id": "devuelta", "steps": [
        [{"id": 1, "title": "Portar", "role": "dev"}, {"id": 2, "title": "Probar", "role": "qa"}],
        [{"id": 1, "title": "Portar", "role": "dev", "state": "pending", "rounds": 1},
         {"id": 2, "title": "Probar", "role": "qa"}]]},
    {"id": "devuelta-ronda-3", "steps": [
        [{"id": 1, "title": "Portar", "role": "dev"}, {"id": 2, "title": "Probar", "role": "qa"}],
        [{"id": 1, "title": "Portar", "role": "dev", "rounds": 3},
         {"id": 2, "title": "Probar", "role": "qa"}]]},
    {"id": "todo-hecho", "steps": [
        [{"id": 1, "title": "Portar", "role": "dev"}, {"id": 2, "title": "Probar", "role": "qa"}],
        [{"id": 1, "title": "Portar", "role": "dev", "state": "done"},
         {"id": 2, "title": "Probar", "role": "qa", "state": "done"}]]},
    {"id": "partes", "steps": [
        [{"id": 1, "title": "Portar", "role": "dev"}, {"id": 2, "title": "Lexer", "role": "dev", "parent": 1},
         {"id": 3, "title": "Probar", "role": "qa"}],
        [{"id": 1, "title": "Portar", "role": "dev"}, {"id": 2, "title": "Lexer", "role": "dev", "parent": 1,
                                                      "state": "done"},
         {"id": 3, "title": "Probar", "role": "qa"}]]},
    {"id": "repetida", "steps": [
        [{"id": 1, "title": "Portar", "role": "dev"}, {"id": 2, "title": "Probar", "role": "qa"}],
        [{"id": 1, "title": "Portar", "role": "dev", "state": "done"},
         {"id": 2, "title": "Probar", "role": "qa"}],
        [{"id": 1, "title": "Portar", "role": "dev", "state": "done"},
         {"id": 2, "title": "Probar", "role": "qa"}]]},
    {"id": "vuelve-a-pendiente", "steps": [
        [{"id": 1, "title": "Portar", "role": "dev", "state": "done"},
         {"id": 2, "title": "Probar", "role": "qa"}],
        [{"id": 1, "title": "Portar", "role": "dev", "state": "pending"},
         {"id": 2, "title": "Probar", "role": "qa"}]]},
    {"id": "nueva-tarea", "steps": [
        [{"id": 1, "title": "Portar", "role": "dev"}],
        [{"id": 1, "title": "Portar", "role": "dev"}, {"id": 2, "title": "Extra", "role": "dev"}]]},
    {"id": "nueva-hecha", "steps": [
        [{"id": 1, "title": "Portar", "role": "dev"}, {"id": 2, "title": "Probar", "role": "qa"}],
        [{"id": 1, "title": "Portar", "role": "dev", "state": "done"},
         {"id": 2, "title": "Probar", "role": "qa"},
         {"id": 3, "title": "Extra", "role": "docs", "state": "done"}]]},
    {"id": "unicode", "steps": [
        [{"id": 1, "title": "Diseñar la «ñ» 🙂", "role": ""}, {"id": 2, "title": "Probar", "role": "qa"}],
        [{"id": 1, "title": "Diseñar la «ñ» 🙂", "role": "", "state": "done"},
         {"id": 2, "title": "Probar", "role": "qa"}]]},
    {"id": "cadena", "steps": [
        [{"id": 1, "title": "Portar", "role": "dev"}, {"id": 2, "title": "Probar", "role": "qa", "deps": [1]},
         {"id": 3, "title": "Publicar", "role": "ops", "deps": [2]}],
        [{"id": 1, "title": "Portar", "role": "dev", "state": "done"},
         {"id": 2, "title": "Probar", "role": "qa", "deps": [1]},
         {"id": 3, "title": "Publicar", "role": "ops", "deps": [2]}],
        [{"id": 1, "title": "Portar", "role": "dev", "state": "done"},
         {"id": 2, "title": "Probar", "role": "qa", "deps": [1], "state": "done"},
         {"id": 3, "title": "Publicar", "role": "ops", "deps": [2]}]]},
    {"id": "fallo-sin-aviso", "steps": [
        [{"id": 1, "title": "Portar", "role": "dev"}, {"id": 2, "title": "Probar", "role": "qa"}],
        [{"id": 1, "title": "Portar", "role": "dev", "state": "failed"},
         {"id": 2, "title": "Probar", "role": "qa"}]]},
    {"id": "hecha-y-devuelta", "steps": [
        [{"id": 1, "title": "Portar", "role": "dev"}, {"id": 2, "title": "Probar", "role": "qa"},
         {"id": 3, "title": "Publicar", "role": "ops"}],
        [{"id": 1, "title": "Portar", "role": "dev", "state": "done"},
         {"id": 2, "title": "Probar", "role": "qa", "rounds": 1},
         {"id": 3, "title": "Publicar", "role": "ops", "deps": [1]}]]},
]


def _progress_cases(out: Any) -> None:
    from ezagent.swarm.progress import Progress

    for case in PROGRESS_CASES:
        progress = Progress()
        news: list[list[str]] = []
        for step in case["steps"]:
            tasks = [
                sb.task(
                    raw["id"],
                    raw["title"],
                    role=raw.get("role", ""),
                    owner=raw.get("owner", ""),
                    state=raw.get("state", "pending"),
                    deps=tuple(raw.get("deps", ())),
                    parent=raw.get("parent", 0),
                    rounds=raw.get("rounds", 0),
                )
                for raw in step
            ]
            news.append(progress.check("abc12345", tasks))
        input_data = {"steps": case["steps"]}
        if case["id"] == "repetida":
            progress.forget("abc12345")
            news.append(progress.check("abc12345", tasks))
            input_data = {"steps": case["steps"], "then_forget": True}
        out.add(f"swarm/progress/{case['id']}", "progress.check", input_data, {"news": news})


# ---------------------------------------------------------------- incidents
SCREENS: list[tuple[str, str]] = [
    ("quota-claude", "You've hit your 5-hour limit · resets 7:10pm"),
    ("quota-429", "Too many requests (429). Try again in 2h 30m."),
    ("quota-es", "Se alcanzó el límite de uso del plan"),
    ("quota-creditos", "Sin créditos disponibles"),
    ("quota-rate", "Rate limited by the provider"),
    ("context-prompt", "Prompt is too long"),
    ("context-maximo", "maximum context length exceeded: 200000"),
    ("auth-login", "Not logged in · Please run /login"),
    ("auth-401", "401 Unauthorized"),
    ("compacting", "Compacting conversation…"),
    ("ventana-cerrada", "La ventana se cerró"),
    ("normal", "todo bien, sigue trabajando"),
    ("prioridad-auth", "quota exceeded\n401 Unauthorized"),
    ("cola-fuera", "usage limit" + "".join(f"\nlinea {index}" for index in range(25))),
    ("detalle-largo", "hit your usage limit " + "x" * 250),
    ("contexto-es", "la ventana de contexto está llena"),
    ("espacios", "  hit   your   usage   limit  "),
    ("sesion", "sesión expirada, vuelve a entrar"),
    ("quota-disponible", "hit your usage limit, available 3:00pm"),
    ("context-lleno", "context window is full"),
    ("compacting-largo", "Compacting the conversation before the next turn"),
]

RESET_TEXTS: list[str] = [
    "resets 7:10pm",
    "resets 7:10 pm",
    "resets 19:10",
    "resets 7:10 a.m.",
    "resets 12:30am",
    "try again in 2h 30m",
    "try again in 45m",
    "in 3 hours",
    "sin hora conocida",
    "resets 20:00",
    "vuelve a las 21:05",
    "resets 7pm",
] + [
    "resets 12:00pm",
    "resets 12:00am",
    "try again at 11:59pm",
    "resets 6:05 p.m.",
    "available 20:15",
    "disponible a las 22:00",
    "vuelve en 90m",
    "in 0m",
]

WHEN_CASES: list[dict[str, Any]] = [
    {"id": "sin-hora", "resets": None},
    {"id": "misma-tarde", "resets": {"hour": 21, "minute": 5}},
    {"id": "misma-noche", "resets": {"hour": 23, "minute": 10}},
    {"id": "manana", "resets": {"day": 2, "hour": 9, "minute": 0}},
    {"id": "medianoche", "resets": {"day": 2, "hour": 0, "minute": 30}},
    {"id": "mediodia", "resets": {"day": 2, "hour": 12, "minute": 0}},
    {"id": "manana-tarde", "resets": {"day": 2, "hour": 13, "minute": 7}},
    {"id": "sin-minutos", "resets": {"day": 2, "hour": 19, "minute": 0}},
] + [
    {"id": f"h{hora:02d}-d{dia}", "resets": {"day": dia, "hour": hora, "minute": 0}}
    for dia in (1, 2)
    for hora in (0, 6, 11, 12, 13, 19, 23)
]

DESCRIBE_CASES: list[dict[str, Any]] = [
    {"id": "quota-hoy", "kind": "quota", "resets": {"hour": 23, "minute": 10}, "tasks": ["T1"]},
    {"id": "quota-manana", "kind": "quota", "resets": {"day": 2, "hour": 9, "minute": 0}},
    {"id": "quota-sin-hora", "kind": "quota"},
    {"id": "contexto", "kind": "context", "resets": {"hour": 23, "minute": 10}, "tasks": ["T3", "T4"]},
    {"id": "cerrada", "kind": "closed", "tasks": ["T2"]},
    {"id": "auth", "kind": "auth", "member": {"agent": "codex", "role": "qa"}},
    {"id": "salida", "kind": "exited"},
    {"id": "error", "kind": "error", "member": {"agent": "opencode", "role": "otro rol"}},
    {"id": "raro", "kind": "raro", "tasks": ["T9"]},
    {"id": "quota-tareas", "kind": "quota", "resets": {"day": 2, "hour": 9, "minute": 0},
     "tasks": ["T1", "T2"]},
    {"id": "contexto-sin-hora", "kind": "context", "tasks": ["T3"]},
    {"id": "auth-tareas", "kind": "auth", "tasks": ["T4"], "member": {"agent": "pi", "role": "qa"}},
    {"id": "cerrada-sin-tareas", "kind": "closed", "member": {"agent": "qwen", "role": ""}},
    {"id": "salida-tareas", "kind": "exited", "tasks": ["T5", "T6"]},
    {"id": "error-sin-tareas", "kind": "error"},
    {"id": "raro-sin-tareas", "kind": "raro"},
    {"id": "quota-madrugada", "kind": "quota", "resets": {"hour": 0, "minute": 5}},
    {"id": "quota-pasado", "kind": "quota", "resets": {"day": 3, "hour": 12, "minute": 0}},
    {"id": "cerrada-tres", "kind": "closed", "tasks": ["T1", "T2", "T3"]},
    {"id": "auth-dos-tareas", "kind": "auth", "tasks": ["T7", "T8"]},
]

QUESTION_CASES: list[dict[str, Any]] = [
    {"id": "quota-hora", "kind": "quota", "resets": {"day": 2, "hour": 9, "minute": 0}, "tasks": ["T1"]},
    {"id": "quota-sin-hora", "kind": "quota"},
    {"id": "cerrada", "kind": "closed", "tasks": ["T2"]},
    {"id": "auth", "kind": "auth"},
    {"id": "contexto", "kind": "context"},
    {"id": "con-alterno", "kind": "quota", "alt": {"agent": "codex", "model": "gpt-5"}},
    {"id": "alterno-sin-modelo", "kind": "closed", "alt": {"agent": "codex"}},
    {"id": "error", "kind": "error", "alt": {"agent": "opencode", "model": "kimi"}},
    {"id": "quota-hora-alterno", "kind": "quota", "resets": {"day": 2, "hour": 9, "minute": 0},
     "alt": {"agent": "codex", "model": "gpt-5"}},
    {"id": "quota-sin-hora-alterno", "kind": "quota", "alt": {"agent": "codex"}},
    {"id": "cerrada-tareas", "kind": "closed", "tasks": ["T1"], "alt": {"agent": "pi"}},
    {"id": "auth-tareas", "kind": "auth", "tasks": ["T2"]},
    {"id": "contexto-alterno", "kind": "context", "alt": {"agent": "qwen", "model": "max"}},
    {"id": "error-tareas", "kind": "error", "tasks": ["T3"], "alt": {"agent": "codex"}},
    {"id": "salida", "kind": "exited"},
    {"id": "raro-alterno", "kind": "raro", "alt": {"agent": "pi", "model": "p"}},
    {"id": "quota-casi-medianoche", "kind": "quota", "resets": {"hour": 23, "minute": 59}},
    {"id": "auth-modelo", "kind": "auth", "alt": {"agent": "opencode", "model": "kimi"}},
    {"id": "cerrada-sin-alterno", "kind": "closed", "tasks": ["T4", "T5"]},
    {"id": "raro-sin-nada", "kind": "raro"},
]

SUGGEST_CASES: list[dict[str, Any]] = [
    {"id": "otro-agente", "failing": "ana", "members": ["ana:claude", "bo:opencode", "cy:codex"]},
    {"id": "primero-gana", "failing": "bo", "members": ["ana:claude", "bo:opencode", "cy:codex"]},
    {"id": "ocupado", "failing": "ana", "members": ["ana:claude", "bo:opencode", "cy:codex"], "busy": ["bo"]},
    {"id": "todos-ocupados", "failing": "ana", "members": ["ana:claude", "bo:opencode"],
     "busy": ["bo"]},
    {"id": "ido", "failing": "ana", "members": ["ana:claude", "bo:opencode", "cy:codex"], "gone": ["bo"]},
    {"id": "mismo-agente", "failing": "ana", "members": ["ana:claude", "bo:claude", "cy:codex"]},
    {"id": "vacio", "failing": "ana", "members": []},
    {"id": "sin-agente", "failing": "ana", "members": ["ana:claude", "bo:"]},
]


def _incident(kind: str, *, resets: dict[str, Any] | None = None, detail: str = "") -> Any:
    from ezagent.swarm.models import Incident

    created = sb.at(day=1, hour=20, minute=0)
    return Incident(
        7, "abc12345", "bo", kind, detail, sb.at(**resets) if resets else 0.0, "open", "", created, created
    )


def _member_spec(raw: dict[str, Any] | None) -> Any:
    raw = raw or {}
    return sb.member("bo", raw.get("role", "dev"), raw.get("agent", "opencode"), model=raw.get("model", ""))


def _classify_cases(out: Any) -> None:
    from datetime import datetime

    from ezagent.swarm import incidents

    now = datetime(2025, 1, 1, 20, 0)
    for index, (name, screen) in enumerate(SCREENS, start=1):
        signal = incidents.classify(screen, now)
        payload: Any = None
        if signal is not None:
            after = round(signal.resets - now.timestamp(), 3) if signal.resets else 0.0
            payload = {"kind": signal.kind, "detail": signal.detail, "resets_after": after}
        out.add(
            f"swarm/incidents/classify/{index:02d}-{name}",
            "incidents.classify",
            {"screen": screen, "now": "2025-01-01T20:00"},
            {"signal": payload},
        )


def _reset_cases(out: Any) -> None:
    from datetime import datetime

    from ezagent.swarm import incidents

    now = datetime(2025, 1, 1, 20, 0)
    for index, text in enumerate(RESET_TEXTS, start=1):
        resets = incidents.parse_reset(text, now)
        out.add(
            f"swarm/incidents/parse-reset/{index:02d}",
            "incidents.parse_reset",
            {"text": text, "now": "2025-01-01T20:00"},
            {"after": round(resets - now.timestamp(), 3) if resets else 0.0},
        )


def _when_cases(out: Any) -> None:
    from ezagent.swarm import incidents

    for case in WHEN_CASES:
        resets = 0.0
        if case["resets"]:
            resets = sb.at(hour=case["resets"]["hour"], minute=case["resets"]["minute"],
                           **({"day": case["resets"]["day"]} if "day" in case["resets"] else {}))
        ref = sb.at(day=1, hour=20, minute=0)
        out.add(
            f"swarm/incidents/when/{case['id']}",
            "incidents.when",
            {"resets": case["resets"], "ref": {"day": 1, "hour": 20, "minute": 0}},
            {"text": incidents.when(resets, ref)},
        )


def _describe_cases(out: Any) -> None:
    from ezagent.swarm import incidents

    for case in DESCRIBE_CASES:
        incident = _incident(case["kind"], resets=case.get("resets"), detail="detalle")
        member = _member_spec(case.get("member"))
        tasks = case.get("tasks", [])
        payload = {
            "incident": {
                "kind": case["kind"],
                "resets": case.get("resets"),
                "detail": "detalle",
                "created": {"day": 1, "hour": 20, "minute": 0},
            },
            "member": {"agent": member.agent, "role": member.role, "model": member.model},
            "tasks": tasks,
        }
        out.add(
            f"swarm/incidents/describe/{case['id']}",
            "incidents.describe",
            payload,
            {"text": incidents.describe(incident, member, tasks)},
        )
        out.add(
            f"swarm/incidents/board-note/{case['id']}",
            "incidents.board_note",
            payload,
            {"text": incidents.board_note(incident, member, tasks)},
        )


def _question_cases(out: Any) -> None:
    from ezagent.swarm import incidents

    for case in QUESTION_CASES:
        incident = _incident(case["kind"], resets=case.get("resets"), detail="detalle")
        member = _member_spec(case.get("member"))
        alt = _member_spec(case["alt"]) if case.get("alt") else None
        out.add(
            f"swarm/incidents/question/{case['id']}",
            "incidents.question",
            {
                "incident": {"kind": case["kind"], "resets": case.get("resets")},
                "member": {"agent": member.agent, "role": member.role},
                "tasks": case.get("tasks", []),
                "alt": {"agent": alt.agent, "model": alt.model} if alt else None,
            },
            {"text": incidents.question(incident, member, case.get("tasks", []), alt)},
        )


def _suggest_cases(out: Any) -> None:
    from ezagent.swarm import incidents

    for case in SUGGEST_CASES:
        members = [
            sb.member(item.split(":")[0], "dev", item.split(":")[1] or "",
                      "gone" if item.split(":")[0] in case.get("gone", []) else "idle")
            for item in case["members"]
        ]
        failing = next((m for m in members if m.name == case["failing"]), sb.member(case["failing"]))
        found = incidents.suggest(members, failing, set(case.get("busy", [])))
        out.add(
            f"swarm/incidents/suggest/{case['id']}",
            "incidents.suggest",
            {"failing": case["failing"], "members": case["members"],
             "busy": sorted(case.get("busy", [])), "gone": case.get("gone", [])},
            {"member": None if found is None else {"name": found.name, "agent": found.agent}},
        )


def cases(out: Any) -> None:
    _progress_cases(out)
    _classify_cases(out)
    _reset_cases(out)
    _when_cases(out)
    _describe_cases(out)
    _question_cases(out)
    _suggest_cases(out)
