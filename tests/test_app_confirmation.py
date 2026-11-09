"""Puente entre la confirmación del runtime y la app de voz.

No crea Tk, audio ni ventanas: sustituye el notch, el agente y los métodos de
estado por dobles.
"""

from __future__ import annotations

import queue
import threading
import time
from types import SimpleNamespace

from ezagent.voice.app import LISTENING, SPEAKING, THINKING, VoiceApp
from ezagent.voice.config import VoiceConfig


class _Notch:
    def __init__(self) -> None:
        self.shown = None
        self.hidden = 0

    def show_confirmation(self, summary, strong, on_yes, on_no):
        self.shown = (summary, strong, on_yes, on_no)

    def hide_confirmation(self):
        self.hidden += 1


class _Agent:
    def __init__(self, pending: bool = True) -> None:
        self.confirm_pending = pending
        self.submitted: list[str] = []
        self.clicks: list[bool] = []
        self.cancelled = 0

    def submit_confirmation(self, text):
        self.submitted.append(text)

    def submit_confirmation_click(self, yes):
        self.clicks.append(yes)

    def cancel_confirmation(self):
        self.cancelled += 1


def _app() -> tuple[VoiceApp, _Notch, _Agent, list]:
    app = VoiceApp.__new__(VoiceApp)
    app.events = queue.Queue()
    app.notch = _Notch()
    app.agent = _Agent()
    app.in_call = True
    app._turn_gen = 7
    app._tts_stop = None
    app._speaking_kind = None
    app.muted = False
    app._ignore_utterance = False
    app._post_tts_ignore = False
    app._post_tts_until = 0.0
    app._user_speaking = False
    app._close_after_answer = False
    app.state = SPEAKING
    app.cfg = SimpleNamespace(barge_in=True, barge_in_rms=0.008)
    calls: list[tuple] = []
    app._speak_same_turn = lambda text, kind="answer": calls.append(
        ("speak", text)
    )
    app._set_state = lambda state: calls.append(("state", state))
    return app, app.notch, app.agent, calls


def test_create_agent_runtime_conecta_los_callbacks(monkeypatch):
    captured = {}

    class _Runtime:
        def __init__(self, cfg, herdr, callbacks):
            captured["cfg"] = cfg
            captured["herdr"] = herdr
            captured["callbacks"] = callbacks

    monkeypatch.setattr("ezagent.agent.runtime.AgentRuntime", _Runtime)
    app = VoiceApp.__new__(VoiceApp)
    app.events = queue.Queue()
    app.herdr = object()
    runtime = VoiceApp._create_agent_runtime(app, VoiceConfig())
    assert isinstance(runtime, _Runtime)
    callbacks = captured["callbacks"]
    assert callbacks.on_confirm_ask is not None
    callbacks.on_confirm_ask("¿Confirmas?", False)
    assert app.events.get_nowait() == ("confirm_ask", "¿Confirmas?", False)


def test_create_agent_runtime_cae_si_falla(monkeypatch):
    class _Broken:
        def __init__(self, *args, **kwargs):
            raise RuntimeError("sin núcleo")

    monkeypatch.setattr("ezagent.agent.runtime.AgentRuntime", _Broken)
    app = VoiceApp.__new__(VoiceApp)
    app.events = queue.Queue()
    app.herdr = object()
    assert VoiceApp._create_agent_runtime(app, VoiceConfig()) is None


def test_show_confirmation_muestra_y_lee_el_resumen():
    app, notch, _agent, calls = _app()
    app._show_confirmation("Voy a borrar 7 archivos.", True)
    assert notch.shown is not None
    assert notch.shown[0] == "Voy a borrar 7 archivos."
    assert notch.shown[1] is True
    assert calls and calls[0][0] == "speak"
    assert "confirmo" in calls[0][1].lower()
    notch.shown[2]()
    event = app.events.get_nowait()
    assert event == ("confirm_click", True)


def test_una_frase_transcrita_resuelve_la_confirmacion_antes_del_turno():
    app, _notch, agent, calls = _app()
    VoiceApp._handle_event(app, ("transcribed", "sí, adelante", 3))
    assert agent.submitted == ["sí, adelante"]
    assert not calls


def test_el_clic_resuelve_la_confirmacion():
    app, _notch, agent, _calls = _app()
    VoiceApp._handle_event(app, ("confirm_click", False))
    assert agent.clicks == [False]


def test_confirm_done_oculta_la_fila():
    app, notch, _agent, _calls = _app()
    VoiceApp._handle_event(app, ("confirm_done",))
    assert notch.hidden == 1


def test_barge_in_durante_la_confirmacion_no_invalida_el_turno():
    app, _notch, _agent, calls = _app()
    app._tts_stop = threading.Event()
    VoiceApp._on_barge_in(app, 0.02)          # voz sostenida confirmada por el detector
    assert app._turn_gen == 7
    assert app._tts_stop.is_set()
    assert app._user_speaking is True
    assert ("state", LISTENING) in calls


def test_un_arranque_de_voz_suelto_no_corta_la_pregunta_de_confirmacion():
    app, _notch, _agent, calls = _app()
    app._tts_stop = threading.Event()
    VoiceApp._on_speech_start(app, 0.02)      # pico de ruido mientras suena la pregunta
    assert not app._tts_stop.is_set()
    assert app._ignore_utterance is True      # esa «frase» se descartará
    assert ("state", LISTENING) not in calls


def test_con_la_interrupcion_apagada_responder_a_la_confirmacion_sigue_cortando():
    app, _notch, _agent, calls = _app()
    app.cfg = SimpleNamespace(barge_in=False, barge_in_rms=0.008)
    app._tts_stop = threading.Event()
    VoiceApp._on_speech_start(app, 0.02)
    assert app._tts_stop.is_set() and ("state", LISTENING) in calls


def test_utterance_durante_la_confirmacion_conserva_la_generacion():
    app, _notch, _agent, calls = _app()
    app._tts_stop = None
    app.mic = SimpleNamespace(last_speech_end=time.monotonic())
    seen: list[int] = []
    app._stt_worker = lambda pcm, rate, gen, timing: seen.append(gen)
    VoiceApp._on_utterance(app, b"\x00\x00" * 320, 16000)
    for _ in range(50):
        if seen:
            break
        time.sleep(0.01)
    assert seen == [7]
    assert app._turn_gen == 7
    assert ("state", THINKING) in calls
