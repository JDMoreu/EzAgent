"""Fixtures de junctions (dominio `paths_junction`, F2-04).

Un junction del sistema (`C:\\Users\\All Users` -> `C:\\ProgramData`) permite
comprobar que `resolve_safe` resuelve el destino real antes de decidir. Python
usa `Path.resolve(strict=False)` y por tanto deniega; un port léxico lo deja
pasar. Las rutas son fijas de Windows, no del perfil del usuario.
"""

from __future__ import annotations

from pathlib import Path

from _common import FixtureFile, main_guard

DOMAIN = "paths_junction"

JUNCTION = Path("C:/Users/All Users")
REAL = Path("C:/ProgramData")


def generate(out: FixtureFile) -> None:
    from ezagent.agent.paths import resolve_safe

    out.add(
        "paths_junction/exists",
        "junction_present",
        {"path": JUNCTION.as_posix()},
        {"exists": JUNCTION.exists(), "real": REAL.as_posix() if JUNCTION.exists() else None},
    )
    if not JUNCTION.exists():
        return

    cases = [
        ("escape-file", "C:/Users/All Users/Microsoft/Windows/win.ini", ["C:/Users"]),
        ("escape-dir", "C:/Users/All Users/Packages", ["C:/Users"]),
        ("inside-ok", "C:/Users/Default User", ["C:/Users"]),
        ("anchor-denied", "C:/Users/All Users", ["C:/Users/All Users"]),
    ]
    for name, raw, roots in cases:
        try:
            resolved = resolve_safe(raw, Path("C:/Users"), roots, [], [])
            value = {"ok": True, "path": resolved.as_posix()}
        except Exception as error:
            value = {"ok": False, "error": str(error)}
        out.add(
            f"paths_junction/{name}",
            "resolve_safe",
            {"raw": raw, "roots": roots},
            value,
        )


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
