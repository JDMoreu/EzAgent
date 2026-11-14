"""Supervisor del enjambre: mantiene el proceso andando sin intervención.

Los agentes delegados no pueden dejar una conexión abierta esperando mensajes,
así que el supervisor hace de canal: vigila el tablero y, cuando un miembro
tiene algo pendiente y su agente está libre, le escribe una línea en su
terminal. Para el agente es una suscripción: se entera solo de lo suyo y en el
momento en que puede atenderlo, sin consultar ni gastar tokens mientras espera.

Qué hace en cada pasada:

- **Abre a cada miembro cuando le toca** (no antes): un validador no ocupa una
  terminal ni contexto hasta que hay algo que validar.
- **Despierta** al miembro libre con mensajes o tareas, y le recuerda cerrar una
  tarea que dejó abierta al terminar su turno.
- **Marca actividad**: si Herdr ve al agente trabajando, cuenta como latido.
- **Avisa al usuario** de cada etapa que termina y la que empieza, de lo que
  necesita atención y del final (con la pregunta de qué hacer después).

Corre en su propio hilo; nunca en el de UI ni en los de audio.
"""

from __future__ import annotations

import os
import threading
import time
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from loguru import logger

from . import protocol
from .models import Member, SwarmInfo
from .progress import Progress
from .recovery import Recovery
from .service import SwarmService

TICK_SECS = 3.0
IDLE_TICK_SECS = 20.0
RENUDGE_SECS = 120.0
MAX_NUDGES = 3
OPEN_TASK_SECS = 45.0
BRIEF_RETRY_SECS = 20.0
LAUNCH_SETTLE_SECS = 4.0
MISSING_TICKS = 3
_WORKING = "working"
_IDLE = "idle"
_BLOCKED = "blocked"
_ERROR = "error"
_MISSING = "missing"
_UNKNOWN = "unknown"
_EXITED = "exited"
MAX_LAUNCHES = 2
_SHELLS = frozenset({"", "powershell", "pwsh", "cmd", "bash", "zsh", "sh", "fish", "nu"})


@dataclass
class _Nudge:
    signature: tuple[Any, ...]
    count: int
    at: float


def _agent_state(raw: object) -> str:
    state = str(raw or "").strip().lower()
    if state == "working":
        return _WORKING
    if state in ("done", "idle"):
        return _IDLE
    if state == "blocked":
        return _BLOCKED
    if state == "error":
        return _ERROR
    return _UNKNOWN


class Supervisor:
    """Lanza, despierta y vigila a los miembros de los enjambres activos."""

    def __init__(
        self,
        service: SwarmService,
        herdr: Any,
        *,
        seat_dir: Path,
        announce: Callable[[str], None],
        ask: Callable[[str], None] | None = None,
        command_dir: str = "",
        clock: Callable[[], float] = time.monotonic,
        wall: Callable[[], float] = time.time,
    ) -> None:
        self.service = service
        self.herdr = herdr
        self.seat_dir = Path(seat_dir)
        self.announce = announce
        self.ask = ask or announce
        self.command_dir = command_dir
        # Enjambres activos en la última pasada (lo lee la UI para su órbita y su
        # vista rápida). Se reasigna una tupla entera: la lectura no necesita lock.
        self.running = 0
        self.active_view: tuple[tuple[str, str], ...] = ()
        self.progress = Progress()
        self._clock = clock
        self._lock = threading.Lock()
        self._wake = threading.Event()
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None
        self._launching: set[tuple[str, str]] = set()
        self._nudged: dict[tuple[str, str], _Nudge] = {}
        self._idle_since: dict[tuple[str, str], float] = {}
        self._announced: dict[str, set[str]] = {}
        self._finished: list[str] = []
        # Miembros abiertos que aún no han usado `ezs`: no consta que leyeran
        # sus instrucciones (el pegado inicial puede perderse si el programa
        # todavía estaba arrancando).
        self._unbriefed: set[tuple[str, str]] = set()
        self._missing: dict[tuple[str, str], int] = {}
        self._launches: dict[tuple[str, str], int] = {}
        self.recovery = Recovery(
            service,
            herdr,
            handoff_dir=self.seat_dir.parent / "handoff",
            announce=announce,
            ask=ask or announce,
            reset_launches=self._reset_launches,
            wall=wall,
            clock=clock,
        )
        service.subscribe(self._on_event)

    # ------------------------------------------------------------ ciclo
    def start(self) -> None:
        if self._thread is not None:
            return
        self._stop.clear()
        self._thread = threading.Thread(target=self._loop, name="ezagent-swarm-sup", daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()
        self._wake.set()
        thread, self._thread = self._thread, None
        if thread is not None:
            thread.join(timeout=5)

    def poke(self) -> None:
        """Adelanta la próxima pasada (hay novedades en el tablero)."""
        self._wake.set()

    def _on_event(self, swarm: str, member: str, op: str) -> None:
        # Llega desde el hilo del servidor: solo se anota y se despierta el ciclo.
        with self._lock:
            self._unbriefed.discard((swarm, member))
            if op == "finished":
                self._finished.append(swarm)
        self._wake.set()

    def _loop(self) -> None:
        while not self._stop.is_set():
            busy = False
            try:
                busy = self.tick()
            except Exception:  # noqa: BLE001 - el supervisor no debe morir
                logger.exception("Fallo en una pasada del supervisor del enjambre")
            self._wake.wait(TICK_SECS if busy else IDLE_TICK_SECS)
            self._wake.clear()

    def tick(self) -> bool:
        """Una pasada completa. Devuelve True si hay enjambres activos."""
        with self._lock:
            finished, self._finished = self._finished, []
        for swarm in finished:
            self._announced.pop(swarm, None)
            self.progress.forget(swarm)
            # Reabre la escucha: el usuario decide qué hacer con el resultado.
            self.ask(f"{self.service.summary(swarm)} {protocol.FINAL_OFFER}")
        active = self.service.store.active()
        self.running = len(active)
        self.active_view = tuple((info.id, info.goal) for info in active)
        if not active:
            return False
        states = self._herdr_states()
        for info in active:
            self._tick_swarm(info, states)
        return True

    # ----------------------------------------------------------- por enjambre
    def _herdr_states(self) -> dict[str, str] | None:
        """Estado de cada terminal según Herdr, o None si no se pudo consultar."""
        try:
            cached = getattr(self.herdr, "cached_agents", None)
            agents = cached(TICK_SECS) if callable(cached) else None
            if agents is None:
                agents = self.herdr.list_agents(timeout=10)
        except Exception as exc:  # noqa: BLE001 - Herdr caído: se reintenta luego
            logger.debug("El supervisor no pudo consultar Herdr: {}", exc)
            return None
        return {str(a.get("address") or ""): _agent_state(a.get("state")) for a in agents}

    def _tick_swarm(self, info: SwarmInfo, states: dict[str, str] | None) -> None:
        store = self.service.store
        for member in store.members(info.id):
            key = (info.id, member.name)
            if member.state == "gone":
                continue
            if not member.address:
                with self._lock:
                    waiting = key in self._launching
                has_work = store.pending(info.id, member.name)[2] or store.mine(
                    info.id, member.name
                )
                if not waiting and has_work:
                    self._launch(info, member)
                continue
            if states is None:
                continue
            state = states.get(member.address) or self._probe(member.address)
            if self.recovery.hold(info, member, working=state == _WORKING):
                continue  # bajo un incidente: lo atiende la recuperación
            if state not in (_MISSING, _EXITED):
                self._missing.pop(key, None)
            if state in (_MISSING, _EXITED):
                # Una sola lectura no basta: se confirma en varias pasadas.
                self._missing[key] = self._missing.get(key, 0) + 1
                if self._missing[key] >= MISSING_TICKS:
                    self._missing.pop(key, None)
                    self._lost(info, member, exited=state == _EXITED)
            elif state == _WORKING:
                store.touch(info.id, member.name)
                self._idle_since.pop(key, None)
                if member.state == "blocked":
                    store.set_member_state(info.id, member.name, "working")
            elif state in (_BLOCKED, _ERROR):
                if self.recovery.inspect(info, member, error=state == _ERROR, stuck=True):
                    continue
                if member.state != "blocked":
                    store.set_member_state(info.id, member.name, "blocked")
            elif state == _IDLE:
                # Libre con trabajo pendiente: antes de despertarlo, ¿se le acabó algo?
                stuck = bool(store.mine(info.id, member.name)) and key not in self._unbriefed
                if self.recovery.inspect(info, member, error=False, stuck=stuck):
                    continue
                self._nudge(info, member)
        for text in self.progress.check(info.id, store.tasks(info.id)):
            self.announce(text)
        self._announce_issues(info.id)

    def _probe(self, address: str) -> str:
        """Estado de una terminal que no salió en el listado de agentes."""
        try:
            found = self.herdr.get_agent(address)
        except Exception:  # noqa: BLE001 - no existe o Herdr no responde
            return _MISSING
        state = _agent_state(found.get("state"))
        app = str(found.get("agent") or found.get("app") or "").strip().lower()
        if state == _UNKNOWN and app.removesuffix(".exe") in _SHELLS:
            return _EXITED  # la terminal sigue, pero el programa del agente salió
        return state

    def _lost(self, info: SwarmInfo, member: Member, *, exited: bool) -> None:
        """El agente ya no está: se reabre una vez; si vuelve a pasar, se avisa."""
        store = self.service.store
        key = (info.id, member.name)
        self._nudged.pop(key, None)
        # Solo si el programa salió solo. Si el usuario cerró la pestaña, se respeta.
        if exited and self._launches.get(key, 0) < MAX_LAUNCHES:
            logger.warning("Enjambre {}: {} se cerró; lo reabro.", info.id, member.name)
            store.set_address(info.id, member.name, "")  # la próxima pasada lo abre
            return
        self.recovery.closed(info, member, exited=exited)

    def _announce_issues(self, swarm: str) -> None:
        """Dice una vez cada aviso nuevo; si se resuelve y vuelve, lo repite."""
        found = self.service.status(swarm).found
        current = {issue.key for issue in found}
        said = self._announced.setdefault(swarm, set())
        said &= current
        for issue in found:
            if issue.key.startswith(("incident:", "paused")):
                continue  # la recuperación ya lo anunció con sus opciones
            if issue.key not in said:
                said.add(issue.key)
                self.announce(f"Enjambre: {issue.text}")

    # ------------------------------------------------------------ despertar
    def _nudge(self, info: SwarmInfo, member: Member) -> None:
        store = self.service.store
        key = (info.id, member.name)
        now = self._clock()
        unread, last_id, ready = store.pending(info.id, member.name)
        signature: tuple[Any, ...] = (last_id, ready)
        line = ""
        wait = RENUDGE_SECS
        with self._lock:
            unbriefed = key in self._unbriefed
        if unbriefed:
            # Libre y sin haber usado `ezs`: se le dice (o repite) cómo empezar.
            signature, wait = ("brief",), BRIEF_RETRY_SECS
            line = self.service.join_line(info.id, member.name)
        elif unread or ready:
            line = protocol.wake_line(unread, len(ready), command=self.service.command)
        else:
            mine = [task.id for task in store.mine(info.id, member.name)]
            since = self._idle_since.setdefault(key, now)
            if mine and now - since >= OPEN_TASK_SECS:
                signature = ("open", tuple(mine))
                line = protocol.open_task_line(mine, command=self.service.command)
        if not line:
            if not store.mine(info.id, member.name):
                self._idle_since.pop(key, None)
                if member.state == "blocked" and not self._has_failed(info.id, member.name):
                    store.set_member_state(info.id, member.name, "idle")
            self._nudged.pop(key, None)
            return
        last = self._nudged.get(key)
        count = 1
        if last is not None and last.signature == signature:
            if last.count >= MAX_NUDGES or now - last.at < wait:
                return
            count = last.count + 1
        try:
            sent = self.herdr.send_task(member.address, line)
        except Exception as exc:  # noqa: BLE001 - se reintenta en otra pasada
            logger.warning("No pude avisar a {}: {}", member.name, exc)
            return
        if sent.get("ok"):
            self._nudged[key] = _Nudge(signature, count, now)
            logger.info("Enjambre {}: aviso {} a {}.", info.id, count, member.name)

    def _reset_launches(self, swarm: str, member: str) -> None:
        """Miembro reabierto o reemplazado: empieza de cero sus contadores."""
        key = (swarm, member)
        with self._lock:
            self._launches.pop(key, None)
            self._unbriefed.discard(key)
        self._nudged.pop(key, None)
        self._missing.pop(key, None)
        self._idle_since.pop(key, None)
        self._wake.set()

    def _has_failed(self, swarm: str, member: str) -> bool:
        return any(
            task.state == "failed" and task.owner == member
            for task in self.service.store.tasks(swarm)
        )

    # --------------------------------------------------------------- lanzar
    def _launch(self, info: SwarmInfo, member: Member) -> None:
        key = (info.id, member.name)
        with self._lock:
            self._launching.add(key)
            self._launches[key] = self._launches.get(key, 0) + 1
        thread = threading.Thread(
            target=self._launch_work, args=(info, member), name="ezagent-swarm-launch", daemon=True
        )
        thread.start()

    def _write_seat(self, swarm: str, member: str) -> Path:
        """Guarda el token del miembro en su archivo; la terminal solo ve la ruta."""
        token = self.service.issue_token(swarm, member)
        self.seat_dir.mkdir(parents=True, exist_ok=True)
        seat = self.seat_dir / f"{swarm}-{member}.seat"
        tmp = seat.with_name(seat.name + ".tmp")
        tmp.write_text(token, encoding="utf-8")
        os.replace(tmp, seat)
        return seat

    def _launch_work(self, info: SwarmInfo, member: Member) -> None:
        key = (info.id, member.name)
        store = self.service.store
        try:
            seat = self._write_seat(info.id, member.name)
            kwargs: dict[str, Any] = {
                "cwd": info.cwd or None,
                "name": f"{member.name} · enjambre",
                "confirmed": True,
                "env": {"EZS_SEAT": str(seat)},
            }
            if self.command_dir:
                kwargs["path_prepend"] = [self.command_dir]
            extra = getattr(self.herdr, "swarm_args", None)
            args = extra(member.agent) if callable(extra) else []
            if args:
                kwargs["extra_args"] = args
            if member.model:
                kwargs["model"] = member.model
            if member.effort:
                kwargs["effort"] = member.effort
            # Se abre sin tarea: la línea de inicio se escribe después, con el
            # programa ya asentado, y se repite hasta que el agente use `ezs`.
            result = self.herdr.run_agent(member.agent, "", **kwargs)
            address = str(result.get("address") or result.get("terminal") or "").strip()
            if not result.get("ok") or not address:
                raise RuntimeError(str(result.get("error") or "Herdr no devolvió una terminal."))
            store.set_address(info.id, member.name, address)
            with self._lock:
                self._unbriefed.add(key)
            for warning in result.get("warnings") or []:
                logger.warning("Enjambre {}: {}: {}", info.id, member.name, warning)
            settled = self._clock() - BRIEF_RETRY_SECS + LAUNCH_SETTLE_SECS
            self._nudged[key] = _Nudge(("brief",), 0, settled)
            logger.info("Enjambre {}: {} abierto en {}.", info.id, member.name, address)
        except Exception as exc:  # noqa: BLE001 - se informa y el enjambre sigue
            logger.warning("Enjambre {}: no pude abrir a {}: {}", info.id, member.name, exc)
            store.set_member_state(info.id, member.name, "gone")
            self.announce(
                f"Enjambre: no pude abrir a {member.name} con {member.agent}. {str(exc)[:160]}"
            )
        finally:
            with self._lock:
                self._launching.discard(key)
            self._wake.set()


__all__ = ["Supervisor"]
