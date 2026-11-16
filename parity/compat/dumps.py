"""Lecturas de Python de cada artefacto del perfil (F7-03).

Los datos se quedan en memoria: el arnés nunca imprime su contenido.
"""

from __future__ import annotations

import json
import sys
from dataclasses import asdict
from pathlib import Path

from support import SRC, jsonable

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
        out["messages"] = [asdict(item) for item in store.history(swarm, since=0, limit=1000)]
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


def first_word(text: str) -> str:
    for word in text.split():
        clean = "".join(char for char in word if char.isalnum())
        if len(clean) >= 4:
            return clean
    return "compat"
