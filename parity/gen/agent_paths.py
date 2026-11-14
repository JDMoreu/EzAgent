"""Fixtures de validación de rutas (dominio `agent_paths`, F1-03).

`resolve_safe` normaliza y valida la ruta antes de tocar el disco. Las rutas
del caso se escriben con los marcadores `<tmp>` (carpeta temporal del oráculo),
`<anchor>` (raíz de la unidad) y `<home>`, y la salida se vuelve a marcar para
que el fixture no dependa de la máquina.
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import Any

from _common import FixtureFile, main_guard

from ezagent.agent.paths import PathDenied, resolve_safe

DOMAIN = "agent_paths"

BASE = "<tmp>/ws"
ROOTS = ["<tmp>/ws", "<tmp>/ro"]
DENY = ["<tmp>/ws/secret", "<tmp>/private"]
GLOBS = ["**/.env", "**/*.pem", "**/id_rsa*", "**/*.kdbx"]

# (raw, roots, deny, deny_globs) — `None` usa el perfil de arriba.
CASES: list[tuple[str, list[str] | None, list[str] | None, list[str] | None]] = [
    ("a.txt", None, None, None),
    ("sub/dir/b.txt", None, None, None),
    ("./a.txt", None, None, None),
    ("sub/../a.txt", None, None, None),
    ("sub/", None, None, None),
    ("   ", None, None, None),
    ("ñandú/canción.txt", None, None, None),
    ("<tmp>/ro/lib/x.py", None, None, None),
    ("<tmp>/ws", None, None, None),
    ("<tmp>/ws/sub/../b.txt", None, None, None),
    ("~/ws/documento.txt", None, None, None),
    ("$HOME/ws/vars.txt", None, None, None),
    ("%APPDATA%/ez.txt", None, None, None),
    ("~", None, None, None),
    ("", None, None, None),
    ("a\x00b", None, None, None),
    ("//server/share/x", None, None, None),
    ("\\\\server\\share\\x", None, None, None),
    ("notas.txt:oculto", None, None, None),
    ("CON", None, None, None),
    ("sub/aux.txt", None, None, None),
    ("sub/com1.log", None, None, None),
    ("sub/COM0.txt", None, None, None),
    ("<tmp>/fuera.txt", None, None, None),
    ("../fuera.txt", None, None, None),
    ("<tmp>/ws/secret", None, None, None),
    ("<tmp>/ws/secret/x.txt", None, None, None),
    ("<tmp>/private/notas.txt", None, None, None),
    ("<tmp>/ws/.env", None, None, None),
    ("<tmp>/ws/.ENV", None, None, None),
    ("sub/.env.local", None, None, None),
    ("certs/server.pem", None, None, None),
    ("notes.txt", None, None, None),
    ("<tmp>/ws/.env", None, None, []),
    ("<tmp>/ws/.env", None, None, ["**/*.txt"]),
    ("<anchor>", ["<anchor>"], [], []),
    ("<anchor>Windows", ["<anchor>"], ["<anchor>Windows"], []),
    ("<anchor>Windows/system32", ["<anchor>"], ["<anchor>Windows"], GLOBS),
    ("<tmp>/ro/../ws/c.txt", ["<tmp>/ws", "<tmp>/ro"], [], GLOBS),
    ("sub", ["<tmp>/other"], [], []),
    ("%APPDATA%/ez.txt", ["<tmp>"], [], []),
    ("$NOPE/ws/x.txt", None, None, None),
    ("  notas.txt  ", None, None, None),
    ("notas.txt.", None, None, None),
]


def _expand(value: str, home: Path) -> str:
    anchor = str(home.anchor)
    return value.replace("<tmp>", str(home)).replace("<anchor>", anchor)


def _collapse(value: str, home: Path) -> str:
    prefixes = sorted({str(home), str(home.resolve())}, key=len, reverse=True)
    text = value
    for prefix in prefixes:
        text = text.replace(prefix, "<tmp>")
    return text.replace(str(home.anchor), "<anchor>")


def _mark(value: Any, home: Path) -> Any:
    if isinstance(value, str):
        return _collapse(value, home)
    if isinstance(value, list):
        return [_mark(item, home) for item in value]
    return value


def generate(out: FixtureFile) -> None:
    home = Path(os.environ["HOME"])
    for index, (raw, roots, deny, globs) in enumerate(CASES, start=1):
        case = f"agent_paths/resolve_safe/{index:04d}"
        input_data = {
            "raw": raw,
            "base": BASE,
            "roots": ROOTS if roots is None else roots,
            "deny": DENY if deny is None else deny,
            "deny_globs": GLOBS if globs is None else globs,
        }
        try:
            resolved = resolve_safe(
                _expand(raw, home),
                Path(_expand(BASE, home)),
                [_expand(root, home) for root in input_data["roots"]],
                [_expand(entry, home) for entry in input_data["deny"]],
                input_data["deny_globs"],
            )
        except PathDenied as error:
            out.add_error(case, "resolve_safe", input_data, _collapse(str(error), home))
            continue
        out.add(
            case,
            "resolve_safe",
            input_data,
            {"path": _mark(str(resolved), home)},
        )


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
