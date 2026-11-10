"""Historial del chat: persistente, de solo añadir y tolerante a archivos dañados."""

from __future__ import annotations

import json
from pathlib import Path

from ezagent.voice.chat.store import MAX_ENTRY_CHARS, ChatStore, as_messages, default_chat_path


def test_entries_survive_a_restart(tmp_path: Path) -> None:
    path = tmp_path / "h" / "chat.jsonl"
    store = ChatStore(path)
    store.append("user", "hola", "chat", t=1.0)
    store.append("assistant", "¡Hola! ¿En qué te ayudo?", "chat", t=2.0)
    again = ChatStore(path)
    assert [(e.role, e.text) for e in again.entries()] == [
        ("user", "hola"),
        ("assistant", "¡Hola! ¿En qué te ayudo?"),
    ]
    assert again.session == store.session


def test_the_file_is_plain_jsonl_and_unicode_is_kept(tmp_path: Path) -> None:
    store = ChatStore(tmp_path / "chat.jsonl")
    store.append("user", "añade café ☕ y ñandú", "voice", t=5.0)
    lines = (tmp_path / "chat.jsonl").read_text(encoding="utf-8").splitlines()
    assert len(lines) == 1
    record = json.loads(lines[0])
    assert record["text"] == "añade café ☕ y ñandú" and record["source"] == "voice"
    assert "\\u" not in lines[0]  # legible, no escapado


def test_new_session_keeps_the_old_ones(tmp_path: Path) -> None:
    store = ChatStore(tmp_path / "chat.jsonl")
    store.append("user", "primera", t=1.0)
    first = store.session
    second = store.new_session()
    store.append("user", "segunda", t=2.0)
    assert second == first + 1
    assert [e.text for e in store.entries()] == ["segunda"]
    assert [e.text for e in store.entries(first)] == ["primera"]
    again = ChatStore(tmp_path / "chat.jsonl")
    assert again.session == second and [e.text for e in again.entries(first)] == ["primera"]


def test_sessions_are_listed_newest_first_with_a_title(tmp_path: Path) -> None:
    store = ChatStore(tmp_path / "chat.jsonl")
    store.append("assistant", "bienvenida", t=1.0)
    store.append(
        "user", "resúmeme   el informe\ntrimestral de ventas por región y por canal de venta", t=2.0
    )
    store.new_session()
    store.append("user", "otra cosa", t=3.0)
    sessions = store.sessions()
    assert [s.title for s in sessions][0] == "otra cosa"
    assert sessions[1].messages == 2
    assert sessions[1].title.startswith("resúmeme el informe trimestral")
    assert len(sessions[1].title) <= 60 and sessions[1].title.endswith("…")


def test_empty_sessions_are_not_listed(tmp_path: Path) -> None:
    store = ChatStore(tmp_path / "chat.jsonl")
    store.new_session()
    store.new_session()
    assert store.sessions() == []


def test_a_damaged_file_loads_what_it_can(tmp_path: Path) -> None:
    path = tmp_path / "chat.jsonl"
    good = json.dumps(
        {"v": 1, "t": 1.0, "session": 1, "role": "user", "source": "chat", "text": "ok"}
    )
    path.write_text(f'{good}\nesto no es json\n{{"session": 1}}\n\n{good}\n', encoding="utf-8")
    store = ChatStore(path)
    assert [e.text for e in store.entries()] == ["ok", "ok"]


def test_unknown_roles_and_sources_are_handled(tmp_path: Path) -> None:
    path = tmp_path / "chat.jsonl"
    rows = [
        {"t": 1, "session": 1, "role": "hacker", "text": "x"},
        {"t": 2, "session": 1, "role": "user", "source": "raro", "text": "y"},
    ]
    path.write_text("\n".join(json.dumps(r) for r in rows), encoding="utf-8")
    store = ChatStore(path)
    assert [(e.role, e.source, e.text) for e in store.entries()] == [("user", "chat", "y")]


def test_huge_messages_are_capped(tmp_path: Path) -> None:
    store = ChatStore(tmp_path / "chat.jsonl")
    entry = store.append("assistant", "x" * (MAX_ENTRY_CHARS + 500))
    assert len(entry.text) == MAX_ENTRY_CHARS


def test_search_is_case_insensitive_and_newest_first(tmp_path: Path) -> None:
    store = ChatStore(tmp_path / "chat.jsonl")
    store.append("user", "Presupuesto de marzo", t=1.0)
    store.append("assistant", "el PRESUPUESTO sube", t=2.0)
    store.append("user", "otra cosa", t=3.0)
    assert [e.t for e in store.search("presupuesto")] == [2.0, 1.0]
    assert store.search("") == [] and store.search("no existe") == []
    assert len(store.search("presupuesto", limit=1)) == 1


def test_recent_returns_the_tail_across_sessions(tmp_path: Path) -> None:
    store = ChatStore(tmp_path / "chat.jsonl")
    for index in range(5):
        store.append("user", str(index), t=float(index))
        if index == 2:
            store.new_session()
    assert [e.text for e in store.recent(3)] == ["2", "3", "4"]
    assert len(store) == 5


def test_a_failing_disk_does_not_break_the_chat(tmp_path: Path) -> None:
    blocker = tmp_path / "es-un-archivo"
    blocker.write_text("x", encoding="utf-8")
    store = ChatStore(blocker / "chat.jsonl")  # el «directorio» padre es un archivo
    entry = store.append("user", "sigue funcionando")
    assert entry in store.entries()


def test_as_messages_keeps_only_user_and_assistant(tmp_path: Path) -> None:
    store = ChatStore(tmp_path / "chat.jsonl")
    store.append("system", "nota")
    store.append("user", "a")
    store.append("assistant", "b")
    assert as_messages(store.entries()) == [
        {"role": "user", "content": "a"},
        {"role": "assistant", "content": "b"},
    ]


def test_default_path_lives_next_to_the_rest_of_the_app_data(monkeypatch, tmp_path: Path) -> None:
    monkeypatch.setenv("APPDATA", str(tmp_path))
    assert default_chat_path() == tmp_path / "EzAgent" / "history" / "chat.jsonl"
