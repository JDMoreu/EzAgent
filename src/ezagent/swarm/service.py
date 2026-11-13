"""Servicio del enjambre: identidad por token y comandos de los agentes.

Cada miembro recibe un token propio al crearse el enjambre. Con él, el servicio
sabe quién habla (enjambre, nombre y rol) sin que el agente lo diga, y nadie
puede hacerse pasar por otro. Los comandos devuelven texto corto en lugar de
JSON para gastar pocos tokens, y los que cierran trabajo ya traen lo siguiente
(``done`` responde con la próxima tarea) para ahorrar una ida y vuelta.

Nada de lo que envía un agente se ejecuta ni se interpreta: solo cambia el
tablero. El token nunca se guarda (solo su resumen) ni se registra en logs.
"""

from __future__ import annotations

import hashlib
import re
import secrets
import threading
import time
from collections.abc import Callable, Sequence
from dataclasses import dataclass

from loguru import logger

from . import protocol
from .models import (
    BROADCAST,
    DEFAULT_LEASE_SECS,
    MAX_ROUNDS,
    SYSTEM,
    Member,
    SwarmError,
    Task,
    task_id,
)
from .store import SwarmStore
from .view import Health, board, health, summary

MAX_TASKS = 60
MAX_WAIT_SECS = 110.0
BURST_MESSAGES = 10
BURST_SECS = 60.0
_VALUE_OPTIONS = frozenset({"ref", "role", "to", "after", "in", "min"})
_FLAG_OPTIONS = frozenset({"all"})
_TASK_WORD = re.compile(r"^[Tt]\d+$")
_QUIET = frozenset({"me", "help", "board", "inbox", "wait"})
Listener = Callable[[str, str, str], None]


@dataclass(frozen=True)
class Reply:
    ok: bool
    text: str


@dataclass(frozen=True)
class MemberSpec:
    """Un miembro que se quiere en el enjambre."""

    name: str
    role: str
    agent: str
    model: str = ""
    effort: str = ""


@dataclass(frozen=True)
class Seat:
    """Lo que EzAgent necesita para sentar a un agente en el enjambre."""

    name: str
    token: str
    briefing: str


@dataclass(frozen=True)
class _Call:
    swarm: str
    me: Member
    words: list[str]
    options: dict[str, str]

    def text(self, skip: int = 0) -> str:
        return " ".join(self.words[skip:])


def _digest(token: str) -> str:
    return hashlib.sha256(token.encode("utf-8")).hexdigest()


def _parse(args: Sequence[str]) -> tuple[list[str], dict[str, str]]:
    """Separa palabras y opciones (``--ref x``, ``--ref=x``, ``--all``)."""
    words: list[str] = []
    options: dict[str, str] = {}
    index = 0
    while index < len(args):
        item = str(args[index])
        index += 1
        if not item.startswith("--") or item == "--":
            words.append(item)
            continue
        key, _, value = item[2:].partition("=")
        if key in _FLAG_OPTIONS:
            options[key] = "1"
        elif key in _VALUE_OPTIONS:
            if not value and index < len(args):
                value = str(args[index])
                index += 1
            options[key] = value
        else:
            raise SwarmError(f"Opción desconocida: --{key}.")
    return words, options


class SwarmService:
    """Punto de entrada único para EzAgent (en proceso) y los agentes (por token)."""

    def __init__(
        self,
        store: SwarmStore,
        *,
        command: str = "ezs",
        clock: Callable[[], float] = time.time,
    ) -> None:
        self.store = store
        self.command = command
        self._clock = clock
        self._news = threading.Condition()
        self._closed = False
        self._listeners: list[Listener] = []
        self._commands: dict[str, Callable[[_Call], str]] = {
            "next": self._next,
            "done": self._done,
            "fail": self._fail,
            "fix": self._fix,
            "add": self._add,
            "say": lambda call: self._say(call, "info"),
            "ask": lambda call: self._say(call, "ask"),
            "answer": lambda call: self._say(call, "answer"),
            "inbox": self._inbox,
            "lock": self._lock_paths,
            "unlock": self._unlock,
            "board": lambda call: board(self.store, call.swarm, member=call.me.name),
            "me": lambda call: self.briefing(call.swarm, call.me.name),
            "help": lambda call: protocol.help_text(self.command),
            "wait": self._wait,
        }

    # ------------------------------------------------------ lado de EzAgent
    def start(
        self,
        goal: str,
        members: Sequence[MemberSpec | tuple[str, ...]],
        *,
        cwd: str = "",
        tasks: Sequence[tuple[str, str]] | None = None,
    ) -> tuple[str, list[Seat]]:
        """Crea un enjambre. ``tasks``: (título, rol); los miembros, en orden de trabajo.

        Las tareas quedan encadenadas en orden; sin ``tasks`` hay una por rol.
        """
        if not members:
            raise SwarmError("Un enjambre necesita al menos un miembro.")
        specs = [m if isinstance(m, MemberSpec) else MemberSpec(*m) for m in members]
        # Se arma en «setup»: el supervisor no lo ve (ni abre agentes ni emite
        # tokens) hasta que está completo. Sin esto, un lanzamiento a mitad de
        # la creación dejaba al agente con un token ya invalidado.
        swarm = self.store.create(goal, cwd=cwd, status="setup")
        try:
            seats = self._populate(swarm, specs, tasks)
        except Exception:
            self.store.set_status(swarm, "stopped")
            raise
        self.store.set_status(swarm, "active")
        logger.info("Enjambre {} creado con {} miembros.", swarm, len(seats))
        return swarm, seats

    def _populate(
        self,
        swarm: str,
        specs: Sequence[MemberSpec],
        tasks: Sequence[tuple[str, str]] | None,
    ) -> list[Seat]:
        for spec in specs:
            self.store.join(
                swarm,
                spec.name,
                role=spec.role,
                agent=spec.agent,
                model=spec.model,
                effort=spec.effort,
            )
        plan = tasks if tasks is not None else protocol.default_tasks([s.role for s in specs])
        previous: list[int] = []
        for title, role in plan:
            task = self.store.add_task(swarm, title, role=role, deps=previous)
            previous = [task.id]
        return [
            Seat(m.name, self.issue_token(swarm, m.name), self.briefing(swarm, m.name))
            for m in self.store.members(swarm)
        ]

    def issue_token(self, swarm: str, member: str) -> str:
        """Token nuevo para un miembro; el anterior deja de servir."""
        token = secrets.token_urlsafe(24)
        self.store.set_token(swarm, member, _digest(token))
        return token

    def briefing(self, swarm: str, member: str) -> str:
        info = self.store.get(swarm)
        members = self.store.members(swarm)
        me = next((m for m in members if m.name == member), None)
        if info is None or me is None:
            raise SwarmError(f"{member!r} no es miembro del enjambre {swarm}.")
        return protocol.briefing(info, me, members, command=self.command)

    def status(self, swarm: str) -> Health:
        return health(self.store, swarm, now=self._clock())

    def pending(self, swarm: str, member: str) -> tuple[int, int]:
        """(mensajes que piden atención, tareas listas) de un miembro, para despertarlo."""
        unread, _, ready = self.store.pending(swarm, member)
        return unread, len(ready)

    def tell(self, swarm: str, text: str, *, to: str = BROADCAST) -> None:
        """Mensaje del usuario (vía EzAgent) a un miembro o a todos."""
        self.store.post(swarm, SYSTEM, text, to=to)
        self._changed(swarm, SYSTEM, "tell")

    def latest(self) -> str:
        """Enjambre activo más reciente, o ''."""
        active = self.store.active()
        return active[-1].id if active else ""

    def summary(self, swarm: str) -> str:
        return summary(self.store, swarm, now=self._clock())

    def wake_line(self, swarm: str, member: str) -> str:
        """Línea para pegar en la terminal del miembro, o '' si no hay nada."""
        unread, ready = self.pending(swarm, member)
        if not unread and not ready:
            return ""
        return protocol.wake_line(unread, ready, command=self.command)

    def join_line(self, swarm: str, member: str) -> str:
        """Línea con la que se sienta a un agente recién abierto."""
        info = self.store.get(swarm)
        me = next((m for m in self.store.members(swarm) if m.name == member), None)
        if info is None or me is None:
            raise SwarmError(f"{member!r} no es miembro del enjambre {swarm}.")
        return protocol.join_line(info, me, command=self.command)

    def stop(self, swarm: str) -> None:
        self.store.set_status(swarm, "stopped")
        self._changed(swarm, SYSTEM, "stop")

    def subscribe(self, listener: Listener) -> None:
        """``listener(enjambre, miembro, comando)`` tras cada cambio. Debe ser rápido."""
        self._listeners.append(listener)

    def open(self) -> None:
        with self._news:
            self._closed = False

    def close(self) -> None:
        """Libera a los agentes que estén en ``wait``."""
        with self._news:
            self._closed = True
            self._news.notify_all()

    # ------------------------------------------------- lado de los agentes
    def authenticate(self, token: str) -> tuple[str, Member] | None:
        return self.store.by_token(_digest(token)) if token else None

    def handle(self, token: str, argv: Sequence[str]) -> Reply:
        """Ejecuta un comando de un agente identificado por su token."""
        who = self.authenticate(token)
        if who is None:
            return Reply(False, protocol.NO_SEAT)
        swarm, me = who
        op = str(argv[0]).lower() if argv else "next"
        handler = self._commands.get(op)
        if handler is None:
            return Reply(False, f"Comando desconocido: {op}. Mira: {self.command} help")
        try:
            info = self.store.get(swarm)
            if info is not None and info.status == "stopped":
                return Reply(True, protocol.STOPPED)
            self.store.touch(swarm, me.name)
            words, options = _parse(list(argv[1:]))
            text = handler(_Call(swarm, me, words, options))
        except SwarmError as exc:
            return Reply(False, str(exc))
        except Exception:  # noqa: BLE001 - un fallo interno no tumba al agente
            logger.exception("Fallo en el comando {} de {} ({})", op, me.name, swarm)
            return Reply(False, "Error interno del tablero. Reintenta una vez.")
        if op not in _QUIET:
            self._changed(swarm, me.name, op)
        return Reply(True, text)

    # ------------------------------------------------------------ comandos
    def _next(self, call: _Call) -> str:
        """Mensajes nuevos y la tarea del miembro (la toma si no tiene)."""
        lines: list[str] = []
        if self.store.mark_briefed(call.swarm, call.me.name):
            # Contexto bajo demanda: el papel y las reglas llegan con el primer
            # `next`, no pegados en la terminal al abrir el agente.
            lines += [self.briefing(call.swarm, call.me.name), ""]
        messages = self.store.inbox(call.swarm, call.me.name)
        if messages:
            lines.append(f"Mensajes ({len(messages)}):")
            lines += [message.line() for message in messages]
        mine = self.store.mine(call.swarm, call.me.name)
        every = "all" in call.options
        taken = self.store.claim(call.swarm, call.me.name, every=every) if every or not mine else []
        tasks = self.store.tasks(call.swarm)
        lines += [f"En curso: {self._describe(task, tasks)}" for task in mine]
        lines += [f"Tu tarea: {self._describe(task, tasks)}" for task in taken]
        if not mine and not taken:
            lines.append(protocol.ALL_DONE if self._finished(call.swarm) else protocol.IDLE)
        return "\n".join(lines)

    @staticmethod
    def _describe(task: Task, tasks: Sequence[Task]) -> str:
        text = f"T{task.id} {task.title}"
        if task.rounds and task.result:
            text += f" · corregir ({task.rounds}/{MAX_ROUNDS}): {task.result}"
        after = [f"T{t.id} {t.title}" for t in tasks if task.id in t.deps][:2]
        return text + (f" · luego: {'; '.join(after)}" if after else "")

    def _finished(self, swarm: str) -> bool:
        return self.status(swarm).state == "done"

    def _own_task(self, call: _Call) -> tuple[int, str]:
        """Tarea en curso a la que se refiere el comando y el texto restante."""
        mine = [task.id for task in self.store.mine(call.swarm, call.me.name)]
        first = call.words[0] if call.words else ""
        explicit = _TASK_WORD.match(first) or (first.isdigit() and int(first) in mine)
        if explicit:
            return task_id(first), call.text(1)
        if len(mine) == 1:
            return mine[0], call.text()
        if not mine:
            raise SwarmError(f"No tienes tareas en curso. Usa: {self.command} next")
        listed = ", ".join(f"T{item}" for item in mine)
        raise SwarmError(f"Tienes varias en curso ({listed}): indica cuál, p. ej. done T{mine[0]}.")

    def _done(self, call: _Call) -> str:
        task, result = self._own_task(call)
        if not result:
            raise SwarmError(f'Falta el resumen: {self.command} done "qué hiciste en una línea"')
        self.store.finish(call.swarm, call.me.name, task, result, ref=call.options.get("ref", ""))
        if self._finished(call.swarm):
            self.store.set_status(call.swarm, "done")
            self._changed(call.swarm, call.me.name, "finished")
        return f"T{task} hecha.\n{self._next(call)}"

    def _fail(self, call: _Call) -> str:
        task, reason = self._own_task(call)
        if not reason:
            raise SwarmError(f'Falta el motivo: {self.command} fail "por qué no puedes seguir"')
        self.store.finish(call.swarm, call.me.name, task, reason, ok=False)
        return f"T{task} marcada como fallida; se avisó al equipo y al usuario. Termina tu turno."

    def _fix(self, call: _Call) -> str:
        if len(call.words) < 2:
            raise SwarmError(f'Uso: {self.command} fix <T> "qué falta" [--ref informe]')
        target = task_id(call.words[0])
        before = {task.id for task in self.store.mine(call.swarm, call.me.name)}
        task, exhausted = self.store.reopen(
            call.swarm, target, by=call.me.name, note=call.text(1), ref=call.options.get("ref", "")
        )
        if exhausted:
            return (
                f"T{target} superó {MAX_ROUNDS} rondas de corrección; se avisó al usuario. "
                "Termina tu turno."
            )
        after = {item.id for item in self.store.mine(call.swarm, call.me.name)}
        text = f"T{target} reabierta (ronda {task.rounds}/{MAX_ROUNDS})."
        released = sorted(before - after)
        if released:
            names = ", ".join(f"T{item}" for item in released)
            text += f" {names} volverá a ti cuando esté corregida. Termina tu turno."
        return text

    def _add(self, call: _Call) -> str:
        if len(self.store.tasks(call.swarm)) >= MAX_TASKS:
            raise SwarmError(f"El enjambre ya tiene {MAX_TASKS} tareas; agrupa el trabajo.")
        options = call.options
        deps = [task_id(item) for item in options.get("after", "").split(",") if item.strip()]
        task = self.store.add_task(
            call.swarm,
            call.text(),
            role=options.get("role", ""),
            owner=options.get("to", "").lower(),
            deps=deps,
            parent=task_id(options["in"]) if options.get("in") else 0,
        )
        return f"Creada: {task.line()}"

    def _say(self, call: _Call, kind: str) -> str:
        if len(call.words) < 2:
            name = "say" if kind == "info" else kind
            raise SwarmError(f'Uso: {self.command} {name} <nombre|all> "texto"')
        recent = [
            message
            for message in self.store.history(call.swarm, limit=40)
            if message.sender == call.me.name and self._clock() - message.ts < BURST_SECS
        ]
        if len(recent) >= BURST_MESSAGES:
            raise SwarmError("Demasiados mensajes seguidos. Junta lo que falta en uno solo.")
        target = call.words[0].lower()
        to = BROADCAST if target in ("all", "todos", BROADCAST) else target
        message, cut = self.store.post(
            call.swarm,
            call.me.name,
            call.text(1),
            to=to,
            kind=kind,
            ref=call.options.get("ref", ""),
        )
        note = " Se recortó a 280 caracteres: para el detalle usa --ref <archivo>." if cut else ""
        return f"#{message.id} enviado a {'todos' if to == BROADCAST else to}.{note}"

    def _inbox(self, call: _Call) -> str:
        messages = self.store.inbox(call.swarm, call.me.name)
        return "\n".join(message.line() for message in messages) or "Sin mensajes nuevos."

    def _lock_paths(self, call: _Call) -> str:
        minutes = call.options.get("min", "")
        ttl = float(minutes) * 60 if minutes.isdigit() else DEFAULT_LEASE_SECS
        conflicts = self.store.reserve(call.swarm, call.me.name, call.words, ttl=ttl)
        if conflicts:
            taken = "; ".join(f"{item.path} lo tiene {item.member}" for item in conflicts)
            raise SwarmError(f"Ocupado: {taken}. Elige otros archivos o coordínalo con say.")
        return f"Reservado: {', '.join(call.words)}."

    def _unlock(self, call: _Call) -> str:
        count = self.store.release(call.swarm, call.me.name, call.words or None)
        return f"Reservas liberadas: {count}."

    def _wait(self, call: _Call) -> str:
        """Espera corta a que haya un mensaje o una tarea; luego responde como next."""
        first = call.words[0] if call.words else ""
        seconds = min(max(float(first) if first.isdigit() else 60.0, 1.0), MAX_WAIT_SECS)
        deadline = time.monotonic() + seconds
        with self._news:
            while not self._closed and not any(self.pending(call.swarm, call.me.name)):
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    return f"Sin novedades en {int(seconds)} s."
                self._news.wait(min(remaining, 5.0))
        return self._next(call)

    # ------------------------------------------------------------- eventos
    def _changed(self, swarm: str, member: str, op: str) -> None:
        with self._news:
            self._news.notify_all()
        for listener in list(self._listeners):
            try:
                listener(swarm, member, op)
            except Exception:  # noqa: BLE001 - un oyente roto no afecta al agente
                logger.exception("Fallo en un oyente del enjambre {}", swarm)


__all__ = ["MemberSpec", "Reply", "Seat", "SwarmService"]
