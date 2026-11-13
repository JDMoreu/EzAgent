"""Recuperación del enjambre: detecta incidentes, pregunta al usuario y retoma.

Reglas (lo decide el código; el LLM solo transmite la elección del usuario):

- **Sin uso (quota)**: se pausa a ese miembro (no a todo el enjambre), se avisa
  a los demás en el tablero y se pregunta al usuario si esperar, reemplazarlo
  o pausar. Si se sabe la hora de restablecimiento se espera sola: a esa hora
  se le escribe al agente para que siga, aunque el usuario no conteste.
- **Sin contexto**: se reabre en una sesión nueva sin preguntar (una vez por
  miembro y tarea; si se repite, se pregunta). El avance queda en un archivo.
- **Ventana cerrada, error o sesión perdida**: se pregunta.
- **Vuelve solo** (trabaja de nuevo o usa ``ezs``): el incidente se cierra y se
  avisa.

Al reabrir o reemplazar, la pantalla del agente anterior se guarda en un
archivo de traspaso; el nuevo recibe solo la ruta (no gasta contexto si no la
necesita). Las tareas siguen a nombre del miembro, así que nadie más tiene que
cambiar nada: el siguiente ``ezs next`` del nuevo agente le entrega su papel,
el traspaso y su tarea en curso.
"""

from __future__ import annotations

import os
import threading
import time
from collections.abc import Callable
from datetime import datetime
from pathlib import Path
from typing import Any

from loguru import logger

from . import incidents as inc
from .models import SYSTEM, Incident, Member, SwarmError, SwarmInfo
from .service import SwarmService

INSPECT_SECS = 30.0
RESUME_GRACE_SECS = 60.0
RESUME_RETRY_SECS = 300.0
MAX_RESUMES = 3
MAX_RENEWALS = 1
SNAPSHOT_LINES = 160
SNAPSHOT_CHARS = 8000
IDLE_TAIL = 8
CHOICES = ("wait", "replace", "reopen", "release", "pause")


class Recovery:
    """Incidentes de los miembros: detección, avisos y decisiones."""

    def __init__(
        self,
        service: SwarmService,
        herdr: Any,
        *,
        handoff_dir: Path,
        announce: Callable[[str], None],
        ask: Callable[[str], None],
        reset_launches: Callable[[str, str], None] = lambda swarm, member: None,
        wall: Callable[[], float] = time.time,
        clock: Callable[[], float] = time.monotonic,
    ) -> None:
        self.service = service
        self.herdr = herdr
        self.handoff_dir = Path(handoff_dir)
        self.announce = announce
        self.ask = ask
        self.reset_launches = reset_launches
        self._wall = wall
        self._clock = clock
        self._lock = threading.Lock()
        self._inspected: dict[tuple[str, str], float] = {}
        self._resumes: dict[int, tuple[int, float]] = {}
        self._renewals: dict[tuple[str, str, tuple[int, ...]], int] = {}
        self._active: set[tuple[str, str]] = set()
        service.on_activity(self._note_activity)

    @property
    def store(self) -> Any:
        return self.service.store

    def _note_activity(self, swarm: str, member: str) -> None:
        # Hilo del servidor: solo se anota; la pasada del supervisor lo atiende.
        with self._lock:
            self._active.add((swarm, member))

    # ------------------------------------------------------------ cada pasada
    def hold(self, info: SwarmInfo, member: Member, working: bool) -> bool:
        """True si el miembro está bajo un incidente y el supervisor no debe tocarlo."""
        incident = self.store.member_incident(info.id, member.name)
        key = (info.id, member.name)
        with self._lock:
            active = key in self._active
            self._active.discard(key)
        if incident is None:
            return False
        if working or active:
            self._back(info, member, incident)
            return False
        if incident.state == "waiting" and incident.resets:
            self._maybe_resume(member, incident)
        return True

    def inspect(self, info: SwarmInfo, member: Member, *, error: bool, stuck: bool) -> bool:
        """Lee la pantalla de un agente parado y abre un incidente si reconoce la causa."""
        key = (info.id, member.name)
        now = self._clock()
        if now - self._inspected.get(key, -INSPECT_SECS) < INSPECT_SECS:
            return False
        if not (error or stuck):
            return False
        self._inspected[key] = now
        try:
            screen = str(self.herdr.read_agent(member.address, 40))
        except Exception as exc:  # noqa: BLE001 - leer es best-effort
            logger.debug("No pude leer a {}: {}", member.name, exc)
            return False
        if not error:  # libre: solo cuenta el final de la pantalla (menos falsos positivos)
            screen = "\n".join([ln for ln in screen.splitlines() if ln.strip()][-IDLE_TAIL:])
        signal = inc.classify(screen, datetime.fromtimestamp(self._wall()))
        if signal is None or signal.kind == "compacting":
            if error and signal is None:
                self.raise_incident(info, member, "error", "Herdr marca el agente con error.")
                return True
            return False
        if signal.kind == "context":
            self._renew(info, member, signal.detail)
        else:
            self.raise_incident(info, member, signal.kind, signal.detail, resets=signal.resets)
        return True

    def raise_incident(
        self, info: SwarmInfo, member: Member, kind: str, detail: str = "", *, resets: float = 0.0
    ) -> Incident:
        """Registra el incidente, avisa a los demás en el tablero y al usuario."""
        incident, new = self.store.open_incident(info.id, member.name, kind, detail, resets=resets)
        if not new:
            return incident
        self.store.set_member_state(info.id, member.name, "blocked")
        tasks = self._tasks(info.id, member.name)
        self.store.post(info.id, SYSTEM, inc.board_note(incident, member, tasks), kind="sys")
        alt = self._suggestion(info.id, member)
        logger.warning(
            "Enjambre {}: incidente I{} {} en {}.", info.id, incident.id, kind, member.name
        )
        if kind in inc.ASK_KINDS:
            self.ask(inc.question(incident, member, tasks, alt))
        else:
            self.announce(f"Enjambre: {inc.describe(incident, member, tasks)}")
        return incident

    # ----------------------------------------------------------- decisiones
    def resolve(
        self,
        incident_id: int,
        choice: str,
        *,
        agent: str = "",
        model: str = "",
        effort: str = "",
    ) -> str:
        """Aplica la decisión del usuario y devuelve qué se hizo, en una frase."""
        incident = self.store.incident(incident_id)
        if incident is None or incident.state == "resolved":
            raise SwarmError(f"No hay un incidente abierto I{incident_id}.")
        choice = choice.strip().lower()
        if choice not in CHOICES:
            raise SwarmError(f"Opción desconocida: {choice!r}. Usa: {', '.join(CHOICES)}.")
        member = self._member(incident.swarm, incident.member)
        if choice == "wait":
            self.store.set_incident(incident.id, "waiting", "wait")
            until = inc.when(incident.resets, incident.created)
            return f"Espero a {member.name}{' hasta ' + until if until else ''}; el resto sigue."
        if choice == "pause":
            self.store.set_incident(incident.id, "waiting", "pause")
            self.service.pause(incident.swarm)
            return "Enjambre en pausa. Dime «retoma el enjambre» para seguir."
        if choice == "release":
            return self._release(incident, member)
        if choice == "reopen":
            self._handoff(incident, member, member.agent, note="Se reabrió tu sesión")
            self.store.set_incident(incident.id, "resolved", "reopen")
            return f"Reabro a {member.name} con {member.agent}; retoma con su avance guardado."
        return self._replace(incident, member, agent, model, effort)

    def _replace(
        self, incident: Incident, member: Member, agent: str, model: str, effort: str
    ) -> str:
        alt = self._suggestion(incident.swarm, member)
        wanted = agent or (alt.agent if alt else "")
        key = self.herdr.resolve_agent(wanted) if wanted else None
        if key is None:
            raise SwarmError(f"Indica con qué agente lo reemplazo (no conozco {wanted!r}).")
        if not agent and alt is not None:
            model, effort = model or alt.model, effort or alt.effort
        old = member.agent
        self._handoff(incident, member, key, note=f"Reemplazas a {old}", model=model, effort=effort)
        self.store.set_incident(incident.id, "resolved", "replace")
        shown = f"{key}{' ' + model if model else ''}"
        return f"Reemplazo a {member.name}: {old} sale y entra {shown}, con el avance guardado."

    def _release(self, incident: Incident, member: Member) -> str:
        """Devuelve sus tareas al rol para que las tome otro miembro."""
        released = self.store.release_tasks(incident.swarm, member.name)
        self.store.set_member_state(incident.swarm, member.name, "gone")
        self.store.set_incident(incident.id, "resolved", "release")
        names = ", ".join(f"T{task}" for task in released) or "nada pendiente"
        return f"{member.name} sale del enjambre; su trabajo ({names}) queda para su rol."

    def _handoff(
        self,
        incident: Incident,
        member: Member,
        agent: str,
        *,
        note: str,
        model: str | None = None,
        effort: str | None = None,
    ) -> None:
        """Guarda el avance, cambia (o reabre) el agente y le deja la nota de traspaso."""
        path = self._snapshot(incident.swarm, member) if member.address else ""
        self.store.update_member(
            incident.swarm,
            member.name,
            agent=agent,
            model=member.model if model is None else model,
            effort=member.effort if effort is None else effort,
        )
        tasks = self._tasks(incident.swarm, member.name)
        text = f"{note}: retoma {', '.join(tasks) or 'tu trabajo'}."
        if path:
            text += (
                " El avance de la sesión anterior está en el archivo (léelo solo si lo necesitas)."
            )
        self.store.post(incident.swarm, SYSTEM, text, to=member.name, ref=path)
        self.reset_launches(incident.swarm, member.name)

    # -------------------------------------------------------------- internos
    def closed(self, info: SwarmInfo, member: Member, *, exited: bool) -> None:
        kind = "exited" if exited else "closed"
        detail = "El programa salió solo." if exited else "Su ventana ya no está abierta."
        self.raise_incident(info, member, kind, detail)

    def _renew(self, info: SwarmInfo, member: Member, detail: str) -> None:
        """Contexto lleno: sesión nueva con el avance guardado (una vez por tarea)."""
        tasks = tuple(task.id for task in self.store.mine(info.id, member.name))
        key = (info.id, member.name, tasks)
        if self._renewals.get(key, 0) >= MAX_RENEWALS:
            self.raise_incident(info, member, "context", detail)
            self.ask(
                f"Enjambre: {member.name} se quedó sin contexto otra vez en la misma tarea. "
                "¿Lo reemplazo con otro agente, reparto su trabajo o pauso el enjambre?"
            )
            return
        self._renewals[key] = self._renewals.get(key, 0) + 1
        incident, _ = self.store.open_incident(info.id, member.name, "context", detail)
        self._handoff(
            incident, member, member.agent, note="Tu sesión anterior se quedó sin contexto"
        )
        self.store.set_incident(incident.id, "resolved", "renew")
        self.announce(
            f"Enjambre: {member.name} se quedó sin contexto; lo reabro en una sesión nueva "
            "con su avance guardado."
        )

    def _back(self, info: SwarmInfo, member: Member, incident: Incident) -> None:
        self.store.set_incident(incident.id, "resolved", "back")
        self._resumes.pop(incident.id, None)
        if member.state == "blocked":
            self.store.set_member_state(info.id, member.name, "working")
        tasks = self._tasks(info.id, member.name)
        self.store.post(info.id, SYSTEM, f"{member.name} volvió.", kind="sys")
        self.announce(
            f"Enjambre: {member.name} ({member.agent}) volvió"
            + (f" y retoma {', '.join(tasks)}." if tasks else ".")
        )

    def _maybe_resume(self, member: Member, incident: Incident) -> None:
        """Pasada la hora de restablecimiento, le escribe al agente para que siga."""
        now = self._wall()
        if now < incident.resets + RESUME_GRACE_SECS or not member.address:
            return
        count, last = self._resumes.get(incident.id, (0, 0.0))
        if count >= MAX_RESUMES or (count and now - last < RESUME_RETRY_SECS):
            return
        line = (
            f"[EzAgent] Ya se restableció tu uso. Sigue con tu trabajo: {self.service.command} next"
        )
        try:
            sent = self.herdr.send_task(member.address, line)
        except Exception as exc:  # noqa: BLE001 - se reintenta luego
            logger.warning("No pude retomar a {}: {}", member.name, exc)
            return
        if sent.get("ok"):
            self._resumes[incident.id] = (count + 1, now)

    def _snapshot(self, swarm: str, member: Member) -> str:
        """Pantalla del agente saliente en un archivo; devuelve su ruta o ''."""
        try:
            screen = str(self.herdr.read_agent(member.address, SNAPSHOT_LINES))
        except Exception as exc:  # noqa: BLE001
            logger.debug("No pude guardar el avance de {}: {}", member.name, exc)
            return ""
        lines = [line.rstrip() for line in screen.splitlines() if line.strip()]
        body = "\n".join(lines)[-SNAPSHOT_CHARS:]
        if not body:
            return ""
        self.handoff_dir.mkdir(parents=True, exist_ok=True)
        stamp = datetime.fromtimestamp(self._wall()).strftime("%Y%m%d-%H%M%S")
        path = self.handoff_dir / f"{swarm}-{member.name}-{stamp}.md"
        header = f"# Avance de {member.name} ({member.agent})\n\nÚltima pantalla, sin editar:\n\n"
        tmp = path.with_name(path.name + ".tmp")
        tmp.write_text(header + "```text\n" + body + "\n```\n", encoding="utf-8")
        os.replace(tmp, path)
        return str(path)

    def _tasks(self, swarm: str, member: str) -> list[str]:
        return [f"T{task.id}" for task in self.store.mine(swarm, member)]

    def _member(self, swarm: str, name: str) -> Member:
        found = next((m for m in self.store.members(swarm) if m.name == name), None)
        if found is None:
            raise SwarmError(f"{name!r} ya no es miembro del enjambre {swarm}.")
        return found

    def _suggestion(self, swarm: str, failing: Member) -> Member | None:
        busy = {item.member for item in self.store.incidents(swarm)}
        return inc.suggest(self.store.members(swarm), failing, busy)


__all__ = ["CHOICES", "Recovery"]
