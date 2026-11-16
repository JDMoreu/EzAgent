"""Comprobaciones de los artefactos del perfil (F7-03).

Cada dirección usa la API del port de Rust (binario `ezagent_compat`) y la del
Python del `.venv`. Los datos del usuario solo se comparan en memoria.
"""

from __future__ import annotations

import json

import checks_data
from dumps import (
    CHAT_MESSAGE,
    CHAT_REPLY,
    UNKNOWN_VOICE_KEYS,
    agent_dump,
    chat_dump,
    first_word,
    journal_dump,
    llm_dump,
    system_dump,
    tasks_dump,
    voice_dump,
)
from support import Report, Workspace


def _check_voice(workspace: Workspace, report: Report) -> None:
    case = workspace.case("voice-read")
    report.compare(
        "voice.json", "Python→Rust (leer)", voice_dump(case.home), case.probe("dump-voice")
    )

    case = workspace.case("voice-rust-write")
    case.probe("save-voice", "--set", "ui_scale=1.05")
    py, rs = voice_dump(case.home), case.probe("dump-voice")
    raw = json.loads((case.home / "voice.json").read_text(encoding="utf-8"))
    kept = all(key in raw for key in UNKNOWN_VOICE_KEYS)
    report.compare(
        "voice.json",
        "Rust→Python (escribir)",
        py,
        rs,
        "88 campos; claves desconocidas conservadas" if kept else "claves desconocidas perdidas",
    )
    if not kept:
        report.add("voice.json", "Rust→Python (claves desconocidas)", False)

    case = workspace.case("voice-python-write")
    from ezagent.voice.config import VoiceConfig

    config = VoiceConfig.load(case.home / "voice.json")
    config.ui_scale = 1.11
    config.save(case.home / "voice.json")
    report.compare(
        "voice.json", "Python→Rust (escribir)", voice_dump(case.home), case.probe("dump-voice")
    )


def _check_llm(workspace: Workspace, report: Report) -> None:
    case = workspace.case("llm-read")
    report.compare("llm.json", "Python→Rust (leer)", llm_dump(case.home), case.probe("dump-llm"))

    case = workspace.case("llm-rust-write")
    before = set(case.probe("dump-llm")["catalog"])
    case.probe("save-llm")
    py, rs = llm_dump(case.home), case.probe("dump-llm")
    after = set(py["catalog"])
    report.compare("llm.json", "Rust→Python (escribir)", py, rs, "perfiles existentes conservados")
    report.add(
        "llm.json", "Rust→Python (perfil nuevo)", before <= after and "saved:compat r1" in after
    )

    case = workspace.case("llm-python-write")
    from ezagent import llm_providers

    llm_providers.save_provider_profile(
        "compat-py",
        model="modelo-py",
        base_url="http://127.0.0.1:9/v1",
        api_key="clave-py",
        api_style="openai",
        profile_name="Compat Py",
        config_file=case.home / "llm.json",
    )
    report.compare(
        "llm.json", "Python→Rust (escribir)", llm_dump(case.home), case.probe("dump-llm")
    )


def check_settings(workspace: Workspace, report: Report) -> None:
    _check_voice(workspace, report)
    _check_llm(workspace, report)


def check_agent_files(workspace: Workspace, report: Report) -> None:
    case = workspace.case("agent-read")
    report.compare(
        "permissions.toml", "Python→Rust (leer)", agent_dump(case.home), case.probe("dump-agent")
    )
    report.compare(
        "system.md", "Python→Rust (leer)", system_dump(case.home), case.probe("dump-system")
    )
    _check_default_templates(workspace, report)


def _check_default_templates(workspace: Workspace, report: Report) -> None:
    rust_case = workspace.case("agent-rust-default")
    (rust_case.home / "permissions.toml").unlink(missing_ok=True)
    rust_dump = rust_case.probe("dump-agent")
    rust_template = (rust_case.home / "permissions.toml").read_bytes()
    report.compare(
        "permissions.toml",
        "Rust→Python (leer plantilla)",
        agent_dump(rust_case.home),
        rust_dump,
        "Python lee la plantilla escrita por Rust",
    )
    python_case = workspace.case("agent-python-default")
    (python_case.home / "permissions.toml").unlink(missing_ok=True)
    agent_dump(python_case.home)
    python_template = (python_case.home / "permissions.toml").read_bytes()
    report.add(
        "permissions.toml",
        "plantilla por defecto (Rust = Python)",
        rust_template == python_template,
        f"Rust {len(rust_template)} B / Python {len(python_template)} B",
    )

    rust_case = workspace.case("system-rust-default")
    (rust_case.home / "system.md").unlink(missing_ok=True)
    rust_dump = rust_case.probe("dump-system")
    rust_system = (rust_case.home / "system.md").read_bytes()
    report.compare(
        "system.md",
        "Rust→Python (leer plantilla)",
        system_dump(rust_case.home),
        rust_dump,
        "Python lee la plantilla escrita por Rust",
    )
    python_case = workspace.case("system-python-default")
    (python_case.home / "system.md").unlink(missing_ok=True)
    system_dump(python_case.home)
    report.add(
        "system.md",
        "plantilla por defecto (Rust = Python)",
        rust_system == (python_case.home / "system.md").read_bytes(),
    )


def check_chat(workspace: Workspace, report: Report) -> None:
    case = workspace.case("chat-read")
    query = "compat"
    entries = chat_dump(case.home, query)["recent"]
    if entries:
        query = first_word(entries[-1]["text"])
    report.compare(
        "chat.jsonl",
        "Python→Rust (leer)",
        chat_dump(case.home, query),
        case.probe("dump-chat", "--query", query),
    )

    case = workspace.case("chat-rust-write")
    before = len(chat_dump(case.home, "x")["recent"])
    case.probe("save-chat")
    py, rs = chat_dump(case.home, "x"), case.probe("dump-chat", "--query", "x")
    texts = [entry["text"] for items in py["entries"].values() for entry in items]
    report.compare(
        "chat.jsonl",
        "Rust→Python (escribir)",
        py,
        rs,
        f"{before} mensajes previos conservados; respuesta corregida",
    )
    report.add(
        "chat.jsonl", "Rust→Python (reescritura)", CHAT_REPLY in texts and CHAT_MESSAGE in texts
    )

    case = workspace.case("chat-python-write")
    from ezagent.voice.chat.store import ChatStore

    store = ChatStore(case.home / "history" / "chat.jsonl")
    first = store.session
    store.append("user", CHAT_MESSAGE, "chat", 1_700_000_000.125)
    second = store.new_session()
    store.append("assistant", "respuesta de compatibilidad F7-03", "voice", 1_700_000_001.5)
    replaced = store.replace_last("assistant", "respuesta de compatibilidad F7-03", CHAT_REPLY)
    resumed = store.resume(first)
    deleted = store.delete_session(second)
    report.compare(
        "chat.jsonl",
        "Python→Rust (escribir)",
        chat_dump(case.home, "compatibilidad"),
        case.probe("dump-chat", "--query", "compatibilidad"),
        "append, replace_last, resume y delete_session",
    )
    report.add("chat.jsonl", "Python→Rust (reescritura)", replaced and resumed and deleted)


def check_tasks(workspace: Workspace, report: Report) -> None:
    case = workspace.case("tasks-read")
    report.compare(
        "tasks.json", "Python→Rust (leer)", tasks_dump(case.home), case.probe("dump-tasks")
    )

    case = workspace.case("tasks-rust-write")
    case.probe("save-tasks")
    py, rs = tasks_dump(case.home), case.probe("dump-tasks")
    report.compare("tasks.json", "Rust→Python (escribir)", py, rs, "tarea y cadena nuevas")

    case = workspace.case("tasks-python-write")
    from ezagent.voice.tasks import TaskRegistry

    registry = TaskRegistry(case.home / "tasks.json")
    registry.add("compat:1", "bob", "tarea de compatibilidad", created_at=1_700_000_100.0)
    registry.add_chain(
        {
            "source": "compat:1",
            "agent": "cy",
            "task": "etapa de compatibilidad",
            "created_at": 1_700_000_101.0,
        }
    )
    report.compare(
        "tasks.json", "Python→Rust (escribir)", tasks_dump(case.home), case.probe("dump-tasks")
    )


def check_journal(workspace: Workspace, report: Report) -> None:
    case = workspace.case("journal-read")
    report.compare(
        "journal", "Python→Rust (leer)", journal_dump(case.home), case.probe("dump-journal")
    )

    case = workspace.case("journal-rust-write")
    before = len(journal_dump(case.home)["entries"])
    case.probe("save-journal")
    py, rs = journal_dump(case.home), case.probe("dump-journal")
    report.compare(
        "journal", "Rust→Python (escribir)", py, rs, f"{before} entradas previas conservadas"
    )

    case = workspace.case("journal-python-write")
    from ezagent.agent.journal import Journal

    Journal(case.home / "journal").record(
        "create", "Entrada de Python F7-03", [{"action": "trash", "path": str(case.home / "x.txt")}]
    )
    report.compare(
        "journal", "Python→Rust (escribir)", journal_dump(case.home), case.probe("dump-journal")
    )


def checks(workspace: Workspace, report: Report) -> None:
    for check in (
        check_settings,
        check_agent_files,
        check_chat,
        check_tasks,
        check_journal,
    ):
        check(workspace, report)
    checks_data.checks(workspace, report)
