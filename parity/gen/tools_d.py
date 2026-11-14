"""Fixtures de herramientas declarativas (dominio `tools_d`, F1-03).

Cubre la carga de `tools.d/*.toml` (validación, niveles, duplicados, archivos
rotos) y el camino de ejecución con `subprocess.run` sustituido por un doble:
así el oráculo fija el argv exacto, el entorno limpio y el recorte de salida
sin lanzar ningún proceso.
"""

from __future__ import annotations

import contextlib
import logging
import os
import subprocess
from collections.abc import Iterator
from dataclasses import asdict
from pathlib import Path
from typing import Any

from _common import FixtureFile, main_guard

from ezagent.agent import tools_d as tools_module
from ezagent.agent.config import AgentConfig
from ezagent.agent.tools_d import clean_environment, load_tools_d

DOMAIN = "tools_d"

APP = "app/permissions.toml"
WORKSPACE = "ws"
DEFAULT_EXECUTABLES = ["echo", "code"]
MAX_OUTPUT_CHARS = tools_module.MAX_OUTPUT_CHARS

REPLY: dict[str, Any] = {"rc": 0, "stdout": "hola\n", "stderr": "", "exc": None}
CALLS: list[dict[str, Any]] = []


def _fake_run(argv: list[str], **kwargs: Any) -> subprocess.CompletedProcess[str]:
    CALLS.append(
        {
            "argv": list(argv),
            "shell": kwargs.get("shell"),
            "cwd": kwargs.get("cwd"),
            "timeout": kwargs.get("timeout"),
            "encoding": kwargs.get("encoding"),
            "errors": kwargs.get("errors"),
            "check": kwargs.get("check"),
            "env_keys": sorted(kwargs.get("env") or {}),
        }
    )
    if REPLY["exc"] == "timeout":
        raise subprocess.TimeoutExpired(argv, kwargs.get("timeout") or 0)
    if REPLY["exc"] == "oserror":
        raise OSError("no se encontró el programa")
    return subprocess.CompletedProcess(argv, REPLY["rc"], REPLY["stdout"], REPLY["stderr"])


@contextlib.contextmanager
def _patched_run() -> Iterator[None]:
    original = tools_module.subprocess.run
    tools_module.subprocess.run = _fake_run  # type: ignore[assignment]
    try:
        yield
    finally:
        tools_module.subprocess.run = original  # type: ignore[assignment]


def _config(
    directory: Path,
    executables: list[str],
    read_roots: tuple[str, ...] = (WORKSPACE,),
    write_roots: tuple[str, ...] = (WORKSPACE,),
    deny: tuple[str, ...] = (),
    deny_globs: tuple[str, ...] = (),
) -> AgentConfig:
    config = AgentConfig.load(Path(APP))
    config.workspace = Path(WORKSPACE)
    config.tools_d_dir = directory
    config.tools_d.allowed_executables[:] = executables
    config.paths.read_roots[:] = list(read_roots)
    config.paths.write_roots[:] = list(write_roots)
    config.paths.deny[:] = list(deny)
    config.paths.deny_globs[:] = list(deny_globs)
    return config


def _load(
    label: str,
    files: dict[str, str],
    executables: list[str] | None = None,
    **paths: Any,
) -> tuple[list[Any], Path]:
    directory = Path("tools.d") / label
    directory.mkdir(parents=True, exist_ok=True)
    for name, text in files.items():
        (directory / name).write_text(text, encoding="utf-8", newline="")
    config = _config(directory, executables or DEFAULT_EXECUTABLES, **paths)
    return load_tools_d(config), directory


def _spec(tool: Any) -> dict[str, Any]:
    return {
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.parameters,
        "level": tool.level,
        "untrusted_output": tool.untrusted_output,
        "timeout_s": tool.timeout_s,
        "schema": tool.schema(),
        "info": asdict(tool.describe({})),
    }


TOOL_MINIMAL = """\
name = "doc_min"
description = "Herramienta mínima."
command = ["echo"]
"""

TOOL_FULL = """\
name = "doc_full"
description = "  Herramienta completa.  "
command = ["code", "--ruta", "{ruta}", "--n", "{n}", "--modo", "{modo}", "--activo", "{activo}", "--ratio", "{ratio}"]
level = 1
timeout_s = 2.5
untrusted_output = true

[params.ruta]
type = "string"
description = "Ruta de lectura."
validate = "path_in_read_roots"

[params.n]
type = "integer"
minimum = 1
maximum = 10
required = false

[params.modo]
type = "string"
validate = "enum:rapido, lento"

[params.activo]
type = "boolean"
required = false

[params.ratio]
type = "number"
minimum = 0.0
"""

TOOL_WRITE = """\
name = "doc_write"
description = "Escribe en una ruta."
command = ["code", "{ruta}"]
level = 3

[params.ruta]
validate = "path_in_write_roots"
"""

TOOL_UPPER_EXE = """\
name = "doc_upper"
description = "Ejecutable en mayúsculas."
command = ["CODE", "--version"]
level = 0
"""

VALID_TOOLS: list[tuple[str, dict[str, str], list[str] | None]] = [
    ("0001", {"tool.toml": TOOL_MINIMAL}, None),
    ("0002", {"tool.toml": TOOL_FULL}, None),
    ("0003", {"tool.toml": TOOL_WRITE}, None),
    ("0004", {"tool.toml": TOOL_UPPER_EXE}, None),
    (
        "0005",
        {
            "b.toml": TOOL_MINIMAL.replace("doc_min", "doc_b"),
            "a.toml": TOOL_MINIMAL.replace("doc_min", "doc_a"),
            "c.TOML": TOOL_MINIMAL.replace("doc_min", "doc_c"),
            "notas.txt": "esto no es una herramienta",
        },
        None,
    ),
    (
        "0006",
        {
            "a.toml": TOOL_MINIMAL.replace("doc_min", "doc_dup"),
            "b.toml": TOOL_MINIMAL.replace("doc_min", "doc_dup").replace("mínima", "duplicada"),
        },
        None,
    ),
    (
        "0007",
        {"roto.toml": 'name = "doc_roto\n', "bueno.toml": TOOL_MINIMAL},
        None,
    ),
    ("0008", {"tool.toml": TOOL_MINIMAL}, []),
]

INVALID_CASES: list[tuple[str, dict[str, str], list[str] | None]] = [
    ("0001", {"tool.toml": 'description = "x"\ncommand = ["echo"]\n'}, None),
    ("0002", {"tool.toml": 'name = "Doc"\ndescription = "x"\ncommand = ["echo"]\n'}, None),
    ("0003", {"tool.toml": 'name = "doc-x"\ndescription = "x"\ncommand = ["echo"]\n'}, None),
    ("0004", {"tool.toml": 'name = "1doc"\ndescription = "x"\ncommand = ["echo"]\n'}, None),
    (
        "0005",
        {"tool.toml": f'name = "doc_{"a" * 62}"\ndescription = "x"\ncommand = ["echo"]\n'},
        None,
    ),
    ("0006", {"tool.toml": 'name = "doc_x"\ncommand = ["echo"]\n'}, None),
    ("0007", {"tool.toml": 'name = "doc_x"\ndescription = "   "\ncommand = ["echo"]\n'}, None),
    ("0008", {"tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = "echo hola"\n'}, None),
    ("0009", {"tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = []\n'}, None),
    ("0010", {"tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = [""]\n'}, None),
    ("0011", {"tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = [1]\n'}, None),
    (
        "0012",
        {"tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["curl", "https://x"]\n'},
        None,
    ),
    (
        "0013",
        {"tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["C:/bin/code"]\n'},
        None,
    ),
    (
        "0014",
        {"tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["echo"]\nlevel = true\n'},
        None,
    ),
    (
        "0015",
        {"tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["echo"]\nlevel = "1"\n'},
        None,
    ),
    (
        "0016",
        {"tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["echo"]\nlevel = 3.5\n'},
        None,
    ),
    (
        "0017",
        {"tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["echo"]\ntimeout_s = 0\n'},
        None,
    ),
    (
        "0018",
        {"tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["echo"]\ntimeout_s = -1.5\n'},
        None,
    ),
    (
        "0019",
        {"tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["echo"]\ntimeout_s = false\n'},
        None,
    ),
    (
        "0020",
        {"tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["echo"]\nparams = "x"\n'},
        None,
    ),
    (
        "0021",
        {
            "tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["echo", "{mi-param}"]\n'
            '[params."mi-param"]\ntype = "string"\n'
        },
        None,
    ),
    (
        "0022",
        {
            "tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["echo"]\nparams = { x = 1 }\n'
        },
        None,
    ),
    (
        "0023",
        {
            "tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["echo", "{x}"]\n'
            '[params.x]\ntype = "texto"\n'
        },
        None,
    ),
    (
        "0024",
        {
            "tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["echo", "{x}"]\n'
            "[params.x]\ndescription = 5\n"
        },
        None,
    ),
    (
        "0025",
        {
            "tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["echo", "{x}"]\n'
            '[params.x]\nrequired = "si"\n'
        },
        None,
    ),
    (
        "0026",
        {
            "tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["echo", "{x}"]\n'
            '[params.x]\nvalidate = "shell"\n'
        },
        None,
    ),
    (
        "0027",
        {
            "tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["echo", "{x}"]\n'
            "[params.x]\nvalidate = 5\n"
        },
        None,
    ),
    (
        "0028",
        {
            "tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["echo", "{x}"]\n'
            '[params.x]\nvalidate = "enum: , ,"\n'
        },
        None,
    ),
    (
        "0029",
        {
            "tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["echo", "{x}"]\n'
            '[params.x]\ntype = "integer"\nminimum = "uno"\n'
        },
        None,
    ),
    (
        "0030",
        {"tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["echo", "{otro}"]\n'},
        None,
    ),
    (
        "0031",
        {
            "tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["echo"]\n'
            'untrusted_output = "yes"\n'
        },
        None,
    ),
    (
        "0032",
        {"tool.toml": 'name = "doc_x"\ndescription = "x"\ncommand = ["echo", "\\u0000"]\n'},
        None,
    ),
]

TOOL_ECHO = """\
name = "doc_echo"
description = "Repite el texto."
command = ["echo", "{texto}"]

[params.texto]
type = "string"
"""

TOOL_INT = """\
name = "doc_int"
description = "Entero con rango."
command = ["echo", "{n}"]

[params.n]
type = "integer"
minimum = 1
maximum = 10
"""

TOOL_NUMBER = """\
name = "doc_num"
description = "Número con rango."
command = ["echo", "{ratio}"]

[params.ratio]
type = "number"
minimum = 0.0
maximum = 1.0
"""

TOOL_BOOL = """\
name = "doc_bool"
description = "Booleano."
command = ["echo", "{activo}"]

[params.activo]
type = "boolean"
"""

TOOL_READ = """\
name = "doc_read"
description = "Lee una ruta."
command = ["code", "--ruta", "{ruta}"]

[params.ruta]
validate = "path_in_read_roots"
"""

TOOL_WRITE_PARAM = """\
name = "doc_write_param"
description = "Escribe una ruta."
command = ["code", "{ruta}"]

[params.ruta]
validate = "path_in_write_roots"
"""

TOOL_URL = """\
name = "doc_url"
description = "Abre una URL."
command = ["code", "{url}"]

[params.url]
validate = "url_http"
"""

TOOL_ENUM = """\
name = "doc_modo"
description = "Modo de ejecución."
command = ["code", "{modo}"]

[params.modo]
validate = "enum:rapido,lento"
"""

TOOL_INTSTR = """\
name = "doc_limite"
description = "Entero escrito como texto."
command = ["code", "{limite}"]

[params.limite]
type = "string"
validate = "integer"
"""

TOOL_OPTIONAL = """\
name = "doc_opt"
description = "Parámetro opcional en el comando."
command = ["code", "{n}"]

[params.n]
type = "integer"
required = false
"""

HANDLER_CASES: list[tuple[str, str, Any, dict[str, Any]]] = [
    ("0001", TOOL_ECHO, {"texto": "hola"}, {}),
    ("0002", TOOL_ECHO, {}, {}),
    ("0003", TOOL_ECHO, {"texto": "hola", "otro": 1}, {}),
    ("0004", TOOL_ECHO, {"texto": 5}, {}),
    ("0005", TOOL_ECHO, {"texto": 'con \\ y "comillas"'}, {}),
    ("0006", TOOL_INT, {"n": "007"}, {}),
    ("0007", TOOL_INT, {"n": "-3"}, {}),
    ("0008", TOOL_INT, {"n": 10}, {}),
    ("0009", TOOL_INT, {"n": 11}, {}),
    ("0010", TOOL_INT, {"n": True}, {}),
    ("0011", TOOL_INT, {"n": None}, {}),
    ("0012", TOOL_NUMBER, {"ratio": 1}, {}),
    ("0013", TOOL_NUMBER, {"ratio": 1.5}, {}),
    ("0014", TOOL_NUMBER, {"ratio": False}, {}),
    ("0015", TOOL_BOOL, {"activo": True}, {}),
    ("0016", TOOL_BOOL, {"activo": "true"}, {}),
    ("0017", TOOL_READ, {"ruta": "docs/a.txt"}, {}),
    ("0018", TOOL_READ, {"ruta": "docs/../a.txt"}, {}),
    ("0019", TOOL_READ, {"ruta": "../fuera.txt"}, {}),
    ("0020", TOOL_READ, {"ruta": "<tmp>/otro.txt"}, {}),
    ("0021", TOOL_WRITE_PARAM, {"ruta": "salida.txt"}, {}),
    ("0022", TOOL_WRITE_PARAM, {"ruta": "secretos/x.txt"}, {"deny": ("ws/secretos",)}),
    ("0023", TOOL_WRITE_PARAM, {"ruta": ".env"}, {"deny_globs": ("**/.env",)}),
    ("0024", TOOL_URL, {"url": "https://example.com/ruta?x=1"}, {}),
    ("0025", TOOL_URL, {"url": "ftp://example.com/x"}, {}),
    ("0026", TOOL_URL, {"url": "https://"}, {}),
    ("0027", TOOL_URL, {"url": "https://usuario:clave@example.com/x"}, {}),
    ("0028", TOOL_ENUM, {"modo": "rapido"}, {}),
    ("0029", TOOL_ENUM, {"modo": "veloz"}, {}),
    ("0030", TOOL_INTSTR, {"limite": " 007 "}, {}),
    ("0031", TOOL_INTSTR, {"limite": "siete"}, {}),
    ("0032", TOOL_OPTIONAL, {}, {}),
    ("0033", TOOL_ECHO, {"texto": "x"}, {"reply": {"rc": 3, "stderr": "  falló  \n"}}),
    ("0034", TOOL_ECHO, {"texto": "x"}, {"reply": {"rc": 2, "stderr": ""}}),
    ("0035", TOOL_ECHO, {"texto": "x"}, {"reply": {"exc": "timeout"}}),
    ("0036", TOOL_ECHO, {"texto": "x"}, {"reply": {"exc": "oserror"}}),
    ("0037", TOOL_ECHO, {"texto": "x"}, {"reply": {"stdout": "z" * (MAX_OUTPUT_CHARS + 3)}}),
    ("0038", TOOL_ECHO, {"texto": "x"}, {"reply": {"stdout": "\u00f1\n", "stderr": "aviso\n"}}),
    ("0039", TOOL_NUMBER, {"ratio": "0.5"}, {}),
    ("0040", TOOL_ECHO, "texto", {}),
]


def _cases_valid(out: FixtureFile, home: Path) -> None:
    for label, files, executables in VALID_TOOLS:
        tools, _directory = _load(f"valid{label}", files, executables)
        out.add(
            f"tools_d/load_tools_d/{label}",
            "load_tools_d",
            {"files": files, "executables": executables or DEFAULT_EXECUTABLES},
            {"tools": [_mark(_spec(tool), home) for tool in tools]},
        )


def _cases_invalid(out: FixtureFile) -> None:
    for label, files, executables in INVALID_CASES:
        tools, _directory = _load(f"invalid{label}", files, executables)
        out.add(
            f"tools_d/load_tools_d/invalid{label}",
            "load_tools_d",
            {"files": files, "executables": executables or DEFAULT_EXECUTABLES},
            {"tools": [tool.name for tool in tools]},
        )

    config = _config(Path("tools.d/no-existe"), DEFAULT_EXECUTABLES)
    out.add(
        "tools_d/load_tools_d/no_dir",
        "load_tools_d",
        {"tools_d_dir": "tools.d/no-existe"},
        {"tools": [tool.name for tool in load_tools_d(config)]},
    )


def _expand(value: Any, home: Path) -> Any:
    if isinstance(value, str):
        return value.replace("<tmp>", str(home))
    if isinstance(value, list):
        return [_expand(item, home) for item in value]
    if isinstance(value, dict):
        return {key: _expand(item, home) for key, item in value.items()}
    return value


def _cases_handler(out: FixtureFile, home: Path) -> None:
    for label, text, args, options in HANDLER_CASES:
        REPLY.update({"rc": 0, "stdout": "hola\n", "stderr": "", "exc": None})
        REPLY.update(options.get("reply", {}))
        CALLS.clear()
        tools, _directory = _load(
            f"handler{label}",
            {"tool.toml": text},
            DEFAULT_EXECUTABLES,
            **{key: value for key, value in options.items() if key != "reply"},
        )
        tool = tools[0]
        with _patched_run():
            result = tool.handler(_expand(args, home))
        out.add(
            f"tools_d/ToolSpec.handler/{label}",
            "ToolSpec.handler",
            {"tool": text, "args": args, "reply": options.get("reply", {})},
            {
                "result": _mark(result.to_dict(), home),
                "calls": _mark(CALLS, home),
            },
        )


def _mark(value: Any, home: Path) -> Any:
    if isinstance(value, str):
        text = value
        for prefix in sorted({str(home), str(home.resolve())}, key=len, reverse=True):
            text = text.replace(prefix, "<tmp>")
        return text.replace(str(home.anchor), "<anchor>")
    if isinstance(value, list):
        return [_mark(item, home) for item in value]
    if isinstance(value, dict):
        return {key: _mark(item, home) for key, item in value.items()}
    return value


def _cases_clean_env(out: FixtureFile) -> None:
    environment = clean_environment()
    out.add(
        "tools_d/clean_environment/0001",
        "clean_environment",
        {},
        {
            "keys": sorted(environment),
            "all_str": all(isinstance(v, str) for v in environment.values()),
        },
    )


def generate(out: FixtureFile) -> None:
    home = Path(os.environ["HOME"])
    original = os.getcwd()
    os.chdir(home)
    previous = logging.root.manager.disable
    logging.disable(logging.CRITICAL)
    try:
        _cases_valid(out, home)
        _cases_invalid(out)
        _cases_handler(out, home)
        _cases_clean_env(out)
    finally:
        logging.disable(previous)
        os.chdir(original)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
