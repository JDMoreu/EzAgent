"""Comprobaciones de memoria, tablero, logs y estado de OpenCode (F7-03)."""

from __future__ import annotations

import json
import sqlite3

from dumps import MEMORY_LINE, MEMORY_TEXT, board_dump, logs_dump, memory_dump, normalize_board
from support import Report, Workspace


def check_memory(workspace: Workspace, report: Report) -> None:
    case = workspace.case("memory-read")
    report.compare(
        "history.db",
        "Python→Rust (leer)",
        memory_dump(case.home, "compat"),
        case.probe("dump-memory", "--query", "compat"),
    )

    case = workspace.case("memory-rust-write")
    case.probe("save-memory")
    py, rs = (
        memory_dump(case.home, "compatibilidad"),
        case.probe("dump-memory", "--query", "compatibilidad"),
    )
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
    from datetime import datetime, timezone

    from ezagent.agent.memory import MemoryStore

    store = MemoryStore.from_paths(case.home / "memory", case.home / "memory" / "history.db")
    saved = store.record(
        "user", MEMORY_TEXT, session="compat", ts=datetime(2026, 11, 1, tzinfo=timezone.utc)
    )
    remembered = store.remember("memory", MEMORY_LINE)
    report.compare(
        "history.db",
        "Python→Rust (escribir)",
        memory_dump(case.home, "compatibilidad"),
        case.probe("dump-memory", "--query", "compatibilidad"),
        "mensaje y viñeta nuevos",
    )
    report.add("history.db", "Python→Rust (fila)", saved and remembered.saved)


def check_board(workspace: Workspace, report: Report) -> None:
    case = workspace.case("board-read")
    report.compare(
        "board.db", "Python→Rust (leer)", board_dump(case.home), case.probe("dump-board")
    )

    case = workspace.case("board-rust-write")
    saved = case.probe("save-board")
    swarm = saved["swarm"]
    report.compare(
        "board.db",
        "Rust→Python (escribir)",
        normalize_board(board_dump(case.home, swarm)),
        normalize_board(case.probe("dump-board", "--swarm", swarm)),
        "enjambre, miembro, tarea y mensaje",
    )

    case = workspace.case("board-python-write")
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
        "board.db",
        "Python→Rust (escribir)",
        normalize_board(board_dump(case.home, swarm)),
        normalize_board(case.probe("dump-board", "--swarm", swarm)),
        "enjambre, miembros, tareas y mensaje",
    )


def _check_rust_log_lines(case, report: Report) -> None:
    last = json.loads(
        (case.home / "logs" / "audit.jsonl").read_text(encoding="utf-8").splitlines()[-1]
    )
    expected = {
        "turn_id": "compat-r1",
        "tool": "compat",
        "level": 1,
        "decision": "allow",
        "taint": False,
        "result_ok": True,
        "ms": 12.3,
    }
    report.add(
        "audit.jsonl",
        "Rust→Python (contenido)",
        all(last.get(key) == value for key, value in expected.items())
        and last.get("args") == {"command": "compat"},
    )
    latency = json.loads(
        (case.home / "logs" / "latency.jsonl").read_text(encoding="utf-8").splitlines()[-1]
    )
    report.add(
        "latency.jsonl",
        "Rust→Python (contenido)",
        latency.get("kind") == "command"
        and latency.get("name") == "compat"
        and latency.get("handled_ms") == 12.3,
    )


def check_logs(workspace: Workspace, report: Report) -> None:
    case = workspace.case("logs-read")
    report.compare(
        "audit/latency", "Python→Rust (leer)", logs_dump(case.home), case.probe("dump-logs")
    )

    case = workspace.case("logs-rust-write")
    case.probe("append-logs")
    py, rs = logs_dump(case.home), case.probe("dump-logs")
    report.compare(
        "audit.jsonl", "Rust→Python (añadir)", py["audit"], rs["audit"], "una línea nueva"
    )
    report.compare(
        "latency.jsonl", "Rust→Python (añadir)", py["latency"], rs["latency"], "una línea nueva"
    )
    _check_rust_log_lines(case, report)

    case = workspace.case("logs-python-write")
    from ezagent.agent.audit import AuditLog
    from ezagent.voice.metrics import LatencyStats

    AuditLog(case.home / "logs" / "audit.jsonl").log(
        turn_id="compat-py",
        tool="compat",
        level=1,
        args={"command": "compat"},
        decision="allow",
        taint=False,
        result_ok=True,
        ms=12.34,
    )
    LatencyStats(jsonl_path=case.home / "logs" / "latency.jsonl").record_command("compat", 12.34)
    report.compare(
        "audit/latency", "Python→Rust (añadir)", logs_dump(case.home), case.probe("dump-logs")
    )


def check_opencode(workspace: Workspace, report: Report) -> None:
    reference = "opencode-go/modelo-de-compatibilidad"
    rust_case = workspace.case("opencode-rust")
    rust_case.probe("write-opencode", "--ref", reference, "--effort", "high")
    rust_file = rust_case.local / "EzAgent" / "opencode-state" / "opencode" / "model.json"
    rust_bytes = rust_file.read_bytes()

    python_case = workspace.case("opencode-python")
    python_case.local.mkdir(parents=True, exist_ok=True)
    from ezagent.voice.agent_prelaunch import prepare_opencode_state

    env = prepare_opencode_state(reference, "high")
    python_file = python_case.local / "EzAgent" / "opencode-state" / "opencode" / "model.json"
    report.add(
        "opencode-state/model.json",
        "Rust = Python (bytes)",
        rust_bytes == python_file.read_bytes(),
        "mismo JSON y mismas variables",
    )
    rust_env = rust_case.probe("write-opencode", "--ref", reference, "--effort", "high")["env"]

    def normalize(value: str, case) -> str:
        return value.replace(str(case.local), "<local>")

    expected_env = str(python_case.local / "EzAgent" / "opencode-state")
    report.add(
        "opencode-state/model.json",
        "Rust = Python (entorno)",
        env.get("XDG_STATE_HOME") == expected_env
        and normalize(rust_env.get("XDG_STATE_HOME", ""), rust_case)
        == normalize(expected_env, python_case),
    )


def checks(workspace: Workspace, report: Report) -> None:
    for check in (check_memory, check_board, check_logs, check_opencode):
        check(workspace, report)
