"""Pruebas de archivos, confirmaciones y deshacer (ST-08 a ST-13 de F2).

Todas las pruebas usan ``tmp_path``. La Papelera se simula moviendo los
elementos a una carpeta temporal, de modo que nunca se tocan archivos reales
del usuario.
"""

from __future__ import annotations

import json
import re
import shutil
import threading
import time
from pathlib import Path
from types import SimpleNamespace
from typing import Any

import pytest

from ezagent.agent import journal as journal_module
from ezagent.agent import tools_fs as tools_fs_module
from ezagent.agent.config import (
    AgentConfig,
    ConfirmConfig,
    GeneralConfig,
    PathsConfig,
    SkillsConfig,
    ToolsDConfig,
    WebConfig,
)
from ezagent.agent.confirm import ConfirmationManager
from ezagent.agent.journal import Journal
from ezagent.agent.policy import decide
from ezagent.agent.tools_fs import build_fs_tools

ACCEPT = ["sí", "si", "confirmo", "adelante", "dale"]
REJECT = ["no", "cancela", "para"]


# ----------------------------------------------------------------------
# Utilidades de prueba
# ----------------------------------------------------------------------
def make_config(
    tmp_path: Path,
    *,
    read_roots: list[Path],
    write_roots: list[Path],
    bulk_threshold: int = 5,
    max_file_mb: float = 10.0,
) -> AgentConfig:
    app_dir = tmp_path / "app"
    workspace = write_roots[0] if write_roots else tmp_path / "workspace"
    return AgentConfig(
        general=GeneralConfig(
            bulk_threshold=bulk_threshold,
            max_file_mb=max_file_mb,
        ),
        paths=PathsConfig(
            read_roots=[str(path) for path in read_roots],
            write_roots=[str(path) for path in write_roots],
            deny=[],
            deny_globs=[],
        ),
        confirm=ConfirmConfig(accept=list(ACCEPT), reject=list(REJECT)),
        web=WebConfig(),
        skills=SkillsConfig(trusted=[]),
        tools_d=ToolsDConfig(allowed_executables=[]),
        app_dir=app_dir,
        workspace=workspace,
        permissions_path=app_dir / "permissions.toml",
        system_md_path=app_dir / "system.md",
        memory_dir=app_dir / "memory",
        history_path=app_dir / "memory" / "history.db",
        audit_path=app_dir / "logs" / "audit.jsonl",
        journal_dir=app_dir / "journal",
        skills_dir=app_dir / "skills",
        global_skills_dir=app_dir / "global-skills",
        tools_d_dir=app_dir / "tools.d",
    )


@pytest.fixture()
def env(tmp_path: Path) -> SimpleNamespace:
    docs = tmp_path / "docs"
    workspace = tmp_path / "workspace"
    docs.mkdir()
    workspace.mkdir()
    config = make_config(tmp_path, read_roots=[docs, workspace], write_roots=[workspace])
    journal = Journal(config.journal_dir)
    tools = {spec.name: spec for spec in build_fs_tools(config, journal)}
    return SimpleNamespace(
        config=config,
        journal=journal,
        tools=tools,
        workspace=workspace,
        docs=docs,
        tmp_path=tmp_path,
    )


@pytest.fixture()
def fake_trash(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> SimpleNamespace:
    trash_dir = tmp_path / "fake-trash"
    trash_dir.mkdir()
    calls: list[Path] = []

    def fake_send(path: Path) -> None:
        source = Path(path)
        calls.append(source)
        shutil.move(str(source), str(trash_dir / f"{len(calls)}-{source.name}"))

    monkeypatch.setattr(tools_fs_module, "_send_to_trash", fake_send)
    monkeypatch.setattr(journal_module, "_send_to_trash", fake_send)
    return SimpleNamespace(calls=calls, directory=trash_dir)


def call(env: SimpleNamespace, name: str, **args: Any) -> Any:
    result = env.tools[name].handler(args)
    assert hasattr(result, "ok"), f"{name} no devolvió un ToolResult"
    return result


def journal_entries(config: AgentConfig) -> list[dict[str, Any]]:
    return [
        json.loads(path.read_text(encoding="utf-8"))
        for path in sorted(config.journal_dir.glob("*.json"))
    ]


def start_ask(
    manager: ConfirmationManager,
    summary: str = "Resumen de prueba",
    *,
    strong: bool = False,
    require_number: int | None = None,
) -> tuple[threading.Thread, dict[str, bool]]:
    box: dict[str, bool] = {}

    def target() -> None:
        box["result"] = manager.ask(
            summary,
            strong=strong,
            require_number=require_number,
        )

    thread = threading.Thread(target=target, daemon=True)
    thread.start()
    assert wait_until(lambda: manager.pending), "La confirmación no quedó pendiente"
    return thread, box


def wait_until(predicate: Any, timeout: float = 2.0) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.005)
    return bool(predicate())


def make_manager(timeout_s: float = 1.0) -> ConfirmationManager:
    return ConfirmationManager(accept=ACCEPT, reject=REJECT, timeout_s=timeout_s)


# ----------------------------------------------------------------------
# ST-08: borrado solo con send2trash
# ----------------------------------------------------------------------
def test_st08_agent_has_no_definitive_removal_calls() -> None:
    agent_dir = Path(__file__).resolve().parents[1] / "src" / "ezagent" / "agent"
    forbidden = re.compile(
        r"\bos\.remove\s*\(|\bshutil\.rmtree\s*\(|\bos\.rmdir\s*\(|\.unlink\s*\("
    )
    offenders = [
        path.name
        for path in sorted(agent_dir.glob("*.py"))
        if forbidden.search(path.read_text(encoding="utf-8"))
    ]
    assert offenders == []


def test_st08_delete_path_uses_send2trash(
    env: SimpleNamespace, fake_trash: SimpleNamespace
) -> None:
    target = env.workspace / "borrar.txt"
    target.write_text("contenido", encoding="utf-8")

    result = call(env, "delete_path", path=str(target))

    assert result.ok, result.error
    assert target in fake_trash.calls
    assert not target.exists()
    assert fake_trash.directory.iterdir()


def test_st08_delete_reports_missing_send2trash(
    env: SimpleNamespace, monkeypatch: pytest.MonkeyPatch
) -> None:
    def unavailable(path: Path) -> None:
        raise RuntimeError("Falta el paquete send2trash; no se puede enviar a la Papelera.")

    monkeypatch.setattr(tools_fs_module, "_send_to_trash", unavailable)
    target = env.workspace / "intacto.txt"
    target.write_text("no se borra", encoding="utf-8")

    result = call(env, "delete_path", path=str(target))

    assert not result.ok
    assert "send2trash" in (result.error or "")
    assert target.exists()


# ----------------------------------------------------------------------
# ST-09 y ST-10: confirmación fuerte
# ----------------------------------------------------------------------
def test_st09_delete_folder_requires_strong_confirmation(env: SimpleNamespace) -> None:
    folder = env.workspace / "carpeta_llena"
    folder.mkdir()
    (folder / "uno.txt").write_text("1", encoding="utf-8")

    info = env.tools["delete_path"].describe({"path": str(folder)})
    decision = decide(
        info.level,
        False,
        info.bulk,
        info.always_confirm,
        False,
        env.config.general.bulk_threshold,
    )

    assert info.always_confirm is True
    assert info.level == 2
    assert info.bulk == 1
    assert decision.action == "confirm_strong"


def test_st09_empty_folder_is_also_strong(env: SimpleNamespace) -> None:
    folder = env.workspace / "vacia"
    folder.mkdir()

    info = env.tools["delete_path"].describe({"path": str(folder)})
    decision = decide(
        info.level, False, info.bulk, info.always_confirm, False,
        env.config.general.bulk_threshold,
    )

    assert info.bulk == 0
    assert decision.action == "confirm_strong"


def test_st09_strong_number_matches_real_count(env: SimpleNamespace) -> None:
    folder = env.workspace / "pequena"
    folder.mkdir()
    (folder / "uno.txt").write_text("1", encoding="utf-8")
    (folder / "dos.txt").write_text("2", encoding="utf-8")

    info = env.tools["delete_path"].describe({"path": str(folder)})
    decision = decide(
        info.level, False, info.bulk, info.always_confirm, False,
        env.config.general.bulk_threshold,
    )
    # Misma lógica que loop.run_turn para exigir el número.
    require_number = (
        info.bulk
        if decision.action == "confirm_strong" and info.bulk > 0
        else None
    )
    manager = make_manager()
    thread, box = start_ask(
        manager, info.summary, strong=True, require_number=require_number
    )

    assert manager.submit_text("confirmo 2") is True
    thread.join(timeout=2.0)
    assert box["result"] is True
    assert "2 elementos" in info.summary


def test_st09_file_delete_is_normal_confirmation(env: SimpleNamespace) -> None:
    target = env.workspace / "archivo.txt"
    target.write_text("x", encoding="utf-8")

    info = env.tools["delete_path"].describe({"path": str(target)})
    decision = decide(
        info.level, False, info.bulk, info.always_confirm, False,
        env.config.general.bulk_threshold,
    )

    assert info.always_confirm is True
    assert decision.action == "confirm"


def test_st10_more_than_five_items_requires_strong_confirmation(
    env: SimpleNamespace,
) -> None:
    folder = env.workspace / "masiva"
    folder.mkdir()
    for index in range(6):
        (folder / f"elemento_{index}.txt").write_text(str(index), encoding="utf-8")

    info = env.tools["copy_path"].describe(
        {"src": str(folder), "dst": str(env.workspace / "copia")}
    )
    decision = decide(
        info.level, False, info.bulk, info.always_confirm, False,
        env.config.general.bulk_threshold,
    )

    assert info.bulk == 6
    assert decision.action == "confirm_strong"


# ----------------------------------------------------------------------
# ST-12: respuestas ambiguas y timeout
# ----------------------------------------------------------------------
def test_st10_delete_more_than_five_items(
    env: SimpleNamespace, fake_trash: SimpleNamespace
) -> None:
    folder = env.workspace / "masiva_borrado"
    folder.mkdir()
    for index in range(6):
        (folder / f"elemento_{index}.txt").write_text(str(index), encoding="utf-8")

    info = env.tools["delete_path"].describe({"path": str(folder)})
    decision = decide(
        info.level, False, info.bulk, info.always_confirm, False,
        env.config.general.bulk_threshold,
    )
    result = call(env, "delete_path", path=str(folder))

    assert info.bulk == 6
    assert decision.action == "confirm_strong"
    assert result.ok, result.error
    assert result.data["items"] == 6
    assert not folder.exists()


def test_st12_ambiguous_answer_rejects() -> None:
    manager = make_manager()
    thread, box = start_ask(manager)

    assert manager.submit_text("mmm") is False
    thread.join(timeout=2.0)

    assert box["result"] is False
    assert manager.pending is False


def test_st12_mixed_accept_and_reject_rejects() -> None:
    manager = make_manager()
    thread, box = start_ask(manager)

    assert manager.submit_text("sí, pero no") is False
    thread.join(timeout=2.0)

    assert box["result"] is False


def test_st12_multiword_accept_phrase_requires_all_words() -> None:
    manager = ConfirmationManager(
        accept=["de acuerdo"], reject=REJECT, timeout_s=1.0
    )

    thread, box = start_ask(manager)
    assert manager.submit_text("acuerdo") is False
    thread.join(timeout=2.0)
    assert box["result"] is False

    thread, box = start_ask(manager)
    assert manager.submit_text("de acuerdo") is True
    thread.join(timeout=2.0)
    assert box["result"] is True


def test_st12_substring_is_not_acceptance() -> None:
    manager = make_manager()
    thread, box = start_ask(manager)

    assert manager.submit_text("silencio") is False
    thread.join(timeout=2.0)

    assert box["result"] is False


def test_st12_whole_word_accepts() -> None:
    manager = make_manager()
    thread, box = start_ask(manager)

    assert manager.submit_text("Sí") is True
    thread.join(timeout=2.0)

    assert box["result"] is True


def test_st12_timeout_rejects() -> None:
    manager = make_manager(timeout_s=0.1)

    started = time.monotonic()
    accepted = manager.ask("Resumen")
    elapsed = time.monotonic() - started

    assert accepted is False
    assert elapsed >= 0.08
    assert manager.pending is False


def test_st12_submit_without_pending_returns_none() -> None:
    manager = make_manager()
    assert manager.submit_text("sí") is None


def test_st12_click_resolves() -> None:
    manager = make_manager()
    thread, box = start_ask(manager)
    manager.submit_click(True)
    thread.join(timeout=2.0)
    assert box["result"] is True

    thread, box = start_ask(manager)
    manager.submit_click(False)
    thread.join(timeout=2.0)
    assert box["result"] is False


def test_st12_cancel_rejects() -> None:
    manager = make_manager()
    thread, box = start_ask(manager)

    manager.cancel("el usuario cambió de tema")
    thread.join(timeout=2.0)

    assert box["result"] is False
    assert manager.pending is False


def test_st12_strong_requires_confirmo_and_number() -> None:
    manager = make_manager()

    thread, box = start_ask(manager, strong=True, require_number=3)
    assert manager.submit_text("sí") is False
    thread.join(timeout=2.0)
    assert box["result"] is False

    thread, box = start_ask(manager, strong=True, require_number=3)
    assert manager.submit_text("confirmo") is False
    thread.join(timeout=2.0)
    assert box["result"] is False

    thread, box = start_ask(manager, strong=True, require_number=3)
    assert manager.submit_text("confirmo, 3 elementos") is True
    thread.join(timeout=2.0)
    assert box["result"] is True


def test_st12_on_ask_receives_summary_and_strength() -> None:
    asked: list[tuple[str, bool]] = []
    manager = ConfirmationManager(
        accept=ACCEPT,
        reject=REJECT,
        timeout_s=1.0,
        on_ask=lambda summary, strong: asked.append((summary, strong)),
    )

    thread, box = start_ask(manager, "Voy a borrar 3 archivos", strong=True)
    assert asked == [("Voy a borrar 3 archivos", True)]
    manager.submit_click(False)
    thread.join(timeout=2.0)
    assert box["result"] is False


# ----------------------------------------------------------------------
# ST-13: una confirmación por operación
# ----------------------------------------------------------------------
def test_st13_one_confirmation_per_operation() -> None:
    manager = make_manager()

    thread, box = start_ask(manager, "Primera operación")
    assert manager.submit_text("sí") is True
    thread.join(timeout=2.0)
    assert box["result"] is True

    # La confirmación ya se consumió: no hay pendiente que resolver.
    assert manager.submit_text("sí") is None
    assert manager.pending is False

    # Nueva operación: necesita una confirmación nueva.
    thread, box = start_ask(manager, "Segunda operación")
    assert manager.submit_text("¿qué hora es?") is False
    thread.join(timeout=2.0)
    assert box["result"] is False

    thread, box = start_ask(manager, "Tercera operación")
    assert manager.submit_text("adelante") is True
    thread.join(timeout=2.0)
    assert box["result"] is True


# ----------------------------------------------------------------------
# Diario y deshacer
# ----------------------------------------------------------------------
def test_journal_records_inverse_after_create(
    env: SimpleNamespace, fake_trash: SimpleNamespace
) -> None:
    target = env.workspace / "Facturas"
    result = call(env, "create_dir", path=str(target))
    assert result.ok, result.error

    entries = journal_entries(env.config)
    assert len(entries) == 1
    entry = entries[0]
    assert entry["kind"] == "create_dir"
    assert entry["undone"] is False
    assert entry["steps"][0]["action"] == "trash"
    assert entry["steps"][0]["path"] == str(target)

    undone = call(env, "undo_last", n=1)
    assert undone.ok, undone.error
    assert not target.exists()
    assert target in fake_trash.calls
    assert journal_entries(env.config)[0]["undone"] is True


def test_journal_undo_write_and_copy(
    env: SimpleNamespace, fake_trash: SimpleNamespace
) -> None:
    created = env.workspace / "nota.txt"
    assert call(env, "write_file", path=str(created), content="hola").ok
    assert call(env, "undo_last", n=1).ok
    assert not created.exists()

    source = env.workspace / "origen.txt"
    source.write_text("origen", encoding="utf-8")
    copied = env.workspace / "copia.txt"
    assert call(env, "copy_path", src=str(source), dst=str(copied)).ok
    assert copied.exists()
    assert call(env, "undo_last", n=1).ok
    assert not copied.exists()
    assert source.exists()


def test_journal_undo_move(env: SimpleNamespace) -> None:
    source = env.workspace / "antes.txt"
    source.write_text("datos", encoding="utf-8")
    destination = env.workspace / "despues.txt"

    assert call(env, "move_path", src=str(source), dst=str(destination)).ok
    assert not source.exists()
    assert destination.exists()

    assert call(env, "undo_last", n=1).ok
    assert source.exists()
    assert not destination.exists()


def test_journal_undo_last_n_most_recent_first(
    env: SimpleNamespace, fake_trash: SimpleNamespace
) -> None:
    folders = [env.workspace / name for name in ("uno", "dos", "tres")]
    for folder in folders:
        assert call(env, "create_dir", path=str(folder)).ok

    result = call(env, "undo_last", n=2)
    assert result.ok, result.error
    assert result.data["undone"] == 2
    assert folders[0].exists()
    assert not folders[1].exists()
    assert not folders[2].exists()

    final = call(env, "undo_last", n=1)
    assert final.ok, final.error
    assert not folders[0].exists()

    empty = call(env, "undo_last", n=1)
    assert empty.ok
    assert empty.data["undone"] == 0


def test_journal_default_directory_uses_appdata(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("APPDATA", str(tmp_path))
    journal = Journal()

    assert journal.directory == tmp_path / "EzAgent" / "journal"
    assert journal.directory.is_dir()


def test_journal_invalid_n_rejected(env: SimpleNamespace) -> None:
    assert not call(env, "undo_last", n=0).ok
    with pytest.raises(ValueError):
        env.journal.undo_last(0)


def test_journal_undo_revalidates_paths(
    env: SimpleNamespace, fake_trash: SimpleNamespace
) -> None:
    outside = env.docs / "no_tocar.txt"
    outside.write_text("sensible", encoding="utf-8")
    env.journal.record_create(outside, kind="tampered")

    result = call(env, "undo_last", n=1)

    assert not result.ok
    assert outside.exists()
    assert result.data["failed"] == 1


# ----------------------------------------------------------------------
# write_file: .bak, tamaño y escritura atómica
# ----------------------------------------------------------------------
def test_write_file_creates_missing_parent_and_reports_it(env: SimpleNamespace) -> None:
    target = env.workspace / "sub" / "archivo.txt"
    info = env.tools["write_file"].describe(
        {"path": str(target), "content": "hola", "overwrite": False}
    )
    assert "carpeta" in info.summary
    assert str(target.parent) in info.summary

    result = call(env, "write_file", path=str(target), content="hola")
    assert result.ok, result.error
    assert target.read_text(encoding="utf-8") == "hola"
    assert result.data["created_parent"] == str(target.parent)


def test_write_file_overwrite_creates_bak_and_undo_restores(
    env: SimpleNamespace,
) -> None:
    target = env.workspace / "documento.txt"
    target.write_text("original", encoding="utf-8")

    info = env.tools["write_file"].describe(
        {"path": str(target), "content": "nuevo", "overwrite": True}
    )
    assert info.level == 2
    assert info.always_confirm is True

    result = call(env, "write_file", path=str(target), content="nuevo", overwrite=True)
    assert result.ok, result.error
    assert target.read_text(encoding="utf-8") == "nuevo"
    backup = Path(result.data["backup"])
    assert backup.exists()
    assert backup.read_text(encoding="utf-8") == "original"

    assert call(env, "undo_last", n=1).ok
    assert target.read_text(encoding="utf-8") == "original"


def test_write_file_does_not_overwrite_existing_bak(env: SimpleNamespace) -> None:
    target = env.workspace / "cuenta.txt"
    target.write_text("v1", encoding="utf-8")

    first = call(env, "write_file", path=str(target), content="v2", overwrite=True)
    second = call(env, "write_file", path=str(target), content="v3", overwrite=True)

    assert first.ok and second.ok
    assert target.read_text(encoding="utf-8") == "v3"
    backups = sorted(env.workspace.glob("cuenta.txt*.bak"))
    assert len(backups) >= 2
    contents = {path.read_text(encoding="utf-8") for path in backups}
    assert "v1" in contents
    assert "v2" in contents


def test_write_file_without_overwrite_keeps_existing(env: SimpleNamespace) -> None:
    target = env.workspace / "existente.txt"
    target.write_text("original", encoding="utf-8")

    result = call(env, "write_file", path=str(target), content="otro")

    assert not result.ok
    assert target.read_text(encoding="utf-8") == "original"


def test_write_file_respects_max_size(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    config = make_config(
        tmp_path, read_roots=[workspace], write_roots=[workspace], max_file_mb=0.00001
    )
    journal = Journal(config.journal_dir)
    tools = {spec.name: spec for spec in build_fs_tools(config, journal)}
    local = SimpleNamespace(config=config, journal=journal, tools=tools, workspace=workspace)

    result = call(local, "write_file", path=str(workspace / "grande.txt"), content="x" * 100)

    assert not result.ok
    assert "MB" in (result.error or "")
    assert not (workspace / "grande.txt").exists()


def test_write_file_is_atomic_and_leaves_no_temp(env: SimpleNamespace) -> None:
    target = env.workspace / "atomico.txt"
    assert call(env, "write_file", path=str(target), content="contenido").ok
    assert target.read_text(encoding="utf-8") == "contenido"
    leftovers = [path for path in env.workspace.iterdir() if path.name.startswith(".ezagent-")]
    assert leftovers == []


# ----------------------------------------------------------------------
# copy_path y move_path
# ----------------------------------------------------------------------
def test_copy_file_without_overwrite(env: SimpleNamespace) -> None:
    source = env.workspace / "origen.txt"
    source.write_text("datos", encoding="utf-8")
    destination = env.workspace / "destino.txt"

    info = env.tools["copy_path"].describe(
        {"src": str(source), "dst": str(destination)}
    )
    assert info.level == 1
    assert info.always_confirm is False

    result = call(env, "copy_path", src=str(source), dst=str(destination))
    assert result.ok, result.error
    assert destination.read_text(encoding="utf-8") == "datos"
    assert source.exists()


def test_copy_overwrite_needs_level_two_and_backs_up(
    env: SimpleNamespace, fake_trash: SimpleNamespace
) -> None:
    source = env.workspace / "nuevo.txt"
    source.write_text("nuevo", encoding="utf-8")
    destination = env.workspace / "destino.txt"
    destination.write_text("viejo", encoding="utf-8")

    info = env.tools["copy_path"].describe(
        {"src": str(source), "dst": str(destination)}
    )
    assert info.level == 2
    assert info.always_confirm is True

    result = call(env, "copy_path", src=str(source), dst=str(destination))
    assert result.ok, result.error
    assert destination.read_text(encoding="utf-8") == "nuevo"
    backup = Path(result.data["backup"])
    assert backup.read_text(encoding="utf-8") == "viejo"

    assert call(env, "undo_last", n=1).ok
    assert destination.read_text(encoding="utf-8") == "viejo"
    assert source.exists()


def test_move_rename_and_undo(env: SimpleNamespace) -> None:
    source = env.workspace / "original.txt"
    source.write_text("datos", encoding="utf-8")
    destination = env.workspace / "renombrado.txt"

    info = env.tools["move_path"].describe(
        {"src": str(source), "dst": str(destination)}
    )
    assert info.level == 1

    assert call(env, "move_path", src=str(source), dst=str(destination)).ok
    assert destination.exists()
    assert not source.exists()
    assert call(env, "undo_last", n=1).ok
    assert source.exists()
    assert not destination.exists()


def test_move_overwrite_replaces_and_backs_up(env: SimpleNamespace) -> None:
    source = env.workspace / "origen.txt"
    source.write_text("nuevo", encoding="utf-8")
    destination = env.workspace / "destino.txt"
    destination.write_text("viejo", encoding="utf-8")

    info = env.tools["move_path"].describe(
        {"src": str(source), "dst": str(destination)}
    )
    assert info.level == 2
    assert info.always_confirm is True

    result = call(env, "move_path", src=str(source), dst=str(destination))
    assert result.ok, result.error
    assert not source.exists()
    assert destination.read_text(encoding="utf-8") == "nuevo"
    backup = Path(result.data["backup"])
    assert backup.read_text(encoding="utf-8") == "viejo"


def test_move_file_overwrite_does_not_need_send2trash(
    env: SimpleNamespace, monkeypatch: pytest.MonkeyPatch
) -> None:
    def unavailable(path: Path) -> None:
        raise RuntimeError("Falta el paquete send2trash.")

    monkeypatch.setattr(tools_fs_module, "_send_to_trash", unavailable)
    source = env.workspace / "origen.txt"
    source.write_text("nuevo", encoding="utf-8")
    destination = env.workspace / "destino.txt"
    destination.write_text("viejo", encoding="utf-8")

    result = call(env, "move_path", src=str(source), dst=str(destination))

    assert result.ok, result.error
    assert not source.exists()
    assert destination.read_text(encoding="utf-8") == "nuevo"

    assert call(env, "undo_last", n=1).ok
    assert source.read_text(encoding="utf-8") == "nuevo"
    assert destination.read_text(encoding="utf-8") == "viejo"


def test_copy_or_move_into_containing_folder_is_rejected(
    env: SimpleNamespace,
) -> None:
    parent = env.workspace / "padre"
    child = parent / "hijo"
    child.mkdir(parents=True)
    (child / "dato.txt").write_text("importante", encoding="utf-8")

    copied = call(env, "copy_path", src=str(child), dst=str(parent))
    moved = call(env, "move_path", src=str(child), dst=str(parent))

    assert not copied.ok
    assert not moved.ok
    assert parent.exists()
    assert child.exists()
    assert (child / "dato.txt").read_text(encoding="utf-8") == "importante"


def test_copy_type_mismatch_is_rejected(env: SimpleNamespace) -> None:
    source = env.workspace / "carpeta"
    source.mkdir()
    destination = env.workspace / "archivo.txt"
    destination.write_text("x", encoding="utf-8")

    result = call(env, "copy_path", src=str(source), dst=str(destination))
    assert not result.ok
    assert destination.read_text(encoding="utf-8") == "x"


# ----------------------------------------------------------------------
# Límites de rutas
# ----------------------------------------------------------------------
def test_write_outside_write_roots_is_rejected(env: SimpleNamespace) -> None:
    target = env.docs / "fuera.txt"
    result = call(env, "write_file", path=str(target), content="no")
    assert not result.ok
    assert not target.exists()

    escape = env.workspace / ".." / "docs" / "escape.txt"
    escaped = call(env, "write_file", path=str(escape), content="no")
    assert not escaped.ok
    assert not (env.docs / "escape.txt").exists()


def test_copy_source_outside_read_roots_is_rejected(
    env: SimpleNamespace, tmp_path: Path
) -> None:
    outside = tmp_path / "outside.txt"
    outside.write_text("x", encoding="utf-8")
    result = call(env, "copy_path", src=str(outside), dst=str(env.workspace / "copia.txt"))
    assert not result.ok
    assert not (env.workspace / "copia.txt").exists()


def test_delete_or_move_root_is_rejected(
    env: SimpleNamespace, fake_trash: SimpleNamespace
) -> None:
    delete_result = call(env, "delete_path", path=str(env.workspace))
    assert not delete_result.ok
    assert env.workspace.exists()

    target = env.workspace / "hijo.txt"
    target.write_text("x", encoding="utf-8")
    move_result = call(env, "move_path", src=str(env.workspace), dst=str(target))
    assert not move_result.ok
    assert env.workspace.exists()


# ----------------------------------------------------------------------
# open_path
# ----------------------------------------------------------------------
def test_open_path_opens_allowed_file(
    env: SimpleNamespace, monkeypatch: pytest.MonkeyPatch
) -> None:
    opened: list[str] = []
    monkeypatch.setattr(
        tools_fs_module.os, "startfile", lambda target: opened.append(target), raising=False
    )
    target = env.docs / "documento.txt"
    target.write_text("hola", encoding="utf-8")

    result = call(env, "open_path", path_or_url=str(target))

    assert result.ok, result.error
    assert len(opened) == 1
    assert Path(opened[0]) == target.resolve()
    assert result.data["kind"] == "file"


def test_open_path_opens_http_url(
    env: SimpleNamespace, monkeypatch: pytest.MonkeyPatch
) -> None:
    opened: list[str] = []
    monkeypatch.setattr(
        tools_fs_module.os, "startfile", lambda target: opened.append(target), raising=False
    )

    result = call(env, "open_path", path_or_url="https://example.com/pagina")

    assert result.ok, result.error
    assert opened == ["https://example.com/pagina"]
    assert result.data["kind"] == "url"


def test_open_path_rejects_other_schemes_and_folders(
    env: SimpleNamespace, monkeypatch: pytest.MonkeyPatch
) -> None:
    opened: list[str] = []
    monkeypatch.setattr(
        tools_fs_module.os, "startfile", lambda target: opened.append(target), raising=False
    )

    scheme = call(env, "open_path", path_or_url="file:///C:/Windows/win.ini")
    folder = call(env, "open_path", path_or_url=str(env.docs))
    missing = call(env, "open_path", path_or_url=str(env.docs / "no-existe.txt"))

    assert not scheme.ok
    assert not folder.ok
    assert not missing.ok
    assert opened == []


# ----------------------------------------------------------------------
# Resúmenes y catálogo
# ----------------------------------------------------------------------
def test_summaries_include_real_paths_and_counts(env: SimpleNamespace) -> None:
    folder = env.workspace / "Facturas"
    folder.mkdir()
    (folder / "a.txt").write_text("a", encoding="utf-8")

    delete_info = env.tools["delete_path"].describe({"path": str(folder)})
    assert str(folder) in delete_info.summary
    assert "Papelera" in delete_info.summary
    assert "1 elemento" in delete_info.summary

    file_info = env.tools["delete_path"].describe({"path": str(folder / "a.txt")})
    assert "archivo" in file_info.summary
    assert str(folder / "a.txt") in file_info.summary

    write_info = env.tools["write_file"].describe(
        {"path": str(env.workspace / "nuevo.txt"), "content": "abc"}
    )
    assert str(env.workspace / "nuevo.txt") in write_info.summary


def test_tool_catalog_has_expected_names_and_levels(env: SimpleNamespace) -> None:
    assert set(env.tools) == {
        "create_dir",
        "write_file",
        "copy_path",
        "move_path",
        "delete_path",
        "open_path",
        "undo_last",
    }
    assert env.tools["delete_path"].level == 2
    for name in ("create_dir", "write_file", "copy_path", "move_path", "open_path", "undo_last"):
        assert env.tools[name].level == 1
    schema = env.tools["write_file"].schema()
    assert schema["type"] == "function"
    assert schema["function"]["name"] == "write_file"
