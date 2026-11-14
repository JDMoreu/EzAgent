"""Fixtures de `fnmatch` y de `deny_globs` (dominio `paths_globs`, F2-04).

Cubre clases negadas, rangos, `]` como primer carácter, mayúsculas y el doble
chequeo de Python (ruta completa en formato posix y nombre del archivo).
"""

from __future__ import annotations

import fnmatch
import tempfile
from pathlib import Path

from _common import FixtureFile, main_guard

DOMAIN = "paths_globs"

PAIRS = [
    ("a.env", "**/.env"),
    (".env", "**/.env"),
    ("x/.env", "**/.env"),
    ("x/y/.env", "**/.env"),
    ("x/.env.local", "**/.env.*"),
    ("A/B.PEM", "**/*.pem"),
    ("a/b.pem", "**/*.pem"),
    ("a.env", "[!x]*.env"),
    ("x.env", "[!x]*.env"),
    ("a1", "[a-c]?"),
    ("d1", "[a-c]?"),
    ("a", "[!]"),
    ("b", "[!]"),
    ("a", "[]a]"),
    ("]", "[]a]"),
    ("c", "[a-c]"),
    ("z", "[!a-c]"),
    ("file1.txt", "file[0-9].txt"),
    ("filex.txt", "file[0-9].txt"),
    ("id_rsa", "**/id_rsa*"),
    ("clave.kdbx", "**/*.kdbx"),
    ("a[b", "a[b"),
    ("ab", "a[b"),
    ("a*b", "a[*]b"),
    ("axb", "a[*]b"),
]


def generate(out: FixtureFile) -> None:
    for index, (name, pattern) in enumerate(PAIRS, start=1):
        out.add(
            f"paths_globs/fnmatch/{index:03d}",
            "fnmatch",
            {"name": name, "pattern": pattern},
            {"match": fnmatch.fnmatch(name, pattern)},
        )

    scenarios = [
        ("a/b.pem", ["**/*.pem"]),
        ("a/b.PEM", ["**/*.pem"]),
        ("a/b.txt", ["**/*.pem"]),
        ("a/.env", ["**/.env"]),
        ("a/sub/.env", ["**/.env"]),
        ("a/id_rsa.pub", ["**/id_rsa*"]),
        ("a/notes.txt", ["[!n]*.txt"]),
        ("a/zeta.txt", ["[!n]*.txt"]),
        ("a/keep.txt", []),
    ]
    with tempfile.TemporaryDirectory(prefix="ezglob-") as tmp:
        workspace = Path(tmp) / "ws"
        workspace.mkdir()
        (workspace / "a").mkdir()
        for relative, patterns in scenarios:
            try:
                resolved = __import__("ezagent.agent.paths", fromlist=["resolve_safe"]).resolve_safe(
                    relative,
                    workspace,
                    [str(workspace)],
                    [],
                    patterns,
                )
                value = {"ok": True, "name": resolved.name}
            except Exception as error:
                value = {"ok": False, "error": str(error)}
            out.add(
                f"paths_globs/resolve/{relative.replace('/', '_')}.{len(patterns)}",
                "resolve_safe",
                {"raw": relative, "deny_globs": patterns},
                value,
            )


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
