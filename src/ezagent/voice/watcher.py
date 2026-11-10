"""Vigilante de agentes de Herdr: polling de `hc ls --json`.

`hc events` puede perder el cambio de estado de un agente que termina y se
queda quieto (o cuyo terminal desaparece). Un polling es más simple y fiable
para el caso de uso: avisar cuando un agente concreto termina.

El bucle de polling solo publica eventos en una cola; un hilo propio los
entrega para que un callback lento (leer salida, llamar al modelo) no retrase
la detección del siguiente cambio. Una consulta fallida nunca se interpreta
como "no hay terminales": se conserva el último snapshot y se reintenta con
backoff.
"""

from __future__ import annotations

import json
import queue
import subprocess
import threading
from collections.abc import Callable
from typing import Any

from loguru import logger

from .config import VoiceConfig

EventCallback = Callable[[str, str, str, str], None]
HealthCallback = Callable[[bool], None]

POLL_S = 1.0
IDLE_POLL_S = 5.0
BACKOFF_S = (1.0, 2.0, 5.0)
STOP_JOIN_S = 5.0


class Watcher:
    def __init__(
        self,
        cfg: VoiceConfig,
        on_event: EventCallback,
        should_announce: Callable[[str], bool],
        *,
        on_health: HealthCallback | None = None,
        has_targets: Callable[[], bool] | None = None,
    ):
        self.cfg = cfg
        self.on_event = on_event
        self.should_announce = should_announce
        self.on_health = on_health
        self.has_targets = has_targets
        self.healthy = True
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None
        self._delivery: threading.Thread | None = None
        self._events: queue.Queue[tuple[str, str, str, str] | None] = queue.Queue()
        self._prev: dict[str, tuple[str, str]] = {}
        self._primed = False

    def start(self) -> None:
        if self._delivery is None or not self._delivery.is_alive():
            self._delivery = threading.Thread(
                target=self._deliver, name="ez-watch-events", daemon=True
            )
            self._delivery.start()
        if self._thread and self._thread.is_alive():
            return
        self._thread = threading.Thread(target=self._loop, name="ez-watch-poll", daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()
        if self._delivery is not None:
            self._events.put(None)  # despierta al hilo de entrega
        for thread in (self._thread, self._delivery):
            if thread is not None and thread.is_alive():
                thread.join(timeout=STOP_JOIN_S)

    def _deliver(self) -> None:
        while True:
            event = self._events.get()
            if event is None:
                return
            try:
                self.on_event(*event)
            except Exception as exc:  # noqa: BLE001 - un aviso no debe matar el hilo
                logger.debug("Watcher: el callback falló: {}", type(exc).__name__)

    def _loop(self) -> None:
        failures = 0
        while not self._stop.is_set():
            if self._poll_once():
                failures = 0
                self._set_health(True)
                delay = POLL_S if self._wants_fast_poll() else IDLE_POLL_S
            else:
                failures += 1
                self._set_health(False)
                delay = BACKOFF_S[min(failures, len(BACKOFF_S)) - 1]
            if self._stop.wait(delay):
                return

    def _wants_fast_poll(self) -> bool:
        if self.has_targets is None:
            return True
        try:
            return bool(self.has_targets())
        except Exception:  # noqa: BLE001 - ante la duda, sondeo normal
            return True

    def _set_health(self, healthy: bool) -> None:
        if healthy == self.healthy:
            return
        self.healthy = healthy
        if self.on_health is None:
            return
        try:
            self.on_health(healthy)
        except Exception as exc:  # noqa: BLE001
            logger.debug("Watcher: on_health falló: {}", type(exc).__name__)

    def _poll_once(self) -> bool:
        """Consulta `hc ls`; False si el resultado no es fiable."""
        try:
            result = subprocess.run(
                [self.cfg.hc_path, "ls", "--json"],
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=10,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
        except Exception:  # noqa: BLE001 - hc ausente, timeout...
            return False
        if result.returncode != 0:
            return False
        output = (result.stdout or "").strip()
        if not output:
            return False
        try:
            data: Any = json.loads(output)
        except ValueError:
            return False
        # DECISION: un objeto sin "terminals" es una respuesta incompleta, no
        # una lista vacía; comparar contra [] anunciaría finales falsos.
        terminals = data.get("terminals") if isinstance(data, dict) else None
        if not isinstance(terminals, list):
            return False
        self._compare(terminals)
        return True

    def _compare(self, terminals: list[dict]) -> None:
        current: dict[str, tuple[str, str]] = {}
        for terminal in terminals:
            status = terminal.get("status")
            if isinstance(status, dict):
                state = str(status.get("state") or "idle")
                agent = str(status.get("app") or terminal.get("agent") or "agente")
            elif isinstance(status, str):
                state = status
                agent = str(terminal.get("agent") or "agente")
            else:
                continue
            address = str(terminal.get("address") or "")
            if not address:
                continue
            current[address] = (state, agent)

        if not self._primed:
            self._prev = dict(current)
            self._primed = True
            return

        # Desaparecer no es terminar: el desenlace es desconocido ("lost"). Solo
        # una transición observada working -> done/idle se anuncia como done.
        for address, (prev_state, prev_agent) in list(self._prev.items()):
            if address in current:
                continue
            if prev_state not in ("done", "idle") and self.should_announce(address):
                self._events.put((address, prev_agent, "lost", "lost"))
            self._prev.pop(address, None)

        for address, (state, agent) in current.items():
            previous = self._prev.get(address)
            self._prev[address] = (state, agent)
            if previous is None:
                # Terminal nuevo: se registra sin avisar.
                continue
            if previous[0] == state:
                continue
            if not self.should_announce(address):
                continue
            if state == "blocked":
                self._events.put((address, agent, state, "blocked"))
            elif state in ("done", "idle") and previous[0] == "working":
                self._events.put((address, agent, state, "done"))
            elif state == "error":
                self._events.put((address, agent, state, "error"))
