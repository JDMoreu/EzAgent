"""Supervisor del enjambre, herramienta ``swarm`` y su integración con el runtime."""

from __future__ import annotations

import time
from collections.abc import Callable
from pathlib import Path
from types import SimpleNamespace
from typing import Any

import pytest

from ezagent.agent.runtime import BASE_TOOLS, select_tool_names
from ezagent.agent.tools_swarm import build_swarm_tools
from ezagent.swarm import cli
from ezagent.swarm.stack import SwarmStack
from ezagent.swarm.supervisor import (
    BRIEF_RETRY_SECS,
    LAUNCH_SETTLE_SECS,
    MISSING_TICKS,
    OPEN_TASK_SECS,
    RENUDGE_SECS,
)
from ezagent.voice.agent_session import command_line

MEMBERS = [
    {"agent": "claude", "role": "plan", "model": "opus", "effort": "high"},
    {"agent": "open code", "role": "desarrollar", "model": "deepseek-v4.1-flash", "effort": "max"},
    {"agent": "codex", "role": "qa", "model": "gpt-6-sol", "effort": "high"},
]


class FakeHerdr:
    """Herdr de mentira: recuerda lanzamientos, avisos y el estado de cada terminal."""

    def __init__(self) -> None:
        self.cfg = SimpleNamespace(
            agents={"claude": {}, "opencode": {}, "codex": {}}, default_cwd="C:/proj"
        )
        self.states: dict[str, str | None] = {}
        self.apps: dict[str, str] = {}
        self.launched: list[tuple[str, str, dict[str, Any]]] = []
        self.sent: list[tuple[str, str]] = []
        self.broken: set[str] = set()
        self.saved: list[str] = []

    def resolve_agent(self, name: str) -> str | None:
        key = name.replace(" ", "").lower()
        return key if key in self.cfg.agents else None

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

    def send_task(self, address: str, text: str) -> dict[str, Any]:
        self.sent.append((address, text))
        return {"ok": True}

    def save_context(self, text: str) -> str:
        self.saved.append(text)
        return "C:/tmp/encargo.md"


class Rig:
    """Enjambre completo con Herdr falso y reloj manual."""

    def __init__(self, tmp_path: Path) -> None:
        self.herdr = FakeHerdr()
        self.said: list[str] = []
        self.now = 1000.0
        self.stack = SwarmStack(tmp_path / "swarm", self.herdr, announce=self.said.append)
        self.stack.command_dir = "C:/ezagent/bin"
        self.stack.supervisor.command_dir = "C:/ezagent/bin"
        self.stack.supervisor._clock = lambda: self.now
        self.stack.store._clock = lambda: self.now  # un solo reloj para todo
        self.stack.service._clock = lambda: self.now
        self.tool = build_swarm_tools(self.stack)[0]
        self.swarm = ""

    def call(self, **args: Any) -> Any:
        return self.tool.handler(args)

    def start(self) -> None:
        result = self.call(action="start", goal="API de login", members=MEMBERS)
        assert result.ok, result.error
        self.swarm = result.data["swarm"]

    def tick(self, launches: int | None = None) -> None:
        """Una pasada; si se espera un lanzamiento, aguarda a que termine."""
        self.stack.supervisor.tick()
        if launches is not None:
            _until(lambda: len(self.herdr.launched) == launches)
            _until(lambda: not self.stack.supervisor._launching)

    def seat(self, member: str, launches: int) -> str:
        """Abre al miembro y simula que leyó su línea de inicio y ejecutó ``next``."""
        self.tick(launches=launches)
        self.now += LAUNCH_SETTLE_SECS + 1
        self.tick()
        address = self.address(member)
        assert (
            self.herdr.sent[-1][0] == address
            and "Ejecuta ahora: ezs next" in self.herdr.sent[-1][1]
        )
        self.herdr.states[address] = "working"
        return self.run(member, "next")

    def run(self, member: str, *argv: str, ok: bool = True) -> str:
        seat = self.stack.base_dir / "seats" / f"{self.swarm}-{member}.seat"
        reply = self.stack.service.handle(seat.read_text(encoding="utf-8"), list(argv))
        assert reply.ok is ok, reply.text
        return reply.text

    def address(self, member: str) -> str:
        found = {m.name: m for m in self.stack.store.members(self.swarm)}
        return found[member].address

    def idle(self, member: str) -> None:
        self.herdr.states[self.address(member)] = "idle"


def _until(condition: Callable[[], bool], timeout: float = 5.0) -> None:
    deadline = time.monotonic() + timeout
    while not condition():
        assert time.monotonic() < deadline, "la condición no se cumplió a tiempo"
        time.sleep(0.01)


@pytest.fixture
def rig(tmp_path: Path) -> Rig:
    return Rig(tmp_path)


def test_abre_a_cada_miembro_cuando_le_toca_y_con_una_sola_linea(rig: Rig) -> None:
    rig.start()
    rig.tick(launches=1)
    agent, task, kwargs = rig.herdr.launched[0]
    assert agent == "claude" and kwargs["model"] == "opus" and kwargs["effort"] == "high"
    assert task == ""  # nada de pegar instrucciones largas al abrir
    assert kwargs["path_prepend"] == ["C:/ezagent/bin"] and kwargs["cwd"] == "C:/proj"
    seat = Path(kwargs["env"]["EZS_SEAT"])
    assert seat.is_file() and "plan" in seat.name
    rig.tick()
    assert rig.herdr.sent == []  # recién abierto: se le da un momento para asentarse
    rig.now += LAUNCH_SETTLE_SECS + 1
    rig.tick()
    assert rig.herdr.sent == [
        (
            "t1",
            f"[EzAgent] Eres «plan» (planificar) en el enjambre {rig.swarm}. Ejecuta ahora: ezs next",
        )
    ]
    assert len(rig.herdr.launched) == 1  # dev y qa no se abren todavía
    first = rig.run("plan", "next")
    assert first.startswith("Eres «plan» (rol: planificar)") and "Tu tarea: T" in first
    rig.run("plan", "done", "Plan en PLAN.md", "--ref", "PLAN.md")
    rig.tick(launches=2)
    assert rig.herdr.launched[1][0] == "opencode" and rig.address("qa") == ""


def test_repite_la_linea_de_inicio_hasta_que_el_agente_usa_ezs(rig: Rig) -> None:
    rig.start()
    rig.tick(launches=1)
    rig.now += LAUNCH_SETTLE_SECS + 1
    rig.tick()
    rig.tick()
    assert len(rig.herdr.sent) == 1  # no insiste en cada pasada
    rig.now += BRIEF_RETRY_SECS + 1
    rig.tick()  # sigue libre y sin usar ezs: el pegado pudo perderse
    assert len(rig.herdr.sent) == 2 and rig.herdr.sent[1] == rig.herdr.sent[0]
    for _ in range(5):
        rig.now += BRIEF_RETRY_SECS + 1
        rig.tick()
    assert len(rig.herdr.sent) == 3  # tope de intentos; después lo señala la salud
    rig.now += 300
    rig.tick()
    assert any("lista y plan no la toma" in text for text in rig.said)


def test_despierta_solo_por_lo_suyo_y_recuerda_la_tarea_abierta(rig: Rig) -> None:
    rig.start()
    rig.seat("plan", launches=1)
    rig.run("plan", "done", "Plan listo")
    rig.idle("plan")
    rig.seat("dev", launches=2)
    sent = len(rig.herdr.sent)
    rig.tick()
    assert len(rig.herdr.sent) == sent  # a plan no se le despierta por el aviso general de nadie
    rig.idle("dev")
    rig.run("plan", "say", "dev", "Usa SQLite")
    rig.tick()
    assert rig.herdr.sent[-1] == (
        rig.address("dev"),
        "[EzAgent] Tienes 1 mensaje nuevo en el enjambre. Ejecuta: ezs next",
    )
    rig.tick()
    assert len(rig.herdr.sent) == sent + 1
    rig.now += RENUDGE_SECS + 1
    rig.tick()
    assert len(rig.herdr.sent) == sent + 2  # sigue sin leerlo: un recordatorio espaciado
    rig.run("dev", "inbox")
    rig.tick()  # libre, sin pendientes y con la tarea abierta: empieza a contar
    rig.now += OPEN_TASK_SECS + 1
    rig.tick()
    assert len(rig.herdr.sent) == sent + 3
    assert "sigue abierta en el enjambre" in rig.herdr.sent[-1][1]
    rig.run("dev", "done", "API lista")
    rig.tick(launches=3)
    assert rig.herdr.launched[2][0] == "codex"


def test_correccion_avisa_al_desarrollador_y_el_final_se_anuncia(rig: Rig) -> None:
    rig.start()
    rig.seat("plan", launches=1)
    rig.run("plan", "done", "Plan listo")
    rig.idle("plan")
    rig.seat("dev", launches=2)
    build = f"T{rig.stack.store.mine(rig.swarm, 'dev')[0].id}"
    rig.run("dev", "done", "API lista")
    rig.idle("dev")
    rig.seat("qa", launches=3)
    sent = len(rig.herdr.sent)
    assert "reabierta (ronda 1/3)" in rig.run("qa", "fix", build, "Falta limitar intentos")
    rig.idle("qa")
    rig.tick()
    assert rig.herdr.sent[sent:] == [
        (
            rig.address("dev"),
            "[EzAgent] Tienes 1 mensaje nuevo y 1 tarea lista en el enjambre. Ejecuta: ezs next",
        )
    ]
    assert "corregir (1/3): Falta limitar intentos" in rig.run("dev", "next")
    rig.run("dev", "done", "Límite añadido")
    rig.tick()
    assert rig.herdr.sent[-1] == (
        rig.address("qa"),
        "[EzAgent] Tienes 1 tarea lista en el enjambre. Ejecuta: ezs next",
    )
    rig.run("qa", "next")
    rig.run("qa", "done", "Todo validado")
    assert rig.said == []
    rig.tick()
    assert rig.said == [
        "El enjambre terminó: 3 de 3 tareas hechas. Planificar: Plan listo. "
        "Desarrollar: Límite añadido. Validar: Todo validado."
    ]


def test_avisa_una_vez_de_lo_que_necesita_atencion(rig: Rig) -> None:
    rig.start()
    rig.seat("plan", launches=1)
    task = f"T{rig.stack.store.mine(rig.swarm, 'plan')[0].id}"
    rig.run("plan", "fail", "No hay repositorio")
    rig.idle("plan")
    rig.tick()
    rig.tick()
    assert rig.said.count(f"Enjambre: {task} falló: No hay repositorio") == 1
    assert "Enjambre: plan está bloqueado." in rig.said
    status = rig.call(action="status")
    assert status.ok and status.data["spoken"].startswith("Van 0 de 3 tareas. Atención:")
    assert status.data["board"].startswith(f"Enjambre {rig.swarm} · activo")


def test_bloqueado_sin_trabajo_no_molesta(rig: Rig) -> None:
    rig.start()
    rig.seat("plan", launches=1)
    rig.run("plan", "done", "Plan listo")
    rig.herdr.states[rig.address("plan")] = "error"  # p. ej. agotó su límite de uso
    rig.tick(launches=2)
    rig.tick()
    assert rig.said == []  # ya no tiene nada pendiente: el enjambre sigue


def test_terminal_cerrada_y_lanzamiento_fallido(rig: Rig) -> None:
    rig.herdr.broken.add("claude")
    rig.start()
    rig.stack.supervisor.tick()
    _until(lambda: any("no pude abrir a plan" in text for text in rig.said))
    assert rig.stack.store.members(rig.swarm)[0].state == "gone"
    second = Rig(rig.stack.base_dir.parent / "otro")
    second.start()
    second.tick(launches=1)
    second.herdr.states["t1"] = None  # Herdr aún no reconoce al agente recién abierto
    for _ in range(MISSING_TICKS + 2):
        second.tick()
    assert second.said == [] and second.address("plan") == "t1"
    second.herdr.apps["t1"] = "powershell.exe"  # el programa salió (p. ej. se actualizó)
    for _ in range(MISSING_TICKS - 1):
        second.tick()
    assert second.address("plan") == "t1"
    second.tick()
    assert second.address("plan") == ""  # se reabre solo, una vez
    second.tick(launches=2)
    assert second.address("plan") == "t2" and second.said == []
    second.herdr.states.clear()  # ahora el usuario cerró la pestaña
    for _ in range(MISSING_TICKS):
        second.tick()
    assert any("plan (claude) ya no tiene su terminal abierta" in text for text in second.said)
    assert second.stack.store.members(second.swarm)[0].state == "gone"


def test_herramienta_valida_y_entrega_recados(rig: Rig) -> None:
    assert "No hay ningún enjambre" in rig.call(action="status").error
    assert "Falta «goal»" in rig.call(action="start", members=MEMBERS).error
    assert "Indica los agentes" in rig.call(action="start", goal="x").error
    bad = rig.call(action="start", goal="x", members=[{"agent": "gemini", "role": "dev"}])
    assert "Agente desconocido: 'gemini'" in bad.error
    twins = rig.call(
        action="start",
        goal="x" * 900,
        members=[{"agent": "opencode", "role": "dev"}, {"agent": "codex", "role": "dev"}],
    )
    assert twins.ok and twins.data["members"] == ["dev", "dev2"]
    info = rig.stack.store.get(twins.data["swarm"])
    assert info is not None and info.goal.endswith("C:/tmp/encargo.md]") and rig.herdr.saved
    rig.swarm = twins.data["swarm"]
    rig.tick(launches=2)  # dos desarrolladores: los dos pueden tomar la tarea
    assert rig.call(action="tell", text="Usen SQLite", to="dev2").ok
    assert "ez→dev2 info: Usen SQLite" in rig.run("dev2", "inbox")
    assert rig.call(action="tell", text="Prioricen las pruebas").ok
    assert rig.stack.service.pending(rig.swarm, "dev")[0] == 1  # un recado del usuario sí despierta
    assert rig.call(action="stop").ok
    assert rig.run("dev", "next") == "El usuario detuvo este enjambre. Termina tu turno."
    assert rig.tool.describe({"action": "status"}).level == 0
    assert rig.tool.describe({"action": "start"}).level == 1
    assert build_swarm_tools(None) == []


def test_la_herramienta_solo_se_ofrece_cuando_aplica(rig: Rig) -> None:
    assert "swarm" not in BASE_TOOLS
    assert "swarm" not in select_tool_names("abre claude y dile que revise el README")
    assert "swarm" not in select_tool_names("¿qué hora es?")
    for text in (
        "arma un enjambre con claude y codex",
        "quiero que trabajen en modo cooperativo",
        "que Claude planifique, OpenCode desarrolle y Codex lo valide",
    ):
        assert "swarm" in select_tool_names(text), text
    assert "swarm" in select_tool_names("¿cómo va?", swarm_active=True)
    assert rig.stack.prompt_block() == "" and not rig.stack.active()
    rig.start()
    block = rig.stack.prompt_block()
    assert block.startswith("<swarms>\n- enjambre ") and "Van 0 de 3 tareas." in block


def test_cliente_usa_el_asiento_y_la_ruta_del_lanzamiento(
    rig: Rig, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    rig.start()
    rig.tick(launches=1)
    seat = rig.herdr.launched[0][2]["env"]["EZS_SEAT"]
    rig.stack.server.start()
    try:
        monkeypatch.delenv("EZS_TOKEN", raising=False)
        monkeypatch.delenv("EZS_ENDPOINT", raising=False)
        monkeypatch.setenv("EZS_SEAT", seat)
        assert cli.main(["next"]) == 0 and "Tu tarea: T" in capsys.readouterr().out
    finally:
        rig.stack.server.stop()
    line = command_line(["claude", "--model", "opus"], {"EZS_SEAT": seat}, ["C:/ezagent/bin"])
    assert line.startswith(f"$env:EZS_SEAT='{seat}'; $env:PATH='C:/ezagent/bin;'+$env:PATH; ")
    assert line.endswith("claude --model opus")
    assert command_line(["claude"]) == "claude"
