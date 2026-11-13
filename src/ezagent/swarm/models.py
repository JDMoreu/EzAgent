"""Modelos y validación del tablero del enjambre.

Todo texto que llega de un agente es dato no confiable (AGENTS.md §5): aquí se
aplana a una línea, se recorta y se valida antes de guardarlo.
"""

from __future__ import annotations

import re
from dataclasses import dataclass

MAX_BODY = 280
MAX_TITLE = 120
MAX_RESULT = 280
MAX_GOAL = 600
MAX_REF = 260
MAX_INBOX = 20
MAX_ROUNDS = 3
DEFAULT_LEASE_SECS = 900.0
BROADCAST = "*"
SYSTEM = "ez"
MESSAGE_KINDS = frozenset({"info", "ask", "answer", "done", "block", "fix", "sys"})
MEMBER_STATES = {"idle": "libre", "working": "trabaja", "blocked": "bloqueado", "gone": "fuera"}
SWARM_STATES = {
    "setup": "preparando",
    "active": "activo",
    "paused": "en pausa",
    "done": "terminado",
    "stopped": "detenido",
}
TASK_MARKS = {"pending": "·", "claimed": "▶", "done": "✓", "failed": "✗"}
NAME_RE = re.compile(r"^[a-z0-9][a-z0-9_-]{0,31}$")


class SwarmError(ValueError):
    """Petición inválida al tablero; el mensaje es legible para el agente."""


# ---------------------------------------------------------------- modelos
@dataclass(frozen=True)
class SwarmInfo:
    id: str
    goal: str
    cwd: str
    status: str
    created: float
    updated: float


@dataclass(frozen=True)
class Member:
    name: str
    role: str
    agent: str
    address: str
    state: str
    seen: float
    model: str = ""
    effort: str = ""

    def label(self) -> str:
        agent = f"({self.agent})" if self.agent else ""
        return f"{self.name}{agent} {MEMBER_STATES.get(self.state, self.state)}"


@dataclass(frozen=True)
class Task:
    id: int
    title: str
    role: str
    owner: str
    state: str
    deps: tuple[int, ...]
    parent: int
    result: str
    rounds: int
    updated: float = 0.0

    def line(self, parts: str = "") -> str:
        """Una línea para el tablero; ``parts`` es «hechas/total» si tiene partes."""
        who = f" @{self.owner}" if self.owner else (f" [{self.role}]" if self.role else "")
        deps = ""
        if self.deps and self.state == "pending":
            deps = " tras " + ",".join(f"T{dep}" for dep in self.deps)
        progress = f" {parts}" if parts else ""
        result = f" → {self.result}" if self.result else ""
        mark = TASK_MARKS.get(self.state, "?")
        return f"T{self.id} {mark} {self.title}{who}{progress}{deps}{result}"


@dataclass(frozen=True)
class Message:
    id: int
    sender: str
    recipient: str
    kind: str
    body: str
    ref: str
    ts: float

    def line(self) -> str:
        target = "todos" if self.recipient == BROADCAST else self.recipient
        ref = f" [{self.ref}]" if self.ref else ""
        return f"#{self.id} {self.sender}→{target} {self.kind}: {self.body}{ref}"


@dataclass(frozen=True)
class Lease:
    path: str
    member: str
    expires: float


@dataclass(frozen=True)
class Incident:
    """Algo que impide trabajar a un miembro (ver ``incidents.py``)."""

    id: int
    swarm: str
    member: str
    kind: str
    detail: str
    resets: float
    state: str
    action: str
    created: float
    updated: float


# ------------------------------------------------------------- validación
def flat(value: object) -> str:
    return " ".join(str(value or "").replace("\x00", " ").split())


def clip(value: object, limit: int) -> tuple[str, bool]:
    """Texto en una línea, recortado a ``limit`` con «…» si hace falta."""
    text = flat(value)
    if len(text) <= limit:
        return text, False
    return text[: limit - 1].rstrip() + "…", True


def required(value: object, limit: int, what: str) -> str:
    text, _ = clip(value, limit)
    if not text:
        raise SwarmError(f"Falta {what}.")
    return text


def member_name(value: object, what: str = "nombre") -> str:
    text = str(value or "").strip().lower()
    if not NAME_RE.match(text):
        raise SwarmError(f"El {what} {text!r} no es válido: usa a-z, 0-9, «-» o «_».")
    return text


def role_name(value: object) -> str:
    """Rol normalizado; vacío significa «cualquiera»."""
    text = str(value or "").strip().lower()
    return member_name(text, "rol") if text else ""


def task_id(value: object) -> int:
    """Acepta «T3», «t3» o «3»."""
    text = str(value or "").strip().lstrip("Tt")
    if not text.isdigit():
        raise SwarmError(f"Número de tarea no válido: {value!r}.")
    return int(text)


def lease_path(value: object) -> str:
    """Ruta normalizada para reservas: relativa, con «/» y sin «..»."""
    text = str(value or "").replace("\x00", "").strip().replace("\\", "/")
    while text.startswith("./"):
        text = text[2:]
    text = text.rstrip("/")
    parts = [part for part in text.split("/") if part not in ("", ".")]
    if not parts or ".." in parts or len(text) > MAX_REF:
        raise SwarmError(f"Ruta de reserva no válida: {value!r}.")
    # DECISION: Windows no distingue mayúsculas; se comparan en minúsculas.
    return "/".join(parts).casefold()


def overlaps(first: str, second: str) -> bool:
    return first == second or first.startswith(second + "/") or second.startswith(first + "/")
