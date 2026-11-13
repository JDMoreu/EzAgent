"""Incidentes del enjambre: sin uso, sin contexto, ventana cerrada y cómo se retoma."""

from __future__ import annotations

import time
from datetime import datetime, timedelta
from pathlib import Path
from types import SimpleNamespace

import pytest
from swarm_rig import Rig

from ezagent.swarm.incidents import classify, parse_reset, when
from ezagent.swarm.recovery import INSPECT_SECS, RESUME_GRACE_SECS
from ezagent.swarm.supervisor import MISSING_TICKS
from ezagent.voice.app import VoiceApp

NOW = datetime(2026, 11, 12, 18, 45)
CLAUDE_QUOTA = (
    "❯ [EzAgent] Tienes 1 mensaje nuevo en el enjambre. Ejecuta: ezs next\n"
    "  ⎿  You've hit your 5-hour limit · resets {clock} (America/Bogota)\n"
    "● Usage limit reached · continuing automatically at {clock} · esc to cancel"
)


def _clock(moment: datetime) -> str:
    return f"{moment.hour % 12 or 12}:{moment.minute:02d}{'am' if moment.hour < 12 else 'pm'}"


@pytest.fixture
def rig(tmp_path: Path) -> Rig:
    rig = Rig(tmp_path)
    rig.start()
    rig.seat("plan", launches=1)  # plan (claude) trabaja en T1
    return rig


def _later(rig: Rig, seconds: float) -> None:
    rig.now += seconds


# ------------------------------------------------------------- detección
@pytest.mark.parametrize(
    ("screen", "kind"),
    [
        (CLAUDE_QUOTA.format(clock="7:10pm"), "quota"),
        ("■ You've hit your usage limit. Upgrade to Pro, or try again at 9:30 PM.", "quota"),
        ("Error: 429 Too Many Requests", "quota"),
        ("Insufficient balance. Recarga tu cuenta.", "quota"),
        ("Prompt is too long", "context"),
        ("Error: this request exceeds the context window of the model", "context"),
        ("✻ Compacting conversation… (esc to interrupt)", "compacting"),
        ("Invalid API key · Please run /login", "auth"),
        ("● T2 hecha: 16 dependencias. Todas las pruebas pasan.", None),
    ],
)
def test_reconoce_la_causa_en_la_pantalla(screen: str, kind: str | None) -> None:
    found = classify(screen, NOW)
    assert (found.kind if found else None) == kind


def test_hora_de_restablecimiento() -> None:
    at = datetime(2026, 11, 12, 19, 10).timestamp()
    assert classify(CLAUDE_QUOTA.format(clock="7:10pm"), NOW).resets == at  # type: ignore[union-attr]
    assert parse_reset("try again at 9:30 PM", NOW) == datetime(2026, 11, 12, 21, 30).timestamp()
    assert parse_reset("vuelve a las 19:10", NOW) == at
    assert parse_reset("resets 8:00am", NOW) == datetime(2026, 11, 13, 8, 0).timestamp()
    assert (
        parse_reset("Try again in 2h 15m", NOW)
        == (NOW + timedelta(hours=2, minutes=15)).timestamp()
    )
    assert parse_reset("Usage limit reached", NOW) == 0.0
    assert when(at, NOW.timestamp()) == "las 7:10 p. m."
    assert when(parse_reset("resets 8:00am", NOW), NOW.timestamp()) == "mañana a las 8:00 a. m."


# ------------------------------------------------------------ sin uso
def test_sin_uso_con_hora_espera_avisa_y_retoma_solo(rig: Rig) -> None:
    reset = (datetime.fromtimestamp(rig.now) + timedelta(hours=1)).replace(second=0, microsecond=0)
    rig.herdr.screens["t1"] = CLAUDE_QUOTA.format(clock=_clock(reset))
    rig.herdr.states["t1"] = "error"
    rig.tick()
    assert len(rig.asked) == 1
    question = rig.asked[0]
    assert "plan (claude, planificar) se quedó sin uso hasta las" in question
    assert "Su trabajo pendiente: T1" in question
    assert "lo reemplazo con opencode deepseek-v4.1-flash o pauso el enjambre?" in question
    incident = rig.stack.store.incidents(rig.swarm)[0]
    assert incident.kind == "quota" and incident.state == "waiting"
    assert incident.resets == reset.timestamp()
    assert "sin uso hasta" in rig.stack.store.inbox(rig.swarm, "qa")[-1].body  # los demás lo ven
    assert rig.stack.service.pending(rig.swarm, "qa") == (0, 0)  # pero no los despierta
    status = rig.call(action="status").data["spoken"]
    assert status.startswith(
        "Van 0 de 3 tareas. Atención: plan (claude, planificar) se quedó sin uso"
    )
    assert "incidente I1 · plan · quota · esperando" in rig.stack.prompt_block()
    sent = len(rig.herdr.sent)
    for _ in range(3):
        _later(rig, 600)
        rig.tick()  # mientras espera, nadie lo molesta ni se repite el aviso
    assert len(rig.herdr.sent) == sent and rig.said == [] and len(rig.asked) == 1
    rig.now = reset.timestamp() + RESUME_GRACE_SECS + 1
    rig.tick()
    assert rig.herdr.sent[-1] == (
        "t1",
        "[EzAgent] Ya se restableció tu uso. Sigue con tu trabajo: ezs next",
    )
    rig.herdr.states["t1"] = "working"
    rig.tick()
    assert rig.said == ["Enjambre: plan (claude) volvió y retoma T1."]
    assert rig.stack.store.incidents(rig.swarm) == []


def test_sin_uso_sin_hora_pregunta_y_reemplaza_con_el_sugerido(rig: Rig) -> None:
    rig.herdr.screens["t1"] = "trabajando en el plan…\nUsage limit reached. Upgrade your plan."
    rig.idle("plan")
    rig.tick()
    assert len(rig.asked) == 1 and "¿Espero a que vuelva, lo reemplazo con opencode" in rig.asked[0]
    result = rig.call(action="resolve", choice="replace")  # sin agente: el sugerido
    assert result.ok, result.error
    assert result.data["message"].startswith("Reemplazo a plan: claude sale y entra opencode")
    member = rig.member("plan")
    assert (member.agent, member.model, member.effort, member.address) == (
        "opencode",
        "deepseek-v4.1-flash",
        "max",
        "",
    )
    rig.tick(launches=2)
    agent, _, kwargs = rig.herdr.launched[1]
    assert agent == "opencode" and kwargs["model"] == "deepseek-v4.1-flash"
    first = rig.run("plan", "next")  # el nuevo agente recibe su papel, el traspaso y su tarea
    assert first.startswith("Eres «plan»") and "Reemplazas a claude: retoma T1." in first
    note = next(line for line in first.splitlines() if "Reemplazas a claude" in line)
    ref = note.rsplit("[", 1)[1].rstrip("]")
    assert Path(ref).is_file() and "Usage limit reached" in Path(ref).read_text(encoding="utf-8")
    assert "En curso: T1" in first
    assert rig.stack.store.incidents(rig.swarm) == []


def test_reemplazo_con_agente_elegido_y_errores(rig: Rig) -> None:
    rig.herdr.screens["t1"] = "Usage limit reached"
    rig.idle("plan")
    rig.tick()
    assert (
        "no conozco 'gemini'" in rig.call(action="resolve", choice="replace", agent="gemini").error
    )
    assert "Opción desconocida" in rig.call(action="resolve", incident="I1", choice="borrar").error
    done = rig.call(
        action="resolve", incident="I1", choice="replace", agent="codex", model="gpt-6-sol"
    )
    assert done.ok and "entra codex gpt-6-sol" in done.data["message"]
    rig.tick(launches=2)
    assert rig.herdr.launched[1][2]["extra_args"] == ["--yolo"]  # Codex con su modo de enjambre
    assert (
        "No hay un incidente abierto I1"
        in rig.call(action="resolve", incident="I1", choice="wait").error
    )


def test_esperar_a_mano_y_pausar_todo(rig: Rig) -> None:
    rig.herdr.screens["t1"] = "Usage limit reached"
    rig.idle("plan")
    rig.tick()
    waited = rig.call(action="resolve", choice="wait")
    assert waited.ok and waited.data["message"] == "Espero a plan; el resto sigue."
    assert rig.call(action="resolve", choice="pause").ok
    assert rig.run("plan", "next").startswith("El enjambre está en pausa.")
    sent = len(rig.herdr.sent)
    _later(rig, 3600)
    rig.tick()
    assert len(rig.herdr.sent) == sent  # en pausa no se despierta a nadie
    assert (
        rig.call(action="status")
        .data["spoken"]
        .endswith("Atención: El enjambre está en pausa. Y 1 aviso más.")
    )
    assert rig.call(action="resume").ok
    assert "Tu tarea" not in rig.run("plan", "next")  # sigue con su T1 en curso
    assert "Ese enjambre no está en pausa" in rig.call(action="resume").error


# ------------------------------------------------------------ contexto
def test_sin_contexto_reabre_solo_y_la_segunda_vez_pregunta(rig: Rig) -> None:
    rig.herdr.screens["t1"] = "Prompt is too long"
    rig.idle("plan")
    rig.tick()
    assert rig.said == [
        "Enjambre: plan se quedó sin contexto; lo reabro en una sesión nueva con su avance guardado."
    ]
    assert rig.asked == []
    rig.tick(launches=2)
    agent, _, kwargs = rig.herdr.launched[1]
    assert agent == "claude" and kwargs["extra_args"] == ["--allowedTools", "Bash(ezs *)"]
    first = rig.run("plan", "next")
    assert (
        "Tu sesión anterior se quedó sin contexto: retoma T1." in first and "En curso: T1" in first
    )
    rig.herdr.screens["t2"] = "Prompt is too long"
    rig.idle("plan")
    _later(rig, INSPECT_SECS + 1)
    rig.tick()
    assert len(rig.asked) == 1 and "sin contexto otra vez en la misma tarea" in rig.asked[0]
    assert len(rig.herdr.launched) == 2  # no se reabre en bucle


def test_compactar_no_es_un_incidente(rig: Rig) -> None:
    rig.herdr.screens["t1"] = "✻ Compacting conversation…"
    rig.tick()  # trabajando: ni se lee la pantalla
    rig.idle("plan")
    rig.tick()
    assert rig.stack.store.incidents(rig.swarm) == [] and rig.asked == [] and rig.said == []


def test_un_texto_viejo_en_pantalla_no_dispara_avisos(rig: Rig) -> None:
    # El agente habló de «rate limit» en su trabajo; el final de la pantalla está bien.
    rig.herdr.screens["t1"] = "Implementé el rate limit del login.\n" + "\n".join(
        f"línea {n}" for n in range(12)
    )
    rig.idle("plan")
    rig.tick()
    assert rig.stack.store.incidents(rig.swarm) == []


# ------------------------------------------------- ventana cerrada y errores
def test_ventana_cerrada_pregunta_y_reabre_con_traspaso(rig: Rig) -> None:
    del rig.herdr.states["t1"]  # el usuario cerró la pestaña
    for _ in range(MISSING_TICKS):
        rig.tick()
    assert len(rig.asked) == 1 and "tiene su ventana cerrada" in rig.asked[0]
    for _ in range(3):
        rig.tick()
    assert len(rig.asked) == 1 and len(rig.herdr.launched) == 1
    reopened = rig.call(action="resolve", choice="reopen")
    assert reopened.ok and reopened.data["message"].startswith("Reabro a plan con claude")
    rig.tick(launches=2)
    assert "Se reabrió tu sesión: retoma T1." in rig.run("plan", "next")


def test_error_desconocido_y_sesion_perdida(rig: Rig) -> None:
    rig.herdr.screens["t1"] = "Algo raro pasó"
    rig.herdr.states["t1"] = "error"
    rig.tick()
    assert "plan (claude, planificar) tuvo un error" in rig.asked[-1]
    rig.call(action="resolve", choice="wait")
    rig.herdr.states["t1"] = "working"
    rig.tick()  # volvió solo
    rig.herdr.screens["t1"] = "Invalid API key · Please run /login"
    rig.herdr.states["t1"] = "error"
    _later(rig, INSPECT_SECS + 1)
    rig.tick()
    assert "perdió la sesión con su proveedor" in rig.asked[-1]
    assert "Inicia sesión en su ventana y lo retomo" in rig.asked[-1]


def test_liberar_su_trabajo_para_otro_del_mismo_rol(tmp_path: Path) -> None:
    rig = Rig(tmp_path)
    rig.start(
        [
            {"agent": "opencode", "role": "dev"},
            {"agent": "codex", "role": "dev"},
        ]
    )
    rig.seat("dev", launches=2)  # los dos se abren: los dos pueden tomar la tarea
    task = rig.stack.store.mine(rig.swarm, "dev")[0].id
    rig.herdr.screens["t1"] = "Insufficient balance"
    rig.idle("dev")
    rig.tick()
    result = rig.call(action="resolve", choice="release")
    assert (
        result.ok
        and result.data["message"]
        == f"dev sale del enjambre; su trabajo (T{task}) queda para su rol."
    )
    assert [t.id for t in rig.stack.store.ready(rig.swarm, "dev2")] == [task]
    assert rig.member("dev").state == "gone"


def test_el_usuario_vuelve_a_ver_el_incidente_tras_reiniciar(tmp_path: Path, rig: Rig) -> None:
    rig.herdr.screens["t1"] = "Usage limit reached"
    rig.idle("plan")
    rig.tick()
    again = Rig(tmp_path)  # EzAgent se reinició: lee el mismo tablero
    assert "incidente I1 · plan · quota · pide decisión" in again.stack.prompt_block()


# ------------------------------------------------------------------- app
def test_las_preguntas_del_enjambre_reabren_la_escucha_aunque_este_silenciado() -> None:
    queued: list[tuple[str, bool]] = []
    stub = SimpleNamespace(
        mute_until=time.time() + 600,
        _queue_announcement=lambda text, reopen_call=True: queued.append((text, reopen_call)),
    )
    VoiceApp._handle_event(stub, ("swarm_notice", "plan volvió.", False))
    VoiceApp._handle_event(stub, ("swarm_notice", "¿Espero o lo reemplazo?", True))
    assert queued == [("¿Espero o lo reemplazo?", True)]
    stub.mute_until = 0
    VoiceApp._handle_event(stub, ("swarm_notice", "plan volvió.", False))
    assert queued[-1] == ("plan volvió.", False)
