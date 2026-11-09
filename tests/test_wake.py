from __future__ import annotations

import queue
import threading
from types import SimpleNamespace

import pytest

from ezagent.voice import app as app_module
from ezagent.voice.app import VoiceApp
from ezagent.voice.wake import (
    is_wake_phrase,
    utterance_secs,
    wake_forms,
    worth_transcribing,
)


# ------------------------------------------------------------ frase pura
@pytest.mark.parametrize(
    "text",
    [
        "EzAgent, escúchame",
        "ezagent escuchame.",
        "Easy agent, escúchame",
        "Oye EzAgent, escúchame por favor",
        "Escúchame, EzAgent",
        "EzAgent, vuelve",
        "EzAgent, abre la escucha",
    ],
)
def test_frase_de_reactivacion_valida(text):
    assert is_wake_phrase(text)


@pytest.mark.parametrize(
    "text",
    [
        "",
        "escúchame",
        "EzAgent",
        "vuelve",
        "oye escúchame lo que te digo, EzAgent es genial",
        "le dije a EzAgent escúchame pero no",
        "abre la escucha",
    ],
)
def test_una_palabra_suelta_o_frase_en_medio_no_reactiva(text):
    assert not is_wake_phrase(text)


def test_frase_configurada_sin_nombre_exige_el_nombre():
    forms = wake_forms("despiértate")
    assert "ezagent despiertate" in forms
    assert "despiertate" not in forms
    assert is_wake_phrase("EzAgent, despiértate", "despiértate")
    assert not is_wake_phrase("despiértate", "despiértate")


def test_solo_se_transcriben_frases_cortas():
    rate = 16000
    assert utterance_secs(b"\0" * rate * 2, rate) == 1.0
    assert worth_transcribing(b"\0" * rate * 2 * 2, rate)  # 2 s
    assert not worth_transcribing(b"\0" * rate * 2 * 5, rate)  # 5 s
    assert not worth_transcribing(b"\0" * 1000, rate)  # ruido corto
    assert not worth_transcribing(b"", rate)


# ------------------------------------------------------------ app
def _app(**cfg) -> VoiceApp:
    app = VoiceApp.__new__(VoiceApp)
    base = {"close_action": "pause", "voice_wake": True, "wake_phrase": "EzAgent, escúchame"}
    base.update(cfg)
    app.cfg = SimpleNamespace(**base)
    app.events = queue.Queue()
    app.in_call = True
    app.muted = False
    app._close_after_answer = False
    app._close_mode = ""
    app.last_summary = ""
    return app


def test_cierra_la_escucha_pausa_por_defecto():
    app = _app()
    spoken: list[str] = []
    app._speak = lambda text, kind="answer": spoken.append(text)
    assert VoiceApp._fast_command(app, "Cierra la escucha")
    assert app._close_after_answer and app._close_mode == "pause"
    assert "EzAgent, escúchame" in spoken[0]
    assert "Vale" not in spoken[0]


def test_adios_cuelga_siempre():
    app = _app()
    app._speak = lambda text, kind="answer": None
    assert VoiceApp._fast_command(app, "adiós")
    assert app._close_mode == "hangup"


def test_cierra_la_escucha_cuelga_si_el_ajuste_lo_pide():
    app = _app(close_action="hangup")
    app._speak = lambda text, kind="answer": None
    assert VoiceApp._fast_command(app, "cierra la escucha")
    assert app._close_mode == "hangup"


def test_close_listening_en_pausa_sigue_en_llamada():
    app = _app()
    calls: list = []
    app._set_mic_muted = lambda muted, source="": calls.append(("muted", muted, source))
    app._set_state = lambda state: calls.append(("state", state))
    app._hang_up = lambda: calls.append(("hangup",))
    app._close_mode = "pause"
    assert VoiceApp._close_listening(app) is True
    assert ("muted", True, "cierre") in calls
    assert ("hangup",) not in calls
    assert app._close_mode == ""


def test_close_listening_cuelga():
    app = _app()
    calls: list = []
    app._hang_up = lambda: calls.append("hangup")
    app._close_mode = "hangup"
    assert VoiceApp._close_listening(app) is False
    assert calls == ["hangup"]


def test_el_llm_solo_puede_pausar():
    app = _app()
    VoiceApp._llm_mic_mute(app, True)
    assert app.events.get_nowait() == ("close_request", "pause")
    VoiceApp._llm_mic_mute(app, False)
    assert app.events.empty()


def _run_wake(app, monkeypatch, text: str, secs: float = 1.5) -> None:
    monkeypatch.setattr(app_module, "transcribe", lambda cfg, wav: text)
    started: list[threading.Thread] = []
    real_thread = threading.Thread

    def thread(*args, **kwargs):
        t = real_thread(*args, **kwargs)
        started.append(t)
        return t

    monkeypatch.setattr(app_module.threading, "Thread", thread)
    rate = 16000
    VoiceApp._maybe_wake(app, b"\0" * int(rate * 2 * secs), rate)
    for t in started:
        t.join(2)


def test_en_pausa_la_frase_reactiva(monkeypatch):
    app = _app()
    app.muted = True
    _run_wake(app, monkeypatch, "EzAgent, escúchame")
    assert app.events.get_nowait() == ("wake",)


def test_en_pausa_otra_frase_no_reactiva(monkeypatch):
    app = _app()
    app.muted = True
    _run_wake(app, monkeypatch, "pásame la sal")
    assert app.events.empty()


def test_en_pausa_una_frase_larga_ni_se_transcribe(monkeypatch):
    app = _app()
    app.muted = True
    seen: list = []
    monkeypatch.setattr(app_module, "transcribe", lambda cfg, wav: seen.append(1) or "")
    VoiceApp._maybe_wake(app, b"\0" * 16000 * 2 * 6, 16000)
    assert seen == []


def test_sin_reactivacion_por_voz_no_se_transcribe(monkeypatch):
    app = _app(voice_wake=False)
    app.muted = True
    seen: list = []
    monkeypatch.setattr(app_module, "transcribe", lambda cfg, wav: seen.append(1) or "")
    VoiceApp._maybe_wake(app, b"\0" * 16000 * 2, 16000)
    assert seen == []


def test_on_wake_quita_la_pausa():
    app = _app()
    app.muted = True
    calls: list = []
    app._set_mic_muted = lambda muted, source="": calls.append((muted, source))
    app._speak = lambda text, kind="answer": calls.append(text)
    VoiceApp._on_wake(app)
    assert calls == [(False, "frase"), "Dime."]


def test_cortesias_iniciales_no_impiden_el_atajo():
    app = _app()
    muted: list[bool] = []
    app._set_mic_muted = lambda value, source="": muted.append(value)
    app._pause_message = lambda: "Micrófono en pausa."
    app._speak = lambda text, kind="answer": None
    assert VoiceApp._fast_command(app, "Ok, gracias. Muteate.")
    assert muted == [True]
    app._close_mode = ""
    assert VoiceApp._fast_command(app, "Bueno, cierra la escucha")
    assert app._close_mode == "pause"


def test_agradecimiento_se_responde_en_local():
    app = _app()
    spoken: list[str] = []
    app._speak = lambda text, kind="answer": spoken.append(text)
    for phrase in ("Gracias.", "Ok, gracias.", "Muchas gracias"):
        assert VoiceApp._fast_command(app, phrase)
    assert spoken == ["De nada."] * 3
    # Una petición que empieza con «gracias» sigue yendo al modelo.
    assert not VoiceApp._fast_command(app, "Gracias, ahora abre OpenCode")
    assert not VoiceApp._fast_command(app, "Ok")
