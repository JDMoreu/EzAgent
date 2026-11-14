"""Fixtures del lanzamiento y la sesión de agentes (dominio `agent_driver`, F1-15).

Cubre `agent_prelaunch.py`, `agent_session.py`, `agent_driver.py` y el
vigilante `watcher.py`. Los archivos de configuración de cada caso viven en
`<tmp>/cases/<caso>`; el reloj monotónico se sustituye por uno fijo que avanza
al dormir, y `shutil.which` se reemplaza por un doble. No se lanza ningún
proceso ni se toca la configuración real del usuario.
"""

from __future__ import annotations

import contextlib
import json
import os
import shutil
import subprocess
from collections.abc import Callable, Iterator
from pathlib import Path
from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "agent_driver"


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
def _isolated(home: Path, case: str, **overrides: str | None) -> Iterator[Path]:
    root = home / "cases" / case.replace("/", "_")
    root.mkdir(parents=True, exist_ok=True)
    values: dict[str, str | None] = {
        "HOME": str(root),
        "USERPROFILE": str(root),
        "LOCALAPPDATA": str(root),
        "APPDATA": str(root),
        "CODEX_HOME": str(root / "codex"),
        "MINIMAX_DATA_DIR": str(root / "minimax"),
        "OPENCODE_CONFIG": None,
        "OPENCODE_CONFIG_DIR": str(root / "opencode-config"),
    }
    values.update(
        {
            key: (value.replace("<root>", str(root)) if value is not None else None)
            for key, value in overrides.items()
        }
    )
    with _env(values):
        yield root


def _scripted(value: Any) -> Any:
    """Pantalla o respuesta de un doble: texto/dict, o la excepción a lanzar."""
    if isinstance(value, (str, dict)):
        return value
    return {"raise": type(value).__name__, "message": str(value)}


def _display(value: Any, case: str) -> Any:
    """Marcador estable para las rutas del caso dentro de un `input`."""
    if isinstance(value, str):
        return value.replace("<root>", f"<tmp>/cases/{case.replace('/', '_')}")
    if isinstance(value, list):
        return [_display(item, case) for item in value]
    if isinstance(value, dict):
        return {key: _display(item, case) for key, item in value.items()}
    return value


def _put(root: Path, relative: str, text: str) -> Path:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="")
    return path


def _snapshot(root: Path) -> dict[str, Any]:
    files: dict[str, Any] = {}
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        raw = path.read_bytes()
        files[path.relative_to(root).as_posix()] = {
            "text": raw.decode("utf-8", errors="replace"),
            "crlf": b"\r\n" in raw,
        }
    return files


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


def _config(**overrides: Any) -> Any:
    from ezagent.voice.config import VoiceConfig

    values: dict[str, Any] = {"hc_path": "hc", "herdr_app_path": "herdr-app"}
    values.update(overrides)
    return VoiceConfig(**values)


# --------------------------------------------------------------- prelaunch
def _prelaunch_paths(out: FixtureFile, home: Path) -> None:
    from ezagent.voice import agent_prelaunch as module

    cases = [
        ("local", {"LOCALAPPDATA": "<root>"}),
        ("home-fallback", {"LOCALAPPDATA": None}),
    ]
    for index, (name, overrides) in enumerate(cases, start=1):
        case = f"agent_driver/prelaunch/opencode_state_root/{index:03d}-{name}"
        with _isolated(home, case, **overrides):
            _capture(
                out,
                case,
                "opencode_state_root",
                {"env": _display(overrides, case)},
                lambda: {"path": str(module.opencode_state_root())},
                home,
            )

    for index, (name, overrides) in enumerate(
        [
            ("minimax", {"MINIMAX_DATA_DIR": "<root>"}),
            ("home-fallback", {"MINIMAX_DATA_DIR": None}),
        ],
        start=1,
    ):
        case = f"agent_driver/prelaunch/mcode_config_path/{index:03d}-{name}"
        with _isolated(home, case, **overrides):
            _capture(
                out,
                case,
                "mcode_config_path",
                {"env": _display(overrides, case)},
                lambda: {"path": str(module.mcode_config_path())},
                home,
            )

    for index, (name, overrides) in enumerate(
        [
            ("codex", {"CODEX_HOME": "<root>/codex"}),
            ("home-fallback", {"CODEX_HOME": None}),
        ],
        start=1,
    ):
        case = f"agent_driver/prelaunch/codex_home/{index:03d}-{name}"
        with _isolated(home, case, **overrides):
            _capture(
                out,
                case,
                "codex_home",
                {"env": _display(overrides, case)},
                lambda: {"path": str(module.codex_home())},
                home,
            )


def _prepare_state_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.voice.agent_prelaunch import prepare_opencode_state

    cases = [
        ("high", "opencode-go/deepseek-v4.1-flash", "high", None),
        ("no-effort", "opencode-go/glm-5.3", "", None),
        ("multi-slash", "a/b/c", "max", None),
        ("no-slash", "modelo", "low", None),
        ("empty", "", "", None),
        ("only-slash", "/modelo", "low", None),
        ("trailing-slash", "proveedor/", "low", None),
        ("unicode", "proveedor/modelo-ñ", "alto", None),
        (
            "broken",
            "opencode-go/glm-5.3",
            "high",
            {"EzAgent/opencode-state/opencode": "no es carpeta"},
        ),
    ]
    for index, (name, model_ref, effort, files) in enumerate(cases, start=1):
        case = f"agent_driver/prelaunch/prepare_opencode_state/{index:03d}-{name}"
        with _isolated(home, case) as root:
            for relative, text in (files or {}).items():
                _put(root, relative, text)
            _capture(
                out,
                case,
                "prepare_opencode_state",
                {"model_ref": model_ref, "effort": effort, "files": files or {}},
                lambda: {
                    "env": prepare_opencode_state(model_ref, effort),
                    "files": _snapshot(root),
                },
                home,
            )


MCODE_YAML = """\
provider:
  minimax:
    models:
      MiniMax-M3.1-Flash: {}
      abab6.5s: {}
  otro:
    models:
      x: {}
"""


def _mcode_discovery_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.voice.agent_prelaunch import discover_mcode_models

    cases = [
        ("full", MCODE_YAML),
        ("empty-provider", "provider: {}\n"),
        ("empty-models", "provider:\n  p:\n    models: {}\n"),
        ("no-models", "provider:\n  p:\n    name: x\n"),
        ("spec-not-dict", "provider:\n  p: texto\n"),
        ("data-list", "- uno\n- dos\n"),
        ("data-empty", ""),
        ("invalid", "provider: [\n"),
        ("missing", None),
        ("numeric-models", "provider:\n  p:\n    models:\n      1: {}\n      2: {}\n"),
    ]
    for index, (name, text) in enumerate(cases, start=1):
        case = f"agent_driver/prelaunch/discover_mcode_models/{index:03d}-{name}"
        with _isolated(home, case) as root:
            if text is not None:
                _put(root, "minimax/config.yaml", text)
            _capture(
                out,
                case,
                "discover_mcode_models",
                {"config": text},
                lambda: {"models": discover_mcode_models()},
                home,
            )


def _mcode_effort_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.voice.agent_prelaunch import set_mcode_effort

    base = "model: x\n"
    cases = [
        ("append-lf", "model: x\n", "low", None),
        ("append-no-newline", "model: x", "low", None),
        ("append-crlf", "model: x\r\n", "high", None),
        ("replace-lf", f"{base}defaultModelThinking:\n  effort: high\n", "low", None),
        ("replace-crlf", f"{base}defaultModelThinking:\r\n  effort: high\r\n", "max", None),
        (
            "replace-tabs",
            f"{base}defaultModelThinking:\t\n\teffort:\thigh\n",
            "medium",
            None,
        ),
        ("same", f"{base}defaultModelThinking:\n  effort: high\n", "high", None),
        (
            "first-only",
            f"{base}defaultModelThinking:\n  effort: high\notro:\n  effort: low\n",
            "low",
            None,
        ),
        ("invalid", f"{base}defaultModelThinking:\n  effort: high\n", "turbo", None),
        ("missing", None, "low", None),
        (
            "backup-exists",
            f"{base}defaultModelThinking:\n  effort: high\n",
            "low",
            {"minimax/config.yaml.ezagent-bak": "copia vieja\n"},
        ),
        ("empty-file", "", "low", None),
        ("dir-config", None, "low", {"minimax/config.yaml/nota.txt": "x"}),
    ]
    for index, (name, text, effort, files) in enumerate(cases, start=1):
        case = f"agent_driver/prelaunch/set_mcode_effort/{index:03d}-{name}"
        with _isolated(home, case) as root:
            if text is not None:
                _put(root, "minimax/config.yaml", text)
            for relative, content in (files or {}).items():
                _put(root, relative, content)
            _capture(
                out,
                case,
                "set_mcode_effort",
                {"effort": effort, "config": text, "files": files or {}},
                lambda: {"ok": set_mcode_effort(effort), "files": _snapshot(root)},
                home,
            )


CODEX_JSON = {
    "models": [
        {
            "slug": "gpt-5.1",
            "visibility": "list",
            "supported_reasoning_levels": [
                {"effort": "low"},
                {"effort": "medium"},
                {"effort": "high"},
            ],
        },
        {
            "slug": "gpt-5.1-codex",
            "visibility": "list",
            "supported_reasoning_levels": ["low", "high"],
        },
        {
            "slug": "oculto",
            "visibility": "hidden",
            "supported_reasoning_levels": [{"effort": "high"}],
        },
        {
            "slug": "sin-niveles",
            "visibility": "list",
            "supported_reasoning_levels": [],
        },
        {
            "visibility": "list",
            "supported_reasoning_levels": [{"effort": "low"}],
        },
        {
            "slug": "  ",
            "visibility": "list",
            "supported_reasoning_levels": [{"effort": "low"}],
        },
        {
            "slug": "raro",
            "visibility": "list",
            "supported_reasoning_levels": [None, {"effort": None}, 5],
        },
        {
            "slug": "gpt-5.1",
            "visibility": "list",
            "supported_reasoning_levels": [{"effort": "ultra"}],
        },
    ]
}


def _codex_discovery_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.voice.agent_prelaunch import discover_codex_models

    cases = [
        ("full", json.dumps(CODEX_JSON, ensure_ascii=False)),
        ("empty", '{"models": []}'),
        ("no-models", "{}"),
        ("data-list", "[]"),
        ("invalid", '{"models": ['),
        ("missing", None),
        ("models-null", '{"models": null}'),
        ("model-not-dict", '{"models": ["texto"]}'),
        (
            "levels-not-list",
            '{"models": [{"slug": "a", "visibility": "list", "supported_reasoning_levels": "xy"}]}',
        ),
    ]
    for index, (name, text) in enumerate(cases, start=1):
        case = f"agent_driver/prelaunch/discover_codex_models/{index:03d}-{name}"
        with _isolated(home, case) as root:
            if text is not None:
                _put(root, "codex/models_cache.json", text)
            _capture(
                out,
                case,
                "discover_codex_models",
                {"cache": text},
                lambda: {"models": discover_codex_models()},
                home,
            )


JSONC_CASES = [
    ("plain", '{"a": 1}'),
    ("line-comment", '{\n  // comentario\n  "a": 1\n}'),
    ("block-comment", '{ /* bloque */ "a": 1 }'),
    ("trailing-object", '{"a": 1,}'),
    ("trailing-array", '{"a": [1, 2,],}'),
    ("string-markers", '{"a": "http://x", "b": "/* no */"}'),
    ("escaped-quote", '{"a": "dice \\"hola\\"", "b": 2}'),
    ("unterminated-block", '{"a": 1} /* sin cierre'),
    ("line-at-end", '{"a": 1} // fin'),
    ("crlf", '{\r\n  // x\r\n  "a": 1,\r\n}\r\n'),
    ("escaped-backslash", '{"a": "c:\\\\x", "b": 1}'),
    ("empty", ""),
]


def _jsonc_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.voice.agent_prelaunch import _strip_jsonc

    for index, (name, text) in enumerate(JSONC_CASES, start=1):
        case = f"agent_driver/prelaunch/strip_jsonc/{index:03d}-{name}"
        with _isolated(home, case):
            _capture(
                out,
                case,
                "_strip_jsonc",
                {"text": text},
                lambda: {"text": _strip_jsonc(text)},
                home,
            )


def _opencode_default_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.voice.agent_prelaunch import opencode_default_model

    cases: list[tuple[str, dict[str, str], dict[str, str | None], str]] = [
        (
            "jsonc",
            {"opencode-config/opencode.jsonc": '{"model": "proveedor/modelo"}'},
            {},
            "jsonc",
        ),
        (
            "json",
            {"opencode-config/opencode.json": '{"model": "proveedor/modelo"}'},
            {},
            "json",
        ),
        (
            "custom-first",
            {
                "custom.json": '{"model": "custom/modelo"}',
                "opencode-config/opencode.jsonc": '{"model": "dir/modelo"}',
            },
            {"OPENCODE_CONFIG": "<root>/custom.json"},
            "custom",
        ),
        (
            "custom-missing-falls-back",
            {"opencode-config/opencode.jsonc": '{"model": "dir/modelo"}'},
            {"OPENCODE_CONFIG": "<root>/nada.json"},
            "fallback",
        ),
        (
            "custom-empty",
            {"opencode-config/opencode.jsonc": '{"model": "dir/modelo"}'},
            {"OPENCODE_CONFIG": ""},
            "empty-custom",
        ),
        (
            "variant",
            {"opencode-config/opencode.jsonc": '{"model": "a/b#high"}'},
            {},
            "variant",
        ),
        (
            "dict-model",
            {
                "opencode-config/opencode.jsonc": (
                    '{"model": {"providerID": "opencode-go", "model": "glm-5.3"}}'
                )
            },
            {},
            "dict",
        ),
        (
            "dict-incomplete",
            {
                "opencode-config/opencode.jsonc": '{"model": {"providerID": "x"}}',
                "opencode-config/opencode.json": '{"model": "otro/modelo"}',
            },
            {},
            "dict-incomplete",
        ),
        (
            "no-slash",
            {"opencode-config/opencode.jsonc": '{"model": "modelo"}'},
            {},
            "no-slash",
        ),
        (
            "invalid",
            {"opencode-config/opencode.jsonc": '{"model": '},
            {},
            "invalid",
        ),
        ("missing", {}, {}, "missing"),
        (
            "jsonc-comments",
            {"opencode-config/opencode.jsonc": ('{\n // x\n "model": "p/m",\n}\n')},
            {},
            "comments",
        ),
        (
            "config-dir-override",
            {"otra/opencode.json": '{"model": "p/m"}'},
            {"OPENCODE_CONFIG_DIR": "<root>/otra"},
            "dir-override",
        ),
    ]
    for index, (name, files, env, _label) in enumerate(cases, start=1):
        case = f"agent_driver/prelaunch/opencode_default_model/{index:03d}-{name}"
        with _isolated(home, case) as root:
            for relative, text in files.items():
                _put(root, relative, text)
            expanded = {
                key: (value.replace("<root>", str(root)) if value is not None else None)
                for key, value in env.items()
            }
            with _env(expanded):
                _capture(
                    out,
                    case,
                    "opencode_default_model",
                    {"files": files, "env": _display(env, case)},
                    lambda: {"model": opencode_default_model()},
                    home,
                )


# ------------------------------------------------------------------ sesión
class _Clock:
    def __init__(self, now: float = 0.0) -> None:
        self.now = now
        self.delays: list[float] = []

    def monotonic(self) -> float:
        return self.now

    def time(self) -> float:
        return self.now

    def sleep(self, seconds: float) -> None:
        self.delays.append(seconds)
        self.now += seconds


@contextlib.contextmanager
def _clock(clock: _Clock) -> Iterator[_Clock]:
    from ezagent.voice import agent_session as module

    original = module.time
    module.time = clock  # type: ignore[assignment]
    try:
        yield clock
    finally:
        module.time = original  # type: ignore[assignment]


def _next(values: list[Any], index: int) -> Any:
    return values[min(index, len(values) - 1)] if values else None


class _FakeSession:
    """Doble mínimo de `AgentSession` para fijar pantallas y respuestas."""

    def __init__(
        self,
        cfg: Any,
        screens: list[Any] | None = None,
        agents: list[Any] | None = None,
        sends: list[Any] | None = None,
        keys: list[Any] | None = None,
    ) -> None:
        self.cfg = cfg
        self.screens = screens or [""]
        self.agents = agents or [{}]
        self.sends = sends or [{"ok": True}]
        self.keys_results = keys or [{"ok": True}]
        self.calls: list[dict[str, Any]] = []
        self.screen_index = self.agent_index = self.send_index = self.key_index = 0

    def _profiles(self) -> dict[str, Any]:
        profiles = getattr(self.cfg, "agent_profiles", {}) or {}
        return profiles if isinstance(profiles, dict) else {}

    def _profile(self, key: str) -> dict[str, Any]:
        profile = self._profiles().get(key)
        return profile if isinstance(profile, dict) else {}

    def get_agent(self, address: str) -> dict[str, Any]:
        value = _next(self.agents, self.agent_index)
        self.agent_index += 1
        self.calls.append({"kind": "get", "address": address})
        if isinstance(value, Exception):
            raise value
        return dict(value)

    def read_agent(self, address: str, lines: int | None = None) -> str:
        value = _next(self.screens, self.screen_index)
        self.screen_index += 1
        self.calls.append({"kind": "read", "address": address, "lines": lines})
        if isinstance(value, Exception):
            raise value
        return str(value)

    def send_agent(self, address: str, text: str, confirmed: bool = False) -> dict[str, Any]:
        value = _next(self.sends, self.send_index)
        self.send_index += 1
        self.calls.append(
            {"kind": "send", "address": address, "text": text, "confirmed": confirmed}
        )
        return dict(value) if isinstance(value, dict) else {"ok": bool(value)}

    def keys_agent(self, address: str, keys: list[str]) -> dict[str, Any]:
        value = _next(self.keys_results, self.key_index)
        self.key_index += 1
        self.calls.append({"kind": "keys", "address": address, "keys": list(keys)})
        return dict(value) if isinstance(value, dict) else {"ok": bool(value)}


def _session(
    cfg: Any,
    screens: list[Any] | None = None,
    agents: list[Any] | None = None,
    sends: list[Any] | None = None,
    keys: list[Any] | None = None,
) -> _FakeSession:
    from ezagent.voice.agent_session import AgentSession

    class Fake(_FakeSession, AgentSession):
        pass

    return Fake(cfg, screens, agents, sends, keys)


def _driver(
    cfg: Any,
    screens: list[Any] | None = None,
    agents: list[Any] | None = None,
    sends: list[Any] | None = None,
    keys: list[Any] | None = None,
) -> _FakeSession:
    from ezagent.voice.agent_driver import AgentDriver

    class Fake(_FakeSession, AgentDriver):
        pass

    return Fake(cfg, screens, agents, sends, keys)


def _session_capture(
    out: FixtureFile,
    home: Path,
    case: str,
    fn_name: str,
    input_data: Any,
    action: Callable[[], Any],
    session: _FakeSession,
    clock: _Clock,
) -> None:
    result: dict[str, Any] = {}
    session._sleep = clock.sleep  # type: ignore[attr-defined]

    def run() -> dict[str, Any]:
        result["value"] = action()
        result["calls"] = session.calls
        result["delays"] = clock.delays
        return result

    _capture(out, case, fn_name, input_data, run, home)


def _session_text_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.voice.agent_session import _is_shell, command_line, fill, valid_keys

    key_cases = [
        ["enter"],
        ["Ctrl+T"],
        ["alt+shift+Tab"],
        ["esc", "up", "down", "left", "right", "home", "end"],
        ["pageup", "pagedown", "backspace", "space"],
        [f"f{number}" for number in range(1, 13)],
        [f"f{number}" for number in range(1, 14)],
        ["ctrl++"],
        ["enter "],
        [" a "],
        ["5"],
        [5],
        [],
        "enter",
        None,
        ["ctrl+alt+shift+a"],
        ["ctrl+ctrl+a"],
        ["meta+a"],
        ["f0"],
        ["f13"],
        ["Enter", "ESC", "TAB"],
    ]
    for index, value in enumerate(key_cases, start=1):
        case = f"agent_driver/session/valid_keys/{index:03d}"
        with _isolated(home, case):
            _capture(
                out,
                case,
                "valid_keys",
                {"keys": value},
                lambda item=value: {"keys": valid_keys(item)},
                home,
            )

    fill_cases = [
        ("/model {model}", {"model": "sonnet"}, False),
        ("/model {model_search}", {"model": "proveedor/mi-modelo_largo"}, False),
        ("{model_search}|{model}|{effort}", {"model": "a/b-c_d", "effort": "high"}, False),
        ("{model_search}", {"model": "a.b+c#d"}, True),
        ("{model}", {"model": "a.b+c#d"}, True),
        ("{effort}", {"effort": "x(y)"}, True),
        ("{model}/{model}", {"model": "m"}, False),
        ("sin marcadores", {"model": "m"}, True),
        ("{otro}", {"model": "m"}, False),
        ("{model}", {}, False),
        ("{model_search}", {"model": "solo"}, True),
        ("{model}", {"model": "ñandú/emoción"}, True),
    ]
    for index, (text, values, escape) in enumerate(fill_cases, start=1):
        case = f"agent_driver/session/fill/{index:03d}"
        with _isolated(home, case):
            _capture(
                out,
                case,
                "fill",
                {"text": text, "values": values, "escape": escape},
                lambda t=text, v=values, e=escape: {"text": fill(t, v, e)},
                home,
            )

    command_cases = [
        ("simple", ["claude", "--model", "sonnet"], None, None),
        ("empty", [], None, None),
        ("spaces", ["a b", "c"], None, None),
        ("quotes", ["dice 'hola'", 'di "hola"'], None, None),
        ("empty-arg", ["a", "", "b"], None, None),
        ("unicode", ["ñandú", "🙂"], None, None),
        ("special", ["a$b", "c;d", "e|f", "g&h", "(i)", "{j}"], None, None),
        ("backslash", [r"C:\tmp\notas.txt"], None, None),
        ("allowed", ["abc_09./:=@%+,\\-"], None, None),
        ("dash", ["-"], None, None),
        ("newline", ["linea\nsiguiente"], None, None),
        ("env", ["opencode"], {"A": "1", "B": "dos"}, None),
        ("env-quote", ["x"], {"A": "a'b"}, None),
        ("path", ["x"], None, ["C:/uno", "C:/dos"]),
        ("env-path", ["x"], {"A": "1"}, ["C:/uno"]),
        ("env-only", [], {"A": "1"}, None),
    ]
    for index, (name, argv, env, path_prepend) in enumerate(command_cases, start=1):
        case = f"agent_driver/session/command_line/{index:03d}-{name}"
        with _isolated(home, case):
            _capture(
                out,
                case,
                "command_line",
                {"argv": argv, "env": env, "path_prepend": path_prepend},
                lambda a=argv, e=env, p=path_prepend: {"line": command_line(a, e, p)},
                home,
            )

    shell_cases = [
        "powershell.exe",
        "PowerShell.EXE",
        "powershell",
        "pwsh",
        "cmd.exe",
        "cmd",
        "bash",
        "sh",
        "zsh",
        "nu",
        "fish",
        "python.exe",
        "git-bash",
        "",
        "  ",
        "powershell.cmd",
        "powershell.bat",
    ]
    for index, name in enumerate(shell_cases, start=1):
        case = f"agent_driver/session/is_shell/{index:03d}"
        with _isolated(home, case):
            _capture(
                out,
                case,
                "_is_shell",
                {"name": name},
                lambda value=name: {"is_shell": _is_shell(value)},
                home,
            )


def _wait_cases(out: FixtureFile, home: Path) -> None:
    prompt_cases = [
        ("immediate", ["PS C:\\Users\\u>"], None),
        ("after-three", ["", "iniciando", "PS C:\\>"], None),
        ("empty", [""], None),
        ("no-prompt", ["uno\ndos\nPS C:\\> pero no"], None),
        ("timeout", ["sin prompt"], None),
        ("read-error", [OSError("ilegible")], None),
        ("prompt-no-space", ["PS>"], None),
        ("blank-lines", ["\n\nPS C:\\x> \n\n"], None),
    ]
    for index, (name, screens, _) in enumerate(prompt_cases, start=1):
        case = f"agent_driver/session/wait_shell_prompt/{index:03d}-{name}"
        with _isolated(home, case):
            clock = _Clock()
            session = _session(_config(), screens)
            with _clock(clock):
                _session_capture(
                    out,
                    home,
                    case,
                    "AgentSession.wait_shell_prompt",
                    {"screens": [_scripted(item) for item in screens]},
                    lambda: session.wait_shell_prompt("t1"),
                    session,
                    clock,
                )

    ready_cases: list[tuple[str, list[Any], list[Any], float]] = [
        ("agent-done", ["hola"], [{"agent": "codex", "state": "done"}], 30.0),
        ("agent-idle", ["hola"], [{"agent": "opencode", "state": "idle"}], 30.0),
        ("working-timeout", ["hola"], [{"agent": "codex", "state": "working"}], 1.0),
        (
            "working-finally-done",
            ["hola"],
            [{"agent": "codex", "state": "working"}, {"agent": "codex", "state": "done"}],
            30.0,
        ),
        ("shell-program", ["uno\ndos\ntres"], [{"agent": "powershell.exe", "state": "idle"}], 30.0),
        (
            "shell-prompt",
            ["uno\ndos\nPS C:\\>"],
            [{"agent": "powershell.exe", "state": "idle"}],
            1.0,
        ),
        ("app-fallback", ["uno\ndos\ntres"], [{"app": "pi", "state": "idle"}], 30.0),
        ("state-spaces", ["hola"], [{"agent": "codex", "state": " Working "}], 1.0),
        ("get-error-shell", ["uno\ndos\ntres"], [OSError("sin terminal")], 30.0),
        ("get-error-agent", ["hola"], [OSError("sin terminal")], 1.0),
        ("timeout-zero", ["hola"], [{"agent": "codex", "state": "done"}], 0.0),
        ("unknown-agent", ["hola"], [{"agent": "desconocido", "state": "idle"}], 30.0),
    ]
    for index, (name, screens, agents, timeout) in enumerate(ready_cases, start=1):
        case = f"agent_driver/session/wait_ready/{index:03d}-{name}"
        with _isolated(home, case):
            clock = _Clock()
            session = _session(_config(), screens, agents)
            with _clock(clock):
                _session_capture(
                    out,
                    home,
                    case,
                    "AgentSession.wait_ready",
                    {
                        "screens": [_scripted(item) for item in screens],
                        "agents": [_scripted(item) for item in agents],
                        "timeout": timeout,
                    },
                    lambda: session.wait_ready("t1", timeout),
                    session,
                    clock,
                )

    program_cases = [
        ("three", "uno\ndos\ntres"),
        ("two", "uno\ndos"),
        ("ends-prompt", "uno\ndos\nPS C:\\>"),
        ("empty", ""),
        ("blank", " \n\t\n"),
        ("one-with-prompt", "PS C:\\>"),
    ]
    for index, (name, screen) in enumerate(program_cases, start=1):
        case = f"agent_driver/session/screen_shows_program/{index:03d}-{name}"
        with _isolated(home, case):
            clock = _Clock()
            session = _session(_config(), [screen])
            with _clock(clock):
                _session_capture(
                    out,
                    home,
                    case,
                    "AgentSession._screen_shows_program",
                    {"screen": screen},
                    lambda: session._screen_shows_program("t1"),
                    session,
                    clock,
                )

    screen_cases = [
        ("short", "uno\ndos"),
        ("long", "\n".join(f"linea {index}" for index in range(50))),
        ("empty", ""),
        ("read-error", OSError("ilegible")),
    ]
    for index, (name, screen) in enumerate(screen_cases, start=1):
        case = f"agent_driver/session/screen/{index:03d}-{name}"
        with _isolated(home, case):
            clock = _Clock()
            session = _session(_config(), [screen])
            with _clock(clock):
                _session_capture(
                    out,
                    home,
                    case,
                    "AgentSession._try_screen",
                    {"screen": _scripted(screen)},
                    lambda: {"screen": session._try_screen("t1"), "safe": session._screen("t1")},
                    session,
                    clock,
                )


def _session_steps_cases(out: FixtureFile, home: Path) -> None:

    profiles = {
        "lista": {
            "session_commands": {
                "model": [
                    "texto",
                    "   ",
                    {"send": "/model {model}"},
                    {"keys": ["esc"]},
                    {"cycle": {"keys": ["ctrl+t"], "until": "x"}},
                    {"nada": 1},
                    5,
                ],
                "effort": {"send": "/effort {effort}"},
            }
        },
        "texto": {"session_commands": {"model": "/model {model}", "effort": ""}},
        "vacio": {"session_commands": {}},
        "no-dict": {"session_commands": ["x"]},
        "nada": {},
    }
    cases = [
        ("lista", "model"),
        ("lista", "effort"),
        ("lista", "otro"),
        ("texto", "model"),
        ("texto", "effort"),
        ("vacio", "model"),
        ("no-dict", "model"),
        ("nada", "model"),
    ]
    for index, (key, kind) in enumerate(cases, start=1):
        case = f"agent_driver/session/session_steps/{index:03d}-{key}-{kind}"
        with _isolated(home, case):
            cfg = _config(agent_profiles=profiles)
            session = _session(cfg)
            _capture(
                out,
                case,
                "AgentSession._session_steps",
                {"profile": key, "kind": kind, "profiles": profiles},
                lambda k=key, t=kind: {"steps": session._session_steps(k, t)},
                home,
            )

    check_cases = [
        (
            "no-conditions",
            {"send": "x"},
            {},
            ["hola"],
        ),
        ("expect-first", {"expect": "Switched"}, {"model": "m"}, ["Switched model to m"]),
        (
            "expect-third",
            {"expect": "Switched"},
            {"model": "m"},
            ["nada", "nada", "Switched model to m"],
        ),
        ("expect-never", {"expect": "Switched"}, {"model": "m"}, ["nada"]),
        ("fail-match", {"fail": "No variants"}, {"model": "m"}, ["No variants available"]),
        ("fail-no-match", {"fail": "No variants"}, {"model": "m"}, ["otra cosa"]),
        (
            "both",
            {"expect": "Switched", "fail": "No variants"},
            {"model": "m"},
            ["Switched model to m"],
        ),
        (
            "both-fail",
            {"expect": "Switched", "fail": "No variants"},
            {"model": "m"},
            ["No variants available"],
        ),
        (
            "custom-message",
            {"expect": "Switched", "message": "no encontré {model}"},
            {"model": "m"},
            ["nada"],
        ),
        ("read-error", {"expect": "Switched"}, {"model": "m"}, [OSError("ilegible")]),
        ("case-insensitive", {"expect": "switched"}, {}, ["SWITCHED"]),
        ("regex", {"expect": r"Switched\s+model"}, {}, ["Switched   model to m"]),
    ]
    for index, (name, step, values, screens) in enumerate(check_cases, start=1):
        case = f"agent_driver/session/check_screen/{index:03d}-{name}"
        with _isolated(home, case):
            clock = _Clock()
            session = _session(_config(), screens)
            with _clock(clock):
                _session_capture(
                    out,
                    home,
                    case,
                    "AgentSession._check_screen",
                    {
                        "step": step,
                        "values": values,
                        "screens": [_scripted(item) for item in screens],
                    },
                    lambda: session._check_screen("t1", step, values),
                    session,
                    clock,
                )

    cycle_cases = [
        (
            "second-try",
            {"cycle": {"keys": ["ctrl+t"], "until": "Switched variant to high", "max": 3}},
            {"effort": "high"},
            ["nada", "Switched variant to high"],
            [{"ok": True}],
        ),
        (
            "never",
            {"cycle": {"keys": ["ctrl+t"], "until": "Switched variant to {effort}", "max": 2}},
            {"effort": "high"},
            ["nada"],
            [{"ok": True}],
        ),
        (
            "keys-fail",
            {"cycle": {"keys": ["ctrl+t"], "until": "x", "max": 2}, "message": "falló"},
            {},
            ["nada"],
            [{"ok": False}],
        ),
        ("malformed-no-keys", {"cycle": {"until": "x"}}, {}, ["x"], [{"ok": True}]),
        ("malformed-until", {"cycle": {"keys": ["enter"], "until": ""}}, {}, ["x"], []),
        (
            "max-string",
            {"cycle": {"keys": ["enter"], "until": "x", "max": "3"}},
            {},
            ["x"],
            [{"ok": True}],
        ),
        (
            "max-zero",
            {"cycle": {"keys": ["enter"], "until": "x", "max": 0}},
            {},
            ["x"],
            [{"ok": True}],
        ),
        (
            "max-bool",
            {"cycle": {"keys": ["enter"], "until": "x", "max": True}},
            {},
            ["x"],
            [{"ok": True}],
        ),
        (
            "message-filled",
            {"cycle": {"keys": ["enter"], "until": "x", "max": 1}, "message": "sin {effort}"},
            {"effort": "high"},
            ["nada"],
            [{"ok": True}],
        ),
        (
            "escaped-until",
            {"cycle": {"keys": ["enter"], "until": "{model_search}", "max": 1}},
            {"model": "a/b.c"},
            ["a b.c"],
            [{"ok": True}],
        ),
    ]
    for index, (name, step, values, screens, keys) in enumerate(cycle_cases, start=1):
        case = f"agent_driver/session/cycle/{index:03d}-{name}"
        with _isolated(home, case):
            clock = _Clock()
            session = _session(_config(), screens, keys=keys)
            with _clock(clock):
                _session_capture(
                    out,
                    home,
                    case,
                    "AgentSession._cycle",
                    {
                        "step": step,
                        "values": values,
                        "screens": [_scripted(item) for item in screens],
                        "keys": keys,
                    },
                    lambda: session._cycle("t1", step, values),
                    session,
                    clock,
                )

    run_cases = [
        ("send", {"send": "hola"}, {}, ["hola"], [{"ok": True}], [{"ok": True}]),
        (
            "send-expect",
            {"send": "hola", "expect": "ok"},
            {},
            ["hola", "ok"],
            [{"ok": True}],
            [{"ok": True}],
        ),
        (
            "send-expect-fail",
            {"send": "hola", "expect": "ok", "message": "no apareció"},
            {},
            ["hola", "nada"],
            [{"ok": True}],
            [{"ok": True}],
        ),
        ("send-fails", {"send": "hola"}, {}, ["hola"], [{"ok": False}], [{"ok": True}]),
        ("keys", {"keys": ["enter", "esc"]}, {}, ["x"], [{"ok": True}], [{"ok": True}]),
        (
            "keys-invalid",
            {"keys": ["meta+a"]},
            {},
            ["x"],
            [{"ok": True}],
            [{"ok": True}],
        ),
        (
            "keys-fails",
            {"keys": ["enter"]},
            {},
            ["x"],
            [{"ok": True}],
            [{"ok": False}],
        ),
        (
            "cycle",
            {"cycle": {"keys": ["enter"], "until": "x", "max": 1}},
            {},
            ["x"],
            [{"ok": True}],
            [{"ok": True}],
        ),
    ]
    for index, (name, step, values, screens, sends, keys) in enumerate(run_cases, start=1):
        case = f"agent_driver/session/run_step/{index:03d}-{name}"
        with _isolated(home, case):
            clock = _Clock()
            session = _session(_config(), screens, sends=sends, keys=keys)
            with _clock(clock):
                _session_capture(
                    out,
                    home,
                    case,
                    "AgentSession._run_step",
                    {
                        "step": step,
                        "values": values,
                        "screens": [_scripted(item) for item in screens],
                        "sends": sends,
                        "keys": keys,
                    },
                    lambda: session._run_step("t1", step, values),
                    session,
                    clock,
                )

    apply_profiles = {
        "dos": {
            "session_commands": {
                "model": [{"send": "/model {model}", "expect": "Switched"}],
                "effort": [{"send": "/effort {effort}", "expect": "Variant"}],
            }
        },
        "soft": {
            "session_commands": {
                "model": [{"send": "/model {model}", "expect": "Switched", "soft": True}],
                "effort": [{"send": "/effort {effort}"}],
            }
        },
        "optional": {
            "session_commands": {
                "model": [{"send": "/model {model}", "expect": "Switched", "optional": True}],
                "effort": [{"send": "/effort {effort}"}],
            }
        },
        "fatal": {
            "session_commands": {
                "model": [{"send": "/model {model}", "expect": "Switched"}],
                "effort": [{"send": "/effort {effort}", "expect": "Variant"}],
            }
        },
        "nada": {},
    }
    apply_cases = [
        (
            "dos",
            {"model": "m", "effort": "high"},
            ["Switched", "Variant"],
            [{"ok": True}],
            [{"ok": True}],
        ),
        ("dos", {"model": "", "effort": ""}, [], [{"ok": True}], [{"ok": True}]),
        ("dos", {"effort": "high"}, ["nada", "Variant"], [{"ok": True}], [{"ok": True}]),
        (
            "soft",
            {"model": "m", "effort": "high"},
            ["nada", "nada"],
            [{"ok": True}],
            [{"ok": True}],
        ),
        (
            "optional",
            {"model": "m", "effort": "high"},
            ["nada", "nada"],
            [{"ok": True}],
            [{"ok": True}],
        ),
        ("fatal", {"model": "m", "effort": "high"}, ["nada"], [{"ok": True}], [{"ok": True}]),
        ("nada", {"model": "m", "effort": "high"}, [], [{"ok": True}], [{"ok": True}]),
    ]
    for index, (key, values, screens, sends, keys) in enumerate(apply_cases, start=1):
        case = f"agent_driver/session/apply/{index:03d}-{key}"
        with _isolated(home, case):
            clock = _Clock()
            cfg = _config(agent_profiles=apply_profiles)
            session = _session(cfg, screens, sends=sends, keys=keys)
            with _clock(clock):
                _session_capture(
                    out,
                    home,
                    case,
                    "AgentSession._apply_in_session",
                    {
                        "profile": key,
                        "values": values,
                        "screens": [_scripted(item) for item in screens],
                        "sends": sends,
                        "keys": keys,
                        "profiles": apply_profiles,
                    },
                    lambda: session._apply_in_session("t1", key, values),
                    session,
                    clock,
                )


def _flow_cases(out: FixtureFile, home: Path) -> None:
    dialog_cases = [
        ("trust", "Do you trust the files in this folder?"),
        ("trust-folder", "trust this folder"),
        ("trust-directory", "Trust this directory"),
        ("yes-trust", "Yes, I trust"),
        ("safety", "Quick safety check"),
        ("none", "PS C:\\> hola"),
        ("empty", ""),
    ]
    for index, (name, screen) in enumerate(dialog_cases, start=1):
        case = f"agent_driver/session/blocking_dialog/{index:03d}-{name}"
        with _isolated(home, case):
            clock = _Clock()
            session = _session(_config(), [screen])
            with _clock(clock):
                _session_capture(
                    out,
                    home,
                    case,
                    "AgentSession.blocking_dialog",
                    {"screen": screen},
                    lambda: session.blocking_dialog("t1"),
                    session,
                    clock,
                )

    unsent_cases = [
        ("pasted", "hola\n[Pasted ~3 lines]", "algo"),
        ("pasted-old", "\n".join(["[Pasted ~3 lines]"] + [f"linea {i}" for i in range(9)]), "algo"),
        ("tail-match", "› haz algo importante", "haz algo importante"),
        ("tail-long", "› prefijo " + "x" * 40, "x" * 40),
        ("tail-short", "› hola", "hola"),
        ("no-glyph", "haz algo importante", "haz algo importante"),
        ("glyph-earlier", "› haz algo importante\notra linea", "haz algo importante"),
        ("task-above", "haz algo importante\n› otra", "haz algo importante"),
        ("spaces", "›   haz   algo   importante  ", "haz algo importante"),
        ("chevron", "> haz algo importante", "haz algo importante"),
        ("none-task", "› algo", None),
        ("empty", "", ""),
    ]
    for index, (name, screen, task) in enumerate(unsent_cases, start=1):
        case = f"agent_driver/session/unsent/{index:03d}-{name}"
        with _isolated(home, case):
            from ezagent.voice.agent_session import AgentSession

            _capture(
                out,
                case,
                "AgentSession._unsent_in_input",
                {"screen": screen, "task": task},
                lambda: {"unsent": AgentSession._unsent_in_input(screen, task)},
                home,
            )

    confirm_cases = [
        ("working", ["hola"], [{"agent": "codex", "state": "working"}], None),
        ("clean", ["hola"], [{"agent": "codex", "state": "done"}], None),
        ("empty", [""], [{"agent": "codex", "state": "done"}], None),
        ("read-error", [OSError("ilegible")], [{"agent": "codex", "state": "done"}], None),
        ("get-error-clean", ["hola"], [OSError("sin terminal")], None),
        (
            "unsent-then-working",
            ["› haz algo importante"],
            [{"agent": "codex", "state": "done"}, {"agent": "codex", "state": "working"}],
            None,
        ),
        ("unsent-forever", ["› haz algo importante"], [{"agent": "codex", "state": "done"}], None),
    ]
    for index, (name, screens, agents, _) in enumerate(confirm_cases, start=1):
        case = f"agent_driver/session/confirm/{index:03d}-{name}"
        with _isolated(home, case):
            clock = _Clock()
            session = _session(_config(), screens, agents)
            with _clock(clock):
                _session_capture(
                    out,
                    home,
                    case,
                    "AgentSession.confirm_submitted",
                    {
                        "screens": [_scripted(item) for item in screens],
                        "agents": [_scripted(item) for item in agents],
                    },
                    lambda: session.confirm_submitted("t1", "haz algo importante"),
                    session,
                    clock,
                )

    send_task_cases = [
        ("confirmed", {"ok": True}, True),
        ("unconfirmed", {"ok": True}, False),
        ("error", {"ok": False, "error": "no"}),
    ]
    for index, item in enumerate(send_task_cases, start=1):
        name, result, *rest = item
        confirmed = rest[0] if rest else None
        case = f"agent_driver/session/send_task/{index:03d}-{name}"
        with _isolated(home, case):
            clock = _Clock()
            session = _session(_config(), sends=[result])
            if confirmed is not None:
                session.confirm_submitted = lambda address, task: confirmed  # type: ignore[method-assign]
            else:
                session.confirm_submitted = lambda address, task: False  # type: ignore[method-assign]

            def action() -> dict[str, Any]:
                return session.send_task("t1", "tarea")

            _session_capture(
                out,
                home,
                case,
                "AgentSession.send_task",
                {"send": result, "confirmed": confirmed},
                action,
                session,
                clock,
            )

    launch_cases = [
        (
            "not-ready",
            {"ready": False, "blocking": False, "apply": ([], [], ""), "send": {"ok": True}},
            {},
        ),
        (
            "blocking",
            {"ready": True, "blocking": True, "apply": ([], [], ""), "send": {"ok": True}},
            {},
        ),
        (
            "fatal",
            {
                "ready": True,
                "blocking": False,
                "apply": (["modelo m"], ["aviso"], "modelo: no"),
                "send": {"ok": True},
            },
            {"pending": {"model": "m"}, "warnings": ["previo"]},
        ),
        (
            "no-task",
            {
                "ready": True,
                "blocking": False,
                "apply": (["modelo m"], [], ""),
                "send": {"ok": True},
            },
            {"pending": {"model": "m"}, "warnings": ["previo"]},
        ),
        (
            "task-ok",
            {
                "ready": True,
                "blocking": False,
                "apply": (["modelo m"], [], ""),
                "send": {"ok": True},
            },
            {"pending": {"model": "m"}, "warnings": []},
        ),
        (
            "task-warning",
            {
                "ready": True,
                "blocking": False,
                "apply": ([], [], ""),
                "send": {"ok": True, "warning": "sin confirmar"},
            },
            {"warnings": ["previo"]},
        ),
        (
            "task-error",
            {
                "ready": True,
                "blocking": False,
                "apply": ([], [], ""),
                "send": {"ok": False, "error": "no pude"},
            },
            {"warnings": []},
        ),
    ]
    from ezagent.voice.agent_session import AgentLaunch

    for index, (name, spec, launch_spec) in enumerate(launch_cases, start=1):
        case = f"agent_driver/session/finish_launch/{index:03d}-{name}"
        with _isolated(home, case):
            clock = _Clock()
            session = _session(_config())
            session.wait_ready = lambda address: spec["ready"]  # type: ignore[method-assign]
            session.blocking_dialog = lambda address: spec["blocking"]  # type: ignore[method-assign]

            def apply(address: str, key: str, values: dict[str, str]) -> Any:
                session.calls.append(
                    {"kind": "apply", "address": address, "key": key, "values": dict(values)}
                )
                return spec["apply"]

            def send(address: str, task: str) -> dict[str, Any]:
                session.calls.append({"kind": "send_task", "address": address, "task": task})
                return spec["send"]

            session._apply_in_session = apply  # type: ignore[method-assign]
            session.send_task = send  # type: ignore[method-assign]
            launch = AgentLaunch(
                argv=["x"],
                warnings=list(launch_spec.get("warnings", [])),
                pending=dict(launch_spec.get("pending", {})),
            )
            task = "" if name == "no-task" else "haz algo"
            _session_capture(
                out,
                home,
                case,
                "AgentSession.finish_launch",
                {"launch": launch_spec, "task": task, "outcome": spec},
                lambda: session.finish_launch("t1", "claude", launch, task),
                session,
                clock,
            )

    configure_cases = [
        ("no-values", {}, {"agent": "codex", "state": "done"}, ([], [], "")),
        ("unknown", {"model": "m"}, {"agent": "desconocido"}, ([], [], "")),
        ("no-agent", {"model": "m"}, {}, ([], [], "")),
        ("bad-model", {"model": "no vale"}, {"agent": "codex"}, ([], [], "")),
        ("bad-effort", {"effort": "turbo"}, {"agent": "codex"}, ([], [], "")),
        ("fatal", {"model": "m"}, {"agent": "codex"}, ([], [], "modelo: no")),
        ("nothing", {"model": "m"}, {"agent": "codex"}, ([], ["aviso"], "")),
        (
            "ok",
            {"model": "m", "effort": "high"},
            {"agent": "codex"},
            (["modelo m", "esfuerzo high"], [], ""),
        ),
    ]
    for index, (name, values, agent_info, apply_result) in enumerate(configure_cases, start=1):
        case = f"agent_driver/session/configure/{index:03d}-{name}"
        with _isolated(home, case):
            clock = _Clock()
            session = _driver(_config())
            session.agents = [agent_info]
            session._apply_in_session = lambda address, key, vals, result=apply_result: result  # type: ignore[method-assign]
            _session_capture(
                out,
                home,
                case,
                "AgentSession.configure_agent",
                {"values": values, "agent": agent_info, "apply": list(apply_result)},
                lambda: session.configure_agent("t1", values.get("model"), values.get("effort")),
                session,
                clock,
            )


# --------------------------------------------------------------- driver
CODEX_CACHE = {
    "models": [
        {
            "slug": "gpt-5.1",
            "visibility": "list",
            "supported_reasoning_levels": [
                {"effort": "low"},
                {"effort": "medium"},
                {"effort": "high"},
            ],
        },
        {
            "slug": "gpt-5.1-codex",
            "visibility": "list",
            "supported_reasoning_levels": [{"effort": "high"}, {"effort": "max"}],
        },
        {
            "slug": "oculto",
            "visibility": "hidden",
            "supported_reasoning_levels": [{"effort": "high"}],
        },
    ]
}

MCODE_CONFIG = """\
provider:
  minimax:
    models:
      MiniMax-M3.1-Flash: {}
      abab6.5s: {}
"""


def _driver_resolve_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.voice.agent_driver import AgentDriver

    names = [
        "claude",
        "Claude Code",
        "cloud code",
        "clod code",
        "open code",
        "open-code",
        "opencoder",
        "mcode",
        "m code",
        "minimax code",
        "Códex",
        "QUEN",
        "pi agent",
        "desconocido",
        "",
        "   ",
        "minimax",
    ]
    for index, name in enumerate(names, start=1):
        case = f"agent_driver/driver/resolve_agent/{index:03d}"
        with _isolated(home, case):
            session = _driver(_config())
            _capture(
                out,
                case,
                "AgentDriver.resolve_agent",
                {"name": name},
                lambda value=name: {"key": session.resolve_agent(value)},
                home,
            )

    text_cases = [
        "x",
        "   ",
        "",
        [],
        ["a", "b"],
        [1, None, "x"],
        None,
        5,
        ("a", "b"),
        {"a": 1},
    ]
    for index, value in enumerate(text_cases, start=1):
        case = f"agent_driver/driver/text_list/{index:03d}"
        with _isolated(home, case):
            _capture(
                out,
                case,
                "AgentDriver._text_list",
                {"value": value},
                lambda item=value: {"list": AgentDriver._text_list(item)},
                home,
            )

    profiles = {
        "claude": {"efforts": ["low", "high"]},
        "vacio": {"efforts": []},
        "texto": {"efforts": "high"},
        "mixto": {"efforts": ["High", "low", 5]},
    }
    check_cases = [
        ("claude", "modelo", "sonnet"),
        ("claude", "modelo", ""),
        ("claude", "modelo", "-sonnet"),
        ("claude", "modelo", "son net"),
        ("claude", "modelo", "a/b#c"),
        ("claude", "modelo", "x" * 81),
        ("claude", "modelo", "x" * 82),
        ("claude", "modelo", "ñandú"),
        ("claude", "modelo", "a\x00b"),
        ("claude", "esfuerzo", "high"),
        ("claude", "esfuerzo", "HIGH"),
        ("claude", "esfuerzo", " medium "),
        ("claude", "esfuerzo", "max"),
        ("vacio", "esfuerzo", "lo que sea"),
        ("texto", "esfuerzo", "high"),
        ("texto", "esfuerzo", "low"),
        ("mixto", "esfuerzo", "high"),
        ("mixto", "esfuerzo", "low"),
        ("desconocido", "esfuerzo", "high"),
        ("claude", "esfuerzo", "a" * 81),
    ]
    for index, (key, kind, value) in enumerate(check_cases, start=1):
        case = f"agent_driver/driver/check_value/{index:03d}"
        with _isolated(home, case):
            session = _driver(_config(agent_profiles=profiles))
            _capture(
                out,
                case,
                "AgentDriver._check_value",
                {"key": key, "kind": kind, "value": value, "profiles": profiles},
                lambda k=key, t=kind, v=value: {"error": session._check_value(k, t, v)},
                home,
            )

    swarm_profiles = {
        "custom": {"swarm_args": "uno"},
        "lista": {"swarm_args": ["uno", "", "dos"]},
        "vacia": {"swarm_args": []},
        "no-lista": {"swarm_args": 5},
    }
    swarm_cases = [
        ("claude", None),
        ("codex", None),
        ("opencode", None),
        ("minimax", None),
        ("desconocido", None),
        ("custom", swarm_profiles),
        ("lista", swarm_profiles),
        ("vacia", swarm_profiles),
        ("no-lista", swarm_profiles),
    ]
    for index, (key, extra) in enumerate(swarm_cases, start=1):
        case = f"agent_driver/driver/swarm_args/{index:03d}-{key}"
        with _isolated(home, case):
            cfg = _config(agent_profiles=extra) if extra else _config()
            session = _driver(cfg)
            _capture(
                out,
                case,
                "AgentDriver.swarm_args",
                {"agent": key, "profiles": extra},
                lambda value=key: {"args": session.swarm_args(value)},
                home,
            )


CUSTOM_PROFILES = {
    "env": {
        "label": "Env",
        "model_prefix": "p/",
        "env": {"A": "{model_ref}{variant}", "B": "fijo"},
        "env_args": ["--x"],
        "session_commands": {},
    },
    "variant": {
        "label": "Var",
        "model_args": ["--model", "{model}{variant}"],
        "effort_args": [],
        "session_commands": {},
    },
    "pending": {
        "label": "Pend",
        "model_args": [],
        "effort_args": [],
        "session_commands": {"model": ["/model {model}"], "effort": ["/effort {effort}"]},
    },
    "warn": {"label": "Warn", "model_args": [], "effort_args": [], "session_commands": {}},
    "cmd": {
        "label": "Cmd",
        "command": ["mi-cli", "--x"],
        "model_args": ["--m", "{model}"],
        "session_commands": {},
    },
    "defaults": {
        "label": "Def",
        "default_model": "dm",
        "default_effort": "low",
        "model_args": ["--model", "{model}"],
        "session_commands": {},
    },
    "bad_efforts": {
        "label": "Bad",
        "efforts": ["low"],
        "effort_args": ["--e", "{effort}"],
        "session_commands": {},
    },
    "model_efforts": {
        "label": "ME",
        "model_efforts": {"m": ["low", "max"]},
        "model_args": ["--model", "{model}"],
        "effort_args": ["--e", "{effort}"],
        "session_commands": {},
    },
    "prelaunch_mcode": {
        "label": "M",
        "prelaunch": "mcode_effort",
        "model_args": ["--model", "{model}"],
        "effort_args": [],
        "session_commands": {},
    },
    "prelaunch_opencode": {
        "label": "O",
        "prelaunch": "opencode_state",
        "model_prefix": "p/",
        "env": {"X": "{model_ref}{variant}"},
        "session_commands": {},
    },
}
CUSTOM_AGENTS = {
    "claude": ["claude"],
    "env": ["env-cli"],
    "variant": ["variant-cli"],
    "pending": ["pending-cli"],
    "warn": ["warn-cli"],
    "cmd": ["cmd-cli"],
    "defaults": ["defaults-cli"],
    "bad_efforts": ["bad-cli"],
    "model_efforts": ["me-cli"],
    "prelaunch_mcode": ["mcode"],
    "prelaunch_opencode": ["opencode"],
}


def _launch_cases(out: FixtureFile, home: Path) -> None:
    launch_cases = [
        ("claude-basic", "claude", None, None, None, None, None, None),
        ("claude-model", "claude", "sonnet", None, None, None, None, None),
        ("claude-effort", "claude", None, "high", None, None, None, None),
        ("claude-both", "claude", "sonnet", "high", None, None, None, None),
        ("claude-bad-model", "claude", "no vale", None, None, None, None, None),
        ("claude-bad-effort", "claude", None, "turbo", None, None, None, None),
        ("claude-effort-case", "claude", None, "HIGH", None, None, None, None),
        ("claude-long", "claude", "x" * 82, None, None, None, None, None),
        (
            "opencode-both",
            "opencode",
            "deepseek-v4.1-flash",
            "high",
            None,
            None,
            None,
            None,
        ),
        (
            "opencode-model",
            "opencode",
            "glm-5.3",
            None,
            None,
            None,
            None,
            None,
        ),
        (
            "opencode-slash",
            "opencode",
            "opencode-go/glm-5.3",
            "high",
            None,
            None,
            None,
            None,
        ),
        (
            "opencode-effort-no-model",
            "opencode",
            None,
            "high",
            None,
            None,
            None,
            None,
        ),
        (
            "opencode-effort-config",
            "opencode",
            None,
            "high",
            None,
            None,
            {"opencode-config/opencode.jsonc": '{"model": "p/m"}'},
            None,
        ),
        (
            "opencode-bad-effort",
            "opencode",
            "deepseek-v4.1-flash",
            "ultra",
            None,
            None,
            None,
            None,
        ),
        (
            "mcode-discovery",
            "mcode",
            "minimax/MiniMax-M3.1-Flash",
            "high",
            None,
            None,
            {"minimax/config.yaml": MCODE_CONFIG},
            None,
        ),
        (
            "mcode-case",
            "mcode",
            "minimax/minimax-m3.1-flash",
            "low",
            None,
            None,
            {"minimax/config.yaml": MCODE_CONFIG},
            None,
        ),
        (
            "mcode-unknown",
            "mcode",
            "minimax/nada",
            None,
            None,
            None,
            {"minimax/config.yaml": MCODE_CONFIG},
            None,
        ),
        ("mcode-no-config", "mcode", "minimax/x", "high", None, None, None, None),
        (
            "codex-model",
            "codex",
            "gpt-5.1",
            "high",
            None,
            None,
            {"codex/models_cache.json": json.dumps(CODEX_CACHE)},
            None,
        ),
        (
            "codex-case",
            "codex",
            "GPT-5.1",
            "HIGH",
            None,
            None,
            {"codex/models_cache.json": json.dumps(CODEX_CACHE)},
            None,
        ),
        (
            "codex-unknown",
            "codex",
            "gpt-9",
            None,
            None,
            None,
            {"codex/models_cache.json": json.dumps(CODEX_CACHE)},
            None,
        ),
        (
            "codex-bad-effort",
            "codex",
            "gpt-5.1",
            "ultra",
            None,
            None,
            {"codex/models_cache.json": json.dumps(CODEX_CACHE)},
            None,
        ),
        (
            "codex-effort-no-model",
            "codex",
            None,
            "max",
            None,
            None,
            {"codex/models_cache.json": json.dumps(CODEX_CACHE)},
            None,
        ),
        ("codex-no-cache", "codex", "cualquiera", "raro", None, None, None, None),
        ("pi", "pi", "proveedor/modelo", "high", None, None, None, None),
        ("qwen", "qwen", "qwen3-coder-plus", None, None, None, None, None),
        (
            "minimax-alias",
            "minimax",
            "minimax/MiniMax-M3.1-Flash",
            "high",
            None,
            None,
            {"minimax/config.yaml": MCODE_CONFIG},
            None,
        ),
        ("unknown", "desconocido", None, None, None, None, None, None),
        ("no-profile", "claude", "m", None, {}, {"claude": ["claude"]}, None, None),
        ("defaults", "defaults", None, None, CUSTOM_PROFILES, CUSTOM_AGENTS, None, None),
        (
            "defaults-override",
            "defaults",
            "otro",
            "max",
            CUSTOM_PROFILES,
            CUSTOM_AGENTS,
            None,
            None,
        ),
        ("env", "env", "modelo", "high", CUSTOM_PROFILES, CUSTOM_AGENTS, None, None),
        ("env-no-effort", "env", "modelo", None, CUSTOM_PROFILES, CUSTOM_AGENTS, None, None),
        ("variant", "variant", "modelo", "high", CUSTOM_PROFILES, CUSTOM_AGENTS, None, None),
        (
            "variant-no-effort",
            "variant",
            "modelo",
            None,
            CUSTOM_PROFILES,
            CUSTOM_AGENTS,
            None,
            None,
        ),
        ("pending", "pending", "modelo", "high", CUSTOM_PROFILES, CUSTOM_AGENTS, None, None),
        ("warn", "warn", "modelo", "high", CUSTOM_PROFILES, CUSTOM_AGENTS, None, None),
        ("cmd", "cmd", "modelo", None, CUSTOM_PROFILES, CUSTOM_AGENTS, None, None),
        ("bad-efforts", "bad_efforts", None, "high", CUSTOM_PROFILES, CUSTOM_AGENTS, None, None),
        (
            "model-efforts-ok",
            "model_efforts",
            "m",
            "max",
            CUSTOM_PROFILES,
            CUSTOM_AGENTS,
            None,
            None,
        ),
        (
            "model-efforts-bad",
            "model_efforts",
            "m",
            "high",
            CUSTOM_PROFILES,
            CUSTOM_AGENTS,
            None,
            None,
        ),
        (
            "prelaunch-mcode",
            "prelaunch_mcode",
            "modelo",
            "max",
            CUSTOM_PROFILES,
            CUSTOM_AGENTS,
            {"minimax/config.yaml": "model: x\n"},
            None,
        ),
        (
            "prelaunch-opencode",
            "prelaunch_opencode",
            "modelo",
            "max",
            CUSTOM_PROFILES,
            CUSTOM_AGENTS,
            None,
            None,
        ),
    ]

    for index, (name, key, model, effort, profiles, agents, files, overrides) in enumerate(
        launch_cases, start=1
    ):
        case = f"agent_driver/driver/agent_launch/{index:03d}-{name}"
        with _isolated(home, case) as root:
            for relative, text in (files or {}).items():
                _put(root, relative, text)
            cfg_values: dict[str, Any] = {}
            if profiles is not None:
                cfg_values["agent_profiles"] = profiles
            if agents is not None:
                cfg_values["agents"] = agents
            cfg_values.update(overrides or {})
            cfg = _config(**cfg_values)
            session = _driver(cfg)

            def action() -> dict[str, Any]:
                launch = session.agent_launch(key, model, effort)
                return {
                    "argv": launch.argv,
                    "warnings": launch.warnings,
                    "pending": launch.pending,
                    "env": launch.env,
                    "error": launch.error,
                    "files": _snapshot(root),
                }

            _capture(
                out,
                case,
                "AgentDriver.agent_launch",
                {
                    "agent": key,
                    "model": model,
                    "effort": effort,
                    "profiles": profiles,
                    "agents": agents,
                    "overrides": overrides,
                    "files": files or {},
                },
                action,
                home,
            )

    argv_cases = [
        ("ok", "claude", "sonnet", "high"),
        ("unknown", "nada", None, None),
        ("bad", "claude", "no vale", None),
    ]
    for index, (name, key, model, effort) in enumerate(argv_cases, start=1):
        case = f"agent_driver/driver/agent_argv/{index:03d}-{name}"
        with _isolated(home, case):
            session = _driver(_config())
            _capture(
                out,
                case,
                "AgentDriver.agent_argv",
                {"agent": key, "model": model, "effort": effort},
                lambda k=key, m=model, e=effort: {"argv": session.agent_argv(k, m, e)},
                home,
            )


def _codex_resolve_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.voice.agent_driver import AgentDriver

    cases = [
        ("model-effort", {"model": "gpt-5.1", "effort": "high"}),
        ("model-case", {"model": "GPT-5.1", "effort": "MEDIUM"}),
        ("unknown-model", {"model": "gpt-9", "effort": ""}),
        ("bad-effort", {"model": "gpt-5.1", "effort": "ultra"}),
        ("no-model-effort", {"model": "", "effort": "max"}),
        ("no-model-bad-effort", {"model": "", "effort": "ultra"}),
        ("empty", {"model": "", "effort": ""}),
        ("codex-model", {"model": "gpt-5.1-codex", "effort": "low"}),
    ]
    for index, (name, wanted) in enumerate(cases, start=1):
        case = f"agent_driver/driver/resolve_codex/{index:03d}-{name}"
        with _isolated(home, case) as root:
            _put(root, "codex/models_cache.json", json.dumps(CODEX_CACHE))
            value = dict(wanted)
            _capture(
                out,
                case,
                "AgentDriver._resolve_codex",
                {"wanted": value, "cache": json.loads(json.dumps(CODEX_CACHE))},
                lambda item=value: {
                    "error": AgentDriver._resolve_codex(item),
                    "model": item["model"],
                },
                home,
            )
    with _isolated(home, "agent_driver_driver_resolve_codex_009_nocache"):
        value = {"model": "gpt-5.1", "effort": "high"}
        _capture(
            out,
            "agent_driver/driver/resolve_codex/009-nocache",
            "AgentDriver._resolve_codex",
            {"wanted": value, "cache": None},
            lambda: {"error": AgentDriver._resolve_codex(value), "model": value["model"]},
            home,
        )


def _catalog_cases(out: FixtureFile, home: Path) -> None:
    def fake_which(mapping: dict[str, str | None]) -> contextlib.AbstractContextManager[Any]:
        original = shutil.which

        @contextlib.contextmanager
        def patch() -> Iterator[None]:
            shutil.which = lambda name: mapping.get(str(name))  # type: ignore[assignment]
            try:
                yield
            finally:
                shutil.which = original  # type: ignore[assignment]

        return patch()

    catalog_cases = [
        (
            "default",
            {},
            {
                "claude": "C:/bin/claude.exe",
                "opencode": "C:/bin/opencode.exe",
                "codex": "C:/bin/codex.exe",
            },
            {},
            {},
        ),
        (
            "custom",
            {
                "agent_profiles": {
                    "claude": {
                        "label": "Claude Code",
                        "aliases": ["cloud code"],
                        "models": ["sonnet"],
                        "efforts": ["low", "high"],
                        "model_args": ["--model", "{model}"],
                        "session_commands": {"effort": ["/effort {effort}"]},
                        "notes": "nota",
                    },
                    "x": {"label": "", "models": ["m"], "notes": ""},
                },
                "agents": {"claude": ["claude"], "x": ["x-cli", "--y"]},
            },
            {"claude": "C:/bin/claude.exe"},
            {},
            {},
        ),
        (
            "discover",
            {
                "agent_profiles": {
                    "mcode": {"label": "M", "discover": "mcode"},
                    "codex": {"label": "C", "discover": "codex"},
                },
                "agents": {"mcode": ["mcode"], "codex": ["codex"]},
            },
            {},
            {
                "minimax/config.yaml": MCODE_CONFIG,
                "codex/models_cache.json": json.dumps(CODEX_CACHE),
            },
            {},
        ),
    ]
    for index, (name, overrides, mapping, files, _) in enumerate(catalog_cases, start=1):
        case = f"agent_driver/driver/agents_catalog/{index:03d}-{name}"
        with _isolated(home, case) as root:
            for relative, text in files.items():
                _put(root, relative, text)
            cfg = _config(**overrides)
            session = _driver(cfg)
            with fake_which(mapping):
                _capture(
                    out,
                    case,
                    "AgentDriver.agents_catalog",
                    {"overrides": overrides, "which": mapping, "files": files},
                    lambda: {"catalog": session.agents_catalog()},
                    home,
                )

    text_cases = [
        ("default", {}, {"claude": "C:/bin/claude.exe", "codex": "C:/bin/codex.exe"}),
        (
            "custom",
            {
                "agent_profiles": {
                    "claude": {
                        "label": "Claude Code",
                        "aliases": ["cloud code", "clod code"],
                        "models": ["sonnet", "opus"],
                        "efforts": ["low", "high"],
                        "model_args": ["--model", "{model}"],
                        "session_commands": {},
                        "notes": "nota del perfil",
                    },
                    "x": {"label": "", "models": [], "notes": ""},
                },
                "agents": {"claude": ["claude"], "x": ["x-cli"]},
            },
            {},
        ),
    ]
    for index, (name, overrides, mapping) in enumerate(text_cases, start=1):
        case = f"agent_driver/driver/catalog_text/{index:03d}-{name}"
        with _isolated(home, case):
            session = _driver(_config(**overrides))
            with fake_which(mapping):
                _capture(
                    out,
                    case,
                    "AgentDriver.catalog_text",
                    {"overrides": overrides, "which": mapping},
                    lambda: {"text": session.catalog_text()},
                    home,
                )


# ----------------------------------------------------------------- watcher
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
        calls.append(
            {
                "argv": [str(part) for part in command],
                "timeout": kwargs.get("timeout"),
                "encoding": kwargs.get("encoding"),
                "errors": kwargs.get("errors"),
                "capture_output": kwargs.get("capture_output"),
                "text": kwargs.get("text"),
                "env_none": kwargs.get("env") is None,
                "shell": kwargs.get("shell"),
            }
        )
        if reply["exc"] == "timeout":
            raise subprocess.TimeoutExpired(command, kwargs.get("timeout") or 0)
        if reply["exc"] == "oserror":
            raise OSError("no se pudo ejecutar")
        return subprocess.CompletedProcess(
            command, reply["rc"], stdout=reply["stdout"], stderr=reply["stderr"]
        )

    subprocess.run = fake  # type: ignore[assignment]
    try:
        yield calls
    finally:
        subprocess.run = original  # type: ignore[assignment]


class _StopAfter:
    def __init__(self, polls: int) -> None:
        self.polls = polls
        self.delays: list[float] = []

    def is_set(self) -> bool:
        return len(self.delays) >= self.polls

    def wait(self, delay: float) -> bool:
        self.delays.append(delay)
        return len(self.delays) >= self.polls


def _terminal(address: str, state: str, app: str = "codex") -> dict[str, Any]:
    return {"address": address, "status": {"state": state, "app": app}}


def _drain(watcher: Any) -> list[Any]:
    found: list[Any] = []
    while True:
        try:
            event = watcher._events.get_nowait()
        except Exception:  # noqa: BLE001 - cola vacía
            return found
        if event is not None:
            found.append(event)


def _watcher_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.voice.watcher import Watcher

    def make(
        events: list[Any] | None = None,
        health: list[bool] | None = None,
        announce: Callable[[str], bool] | None = None,
        targets: Callable[[], bool] | None = None,
    ) -> Any:
        def on_event(*args: Any) -> None:
            if events is not None:
                events.append(args)

        def on_health(value: bool) -> None:
            if health is not None:
                health.append(value)

        return Watcher(
            _config(),
            on_event=on_event,
            should_announce=announce or (lambda address: True),
            on_health=on_health,
            has_targets=targets,
        )

    compare_cases = [
        ("prime", [[_terminal("t1", "working")]]),
        ("working-done", [[_terminal("t1", "working")], [_terminal("t1", "done")]]),
        ("idle-done", [[_terminal("t1", "idle")], [_terminal("t1", "done")]]),
        ("working-idle", [[_terminal("t1", "working")], [_terminal("t1", "idle")]]),
        ("blocked", [[_terminal("t1", "idle")], [_terminal("t1", "blocked")]]),
        ("error", [[_terminal("t1", "working")], [_terminal("t1", "error")]]),
        (
            "new-terminal",
            [[_terminal("t1", "working")], [_terminal("t1", "working"), _terminal("t2", "idle")]],
        ),
        ("vanished-working", [[_terminal("t1", "working")], []]),
        ("vanished-idle", [[_terminal("t1", "idle")], []]),
        ("vanished-done", [[_terminal("t1", "done")], []]),
        (
            "string-status",
            [
                [{"address": "t1", "status": "working", "agent": "opencode"}],
                [{"address": "t1", "status": "done", "agent": "opencode"}],
            ],
        ),
        (
            "missing-state",
            [[{"address": "t1", "status": {}}], [{"address": "t1", "status": {"state": "done"}}]],
        ),
        (
            "missing-app",
            [
                [{"address": "t1", "status": {"state": "working"}}],
                [{"address": "t1", "status": {"state": "done"}}],
            ],
        ),
        ("missing-address", [[{"status": "working"}], []]),
        ("no-status", [[{"address": "t1"}], []]),
        (
            "int-address",
            [[{"address": 5, "status": "working"}], [{"address": 5, "status": "done"}]],
        ),
        ("duplicate", [[_terminal("t1", "working"), _terminal("t1", "done")]]),
    ]
    for index, (name, snapshots) in enumerate(compare_cases, start=1):
        case = f"agent_driver/watcher/compare/{index:03d}-{name}"
        with _isolated(home, case):
            events: list[Any] = []
            watcher = make(events)
            states = []
            for snapshot in snapshots:
                watcher._compare(snapshot)
                states.append(
                    {
                        "live": dict(watcher._live),
                        "prev": {key: list(value) for key, value in watcher._prev.items()},
                    }
                )
            out.add(
                case,
                "Watcher._compare",
                {"snapshots": snapshots},
                _mark({"events": _drain(watcher), "states": states}, home),
            )

    # should_announce(False) para el terminal que desaparece.
    case = "agent_driver/watcher/compare/018-no-announce"
    with _isolated(home, case):
        events = []
        watcher = make(events, announce=lambda address: False)
        watcher._compare([_terminal("t1", "working")])
        watcher._compare([])
        out.add(
            case,
            "Watcher._compare",
            {"snapshots": [[_terminal("t1", "working")], []], "announce": False},
            _mark({"events": _drain(watcher), "live": watcher._live}, home),
        )

    poll_cases = [
        ("ok", [_reply(stdout=json.dumps({"terminals": [_terminal("t1", "working")]}))]),
        ("empty", [_reply(stdout="   ")]),
        ("rc", [_reply(rc=1, stdout="")]),
        ("invalid-json", [_reply(stdout="no es json")]),
        ("list", [_reply(stdout="[]")]),
        ("no-terminals", [_reply(stdout="{}")]),
        ("terminals-string", [_reply(stdout='{"terminals": "x"}')]),
        ("exception", [_reply(exc="oserror")]),
        ("timeout", [_reply(exc="timeout")]),
        ("empty-list", [_reply(stdout='{"terminals": []}')]),
    ]
    for index, (name, replies) in enumerate(poll_cases, start=1):
        case = f"agent_driver/watcher/poll/{index:03d}-{name}"
        with _isolated(home, case):
            watcher = make()
            with _fake_run(replies) as calls:
                _capture(
                    out,
                    case,
                    "Watcher._poll_once",
                    {"reply": replies[0]},
                    lambda: {"ok": watcher._poll_once(), "calls": calls, "live": watcher._live},
                    home,
                )

    fast_cases = [
        ("none", None, True),
        ("true", lambda: True, True),
        ("false", lambda: False, False),
        ("raises", lambda: (_ for _ in ()).throw(OSError("x")), True),
    ]
    for index, (name, target, expected) in enumerate(fast_cases, start=1):
        case = f"agent_driver/watcher/fast/{index:03d}-{name}"
        with _isolated(home, case):
            watcher = make(targets=target)
            _capture(
                out,
                case,
                "Watcher._wants_fast_poll",
                {"has_targets": target is not None, "expected": expected},
                lambda: watcher._wants_fast_poll(),
                home,
            )

    health_cases = [
        ("change-true", True),
        ("change-false", False),
        ("same", None),
    ]
    for index, (name, value) in enumerate(health_cases, start=1):
        case = f"agent_driver/watcher/health/{index:03d}-{name}"
        with _isolated(home, case):
            health: list[bool] = []
            watcher = make(health=health)
            if name == "same":
                watcher.healthy = True
            health.clear()
            _capture(
                out,
                case,
                "Watcher._set_health",
                {"initial": name == "same", "value": value},
                lambda v=value: {
                    "healthy": watcher._set_health(v) or watcher.healthy,
                    "calls": health,
                },
                home,
            )

    # on_health que falla no rompe.
    case = "agent_driver/watcher/health/004-raises"
    with _isolated(home, case):
        watcher = make()
        watcher.on_health = lambda value: (_ for _ in ()).throw(ValueError("x"))
        _capture(
            out,
            case,
            "Watcher._set_health",
            {"value": False, "raises": True},
            lambda: {"healthy": watcher._set_health(False) or watcher.healthy},
            home,
        )

    loop_cases = [
        ("fast", lambda: True, [_reply(stdout='{"terminals": []}')], 2),
        ("idle", lambda: False, [_reply(stdout='{"terminals": []}')], 2),
        (
            "backoff",
            None,
            [_reply(rc=1), _reply(rc=1), _reply(rc=1), _reply(rc=1), _reply(rc=1)],
            5,
        ),
        (
            "recover",
            lambda: True,
            [
                _reply(rc=1),
                _reply(rc=1),
                _reply(stdout='{"terminals": []}'),
                _reply(stdout='{"terminals": []}'),
            ],
            4,
        ),
        (
            "targets-raises",
            lambda: (_ for _ in ()).throw(OSError("x")),
            [_reply(stdout='{"terminals": []}')],
            2,
        ),
    ]
    for index, (name, target, replies, polls) in enumerate(loop_cases, start=1):
        case = f"agent_driver/watcher/loop/{index:03d}-{name}"
        with _isolated(home, case):
            health: list[bool] = []
            watcher = make(health=health, targets=target)
            stop = _StopAfter(polls)
            watcher._stop = stop  # type: ignore[assignment]
            with _fake_run(replies) as calls:
                _capture(
                    out,
                    case,
                    "Watcher._loop",
                    {
                        "polls": polls,
                        "replies": replies,
                        "has_targets": target is not None,
                        "targets": name,
                    },
                    lambda: {
                        "delays": stop.delays,
                        "health": health,
                        "calls": calls,
                        "live": watcher._live,
                    },
                    home,
                )

    deliver_cases = [
        ("ok", [("t1", "codex", "done", "done")], False),
        ("raises", [("t1", "codex", "done", "done")], True),
        (
            "several",
            [("t1", "codex", "done", "done"), ("t2", "opencode", "blocked", "blocked")],
            False,
        ),
    ]
    for index, (name, events, raises) in enumerate(deliver_cases, start=1):
        case = f"agent_driver/watcher/deliver/{index:03d}-{name}"
        with _isolated(home, case):
            seen: list[Any] = []

            def on_event(*args: Any) -> None:
                seen.append(args)
                if raises:
                    raise ValueError("callback roto")

            watcher = Watcher(
                _config(),
                on_event=on_event,
                should_announce=lambda address: True,
            )
            for event in events:
                watcher._events.put(event)
            watcher._events.put(None)
            _capture(
                out,
                case,
                "Watcher._deliver",
                {"events": events, "raises": raises},
                lambda: {"seen": seen},
                home,
            )

    from ezagent.voice import watcher as watcher_module

    class FakeThread:
        def __init__(self, *, target: Any, name: str | None = None, daemon: bool = False):
            self.target = target
            self.name = name
            self.daemon = daemon
            self.alive = False
            self.record: dict[str, Any] | None = None

        def start(self) -> None:
            self.alive = True
            if self.record is not None:
                self.record["start"] = self.name

        def is_alive(self) -> bool:
            return self.alive

        def join(self, timeout: float | None = None) -> None:
            if self.record is not None:
                self.record["join"] = timeout

    for index, kill_delivery in enumerate([False, True], start=1):
        case = f"agent_driver/watcher/lifecycle/{index:03d}"
        with _isolated(home, case):
            threads: list[FakeThread] = []

            def spawn(**kwargs: Any) -> FakeThread:
                thread = FakeThread(**kwargs)
                thread.record = {}
                threads.append(thread)
                return thread

            original_thread = watcher_module.threading.Thread
            watcher_module.threading.Thread = spawn  # type: ignore[assignment]
            try:
                watcher = make()
                watcher.start()
                watcher.start()
                if kill_delivery:
                    threads[0].alive = False
                    watcher.start()
                watcher.stop()
            finally:
                watcher_module.threading.Thread = original_thread  # type: ignore[assignment]
            _capture(
                out,
                case,
                "Watcher.start/stop",
                {"start_calls": 3 if kill_delivery else 2, "kill_delivery": kill_delivery},
                lambda: {
                    "threads": [
                        {
                            "name": thread.name,
                            "alive": thread.alive,
                            "record": thread.record,
                        }
                        for thread in threads
                    ],
                    "stop_set": watcher._stop.is_set(),
                    "queue": watcher._events.get_nowait(),
                },
                home,
            )

    case = "agent_driver/watcher/state/001"
    with _isolated(home, case):
        watcher = make()
        watcher._compare([_terminal("t1", "working")])
        _capture(
            out,
            case,
            "Watcher.state_of",
            {
                "known": "t1",
                "unknown": "t9",
                "snapshots": [[_terminal("t1", "working")]],
            },
            lambda: {
                "known": watcher.state_of("t1"),
                "unknown": watcher.state_of("t9"),
            },
            home,
        )


def generate(out: FixtureFile) -> None:
    home = Path(os.environ["HOME"])
    _prelaunch_paths(out, home)
    _prepare_state_cases(out, home)
    _mcode_discovery_cases(out, home)
    _mcode_effort_cases(out, home)
    _codex_discovery_cases(out, home)
    _jsonc_cases(out, home)
    _opencode_default_cases(out, home)
    _session_text_cases(out, home)
    _wait_cases(out, home)
    _session_steps_cases(out, home)
    _flow_cases(out, home)
    _driver_resolve_cases(out, home)
    _launch_cases(out, home)
    _codex_resolve_cases(out, home)
    _catalog_cases(out, home)
    _watcher_cases(out, home)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
