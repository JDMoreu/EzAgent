"""Chat: borrar conversaciones del historial y un contexto del modelo por conversación."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pytest
from test_chat_ui import WORK, FakeUI, FakeWindow
from test_chat_view import Action, click, kinds, make, reply
from test_chat_window import M, opened

from ezagent.voice.chat.contexts import ChatContexts
from ezagent.voice.chat.session import ChatModel
from ezagent.voice.chat.store import ChatStore
from ezagent.voice.notch import chat_ui as cu
from ezagent.voice.notch import icons
from ezagent.voice.notch.chat_ui import ChatPanel


@pytest.fixture
def clock(monkeypatch):
    now = [100.0]
    monkeypatch.setattr(cu.time, "monotonic", lambda: now[0])
    monkeypatch.setattr(cu, "ChatWindow", FakeWindow)
    monkeypatch.setattr(cu._shell, "work_area", lambda: WORK)
    return now


@pytest.fixture
def panel(tmp_path: Path, clock) -> ChatPanel:
    return ChatPanel(FakeUI(ChatModel(ChatStore(tmp_path / "chat.jsonl"))))


class FakeHolder:
    """Un ``Messenger`` de mentira: guarda mensajes, resumen y último texto."""

    def __init__(self) -> None:
        self.messages: list[dict] = [{"role": "system", "content": "sys"}]
        self.summary = ""
        self.last_user = ""

    def reset_history(self) -> None:
        self.messages = self.messages[:1]
        self.summary = self.last_user = ""

    def export_context(self):
        return self.messages, self.summary, self.last_user

    def import_context(self, messages, summary="", last_user=""):
        body = [m for m in messages if m["role"] != "system"]  # igual que el Messenger real
        self.messages = [{"role": "system", "content": "sys"}, *body]
        self.summary, self.last_user = summary, last_user

    def say(self, user: str, answer: str) -> None:
        self.messages += [
            {"role": "user", "content": user},
            {"role": "assistant", "content": answer},
        ]
        self.last_user = user


def rig(tmp_path: Path):
    chat = ChatModel(ChatStore(tmp_path / "chat.jsonl"))
    holder = FakeHolder()
    return chat, holder, ChatContexts(chat, lambda: holder)


def bodies(holder: FakeHolder) -> list[str]:
    return [m["content"] for m in holder.messages[1:]]


# ------------------------------------------------------------------ el almacén
def test_a_conversation_can_be_deleted_and_the_file_is_rewritten(tmp_path: Path) -> None:
    store = ChatStore(tmp_path / "c.jsonl")
    store.append("user", "primera")
    first = store.session
    store.new_session()
    store.append("user", "segunda")
    assert store.delete_session(first) is True
    assert [s.title for s in store.sessions()] == ["segunda"]
    again = ChatStore(tmp_path / "c.jsonl")  # también en disco
    assert [s.title for s in again.sessions()] == ["segunda"] and again.session == store.session
    assert not list(tmp_path.glob("*.tmp"))  # sin restos del archivo temporal


def test_deleting_what_does_not_exist_changes_nothing(tmp_path: Path) -> None:
    store = ChatStore(tmp_path / "c.jsonl")
    store.append("user", "hola")
    assert store.delete_session(99) is False and len(store) == 1


def test_deleting_the_current_conversation_leaves_it_empty_but_current(tmp_path: Path) -> None:
    store = ChatStore(tmp_path / "c.jsonl")
    store.append("user", "hola")
    current = store.session
    assert store.delete_session(current) and store.session == current
    assert store.entries() == [] and store.sessions() == []
    store.append("user", "otra vez")
    assert store.entries()[0].session == current


def test_a_failed_rewrite_keeps_the_conversation(tmp_path: Path, monkeypatch) -> None:
    store = ChatStore(tmp_path / "c.jsonl")
    store.append("user", "hola")
    monkeypatch.setattr("os.replace", lambda *a: (_ for _ in ()).throw(OSError("disco lleno")))
    assert store.delete_session(store.session) is False
    assert [e.text for e in store.entries()] == ["hola"]


def test_resuming_an_old_conversation_makes_it_current_even_after_a_restart(tmp_path: Path) -> None:
    store = ChatStore(tmp_path / "c.jsonl")
    store.append("user", "vieja")
    old = store.session
    store.new_session()
    store.append("user", "nueva")
    assert store.resume(old) and store.session == old
    store.append("assistant", "sigue la vieja")
    assert [e.text for e in store.entries()] == ["vieja", "sigue la vieja"]
    assert ChatStore(tmp_path / "c.jsonl").session == old
    assert store.resume(77) is False
    assert store.sessions()[0].session == old  # la usada hace menos va primero


def test_the_current_choice_survives_deleting_another_conversation(tmp_path: Path) -> None:
    store = ChatStore(tmp_path / "c.jsonl")
    store.append("user", "uno")
    one = store.session
    store.new_session()
    store.append("user", "dos")
    two = store.session
    store.resume(one)
    store.delete_session(two)
    assert ChatStore(tmp_path / "c.jsonl").session == one


# ----------------------------------------------------------------- el modelo
def test_switching_loads_that_conversation_on_screen(tmp_path: Path) -> None:
    chat, _, _ = rig(tmp_path)
    reply(chat, "tema A", "respuesta A")
    a = chat.current_session
    chat.new_session()
    reply(chat, "tema B", "respuesta B")
    assert chat.switch_session(a) is True
    assert [m.text for m in chat.snapshot()[1]] == ["tema A", "respuesta A"]
    reply(chat, "sigo en A", "claro")  # lo nuevo se añade a esa conversación
    assert [e.text for e in chat.store.entries(a)][-2:] == ["sigo en A", "claro"]


def test_it_will_not_switch_in_the_middle_of_an_answer(tmp_path: Path) -> None:
    chat, _, _ = rig(tmp_path)
    reply(chat, "a", "b")
    old = chat.current_session
    chat.new_session()
    chat.begin_assistant()  # respondiendo
    assert chat.switch_session(old) is False and chat.delete_session(chat.current_session) is False


def test_deleting_a_past_conversation_leaves_the_screen_alone(tmp_path: Path) -> None:
    chat, _, _ = rig(tmp_path)
    reply(chat, "vieja", "x")
    old = chat.current_session
    chat.new_session()
    reply(chat, "actual", "y")
    assert chat.delete_session(old) is True
    assert [m.text for m in chat.snapshot()[1]] == ["actual", "y"]
    assert [s.title for s in chat.sessions()] == ["actual"]


def test_deleting_the_current_conversation_clears_the_screen(tmp_path: Path) -> None:
    chat, _, _ = rig(tmp_path)
    reply(chat, "actual", "y")
    assert chat.delete_session(chat.current_session) is True
    assert chat.snapshot()[1] == ()


# ------------------------------------------------- un contexto por conversación
def test_each_conversation_keeps_its_own_model_context(tmp_path: Path) -> None:
    chat, holder, ctx = rig(tmp_path)
    reply(chat, "mi color es azul", "anotado")
    holder.say("mi color es azul", "anotado")
    a = chat.current_session
    ctx.new_chat()
    assert bodies(holder) == []  # una nueva empieza de cero
    reply(chat, "mi color es rojo", "listo")
    holder.say("mi color es rojo", "listo")
    assert ctx.switch(a) is True
    assert bodies(holder) == ["mi color es azul", "anotado"]  # no se mezcló con la otra
    holder.say("¿y mi color?", "azul")
    b = next(s.session for s in chat.sessions() if s.session != a)
    ctx.switch(b)
    assert bodies(holder) == ["mi color es rojo", "listo"]
    ctx.switch(a)
    assert bodies(holder)[-2:] == ["¿y mi color?", "azul"]  # volvió exactamente donde estaba


def test_the_summary_travels_with_its_conversation(tmp_path: Path) -> None:
    chat, holder, ctx = rig(tmp_path)
    reply(chat, "hola", "buenas")
    a = chat.current_session
    holder.summary = "Resumen de A"
    ctx.new_chat()
    assert holder.summary == ""
    ctx.switch(a)
    assert holder.summary == "Resumen de A"


def test_without_saved_context_it_is_rebuilt_from_the_history(tmp_path: Path) -> None:
    chat, holder, ctx = rig(tmp_path)
    reply(chat, "recuérdame esto", "hecho")
    a = chat.current_session
    chat.new_session()  # el contexto de A no se guardó (p. ej. otra ejecución)
    reply(chat, "otra", "cosa")
    assert ctx.switch(a) is True
    assert bodies(holder) == ["recuérdame esto", "hecho"] and holder.last_user == "recuérdame esto"


def test_startup_rebuilds_the_context_of_the_open_conversation(tmp_path: Path) -> None:
    chat, holder, ctx = rig(tmp_path)
    reply(chat, "ayer hablamos", "sí")
    ctx.rebuild()
    assert bodies(holder) == ["ayer hablamos", "sí"]


def test_switching_to_the_current_or_a_missing_conversation_keeps_the_context(
    tmp_path: Path,
) -> None:
    chat, holder, ctx = rig(tmp_path)
    reply(chat, "hola", "buenas")
    holder.say("hola", "buenas")
    assert ctx.switch(chat.current_session) is False and ctx.switch(404) is False
    assert bodies(holder) == ["hola", "buenas"]


def test_deleting_a_conversation_drops_its_context(tmp_path: Path) -> None:
    chat, holder, ctx = rig(tmp_path)
    reply(chat, "vieja", "x")
    holder.say("vieja", "x")
    old = chat.current_session
    ctx.new_chat()
    reply(chat, "actual", "y")
    holder.say("actual", "y")
    assert old in ctx.saved
    assert ctx.delete(old) is True and old not in ctx.saved
    assert bodies(holder) == ["actual", "y"]  # el modelo sigue con la actual
    assert ctx.delete(chat.current_session) is True
    assert bodies(holder) == []  # borrar la actual deja al modelo en blanco


def test_the_real_messenger_swaps_contexts_with_a_fresh_system_prompt() -> None:
    from ezagent.voice.llm import Messenger

    class Stub:
        _system_prompt = Messenger._system_prompt
        import_context = Messenger.import_context
        export_context = Messenger.export_context
        reset_history = Messenger.reset_history

    stub = Stub.__new__(Stub)
    stub._system_prompt = lambda: "prompt nuevo"  # type: ignore[method-assign]
    stub.messages = [{"role": "system", "content": "viejo"}]
    stub.summary, stub._last_user_text = "", ""
    stub.import_context(
        [{"role": "system", "content": "otro"}, {"role": "user", "content": "hola"}], "res", "hola"
    )
    assert stub.messages == [
        {"role": "system", "content": "prompt nuevo"},
        {"role": "user", "content": "hola"},
    ]
    assert stub.export_context() == (stub.messages, "res", "hola")


# ------------------------------------------------------- historial en la vista
def history_with_two(tmp_path: Path):
    model, view = make(tmp_path)
    reply(model, "vieja", "x")
    old = model.current_session
    model.new_session()
    reply(model, "actual", "y")
    click(view, "history")
    view.render(1.0)
    return model, view, old


def test_every_history_row_has_a_trash_button(tmp_path: Path) -> None:
    model, view, old = history_with_two(tmp_path)
    assert [d for _r, k, d in view._hits if k == "delete_ask"] == [model.current_session, old]
    assert {"session", "delete_ask"} <= kinds(view)


def test_deleting_asks_first_and_only_the_second_click_deletes(tmp_path: Path) -> None:
    model, view, old = history_with_two(tmp_path)
    assert click(view, "delete_ask", old) is None
    assert view.pending_delete == old
    view.render(1.0)
    assert {"delete_yes", "delete_no"} <= kinds(view)
    assert [d for _r, k, d in view._hits if k == "delete_ask"] == [model.current_session]  # la otra
    assert click(view, "delete_yes", old) == Action("delete", old)
    assert view.pending_delete is None


def test_cancelling_the_confirmation_deletes_nothing(tmp_path: Path) -> None:
    _, view, old = history_with_two(tmp_path)
    click(view, "delete_ask", old)
    assert click(view, "delete_no", old) is None and view.pending_delete is None
    click(view, "delete_ask", old)
    assert view._escape() is None and view.pending_delete is None and view.mode == "history"


def test_the_deleted_row_fades_out_instead_of_vanishing(tmp_path: Path) -> None:
    model, view, old = history_with_two(tmp_path)
    click(view, "delete_ask", old)
    assert click(view, "delete_yes", old) == Action("delete", old)
    model.delete_session(old)
    assert view._deleting is not None
    start = view._deleting[2]
    assert view.animating(start + 0.05)
    view.render(start + 0.05)
    assert view._deleting is not None  # la fila fantasma sigue en pantalla
    view.render(start + 0.3)
    assert view._deleting is None and not view.animating(start + 0.3)


def test_leaving_the_history_cancels_a_pending_delete(tmp_path: Path) -> None:
    _, view, old = history_with_two(tmp_path)
    click(view, "delete_ask", old)
    click(view, "history")
    assert view.pending_delete is None


def test_opening_the_current_conversation_just_goes_back_to_the_chat(tmp_path: Path) -> None:
    model, view, _ = history_with_two(tmp_path)
    assert click(view, "session", model.current_session) is None and view.mode == "chat"


def test_while_the_model_answers_the_history_is_locked(tmp_path: Path) -> None:
    model, view, _ = history_with_two(tmp_path)
    model.begin_assistant()
    view.render(1.0)
    assert not {"session", "delete_ask"} & kinds(view)


def test_the_current_conversation_is_marked(tmp_path: Path, monkeypatch) -> None:
    _, view, _ = history_with_two(tmp_path)
    texts: list[str] = []
    real = view._text
    monkeypatch.setattr(
        view, "_text", lambda b, t, *a, **k: (texts.append(t), real(b, t, *a, **k))[1]
    )
    view.render(1.0)
    assert texts.count("ACTUAL") == 1 and "¿Eliminar esta conversación?" not in texts
    click(view, "delete_ask", next(d for _r, k, d in view._hits if k == "delete_ask"))
    texts.clear()
    view.render(1.0)
    assert "¿Eliminar esta conversación?" in texts


# ------------------------------------------------------ el panel reparte acciones
def test_the_panel_routes_switch_and_delete_to_the_app(panel, clock) -> None:
    opened(panel, clock)
    panel.ui.calls.clear()
    panel._perform(cu.Action("switch", 3))
    panel._perform(cu.Action("delete", 4))
    assert panel.ui.calls == [("switch", 3), ("delete", 4)]


# ----------------------------------------------------------------- los iconos
def test_the_trash_icon_draws_and_differs_from_the_others() -> None:
    def sprite(name: str) -> np.ndarray:
        buf = np.zeros((48, 48, 4), np.float32)
        icons.draw_icon(buf, name, 24 / 1.0 / 2, 12, 18.0, (200, 200, 200), 2.0)
        return buf

    assert sprite("trash")[..., 3].max() > 0.5
    assert not np.array_equal(sprite("trash"), sprite("copy"))


def test_the_copy_button_in_code_blocks_is_an_icon_not_text() -> None:
    from ezagent.voice.chat import render as R
    from ezagent.voice.notch.bar_theme import card_palette

    pal = card_palette("dark")
    out = R.render_message("```py\nprint(1)\n```", pal, 380.0, 1.0)
    hit = next(h for h in out.hits if h.kind == "copy_code")
    assert hit.rect[2] - hit.rect[0] <= 40 and hit.data == "print(1)"  # zona de un icono


# ------------------------------------------------------------------ el orbe fijo
def test_the_orb_keeps_a_fixed_size_whatever_the_bar_is_doing(panel, clock) -> None:
    opened(panel, clock)
    sizes = set()
    for act in (0.0, 0.5, 1.0):
        panel._act = act
        for step in range(8):
            now = 5.0 + step * 0.13
            layers = panel._live_layers(300, 600, now, 1.0, np.ones(1, np.float32))
            sizes.add(layers[0][2].shape[:2])
    assert len(sizes) == 1  # antes la esfera latía (cambiaba de tamaño con la actividad)
    side = round(panel.view.orb[2] * panel.ui.s)
    assert sizes == {(side, side)}


def test_the_orb_has_no_halo_around_it(panel, clock) -> None:
    opened(panel, clock)
    panel._act = 1.0
    x, y, orb = panel._live_layers(300, 600, 5.0, 1.0, np.ones(1, np.float32))[0]
    assert orb[0, 0, 3] == 0 and orb[0, -1, 3] == 0  # las esquinas están vacías
    assert orb[orb.shape[0] // 2, orb.shape[1] // 2, 3] == pytest.approx(1.0)


def test_the_orb_slot_keeps_its_place_in_the_header(panel, clock) -> None:
    opened(panel, clock)
    cx, cy, d = panel.view.orb
    assert (cx, cy, d) == (16 + 14, 28, 28)
    assert M == round(cu.MARGIN)
