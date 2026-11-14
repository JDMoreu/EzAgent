"""Fixtures del historial de chat, la conversación en pantalla y sus contextos.

Dominio `chat_store` (F1-07). Cubre:

- `src/ezagent/voice/chat/store.py`
- `src/ezagent/voice/chat/session.py`
- `src/ezagent/voice/chat/contexts.py`

Formato: escenarios de operaciones sobre un archivo `chat.jsonl` temporal. La
salida incluye el resultado de cada operación, el estado observable del store o
del modelo y el **texto exacto** del archivo. El reloj está congelado en
`2025-01-01T00:00:00Z`; las rutas del perfil temporal se normalizan a `<tmp>`.
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "chat_store"


def _norm(value: Any, home: Path) -> Any:
    if isinstance(value, str):
        value = value.replace("\\\\", "/")
        for prefix in (home.as_posix(), str(home)):
            value = value.replace(prefix, "<tmp>")
        return value.replace("\\", "/")
    if isinstance(value, dict):
        return {key: _norm(item, home) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_norm(item, home) for item in value]
    return value


def _write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")


def _file_text(path: Path) -> Any:
    if path.is_dir():
        return "<dir>"
    if not path.exists():
        return None
    return path.read_text(encoding="utf-8", errors="replace")


def _entry(entry: Any) -> dict[str, Any]:
    return {
        "t": entry.t,
        "session": entry.session,
        "role": entry.role,
        "source": entry.source,
        "text": entry.text,
        "seq": entry.seq,
    }


def _session_info(info: Any) -> dict[str, Any]:
    return {
        "session": info.session,
        "started": info.started,
        "messages": info.messages,
        "title": info.title,
    }


def _message(message: Any) -> dict[str, Any]:
    return {
        "id": message.id,
        "role": message.role,
        "text": message.text,
        "source": message.source,
        "t": message.t,
        "streaming": message.streaming,
        "error": message.error,
    }


# ---------------------------------------------------------------- store


class _StoreRun:
    def __init__(self, out: FixtureFile, home: Path, case: str, seed: str | None) -> None:
        from ezagent.voice.chat.store import ChatStore

        self.out = out
        self.case = case
        self.seed = seed
        self.work = home / "chat-store-cases" / case
        self.path = self.work / "chat.jsonl"
        if seed == "<dir>":
            self.path.mkdir(parents=True, exist_ok=True)
        elif seed is not None:
            _write(self.path, seed)
        self.store = ChatStore(self.path)
        self.ops: list[dict[str, Any]] = []
        self.results: list[Any] = []

    def run(self, op: dict[str, Any]) -> Any:
        from ezagent.voice.chat.store import ChatStore

        self.ops.append(op)
        action = op["op"]
        store = self.store
        try:
            if action == "append":
                entry = store.append(
                    op.get("role", "user"),
                    op.get("text", ""),
                    op.get("source", "chat"),
                    op.get("t"),
                )
                return _entry(entry)
            if action == "replace_last":
                return store.replace_last(
                    op.get("role", "user"), op.get("old", ""), op.get("new", "")
                )
            if action == "new_session":
                return store.new_session()
            if action == "resume":
                return store.resume(op["session"])
            if action == "delete_session":
                return store.delete_session(op["session"])
            if action == "entries":
                return [_entry(entry) for entry in store.entries(op.get("session"))]
            if action == "recent":
                return [_entry(entry) for entry in store.recent(op.get("limit", 200))]
            if action == "sessions":
                return [_session_info(info) for info in store.sessions()]
            if action == "search":
                return [
                    _entry(entry)
                    for entry in store.search(op.get("query", ""), op.get("limit", 50))
                ]
            if action == "len":
                return len(store)
            if action == "reload":
                self.store = ChatStore(self.path)
                return {"session": self.store.session, "len": len(self.store)}
            if action == "write_raw":
                _write(self.path, op.get("text", ""))
                return True
            if action == "block_path":
                if self.path.is_file():
                    self.path.replace(self.path.with_name("chat.jsonl.old"))
                self.path.mkdir(parents=True, exist_ok=True)
                return True
            if action == "unblock_path":
                if self.path.is_dir():
                    self.path.rmdir()
                old = self.path.with_name("chat.jsonl.old")
                if old.exists():
                    old.replace(self.path)
                return True
            raise ValueError(f"operación desconocida: {action}")
        except Exception as error:
            return {"error": f"{type(error).__name__}: {error}"}

    def finish(self) -> None:
        state = {
            "session": self.store.session,
            "len": len(self.store),
            "sessions": [_session_info(info) for info in self.store.sessions()],
            "entries": [_entry(entry) for entry in self.store.entries()],
            "recent": [_entry(entry) for entry in self.store.recent(200)],
            "file": _file_text(self.path),
        }
        self.out.add(
            f"chat_store/{self.case}",
            "ChatStore",
            {"seed": self.seed, "ops": self.ops},
            _norm({"results": self.results, "state": state}, self.work.parent.parent),
        )


def _store_scenario(
    out: FixtureFile, home: Path, case: str, ops: list[dict[str, Any]], seed: str | None = None
) -> None:
    runner = _StoreRun(out, home, case, seed)
    runner.results = [runner.run(op) for op in ops]
    runner.finish()


STORE_CASES: list[tuple[str, str | None, list[dict[str, Any]]]] = [
    (
        "append-basic",
        None,
        [
            {"op": "append", "role": "user", "text": "hola", "source": "chat"},
            {
                "op": "append",
                "role": "assistant",
                "text": "buenas",
                "source": "voice",
                "t": 1735689601.25,
            },
            {"op": "append", "role": "system", "text": "nota", "source": "agent"},
            {"op": "append", "role": "user", "text": "sin hora"},
        ],
    ),
    (
        "append-cap",
        None,
        [{"op": "append", "role": "user", "text": "x" * 200_001}],
    ),
    (
        "append-coercions",
        None,
        [
            {"op": "append", "role": "user", "text": "", "source": "otro"},
            {"op": "append", "role": "user", "text": None},
            {"op": "append", "role": "user", "text": "t texto", "t": "12.5"},
            {"op": "append", "role": "user", "text": "t bool", "t": True},
            {"op": "append", "role": "user", "text": "t negativo", "t": -1},
            {"op": "reload"},
            {"op": "entries"},
        ],
    ),
    (
        "replace-last",
        None,
        [
            {"op": "append", "role": "user", "text": "cortada"},
            {"op": "append", "role": "assistant", "text": "respuesta"},
            {"op": "append", "role": "user", "text": "otra"},
            {"op": "replace_last", "role": "user", "old": "otra", "new": "otra completa"},
            {"op": "replace_last", "role": "user", "old": "no", "new": "x"},
            {"op": "replace_last", "role": "system", "old": "x", "new": "y"},
            {"op": "entries"},
            {"op": "recent", "limit": 10},
        ],
    ),
    (
        "replace-last-through-assistant",
        None,
        [
            {"op": "append", "role": "user", "text": "cortada"},
            {"op": "append", "role": "assistant", "text": "respuesta parcial"},
            {"op": "replace_last", "role": "user", "old": "cortada", "new": "cortada completa"},
            {"op": "entries"},
        ],
    ),
    (
        "resume-after-reload",
        None,
        [
            {"op": "append", "role": "user", "text": "uno"},
            {"op": "new_session"},
            {"op": "append", "role": "user", "text": "dos"},
            {"op": "reload"},
            {"op": "resume", "session": 1},
            {"op": "entries"},
            {"op": "sessions"},
        ],
    ),
    (
        "replace-rewrite",
        None,
        [
            {"op": "append", "role": "user", "text": "uno"},
            {"op": "new_session"},
            {"op": "append", "role": "user", "text": "dos"},
            {"op": "resume", "session": 1},
            {"op": "replace_last", "role": "user", "old": "uno", "new": "uno bis"},
        ],
    ),
    (
        "new-session",
        None,
        [
            {"op": "append", "role": "user", "text": "uno"},
            {"op": "new_session"},
            {"op": "append", "role": "user", "text": "dos"},
            {"op": "new_session"},
            {"op": "sessions"},
            {"op": "len"},
        ],
    ),
    (
        "resume",
        None,
        [
            {"op": "append", "role": "user", "text": "uno"},
            {"op": "new_session"},
            {"op": "append", "role": "user", "text": "dos"},
            {"op": "resume", "session": 1},
            {"op": "resume", "session": 1},
            {"op": "resume", "session": 99},
            {"op": "sessions"},
            {"op": "entries"},
        ],
    ),
    (
        "delete-session",
        None,
        [
            {"op": "append", "role": "user", "text": "uno"},
            {"op": "new_session"},
            {"op": "append", "role": "user", "text": "dos"},
            {"op": "new_session"},
            {"op": "append", "role": "user", "text": "tres"},
            {"op": "delete_session", "session": 2},
            {"op": "delete_session", "session": 2},
            {"op": "sessions"},
            {"op": "append", "role": "user", "text": "cuatro"},
        ],
    ),
    (
        "delete-current",
        None,
        [
            {"op": "append", "role": "user", "text": "uno"},
            {"op": "delete_session", "session": 2},
            {"op": "delete_session", "session": 1},
            {"op": "append", "role": "user", "text": "dos"},
            {"op": "sessions"},
        ],
    ),
    (
        "delete-missing",
        None,
        [
            {"op": "append", "role": "user", "text": "uno"},
            {"op": "delete_session", "session": 9},
        ],
    ),
    (
        "delete-failure",
        None,
        [
            {"op": "append", "role": "user", "text": "uno"},
            {"op": "new_session"},
            {"op": "append", "role": "user", "text": "dos"},
            {"op": "block_path"},
            {"op": "delete_session", "session": 1},
            {"op": "entries", "session": 1},
            {"op": "unblock_path"},
            {"op": "delete_session", "session": 1},
        ],
    ),
    (
        "init-directory",
        "<dir>",
        [{"op": "len"}, {"op": "sessions"}, {"op": "append", "role": "user", "text": "x"}],
    ),
    (
        "replace-failure",
        None,
        [
            {"op": "append", "role": "user", "text": "uno"},
            {"op": "block_path"},
            {"op": "replace_last", "role": "user", "old": "uno", "new": "uno bis"},
            {"op": "entries"},
            {"op": "unblock_path"},
            {"op": "replace_last", "role": "user", "old": "uno", "new": "uno bis"},
        ],
    ),
    (
        "write-failure",
        None,
        [
            {"op": "block_path"},
            {"op": "append", "role": "user", "text": "en memoria"},
            {"op": "append", "role": "assistant", "text": "también"},
            {"op": "unblock_path"},
            {"op": "append", "role": "user", "text": "al disco"},
            {"op": "reload"},
        ],
    ),
    (
        "recent",
        None,
        [
            {"op": "append", "role": "user", "text": "1"},
            {"op": "append", "role": "assistant", "text": "2"},
            {"op": "new_session"},
            {"op": "append", "role": "user", "text": "3"},
            {"op": "append", "role": "assistant", "text": "4"},
            {"op": "append", "role": "user", "text": "5"},
            {"op": "recent", "limit": 2},
            {"op": "recent", "limit": 0},
            {"op": "recent", "limit": -1},
            {"op": "recent", "limit": 100},
        ],
    ),
    (
        "search",
        None,
        [
            {"op": "append", "role": "user", "text": "Café con leche"},
            {"op": "append", "role": "assistant", "text": "STRASSE"},
            {"op": "append", "role": "user", "text": "straße"},
            {"op": "append", "role": "user", "text": "café otra vez"},
            {"op": "search", "query": "CAFÉ"},
            {"op": "search", "query": "strasse"},
            {"op": "search", "query": "  "},
            {"op": "search", "query": "café", "limit": 1},
            {"op": "search", "query": "café", "limit": 0},
            {"op": "search", "query": "café", "limit": -2},
        ],
    ),
    (
        "persist",
        None,
        [
            {"op": "append", "role": "user", "text": "persistente ñ"},
            {"op": "new_session"},
            {"op": "reload"},
            {"op": "entries"},
            {"op": "sessions"},
        ],
    ),
    (
        "empty-json-file",
        "",
        [{"op": "len"}, {"op": "sessions"}, {"op": "entries"}],
    ),
    (
        "damaged-file",
        "\n".join(
            [
                "no es json",
                "[]",
                '{"session": "x", "role": "user", "text": "sesión mala"}',
                '{"role": "user", "text": "sin sesión"}',
                '{"session": 1, "role": "tool", "text": "rol malo"}',
                '{"session": 1, "event": "session", "seq": 1}',
                '{"session": 1, "event": "resume", "seq": 2}',
                '{"session": 1, "role": "user", "text": "sin seq"}',
                '{"session": 1, "role": "assistant", "text": "seq texto", "seq": "7"}',
                '{"session": 1, "role": "user", "text": "seq malo", "seq": "x"}',
                '{"session": 1, "role": "user", "text": "fuente rara", "source": "otro", "seq": 9}',
                '{"session": 2, "role": "user", "text": 123, "seq": 10}',
                '{"session": 2, "role": "system", "text": "sistema", "source": "agent", "seq": 11}',
                '{"session": 3, "role": "user", "text": "sin t"}',
                '{"session": 3, "role": "user", "text": "t malo", "t": "x"}',
                '{"session": 3, "role": "user", "text": "t cero", "t": 0}',
                "",
                "   ",
            ]
        ),
        [{"op": "len"}, {"op": "sessions"}, {"op": "entries"}, {"op": "recent", "limit": 10}],
    ),
    (
        "legacy-no-seq",
        '{"session": 1, "role": "user", "text": "uno"}\n'
        '{"session": 1, "role": "assistant", "text": "dos"}\n',
        [{"op": "entries"}, {"op": "sessions"}, {"op": "len"}],
    ),
    (
        "title-long",
        None,
        [
            {"op": "append", "role": "user", "text": "  " + "palabra " * 20 + "  "},
            {"op": "new_session"},
            {"op": "append", "role": "user", "text": "ñ" * 61},
            {"op": "sessions"},
        ],
    ),
    (
        "sessions-order",
        None,
        [
            {"op": "append", "role": "user", "text": "s1"},
            {"op": "new_session"},
            {"op": "append", "role": "user", "text": "s2"},
            {"op": "new_session"},
            {"op": "append", "role": "user", "text": "s3"},
            {"op": "resume", "session": 1},
            {"op": "sessions"},
            {"op": "resume", "session": 2},
            {"op": "sessions"},
        ],
    ),
    (
        "delete-current-empty",
        None,
        [
            {"op": "new_session"},
            {"op": "delete_session", "session": 2},
            {"op": "sessions"},
            {"op": "append", "role": "user", "text": "después"},
        ],
    ),
    (
        "rewrite-current-not-last",
        None,
        [
            {"op": "append", "role": "user", "text": "uno"},
            {"op": "new_session"},
            {"op": "append", "role": "user", "text": "dos"},
            {"op": "new_session"},
            {"op": "append", "role": "user", "text": "tres"},
            {"op": "resume", "session": 1},
            {"op": "delete_session", "session": 3},
        ],
    ),
]


# --------------------------------------------------------------- sesión


def _run_setup(chat: Any, setup: list[dict[str, Any]]) -> None:
    for op in setup:
        if "add_user" in op:
            chat.add_user(op["add_user"], op.get("source", "chat"))
        elif "new_session" in op:
            chat.new_session()
        elif "begin_assistant" in op:
            chat.begin_assistant(op.get("source", "chat"))


class _SessionRun:
    def __init__(self, out: FixtureFile, home: Path, case: str, spec: dict[str, Any]) -> None:
        from ezagent.voice.chat.session import ChatModel
        from ezagent.voice.chat.store import ChatStore

        self.out = out
        self.case = case
        self.spec = spec
        self.work = home / "chat-session-cases" / case
        self.path = self.work / "chat.jsonl"
        if spec.get("seed") is not None:
            _write(self.path, spec["seed"])
        self.store = ChatStore(self.path) if spec.get("store", True) else None
        self.chat = ChatModel(self.store, spec.get("voice_replies", False))
        _run_setup(self.chat, spec.get("setup", []))
        self.ops: list[dict[str, Any]] = []
        self.results: list[Any] = []
        self.ids: list[int | None] = []

    def _id(self, value: Any) -> int:
        if isinstance(value, str) and value.startswith("$"):
            return int(self.ids[int(value[1:])] or 0)
        return int(value)

    def run(self, op: dict[str, Any]) -> Any:
        self.ops.append(op)
        action = op["op"]
        chat = self.chat
        try:
            if action == "add_user":
                return _message(chat.add_user(op.get("text", ""), op.get("source", "chat")))
            if action == "revise_user":
                return chat.revise_user(self._id(op["id"]), op.get("text", ""))
            if action == "add_note":
                return _message(chat.add_note(op.get("text", "")))
            if action == "begin_assistant":
                return _message(chat.begin_assistant(op.get("source", "chat")))
            if action == "append":
                return chat.append(self._id(op["id"]), op.get("delta", ""))
            if action == "finish":
                return chat.finish(self._id(op["id"]))
            if action == "fail":
                return chat.fail(self._id(op["id"]), op.get("text", ""))
            if action == "cancel":
                return chat.cancel(self._id(op["id"]))
            if action == "ask_confirm":
                return chat.ask_confirm(op.get("summary", ""), op.get("strong", False))
            if action == "clear_confirm":
                return chat.clear_confirm()
            if action == "new_session":
                chat.new_session()
                return chat.current_session
            if action == "switch_session":
                return chat.switch_session(op["session"])
            if action == "delete_session":
                return chat.delete_session(op["session"])
            if action == "view_session":
                chat.view_session(op.get("session"))
                return chat.viewing
            if action == "set_voice_replies":
                chat.set_voice_replies(op["enabled"])
                return chat.voice_replies
            if action == "set_mic_muted":
                chat.set_mic_muted(op["muted"])
                return chat.mic_muted
            if action == "last_assistant":
                return chat.last_assistant_text()
            if action == "sessions":
                return [_session_info(info) for info in chat.sessions()]
            if action == "snapshot":
                version, messages = chat.snapshot()
                return {"version": version, "messages": [_message(message) for message in messages]}
            if action == "state":
                return self.state()
            raise ValueError(f"operación desconocida: {action}")
        except Exception as error:
            return {"error": f"{type(error).__name__}: {error}"}

    def state(self) -> dict[str, Any]:
        confirm = self.chat.confirm
        return {
            "version": self.chat.version,
            "busy": self.chat.busy,
            "confirm": None
            if confirm is None
            else {"summary": confirm.summary, "strong": confirm.strong},
            "viewing": self.chat.viewing,
            "current_session": self.chat.current_session,
            "voice_replies": self.chat.voice_replies,
            "mic_muted": self.chat.mic_muted,
        }

    def finish(self) -> None:
        version, messages = self.chat.snapshot()
        state = self.state()
        state["sessions"] = [_session_info(info) for info in self.chat.sessions()]
        state["snapshot"] = {
            "version": version,
            "messages": [_message(message) for message in messages],
        }
        payload = {
            "results": self.results,
            "state": state,
            "file": _file_text(self.path),
        }
        self.out.add(
            f"chat_store/session/{self.case}",
            "ChatModel",
            {
                "store": self.store is not None,
                "voice_replies": self.spec.get("voice_replies", False),
                "setup": self.spec.get("setup", []),
                "seed": self.spec.get("seed"),
                "ops": self.ops,
            },
            _norm(payload, self.work.parent.parent),
        )


def _session_scenario(out: FixtureFile, home: Path, case: str, spec: dict[str, Any]) -> None:
    runner = _SessionRun(out, home, case, spec)
    for op in spec.get("ops", []):
        result = runner.run(op)
        runner.results.append(result)
        runner.ids.append(result["id"] if isinstance(result, dict) and "id" in result else None)
    runner.finish()


SESSION_CASES: list[dict[str, Any]] = [
    {
        "id": "lifecycle",
        "ops": [
            {"op": "add_user", "text": "  pregunta  "},
            {"op": "begin_assistant"},
            {"op": "append", "id": "$1", "delta": "Hola"},
            {"op": "append", "id": "$1", "delta": ", mundo"},
            {"op": "finish", "id": "$1"},
            {"op": "state"},
            {"op": "snapshot"},
        ],
    },
    {
        "id": "empty-input",
        "ops": [
            {"op": "add_user", "text": ""},
            {"op": "add_user", "text": "   "},
            {"op": "state"},
            {"op": "snapshot"},
        ],
    },
    {
        "id": "revision",
        "ops": [
            {"op": "add_user", "text": "voz cortada"},
            {"op": "add_user", "text": "otra"},
            {"op": "revise_user", "id": "$1", "text": "otra completa"},
            {"op": "revise_user", "id": "$0", "text": "vieja"},
            {"op": "revise_user", "id": "$1", "text": "gerundio"},
            {"op": "snapshot"},
        ],
    },
    {
        "id": "notes",
        "ops": [
            {"op": "add_note", "text": "conversación compactada"},
            {"op": "state"},
            {"op": "snapshot"},
        ],
    },
    {
        "id": "revision-edge",
        "ops": [
            {"op": "begin_assistant"},
            {"op": "revise_user", "id": "$0", "text": "no es usuario"},
            {"op": "append", "id": "$0", "delta": ""},
            {"op": "append", "id": "$0", "delta": "texto"},
            {"op": "revise_user", "id": 999, "text": "no existe"},
            {"op": "snapshot"},
        ],
    },
    {
        "id": "fail",
        "ops": [
            {"op": "begin_assistant", "source": "voice"},
            {"op": "append", "id": "$0", "delta": "parcial"},
            {"op": "fail", "id": "$0", "text": "falló la red"},
            {"op": "state"},
            {"op": "last_assistant"},
            {"op": "snapshot"},
        ],
    },
    {
        "id": "cancel-partial",
        "ops": [
            {"op": "begin_assistant", "source": "voice"},
            {"op": "append", "id": "$0", "delta": "  quedó esto  "},
            {"op": "cancel", "id": "$0"},
            {"op": "state"},
            {"op": "snapshot"},
        ],
    },
    {
        "id": "cancel-empty",
        "ops": [
            {"op": "begin_assistant"},
            {"op": "append", "id": "$0", "delta": "   "},
            {"op": "cancel", "id": "$0"},
            {"op": "state"},
            {"op": "snapshot"},
        ],
    },
    {
        "id": "finish-empty",
        "ops": [
            {"op": "begin_assistant"},
            {"op": "finish", "id": "$0"},
            {"op": "state"},
            {"op": "snapshot"},
        ],
    },
    {
        "id": "two-streams",
        "ops": [
            {"op": "begin_assistant"},
            {"op": "begin_assistant"},
            {"op": "append", "id": "$1", "delta": "dos"},
            {"op": "finish", "id": "$1"},
            {"op": "state"},
            {"op": "cancel", "id": "$0"},
            {"op": "state"},
        ],
    },
    {
        "id": "confirm-card",
        "ops": [
            {"op": "ask_confirm", "summary": "¿Borro el archivo?", "strong": True},
            {"op": "state"},
            {"op": "ask_confirm", "summary": "¿Otra?", "strong": False},
            {"op": "clear_confirm"},
            {"op": "clear_confirm"},
            {"op": "state"},
        ],
    },
    {
        "id": "switches",
        "ops": [
            {"op": "set_voice_replies", "enabled": True},
            {"op": "set_voice_replies", "enabled": True},
            {"op": "set_voice_replies", "enabled": 0},
            {"op": "set_mic_muted", "muted": True},
            {"op": "set_mic_muted", "muted": True},
            {"op": "set_mic_muted", "muted": False},
            {"op": "state"},
        ],
    },
    {
        "id": "store-lifecycle",
        "ops": [
            {"op": "add_user", "text": "uno"},
            {"op": "begin_assistant"},
            {"op": "append", "id": "$1", "delta": " respuesta "},
            {"op": "finish", "id": "$1"},
            {"op": "new_session"},
            {"op": "add_user", "text": "dos"},
            {"op": "sessions"},
            {"op": "switch_session", "session": 1},
            {"op": "snapshot"},
        ],
    },
    {
        "id": "store-revise",
        "ops": [
            {"op": "add_user", "text": "frase"},
            {"op": "revise_user", "id": "$0", "text": "frase completa"},
            {"op": "add_user", "text": "otra"},
            {"op": "revise_user", "id": "$0", "text": "vieja"},
            {"op": "snapshot"},
        ],
    },
    {
        "id": "store-notes",
        "ops": [
            {"op": "add_user", "text": "hola"},
            {"op": "add_note", "text": "nota interna"},
            {"op": "state"},
            {"op": "snapshot"},
        ],
    },
    {
        "id": "switch-busy",
        "ops": [
            {"op": "begin_assistant"},
            {"op": "switch_session", "session": 1},
            {"op": "delete_session", "session": 1},
            {"op": "state"},
        ],
    },
    {
        "id": "delete-current",
        "ops": [
            {"op": "add_user", "text": "uno"},
            {"op": "new_session"},
            {"op": "add_user", "text": "dos"},
            {"op": "delete_session", "session": 2},
            {"op": "state"},
            {"op": "snapshot"},
            {"op": "sessions"},
        ],
    },
    {
        "id": "delete-past",
        "ops": [
            {"op": "add_user", "text": "uno"},
            {"op": "new_session"},
            {"op": "add_user", "text": "dos"},
            {"op": "delete_session", "session": 1},
            {"op": "state"},
            {"op": "snapshot"},
        ],
    },
    {
        "id": "view",
        "ops": [
            {"op": "add_user", "text": "uno"},
            {"op": "new_session"},
            {"op": "add_user", "text": "dos"},
            {"op": "view_session", "session": 1},
            {"op": "snapshot"},
            {"op": "view_session", "session": 2},
            {"op": "view_session", "session": None},
            {"op": "view_session", "session": 1},
            {"op": "add_user", "text": "tres"},
            {"op": "state"},
            {"op": "snapshot"},
        ],
    },
    {
        "id": "last-assistant",
        "ops": [
            {"op": "begin_assistant"},
            {"op": "append", "id": "$0", "delta": "primera"},
            {"op": "finish", "id": "$0"},
            {"op": "begin_assistant"},
            {"op": "fail", "id": "$1", "text": "error"},
            {"op": "begin_assistant"},
            {"op": "finish", "id": "$2"},
            {"op": "last_assistant"},
        ],
    },
    {
        "id": "no-store",
        "store": False,
        "ops": [
            {"op": "add_user", "text": "sin store"},
            {"op": "new_session"},
            {"op": "sessions"},
            {"op": "switch_session", "session": 1},
            {"op": "delete_session", "session": 1},
            {"op": "view_session", "session": 2},
            {"op": "state"},
        ],
    },
    {
        "id": "from-store",
        "seed": '{"v": 1, "t": 1735689600.0, "session": 1, "role": "user", "source": "voice", "text": "guardado", "seq": 1}\n',
        "ops": [
            {"op": "state"},
            {"op": "snapshot"},
            {"op": "last_assistant"},
            {"op": "sessions"},
        ],
    },
    {
        "id": "revise-fallback",
        "ops": [
            {"op": "add_user", "text": "primera"},
            {"op": "new_session"},
            {"op": "add_user", "text": "segunda"},
            {"op": "switch_session", "session": 1},
            {"op": "revise_user", "id": 1, "text": "no es la última guardada"},
            {"op": "state"},
            {"op": "snapshot"},
        ],
    },
    {
        "id": "missing-message",
        "ops": [
            {"op": "append", "id": 999},
            {"op": "finish", "id": 999},
            {"op": "fail", "id": 999, "text": "x"},
            {"op": "cancel", "id": 999},
            {"op": "revise_user", "id": 999, "text": "x"},
            {"op": "state"},
        ],
    },
]


# -------------------------------------------------------------- contextos


class _FakeHolder:
    def __init__(self, calls: list[dict[str, Any]], exports: dict[str, Any]) -> None:
        self.calls = calls
        self.exports = exports

    def reset_history(self) -> None:
        self.calls.append({"call": "reset"})

    def export_context(self) -> tuple[list[dict[str, Any]], str, str]:
        session = str(getattr(self, "session", "1"))
        data = self.exports.get(session, self.exports.get("*", {}))
        self.calls.append({"call": "export", "session": session})
        return (
            list(data.get("messages", [])),
            data.get("summary", ""),
            data.get("last_user", ""),
        )

    def import_context(
        self, messages: list[dict[str, Any]], summary: str = "", last_user: str = ""
    ) -> None:
        self.calls.append(
            {
                "call": "import",
                "messages": list(messages),
                "summary": summary,
                "last_user": last_user,
            }
        )


class _HolderProxy:
    """Devuelve el holder y le comunica la sesión actual en cada exportación."""

    def __init__(self, holder: _FakeHolder, chat: Any) -> None:
        self.holder = holder
        self.chat = chat

    def __call__(self) -> _FakeHolder:
        self.holder.session = str(self.chat.current_session or "")
        return self.holder


class _ContextRun:
    def __init__(self, out: FixtureFile, home: Path, case: str, spec: dict[str, Any]) -> None:
        from ezagent.voice.chat.contexts import ChatContexts
        from ezagent.voice.chat.session import ChatModel
        from ezagent.voice.chat.store import ChatStore

        self.out = out
        self.case = case
        self.spec = spec
        self.work = home / "chat-context-cases" / case
        self.path = self.work / "chat.jsonl"
        if spec.get("seed") is not None:
            _write(self.path, spec["seed"])
        self.store = ChatStore(self.path) if spec.get("store", True) else None
        self.chat = ChatModel(self.store, False)
        _run_setup(self.chat, spec.get("setup", []))
        self.calls: list[dict[str, Any]] = []
        holder = _FakeHolder(self.calls, spec.get("exports", {}))
        source = (lambda: None) if spec.get("holder") == "none" else _HolderProxy(holder, self.chat)
        self.contexts = ChatContexts(self.chat, source)
        self.ops: list[dict[str, Any]] = []
        self.results: list[Any] = []

    def run(self, op: dict[str, Any]) -> Any:
        self.ops.append(op)
        action = op["op"]
        try:
            if action == "new_chat":
                self.contexts.new_chat()
                return True
            if action == "switch":
                return self.contexts.switch(op["session"])
            if action == "delete":
                return self.contexts.delete(op["session"])
            if action == "rebuild":
                self.contexts.rebuild(op.get("session"))
                return True
            if action == "saved":
                return self.saved()
            if action == "state":
                return self.state()
            raise ValueError(f"operación desconocida: {action}")
        except Exception as error:
            return {"error": f"{type(error).__name__}: {error}"}

    def saved(self) -> list[dict[str, Any]]:
        return [
            {
                "session": session,
                "messages": saved.messages,
                "summary": saved.summary,
                "last_user": saved.last_user,
            }
            for session, saved in self.contexts.saved.items()
        ]

    def state(self) -> dict[str, Any]:
        version, messages = self.chat.snapshot()
        return {
            "session": self.chat.current_session,
            "version": version,
            "busy": self.chat.busy,
            "messages": [_message(message) for message in messages],
            "sessions": [_session_info(info) for info in self.chat.sessions()],
        }

    def finish(self) -> None:
        payload = {
            "results": self.results,
            "calls": self.calls,
            "saved": self.saved(),
            "state": self.state(),
            "file": _file_text(self.path),
        }
        self.out.add(
            f"chat_store/contexts/{self.case}",
            "ChatContexts",
            {
                "store": self.store is not None,
                "holder": self.spec.get("holder", "fake"),
                "setup": self.spec.get("setup", []),
                "exports": self.spec.get("exports", {}),
                "ops": self.ops,
            },
            _norm(payload, self.work.parent.parent),
        )


def _context_scenario(out: FixtureFile, home: Path, case: str, spec: dict[str, Any]) -> None:
    runner = _ContextRun(out, home, case, spec)
    runner.results = [runner.run(op) for op in spec.get("ops", [])]
    runner.finish()


def _session_setup(count: int) -> list[dict[str, Any]]:
    setup: list[dict[str, Any]] = []
    for index in range(1, count + 1):
        setup.append({"add_user": f"mensaje {index}"})
        if index < count:
            setup.append({"new_session": True})
    return setup


CONTEXT_CASES: list[dict[str, Any]] = [
    {
        "id": "new-chat",
        "setup": _session_setup(2),
        "exports": {
            "2": {
                "messages": [{"role": "user", "content": "m2"}],
                "summary": "resumen 2",
                "last_user": "m2",
            }
        },
        "ops": [{"op": "state"}, {"op": "new_chat"}, {"op": "saved"}, {"op": "state"}],
    },
    {
        "id": "switch-saved",
        "setup": _session_setup(2),
        "exports": {
            "1": {"messages": [], "summary": "r1", "last_user": "u1"},
            "2": {
                "messages": [{"role": "assistant", "content": "a2"}],
                "summary": "r2",
                "last_user": "u2",
            },
        },
        "ops": [
            {"op": "switch", "session": 1},
            {"op": "saved"},
            {"op": "switch", "session": 2},
            {"op": "saved"},
            {"op": "state"},
        ],
    },
    {
        "id": "switch-same-missing",
        "setup": _session_setup(2),
        "ops": [
            {"op": "switch", "session": 1},
            {"op": "switch", "session": 1},
            {"op": "switch", "session": 99},
            {"op": "saved"},
            {"op": "state"},
        ],
    },
    {
        "id": "rebuild",
        "setup": _session_setup(2),
        "seed": '{"v": 1, "t": 1735689600.0, "session": 1, "role": "user", "text": "guardado", "seq": 1}\n',
        "ops": [
            {"op": "rebuild", "session": 1},
            {"op": "rebuild"},
            {"op": "rebuild", "session": 99},
        ],
    },
    {
        "id": "delete-past",
        "setup": _session_setup(2),
        "exports": {
            "1": {"messages": [], "summary": "r1", "last_user": ""},
            "2": {"messages": [], "summary": "r2", "last_user": ""},
        },
        "ops": [
            {"op": "switch", "session": 1},
            {"op": "switch", "session": 2},
            {"op": "delete", "session": 1},
            {"op": "saved"},
            {"op": "state"},
        ],
    },
    {
        "id": "delete-current",
        "setup": _session_setup(2),
        "ops": [
            {"op": "delete", "session": 2},
            {"op": "saved"},
            {"op": "state"},
        ],
    },
    {
        "id": "delete-missing",
        "setup": _session_setup(1),
        "ops": [{"op": "delete", "session": 9}, {"op": "state"}],
    },
    {
        "id": "max-saved",
        "setup": _session_setup(10),
        "ops": [
            {"op": "switch", "session": 1},
            {"op": "switch", "session": 2},
            {"op": "switch", "session": 3},
            {"op": "switch", "session": 4},
            {"op": "switch", "session": 5},
            {"op": "switch", "session": 6},
            {"op": "switch", "session": 7},
            {"op": "switch", "session": 8},
            {"op": "switch", "session": 9},
            {"op": "saved"},
            {"op": "state"},
        ],
    },
    {
        "id": "holder-none",
        "setup": _session_setup(2),
        "holder": "none",
        "ops": [
            {"op": "new_chat"},
            {"op": "switch", "session": 1},
            {"op": "delete", "session": 1},
            {"op": "saved"},
            {"op": "state"},
        ],
    },
    {
        "id": "no-store",
        "store": False,
        "ops": [
            {"op": "new_chat"},
            {"op": "rebuild"},
            {"op": "switch", "session": 1},
            {"op": "delete", "session": 1},
            {"op": "state"},
        ],
    },
    {
        "id": "busy-switch",
        "setup": [*_session_setup(2), {"begin_assistant": True}],
        "ops": [
            {"op": "state"},
            {"op": "switch", "session": 1},
            {"op": "saved"},
        ],
    },
]


# ---------------------------------------------------------------- puras

TITLE_CASES: list[str] = [
    "",
    "  hola   mundo ",
    "x" * 59,
    "x" * 60,
    "x" * 61,
    "ñ" * 61,
    "línea\ncon\ttabulador",
    "  ",
    None,
]

AS_MESSAGES_CASES: list[dict[str, Any]] = [
    [],
    [
        {"t": 1.0, "session": 1, "role": "user", "source": "chat", "text": "hola", "seq": 1},
        {"t": 2.0, "session": 1, "role": "assistant", "source": "chat", "text": "buenas", "seq": 2},
        {"t": 3.0, "session": 1, "role": "system", "source": "agent", "text": "nota", "seq": 3},
    ],
]

SEQ_OF_CASES: list[Any] = [
    {"session": 1, "seq": 5},
    {"session": 1, "seq": "5"},
    {"session": 1, "seq": "x"},
    {"session": 1},
    {"session": 1, "seq": None},
    {"session": 1, "seq": True},
    {"session": 1, "seq": 0},
    {"session": 1, "seq": -3},
]


def _pure_cases(out: FixtureFile, home: Path, store_module: Any, contexts_module: Any) -> None:
    for index, text in enumerate(TITLE_CASES, start=1):
        out.add(
            f"chat_store/title/{index:03d}",
            "title",
            {"text": text},
            {"title": store_module._title(text)},
        )
    out.add(
        "chat_store/title/custom-limit",
        "title",
        {"text": "abcdefghij", "limit": 4},
        {"title": store_module._title("abcdefghij", 4)},
    )
    for index, raw in enumerate(AS_MESSAGES_CASES, start=1):
        entries = [
            store_module.ChatEntry(
                item["t"], item["session"], item["role"], item["source"], item["text"], item["seq"]
            )
            for item in raw
        ]
        out.add(
            f"chat_store/as_messages/{index:03d}",
            "as_messages",
            {"entries": raw},
            {"messages": store_module.as_messages(entries)},
        )
    for index, data in enumerate(SEQ_OF_CASES, start=1):
        out.add(
            f"chat_store/seq_of/{index:03d}",
            "seq_of",
            {"data": data, "index": index},
            {"seq": store_module._seq_of(data, index)},
        )
    original = dict(os.environ)
    try:
        for index, appdata in enumerate([None, str(home / "AppData"), ""], start=1):
            if appdata is None:
                os.environ.pop("APPDATA", None)
            else:
                os.environ["APPDATA"] = appdata
            os.environ["XDG_CONFIG_HOME"] = str(home / "xdg")
            out.add(
                f"chat_store/default-path/{index:02d}",
                "default_chat_path",
                {"APPDATA": _norm(appdata, home), "XDG_CONFIG_HOME": "<tmp>/xdg"},
                {"path": _norm(str(store_module.default_chat_path()), home)},
            )
    finally:
        os.environ.clear()
        os.environ.update(original)
    out.add(
        "chat_store/constants",
        "module_constants",
        {},
        {
            "max_entry_chars": store_module.MAX_ENTRY_CHARS,
            "version": store_module._VERSION,
            "max_saved": contexts_module.MAX_SAVED,
        },
    )


def generate(out: FixtureFile) -> None:
    from ezagent.voice.chat import contexts as contexts_module
    from ezagent.voice.chat import session as session_module
    from ezagent.voice.chat import store as store_module

    home = Path(os.environ["HOME"])
    del session_module
    _pure_cases(out, home, store_module, contexts_module)
    for case, seed, ops in STORE_CASES:
        _store_scenario(out, home, case, ops, seed)
    for spec in SESSION_CASES:
        _session_scenario(out, home, spec["id"], spec)
    for spec in CONTEXT_CASES:
        _context_scenario(out, home, spec["id"], spec)
    del contexts_module


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
