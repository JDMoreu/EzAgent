"""Herramienta ``swarm``: varios agentes que colaboran con un tablero común.

Es la vía cara de delegar, así que solo se ofrece al modelo cuando el usuario
habla de trabajo en equipo o ya hay un enjambre en marcha (ver
``select_tool_names``). Para una tarea suelta o pasos en fila sigue valiendo
``delegate_to_agent``.

El modelo solo aporta valores (objetivo, agentes, roles, modelo, esfuerzo); las
instrucciones de cada agente, los avisos y los resúmenes los genera el código.
"""

from __future__ import annotations

import re
import unicodedata
from typing import Any

from ezagent.agent.registry import ToolInfo, ToolResult, ToolSpec
from ezagent.swarm.models import (
    MAX_GOAL,
    MAX_REF,
    MAX_TITLE,
    SwarmError,
    clip,
    member_name,
    role_name,
)
from ezagent.swarm.service import MemberSpec
from ezagent.swarm.view import board

MAX_MEMBERS = 8
_ACTIONS = ("start", "status", "tell", "resolve", "pause", "resume", "stop")
_LEVELS = {
    "start": 1,
    "status": 0,
    "tell": 1,
    "resolve": 1,
    "pause": 1,
    "resume": 1,
    "stop": 1,
}
# Sin tildes ni mayúsculas (ver ``_role``). Un rol que no esté aquí se usa tal
# cual, como nombre corto: el equipo es libre.
_ROLE_ALIASES = {
    "plan": ("planificar", "planificador", "planner", "planear", "planeacion", "planificacion"),
    "arch": ("arquitecto", "arquitectura", "architect", "arquitecta"),
    "lead": ("orquestador", "orquestar", "coordinador", "coordinar", "lider", "orchestrator"),
    "dev": ("desarrollar", "desarrollador", "developer", "implementar", "programador"),
    "back": ("backend", "back-end", "servidor", "api"),
    "front": ("frontend", "front-end", "interfaz web", "cliente"),
    "ux": ("ui", "disenador", "diseno", "designer", "ux/ui", "ui/ux"),
    "data": ("datos", "data engineer", "base de datos"),
    "ops": ("devops", "despliegue", "desplegar", "deploy", "infraestructura"),
    "docs": ("documentar", "documentador", "documentacion", "writer"),
    "qa": ("validar", "validador", "auditar", "auditor", "revisar", "reviewer", "revisor"),
    "test": ("tester", "pruebas", "probar", "testing"),
    "sec": ("seguridad", "security", "pentester"),
}
_ALIAS = {alias: role for role, names in _ROLE_ALIASES.items() for alias in names}

_PARAMETERS: dict[str, Any] = {
    "type": "object",
    "properties": {
        "action": {
            "type": "string",
            "enum": list(_ACTIONS),
            "description": (
                "start: crea un enjambre; status: cómo va; tell: recado del usuario "
                "para el equipo; stop: lo detiene."
            ),
        },
        "goal": {
            "type": "string",
            "description": "start: el encargo completo, con las palabras del usuario.",
        },
        "members": {
            "type": "array",
            "maxItems": MAX_MEMBERS,
            "description": (
                "start: los agentes, en el orden en que trabajan; cada etapa espera a "
                "la anterior salvo los marcados parallel."
            ),
            "items": {
                "type": "object",
                "properties": {
                    "agent": {"type": "string", "description": "Agente del catálogo."},
                    "role": {
                        "type": "string",
                        "description": (
                            "plan, arch (arquitecto), lead (orquestador), dev, back "
                            "(backend), front (frontend), ux, data, ops, docs, qa, test, "
                            "sec u otro nombre corto."
                        ),
                    },
                    "task": {
                        "type": "string",
                        "description": "Opcional: qué hace este miembro, si no basta su rol.",
                    },
                    "parallel": {
                        "type": "boolean",
                        "description": "true: trabaja a la vez que el miembro anterior.",
                    },
                    "model": {"type": "string"},
                    "effort": {"type": "string"},
                    "name": {"type": "string", "description": "Opcional; por defecto, el rol."},
                },
                "required": ["agent", "role"],
            },
        },
        "cwd": {"type": "string", "description": "start: carpeta del proyecto."},
        "text": {"type": "string", "description": "tell: el recado, en una línea."},
        "to": {"type": "string", "description": "tell: nombre del miembro; sin él, a todos."},
        "swarm": {"type": "string", "description": "Id del enjambre; sin él, el más reciente."},
        "incident": {"type": "string", "description": "resolve: id del incidente (I3)."},
        "choice": {
            "type": "string",
            "enum": ["wait", "replace", "reopen", "release", "pause"],
            "description": (
                "resolve: wait espera a que vuelva; replace lo cambia por otro agente "
                "(agent/model/effort; sin agent, el sugerido); reopen lo reabre; release "
                "pasa su trabajo a otro de su rol; pause pausa el enjambre."
            ),
        },
        "agent": {"type": "string", "description": "resolve con replace: agente nuevo."},
        "model": {"type": "string", "description": "resolve con replace: modelo."},
        "effort": {"type": "string", "description": "resolve con replace: esfuerzo."},
    },
    "required": ["action"],
}


def _text(args: dict[str, Any], key: str) -> str:
    value = args.get(key)
    return value.strip() if isinstance(value, str) else ""


def _members(stack: Any, raw: Any) -> list[MemberSpec]:
    if not isinstance(raw, list) or not raw:
        raise SwarmError("Indica los agentes del enjambre en «members».")
    if len(raw) > MAX_MEMBERS:
        raise SwarmError(f"Un enjambre admite como máximo {MAX_MEMBERS} agentes.")
    specs: list[MemberSpec] = []
    used: set[str] = set()
    for index, item in enumerate(raw, start=1):
        if not isinstance(item, dict):
            raise SwarmError(f"El miembro {index} debe ser un objeto.")
        asked = _text(item, "agent")
        agent = stack.herdr.resolve_agent(asked) if asked else None
        if agent is None:
            known = ", ".join(sorted(stack.herdr.cfg.agents))
            raise SwarmError(f"Agente desconocido: {asked!r}. Disponibles: {known}.")
        role = _role(_text(item, "role"))
        if not role:
            raise SwarmError(f"El miembro {index} necesita «role» (p. ej. plan, dev o qa).")
        base = member_name(_slug(_text(item, "name")) or role)
        name, suffix = base, 2
        while name in used:
            name, suffix = f"{base}{suffix}", suffix + 1
        used.add(name)
        task, _ = clip(_text(item, "task"), MAX_TITLE)
        specs.append(
            MemberSpec(
                name,
                role,
                agent,
                _text(item, "model"),
                _text(item, "effort"),
                task=task,
                parallel=item.get("parallel") is True and bool(specs),
            )
        )
    return specs


def _slug(text: str) -> str:
    """Nombre corto válido para el tablero: sin tildes, a-z, 0-9, «-» y «_»."""
    plain = unicodedata.normalize("NFKD", text.lower())
    plain = "".join(c for c in plain if not unicodedata.combining(c))
    return re.sub(r"[^a-z0-9_-]+", "-", plain).strip("-_")[:32]


def _role(raw: str) -> str:
    plain = unicodedata.normalize("NFKD", raw.strip().lower())
    plain = "".join(c for c in plain if not unicodedata.combining(c))
    return role_name(_ALIAS.get(plain) or _slug(plain))


def _goal(stack: Any, goal: str) -> str:
    """Un encargo largo va a un archivo; el tablero guarda el inicio y la ruta."""
    if len(goal) <= MAX_GOAL:
        return goal
    saver = getattr(stack.herdr, "save_context", None)
    path = saver(goal) if callable(saver) else None
    if not path or len(str(path)) > MAX_REF:
        return goal  # el tablero lo recorta
    note = f" [Encargo completo, léelo antes de empezar: {path}]"
    head, _ = clip(goal, MAX_GOAL - len(note))
    return head + note


def _start(stack: Any, args: dict[str, Any]) -> ToolResult:
    goal = _text(args, "goal")
    if not goal:
        raise SwarmError("Falta «goal»: el encargo para el enjambre.")
    specs = _members(stack, args.get("members"))
    if not stack.command_dir:
        raise SwarmError("No encuentro el comando ezs; reinstala EzAgent con «uv sync».")
    cwd = _text(args, "cwd") or str(getattr(stack.herdr.cfg, "default_cwd", "") or "")
    swarm, _ = stack.service.start(_goal(stack, goal), specs, cwd=cwd)
    stack.supervisor.poke()
    team = ", ".join(
        f"{spec.name} ({spec.agent}{' ' + spec.model if spec.model else ''})" for spec in specs
    )
    return ToolResult(
        ok=True,
        data={
            "swarm": swarm,
            "members": [spec.name for spec in specs],
            "message": (
                f"Enjambre {swarm} en marcha: {team}. Abro a cada agente cuando le toca; "
                "se coordinan solos y aviso al terminar o si algo necesita atención."
            ),
        },
    )


def _target(stack: Any, args: dict[str, Any]) -> str:
    swarm = _text(args, "swarm") or stack.service.latest() or stack.store.latest()
    if not swarm or stack.store.get(swarm) is None:
        raise SwarmError("No hay ningún enjambre.")
    return swarm


def _resolve(stack: Any, swarm: str, args: dict[str, Any]) -> ToolResult:
    """Aplica la decisión del usuario sobre un incidente."""
    raw = _text(args, "incident").upper().lstrip("I")
    if not raw.isdigit():
        found = stack.store.incidents(swarm)
        if len(found) != 1:
            raise SwarmError("Indica el incidente (por ejemplo I3).")
        raw = str(found[0].id)
    message = stack.recovery.resolve(
        int(raw),
        _text(args, "choice"),
        agent=_text(args, "agent"),
        model=_text(args, "model"),
        effort=_text(args, "effort"),
    )
    stack.supervisor.poke()
    return ToolResult(ok=True, data={"swarm": swarm, "message": message})


def _handle(stack: Any, args: dict[str, Any]) -> ToolResult:
    action = _text(args, "action").lower()
    try:
        if action == "start":
            return _start(stack, args)
        if action not in _ACTIONS:
            raise SwarmError(f"Acción desconocida: {action!r}. Usa: {', '.join(_ACTIONS)}.")
        swarm = _target(stack, args)
        if action == "status":
            return ToolResult(
                ok=True,
                data={
                    "swarm": swarm,
                    "spoken": stack.service.summary(swarm),
                    "board": board(stack.store, swarm),
                },
            )
        if action == "resolve":
            return _resolve(stack, swarm, args)
        if action == "pause":
            stack.service.pause(swarm)
            return ToolResult(ok=True, data={"swarm": swarm, "message": "Enjambre en pausa."})
        if action == "resume":
            stack.service.resume(swarm)
            stack.supervisor.poke()
            return ToolResult(ok=True, data={"swarm": swarm, "message": "Enjambre retomado."})
        if action == "tell":
            text = _text(args, "text")
            if not text:
                raise SwarmError("Falta «text»: el recado para el equipo.")
            to = _text(args, "to").lower()
            if to:
                stack.service.tell(swarm, text, to=to)
            else:
                stack.service.tell(swarm, text)
            stack.supervisor.poke()
            return ToolResult(ok=True, data={"swarm": swarm, "message": "Recado entregado."})
        stack.service.stop(swarm)
        return ToolResult(
            ok=True,
            data={
                "swarm": swarm,
                "message": "Enjambre detenido. Las terminales de los agentes siguen abiertas.",
            },
        )
    except SwarmError as exc:
        return ToolResult(ok=False, error=str(exc))
    except Exception as exc:  # noqa: BLE001 - se devuelve un error legible
        return ToolResult(ok=False, error=f"Error del enjambre: {type(exc).__name__}: {exc}")


def _info(args: dict[str, Any]) -> ToolInfo:
    action = _text(args, "action").lower()
    summaries = {
        "start": "Crear un enjambre de agentes que colaboran en una tarea.",
        "status": "Consultar cómo va el enjambre.",
        "tell": "Pasar un recado al enjambre.",
        "resolve": "Decidir qué hacer con un incidente del enjambre.",
        "pause": "Pausar el enjambre.",
        "resume": "Retomar el enjambre.",
        "stop": "Detener el enjambre.",
    }
    return ToolInfo(
        level=_LEVELS.get(action, 2),
        always_confirm=action not in _LEVELS,
        summary=summaries.get(action, "Acción desconocida del enjambre."),
    )


def build_swarm_tools(stack: Any) -> list[ToolSpec]:
    """Herramienta del enjambre; lista vacía si la app no montó el modo."""
    if stack is None:
        return []
    return [
        ToolSpec(
            name="swarm",
            description=(
                "Enjambre: varios agentes con roles (planificador, arquitecto, "
                "orquestador, backend, frontend, QA…) que colaboran solos con un tablero "
                "común, se hablan entre sí y se devuelven correcciones. EzAgent avisa "
                "cada etapa. start lo crea con goal y members en orden de trabajo; "
                "status dice cómo va; tell pasa un recado del usuario; resolve aplica la "
                "decisión del usuario sobre un incidente (I3); pause/resume; stop lo detiene. "
                "Úsalo solo si el usuario pide trabajo en equipo o validación con ida y "
                "vuelta; para una tarea suelta usa delegate_to_agent."
            ),
            parameters=_PARAMETERS,
            level=2,
            handler=lambda args: _handle(stack, args),
            # El tablero trae texto escrito por los agentes: dato no confiable.
            untrusted_output=True,
            timeout_s=20.0,
            describe_fn=_info,
        )
    ]


__all__ = ["build_swarm_tools"]
