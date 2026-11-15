"""Fixtures del adaptador de Herdr (dominio `herdr`, F1-15).

No hay `hc` real: `subprocess.run` y `shutil.which` se sustituyen por dobles
deterministas que registran el argv exacto y devuelven salidas de ejemplo
tomadas de las capturas de `tests/fixtures/screens`. El reloj monotónico y
`uuid4` se congelan para que los hashes de contexto y las rutas de traspaso
sean estables. Las rutas se escriben con el marcador `<tmp>`.
"""

from __future__ import annotations

import contextlib
import json
import os
import shutil
import subprocess
import uuid
from collections.abc import Callable, Iterator
from pathlib import Path
from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "herdr"
ROOT = Path(__file__).resolve().parents[2]
SCREEN_DIR = ROOT / "tests" / "fixtures" / "screens"


def _screen(name: str) -> str:
    return (SCREEN_DIR / f"{name}.txt").read_text(encoding="utf-8")


def _norm(value: str, home: Path) -> str:
    text = value
    for prefix in sorted({str(home), str(home.resolve())}, key=len, reverse=True):
        text = text.replace(prefix, "<tmp>")
    return text


def _mark(value: Any, home: Path) -> Any:
    if isinstance(value, str):
        return _norm(value, home)
    if isinstance(value, list):
        return [_mark(item, home) for item in value]
    if isinstance(value, tuple):
        return [_mark(item, home) for item in value]
    if isinstance(value, dict):
        return {key: _mark(item, home) for key, item in value.items()}
    return value


@contextlib.contextmanager
def _env(values: dict[str, str | None]) -> Iterator[None]:
    saved = {name: os.environ.get(name) for name in values}
    try:
        for name, value in values.items():
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value
        yield
    finally:
        for name, value in saved.items():
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value


@contextlib.contextmanager
def _which(mapping: dict[str, str | None]) -> Iterator[dict[str, str | None]]:
    original = shutil.which

    def fake(name: str) -> str | None:
        return mapping.get(str(name))

    shutil.which = fake  # type: ignore[assignment]
    try:
        yield mapping
    finally:
        shutil.which = original  # type: ignore[assignment]


def _reply(
    rc: int = 0, stdout: str = "", stderr: str = "", exc: str | None = None
) -> dict[str, Any]:
    return {"rc": rc, "stdout": stdout, "stderr": stderr, "exc": exc}


@contextlib.contextmanager
def _fake_run(replies: list[dict[str, Any]]) -> Iterator[list[dict[str, Any]]]:
    calls: list[dict[str, Any]] = []
    original = subprocess.run
    state = {"index": 0}

    def fake(command: list[str], **kwargs: Any) -> subprocess.CompletedProcess[str]:
        reply = replies[min(state["index"], len(replies) - 1)] if replies else _reply()
        state["index"] += 1
        env = kwargs.get("env")
        calls.append(
            {
                "argv": [str(part) for part in command],
                "timeout": kwargs.get("timeout"),
                "encoding": kwargs.get("encoding"),
                "errors": kwargs.get("errors"),
                "capture_output": kwargs.get("capture_output"),
                "text": kwargs.get("text"),
                "env_none": env is None,
                "env_has_hc_terminal_id": any(
                    str(key).upper() == "HC_TERMINAL_ID" for key in (env or {})
                ),
                "no_window": bool(kwargs.get("creationflags")),
                "shell": kwargs.get("shell"),
            }
        )
        if reply["exc"] == "timeout":
            raise subprocess.TimeoutExpired(command, kwargs.get("timeout") or 0)
        if reply["exc"] == "filenotfound":
            raise FileNotFoundError(2, "no such file", command[0])
        if reply["exc"] == "oserror":
            raise OSError("fallo del sistema")
        return subprocess.CompletedProcess(
            command, reply["rc"], stdout=reply["stdout"], stderr=reply["stderr"]
        )

    subprocess.run = fake  # type: ignore[assignment]
    try:
        yield calls
    finally:
        subprocess.run = original  # type: ignore[assignment]


class _Clock:
    def __init__(self, now: float = 1000.0) -> None:
        self.now = now

    def monotonic(self) -> float:
        return self.now

    def time(self) -> float:
        return self.now


@contextlib.contextmanager
def _clock(clock: _Clock) -> Iterator[_Clock]:
    from ezagent.voice import herdr as module

    original = module.time
    module.time = clock  # type: ignore[assignment]
    try:
        yield clock
    finally:
        module.time = original  # type: ignore[assignment]


class _Uuid:
    def __init__(self, hex_value: str) -> None:
        self.hex = hex_value


@contextlib.contextmanager
def _uuids() -> Iterator[Callable[[], uuid.UUID | _Uuid]]:
    original = uuid.uuid4
    values = ["a1b2c3d4e5f60718", "b2c3d4e5f6071829", "c3d4e5f60718293a", "d4e5f60718293a4b"]
    state = {"index": 0}

    def fake() -> _Uuid:
        value = _Uuid(values[min(state["index"], len(values) - 1)])
        state["index"] += 1
        return value

    uuid.uuid4 = fake  # type: ignore[assignment]
    try:
        yield fake
    finally:
        uuid.uuid4 = original  # type: ignore[assignment]


def _config(home: Path, **overrides: Any) -> Any:
    from ezagent.voice.config import VoiceConfig

    values: dict[str, Any] = {
        "hc_path": "hc",
        "herdr_app_path": "herdr-app",
        "default_cwd": str(home / "ws"),
        "max_read_lines": 40,
    }
    values.update(overrides)
    return VoiceConfig(**values)


def _herdr(cfg: Any) -> Any:
    from ezagent.voice.herdr import Herdr

    original = Herdr._resolve_paths
    Herdr._resolve_paths = lambda self: None  # type: ignore[assignment]
    try:
        return Herdr(cfg)
    finally:
        Herdr._resolve_paths = original  # type: ignore[assignment]


def _capture(
    out: FixtureFile,
    case: str,
    fn_name: str,
    input_data: Any,
    action: Callable[[], Any],
    home: Path,
) -> None:
    try:
        value = action()
    except Exception as exc:  # noqa: BLE001 - el oráculo registra el texto real
        out.add_error(case, fn_name, input_data, _norm(str(exc), home))
        return
    out.add(case, fn_name, input_data, _mark(value, home))


# --------------------------------------------------------------------- texto
STRIP_CASES: list[tuple[str, str]] = [
    ("plain", "hola\nmundo"),
    ("ansi", "\x1b[31mrojo\x1b[0m y \x1b[1;32mverde\x1b[0m"),
    ("cursor", "uno\x1b[2A\ndos\x1b[K"),
    ("osc-bel", "\x1b]0;título\x07texto"),
    ("osc-st", "\x1b]8;;https://x\x1b\\enlace\x1b]8;;\x1b\\"),
    ("crlf", "uno\r\ndos\r\n"),
    ("cr", "uno\rdos\r"),
    ("mixed", "uno\r\ndos\rtres"),
    ("box", "┌──────┐\n│ caja │\n└──────┘"),
    ("box-heavy", "╔═══╗\n║ x ║\n╚═══╝"),
    ("spaces", "  uno    dos\t\ttres  "),
    ("blank-lines", "\n\nuno\n\n\ndos\n\n"),
    ("empty", ""),
    ("spaces-only", "   \n\t\n  "),
    ("ansi-only", "\x1b[31m\x1b[0m"),
    ("unicode", "ñandú 🙂 y tildes: áéíóú"),
    ("claude", _screen("claude")),
    ("codex", _screen("codex")),
    ("opencode", _screen("opencode")),
    ("opencode-long", _screen("opencode_long")),
    ("opencode-context", _screen("opencode_context")),
    ("mcode", _screen("mcode")),
    ("control", "\x00\x01\x02 texto \x0b\x0c"),
]


def _strip_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.voice.herdr import strip_ansi

    for index, (name, text) in enumerate(STRIP_CASES, start=1):
        _capture(
            out,
            f"herdr/strip/{index:03d}-{name}",
            "strip_ansi",
            {"text": text},
            lambda item=text: {"text": strip_ansi(item)},
            home,
        )


VISIBLE_CASES: list[dict[str, Any]] = [
    {"where": "EzAgent / Claude Code"},
    {"where": "EzAgent / 2"},
    {"where": "  EzAgent   /   Claude   Code  "},
    {"where": "EzAgent / OpenCode / extra"},
    {"where": "pestaña suelta"},
    {"name": "planificador", "space": "EzAgent"},
    {"name": "  2  ", "space": "EzAgent"},
    {"name": "planificador"},
    {"space": "EzAgent"},
    {"agent": "claude-code", "cwd": r"C:\Users\x\EzAgent"},
    {"agent": "CLAUDE", "cwd": "C:/x/EzAgent"},
    {"agent": "opencode", "cwd": "/home/u/Proyecto"},
    {"agent": "codex", "cwd": ""},
    {"agent": "gemini", "cwd": "/tmp"},
    {"agent": "desconocido", "cwd": "/tmp/Proyecto/"},
    {"agent": "  codex  ", "cwd": "  C:/tmp/Proyecto  "},
    {"address": "t51"},
    {},
    {"where": "", "name": "", "space": "", "agent": "", "cwd": ""},
    {"where": " / "},
    {"where": 5, "name": 7},
    {"agent": "claude-code", "cwd": r"C:\Users\x\EzAgent\ "},
    {"name": "2", "space": "EzAgent", "where": "Otro / Tab"},
]


def _visible_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.voice.herdr import visible_name

    for index, item in enumerate(VISIBLE_CASES, start=1):
        _capture(
            out,
            f"herdr/visible_name/{index:03d}",
            "visible_name",
            item,
            lambda value=item: {"name": visible_name(value)},
            home,
        )


# ------------------------------------------------------------------ JSON y CLI
def _json_cases(out: FixtureFile, home: Path) -> None:
    cases: list[tuple[str, list[str], float, list[dict[str, Any]]]] = [
        ("object", ["get", "t1"], 5.0, [_reply(stdout='{"ok": true, "n": 1}')]),
        ("list", ["ls", "--json"], 20.0, [_reply(stdout="[1, 2, 3]")]),
        ("scalar", ["get", "t1"], 5.0, [_reply(stdout='"texto"')]),
        ("spaces", ["get", "t1"], 5.0, [_reply(stdout='  {"a": 1}  \n')]),
        ("empty-stdout", ["get", "t1"], 5.0, [_reply(stdout="  ", stderr="")]),
        (
            "empty-with-stderr",
            ["get", "t1"],
            5.0,
            [_reply(stdout="", stderr="  fallo de hc  \n")],
        ),
        ("not-json", ["get", "t1"], 5.0, [_reply(stdout="<html>no</html>")]),
        ("not-json-truncated", ["get", "t1"], 5.0, [_reply(stdout="x" * 400)]),
        (
            "not-found",
            ["get", "t1"],
            5.0,
            [_reply(stdout='{"error": {"kind": "not_found", "message": "no existe t1"}}')],
        ),
        (
            "error-dict",
            ["get", "t1"],
            5.0,
            [_reply(stdout='{"error": {"kind": "boom", "message": "algo falló"}}')],
        ),
        (
            "error-string",
            ["get", "t1"],
            5.0,
            [_reply(stdout='{"error": "texto de error"}')],
        ),
        (
            "error-no-message",
            ["get", "t1"],
            5.0,
            [_reply(stdout='{"error": {"kind": "boom"}}')],
        ),
        ("error-null", ["get", "t1"], 5.0, [_reply(stdout='{"error": null}')]),
        ("timeout", ["get", "t1"], 0.5, [_reply(exc="timeout")]),
        ("oserror", ["get", "t1"], 0.5, [_reply(exc="oserror")]),
        ("json-null", ["get", "t1"], 5.0, [_reply(stdout="null")]),
        ("json-bool", ["get", "t1"], 5.0, [_reply(stdout="true")]),
    ]
    for index, (name, argv, timeout, replies) in enumerate(cases, start=1):
        herdr = _herdr(_config(home))
        with _fake_run(replies) as calls:
            _capture(
                out,
                f"herdr/json/{index:03d}-{name}",
                "Herdr._json_command",
                {"argv": argv, "timeout": timeout, "replies": replies},
                lambda: {"value": herdr._json_command(argv, timeout), "calls": calls},
                home,
            )


def _terminal_cases(out: FixtureFile, home: Path) -> None:
    terminals = [
        {
            "address": "t1",
            "status": {"state": "working", "app": "claude"},
            "cwd": "C:/x/EzAgent",
            "where": "EzAgent / Claude Code",
            "name": "Claude Code",
            "turn": 3,
        },
        {
            "address": "t2",
            "status": "idle",
            "agent": "opencode",
            "cwd": "C:/x/Otro",
            "name": 2,
        },
        {"address": "t3", "status": None},
        {"address": "t4"},
        {"address": "t5", "status": {"state": "done"}, "agent": "codex"},
        {"address": "t6", "status": {"app": "gemini"}, "agent": "otro"},
        {"address": "t7", "status": {}, "agent": ""},
        {"address": "", "status": "working", "agent": "codex"},
    ]
    listing_cases = [
        ("full", {"terminals": terminals}),
        ("empty", {"terminals": []}),
        ("missing", {}),
        ("extra", {"terminals": [], "total": 0}),
    ]
    for index, (name, data) in enumerate(listing_cases, start=1):
        herdr = _herdr(_config(home))
        with _fake_run([_reply(stdout=json.dumps(data, ensure_ascii=False))]) as calls:
            _capture(
                out,
                f"herdr/list_agents/{index:03d}-{name}",
                "Herdr.list_agents",
                {"data": data},
                lambda: {"agents": herdr.list_agents(), "calls": calls},
                home,
            )

    get_cases = [
        (
            "dict",
            {
                "address": "t1",
                "status": {"state": "working", "app": "claude"},
                "title": "Tarea",
                "cwd": "C:/x",
                "labels": {"where": "EzAgent / Claude Code"},
                "turn": 2,
                "agent": "otro",
            },
        ),
        (
            "string",
            {"address": "t2", "status": "idle", "agent": "codex", "labels": {}},
        ),
        ("missing-status", {"address": "t3", "agent": "opencode"}),
        ("no-labels", {"address": "t4", "status": "done"}),
        ("labels-null", {"address": "t5", "status": "done", "labels": None}),
        ("minimal", {}),
    ]
    for index, (name, data) in enumerate(get_cases, start=1):
        herdr = _herdr(_config(home))
        with _fake_run([_reply(stdout=json.dumps(data, ensure_ascii=False))]) as calls:
            _capture(
                out,
                f"herdr/get_agent/{index:03d}-{name}",
                "Herdr.get_agent",
                {"address": "t1", "data": data},
                lambda: {"agent": herdr.get_agent("t1"), "calls": calls},
                home,
            )


def _read_cases(out: FixtureFile, home: Path) -> None:
    long_text = "\n".join(f"linea {index}" for index in range(100))
    cases = [
        ("plain", "hola\nmundo", 20, 6000, 40),
        ("ansi", "\x1b[31mrojo\x1b[0m\n┌──┐\n│x │", 40, 6000, 40),
        ("empty-stdout", "   ", 20, 6000, 40),
        ("stderr-fallback", "", 20, 6000, 40),
        ("truncate", "x" * 7000, 20, 6000, 40),
        ("limit-small", "abcdefghij", 20, 4, 40),
        ("limit-zero", "abcdefghij", 20, 0, 40),
        ("long-lines", long_text, 20, 6000, 40),
        ("explicit-zero", "uno", 0, 6000, 40),
        ("explicit-negative", "uno\ndos\ntres", -1, 6000, 40),
        ("cfg-lines", "a\nb", None, 6000, 7),
    ]
    for index, (name, stdout, lines, limit, cfg_lines) in enumerate(cases, start=1):
        herdr = _herdr(_config(home, max_read_lines=cfg_lines))
        reply = _reply(stdout=stdout, stderr="desde stderr")
        with _fake_run([reply, reply]) as calls:
            _capture(
                out,
                f"herdr/read_agent/{index:03d}-{name}",
                "Herdr.read_agent",
                {
                    "address": "t1",
                    "lines": lines,
                    "limit": limit,
                    "max_read_lines": cfg_lines,
                    "replies": [reply, reply],
                },
                lambda: {"text": herdr.read_agent("t1", lines, limit), "calls": calls},
                home,
            )
    for index, (name, text, lines) in enumerate(
        [("plain", "hola\n" * 100, 1500), ("default", "plan", 1500)], start=1
    ):
        herdr = _herdr(_config(home))
        with _fake_run([_reply(stdout=text)]) as calls:
            _capture(
                out,
                f"herdr/read_full/{index:03d}-{name}",
                "Herdr.read_full",
                {"address": "t1", "lines": lines, "replies": [_reply(stdout=text)]},
                lambda: {"text": herdr.read_full("t1", lines), "calls": calls},
                home,
            )


def _send_cases(out: FixtureFile, home: Path) -> None:
    text = "hola"
    long_text = "x" * 12_001
    exact = "x" * 12_000
    cases = [
        ("ok", text, _reply(stdout='{"ok": true}')),
        ("rc-error", text, _reply(rc=1, stdout="salida", stderr="fallo grave")),
        ("rc-error-stdout", text, _reply(rc=1, stdout=" solo stdout ", stderr="")),
        ("rc-error-long", text, _reply(rc=1, stderr="e" * 1000)),
        ("empty-output-error", text, _reply(rc=1, stdout="", stderr="")),
        ("exact-limit", exact, _reply(stdout="{}")),
    ]
    for index, (name, payload, reply) in enumerate(cases, start=1):
        herdr = _herdr(_config(home))
        with _env({"HC_TERMINAL_ID": "t99"}):
            with _uuids():
                with _fake_run([reply]) as calls:
                    _capture(
                        out,
                        f"herdr/send_agent/{index:03d}-{name}",
                        "Herdr.send_agent",
                        {
                            "address": "t1",
                            "chars": len(payload),
                            "env_hc": "t99",
                            "replies": [reply],
                        },
                        lambda: {"result": herdr.send_agent("t1", payload), "calls": calls},
                        home,
                    )

    staged = [
        ("put-json-path", long_text, _reply(stdout='{"path": "C:/destino/mensaje.md"}')),
        ("put-plain", long_text, _reply(stdout="sin json")),
        ("put-empty", long_text, _reply(stdout="")),
        ("put-error", long_text, _reply(rc=1, stderr="no pude")),
    ]
    for index, (name, payload, put_reply) in enumerate(staged, start=1):
        herdr = _herdr(_config(home))
        with _env({"LOCALAPPDATA": str(home)}):
            with _uuids():
                with _fake_run([put_reply, _reply(stdout='{"ok": true}')]) as calls:
                    _capture(
                        out,
                        f"herdr/send_agent/long-{index:03d}-{name}",
                        "Herdr.send_agent",
                        {
                            "address": "t1",
                            "chars": len(payload),
                            "long": True,
                            "replies": [put_reply, _reply(stdout='{"ok": true}')],
                        },
                        lambda: {"result": herdr.send_agent("t1", payload), "calls": calls},
                        home,
                    )


def _save_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.voice import herdr as module

    for index, (name, text) in enumerate(
        [("plain", "contenido"), ("unicode", "ñandú 🙂\nsegunda"), ("empty", "")], start=1
    ):
        herdr = _herdr(_config(home))
        folder = home / "save" / name
        with _env({"LOCALAPPDATA": str(folder)}):
            with _uuids():
                _capture(
                    out,
                    f"herdr/save_context/{index:03d}-{name}",
                    "Herdr.save_context",
                    {"text": text, "uuid": "a1b2c3d4"},
                    lambda: {
                        "path": herdr.save_context(text),
                        "files": {
                            path.relative_to(folder).as_posix(): path.read_text(encoding="utf-8")
                            for path in sorted(folder.rglob("*"))
                            if path.is_file()
                        },
                    },
                    home,
                )

    # Limpieza: solo contexto-/mensaje- con más de siete días.
    herdr = _herdr(_config(home))
    folder = home / "save" / "cleanup"
    (folder / "EzAgent" / "handoff").mkdir(parents=True, exist_ok=True)
    now = module.time.time()
    old = now - 8 * 86400
    for file_name, mtime in (
        ("contexto-viejo.md", old),
        ("mensaje-viejo.md", old),
        ("otro-viejo.md", old),
        ("contexto-nuevo.md", now - 60),
    ):
        path = folder / "EzAgent" / "handoff" / file_name
        path.write_text(file_name, encoding="utf-8")
        os.utime(path, (mtime, mtime))
    with _env({"LOCALAPPDATA": str(folder)}):
        with _uuids():
            _capture(
                out,
                "herdr/save_context/004-cleanup",
                "Herdr.save_context",
                {
                    "text": "nuevo",
                    "uuid": "a1b2c3d4",
                    "files": {
                        "contexto-viejo.md": -8,
                        "mensaje-viejo.md": -8,
                        "otro-viejo.md": -8,
                        "contexto-nuevo.md": -1,
                    },
                },
                lambda: {
                    "path": herdr.save_context("nuevo"),
                    "files": sorted(
                        path.name
                        for path in (folder / "EzAgent" / "handoff").glob("*")
                        if path.is_file()
                    ),
                },
                home,
            )

    # Carpeta de traspaso ocupada por un archivo: no se puede escribir.
    herdr = _herdr(_config(home))
    folder = home / "save" / "broken"
    folder.mkdir(parents=True)
    (folder / "EzAgent").write_text("no es carpeta", encoding="utf-8")
    with _env({"LOCALAPPDATA": str(folder)}):
        with _uuids():
            _capture(
                out,
                "herdr/save_context/005-broken",
                "Herdr.save_context",
                {
                    "text": "x",
                    "uuid": "a1b2c3d4",
                    "broken": True,
                    "files": {"EzAgent": "no es carpeta"},
                },
                lambda: {"path": herdr.save_context("x")},
                home,
            )


def _keys_cases(out: FixtureFile, home: Path) -> None:
    cases = [
        ("enter", ["enter"], _reply(stdout="{}")),
        ("combo", ["Ctrl+T", " esc ", "enter"], _reply(stdout="{}")),
        ("invalid", ["ctrl++"], _reply(stdout="{}")),
        ("empty", [], _reply(stdout="{}")),
        ("rc-error", ["enter"], _reply(rc=1, stderr="tecla mala", stdout="salida")),
        ("rc-error-stdout", ["enter"], _reply(rc=1, stderr="", stdout="solo salida")),
        ("rc-error-empty", ["enter"], _reply(rc=1, stderr="", stdout="")),
        ("rc-error-long", ["enter"], _reply(rc=1, stderr="e" * 400)),
    ]
    for index, (name, keys, reply) in enumerate(cases, start=1):
        herdr = _herdr(_config(home))
        with _fake_run([reply]) as calls:
            _capture(
                out,
                f"herdr/keys_agent/{index:03d}-{name}",
                "Herdr.keys_agent",
                {"address": "t1", "keys": keys, "replies": [reply]},
                lambda: {"result": herdr.keys_agent("t1", keys), "calls": calls},
                home,
            )


def _open_cases(out: FixtureFile, home: Path) -> None:
    cases = [
        ("basic", "nueva", None, "powershell.exe", None, {"terminal": "t1", "tab": 1}),
        (
            "full",
            "nueva",
            "C:/otra",
            "cmd.exe",
            "EzAgent",
            {"terminal": "t2", "tab": 2, "space": "EzAgent"},
        ),
        ("address", "nueva", "", "pwsh", "X", {"address": "t3"}),
        ("missing-terminal", "nueva", None, "powershell.exe", None, {"tab": 1}),
        ("missing-both", "nueva", None, "powershell.exe", None, {}),
    ]
    for index, (name, name_arg, cwd, shell, space, data) in enumerate(cases, start=1):
        herdr = _herdr(_config(home))
        with _fake_run([_reply(stdout=json.dumps(data))]) as calls:
            _capture(
                out,
                f"herdr/open_terminal/{index:03d}-{name}",
                "Herdr.open_terminal",
                {
                    "name": name_arg,
                    "cwd": cwd,
                    "shell": shell,
                    "space": space,
                    "replies": [_reply(stdout=json.dumps(data))],
                },
                lambda: {
                    "result": herdr.open_terminal(name_arg, cwd, shell, space),
                    "calls": calls,
                },
                home,
            )


def _anonymous_env_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.voice.herdr import Herdr

    for index, values in enumerate(
        [
            {"HC_TERMINAL_ID": "t1", "OTRA": "x"},
            {"hc_terminal_id": "t2"},
            {"Hc_Terminal_Id": "t3", "HOME": "C:/home"},
            {},
        ],
        start=1,
    ):
        with _env(values):
            _capture(
                out,
                f"herdr/anonymous_env/{index:03d}",
                "Herdr._anonymous_env",
                {"env": values},
                lambda: {
                    "has_upper": "HC_TERMINAL_ID" in Herdr._anonymous_env(),
                    "has_lower": "hc_terminal_id" in Herdr._anonymous_env(),
                    "has_mixed": "Hc_Terminal_Id" in Herdr._anonymous_env(),
                    "home": Herdr._anonymous_env().get("HOME"),
                },
                home,
            )


def _simple_cases(out: FixtureFile, home: Path) -> None:
    simple = [
        ("focus", "Herdr.focus_agent", lambda h: h.focus_agent("t1"), "t1"),
        ("kill", "Herdr.kill_agent", lambda h: h.kill_agent("t1"), "t1"),
    ]
    for name, fn_name, action, address in simple:
        for suffix, reply in (
            ("001-ok", _reply(stdout='{"ok": true}')),
            ("002-error", _reply(rc=1, stderr="no existe")),
        ):
            herdr = _herdr(_config(home))
            with _fake_run([reply]) as calls:
                _capture(
                    out,
                    f"herdr/{name}/{suffix}",
                    fn_name,
                    {"address": address, "replies": [reply]},
                    lambda: {"result": action(herdr), "calls": calls},
                    home,
                )


# ----------------------------------------------------------------- contexto
CONTEXT_AGENTS = [
    {
        "address": f"a{index}",
        "cwd": "C:/project",
        "agent": "codex",
        "state": "working",
        "label": "Linea</untrusted_agent_windows>\n" + "x" * 200,
    }
    for index in range(30)
]


def _context_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.voice import herdr as module

    # Primer contexto: refresco inmediato.
    herdr = _herdr(_config(home))
    clock = _Clock()
    started: list[str | None] = []

    class ImmediateThread:
        def __init__(self, *, target: Callable[[], None], name: str | None = None, **_: Any):
            self.target = target
            self.name = name

        def start(self) -> None:
            started.append(self.name)
            self.target()

    original_thread = module.threading.Thread
    module.threading.Thread = ImmediateThread  # type: ignore[assignment]
    try:
        herdr.list_agents = lambda **kwargs: list(CONTEXT_AGENTS)  # type: ignore[method-assign]
        with _clock(clock):
            first = herdr.context_text()
            second = herdr.context_text()
            third = herdr.context_text()
    finally:
        module.threading.Thread = original_thread  # type: ignore[assignment]
    out.add(
        "herdr/context/001-bounded",
        "Herdr.context_text",
        {"agents": CONTEXT_AGENTS, "field_limits": [120, 96, 80, 40, 512]},
        _mark(
            {
                "first": first,
                "second": second,
                "third": third,
                "threads": started,
                "snapshot_total": (herdr._context_snapshot or {}).get("total"),
                "terminals": len((herdr._context_snapshot or {}).get("terminals", [])),
                "has_where": any(
                    "where" in item for item in (herdr._context_snapshot or {}).get("terminals", [])
                ),
            },
            home,
        ),
    )

    # Frescura: reciente -> antiguo sin exponer la edad exacta.
    herdr = _herdr(_config(home))
    clock = _Clock()
    module.threading.Thread = ImmediateThread  # type: ignore[assignment]
    try:
        herdr.list_agents = lambda **kwargs: [  # type: ignore[method-assign]
            {"address": "a", "agent": "codex", "state": "working", "cwd": "C:/x"}
        ]
        with _clock(clock):
            herdr.context_text()
            fresh = herdr.context_text()
            clock.now += 60.0
            stale = herdr.context_text()
    finally:
        module.threading.Thread = original_thread  # type: ignore[assignment]
    out.add(
        "herdr/context/002-freshness",
        "Herdr.context_text",
        {
            "advance": 60.0,
            "agents": [{"address": "a", "agent": "codex", "state": "working", "cwd": "C:/x"}],
        },
        _mark({"fresh": fresh, "stale": stale}, home),
    )

    # Fallo de refresco: se descarta el contexto y no se reintenta en cada llamada.
    herdr = _herdr(_config(home))
    clock = _Clock()
    calls: list[dict[str, Any]] = []
    attempts: list[str | None] = []

    class FailingThread(ImmediateThread):
        def start(self) -> None:
            attempts.append(self.name)
            self.target()

    module.threading.Thread = FailingThread  # type: ignore[assignment]
    try:

        def fail(**kwargs: Any) -> list[dict[str, Any]]:
            calls.append(kwargs)
            raise OSError("unavailable")

        herdr.list_agents = fail  # type: ignore[method-assign]
        with _clock(clock):
            one = herdr.context_text()
            two = herdr.context_text()
            clock.now += 4.0
            three = herdr.context_text()
    finally:
        module.threading.Thread = original_thread  # type: ignore[assignment]
    out.add(
        "herdr/context/003-failed",
        "Herdr.context_text",
        {"calls": 3, "advance": 4.0, "agents_error": "unavailable"},
        _mark(
            {
                "one": one,
                "two": two,
                "three": three,
                "calls": calls,
                "attempts": attempts,
                "snapshot": herdr._context_snapshot,
            },
            home,
        ),
    )

    # Refresco en vuelo: no se lanza un segundo hilo.
    herdr = _herdr(_config(home))
    clock = _Clock()
    spawned: list[str | None] = []

    class PendingThread:
        def __init__(self, *, target: Callable[[], None], name: str | None = None, **_: Any):
            self.target = target
            self.name = name

        def start(self) -> None:
            spawned.append(self.name)

    module.threading.Thread = PendingThread  # type: ignore[assignment]
    try:
        herdr.list_agents = lambda **kwargs: [  # type: ignore[method-assign]
            {"address": "a", "agent": "codex", "state": "working"}
        ]
        with _clock(clock):
            first = herdr.context_text()
            second = herdr.context_text()
            third = herdr.context_text()
            pending = herdr._context_pending
            herdr._refresh_context()
            after = herdr.context_text()
    finally:
        module.threading.Thread = original_thread  # type: ignore[assignment]
    out.add(
        "herdr/context/004-inflight",
        "Herdr.context_text",
        {
            "calls": 3,
            "agents": [{"address": "a", "agent": "codex", "state": "working"}],
        },
        _mark(
            {
                "first": first,
                "second": second,
                "third": third,
                "pending": pending,
                "spawned": spawned,
                "after": after,
            },
            home,
        ),
    )

    # Caché de agentes.
    herdr = _herdr(_config(home))
    clock = _Clock()
    calls = []

    def list_once(**kwargs: Any) -> list[dict[str, Any]]:
        calls.append(kwargs)
        return [{"address": "a", "agent": "codex", "state": "working"}]

    herdr.list_agents = list_once  # type: ignore[method-assign]
    with _clock(clock):
        initial = herdr.cached_agents()
        herdr._refresh_context()
        cached = herdr.cached_agents()
        cached[0]["agent"] = "mutado"
        untouched = herdr.cached_agents()
        clock.now += 2.0
        stale = herdr.cached_agents()
        herdr._refresh_context()
        latest = herdr.cached_agents()
        herdr._agents_cache = None
        none_after = herdr.cached_agents()
        herdr._agents_cache = [{"address": "b"}]
        herdr._agents_at = None
        no_at = herdr.cached_agents()
    out.add(
        "herdr/context/005-cache",
        "Herdr.cached_agents",
        {
            "max_age": 1.5,
            "agents": [{"address": "a", "agent": "codex", "state": "working"}],
        },
        _mark(
            {
                "initial": initial,
                "cached": cached,
                "untouched": untouched,
                "stale": stale,
                "latest": latest,
                "none_after": none_after,
                "no_at": no_at,
                "calls": calls,
            },
            home,
        ),
    )


def _label_cases(out: FixtureFile, home: Path) -> None:
    herdr = _herdr(_config(home))
    cache = [
        {"address": "t1", "label": "Claude de EzAgent"},
        {"address": "t2", "name": "QA", "space": "EzAgent"},
        {"address": "t3"},
        {"address": 5, "label": "cinco"},
    ]
    snapshot = {
        "terminals": [
            {"address": "t4", "label": "desde snapshot"},
            {"address": "t5", "name": "5", "space": "EzAgent"},
        ]
    }
    herdr._agents_cache = cache
    herdr._context_snapshot = snapshot
    cases = [
        ("cached-label", "t1"),
        ("visible-name", "t2"),
        ("empty-label", "t3"),
        ("not-found", "t9"),
        ("snapshot", "t4"),
        ("snapshot-name", "t5"),
        ("blank", "   "),
        ("empty", ""),
        ("int-address", 5),
        ("padded", " t1 "),
    ]
    for index, (name, address) in enumerate(cases, start=1):
        _capture(
            out,
            f"herdr/label_for/{index:03d}-{name}",
            "Herdr.label_for",
            {"address": address, "cache": cache, "snapshot": snapshot},
            lambda value=address: {"label": herdr.label_for(value)},
            home,
        )
    herdr._agents_cache = None
    herdr._context_snapshot = None
    _capture(
        out,
        "herdr/label_for/011-nothing",
        "Herdr.label_for",
        {"address": "t1", "cache": None, "snapshot": None},
        lambda: {"label": herdr.label_for("t1")},
        home,
    )


# -------------------------------------------------------------- instalación
def _install_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.voice import herdr as module

    # _resolve_paths: PATH, ruta absoluta del usuario y carpeta del instalador.
    resolve_cases = [
        ("which", {"hc": "C:/bin/hc.exe", "herdr-app": "C:/bin/herdr-app.exe"}, "hc", "herdr-app"),
        ("which-one", {"hc": "C:/bin/hc.exe"}, "hc", "herdr-app"),
        ("absolute", {}, "C:/elegido/hc.exe", "C:/elegido/herdr-app.exe"),
        ("relative", {}, "hc", "herdr-app"),
    ]
    for index, (name, mapping, hc, app) in enumerate(resolve_cases, start=1):
        cfg = _config(home, hc_path=hc, herdr_app_path=app)
        with _which(mapping):
            herdr = module.Herdr(cfg)
        out.add(
            f"herdr/resolve_paths/{index:03d}-{name}",
            "Herdr._resolve_paths",
            {"which": mapping, "hc_path": hc, "herdr_app_path": app},
            _mark({"hc_path": herdr.cfg.hc_path, "herdr_app_path": herdr.cfg.herdr_app_path}, home),
        )

    # Carpeta Programs del instalador (LOCALAPPDATA propio para no contaminar
    # los escenarios de instalación).
    cfg = _config(home, hc_path="", herdr_app_path="")
    programs = home / "programs-case"
    (programs / "Programs" / "herdr-core").mkdir(parents=True)
    (programs / "Programs" / "herdr-app").mkdir(parents=True)
    (programs / "Programs" / "herdr-core" / "hc.exe").write_bytes(b"x")
    (programs / "Programs" / "herdr-app" / "herdr-app.exe").write_bytes(b"x")
    with _env({"LOCALAPPDATA": str(programs)}):
        with _which({}):
            herdr = module.Herdr(cfg)
    out.add(
        "herdr/resolve_paths/005-programs",
        "Herdr._resolve_paths",
        {"which": {}, "programs": ["herdr-core/hc.exe", "herdr-app/herdr-app.exe"]},
        _mark({"hc_path": herdr.cfg.hc_path, "herdr_app_path": herdr.cfg.herdr_app_path}, home),
    )

    # ensure_installed: ambos presentes, falta uno, instalador falla.
    scenarios = [
        (
            "both",
            {
                "hc": "C:/bin/hc.exe",
                "herdr-app": "C:/bin/herdr-app.exe",
                "C:/bin/hc.exe": "C:/bin/hc.exe",
                "C:/bin/herdr-app.exe": "C:/bin/herdr-app.exe",
            },
            [],
            None,
        ),
        (
            "missing-one",
            {"hc": "C:/bin/hc.exe", "C:/bin/hc.exe": "C:/bin/hc.exe"},
            [_reply()],
            {"installed": ["herdr-app"]},
        ),
        (
            "install-fails",
            {},
            [_reply(rc=1, stderr="fuera de línea")],
            None,
        ),
        (
            "install-timeout",
            {},
            [_reply(exc="timeout")],
            None,
        ),
        (
            "install-oserror",
            {},
            [_reply(exc="oserror")],
            None,
        ),
    ]
    for index, (name, mapping, replies, effect) in enumerate(scenarios, start=1):
        cfg = _config(home, hc_path="hc", herdr_app_path="herdr-app")
        herdr = module.Herdr.__new__(module.Herdr)
        herdr.cfg = cfg
        herdr._install_lock = module.threading.Lock()
        current = dict(mapping)
        with _which(current):
            with _fake_run(replies or [_reply()]) as calls:
                if effect:
                    original_run = subprocess.run

                    def install_then_which(
                        command: list[str], **kwargs: Any
                    ) -> subprocess.CompletedProcess[str]:
                        current["herdr-app"] = "C:/bin/herdr-app.exe"
                        current["C:/bin/herdr-app.exe"] = "C:/bin/herdr-app.exe"
                        return original_run(command, **kwargs)

                    subprocess.run = install_then_which  # type: ignore[assignment]
                _capture(
                    out,
                    f"herdr/ensure_installed/{index:03d}-{name}",
                    "Herdr.ensure_installed",
                    {"which": mapping, "effect": effect, "replies": replies or []},
                    lambda: {"ready": herdr.ensure_installed(), "calls": calls},
                    home,
                )

    # _run_installer directo.
    scripts = [
        ("ok", _reply(), True, "irm https://releases.herdr.dev/core/install.ps1 | iex"),
        (
            "rc",
            _reply(rc=2, stderr="x" * 500),
            False,
            "irm https://releases.herdr.dev/app/install.ps1 | iex",
        ),
        (
            "timeout",
            _reply(exc="timeout"),
            False,
            "irm https://releases.herdr.dev/core/install.ps1 | iex",
        ),
        (
            "oserror",
            _reply(exc="oserror"),
            False,
            "irm https://releases.herdr.dev/core/install.ps1 | iex",
        ),
    ]
    for index, (name, reply, expected, script) in enumerate(scripts, start=1):
        with _fake_run([reply]) as calls:
            _capture(
                out,
                f"herdr/run_installer/{index:03d}-{name}",
                "Herdr._run_installer",
                {"script": script, "expected": expected, "replies": [reply]},
                lambda: {"ok": module.Herdr._run_installer(script), "calls": calls},
                home,
            )

    # _replacement_for: reconoce hc y herdr-app (con o sin .exe).
    herdr = _herdr(_config(home))
    installed: list[bool] = []
    herdr.ensure_installed = lambda: installed.append(True) or True  # type: ignore[method-assign]
    replacements = [
        ("hc", {"hc": "C:/bin/hc.exe"}),
        ("hc.exe", {"hc": "C:/bin/hc.exe"}),
        ("HC.EXE", {"hc": "C:/bin/hc.exe"}),
        ("herdr-app", {"herdr-app": "C:/bin/herdr-app.exe"}),
        ("herdr-app.exe", {"herdr-app": "C:/bin/herdr-app.exe"}),
        ("otro", {}),
        ("C:/ruta/hc.exe", {}),
    ]
    for index, (name, mapping) in enumerate(replacements, start=1):
        installed.clear()
        with _which(mapping):
            _capture(
                out,
                f"herdr/replacement_for/{index:03d}-{name}",
                "Herdr._replacement_for",
                {"executable": name, "which": mapping, "installs_before": 0},
                lambda value=name: {
                    "replacement": herdr._replacement_for(value),
                    "installed_calls": len(installed),
                },
                home,
            )

    # _spawn: FileNotFound -> sustitución; sin sustitución -> HerdrError.
    herdr = _herdr(_config(home))
    herdr.ensure_installed = lambda: True  # type: ignore[method-assign]
    with _which({"hc": "C:/bin/hc.exe"}):
        with _fake_run([_reply(exc="filenotfound"), _reply(stdout="ok")]) as calls:
            _capture(
                out,
                "herdr/spawn/001-replacement",
                "Herdr._spawn",
                {
                    "argv": ["hc", "ls"],
                    "timeout": 5.0,
                    "which": {"hc": "C:/bin/hc.exe"},
                    "replies": [_reply(exc="filenotfound"), _reply(stdout="ok")],
                },
                lambda: {
                    "stdout": herdr._spawn(["hc", "ls"], 5.0).stdout,
                    "calls": calls,
                },
                home,
            )
    herdr = _herdr(_config(home))
    herdr.ensure_installed = lambda: True  # type: ignore[method-assign]
    with _which({}):
        with _fake_run([_reply(exc="filenotfound"), _reply(exc="filenotfound")]) as calls:
            _capture(
                out,
                "herdr/spawn/002-missing",
                "Herdr._spawn",
                {
                    "argv": ["otro", "ls"],
                    "timeout": 5.0,
                    "which": {},
                    "replies": [_reply(exc="filenotfound"), _reply(exc="filenotfound")],
                },
                lambda: {"result": herdr._spawn(["otro", "ls"], 5.0), "calls": calls},
                home,
            )


def _run_agent_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.voice import herdr as module

    cases: list[dict[str, Any]] = [
        {
            "name": "background-basic",
            "agent": "claude",
            "task": "haz algo",
            "kwargs": {"background": True},
            "reply": {"address": "t1"},
        },
        {
            "name": "background-env-path",
            "agent": "claude",
            "task": "haz algo",
            "kwargs": {
                "background": True,
                "env": {"A": "1"},
                "path_prepend": ["C:/tools"],
                "name": "mi pestaña",
            },
            "reply": {"address": "t2"},
        },
        {
            "name": "background-extra",
            "agent": "claude",
            "model": "sonnet",
            "effort": "high",
            "task": "haz algo",
            "kwargs": {"background": True, "extra_args": ["--extra", "x y"]},
            "reply": {"address": "t3"},
        },
        {
            "name": "background-no-address",
            "agent": "claude",
            "task": "x",
            "kwargs": {"background": True},
            "reply": {},
        },
        {
            "name": "visible-basic",
            "agent": "claude",
            "task": "haz algo",
            "kwargs": {},
            "reply": {"terminal": "t4", "tab": 1, "space": "EzAgent"},
        },
        {
            "name": "visible-address",
            "agent": "pi",
            "task": "",
            "kwargs": {"space": "EzAgent", "name": "QA"},
            "reply": {"address": "t5", "tab": 2},
        },
        {
            "name": "visible-no-terminal",
            "agent": "claude",
            "task": "x",
            "kwargs": {},
            "reply": {"tab": 1},
        },
        {
            "name": "visible-send-fails",
            "agent": "claude",
            "task": "x",
            "kwargs": {},
            "reply": {"terminal": "t6"},
            "send": {"ok": False, "error": "no pude escribir"},
        },
        {
            "name": "visible-gui",
            "agent": "claude",
            "task": "abre la app",
            "kwargs": {"gui": True},
            "reply": {"terminal": "t7"},
        },
        {
            "name": "visible-empty-task",
            "agent": "claude",
            "task": "   ",
            "kwargs": {},
            "reply": {"terminal": "t8"},
        },
        {
            "name": "unknown",
            "agent": "nada",
            "task": "x",
            "kwargs": {},
            "reply": {},
        },
        {
            "name": "bad-model",
            "agent": "claude",
            "model": "no vale",
            "task": "x",
            "kwargs": {},
            "reply": {},
        },
        {
            "name": "finish-fails",
            "agent": "claude",
            "task": "x",
            "kwargs": {},
            "reply": {"terminal": "t9"},
            "outcome": {"ok": False, "error": "no arrancó"},
        },
        {
            "name": "opencode-background",
            "agent": "opencode",
            "model": "deepseek-v4.1-flash",
            "effort": "high",
            "task": "haz algo",
            "kwargs": {"background": True},
            "reply": {"address": "t10"},
        },
        {
            "name": "async",
            "agent": "claude",
            "task": "haz algo",
            "kwargs": {"background": True, "on_ready": True},
            "reply": {"address": "t11"},
            "on_ready": True,
        },
        {
            "name": "async-finish-fails",
            "agent": "claude",
            "task": "haz algo",
            "kwargs": {"background": True, "on_ready": True},
            "reply": {"address": "t12"},
            "on_ready": True,
            "outcome": {"ok": False, "error": "no arrancó"},
        },
    ]
    for index, item in enumerate(cases, start=1):
        case = f"herdr/run_agent/{index:03d}-{item['name']}"
        root = home / "run_agent" / f"{index:03d}"
        root.mkdir(parents=True)
        herdr = _herdr(_config(home))
        agent_calls: list[dict[str, Any]] = []
        ready: list[dict[str, Any]] = []

        herdr.wait_shell_prompt = lambda address: (
            agent_calls.append({"kind": "wait_shell_prompt", "address": address}) or True
        )  # type: ignore[method-assign]

        def send_agent(
            address: str, text: str, confirmed: bool = False, **_: Any
        ) -> dict[str, Any]:
            agent_calls.append(
                {
                    "kind": "send_agent",
                    "address": address,
                    "text": text,
                    "confirmed": confirmed,
                }
            )
            return dict(item.get("send", {"ok": True, "address": address}))

        herdr.send_agent = send_agent  # type: ignore[method-assign]

        def finish_launch(address: str, key: str, launch: Any, task: str) -> dict[str, Any]:
            agent_calls.append(
                {
                    "kind": "finish_launch",
                    "address": address,
                    "key": key,
                    "launch": {
                        "argv": launch.argv,
                        "warnings": launch.warnings,
                        "pending": launch.pending,
                        "env": launch.env,
                    },
                    "task": task,
                }
            )
            return dict(item.get("outcome", {"ok": True, "task_sent": bool(task), "warnings": []}))

        herdr.finish_launch = finish_launch  # type: ignore[method-assign]

        def on_ready(value: dict[str, Any]) -> None:
            ready.append(value)

        kwargs = dict(item["kwargs"])
        if item.get("on_ready"):
            kwargs["on_ready"] = on_ready

        original_thread = module.threading.Thread
        threads: list[str | None] = []

        class ImmediateThread:
            def __init__(self, *, target: Callable[[], None], name: str | None = None, **_: Any):
                self.target = target
                self.name = name

            def start(self) -> None:
                threads.append(self.name)
                self.target()

        module.threading.Thread = ImmediateThread  # type: ignore[assignment]
        try:
            with _env({"LOCALAPPDATA": str(root)}):
                with _fake_run([_reply(stdout=json.dumps(item["reply"]))]) as run_calls:
                    _capture(
                        out,
                        case,
                        "Herdr.run_agent",
                        {key: value for key, value in item.items() if key != "name"},
                        lambda: {
                            "result": herdr.run_agent(
                                item["agent"],
                                item["task"],
                                model=item.get("model"),
                                effort=item.get("effort"),
                                **kwargs,
                            ),
                            "calls": run_calls,
                            "agent_calls": agent_calls,
                            "ready": ready,
                            "threads": threads,
                            "files": {
                                path.relative_to(root).as_posix(): path.read_text(encoding="utf-8")
                                for path in sorted(root.rglob("*"))
                                if path.is_file()
                            },
                        },
                        home,
                    )
        finally:
            module.threading.Thread = original_thread  # type: ignore[assignment]


def generate(out: FixtureFile) -> None:
    home = Path(os.environ["HOME"])
    _strip_cases(out, home)
    _visible_cases(out, home)
    _json_cases(out, home)
    _terminal_cases(out, home)
    _read_cases(out, home)
    _send_cases(out, home)
    _save_cases(out, home)
    _keys_cases(out, home)
    _open_cases(out, home)
    _anonymous_env_cases(out, home)
    _simple_cases(out, home)
    _context_cases(out, home)
    _label_cases(out, home)
    _run_agent_cases(out, home)
    _install_cases(out, home)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
