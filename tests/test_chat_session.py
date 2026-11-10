"""Sesión del chat y versión hablada: el hilo completo, en streaming y sin leer símbolos."""

from __future__ import annotations

import threading
from pathlib import Path

import pytest

from ezagent.voice.chat.session import ChatModel
from ezagent.voice.chat.speech import MAX_SPOKEN_CHARS, SCREEN_NOTE, spoken_version
from ezagent.voice.chat.store import ChatStore


@pytest.fixture
def store(tmp_path: Path) -> ChatStore:
    return ChatStore(tmp_path / "chat.jsonl")


# --------------------------------------------------------------- sesión
def test_a_streamed_answer_grows_and_is_saved_when_it_finishes(store: ChatStore) -> None:
    model = ChatModel(store)
    model.add_user("resume el informe", "chat")
    answer = model.begin_assistant()
    assert model.busy
    for piece in ("# Resumen\n\n", "Las ventas ", "subieron."):
        model.append(answer.id, piece)
    _, messages = model.snapshot()
    assert messages[-1].streaming and messages[-1].text.endswith("subieron.")
    assert [e.role for e in store.entries()] == ["user"]  # aún no se guarda
    model.finish(answer.id)
    assert not model.busy
    assert [(e.role, e.source) for e in store.entries()] == [
        ("user", "chat"),
        ("assistant", "chat"),
    ]
    assert store.entries()[-1].text == "# Resumen\n\nLas ventas subieron."


def test_voice_turns_are_part_of_the_same_thread(store: ChatStore) -> None:
    model = ChatModel(store)
    model.add_user("qué hora es", "voice")
    reply = model.begin_assistant("voice")
    model.append(reply.id, "Son las tres.")
    model.finish(reply.id)
    _, messages = model.snapshot()
    assert [(m.role, m.source) for m in messages] == [("user", "voice"), ("assistant", "voice")]


def test_the_version_changes_with_every_change_and_snapshots_are_copies(store: ChatStore) -> None:
    model = ChatModel(store)
    start = model.version
    message = model.add_user("hola")
    assert model.version > start
    version, snap = model.snapshot()
    model.append(message.id, "!")  # no es del asistente, pero cambia el texto
    assert snap[0].text == "hola"  # la copia no cambia
    assert model.version > version


def test_an_open_chat_resumes_the_current_conversation(store: ChatStore) -> None:
    first = ChatModel(store)
    first.add_user("hola")
    reply = first.begin_assistant()
    first.append(reply.id, "¿Qué tal?")
    first.finish(reply.id)
    again = ChatModel(ChatStore(store.path))
    _, messages = again.snapshot()
    assert [m.text for m in messages] == ["hola", "¿Qué tal?"]
    assert not again.busy


def test_a_failed_answer_shows_the_reason_but_is_not_saved_as_an_answer(store: ChatStore) -> None:
    model = ChatModel(store)
    model.add_user("hola")
    reply = model.begin_assistant()
    model.fail(reply.id, "No pude conectar con el proveedor.")
    _, messages = model.snapshot()
    assert messages[-1].error and messages[-1].text == "No pude conectar con el proveedor."
    assert not model.busy
    assert [e.role for e in store.entries()] == ["user"]


def test_cancelling_keeps_what_arrived_or_drops_an_empty_answer(store: ChatStore) -> None:
    model = ChatModel(store)
    model.add_user("a")
    partial = model.begin_assistant()
    model.append(partial.id, "  Respuesta a medias ")
    model.cancel(partial.id)
    empty = model.begin_assistant()
    model.cancel(empty.id)
    _, messages = model.snapshot()
    assert [m.text for m in messages] == ["a", "Respuesta a medias"]
    assert store.entries()[-1].text == "Respuesta a medias"
    assert not model.busy


def test_new_session_clears_the_screen_but_keeps_the_history(store: ChatStore) -> None:
    model = ChatModel(store)
    model.add_user("primera")
    old = store.session
    model.new_session()
    assert model.snapshot()[1] == ()
    model.add_user("segunda")
    assert [s.title for s in model.sessions()] == ["segunda", "primera"]
    model.view_session(old)
    assert model.viewing == old
    assert [m.text for m in model.snapshot()[1]] == ["primera"]
    model.view_session(None)
    assert [m.text for m in model.snapshot()[1]] == ["segunda"]


def test_writing_while_browsing_the_history_returns_to_the_current_conversation(
    store: ChatStore,
) -> None:
    model = ChatModel(store)
    model.add_user("vieja")
    old = store.session
    model.new_session()
    model.view_session(old)
    model.add_user("nueva")
    assert model.viewing is None
    assert [m.text for m in model.snapshot()[1]] == ["nueva"]


def test_viewing_the_current_session_is_not_browsing(store: ChatStore) -> None:
    model = ChatModel(store)
    model.add_user("x")
    model.view_session(store.session)
    assert model.viewing is None


def test_confirmations_are_shown_and_cleared(store: ChatStore) -> None:
    model = ChatModel(store)
    model.ask_confirm("Mover 12 archivos a la papelera", True)
    assert model.confirm is not None and model.confirm.strong
    model.clear_confirm()
    assert model.confirm is None


def test_voice_replies_setting_is_a_simple_switch(store: ChatStore) -> None:
    model = ChatModel(store, voice_replies=False)
    version = model.version
    model.set_voice_replies(True)
    assert model.voice_replies and model.version > version
    version = model.version
    model.set_voice_replies(True)
    assert model.version == version  # sin cambio no hay repintado


def test_the_chat_works_without_a_store() -> None:
    model = ChatModel(None)
    model.add_user("hola")
    reply = model.begin_assistant()
    model.append(reply.id, "ok")
    model.finish(reply.id)
    assert model.sessions() == [] and [m.text for m in model.snapshot()[1]] == ["hola", "ok"]
    assert model.last_assistant_text() == "ok"


def test_llm_and_tts_threads_can_feed_it_at_the_same_time(store: ChatStore) -> None:
    model = ChatModel(store)
    reply = model.begin_assistant()

    def worker(letter: str) -> None:
        for _ in range(200):
            model.append(reply.id, letter)

    threads = [threading.Thread(target=worker, args=(c,)) for c in "abcd"]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    model.finish(reply.id)
    assert len(model.snapshot()[1][0].text) == 800


# ------------------------------------------------------------ versión hablada
def test_plain_prose_is_spoken_as_is() -> None:
    assert spoken_version("Hola, ¿cómo estás? Todo bien.") == "Hola, ¿cómo estás? Todo bien."


def test_formatting_is_not_read_out() -> None:
    said = spoken_version(
        "# Resumen\n\nLas ventas **subieron** un `12 %` según [el informe](https://x.com)."
    )
    assert said == "Resumen. Las ventas subieron un 12 % según el informe."
    assert "*" not in said and "#" not in said and "http" not in said and "`" not in said


def test_lists_are_spoken_item_by_item_and_long_ones_are_cut() -> None:
    said = spoken_version("\n".join(f"- punto {i}" for i in range(1, 9)))
    assert "punto 1." in said and "punto 5." in said and "punto 6" not in said
    assert said.endswith(SCREEN_NOTE)


@pytest.mark.parametrize(
    "markdown",
    [
        "Mira:\n\n| a | b |\n|---|---|\n| 1 | 2 |",
        "Mira:\n\n```python\nprint(1)\n```",
        r"Mira:" + "\n\n" + r"![gráfico](C:\a\b.png)",
        r"Mira:" + "\n\n" + r"[Informe](C:\a\informe.docx)",
    ],
)
def test_tables_code_images_and_documents_stay_on_screen(markdown: str) -> None:
    said = spoken_version(markdown)
    assert said == f"Mira: {SCREEN_NOTE}"
    assert "print" not in said and "|" not in said and "C:" not in said


def test_an_answer_that_is_only_rich_content_still_says_something() -> None:
    assert spoken_version("| a |\n|---|\n| 1 |") == "Te lo dejé en el chat."


def test_long_answers_are_cut_at_a_sentence_boundary() -> None:
    long = " ".join(f"Esta es la frase número {i} del resumen." for i in range(60))
    said = spoken_version(long)
    assert len(said) <= MAX_SPOKEN_CHARS + len(SCREEN_NOTE) + 1
    assert said.endswith(SCREEN_NOTE)
    body = said[: -len(SCREEN_NOTE)].strip()
    assert body.endswith("resumen.") and "frase número 0" in body


def test_one_enormous_sentence_is_cut_at_a_word() -> None:
    said = spoken_version("palabra " * 300)
    assert len(said) < MAX_SPOKEN_CHARS + len(SCREEN_NOTE) + 5
    assert said.endswith(SCREEN_NOTE) and "palabra palabra" in said


def test_empty_input_says_nothing() -> None:
    assert spoken_version("") == ""
    assert spoken_version("   \n\n") == ""
    assert spoken_version("---") == ""


def test_notes_are_visible_but_never_saved(store: ChatStore) -> None:
    model = ChatModel(store)
    model.add_user("hola")
    note = model.add_note("Conversación compactada.")
    version = model.version
    _, messages = model.snapshot()
    assert [(m.role, m.text) for m in messages] == [
        ("user", "hola"),
        ("system", "Conversación compactada."),
    ]
    assert note.role == "system" and note.source == "agent"
    assert [e.role for e in store.entries()] == ["user"]  # no entra en el historial
    again = ChatModel(ChatStore(store.path))
    assert [m.role for m in again.snapshot()[1]] == ["user"]
    assert not model.busy and model.version == version
