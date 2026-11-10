"""F30/F37: historial indexado por sesión y orden por secuencia persistida (sin reloj)."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from ezagent.voice.chat.contexts import MAX_SAVED, ChatContexts
from ezagent.voice.chat.session import ChatModel
from ezagent.voice.chat.store import ChatStore


@pytest.fixture
def frozen_clock(monkeypatch) -> float:
    """Todos los registros nuevos llevan el mismo ``t`` (peor caso para ordenar)."""
    monkeypatch.setattr("ezagent.voice.chat.store.time.time", lambda: 1000.0)
    return 1000.0


def test_every_record_carries_a_persisted_seq(tmp_path: Path, frozen_clock: float) -> None:
    path = tmp_path / "chat.jsonl"
    store = ChatStore(path)
    store.append("user", "hola")
    store.new_session()
    store.resume(1)
    records = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]
    assert [(r.get("event"), r["seq"]) for r in records] == [
        (None, 1),
        ("session", 2),
        ("resume", 3),
    ]


def test_resume_moves_the_old_session_first_without_writing_a_message(
    tmp_path: Path, frozen_clock: float
) -> None:
    path = tmp_path / "chat.jsonl"
    store = ChatStore(path)
    store.append("user", "vieja")
    old = store.session
    store.new_session()
    store.append("user", "nueva")
    newest = store.session
    assert [s.session for s in store.sessions()] == [newest, old]

    assert store.resume(old) is True
    # Sin ningún mensaje nuevo y con todos los timestamps idénticos.
    assert [s.session for s in store.sessions()] == [old, newest]
    assert [e.t for e in store.entries(old)] == [frozen_clock]

    again = ChatStore(path)  # segunda instancia sobre el mismo archivo
    assert again.session == old
    assert [s.session for s in again.sessions()] == [old, newest]


def test_the_order_does_not_depend_on_a_clock_that_goes_backwards(
    tmp_path: Path, monkeypatch
) -> None:
    times = iter([900.0, 800.0, 700.0, 600.0])
    monkeypatch.setattr("ezagent.voice.chat.store.time.time", lambda: next(times))
    store = ChatStore(tmp_path / "chat.jsonl")
    store.append("user", "uno")
    one = store.session
    store.new_session()
    store.append("user", "dos")
    two = store.session
    assert store.resume(one) is True
    assert [e.t for e in store.entries(one)] == [900.0]  # el reloj retrocedió
    assert [s.session for s in ChatStore(store.path).sessions()] == [one, two]


def test_old_files_without_seq_use_the_line_index(tmp_path: Path, frozen_clock: float) -> None:
    path = tmp_path / "chat.jsonl"
    rows = [
        {"v": 1, "t": 1.0, "session": 1, "role": "user", "source": "chat", "text": "uno"},
        {"v": 1, "t": 1.0, "event": "session", "session": 2},
        {"v": 1, "t": 1.0, "session": 2, "role": "user", "source": "chat", "text": "dos"},
        {"v": 1, "t": 1.0, "event": "resume", "session": 1},
    ]
    path.write_text("\n".join(json.dumps(r) for r in rows) + "\n", encoding="utf-8")
    store = ChatStore(path)
    assert [s.session for s in store.sessions()] == [1, 2]
    store.append("user", "tres")
    last = json.loads(path.read_text(encoding="utf-8").splitlines()[-1])
    assert last["seq"] == 5  # 4 líneas antiguas: la siguiente secuencia es 5
    assert [s.session for s in ChatStore(path).sessions()] == [1, 2]


def test_entries_use_the_per_session_index_and_do_not_leak_the_cache(
    tmp_path: Path, frozen_clock: float
) -> None:
    path = tmp_path / "chat.jsonl"
    store = ChatStore(path)
    store.append("user", "a1")
    a = store.session
    store.new_session()
    store.append("user", "b1")
    store.append("assistant", "b2")
    b = store.session
    store.resume(a)
    store.append("user", "a2")

    assert {s: len(v) for s, v in store._by_session.items()} == {a: 2, b: 2}
    copy = store.entries(a)
    copy.clear()
    assert [e.text for e in store.entries(a)] == ["a1", "a2"]

    assert store.delete_session(b) is True
    assert {s: len(v) for s, v in store._by_session.items()} == {a: 2}
    assert [e.text for e in ChatStore(path).entries(a)] == ["a1", "a2"]


class FakeHolder:
    """Un ``Messenger`` mínimo: solo guarda los mensajes del contexto."""

    def __init__(self) -> None:
        self.messages: list[dict] = [{"role": "system", "content": "sys"}]

    def reset_history(self) -> None:
        self.messages = self.messages[:1]

    def export_context(self):
        return list(self.messages), "", ""

    def import_context(self, messages, summary="", last_user=""):
        self.messages = [{"role": "system", "content": "sys"}, *messages]


def test_saved_contexts_are_a_bounded_lru_that_keeps_the_current_one(tmp_path: Path) -> None:
    chat = ChatModel(ChatStore(tmp_path / "chat.jsonl"))
    holder = FakeHolder()
    ctx = ChatContexts(chat, lambda: holder)
    sessions: list[int] = []
    for index in range(MAX_SAVED + 2):
        if index:
            ctx.new_chat()
        chat.add_user(f"tema {index}")
        holder.messages.append({"role": "user", "content": f"tema {index}"})
        sessions.append(chat.current_session)

    # Se guardó cada conversación al dejarla: 9 guardadas, tope 8, la más antigua fuera.
    assert len(ctx.saved) == MAX_SAVED
    assert list(ctx.saved) == sessions[1:-1]
    assert sessions[0] not in ctx.saved

    retaken = sessions[2]
    assert ctx.switch(retaken) is True
    # Al retomar, la que se deja entra y la más antigua sale; la retomada queda al final.
    assert len(ctx.saved) == MAX_SAVED
    assert list(ctx.saved) == [*sessions[3:], retaken]
    assert chat.current_session == retaken

    ctx.new_chat()  # guardar la actual nunca la expulsa
    assert retaken in ctx.saved
    assert len(ctx.saved) == MAX_SAVED
