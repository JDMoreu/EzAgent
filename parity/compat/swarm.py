"""Enjambre mixto: `ezs` de Python ↔ servidor Rust y al revés (F7-03).

Las cuatro combinaciones (cliente × servidor) corren la misma secuencia de
comandos sobre un tablero sembrado igual y se comparan entre sí.
"""

from __future__ import annotations

import json
import re
import sqlite3
import subprocess
import sys
import time
from pathlib import Path

from support import EZS, PROBE, ROOT, HarnessError, Report, Workspace, read_json

SCRIPT = Path(__file__).resolve().with_name("py_swarm_server.py")
START_TIMEOUT = 30.0
COMMANDS = [
    ["board"],
    ["next"],
    ["say", "*", "mensaje de prueba"],
    ["inbox"],
    ["next"],
    ["lock", "compat/file.txt", "--min", "2"],
    ["board"],
    ["unlock", "compat/file.txt"],
    ["done", "resultado de compatibilidad"],
    ["board"],
    ["me"],
    ["help"],
    ["noexiste"],
]
TIME_KEYS = ("created", "updated", "seen", "ts", "expires")


def start_server(kind: str, case) -> tuple[subprocess.Popen, dict, Path]:
    base = case.root / "swarm"
    out = case.root / "server.json"
    if kind == "rust":
        args = [str(PROBE), "serve", "--base", str(base), "--out", str(out)]
    else:
        args = [sys.executable, str(SCRIPT), "--base", str(base), "--out", str(out)]
    process = subprocess.Popen(
        args,
        cwd=ROOT,
        env=case.env(),
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    deadline = time.monotonic() + START_TIMEOUT
    while not out.exists() and process.poll() is None and time.monotonic() < deadline:
        time.sleep(0.05)
    if not out.exists():
        stderr = process.stderr.read()[-600:] if process.poll() is not None else ""
        stop_server(process)
        raise HarnessError(f"el servidor {kind} no publicó endpoint.json: {stderr}")
    info = read_json(out)
    if "error" in info:
        stop_server(process)
        raise HarnessError(f"el servidor {kind} falló: {info['error']}")
    return process, info, base


def stop_server(process: subprocess.Popen) -> None:
    try:
        if process.stdin is not None:
            process.stdin.close()
        process.wait(timeout=20)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)
    except OSError:
        pass


def run_client(kind: str, case, base: Path, info: dict) -> list[dict]:
    env = case.env()
    env["EZS_ENDPOINT"] = str(base / "endpoint.json")
    env["EZS_TOKEN"] = info["token"]
    args = [str(EZS)] if kind == "rust" else [sys.executable, "-m", "ezagent.swarm.cli"]
    transcript = []
    for argv in COMMANDS:
        result = subprocess.run(
            args + argv,
            cwd=ROOT,
            env=env,
            capture_output=True,
            text=True,
            timeout=120,
        )
        transcript.append(
            {
                "argv": argv,
                "code": result.returncode,
                "out": result.stdout,
                "err": result.stderr,
            }
        )
    return transcript


def normalize(transcript: list[dict], info: dict, case, base: Path) -> list[dict]:
    text = json.dumps(transcript, ensure_ascii=False)
    text = text.replace(info["token"], "<token>").replace(info["swarm"], "<swarm>")
    text = text.replace(str(base), "<base>").replace(str(case.root), "<root>")
    text = re.sub(r"http://127\.0\.0\.1:\d+", "<url>", text)
    text = re.sub(r"\bhace \d+[smhd]\b", "hace <edad>", text)
    return json.loads(text)


def dump_board(base: Path) -> dict:
    connection = sqlite3.connect(base / "board.db")
    connection.row_factory = sqlite3.Row
    tables = ("swarm", "member", "task", "message", "cursor", "lease", "incident")
    out = {}
    try:
        for table in tables:
            rows = []
            for row in connection.execute(f"select * from {table} order by rowid"):
                item = dict(row)
                item.pop("token", None)
                for key in TIME_KEYS:
                    if key in item and isinstance(item[key], (int, float)):
                        item[key] = "<time>"
                if "swarm" in item:
                    item["swarm"] = "<swarm>"
                if table == "swarm" and "id" in item:
                    item["id"] = "<swarm>"
                rows.append(item)
            out[table] = rows
    finally:
        connection.close()
    return out


def checks(workspace: Workspace, report: Report) -> None:
    transcripts: dict[tuple[str, str], list[dict]] = {}
    boards: dict[tuple[str, str], dict] = {}
    endpoints: dict[str, dict] = {}
    for server in ("python", "rust"):
        for client in ("python", "rust"):
            case = workspace.case(f"swarm-{server}-{client}")
            process, info, base = start_server(server, case)
            try:
                transcript = run_client(client, case, base, info)
                endpoints.setdefault(server, read_json(base / "endpoint.json"))
            finally:
                stop_server(process)
            transcripts[(server, client)] = normalize(transcript, info, case, base)
            boards[(server, client)] = dump_board(base)
            codes = [item["code"] for item in transcripts[(server, client)]]
            report.add(
                "swarm",
                f"ezs {client} → servidor {server}",
                codes == [0] * (len(COMMANDS) - 1) + [1],
                f"{len(COMMANDS)} comandos; códigos {codes}",
            )

    baseline = transcripts[("python", "python")]
    for key, transcript in transcripts.items():
        if key == ("python", "python"):
            continue
        report.compare(
            "swarm",
            f"ezs {key[1]} → servidor {key[0]} (respuestas)",
            baseline,
            transcript,
            "mismas salidas que Python ↔ Python",
        )

    baseline_board = boards[("python", "python")]
    for key, board in boards.items():
        if key == ("python", "python"):
            continue
        report.compare(
            "board.db",
            f"servidor {key[0]} · cliente {key[1]} (estado final)",
            baseline_board,
            board,
            "mismo tablero normalizado",
        )

    for server, endpoint in endpoints.items():
        report.add(
            "endpoint.json",
            f"servidor {server}",
            set(endpoint) == {"url", "pid"}
            and str(endpoint["url"]).startswith("http://127.0.0.1:")
            and isinstance(endpoint["pid"], int),
            "url y pid como el Python",
        )
