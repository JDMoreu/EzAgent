"""Registro persistente de tareas y avisos de finalización."""

from __future__ import annotations

import json
import queue
from types import SimpleNamespace

import pytest

from ezagent.agent.tools_herdr import HerdrCallbacks, build_herdr_tools
from ezagent.voice.app import LISTENING, VoiceApp
from ezagent.voice.tasks import TaskRegistry


class _Herdr:
    def __init__(self, state: str = "working", agent: str = "claude"):
        self.state = state
        self.agent = agent
        self.calls: list[tuple] = []
        self.sent: list[tuple[str, str]] = []
        self.fail_prelaunch = False

    def send_agent(self, address, text, confirmed=False):
        self.sent.append((address, text))
        return {"ok": True}

    def get_agent(self, address):
        state = "working" if address == "t9" else self.state
        return {
            "address": address,
            "agent": self.agent,
            "state": state,
        }

    def read_agent(self, address, lines=None):
        return "salida reciente"

    def run_agent(self, agent, task, **kwargs):
        self.calls.append((agent, task, kwargs))
        if self.fail_prelaunch and not task:
            return {"ok": False, "error": "no abrió"}
        return {"ok": True, "address": "t9", "terminal": "t9"}


def test_registry_persiste_y_recarga(tmp_path):
    path = tmp_path / "tasks.json"
    registry = TaskRegistry(path)
    registry.add("t1", "claude", "revisa la web")
    registry.add("t2", "opencode", "ordena facturas")
    assert path.exists()
    assert [item["address"] for item in registry.all()] == ["t1", "t2"]
    assert registry.get("t1")["task"] == "revisa la web"

    reloaded = TaskRegistry(path)
    assert reloaded.get("t2")["agent"] == "opencode"
    assert "revisa la web" in reloaded.prompt_block()
    assert "<pending_tasks>" in reloaded.prompt_block()

    assert reloaded.remove("t1")["task"] == "revisa la web"
    assert reloaded.get("t1") is None
    assert not path.with_name(path.name + ".tmp").exists()
    data = json.loads(path.read_text(encoding="utf-8"))
    assert data["version"] == 1
    assert [item["address"] for item in data["tasks"]] == ["t2"]


def test_registry_persiste_cadenas(tmp_path):
    path = tmp_path / "tasks.json"
    registry = TaskRegistry(path)
    registry.add_chain(
        {
            "source": "t1",
            "agent": "opencode",
            "task": "haz el plan con la salida",
            "model": "deepseek",
            "effort": "high",
            "notify_on_done": True,
        }
    )
    reloaded = TaskRegistry(path)
    chains = reloaded.chains()
    assert len(chains) == 1
    assert chains[0]["source"] == "t1"
    assert chains[0]["agent"] == "opencode"
    assert "cuando termine t1" in reloaded.prompt_block()
    removed = reloaded.remove_chains_for("t1")
    assert removed and removed[0]["model"] == "deepseek"
    assert reloaded.chains() == []


def test_cadena_lanza_el_destino_con_la_salida_y_modelo(tmp_path):
    app = VoiceApp.__new__(VoiceApp)
    app.tasks = TaskRegistry(tmp_path / "tasks.json")
    app.watched = set()
    app.events = queue.Queue()
    app.herdr = _Herdr("working", "claude")
    app.messenger = SimpleNamespace(_last_user_text="avísame al terminar")
    spec = {
        "source": "t1",
        "agent": "opencode",
        "task": "haz el plan definitivo",
        "model": "deepseek-v4.1-flash",
        "effort": "high",
        "notify_on_done": True,
    }
    VoiceApp._enqueue_chain(app, spec)
    assert "t1" in app.watched
    assert app.tasks.chains_for("t1")

    app.herdr.state = "done"
    VoiceApp._check_task_now(app, "t1")
    # El destino se abrió por adelantado (sin tarea, ya con modelo y esfuerzo)...
    agent, task, kwargs = app.herdr.calls[0]
    assert agent == "opencode"
    assert task == ""
    assert kwargs["model"] == "deepseek-v4.1-flash"
    assert kwargs["effort"] == "high"
    assert kwargs["confirmed"] is True
    # ...y al terminar la fuente solo se le pega instrucción + respuesta.
    assert len(app.herdr.sent) == 1
    address, handoff = app.herdr.sent[0]
    assert address == "t9"
    assert "haz el plan definitivo" in handoff
    assert "salida reciente" in handoff
    assert "salida_agente" not in handoff
    assert app.tasks.get("t9")["task"] == "haz el plan definitivo"
    assert "t9" in app.watched


def test_cadena_sin_destino_por_adelantado_lanza_normal(tmp_path):
    app = VoiceApp.__new__(VoiceApp)
    app.tasks = TaskRegistry(tmp_path / "tasks.json")
    app.watched = set()
    app.events = queue.Queue()
    app.herdr = _Herdr("working", "claude")
    app.herdr.fail_prelaunch = True
    app.messenger = SimpleNamespace(_last_user_text="")
    VoiceApp._enqueue_chain(
        app,
        {
            "source": "t1",
            "agent": "opencode",
            "task": "responde: {output}",
            "notify_on_done": True,
        },
    )
    app.herdr.state = "done"
    VoiceApp._check_task_now(app, "t1")
    last_agent, last_task, _ = app.herdr.calls[-1]
    assert last_agent == "opencode"
    assert last_task == "responde: salida reciente"
    assert app.herdr.sent == []
    # En el aviso se muestra la tarea sin el marcador.
    assert app.tasks.get("t9")["task"] == "responde"


def test_respuesta_del_ultimo_agente_se_dice_sola(tmp_path):
    app = VoiceApp.__new__(VoiceApp)
    app.tasks = TaskRegistry(tmp_path / "tasks.json")
    app.watched = {"t9"}
    app.events = queue.Queue()
    app.herdr = _Herdr("done")
    app.messenger = SimpleNamespace(_last_user_text="dime la respuesta")
    app.tasks.add("t9", "opencode", "responde la pregunta")
    VoiceApp._deliver_answer(app, "t9")
    VoiceApp._on_watcher_event(app, "t9", "opencode", "done", "done")
    event = app.events.get_nowait()
    assert event[0] == "done_answer"
    assert event[1] == "t9"
    assert event[3] == "salida reciente"


def test_watch_guarda_la_tarea_y_avisa_al_terminar(tmp_path):
    app = VoiceApp.__new__(VoiceApp)
    app.tasks = TaskRegistry(tmp_path / "tasks.json")
    app.watched = set()
    app.events = queue.Queue()
    app.herdr = _Herdr("working")
    app.messenger = SimpleNamespace(
        _last_user_text="vigila esta tarea y avísame cuando termine"
    )

    VoiceApp._watch(app, "t1")
    assert "t1" in app.watched
    assert (
        app.tasks.get("t1")["task"]
        == "vigila esta tarea y avísame cuando termine"
    )

    app.herdr.state = "done"
    VoiceApp._check_task_now(app, "t1")
    event = app.events.get_nowait()
    assert event[0] == "done_prompt"
    assert event[1] == "t1"
    assert event[5] == "vigila esta tarea y avísame cuando termine"
    assert app.tasks.get("t1") is None
    assert "t1" not in app.watched


def test_watcher_elimina_la_tarea_y_la_incluye_en_el_aviso(tmp_path):
    app = VoiceApp.__new__(VoiceApp)
    app.tasks = TaskRegistry(tmp_path / "tasks.json")
    app.tasks.add("t1", "claude", "abre el navegador y busca X")
    app.watched = {"t1"}
    app.events = queue.Queue()
    app.herdr = _Herdr("done")

    VoiceApp._on_watcher_event(app, "t1", "claude", "done", "done")
    event = app.events.get_nowait()
    assert event[5] == "abre el navegador y busca X"
    assert app.tasks.get("t1") is None


def test_resume_pending_tasks_recupera_tras_reinicio(tmp_path):
    path = tmp_path / "tasks.json"
    TaskRegistry(path).add("t1", "claude", "termina el informe")
    app = VoiceApp.__new__(VoiceApp)
    app.tasks = TaskRegistry(path)
    app.watched = set()
    app.events = queue.Queue()
    app.herdr = _Herdr("done")
    app.messenger = SimpleNamespace(_last_user_text="")

    VoiceApp._resume_pending_tasks(app)
    event = app.events.get_nowait()
    assert event[5] == "termina el informe"
    assert not app.watched
    assert app.tasks.get("t1") is None


def test_handle_done_prompt_dice_la_tarea_y_la_pasa_al_llm():
    app = VoiceApp.__new__(VoiceApp)
    app.in_call = True
    app.state = LISTENING
    app._turn_gen = 3
    calls: list[tuple] = []

    def note_done(address, agent, reason, output, *, task=""):
        calls.append(("note", address, agent, reason, output, task))

    app.messenger = SimpleNamespace(note_done=note_done)
    app.notch = SimpleNamespace(notice=lambda: calls.append(("notice",)))
    app._speak = lambda text, kind="answer": calls.append(("speak", text))

    class _Root:
        @staticmethod
        def after(delay, callback):
            calls.append(("after", delay))
            callback()

    app.root = _Root()
    VoiceApp._handle_done_prompt(
        app, "t1", "claude", "done", "salida", task="abre el navegador"
    )
    assert (
        "note",
        "t1",
        "claude",
        "done",
        "salida",
        "abre el navegador",
    ) in calls
    spoken = [call[1] for call in calls if call[0] == "speak"]
    assert spoken
    assert "abre el navegador" in spoken[0]
    assert "resuma" in spoken[0]


def test_delegate_recuerda_la_tarea_al_pedir_aviso():
    class _FakeHerdr:
        def run_agent(self, agent, task, **kwargs):
            return {"ok": True, "address": "t9", "terminal": "t9"}

    remembered: list[tuple[str, str, str]] = []
    watched: list[str] = []
    callbacks = HerdrCallbacks(
        watch=watched.append,
        request_close=lambda: None,
        mic_mute=lambda muted: None,
        last_summary=lambda: "",
        mute=lambda minutes: None,
        get_last_user_text=lambda: "avísame cuando termine",
        remember_task=lambda address, agent, task: remembered.append(
            (address, agent, task)
        ),
    )
    tools = build_herdr_tools(_FakeHerdr(), callbacks)
    delegate = next(tool for tool in tools if tool.name == "delegate_to_agent")
    result = delegate.handler(
        {
            "action": "run",
            "agent": "claude",
            "task": "revisa la web",
            "notify_on_done": True,
        }
    )
    assert result.ok
    assert remembered == [("t9", "claude", "revisa la web")]
    assert watched == ["t9"]


def test_delegate_encadena_si_la_fuente_sigue_trabajando():
    herdr = _Herdr("working")
    chains: list[dict] = []
    callbacks = HerdrCallbacks(
        watch=lambda address: None,
        request_close=lambda: None,
        mic_mute=lambda muted: None,
        last_summary=lambda: "",
        mute=lambda minutes: None,
        get_last_user_text=lambda: "avísame al terminar",
        enqueue_chain=chains.append,
    )
    tools = build_herdr_tools(herdr, callbacks)
    delegate = next(tool for tool in tools if tool.name == "delegate_to_agent")
    result = delegate.handler(
        {
            "action": "run",
            "agent": "opencode",
            "task": "haz el plan",
            "input_from": "t1",
            "model": "deepseek",
            "effort": "high",
            "notify_on_done": True,
        }
    )
    assert result.ok
    assert result.data["chained"] is True
    assert chains and chains[0]["source"] == "t1"
    assert chains[0]["model"] == "deepseek"
    assert chains[0]["effort"] == "high"
    assert herdr.calls == []


def test_delegate_usa_la_salida_si_la_fuente_ya_termino():
    herdr = _Herdr("done")
    callbacks = HerdrCallbacks(
        watch=lambda address: None,
        request_close=lambda: None,
        mic_mute=lambda muted: None,
        last_summary=lambda: "",
        mute=lambda minutes: None,
        get_last_user_text=lambda: "avísame al terminar",
    )
    tools = build_herdr_tools(herdr, callbacks)
    delegate = next(tool for tool in tools if tool.name == "delegate_to_agent")
    result = delegate.handler(
        {
            "action": "run",
            "agent": "opencode",
            "task": "haz el plan",
            "input_from": "t1",
            "model": "deepseek",
            "effort": "high",
        }
    )
    assert result.ok
    agent, task, kwargs = herdr.calls[0]
    assert agent == "opencode"
    assert "salida_agente" not in task  # sin explicaciones del traspaso
    assert "haz el plan" in task
    assert kwargs["model"] == "deepseek"
    assert kwargs["effort"] == "high"


def test_atajo_local_lista_las_tareas_pendientes(tmp_path):
    app = VoiceApp.__new__(VoiceApp)
    app.tasks = TaskRegistry(tmp_path / "tasks.json")
    app.tasks.add("t1", "claude", "revisa la web")
    app.tasks.add("t2", "opencode", "ordena facturas")
    spoken: list[str] = []
    app._speak = lambda text, kind="answer": spoken.append(text)

    assert VoiceApp._fast_command(app, "¿qué tareas pendientes tengo?")
    assert spoken
    assert "2 tareas pendientes" in spoken[0]
    assert "revisa la web" in spoken[0]


def _app_for_mute(tmp_path, calls):
    app = VoiceApp.__new__(VoiceApp)
    app.tasks = TaskRegistry(tmp_path / "tasks.json")
    app.last_summary = ""
    app._set_mic_muted = lambda muted, source="": calls.append((muted, source))
    app._speak = lambda text, kind="answer": None
    return app


@pytest.mark.parametrize(
    "phrase", ["silencio", "silencia", "mute", "Silencio.", "silencio por favor"]
)
def test_palabras_sueltas_no_silencian_el_micro(tmp_path, phrase):
    calls: list[tuple] = []
    app = _app_for_mute(tmp_path, calls)
    VoiceApp._fast_command(app, phrase)
    assert calls == []


@pytest.mark.parametrize(
    "phrase", ["silencia el micro", "Silencia el micrófono.", "mutea"]
)
def test_orden_explicita_si_silencia_el_micro(tmp_path, phrase):
    calls: list[tuple] = []
    app = _app_for_mute(tmp_path, calls)
    assert VoiceApp._fast_command(app, phrase) is True
    assert calls == [(True, "voz")]
