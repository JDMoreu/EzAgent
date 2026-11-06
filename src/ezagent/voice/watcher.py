"""Vigilante de agentes de Herdr: polling de `hc ls --json`.

`hc events` puede perder el cambio de estado de un agente que termina y se
queda quieto (o cuyo terminal desaparece). Un polling de 1 s es más simple y
fiable para el caso de uso: avisar cuando un agente concreto termina.
"""

from __future__ import annotations

import json
import subprocess
import threading
from collections.abc import Callable

from .config import VoiceConfig

EventCallback = Callable[[str, str, str, str], None]


class Watcher:
    def __init__(
        self,
        cfg: VoiceConfig,
        on_event: EventCallback,
        should_announce: Callable[[str], bool],
    ):
        self.cfg = cfg
        self.on_event = on_event
        self.should_announce = should_announce
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None
        self._prev: dict[str, tuple[str, str]] = {}
        self._primed = False

    def start(self) -> None:
        if self._thread and self._thread.is_alive():
            return
        self._thread = threading.Thread(target=self._loop, daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()

    def _loop(self) -> None:
        while not self._stop.is_set():
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
                data = json.loads((result.stdout or "{}").strip() or "{}")
                self._compare(data.get("terminals", []))
            except Exception:
                pass
            if self._stop.wait(1.0):
                return

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

        # Un terminal que desaparece es un final (aunque no hayamos visto el
        # estado "done" en un snapshot intermedio).
        for address, (prev_state, prev_agent) in list(self._prev.items()):
            if address in current:
                continue
            if prev_state not in ("done", "idle") and self.should_announce(address):
                self.on_event(address, prev_agent, "done", "done")
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
                self.on_event(address, agent, state, "blocked")
            elif state in ("done", "idle") and previous[0] == "working":
                self.on_event(address, agent, state, "done")
            elif state == "error":
                self.on_event(address, agent, state, "error")
