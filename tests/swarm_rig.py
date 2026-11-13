"""Banco de pruebas del enjambre: Herdr simulado y relojes manuales."""

from __future__ import annotations

import time
from collections.abc import Callable
from pathlib import Path
from types import SimpleNamespace
from typing import Any

from ezagent.agent.tools_swarm import build_swarm_tools
from ezagent.swarm.stack import SwarmStack
from ezagent.swarm.supervisor import LAUNCH_SETTLE_SECS

MEMBERS = [
    {"agent": "claude", "role": "plan", "model": "opus", "effort": "high"},
    {"agent": "open code", "role": "desarrollar", "model": "deepseek-v4.1-flash", "effort": "max"},
    {"agent": "codex", "role": "qa", "model": "gpt-6-sol", "effort": "high"},
]
SWARM_ARGS = {"codex": ["--yolo"], "claude": ["--allowedTools", "Bash(ezs *)"]}


class FakeHerdr:
    """Herdr de mentira: recuerda lanzamientos, avisos, pantallas y estados."""

    def __init__(self) -> None:
        self.cfg = SimpleNamespace(
            agents={"claude": {}, "opencode": {}, "codex": {}}, default_cwd="C:/proj"
        )
        self.states: dict[str, str | None] = {}
        self.apps: dict[str, str] = {}
        self.screens: dict[str, str] = {}
        self.launched: list[tuple[str, str, dict[str, Any]]] = []
        self.sent: list[tuple[str, str]] = []
        self.broken: set[str] = set()
        self.saved: list[str] = []
        self.reads = 0

    def resolve_agent(self, name: str) -> str | None:
        key = name.replace(" ", "").lower()
        return key if key in self.cfg.agents else None

    def swarm_args(self, agent: str) -> list[str]:
        return list(SWARM_ARGS.get(self.resolve_agent(agent) or "", []))

    def run_agent(self, agent: str, task: str = "", **kwargs: Any) -> dict[str, Any]:
        if agent in self.broken:
            return {"ok": False, "error": "el programa no arrancó"}
        address = f"t{len(self.launched) + 1}"
        self.launched.append((agent, task, kwargs))
        self.states[address] = "idle"
        self.apps[address] = agent
        return {"ok": True, "address": address, "task_sent": False}

    def list_agents(self, timeout: float = 10) -> list[dict[str, Any]]:
        # Como el Herdr real: una terminal sin agente reconocido no aparece.
        return [{"address": a, "state": s} for a, s in self.states.items() if s is not None]

    def get_agent(self, address: str) -> dict[str, Any]:
        if address not in self.states:
            raise RuntimeError("no existe")
        return {"address": address, "state": self.states[address], "agent": self.apps[address]}

    def read_agent(self, address: str, lines: int | None = None) -> str:
        if address not in self.states:
            raise RuntimeError("no existe")
        self.reads += 1
        return self.screens.get(address, "")

    def send_task(self, address: str, text: str) -> dict[str, Any]:
        self.sent.append((address, text))
        return {"ok": True}

    def save_context(self, text: str) -> str:
        self.saved.append(text)
        return "C:/tmp/encargo.md"


class Rig:
    """Enjambre completo con Herdr falso; ``now`` es el reloj de todo."""

    def __init__(self, tmp_path: Path) -> None:
        self.herdr = FakeHerdr()
        self.said: list[str] = []
        self.asked: list[str] = []
        self.now = 1_800_000_000.0  # época: sirve también como hora de pared
        self.stack = SwarmStack(
            tmp_path / "swarm", self.herdr, announce=self.said.append, ask=self.asked.append
        )
        self.stack.command_dir = "C:/ezagent/bin"
        sup = self.stack.supervisor
        sup.command_dir = "C:/ezagent/bin"
        sup._clock = lambda: self.now
        sup.recovery._clock = lambda: self.now
        sup.recovery._wall = lambda: self.now
        self.stack.store._clock = lambda: self.now
        self.stack.service._clock = lambda: self.now
        self.tool = build_swarm_tools(self.stack)[0]
        self.swarm = ""

    def call(self, **args: Any) -> Any:
        return self.tool.handler(args)

    def start(self, members: list[dict[str, str]] | None = None) -> None:
        result = self.call(action="start", goal="API de login", members=members or MEMBERS)
        assert result.ok, result.error
        self.swarm = result.data["swarm"]

    def tick(self, launches: int | None = None) -> None:
        """Una pasada; si se espera un lanzamiento, aguarda a que termine."""
        self.stack.supervisor.tick()
        if launches is not None:
            until(lambda: len(self.herdr.launched) == launches)
            until(lambda: not self.stack.supervisor._launching)

    def seat(self, member: str, launches: int) -> str:
        """Abre al miembro y simula que leyó su línea de inicio y ejecutó ``next``."""
        self.tick(launches=launches)
        self.now += LAUNCH_SETTLE_SECS + 1
        self.tick()
        address = self.address(member)
        lines = [text for to, text in self.herdr.sent if to == address]
        assert lines and "Ejecuta ahora: ezs next" in lines[-1]
        self.herdr.states[address] = "working"
        return self.run(member, "next")

    def run(self, member: str, *argv: str, ok: bool = True) -> str:
        seat = self.stack.base_dir / "seats" / f"{self.swarm}-{member}.seat"
        reply = self.stack.service.handle(seat.read_text(encoding="utf-8"), list(argv))
        assert reply.ok is ok, reply.text
        return reply.text

    def member(self, name: str) -> Any:
        return {m.name: m for m in self.stack.store.members(self.swarm)}[name]

    def address(self, member: str) -> str:
        return self.member(member).address

    def idle(self, member: str) -> None:
        self.herdr.states[self.address(member)] = "idle"


def until(condition: Callable[[], bool], timeout: float = 5.0) -> None:
    deadline = time.monotonic() + timeout
    while not condition():
        assert time.monotonic() < deadline, "la condición no se cumplió a tiempo"
        time.sleep(0.01)
