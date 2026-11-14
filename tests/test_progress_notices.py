"""Avisos de avance (enjambre y cadenas), roles libres y actividad de la barra."""

from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace
from typing import Any

import numpy as np
import pytest
from swarm_rig import Rig

from ezagent.agent.orchestration import FLOW_PREFIX, chain_spec, flow_task
from ezagent.agent.runtime import _SWARM_RE
from ezagent.agent.tools_herdr import HerdrCallbacks, _handle_run, _SessionState
from ezagent.swarm.models import Task
from ezagent.swarm.progress import Progress
from ezagent.voice.hud import marks
from ezagent.voice.hud.drop import DropModel, layout_size, render_model


def _task(id_: int, role: str, state: str, deps: tuple[int, ...] = (), rounds: int = 0) -> Task:
    return Task(id_, f"T{id_}", role, "", state, deps, 0, "", rounds)


# ------------------------------------------------------------ enjambre: avance
def test_progress_toma_nota_la_primera_vez_y_luego_avisa_cada_etapa() -> None:
    progress = Progress()
    tasks = [_task(1, "plan", "done"), _task(2, "dev", "pending", (1,)), _task(3, "qa", "pending")]
    assert progress.check("s", tasks) == []  # arranque o reinicio: nada atrasado
    tasks = [
        _task(1, "plan", "done"),
        _task(2, "dev", "done", (1,)),
        _task(3, "qa", "pending", (2,)),
    ]
    assert progress.check("s", tasks) == [
        "Enjambre: terminó el desarrollo. Ahora empieza la validación."
    ]
    assert progress.check("s", tasks) == []  # una sola vez


def test_progress_etapas_en_paralelo_y_devoluciones() -> None:
    progress = Progress()
    start = [
        _task(1, "arch", "claimed"),
        _task(2, "back", "pending", (1,)),
        _task(3, "front", "pending", (1,)),
        _task(4, "qa", "pending", (2, 3)),
    ]
    progress.check("s", start)
    step = [
        _task(1, "arch", "done"),
        _task(2, "back", "pending", (1,)),
        _task(3, "front", "pending", (1,)),
        _task(4, "qa", "pending", (2, 3)),
    ]
    assert progress.check("s", step) == [
        "Enjambre: terminó la arquitectura. Ahora empiezan el backend y el frontend."
    ]
    step = [
        _task(1, "arch", "done"),
        _task(2, "back", "done", (1,)),
        _task(3, "front", "claimed", (1,)),
        _task(4, "qa", "pending", (2, 3)),
    ]
    assert progress.check("s", step) == [
        "Enjambre: terminó el backend. Ahora empieza la validación."
    ]
    back = [
        _task(1, "arch", "done"),
        _task(2, "back", "claimed", (1,), rounds=1),
        _task(3, "front", "claimed", (1,)),
        _task(4, "qa", "pending", (2, 3)),
    ]
    assert progress.check("s", back) == [
        "Enjambre: devolvieron el backend para corregir (ronda 1 de 3)."
    ]


def test_progress_no_dice_la_ultima_etapa_aparte() -> None:
    progress = Progress()
    progress.check("s", [_task(1, "plan", "done"), _task(2, "qa", "claimed", (1,))])
    assert progress.check("s", [_task(1, "plan", "done"), _task(2, "qa", "done", (1,))]) == []


def test_un_rol_libre_se_nombra_por_su_rol() -> None:
    progress = Progress()
    progress.check("s", [_task(1, "copy", "claimed"), _task(2, "seo", "pending", (1,))])
    texts = progress.check("s", [_task(1, "copy", "done"), _task(2, "seo", "pending", (1,))])
    assert texts == ["Enjambre: terminó el trabajo de copy. Ahora empieza el trabajo de seo."]


# ------------------------------------------------------------- roles y etapas
def test_roles_libres_paralelo_y_tarea_propia(tmp_path: Path) -> None:
    rig = Rig(tmp_path)
    rig.start(
        [
            {"agent": "claude", "role": "Arquitecto"},
            {"agent": "opencode", "role": "backend", "task": "API de pagos en FastAPI"},
            {"agent": "opencode", "role": "Frontend", "parallel": True},
            {"agent": "codex", "role": "QA"},
            {"agent": "codex", "role": "Diseñador gráfico"},
        ]
    )
    members = {m.name: m.role for m in rig.stack.store.members(rig.swarm)}
    assert members == {
        "arch": "arch",
        "back": "back",
        "front": "front",
        "qa": "qa",
        "disenador-grafico": "disenador-grafico",
    }
    tasks = rig.stack.store.tasks(rig.swarm)
    arch, back, front, qa, extra = tasks
    assert back.title == "API de pagos en FastAPI" and back.owner == "back"
    assert back.deps == front.deps == (arch.id,)  # backend y frontend a la vez
    assert set(qa.deps) == {back.id, front.id}
    assert extra.deps == (qa.id,) and extra.title == "Trabajo de disenador-grafico"
    assert "lock" in rig.stack.service.briefing(rig.swarm, "disenador-grafico")
    assert "ask" in rig.stack.service.briefing(rig.swarm, "back")


def test_el_primer_miembro_no_puede_ser_paralelo(tmp_path: Path) -> None:
    rig = Rig(tmp_path)
    rig.start([{"agent": "claude", "role": "plan", "parallel": True}])
    assert len(rig.stack.store.tasks(rig.swarm)) == 1


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ("delegale a uno el rol de planificador, otro desarrollador y el otro de QA", True),
        ("un arquitecto y luego el frontend", True),
        ("haz el backend de la API", False),
        ("abre codex y dime la hora", False),
    ],
)
def test_repartir_roles_ofrece_el_enjambre(text: str, expected: bool) -> None:
    assert bool(_SWARM_RE.search(text)) is expected


# ------------------------------------------------------------ cadenas clásicas
def test_chain_spec_lleva_las_etapas_del_flujo() -> None:
    spec = chain_spec(
        "t1", [{"agent": "codex", "task": "x"}], cwd=None, space=None, notify=True, flow=["plan"]
    )
    assert spec["flow"] == ["plan"]
    assert flow_task(["a", "b"]) == FLOW_PREFIX + "a → b"


class _Herdr:
    def __init__(self) -> None:
        self.cfg = SimpleNamespace(agents={"opencode": {}})

    def resolve_agent(self, name: str) -> str | None:
        return name if name in self.cfg.agents else None

    def run_agent(self, agent: str, task: str, **kwargs: Any) -> dict[str, Any]:
        return {"ok": True, "address": "t9"}


def _callbacks(watched: list[str], text: str = "") -> HerdrCallbacks:
    return HerdrCallbacks(
        watch=watched.append,
        request_close=lambda: None,
        mic_mute=lambda muted: None,
        last_summary=lambda: "",
        mute=lambda minutes: None,
        get_last_user_text=lambda: text,
    )


def test_delegar_avisa_al_terminar_por_defecto() -> None:
    watched: list[str] = []
    args = {"action": "run", "agent": "opencode", "task": "haz la landing"}
    assert _handle_run(_Herdr(), _callbacks(watched), _SessionState(), args).ok
    assert watched == ["t9"]
    watched.clear()
    quiet = {**args, "notify_on_done": False}
    assert _handle_run(_Herdr(), _callbacks(watched), _SessionState(), quiet).ok
    assert watched == []


def test_la_cadena_avisa_cada_etapa_y_el_final_nombra_el_flujo(tmp_path: Path) -> None:
    from ezagent.voice.app import VoiceApp
    from ezagent.voice.tasks import TaskRegistry

    app = VoiceApp.__new__(VoiceApp)
    said: list[tuple[str, bool]] = []
    app.tasks = TaskRegistry(tmp_path / "tasks.json")
    app.cfg = SimpleNamespace(agent_profiles={"opencode": {"label": "OpenCode"}})
    app.herdr = SimpleNamespace(
        resolve_agent=lambda name: name,
        read_full=lambda address: "Plan: hacer la landing",
        run_agent=lambda agent, task, **kw: {"ok": True, "address": "t2"},
        get_agent=lambda address: {"state": "working"},
    )
    app.watched = set()
    app._queue_announcement = lambda text, reopen_call=True: said.append((text, reopen_call))
    app.tasks.add_chain(
        chain_spec(
            "t1",
            [{"agent": "opencode", "task": "Desarrolla {output}", "name": "desarrollador"}],
            cwd=None,
            space=None,
            notify=True,
            flow=["planificador"],
        )
    )
    assert app._launch_chains("t1") == 1
    assert said == [("Terminó «planificador». Ahora empieza «desarrollador».", False)]
    assert app.tasks.get("t2")["task"] == FLOW_PREFIX + "planificador → desarrollador"
    assert "t2" in app.watched

    app.messenger = SimpleNamespace(note_done=lambda *a, **k: None)
    app.hud = SimpleNamespace(notice=lambda: None)
    app._handle_done_prompt("t2", "opencode", "done", "", app.tasks.get("t2")["task"])
    question = said[-1][0]
    assert question.startswith("Oye, terminó todo el flujo: planificador → desarrollador.")
    assert "resuma" in question and "muestre" in question


# ------------------------------------------------------------------ la barra
def test_la_orbita_dibuja_una_marca_por_estado_y_se_cachea() -> None:
    assert marks.orbit_tile("circle", (), 58, 58, 1.0, "dark") is None
    one = marks.orbit_tile("circle", ("working",), 58, 58, 1.0, "dark")
    assert one is not None
    assert marks.orbit_tile("circle", ("working",), 58, 58, 1.0, "dark") is one  # caché
    three = marks.orbit_tile("circle", ("needs_you", "working", "done"), 58, 58, 1.0, "dark")
    ink = lambda tile: int((np.asarray(tile)[..., 3] > 128).sum())  # noqa: E731
    assert ink(three) > ink(one) * 2


def test_la_marca_en_curso_respira_y_con_movimiento_reducido_no() -> None:
    values = {marks.breath(t / 10, False) for t in range(20)}
    assert len(values) > 3 and max(values) <= 1.0
    assert marks.breath(0.37, True) == 1.0


def _frame(muted: bool, orbit: tuple[str, ...] = (), layout: str = "pill") -> np.ndarray:
    model = DropModel(layout, None)
    for step in range(90):
        pose = "mute" if muted else "idle"
        model.update(step / 60, 1 / 60, pose, 0.0, "dark", False, False, False, False, muted=muted)
    w, h = layout_size(layout, None)
    return render_model(
        model, round(w), round(h), 1.0, "dark", muted, False, False, 1.5, orbit=orbit
    )


def _form_stats(frame: np.ndarray) -> tuple[int, float]:
    """Píxeles opacos de la forma y su saturación media (BGRA premultiplicado)."""
    solid = frame[..., 3] > 200
    rgb = frame[solid][:, :3].astype(float)
    sat = (rgb.max(axis=1) - rgb.min(axis=1)).mean() if len(rgb) else 0.0
    return int(solid.sum()), float(sat)


@pytest.mark.parametrize("layout", ["circle", "pill"])
def test_micro_en_pausa_vuelve_gris_la_forma_y_la_encoge_sin_iconos_ni_bordes(layout: str) -> None:
    plain, muted = _frame(False, layout=layout), _frame(True, layout=layout)
    plain_area, plain_sat = _form_stats(plain)
    muted_area, muted_sat = _form_stats(muted)
    assert muted_sat < plain_sat * 0.3  # casi sin color: gris, no un rojo fijo
    assert muted_area < plain_area * 0.9  # y un poco más pequeña
    red = (muted[..., 2].astype(int) - muted[..., 1].astype(int)) > 60  # BGRA: R ≫ G
    assert red.sum() == 0  # ni icono ni borde rojo


def _amber(frame: np.ndarray) -> np.ndarray:
    b, g, r = (frame[..., i].astype(int) for i in range(3))
    return (r > 150) & (r - b > 90) & (g > 90)


def test_lo_que_te_necesita_se_marca_en_ambar_arriba_a_la_derecha_de_la_esfera() -> None:
    frame = _frame(False, ("needs_you",), "circle")
    assert _amber(_frame(False, (), "circle")).sum() == 0
    h, w = frame.shape[:2]
    amber = _amber(frame)
    assert amber[: h // 2, w // 2 :].sum() > 10
    assert amber[h // 2 :, :].sum() == 0


def test_en_la_pildora_las_marcas_van_debajo_de_la_forma() -> None:
    frame = _frame(False, ("needs_you",), "pill")
    h = frame.shape[0]
    amber = _amber(frame)
    assert amber[h // 2 :].sum() > 10 and amber[: h // 2].sum() == 0


def test_set_activity_guarda_la_foto_y_la_pasa_a_la_vista_rapida() -> None:
    from ezagent.voice.activity import ActivityItem
    from ezagent.voice.hud.drop import DropUI

    seen: list[tuple] = []
    ui = DropUI.__new__(DropUI)
    ui._peek = SimpleNamespace(refresh=seen.append)
    item = ActivityItem("t1", "Tarea", "codex", "working", 0.0, live=True)
    ui.set_activity([item])
    assert ui._activity == (item,) and seen == [(item,)]


def test_la_app_registra_desenlaces_y_los_da_por_vistos() -> None:
    from ezagent.voice import app as voice_app

    snapshots: list[tuple] = []
    toasts: list[str] = []
    app = SimpleNamespace(
        hud=SimpleNamespace(
            set_activity=snapshots.append, announce_activity=lambda item: toasts.append(item.key)
        ),
        watched=set(),
    )
    voice_app._news_arrived(app, "t1", "codex", "Revisar permisos", "blocked")
    item = snapshots[-1][0]
    assert (item.state, item.unseen, item.title) == ("needs_you", True, "Revisar permisos")
    assert toasts == ["t1"]
    voice_app._news_seen(app)
    assert snapshots[-1][0].unseen is False
    count = len(snapshots)
    voice_app._news_seen(app)  # nada nuevo: no repinta
    assert len(snapshots) == count


def test_lo_vivo_sale_del_ultimo_estado_del_vigilante() -> None:
    from ezagent.voice.activity import live_items

    states = {"t1": "working", "t2": "idle", "t3": "blocked"}  # t9: terminal cerrada
    app = SimpleNamespace(
        watched={"t1", "t2", "t3", "t9"},
        watcher=SimpleNamespace(state_of=states.get),
        swarm=SimpleNamespace(supervisor=SimpleNamespace(active_view=(("s1", "Migrar"),))),
    )
    got = {item.key: item.state for item in live_items(app, now=100.0)}
    assert got == {"t1": "working", "t3": "needs_you", "t9": "stale", "swarm:s1": "working"}


def test_el_vigilante_publica_el_ultimo_estado() -> None:
    from ezagent.voice.watcher import Watcher

    watcher = Watcher(SimpleNamespace(), lambda *a: None, lambda a: True)
    watcher._compare([{"address": "t1", "status": {"state": "working", "app": "codex"}}])
    assert watcher.state_of("t1") == "working" and watcher.state_of("t2") is None


def test_una_tarea_guardada_sin_terminal_se_olvida(tmp_path: Path) -> None:
    from ezagent.voice.app import VoiceApp
    from ezagent.voice.herdr import HerdrNotFound
    from ezagent.voice.tasks import TaskRegistry

    def gone(address: str) -> dict[str, Any]:
        raise HerdrNotFound(f'no terminal "{address}"')

    app = VoiceApp.__new__(VoiceApp)
    app.tasks = TaskRegistry(tmp_path / "tasks.json")
    app.tasks.add("t41", "", "una tarea de hace días")
    app.herdr = SimpleNamespace(get_agent=gone)
    app.watched = set()
    app.events = SimpleNamespace(put=lambda event: pytest.fail("no debe avisar"))
    app._resume_pending_tasks()
    assert app.watched == set() and app.tasks.get("t41") is None


def test_herdr_distingue_terminal_inexistente(monkeypatch: pytest.MonkeyPatch) -> None:
    from ezagent.voice.herdr import Herdr, HerdrError, HerdrNotFound

    herdr = Herdr.__new__(Herdr)
    out = '{"error":{"kind":"not_found","message":"no terminal \\"t41\\""}}'
    done = SimpleNamespace(stdout=out, stderr="", returncode=1)
    monkeypatch.setattr(herdr, "_run_command", lambda command, timeout: done)
    with pytest.raises(HerdrNotFound):
        herdr._json_command(["hc", "get", "t41"], 5)
    done.stdout = '{"error":{"kind":"busy","message":"x"}}'
    with pytest.raises(HerdrError) as info:
        herdr._json_command(["hc", "get", "t41"], 5)
    assert not isinstance(info.value, HerdrNotFound)
