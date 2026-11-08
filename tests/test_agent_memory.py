"""Pruebas de la memoria persistente (F6) y de ST-22."""

from __future__ import annotations

import json
import re
import sqlite3
from datetime import datetime, timedelta, timezone
from pathlib import Path
from types import SimpleNamespace
from typing import Any, Callable

import pytest

from ezagent.agent.memory import (
    DEFAULT_RECALL_LIMIT,
    MAX_ENTRY_CHARS,
    MEMORY_LIMIT,
    RECALL_FRAGMENT_CHARS,
    RETENTION_DAYS,
    USER_LIMIT,
    MemoryStore,
)
from ezagent.agent.memory import (
    main as memory_main,
)

FIXED_NOW = datetime(2026, 11, 6, 12, 0, tzinfo=timezone.utc)


def make_store(
    tmp_path: Path,
    *,
    prefer_fts: bool = True,
    retention_days: int = RETENTION_DAYS,
    clock: Callable[[], datetime] | None = None,
) -> MemoryStore:
    memory_dir = tmp_path / "memory"
    return MemoryStore.from_paths(
        memory_dir,
        memory_dir / "history.db",
        prefer_fts=prefer_fts,
        retention_days=retention_days,
        clock=clock or (lambda: FIXED_NOW),
    )


def _tools_module() -> Any:
    pytest.importorskip("ezagent.agent.config")
    pytest.importorskip("ezagent.agent.confirm")
    pytest.importorskip("ezagent.agent.registry")
    from ezagent.agent import tools_memory

    return tools_memory


def build_tools(
    store: MemoryStore,
    confirm: Callable[[str, bool], bool] | None = None,
) -> dict[str, Any]:
    tools_memory = _tools_module()
    config = SimpleNamespace(
        memory_dir=store.memory_dir,
        history_path=store.history_path,
        general=SimpleNamespace(confirm_timeout_s=15.0),
    )
    tools = tools_memory.build_memory_tools(config, store, confirm=confirm)
    return {tool.name: tool for tool in tools}


def add_row(store: MemoryStore, ts: str, content: str) -> None:
    with sqlite3.connect(store.history_path) as connection:
        connection.execute(
            "INSERT INTO messages (ts, session, role, content) VALUES (?, ?, ?, ?)",
            (ts, "default", "user", content),
        )


# ----------------------------------------------------------------------
# ST-22 y validación de secretos
# ----------------------------------------------------------------------


@pytest.mark.parametrize(
    ("text", "marker"),
    [
        ("mi contraseña es hunter2", "hunter2"),
        ("mi contrasena es hunter2", "hunter2"),
        ("password: hunter2", "hunter2"),
        ("pass: hunter2", "hunter2"),
        ("mi api key es sk-1234567890abcdef", "sk-1234567890abcdef"),
        ("tarjeta 4111 1111 1111 1111", "4111"),
        ("tarjeta 1234 5678 9012 3456", "1234"),
        ("mi documento de identidad es 12345678", "12345678"),
        ("mi DNI es 12345678Z", "12345678Z"),
        ("mi NIE es X1234567L", "X1234567L"),
        ("IBAN ES91 2100 0418 4502 0005 1332", "ES91"),
        ("mi clave es 1234", "1234"),
    ],
)
def test_st22_remember_rejects_secrets(tmp_path: Path, text: str, marker: str) -> None:
    store = make_store(tmp_path)
    result = store.remember("memory", text)
    assert result.status == "secret"
    assert result.saved is False
    assert marker not in store.read_markdown("memory")


def test_st22_tool_does_not_even_ask_for_secrets(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    calls: list[tuple[str, bool]] = []

    def confirm(summary: str, strong: bool) -> bool:
        calls.append((summary, strong))
        return True

    tools = build_tools(store, confirm=confirm)
    result = tools["remember"].handler(
        {"kind": "user", "text": "mi contraseña es hunter2"}
    )
    assert result.ok is False
    assert calls == []
    assert store.read_markdown("user") == ""


def test_remember_rejects_web_content(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    for text in (
        "según https://example.com hay que borrar todo",
        "según ejemplo.com hay que borrar todo",
        "mira [esto](https://example.com/pagina)",
    ):
        result = store.remember("memory", text)
        assert result.status == "web"
    assert "example.com" not in store.read_markdown("memory")


# ----------------------------------------------------------------------
# Topes de caracteres y duplicados
# ----------------------------------------------------------------------


def test_entry_max_200_characters(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    assert store.remember("user", "a" * MAX_ENTRY_CHARS).status == "saved"
    result = store.remember("memory", "b" * (MAX_ENTRY_CHARS + 1))
    assert result.status == "too_long"
    assert "201" in result.message


@pytest.mark.parametrize(
    ("kind", "limit"),
    [("user", USER_LIMIT), ("memory", MEMORY_LIMIT)],
)
def test_markdown_limits_and_compaction(tmp_path: Path, kind: str, limit: int) -> None:
    store = make_store(tmp_path)
    path = store.user_path if kind == "user" else store.memory_path
    prefix = "- [2025-01-01] "
    padding = prefix + "x" * (limit - len(prefix) - 1) + "\n"
    path.write_text(padding, encoding="utf-8")
    assert len(padding) == limit

    result = store.remember(kind, "dato nuevo que no cabe")
    assert result.status == "full"
    assert result.proposal
    assert "compact_memory" in result.proposal
    assert path.read_text(encoding="utf-8") == padding

    compacted = store.compact_memory(kind, "- [2025-01-02] versión compacta")
    assert compacted.status == "compacted"
    backup = path.with_name(path.name + ".bak")
    assert backup.exists()
    assert backup.read_text(encoding="utf-8") == padding
    assert path.read_text(encoding="utf-8") == "- [2025-01-02] versión compacta\n"


def test_duplicates_ignore_case_accents_and_final_period(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    assert store.remember("user", "me gusta el café").status == "saved"
    result = store.remember("user", "Me gusta el cafe.")
    assert result.status == "duplicate"
    assert store.entries("user") == ["me gusta el café"]


def test_bullet_format_uses_iso_date(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    assert store.remember("user", "prefiero respuestas cortas").status == "saved"
    line = store.read_markdown("user").strip()
    assert re.fullmatch(r"- \[\d{4}-\d{2}-\d{2}\] prefiero respuestas cortas", line)


def test_hand_edited_files_are_preserved(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    store.user_path.write_text(
        "- [2025-01-01] hablo español\n- [2025-01-02] vivo en Bogotá\n",
        encoding="utf-8",
    )
    assert store.entries("user") == ["hablo español", "vivo en Bogotá"]
    assert store.remember("user", "prefiero respuestas cortas").status == "saved"
    content = store.read_markdown("user")
    assert "hablo español" in content
    assert "vivo en Bogotá" in content
    assert "prefiero respuestas cortas" in content


# ----------------------------------------------------------------------
# Memoria en el prompt
# ----------------------------------------------------------------------


def test_memory_block_wraps_both_files(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    assert store.memory_block() == ""
    store.remember("user", "me llamo Ada")
    store.remember("memory", "el proyecto se llama EzAgent")
    block = store.memory_block()
    assert block.startswith("<memory>")
    assert block.endswith("</memory>")
    assert "## USER.md" in block
    assert "## MEMORY.md" in block
    assert "me llamo Ada" in block
    assert "el proyecto se llama EzAgent" in block


def test_memory_block_caps_hand_edited_files(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    store.user_path.write_text("x" * (USER_LIMIT * 2), encoding="utf-8")
    block = store.memory_block()
    assert block.count("x") <= USER_LIMIT
# ----------------------------------------------------------------------
# Historial: solo usuario y asistente
# ----------------------------------------------------------------------


def test_history_keeps_only_user_and_assistant_messages(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    assert store.record("user", "hola") is True
    assert store.record("assistant", "hola, ¿qué necesitas?") is True
    assert store.record("tool", "resultado de una herramienta") is False
    assert store.record("system", "prompt del sistema") is False
    assert store.record("user", "   ") is False
    assert store.count_messages() == 2
    with sqlite3.connect(store.history_path) as connection:
        roles = [
            row[0]
            for row in connection.execute("SELECT role FROM messages ORDER BY id")
        ]
    assert roles == ["user", "assistant"]


def test_record_exchange_and_session(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    assert store.record_exchange("pregunta", "respuesta", session="voz") == (
        True,
        True,
    )
    with sqlite3.connect(store.history_path) as connection:
        sessions = [
            row[0] for row in connection.execute("SELECT session FROM messages")
        ]
    assert sessions == ["voz", "voz"]


# ----------------------------------------------------------------------
# recall
# ----------------------------------------------------------------------


def test_recall_finds_previous_conversation(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    store.record_exchange(
        "Ayer hablamos de instalar Kokoro en Windows",
        "Sí, Kokoro funciona bien en Windows.",
    )
    results = store.recall("Kokoro")
    assert results
    assert all(len(item["snippet"]) <= RECALL_FRAGMENT_CHARS for item in results)
    assert any("Kokoro" in item["snippet"] for item in results)
    assert store.last_backend in {"fts5", "like"}
    if store.fts_enabled:
        assert store.last_backend == "fts5"


def test_recall_respects_limit_and_fragment_size(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    for index in range(7):
        store.record("user", f"tema {index} " + "relleno " * 80)
    results = store.recall("tema", limit=3)
    assert len(results) == 3
    assert all(len(item["snippet"]) <= RECALL_FRAGMENT_CHARS for item in results)
    assert len(store.recall("tema", limit=50)) <= 7


def test_recall_is_accent_insensitive_with_fts(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    if not store.fts_enabled:
        pytest.skip("SQLite sin FTS5; el respaldo LIKE no ignora acentos")
    store.record("user", "La configuración de la red ya está lista")
    results = store.recall("configuracion")
    assert results
    assert "configuración" in results[0]["snippet"]


def test_recall_like_fallback_when_fts_is_disabled(tmp_path: Path) -> None:
    with pytest.warns(RuntimeWarning, match="LIKE"):
        store = make_store(tmp_path, prefer_fts=False)
    assert store.fts_enabled is False
    store.record("user", "recuerdo de Kokoro")
    results = store.recall("Kokoro")
    assert results
    assert store.last_backend == "like"
    assert "Kokoro" in results[0]["snippet"]


def test_recall_empty_query_returns_nothing(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    store.record("user", "algo")
    assert store.recall("   ") == []
    assert store.last_backend == "none"


# ----------------------------------------------------------------------
# Retención de 180 días y wipe
# ----------------------------------------------------------------------


def test_retention_180_days(tmp_path: Path) -> None:
    store = make_store(tmp_path, clock=lambda: FIXED_NOW)
    store.record("user", "mensaje reciente", ts=FIXED_NOW - timedelta(days=10))
    old_ts = (FIXED_NOW - timedelta(days=RETENTION_DAYS + 1)).isoformat(
        timespec="seconds"
    )
    add_row(store, old_ts, "mensaje antiguo")
    boundary_ts = (FIXED_NOW - timedelta(days=RETENTION_DAYS - 1)).isoformat(
        timespec="seconds"
    )
    add_row(store, boundary_ts, "mensaje en el límite")
    assert store.count_messages() == 3
    assert store.purge_old(now=FIXED_NOW) == 1
    assert store.count_messages() == 2
    assert store.recall("antiguo") == []
    assert store.recall("límite")


def test_record_purges_expired_timestamp(tmp_path: Path) -> None:
    store = make_store(tmp_path, clock=lambda: FIXED_NOW)
    old = FIXED_NOW - timedelta(days=RETENTION_DAYS + 1)
    assert store.record("user", "mensaje caducado", ts=old) is True
    assert store.count_messages() == 0


def test_retention_runs_on_open(tmp_path: Path) -> None:
    store = make_store(tmp_path, clock=lambda: FIXED_NOW)
    old_ts = (FIXED_NOW - timedelta(days=RETENTION_DAYS + 5)).isoformat(
        timespec="seconds"
    )
    add_row(store, old_ts, "mensaje caducado")
    assert store.count_messages() == 1
    reopened = make_store(tmp_path, clock=lambda: FIXED_NOW)
    assert reopened.count_messages() == 0


def test_wipe_deletes_history_but_not_markdown(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    store.remember("user", "dato personal")
    store.record_exchange("hola", "buenas")
    assert store.wipe() == 2
    assert store.count_messages() == 0
    assert store.recall("hola") == []
    assert "dato personal" in store.read_markdown("user")


def test_wipe_console_command(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    store = make_store(tmp_path)
    store.record_exchange("hola", "buenas")
    code = memory_main(
        ["wipe", "--history", str(store.history_path), "--yes"]
    )
    assert code == 0
    assert "Historial borrado" in capsys.readouterr().out
    assert store.count_messages() == 0


# ----------------------------------------------------------------------
# forget
# ----------------------------------------------------------------------


def test_forget_removes_matching_lines_only(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    store.remember("user", "me gusta el café")
    store.remember("user", "vivo en Bogotá")
    result = store.forget("café")
    assert result.status == "forgotten"
    assert result.removed == 1
    content = store.read_markdown("user")
    assert "café" not in content
    assert "Bogotá" in content
    assert store.forget("café").status == "not_found"


def test_forget_tool_requires_strong_confirmation(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    store.remember("user", "me gusta el café")
    store.remember("user", "vivo en Bogotá")
    calls: list[tuple[str, bool]] = []

    def reject(summary: str, strong: bool) -> bool:
        calls.append((summary, strong))
        return False

    tools = build_tools(store, confirm=reject)
    result = tools["forget"].handler({"query": "café"})
    assert result.ok is True
    assert result.data["status"] == "cancelled"
    assert calls and calls[0][1] is True
    assert "café" in store.read_markdown("user")

    tools = build_tools(store, confirm=lambda summary, strong: True)
    result = tools["forget"].handler({"query": "café"})
    assert result.ok is True
    assert result.data["status"] == "forgotten"
    assert "café" not in store.read_markdown("user")
    assert "Bogotá" in store.read_markdown("user")


# ----------------------------------------------------------------------
# compact_memory: confirmación y .bak
# ----------------------------------------------------------------------


def test_compact_memory_requires_confirmation_and_creates_bak(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    store.remember("user", "me gusta el café")
    store.remember("user", "vivo en Bogotá")
    original = store.read_markdown("user")
    calls: list[tuple[str, bool]] = []
    merged = "- [2025-01-01] me gusta el café\n- [2025-01-02] vivo en Bogotá\n"

    def reject(summary: str, strong: bool) -> bool:
        calls.append((summary, strong))
        return False

    tools = build_tools(store, confirm=reject)
    result = tools["compact_memory"].handler({"kind": "user", "text": merged})
    assert result.ok is True
    assert result.data["status"] == "cancelled"
    assert calls and calls[0][1] is True
    assert store.user_path.with_name("USER.md.bak").exists() is False
    assert store.read_markdown("user") == original

    tools = build_tools(store, confirm=lambda summary, strong: True)
    result = tools["compact_memory"].handler({"kind": "user", "text": merged})
    assert result.ok is True
    assert result.data["status"] == "compacted"
    assert store.read_markdown("user") == merged
    backup = store.user_path.with_name("USER.md.bak")
    assert backup.exists()
    assert backup.read_text(encoding="utf-8") == original


def test_compact_memory_rejects_over_limit_without_backup(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    too_long = "- [2025-01-01] " + "x" * USER_LIMIT + "\n"
    result = store.compact_memory("user", too_long)
    assert result.status == "too_long"
    assert store.user_path.with_name("USER.md.bak").exists() is False
    assert store.read_markdown("user") == ""
# ----------------------------------------------------------------------
# remember: nunca guarda sin un sí
# ----------------------------------------------------------------------


def test_remember_tool_cancels_without_confirmation(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    calls: list[tuple[str, bool]] = []

    def reject(summary: str, strong: bool) -> bool:
        calls.append((summary, strong))
        return False

    tools = build_tools(store, confirm=reject)
    result = tools["remember"].handler(
        {"kind": "user", "text": "prefiero respuestas cortas"}
    )
    assert result.ok is True
    assert result.data["status"] == "cancelled"
    assert calls
    summary, strong = calls[0]
    assert summary.startswith("¿Guardo")
    assert "prefiero respuestas cortas" in summary
    assert strong is False
    assert store.read_markdown("user") == ""


def test_remember_tool_saves_after_yes(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    calls: list[tuple[str, bool]] = []

    def accept(summary: str, strong: bool) -> bool:
        calls.append((summary, strong))
        return True

    tools = build_tools(store, confirm=accept)
    result = tools["remember"].handler(
        {"kind": "memory", "text": "el proyecto se llama EzAgent"}
    )
    assert result.ok is True
    assert result.data["status"] == "saved"
    assert calls and calls[0][1] is False
    assert "el proyecto se llama EzAgent" in store.read_markdown("memory")


def test_remember_tool_without_callback_never_saves(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    tools = build_tools(store)
    result = tools["remember"].handler(
        {"kind": "user", "text": "prefiero respuestas cortas"}
    )
    assert result.ok is True
    assert result.data["status"] == "cancelled"
    assert store.read_markdown("user") == ""


def test_remember_tool_confirmation_exception_cancels(tmp_path: Path) -> None:
    store = make_store(tmp_path)

    def explode(summary: str, strong: bool) -> bool:
        raise RuntimeError("UI caída")

    tools = build_tools(store, confirm=explode)
    result = tools["remember"].handler(
        {"kind": "user", "text": "prefiero respuestas cortas"}
    )
    assert result.ok is True
    assert result.data["status"] == "cancelled"
    assert store.read_markdown("user") == ""


def test_remember_does_not_write_after_registry_timeout(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    tools_memory = _tools_module()
    now = [100.0]
    monkeypatch.setattr(tools_memory.time, "monotonic", lambda: now[0])
    store = make_store(tmp_path)

    def slow_yes(summary: str, strong: bool) -> bool:
        now[0] += 10_000
        return True

    tools = build_tools(store, confirm=slow_yes)
    result = tools["remember"].handler(
        {"kind": "user", "text": "prefiero respuestas cortas"}
    )
    assert result.ok is True
    assert result.data["status"] == "cancelled"
    assert "tiempo" in result.data["message"]
    assert store.read_markdown("user") == ""


def test_remember_tool_full_returns_proposal_without_asking(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    padding = f"- [2025-01-01] {'x' * (USER_LIMIT - 20)}\n"
    store.user_path.write_text(padding, encoding="utf-8")
    calls: list[tuple[str, bool]] = []
    tools = build_tools(
        store,
        confirm=lambda summary, strong: calls.append((summary, strong)) or True,
    )
    result = tools["remember"].handler(
        {"kind": "user", "text": "dato nuevo que no cabe"}
    )
    assert result.ok is True
    assert result.data["status"] == "full"
    assert result.data["proposal"]
    assert calls == []
    assert store.read_markdown("user") == padding


# ----------------------------------------------------------------------
# Características de las herramientas
# ----------------------------------------------------------------------


def test_builder_accepts_a_confirmation_manager(tmp_path: Path) -> None:
    tools_memory = _tools_module()
    store = make_store(tmp_path)
    calls: list[tuple[str, bool, int | None]] = []

    class FakeManager:
        def ask(
            self,
            summary: str,
            *,
            strong: bool = False,
            require_number: int | None = None,
        ) -> bool:
            calls.append((summary, strong, require_number))
            return True

    config = SimpleNamespace(
        memory_dir=store.memory_dir,
        history_path=store.history_path,
        general=SimpleNamespace(confirm_timeout_s=15.0),
    )
    tools = {
        tool.name: tool
        for tool in tools_memory.build_memory_tools(
            config, store, confirmation=FakeManager()
        )
    }
    result = tools["remember"].handler(
        {"kind": "memory", "text": "el proyecto se llama EzAgent"}
    )
    assert result.ok is True
    assert result.data["status"] == "saved"
    assert calls and calls[0][1] is False
    assert calls[0][2] is None


def test_tool_levels_schemas_and_descriptions(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    tools = build_tools(store)
    assert set(tools) == {"remember", "recall", "forget", "compact_memory"}
    assert tools["remember"].level == 1
    assert tools["recall"].level == 0
    assert tools["forget"].level == 2
    assert tools["compact_memory"].level == 2

    remember_params = tools["remember"].parameters
    assert remember_params["properties"]["kind"]["enum"] == ["user", "memory"]
    assert remember_params["properties"]["text"]["maxLength"] == MAX_ENTRY_CHARS
    recall_params = tools["recall"].parameters
    assert recall_params["properties"]["limit"]["default"] == DEFAULT_RECALL_LIMIT
    assert recall_params["properties"]["limit"]["maximum"] == 10

    info = tools["remember"].describe({"kind": "user", "text": "hola"})
    assert info.level == 1
    assert "hola" in info.summary


def test_recall_tool_limit(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    for index in range(7):
        store.record("user", f"tema numero {index}")
    tools = build_tools(store)
    result = tools["recall"].handler({"query": "tema", "limit": 3})
    assert result.ok is True
    assert result.data["count"] == 3
    default_result = tools["recall"].handler({"query": "tema"})
    assert default_result.data["count"] == DEFAULT_RECALL_LIMIT


def test_recall_tool_rejects_empty_query(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    tools = build_tools(store)
    result = tools["recall"].handler({"query": "   "})
    assert result.ok is False
    assert result.error


def test_memory_result_to_dict_is_json_serializable(tmp_path: Path) -> None:
    store = make_store(tmp_path)
    saved = store.remember("user", "prefiero respuestas cortas")
    payload = saved.to_dict()
    assert json.loads(json.dumps(payload))["status"] == "saved"


def test_memory_store_accepts_config_like_object(tmp_path: Path) -> None:
    memory_dir = tmp_path / "configmemory"
    config = SimpleNamespace(
        memory_dir=memory_dir,
        history_path=memory_dir / "history.db",
    )
    store = MemoryStore(config, clock=lambda: FIXED_NOW)
    assert store.remember("user", "desde configuración").status == "saved"
    assert (memory_dir / "USER.md").exists()


