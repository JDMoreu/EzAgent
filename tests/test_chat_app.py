"""La app y el chat: lo escrito es un turno más; la voz es opcional; lo hablado también se ve."""

from __future__ import annotations

import queue
import threading
from pathlib import Path
from types import SimpleNamespace

import pytest

from ezagent.voice.chat.session import ChatModel
from ezagent.voice.chat.store import ChatStore

app = pytest.importorskip("ezagent.voice.app")


class FakeThread:
    started: list[tuple] = []

    def __init__(self, target=None, args=(), daemon=None, **_) -> None:
        self.target, self.args = target, args

    def start(self) -> None:
        FakeThread.started.append((self.target, self.args))


class Messenger:
    def __init__(self, *deltas, boom: Exception | None = None) -> None:
        self.deltas, self.boom, self.modes = deltas, boom, []

    def ask_stream(self, text: str, mode: str = "voice"):
        self.modes.append(mode)
        yield from self.deltas
        if self.boom:
            raise self.boom


class Speaker:
    def __init__(self) -> None:
        self.said: list[str] = []

    def speak(self, text, stop_event, on_level=None) -> None:
        self.said.append(text)


def make(tmp_path: Path, *deltas, voice: bool = False, boom=None, store: bool = True):
    chat = ChatModel(ChatStore(tmp_path / "chat.jsonl") if store else None, voice_replies=voice)
    events: queue.Queue = queue.Queue()
    states: list[str] = []
    stub = SimpleNamespace(
        chat=chat,
        events=events,
        messenger=Messenger(*deltas, boom=boom),
        speaker=Speaker(),
        notch=SimpleNamespace(draw=lambda level, state: None),
        cfg=SimpleNamespace(
            chat_voice=voice,
            llm_provider="openai",
            llm_model="gpt-x",
            save_key=lambda key, value: states.append(f"{key}={value}"),
        ),
        agent=None,
        last_summary="",
        _turn_gen=4,
        _speaking_kind=None,
        _signal_tts_stop=lambda: states.append("tts-stop"),
        _set_state=lambda state: states.append(state),
        _new_tts_stop=lambda: threading.Event(),
        _llm_label=lambda: "OpenAI",
        _chat_worker=lambda text, gen: None,
    )
    return stub, states


@pytest.fixture(autouse=True)
def _threads(monkeypatch):
    FakeThread.started = []
    monkeypatch.setattr(app.threading, "Thread", FakeThread)
    monkeypatch.setattr(app, "log", lambda message: None)


def _events(stub) -> list[tuple]:
    out = []
    while not stub.events.empty():
        out.append(stub.events.get_nowait())
    return out


# ------------------------------------------------------------- enviar texto
def test_typing_a_message_starts_a_new_turn_like_a_spoken_one(tmp_path: Path) -> None:
    stub, states = make(tmp_path)
    app.VoiceApp._chat_send(stub, "  resume el informe  ")
    assert stub._turn_gen == 5  # invalida lo que estuviera sonando o pensándose
    assert "tts-stop" in states and "thinking" in states
    ((target, args),) = FakeThread.started
    assert target == stub._chat_worker
    assert args == ("resume el informe", 5)
    _, messages = stub.chat.snapshot()
    assert [(m.role, m.source, m.text) for m in messages] == [("user", "chat", "resume el informe")]


def test_empty_messages_are_ignored(tmp_path: Path) -> None:
    stub, _ = make(tmp_path)
    for text in ("", "   ", None):
        app.VoiceApp._chat_send(stub, text)  # type: ignore[arg-type]
    assert FakeThread.started == [] and stub._turn_gen == 4 and stub.chat.snapshot()[1] == ()


def test_writing_yes_answers_a_pending_confirmation_instead_of_starting_a_turn(
    tmp_path: Path,
) -> None:
    stub, _ = make(tmp_path)
    answered: list[str] = []
    stub.agent = SimpleNamespace(confirm_pending=True, submit_confirmation=answered.append)
    app.VoiceApp._chat_send(stub, "confirmo 3")
    assert answered == ["confirmo 3"] and FakeThread.started == [] and stub._turn_gen == 4
    assert [m.text for m in stub.chat.snapshot()[1]] == ["confirmo 3"]


# --------------------------------------------------------------- turno
def test_the_answer_streams_into_the_chat_in_chat_mode_and_stays_silent_by_default(
    tmp_path: Path,
) -> None:
    stub, _ = make(tmp_path, "## Resumen\n\n", "Ventas **+12 %**.")
    app.VoiceApp._chat_worker(stub, "resume", 4)
    assert stub.messenger.modes == ["chat"]
    _, messages = stub.chat.snapshot()
    assert messages[-1].text == "## Resumen\n\nVentas **+12 %**." and not messages[-1].streaming
    assert stub.speaker.said == []  # el usuario no pidió voz
    assert _events(stub) == [("answer_done", 4)]
    assert stub.last_summary == "Resumen. Ventas +12 %."  # «repetir» lee la versión hablada


def test_with_voice_on_a_written_question_gets_a_spoken_answer(tmp_path: Path) -> None:
    stub, _ = make(tmp_path, "Las ventas subieron un 12 %.", voice=True)
    app.VoiceApp._chat_worker(stub, "cómo van las ventas", 4)
    assert stub.speaker.said == ["Las ventas subieron un 12 %."]
    assert ("speaking", 4) in _events(stub)


def test_spoken_answers_skip_tables_and_code_and_point_to_the_chat(tmp_path: Path) -> None:
    stub, _ = make(
        tmp_path, "Te dejo la comparativa.\n\n| a | b |\n|---|---|\n| 1 | 2 |", voice=True
    )
    app.VoiceApp._chat_worker(stub, "compara", 4)
    assert stub.speaker.said == ["Te dejo la comparativa. El detalle está en el chat."]


def test_voice_can_be_switched_on_between_two_questions(tmp_path: Path) -> None:
    stub, _ = make(tmp_path, "Hola.")
    app.VoiceApp._chat_worker(stub, "hola", 4)
    assert stub.speaker.said == []
    stub.chat.set_voice_replies(True)
    stub.messenger.deltas = ("Adiós.",)
    app.VoiceApp._chat_worker(stub, "adiós", 4)
    assert stub.speaker.said == ["Adiós."]


def test_a_newer_turn_interrupts_the_old_one_and_keeps_what_arrived(tmp_path: Path) -> None:
    stub, _ = make(tmp_path, "primera ", "mitad ", "de la respuesta")

    def supersede(delta: str):  # el usuario escribe otra cosa a mitad del streaming
        stub._turn_gen += 1

    original = stub.chat.append

    def append(message_id, delta):
        original(message_id, delta)
        if delta == "primera ":
            supersede(delta)

    stub.chat.append = append  # type: ignore[method-assign]
    app.VoiceApp._chat_worker(stub, "x", 4)
    _, messages = stub.chat.snapshot()
    assert messages[-1].text == "primera" and not messages[-1].streaming  # conserva lo recibido
    assert stub.speaker.said == []


def test_a_provider_error_is_shown_in_the_chat_and_not_stored_as_an_answer(tmp_path: Path) -> None:
    stub, _ = make(tmp_path, boom=RuntimeError("se cayó"))
    app.VoiceApp._chat_worker(stub, "hola", 4)
    _, messages = stub.chat.snapshot()
    assert messages[-1].error and "se cayó" in messages[-1].text
    assert [e.role for e in stub.chat.store.entries()] == []  # solo se guardan respuestas buenas
    assert _events(stub) == [("answer_done", 4)]


def test_an_empty_answer_leaves_no_blank_bubble(tmp_path: Path) -> None:
    stub, _ = make(tmp_path)
    app.VoiceApp._chat_worker(stub, "hola", 4)
    _, messages = stub.chat.snapshot()
    assert messages[-1].error and "no devolvió respuesta" in messages[-1].text
    assert stub.speaker.said == [] and _events(stub) == [("answer_done", 4)]
    assert stub.chat.store.entries() == []  # un fallo no se guarda como respuesta


# -------------------------------------------------- lo hablado también se ve
def test_a_spoken_turn_is_mirrored_into_the_chat(tmp_path: Path) -> None:
    stub, _ = make(tmp_path, "Son ", "las tres.")
    stub.chat.add_user("qué hora es", "voice")
    sentences_said: list[str] = []
    stub._speech_queue = lambda stop, timing: SimpleNamespace(
        say=sentences_said.append, finish=lambda: None, abort=lambda: None
    )
    stub._turn_timing = None
    app.VoiceApp._answer_worker(stub, "qué hora es", 4)
    _, messages = stub.chat.snapshot()
    assert [(m.role, m.source, m.text) for m in messages] == [
        ("user", "voice", "qué hora es"),
        ("assistant", "voice", "Son las tres."),
    ]
    assert sentences_said == ["Son las tres."]  # la voz sigue funcionando como siempre


def test_a_failed_spoken_turn_shows_the_reason_in_the_chat(tmp_path: Path) -> None:
    stub, _ = make(tmp_path, boom=RuntimeError("sin red"))
    stub._speech_queue = lambda stop, timing: SimpleNamespace(
        say=lambda s: None, finish=lambda: None, abort=lambda: None
    )
    stub._turn_timing = None
    app.VoiceApp._answer_worker(stub, "hola", 4)
    _, messages = stub.chat.snapshot()
    assert messages[-1].error and "sin red" in messages[-1].text
    assert any(event[0] == "error" for event in _events(stub))


# ---------------------------------------------------------------- ajustes
def test_the_voice_switch_applies_now_and_is_saved(tmp_path: Path) -> None:
    stub, states = make(tmp_path)
    app.VoiceApp._set_chat_voice(stub, True)
    assert stub.cfg.chat_voice is True and stub.chat.voice_replies is True
    assert "chat_voice=True" in states
    app.VoiceApp._set_chat_voice(stub, False)
    assert stub.chat.voice_replies is False and "chat_voice=False" in states


def test_the_settings_switch_updates_the_open_chat(tmp_path: Path) -> None:
    stub, _ = make(tmp_path)
    stub.cfg.save_key = lambda key, value: None
    stub.notch = SimpleNamespace()
    stub._push_custom_colors = lambda: None
    app.VoiceApp._apply_instant_setting(stub, "chat_voice", True, persist=False)
    assert stub.chat.voice_replies is True


def test_the_chat_hotkey_opens_the_chat_even_with_the_bar_hidden(tmp_path: Path) -> None:
    stub, _ = make(tmp_path)
    calls: list[str] = []
    stub._show_bar_if_hidden = lambda: calls.append("show")
    stub._toggle_chat = lambda: calls.append("toggle")
    app.VoiceApp._handle_hotkey(stub, "chat")
    assert calls == ["show", "toggle"]


def test_a_hidden_bar_comes_back_for_the_chat(tmp_path: Path) -> None:
    stub, states = make(tmp_path)
    shown: list[str] = []
    stub._notch_hidden = True
    stub.notch = SimpleNamespace(show=shown.append)
    app.VoiceApp._show_bar_if_hidden(stub)
    assert stub._notch_hidden is False and shown == [app.IDLE] and "notch_hidden=False" in states


def test_toggle_chat_never_raises_if_the_window_is_not_ready(tmp_path: Path) -> None:
    stub, _ = make(tmp_path)
    stub.notch = SimpleNamespace()  # sin toggle_chat
    app.VoiceApp._toggle_chat(stub)
    opened: list[bool] = []
    stub.notch = SimpleNamespace(toggle_chat=lambda: opened.append(True))
    app.VoiceApp._toggle_chat(stub)
    assert opened == [True]


def test_chat_hotkey_is_part_of_the_checked_hotkeys() -> None:
    from ezagent.voice.notch.hotkey_field import HOTKEY_KEYS

    assert "chat_hotkey" in app.HOTKEY_KEYS and "chat_hotkey" in HOTKEY_KEYS
    assert "chat_voice" in app.INSTANT_SETTINGS
