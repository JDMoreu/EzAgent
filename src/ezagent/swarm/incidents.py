"""Incidentes del enjambre: qué le pasó a un agente y qué se le propone al usuario.

Un agente puede dejar de responder por causas que no son un fallo de su tarea:

- **quota**: se acabó el uso de la suscripción o del proveedor (p. ej. «You've
  hit your 5-hour limit · resets 7:10pm» en Claude Code).
- **context**: se le llenó el contexto y ya no acepta más.
- **auth**: perdió la sesión o la clave del proveedor.
- **closed**: el usuario cerró su ventana.
- **exited**: el programa del agente salió solo (dos veces seguidas).
- **error**: Herdr lo marca con error y no se reconoce la causa.

Compactar el contexto **no** es un incidente: el agente sigue trabajando y, si
pierde el hilo, ``ezs me`` le repite su papel.

La pantalla del agente es un dato no confiable (AGENTS.md §5): aquí solo se
busca un patrón en sus últimas líneas para elegir una etiqueta. Lo peor que
puede provocar un texto malicioso es un aviso al usuario; nunca una acción
destructiva. Todos los textos que oye el usuario los arma este módulo.
"""

from __future__ import annotations

import re
from collections.abc import Sequence
from dataclasses import dataclass
from datetime import datetime, timedelta

from .models import Incident, Member
from .protocol import role_label

TAIL_LINES = 18
ASK_KINDS = frozenset({"quota", "auth", "closed", "exited", "error"})
KIND_LABELS = {
    "quota": "se quedó sin uso",
    "context": "se quedó sin contexto",
    "auth": "perdió la sesión con su proveedor",
    "closed": "tiene su ventana cerrada",
    "exited": "se cerró solo dos veces",
    "error": "tuvo un error",
}
_PATTERNS: tuple[tuple[str, re.Pattern[str]], ...] = (
    (
        "auth",
        re.compile(
            r"not logged in|please (?:run )?/?log ?in|invalid api key|authentication (?:failed|error)"
            r"|(?:oauth )?token (?:has )?expired|401 unauthorized|sesi[oó]n (?:caducada|expirada)",
            re.IGNORECASE,
        ),
    ),
    (
        "quota",
        re.compile(
            r"hit your (?:[\w-]+ )?(?:usage )?limit|usage limit|rate[ -]limit(?:ed)?"
            r"|quota (?:exceeded|exhausted)|insufficient (?:balance|credits?|quota)"
            r"|credit balance is too low|out of credits|too many requests|\b429\b"
            r"|l[ií]mite de uso|sin cr[eé]ditos",
            re.IGNORECASE,
        ),
    ),
    (
        "context",
        re.compile(
            r"prompt is too long|context (?:window|length|limit) (?:exceeded|reached|is full)"
            r"|exceeds? the (?:model'?s )?context window|maximum context length"
            r"|ran out of room in the model'?s context|context window (?:exceeded|is full)",
            re.IGNORECASE,
        ),
    ),
    (
        "compacting",
        re.compile(
            r"compacting (?:the )?(?:conversation|context)|auto-?compact(?:ing)?\b"
            r"|summari[sz]ing (?:the )?conversation",
            re.IGNORECASE,
        ),
    ),
)
_CLOCK_12 = re.compile(
    r"(?:resets?|try again|available|disponible|vuelve)\D{0,12}?(\d{1,2})(?::(\d{2}))?\s*([ap])\.?\s*m\b",
    re.IGNORECASE,
)
_CLOCK_24 = re.compile(
    r"(?:resets?|try again|available|disponible|vuelve)\D{0,12}?\b([01]?\d|2[0-3]):([0-5]\d)\b",
    re.IGNORECASE,
)
_IN = re.compile(
    r"(?:resets?|try again|in|en)\s+(?:in\s+)?(?:(\d+)\s*h(?:ours?|oras?)?)?\s*"
    r"(?:(\d+)\s*m(?:in(?:utes?|utos?)?)?\b)?",
    re.IGNORECASE,
)


@dataclass(frozen=True)
class Signal:
    """Lo que se reconoció en la pantalla de un agente."""

    kind: str
    detail: str
    resets: float = 0.0  # hora de restablecimiento (época); 0 si no se sabe


def _tail(screen: str) -> list[str]:
    lines = [" ".join(line.split()) for line in str(screen or "").splitlines()]
    return [line for line in lines if line][-TAIL_LINES:]


def parse_reset(text: str, now: datetime) -> float:
    """Hora de restablecimiento mencionada en ``text`` (época), o 0."""
    match = _CLOCK_12.search(text)
    hour = minute = -1
    if match:
        hour = int(match.group(1)) % 12 + (12 if match.group(3).lower() == "p" else 0)
        minute = int(match.group(2) or 0)
    else:
        match = _CLOCK_24.search(text)
        if match:
            hour, minute = int(match.group(1)), int(match.group(2))
    if hour >= 0:
        moment = now.replace(hour=hour, minute=minute, second=0, microsecond=0)
        if moment <= now:
            moment += timedelta(days=1)
        return moment.timestamp()
    for found in _IN.finditer(text):
        hours, minutes = found.group(1), found.group(2)
        if hours or minutes:
            span = timedelta(hours=int(hours or 0), minutes=int(minutes or 0))
            return (now + span).timestamp()
    return 0.0


def classify(screen: str, now: datetime) -> Signal | None:
    """Etiqueta lo que muestran las últimas líneas de la pantalla, o None."""
    lines = _tail(screen)
    for kind, pattern in _PATTERNS:
        for line in reversed(lines):
            if pattern.search(line):
                resets = parse_reset(" ".join(lines), now) if kind == "quota" else 0.0
                return Signal(kind, line[:200], resets)
    return None


# ------------------------------------------------------------------ textos
def when(resets: float, ref: float = 0.0) -> str:
    """«las 7:10 p. m.» o «mañana a las 7:10 p. m.» (respecto a ``ref``); vacío si no se sabe."""
    if resets <= 0:
        return ""
    moment = datetime.fromtimestamp(resets)
    base = datetime.fromtimestamp(ref) if ref > 0 else datetime.now()
    hour = moment.hour % 12 or 12
    suffix = "a. m." if moment.hour < 12 else "p. m."
    clock = f"las {hour}:{moment.minute:02d} {suffix}"
    return f"mañana a {clock}" if moment.date() > base.date() else clock


def suggest(members: Sequence[Member], failing: Member, busy: set[str]) -> Member | None:
    """Otro miembro (con otro agente) cuyo agente y modelo pueden sustituir al caído."""
    for member in members:
        if member.name == failing.name or member.name in busy or member.state == "gone":
            continue
        if member.agent and member.agent != failing.agent:
            return member
    return None


def describe(incident: Incident, member: Member, tasks: Sequence[str]) -> str:
    """Qué pasó, en una frase: «plan (claude, planificar) se quedó sin uso hasta…»."""
    label = KIND_LABELS.get(incident.kind, incident.kind)
    until = when(incident.resets, incident.created)
    text = f"{member.name} ({member.agent}, {role_label(member.role)}) {label}"
    if until and incident.kind == "quota":
        text += f" hasta {until}"
    if tasks:
        text += f". Su trabajo pendiente: {', '.join(tasks)}"
    return text + "."


def question(incident: Incident, member: Member, tasks: Sequence[str], alt: Member | None) -> str:
    """Pregunta al usuario con las opciones; la arma el código, nunca el LLM."""
    head = f"Enjambre: {describe(incident, member, tasks)}"
    swap = (
        f"lo reemplazo con {alt.agent}{' ' + alt.model if alt.model else ''}"
        if alt
        else ("lo reemplazo con otro agente")
    )
    if incident.kind == "quota":
        wait = "espero a que se restablezca" if incident.resets else "espero a que vuelva"
        return f"{head} ¿{wait.capitalize()}, {swap} o pauso el enjambre?"
    if incident.kind == "closed":
        return f"{head} ¿Lo reabro, {swap} o pauso el enjambre?"
    if incident.kind == "auth":
        return f"{head} Inicia sesión en su ventana y lo retomo, o dime si {swap}."
    return f"{head} ¿Lo reabro, {swap} o pauso el enjambre?"


def board_note(incident: Incident, member: Member, tasks: Sequence[str]) -> str:
    """Aviso corto para el tablero: los demás miembros lo ven en su próximo ``next``."""
    until = when(incident.resets, incident.created)
    wait = f" hasta {until}" if until and incident.kind == "quota" else ""
    held = f"; {', '.join(tasks)} en espera" if tasks else ""
    return f"{member.name} ({member.agent}) {KIND_LABELS.get(incident.kind, incident.kind)}{wait}{held}."


__all__ = [
    "ASK_KINDS",
    "KIND_LABELS",
    "Signal",
    "board_note",
    "classify",
    "describe",
    "parse_reset",
    "question",
    "suggest",
    "when",
]
