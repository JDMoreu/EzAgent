"""Comprobaciones de los artefactos del perfil (F7-03).

Cada dirección usa la API del port de Rust (binario `ezagent_compat`) y la del
Python del `.venv`. Los datos del usuario solo se comparan en memoria.
"""

from __future__ import annotations

import json
import sqlite3
import sys
from dataclasses import asdict
from pathlib import Path

from support import SRC, Report, Workspace, jsonable

sys.path.insert(0, str(SRC))

UNKNOWN_VOICE_KEYS = ("input_device_name", "notch_x", "notch_y", "ui_palette", "ui_style")
CHAT_MESSAGE = "mensaje de compatibilidad F7-03"
CHAT_REPLY = "respuesta corregida de compatibilidad F7-03"
MEMORY_TEXT = "mensaje de compatibilidad F7-03"
MEMORY_LINE = "hecho de compatibilidad F7-03"


def voice_dump(home: Path):
    from ezagent.voice.config import VoiceConfig

    return {"values": asdict(VoiceConfig.load(home / "voice.json"))}


def llm_dump(home: Path):
    from ezagent import llm_providers

    catalog = llm_providers.load_provider_catalog(config_file=home / "llm.json")
    resolved = llm_providers.resolve_llm(config_file=home / "llm.json")
    return {
        "order": list(catalog),
        "catalog": catalog,
        "resolved": {
            "provider": resolved.provider,
            "model": resolved.model,
            "base_url": resolved.base_url,
            "api_key": resolved.api_key,
            "source": resolved.source,
            "api_style": resolved.api_style,
        },
    }


def agent_dump(home: Path):
    from ezagent.agent.config import AgentConfig

    config = AgentConfig.load(home / "permissions.toml")
    names = (
        "app_dir",
        "workspace",
        "permissions_path",
        "system_md_path",
        "memory_dir",
        "history_path",
        "audit_path",
        "journal_dir",
        "skills_dir",
        "global_skills_dir",
        "tools_d_dir",
    )
    return {
        "general": asdict(config.general),
        "paths": asdict(config.paths),
        "confirm": asdict(config.confirm),
        "web": asdict(config.web),
        "skills": asdict(config.skills),
        "tools_d": asdict(config.tools_d),
        "names": {name: str(getattr(config, name)) for name in names},
        "warnings": [],
    }


def system_dump(home: Path):
    from ezagent.agent.config import AgentConfig
    from ezagent.agent.system_prompt import SystemPromptBuilder

    config = AgentConfig.load(home / "permissions.toml")
    return {"text": SystemPromptBuilder(config).last_valid_text}


def chat_dump(home: Path, query: str):
    from ezagent.voice.chat.store import ChatStore

    store = ChatStore(home / "history" / "chat.jsonl")
    sessions = store.sessions()
    return {
        "path": store.path,
        "session": store.session,
        "len": len(store),
        "entries": {
            str(info.session): [asdict(entry) for entry in store.entries(info.session)]
            for info in sessions
        },
        "sessions": [asdict(info) for info in sessions],
        "recent": [asdict(entry) for entry in store.recent(10)],
        "search": [asdict(entry) for entry in store.search(query, 5)],
    }


def tasks_dump(home: Path):
    from ezagent.voice.tasks import TaskRegistry

    registry = TaskRegistry(home / "tasks.json")
    return {
        "tasks": registry.all(),
        "chains": registry.chains(),
        "prompt": registry.prompt_block(),
    }


def journal_dump(home: Path):
    from ezagent.agent.journal import Journal

    # El Python no expone las entradas en público; el arnés usa su lector.
    return {"entries": Journal(home / "journal")._load_entries()}


def memory_dump(home: Path, query: str):
    from ezagent.agent.memory import MemoryStore

    store = MemoryStore.from_paths(home / "memory", home / "memory" / "history.db")
    out = {
        "count": store.count_messages(),
        "fts": store.fts_enabled,
        "user": store.read_markdown("user"),
        "memory": store.read_markdown("memory"),
        "recall": store.recall(query),
        "block": store.memory_block(),
    }
    out["backend"] = store.last_backend
    return out


def board_dump(home: Path, swarm: str = ""):
    from ezagent.swarm.store import SwarmStore

    store = SwarmStore(home / "swarm" / "board.db")
    out: dict[str, object] = {
        "live": [asdict(info) for info in store.live()],
        "latest": store.latest(),
    }
    if swarm:
        info = store.get(swarm)
        out["info"] = asdict(info) if info is not None else None
        out["members"] = [asdict(item) for item in store.members(swarm)]
        out["tasks"] = [asdict(item) for item in store.tasks(swarm)]
        out["messages"] = [
            asdict(item) for item in store.history(swarm, since=0, limit=1000)
        ]
        out["leases"] = [asdict(item) for item in store.leases(swarm)]
    return out


def logs_dump(home: Path):
    def summarize(path: Path):
        keys: set[str] = set()
        kinds: dict[str, int] = {}
        lines = bad = 0
        if path.exists():
            for line in path.read_text(encoding="utf-8").splitlines():
                if not line.strip():
                    continue
                lines += 1
                try:
                    data = json.loads(line)
                except ValueError:
                    bad += 1
                    continue
                if not isinstance(data, dict):
                    bad += 1
                    continue
                keys.update(data)
                kind = data.get("kind")
                if isinstance(kind, str):
                    kinds[kind] = kinds.get(kind, 0) + 1
        return {"lines": lines, "bad": bad, "keys": sorted(keys), "kinds": kinds}

    logs = home / "logs"
    return {
        "audit": summarize(logs / "audit.jsonl"),
        "latency": summarize(logs / "latency.jsonl"),
    }


def normalize_board(value):
    def walk(node):
        if isinstance(node, dict):
            out = {}
            for key, item in node.items():
                if key in ("created", "updated", "seen", "ts", "expires") and isinstance(
                    item, (int, float)
                ):
                    out[key] = "<time>"
                elif key in ("swarm", "id") and isinstance(item, str):
                    out[key] = "<swarm>"
                else:
                    out[key] = walk(item)
            return out
        if isinstance(node, list):
            return [walk(item) for item in node]
        return node

    return walk(jsonable(value))


def _first_word(text: str) -> str:
    for word in text.split():
        clean = "".join(char for char in word if char.isalnum())
        if len(clean) >= 4:
            return clean
    return "compat"


def check_settings(workspace: Workspace, report: Report) -> None:
    case = workspace.case("voice-read")
    report.compare("voice.json", "Python→Rust (leer)", voice_dump(case.home), case.probe("dump-voice"))

    case = workspace.case("voice-rust-write")
    case.probe("save-voice", "--set", "ui_scale=1.05")
    py, rs = voice_dump(case.home), case.probe("dump-voice")
    raw = json.loads((case.home / "voice.json").read_text(encoding="utf-8"))
    kept = all(key in raw for key in UNKNOWN_VOICE_KEYS)
    report.compare(
        "voice.json", "Rust→Python (escribir)", py, rs,
        "88 campos; claves desconocidas conservadas" if kept else "claves desconocidas perdidas",
    )
    if not kept:
        report.add("voice.json", "Rust→Python (claves desconocidas)", False)

    case = workspace.case("voice-python-write")
    case.activate()
    from ezagent.voice.config import VoiceConfig

    config = VoiceConfig.load(case.home / "voice.json")
    config.ui_scale = 1.11
    config.save(case.home / "voice.json")
    report.compare("voice.json", "Python→Rust (escribir)", voice_dump(case.home), case.probe("dump-voice"))

    case = workspace.case("llm-read")
    report.compare("llm.json", "Python→Rust (leer)", llm_dump(case.home), case.probe("dump-llm"))

    case = workspace.case("llm-rust-write")
    before = set(case.probe("dump-llm")["catalog"])
    case.probe("save-llm")
    py, rs = llm_dump(case.home), case.probe("dump-llm")
    after = set(py["catalog"])
    report.compare("llm.json", "Rust→Python (escribir)", py, rs, "perfiles existentes conservados")
    report.add("llm.json", "Rust→Python (perfil nuevo)", before <= after and "saved:compat r1" in after)

    case = workspace.case("llm-python-write")
    case.activate()
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
    report.compare("llm.json", "Python→Rust (escribir)", llm_dump(case.home), case.probe("dump-llm"))


def check_agent_files(workspace: Workspace, report: Report) -> None:
    case = workspace.case("agent-read")
    report.compare("permissions.toml", "Python→Rust (leer)", agent_dump(case.home), case.probe("dump-agent"))
    report.compare("system.md", "Python→Rust (leer)", system_dump(case.home), case.probe("dump-system"))

    rust_case = workspace.case("agent-rust-default")
    (rust_case.home / "permissions.toml").unlink()
    rust_case.probe("dump-agent")
    rust_template = (rust_case.home / "permissions.toml").read_bytes()
    python_case = workspace.case("agent-python-default")
    (python_case.home / "permissions.toml").unlink()
    python_case.activate()
    agent_dump(python_case.home)
    report.add(
        "permissions.toml",
        "plantilla por defecto (Rust = Python)",
        rust_template == (python_case.home / "permissions.toml").read_bytes(),
        f"Rust {len(rust_template)} B / Python "
        f"{len((python_case.home / 'permissions.toml').read_bytes())} B",
    )

    rust_case = workspace.case("system-rust-default")
    (rust_case.home / "system.md").unlink()
    rust_case.probe("dump-system")
    rust_system = (rust_case.home / "system.md").read_bytes()
    python_case = workspace.case("system-python-default")
    (python_case.home / "system.md").unlink()
    python_case.activate()
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
        query = _first_word(entries[-1]["text"])
    report.compare(
        "chat.jsonl", "Python→Rust (leer)", chat_dump(case.home, query), case.probe("dump-chat", "--query", query)
    )

    case = workspace.case("chat-rust-write")
    before = len(chat_dump(case.home, "x")["recent"])
    case.probe("save-chat")
    py, rs = chat_dump(case.home, "x"), case.probe("dump-chat", "--query", "x")
    texts = [entry["text"] for items in py["entries"].values() for entry in items]
    report.compare(
        "chat.jsonl", "Rust→Python (escribir)", py, rs,
        f"{before} mensajes previos conservados; respuesta corregida",
    )
    report.add("chat.jsonl", "Rust→Python (reescritura)", CHAT_REPLY in texts and CHAT_MESSAGE in texts)

    case = workspace.case("chat-python-write")
    case.activate()
    from ezagent.voice.chat.store import ChatStore

    store = ChatStore(case.home / "history" / "chat.jsonl")
    first = store.session
    store.append("user", CHAT_MESSAGE, "chat", 1_700_000_000.125)
    second = store.new_session()
    store.append("assistant", "respuesta de compatibilidad F7-03", "voice", 1_700_000_001.5)
    replaced = store.replace_last("assistant", "respuesta de compatibilidad F7-03", CHAT_REPLY)
    resumed = store.resume(first)
    deleted = store.delete_session(second)
    rs = case.probe("dump-chat", "--query", "compatibilidad")
    report.compare("chat.jsonl", "Python→Rust (escribir)", chat_dump(case.home, "compatibilidad"), rs,
                   "append, replace_last, resume y delete_session")
    report.add("chat.jsonl", "Python→Rust (reescritura)", replaced and resumed and deleted)


def check_tasks(workspace: Workspace, report: Report) -> None:
    case = workspace.case("tasks-read")
    report.compare("tasks.json", "Python→Rust (leer)", tasks_dump(case.home), case.probe("dump-tasks"))

    case = workspace.case("tasks-rust-write")
    case.probe("save-tasks")
    py, rs = tasks_dump(case.home), case.probe("dump-tasks")
    report.compare("tasks.json", "Rust→Python (escribir)", py, rs, "tarea y cadena nuevas")

    case = workspace.case("tasks-python-write")
    case.activate()
    from ezagent.voice.tasks import TaskRegistry

    registry = TaskRegistry(case.home / "tasks.json")
    registry.add("compat:1", "bob", "tarea de compatibilidad", created_at=1_700_000_100.0)
    registry.add_chain(
        {"source": "compat:1", "agent": "cy", "task": "etapa de compatibilidad", "created_at": 1_700_000_101.0}
    )
    report.compare("tasks.json", "Python→Rust (escribir)", tasks_dump(case.home), case.probe("dump-tasks"))


def check_journal(workspace: Workspace, report: Report) -> None:
    case = workspace.case("journal-read")
    report.compare("journal", "Python→Rust (leer)", journal_dump(case.home), case.probe("dump-journal"))

    case = workspace.case("journal-rust-write")
    before = len(journal_dump(case.home)["entries"])
    case.probe("save-journal")
    py, rs = journal_dump(case.home), case.probe("dump-journal")
    report.compare("journal", "Rust→Python (escribir)", py, rs, f"{before} entradas previas conservadas")

    case = workspace.case("journal-python-write")
    case.activate()
    from ezagent.agent.journal import Journal

    Journal(case.home / "journal").record(
        "create", "Entrada de Python F7-03", [{"action": "trash", "path": str(case.home / "x.txt")}]
    )
    report.compare("journal", "Python→Rust (escribir)", journal_dump(case.home), case.probe("dump-journal"))


def check_memory(workspace: Workspace, report: Report) -> None:
    case = workspace.case("memory-read")
    report.compare(
        "history.db", "Python→Rust (leer)", memory_dump(case.home, "compat"), case.probe("dump-memory", "--query", "compat")
    )

    case = workspace.case("memory-rust-write")
    case.probe("save-memory")
    py, rs = memory_dump(case.home, "compatibilidad"), case.probe("dump-memory", "--query", "compatibilidad")
    report.compare("history.db", "Rust→Python (escribir)", py, rs, "mensaje y viñeta nuevos")
    connection = sqlite3.connect(case.home / "memory" / "history.db")
    row = connection.execute(
        "SELECT session, role, content FROM messages WHERE ts = '2026-11-01T00:00:00+00:00'"
    ).fetchone()
    connection.close()
    report.add(
        "history.db",
        "Rust→Python (fila)",
        row == ("compat", "user", MEMORY_TEXT) and MEMORY_LINE in py["memory"],
    )

    case = workspace.case("memory-python-write")
    case.activate()
    from datetime import datetime, timezone

    from ezagent.agent.memory import MemoryStore

    store = MemoryStore.from_paths(case.home / "memory", case.home / "memory" / "history.db")
    saved = store.record(
        "user", MEMORY_TEXT, session="compat", ts=datetime(2026, 11, 1, tzinfo=timezone.utc)
    )
    remembered = store.remember("memory", MEMORY_LINE)
    report.compare(
        "history.db", "Python→Rust (escribir)", memory_dump(case.home, "compatibilidad"), case.probe("dump-memory", "--query", "compatibilidad"),
        "mensaje y viñeta nuevos",
    )
    report.add("history.db", "Python→Rust (fila)", saved and remembered.saved)


def check_board(workspace: Workspace, report: Report) -> None:
    case = workspace.case("board-read")
    report.compare("board.db", "Python→Rust (leer)", board_dump(case.home), case.probe("dump-board"))

    case = workspace.case("board-rust-write")
    saved = case.probe("save-board")
    swarm = saved["swarm"]
    report.compare(
        "board.db", "Rust→Python (escribir)", normalize_board(board_dump(case.home, swarm)),
        normalize_board(case.probe("dump-board", "--swarm", swarm)), "enjambre, miembro, tarea y mensaje",
    )

    case = workspace.case("board-python-write")
    case.activate()
    from ezagent.swarm.service import SwarmService
    from ezagent.swarm.store import SwarmStore

    service = SwarmService(SwarmStore(case.home / "swarm" / "board.db"))
    swarm, _seats = service.start(
        "Compatibilidad F7-03",
        [("ana", "dev", "opencode"), ("bo", "qa", "codex")],
        tasks=[
            ("Primera tarea de compatibilidad", "dev"),
            ("Segunda tarea de compatibilidad", "dev"),
        ],
    )
    service.store.post(swarm, "ana", "mensaje de compatibilidad", kind="info")
    report.compare(
        "board.db", "Python→Rust (escribir)", normalize_board(board_dump(case.home, swarm)),
        normalize_board(case.probe("dump-board", "--swarm", swarm)), "enjambre, miembros, tareas y mensaje",
    )


def check_logs(workspace: Workspace, report: Report) -> None:
    case = workspace.case("logs-read")
    report.compare("audit/latency", "Python→Rust (leer)", logs_dump(case.home), case.probe("dump-logs"))

    case = workspace.case("logs-rust-write")
    case.probe("append-logs")
    py, rs = logs_dump(case.home), case.probe("dump-logs")
    report.compare("audit.jsonl", "Rust→Python (añadir)", py["audit"], rs["audit"], "una línea nueva")
    report.compare("latency.jsonl", "Rust→Python (añadir)", py["latency"], rs["latency"], "una línea nueva")
    last = json.loads((case.home / "logs" / "audit.jsonl").read_text(encoding="utf-8").splitlines()[-1])
    expected = {"turn_id": "compat-r1", "tool": "compat", "level": 1, "decision": "allow",
                "taint": False, "result_ok": True, "ms": 12.3}
    report.add(
        "audit.jsonl", "Rust→Python (contenido)",
        all(last.get(key) == value for key, value in expected.items())
        and last.get("args") == {"command": "compat"},
    )
    latency = json.loads(
        (case.home / "logs" / "latency.jsonl").read_text(encoding="utf-8").splitlines()[-1]
    )
    report.add(
        "latency.jsonl", "Rust→Python (contenido)",
        latency.get("kind") == "command" and latency.get("name") == "compat"
        and latency.get("handled_ms") == 12.3,
    )

    case = workspace.case("logs-python-write")
    case.activate()
    from ezagent.agent.audit import AuditLog
    from ezagent.voice.metrics import LatencyStats

    AuditLog(case.home / "logs" / "audit.jsonl").log(
        turn_id="compat-py", tool="compat", level=1, args={"command": "compat"},
        decision="allow", taint=False, result_ok=True, ms=12.34,
    )
    LatencyStats(jsonl_path=case.home / "logs" / "latency.jsonl").record_command("compat", 12.34)
    report.compare("audit/latency", "Python→Rust (añadir)", logs_dump(case.home), case.probe("dump-logs"))


def check_opencode(workspace: Workspace, report: Report) -> None:
    reference = "opencode-go/modelo-de-compatibilidad"
    rust_case = workspace.case("opencode-rust")
    rust_case.probe("write-opencode", "--ref", reference, "--effort", "high")
    rust_file = rust_case.local / "EzAgent" / "opencode-state" / "opencode" / "model.json"
    rust_bytes = rust_file.read_bytes()

    python_case = workspace.case("opencode-python")
    python_case.local.mkdir(parents=True, exist_ok=True)
    python_case.activate()
    from ezagent.voice.agent_prelaunch import prepare_opencode_state

    env = prepare_opencode_state(reference, "high")
    python_file = python_case.local / "EzAgent" / "opencode-state" / "opencode" / "model.json"
    report.add(
        "opencode-state/model.json", "Rust = Python (bytes)", rust_bytes == python_file.read_bytes(),
        "mismo JSON y mismas variables",
    )
    rust_env = rust_case.probe("write-opencode", "--ref", reference, "--effort", "high")["env"]

    def normalize(value: str, case) -> str:
        return value.replace(str(case.local), "<local>")

    expected_env = str(python_case.local / "EzAgent" / "opencode-state")
    report.add(
        "opencode-state/model.json", "Rust = Python (entorno)",
        normalize(rust_env.get("XDG_STATE_HOME", ""), rust_case) == normalize(expected_env, python_case),
    )


def checks(workspace: Workspace, report: Report) -> None:
    for check in (
        check_settings,
        check_agent_files,
        check_chat,
        check_tasks,
        check_journal,
        check_memory,
        check_board,
        check_logs,
        check_opencode,
    ):
        check(workspace, report)
