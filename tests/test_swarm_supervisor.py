"""Supervisor del enjambre, herramienta ``swarm`` y su integración con el runtime."""

from __future__ import annotations

from pathlib import Path

import pytest
from swarm_rig import MEMBERS, Rig, until

from ezagent.agent.runtime import BASE_TOOLS, select_tool_names
from ezagent.agent.tools_swarm import build_swarm_tools
from ezagent.swarm import cli
from ezagent.swarm.supervisor import (
    BRIEF_RETRY_SECS,
    LAUNCH_SETTLE_SECS,
    MISSING_TICKS,
    OPEN_TASK_SECS,
    RENUDGE_SECS,
)
from ezagent.voice.agent_session import command_line


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
    # Cada etapa se avisa al pasar, sin reabrir la escucha.
    assert rig.said == [
        "Enjambre: terminó la planificación. Ahora empieza el desarrollo.",
        "Enjambre: terminó el desarrollo. Ahora empieza la validación.",
        "Enjambre: devolvieron el desarrollo para corregir (ronda 1 de 3).",
        "Enjambre: terminó el desarrollo. Ahora empieza la validación.",
    ]
    rig.run("qa", "done", "Todo validado")
    rig.tick()
    assert len(rig.said) == 4  # la última etapa no se dice aparte: va en el resumen
    # El final reabre la escucha: el usuario decide qué hacer con el resultado.
    assert rig.asked == [
        "El enjambre terminó: 3 de 3 tareas hechas. Planificar: Plan listo. "
        "Desarrollar: Límite añadido. Validar: Todo validado. "
        "¿Quieres más detalle o que te muestre el resultado?"
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
    # Ya no tiene nada pendiente: el enjambre sigue y solo se avisa del avance.
    assert rig.said == ["Enjambre: terminó la planificación. Ahora empieza el desarrollo."]


def test_terminal_cerrada_y_lanzamiento_fallido(rig: Rig) -> None:
    rig.herdr.broken.add("claude")
    rig.start()
    rig.stack.supervisor.tick()
    until(lambda: any("no pude abrir a plan" in text for text in rig.said))
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
    assert len(second.asked) == 1
    assert "plan (claude, planificar) tiene su ventana cerrada" in second.asked[0]
    assert "¿Lo reabro, lo reemplazo con opencode deepseek-v4.1-flash" in second.asked[0]
    second.tick()
    assert len(second.asked) == 1  # se pregunta una sola vez


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
