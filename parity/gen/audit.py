"""Fixtures del registro de auditoría (dominio `audit`, F1-04).

Fija la línea JSONL exacta de cada llamada, la redacción de contenidos y
secretos, y la rotación por tamaño. El reloj se congela a 2025-01-01T00:00:00Z
para que `ts` sea determinista.
"""

from __future__ import annotations

import json
import os
import pathlib
from datetime import datetime, timezone
from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "audit"

_FIXED = datetime(2025, 1, 1, 0, 0, 0, tzinfo=timezone.utc)
_TRAPS = ("SUPERSECRETO", "SUPERSECRETO2", "SUPERSECRETO3", "hunter2", "abc.DEF-123", "sk-proj")


class _FrozenDatetime(datetime):
    @classmethod
    def now(cls, tz: Any = None) -> datetime:
        if tz is None:
            return _FIXED.replace(tzinfo=None)
        return _FIXED.astimezone(tz)


REDACT_CASES: list[dict[str, Any]] = [
    {
        "id": "content-keys",
        "args": {
            "content": "hola mundo",
            "text": "texto libre",
            "data": [1, 2],
            "output": {"a": 1},
            "path": "a.txt",
        },
    },
    {"id": "content-upper", "args": {"CONTENT": "hola", "Output": "ñandú"}},
    {"id": "content-none", "args": {"content": None, "text": ""}},
    {"id": "content-bytes", "args": {"data": {"$bytes": "abc"}}},
    {
        "id": "secret-keys",
        "args": {
            "api_key": "SUPERSECRETO",
            "API_KEY": "SUPERSECRETO",
            "apikey": "SUPERSECRETO",
            "token": "SUPERSECRETO",
            "access-token": "SUPERSECRETO",
            "password": "SUPERSECRETO",
            "db_passwd": "SUPERSECRETO",
            "client_secret": "SUPERSECRETO",
            "tokens": "SUPERSECRETO",
            "api-key": "SUPERSECRETO",
            "monkey": "no es secreto",
        },
    },
    {
        "id": "nested",
        "args": {"a": {"b": [{"password": "SUPERSECRETO", "content": "yyyy"}]}},
    },
    {
        "id": "free-text",
        "args": {
            "cmd": "curl -H 'Authorization: Bearer abc.DEF-123' https://x",
            "nota": "usa sk-proj_ABCdef123 para entrar",
            "lista": ["sk-live-999", "normal"],
            "conn": "postgres://usuario:hunter2@localhost/db",
            "kv": "password=hunter2; token=SUPERSECRETO",
        },
    },
    {
        "id": "not-text",
        "args": {"n": 3, "ok": True, "x": None, "f": 1.5},
    },
]

LOG_CASES: list[dict[str, Any]] = [
    {"id": "basic", "tool": "write_file", "args": {"path": "a.txt"}, "ms": 1.234},
    {"id": "rounding-up", "tool": "read_file", "args": {}, "ms": 1.25},
    {"id": "rounding-55", "tool": "read_file", "args": {}, "ms": 2.55},
    {"id": "rounding-zero", "tool": "read_file", "args": {}, "ms": 0.049},
    {"id": "rounding-negative", "tool": "read_file", "args": {}, "ms": -0.04},
    {
        "id": "traps",
        "tool": "delete_path",
        "args": {
            "api_key": "SUPERSECRETO",
            "note": "Bearer SUPERSECRETO2",
            "content": "SUPERSECRETO3",
            "path": "C:/tmp/x.txt",
        },
        "level": 2,
        "decision": "confirm_yes",
        "taint": True,
        "result_ok": False,
        "ms": 12.0,
    },
    {
        "id": "error-kind-redacted",
        "tool": "web_fetch",
        "args": {},
        "error_kind": "token=SUPERSECRETO",
        "ms": 3.0,
    },
    {
        "id": "coercions",
        "tool": "write_file",
        "args": {"n": 1, "ok": True},
        "level": 2.9,
        "taint": 1,
        "result_ok": 0,
        "ms": 0.0,
    },
    {
        "id": "unicode",
        "tool": "write_file",
        "args": {"p": "ñ" * 50, "emoji": "🙂"},
        "turn_id": "turno-ñ-1",
        "ms": 7.5,
    },
]


def _expand(value: Any) -> Any:
    if isinstance(value, dict):
        if set(value) == {"$bytes"}:
            return str(value["$bytes"]).encode("utf-8")
        return {key: _expand(item) for key, item in value.items()}
    if isinstance(value, list):
        return [_expand(item) for item in value]
    return value


def _entry(case: dict[str, Any]) -> dict[str, Any]:
    return {
        "turn_id": case.get("turn_id", "t1"),
        "tool": case.get("tool", "write_file"),
        "level": case.get("level", 1),
        "args": _expand(case.get("args", {})),
        "decision": case.get("decision", "allow"),
        "taint": case.get("taint", False),
        "result_ok": case.get("result_ok", True),
        "ms": case.get("ms", 1.0),
        "error_kind": case.get("error_kind"),
    }


def _leaks(text: str) -> list[str]:
    return [needle for needle in _TRAPS if needle in text]


def _log_cases(out: FixtureFile, AuditLog: Any, root: pathlib.Path) -> None:
    for case in LOG_CASES:
        path = root / "logs" / f"{case['id']}.jsonl"
        entry = _entry(case)
        AuditLog(path).log(**entry)
        lines = path.read_text(encoding="utf-8").splitlines()
        leaks = _leaks("\n".join(lines))
        if leaks:  # pragma: no cover - un secreto en el archivo rompe el oráculo
            raise AssertionError(f"auditoría filtró secretos en {case['id']}: {leaks}")
        payload = {
            "turn_id": entry["turn_id"],
            "tool": entry["tool"],
            "level": entry["level"],
            "args": case.get("args", {}),
            "decision": entry["decision"],
            "taint": entry["taint"],
            "result_ok": entry["result_ok"],
            "ms": entry["ms"],
            "error_kind": entry["error_kind"],
        }
        out.add(
            f"audit/log/{case['id']}",
            "log",
            payload,
            {
                "line": lines[-1] if lines else "",
                "lines": len(lines),
                "created_dirs": path.parent.is_dir(),
                "leaks": leaks,
            },
        )


def _rotation_case(out: FixtureFile, AuditLog: Any, root: pathlib.Path) -> None:
    folder = root / "rot"
    log = AuditLog(folder / "audit.jsonl", max_bytes=600, backups=2)
    for index in range(40):
        log.log(**_entry({"turn_id": f"turno-{index:03d}", "args": {"p": "ñ" * 50}}))
    files = {path.name: len(path.read_text(encoding="utf-8").splitlines()) for path in sorted(folder.iterdir())}
    live = folder / "audit.jsonl"
    last = json.loads(live.read_text(encoding="utf-8").splitlines()[-1])
    everything = [line for path in sorted(folder.iterdir()) for line in path.read_text("utf-8").splitlines()]
    out.add(
        "audit/rotation/keeps-two-backups",
        "log",
        {"entries": 40, "max_bytes": 600, "backups": 2, "args": {"p": "ñ" * 50}},
        {
            "files": files,
            "last_turn": last["turn_id"],
            "has_oldest": any("turno-000" in line for line in everything),
            "has_newest": any("turno-039" in line for line in everything),
        },
    )

    folder = root / "no-rot"
    log = AuditLog(folder / "audit.jsonl", max_bytes=10, backups=0)
    for _ in range(5):
        log.log(**_entry({}))
    files = {path.name: len(path.read_text(encoding="utf-8").splitlines()) for path in sorted(folder.iterdir())}
    out.add(
        "audit/rotation/disabled",
        "log",
        {"entries": 5, "max_bytes": 10, "backups": 0},
        {"files": files},
    )


def generate(out: FixtureFile) -> None:
    from ezagent.agent import audit as audit_module

    original = audit_module.datetime
    audit_module.datetime = _FrozenDatetime
    root = pathlib.Path(os.environ["HOME"]) / "audit-cases"
    try:
        log = audit_module.AuditLog(root / "redact.jsonl")
        for case in REDACT_CASES:
            args = _expand(case["args"])
            redacted = log.redact(args)
            text = json.dumps(redacted, ensure_ascii=False, default=str)
            leaks = _leaks(text)
            if leaks:  # pragma: no cover
                raise AssertionError(f"redact filtró secretos en {case['id']}: {leaks}")
            out.add(
                f"audit/redact/{case['id']}",
                "redact",
                {"args": case["args"]},
                {"redacted": redacted, "leaks": leaks},
            )
        _log_cases(out, audit_module.AuditLog, root)
        _rotation_case(out, audit_module.AuditLog, root)
        out.add(
            "audit/constants",
            "module_constants",
            {},
            {
                "content_keys": sorted(audit_module._CONTENT_KEYS),
                "secret_key_parts": list(audit_module._SECRET_KEY_PARTS),
            },
        )
    finally:
        audit_module.datetime = original


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
