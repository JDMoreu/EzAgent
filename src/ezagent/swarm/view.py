"""Vistas del enjambre: tablero compacto y chequeo de salud.

Todo el texto lo arma el código a partir del estado guardado; nunca un modelo.
El tablero está pensado para gastar poco: una línea por miembro, tarea y
mensaje. La salud responde «¿va todo bien?» con reglas simples y comprobables.
"""

from __future__ import annotations

from dataclasses import dataclass

from .incidents import describe
from .models import BROADCAST, SWARM_STATES, Member, Message, SwarmError, Task
from .protocol import role_label
from .store import SwarmStore

STALL_SECS = 900.0
UNCLAIMED_SECS = 180.0
ASK_SECS = 600.0


def _minutes(seconds: float) -> int:
    return max(1, round(seconds / 60))


def _parts(tasks: list[Task]) -> dict[int, list[Task]]:
    grouped: dict[int, list[Task]] = {}
    for task in tasks:
        if task.parent:
            grouped.setdefault(task.parent, []).append(task)
    return grouped


def board(store: SwarmStore, swarm: str, *, member: str = "", recent: int = 4) -> str:
    """Tablero compacto: estado, miembros, tareas, reservas y últimos mensajes."""
    info = store.get(swarm)
    if info is None:
        raise SwarmError(f"No existe el enjambre {swarm!r}.")
    tasks = store.tasks(swarm)
    parts = _parts(tasks)
    leaves = [task for task in tasks if task.id not in parts]
    done = sum(task.state == "done" for task in leaves)
    lines = [f"Enjambre {info.id} · {SWARM_STATES.get(info.status, info.status)} · {info.goal}"]
    members = store.members(swarm)
    if members:
        lines.append("Miembros: " + " · ".join(item.label() for item in members))
    if tasks:
        lines.append(f"Tareas {done}/{len(leaves)}:")
    for task in tasks:
        if task.parent:
            continue
        below = parts.get(task.id, [])
        count = f"{sum(p.state == 'done' for p in below)}/{len(below)}" if below else ""
        lines.append(task.line(count))
        lines += [f"  {part.line()}" for part in below]
    leases = store.leases(swarm)
    if leases:
        lines.append("Reservas: " + " · ".join(f"{item.path} @{item.member}" for item in leases))
    last = [message.line() for message in store.history(swarm, limit=recent)]
    if last:
        lines += ["Últimos:"] + last
    if member:
        lines.append(f"Sin leer para {member}: {store.unread(swarm, member)}")
    return "\n".join(lines)


# ------------------------------------------------------------------ salud
@dataclass(frozen=True)
class Issue:
    """Algo que necesita atención. ``key`` no cambia aunque cambie el texto."""

    key: str
    text: str
    member: str = ""


@dataclass(frozen=True)
class Health:
    state: str  # empty | ok | attention | done
    done: int
    total: int
    working: tuple[str, ...]
    found: tuple[Issue, ...] = ()

    @property
    def issues(self) -> tuple[str, ...]:
        return tuple(issue.text for issue in self.found)

    def spoken(self) -> str:
        """Frase corta para decir por voz."""
        if self.state == "empty":
            return "El enjambre todavía no tiene tareas."
        if self.state == "done":
            return f"El enjambre terminó: {self.total} de {self.total} tareas hechas."
        text = f"Van {self.done} de {self.total} tareas."
        if self.working:
            text += " Trabajando: " + ", ".join(self.working) + "."
        if not self.issues:
            return text + " Todo en orden."
        extra = len(self.issues) - 1
        more = f" Y {extra} aviso{'s' if extra > 1 else ''} más." if extra else ""
        return f"{text} Atención: {self.issues[0]}{more}"


def _task_issues(
    tasks: list[Task], members: list[Member], parts: dict[int, list[Task]], now: float
) -> list[Issue]:
    by_id = {task.id: task for task in tasks}
    by_name = {member.name: member for member in members}
    roles = {member.role for member in members if member.state != "gone"}
    issues: list[Issue] = []
    for task in tasks:
        key = f"T{task.id}"
        if task.state == "failed":
            text = f"{key} falló: {task.result or task.title}"
            issues.append(Issue(f"failed:{key}", text, task.owner))
        elif task.state == "claimed":
            owner = by_name.get(task.owner)
            idle = now - owner.seen if owner else 0.0
            if idle > STALL_SECS:
                text = f"{task.owner} lleva {_minutes(idle)} min sin señales en {key}."
                issues.append(Issue(f"stall:{key}", text, task.owner))
        elif task.state == "pending" and task.id not in parts:
            above = by_id.get(task.parent)
            deps = set(task.deps) | (set(above.deps) if above else set())
            broken = sorted(dep for dep in deps if dep in by_id and by_id[dep].state == "failed")
            if broken:
                text = f"{key} no puede empezar: depende de T{broken[0]}, que falló."
                issues.append(Issue(f"broken:{key}", text))
            elif task.owner and by_name.get(task.owner, None) is None:
                text = f"{key} está a nombre de {task.owner}, que no es miembro."
                issues.append(Issue(f"orphan:{key}", text))
            elif not task.owner and task.role and task.role not in roles:
                text = f"{key} es para el rol «{task.role}» y nadie lo tiene."
                issues.append(Issue(f"orphan:{key}", text))
    return issues


def _unclaimed_issues(
    store: SwarmStore, swarm: str, tasks: list[Task], members: list[Member], now: float
) -> list[Issue]:
    """Tareas listas que nadie toma: señal de que un agente no despertó."""
    by_id = {task.id: task for task in tasks}
    issues: list[Issue] = []
    flagged: set[int] = set()
    for member in members:
        if member.state in ("gone", "working"):
            continue
        for task in store.ready(swarm, member.name):
            if task.id in flagged:
                continue
            above = by_id.get(task.parent)
            deps = set(task.deps) | (set(above.deps) if above else set())
            since = max([task.updated] + [by_id[dep].updated for dep in deps if dep in by_id])
            waiting = now - max(since, member.seen)
            if waiting > UNCLAIMED_SECS:
                flagged.add(task.id)
                text = f"T{task.id} lleva {_minutes(waiting)} min lista y "
                text += f"{member.name} no la toma."
                issues.append(Issue(f"unclaimed:T{task.id}", text, member.name))
    return issues


def _ask_issues(messages: list[Message], now: float) -> list[Issue]:
    issues: list[Issue] = []
    for index, message in enumerate(messages):
        if message.kind != "ask" or message.recipient == BROADCAST:
            continue
        age = now - message.ts
        answered = any(
            later.sender == message.recipient and later.recipient in (message.sender, BROADCAST)
            for later in messages[index + 1 :]
        )
        if age > ASK_SECS and not answered:
            text = f"{message.recipient} no respondió a {message.sender} en "
            text += f"{_minutes(age)} min."
            issues.append(Issue(f"ask:{message.id}", text, message.recipient))
    return issues


def _incident_issues(
    store: SwarmStore, swarm: str, members: list[Member], issues: list[Issue]
) -> list[Issue]:
    """Los incidentes van primero y tapan los avisos derivados del mismo miembro."""
    by_name = {member.name: member for member in members}
    found = [i for i in store.incidents(swarm) if i.member in by_name]
    held = {incident.member for incident in found}
    first = [
        Issue(
            f"incident:{incident.id}",
            describe(
                incident,
                by_name[incident.member],
                [f"T{t.id}" for t in store.mine(swarm, incident.member)],
            ),
            incident.member,
        )
        for incident in found
    ]
    info = store.get(swarm)
    if info is not None and info.status == "paused":
        first.insert(0, Issue("paused", "El enjambre está en pausa."))
    return first + [issue for issue in issues if issue.member not in held]


def health(store: SwarmStore, swarm: str, *, now: float) -> Health:
    """¿Va todo bien? Progreso, quién trabaja y qué necesita atención."""
    if store.get(swarm) is None:
        raise SwarmError(f"No existe el enjambre {swarm!r}.")
    tasks = store.tasks(swarm)
    members = store.members(swarm)
    parts = _parts(tasks)
    leaves = [task for task in tasks if task.id not in parts]
    done = sum(task.state == "done" for task in leaves)
    held = {incident.member for incident in store.incidents(swarm)}
    working = tuple(
        f"{task.owner} en T{task.id}"
        for task in leaves
        if task.state == "claimed" and task.owner not in held
    )
    if not leaves:
        return Health("empty", 0, 0, ())
    if done == len(leaves):
        return Health("done", done, len(leaves), ())
    issues = _task_issues(tasks, members, parts, now)
    issues += [
        Issue(f"blocked:{m.name}", f"{m.name} está bloqueado.", m.name)
        for m in members
        # Un miembro bloqueado sin nada que hacer no frena al enjambre.
        if m.state == "blocked"
        and (
            any(t.owner == m.name and t.state in ("claimed", "failed") for t in tasks)
            or store.ready(swarm, m.name)
        )
    ]
    issues += _unclaimed_issues(store, swarm, tasks, members, now)
    issues += _ask_issues(store.history(swarm, limit=200), now)
    issues = _incident_issues(store, swarm, members, issues)
    state = "attention" if issues else "ok"
    return Health(state, done, len(leaves), working, tuple(issues))


def summary(store: SwarmStore, swarm: str, *, now: float) -> str:
    """Resumen final para decir por voz: progreso y resultado de cada etapa."""
    state = health(store, swarm, now=now)
    stages = [
        f"{role_label(task.role).capitalize()}: {task.result}"
        for task in store.tasks(swarm)
        if not task.parent and task.result
    ]
    return " ".join([state.spoken()] + [s if s.endswith(".") else s + "." for s in stages])


__all__ = ["Health", "Issue", "board", "health", "summary"]
