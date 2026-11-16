#!/usr/bin/env python
"""Compatibilidad de datos Python ↔ Rust y enjambre mixto (F7-03).

Uso (desde la raíz del repo, con el `.venv`):

    C:/Users/JDMoreu/Documents/EzAgent/.venv/Scripts/python.exe \\
        parity/compat/run_compat.py

Copia `%APPDATA%/EzAgent` a un temporal propio y nunca imprime ni commitea su
contenido: la tabla final solo lleva artefacto, dirección, resultado y conteos.
Ver `rust-migration/reports/F7-03-r1.md` para el alcance y las decisiones.
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import artifacts  # noqa: E402
import swarm  # noqa: E402
from support import ROOT, SRC, HarnessError, Report, Workspace, ensure_binaries, python_available  # noqa: E402


def default_profile() -> Path:
    base = os.environ.get("APPDATA") or os.environ.get("XDG_CONFIG_HOME")
    if base:
        return Path(base) / "EzAgent"
    return Path.home() / ".config" / "EzAgent"


def parse_args(argv: list[str] | None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--profile",
        type=Path,
        default=default_profile(),
        help="perfil real a copiar (por omisión, %%APPDATA%%/EzAgent)",
    )
    parser.add_argument("--no-build", action="store_true", help="no compila los binarios")
    parser.add_argument("--keep", action="store_true", help="conserva el temporal para depurar")
    parser.add_argument(
        "--only",
        choices=("all", "artifacts", "swarm"),
        default="all",
        help="ejecuta solo un grupo de comprobaciones",
    )
    return parser.parse_args(argv)


def print_report(report: Report) -> None:
    if not report.rows:
        print("Sin comprobaciones.")
        return
    artifact_width = max(len(row.artifact) for row in report.rows)
    direction_width = max(len(row.direction) for row in report.rows)
    print(f"{'Artefacto':<{artifact_width}}  {'Dirección':<{direction_width}}  Resultado  Detalle")
    for row in report.rows:
        state = "OK " if row.ok else "FALLA"
        print(
            f"{row.artifact:<{artifact_width}}  {row.direction:<{direction_width}}  "
            f"{state:<9}  {row.detail}"
        )
    failures = sum(1 for row in report.rows if not row.ok)
    print()
    print(f"{len(report.rows)} comprobaciones, {failures} fallo(s).")


def main(argv: list[str] | None = None) -> int:
    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if callable(reconfigure):
            reconfigure(encoding="utf-8", errors="replace")
    args = parse_args(argv)
    sys.path.insert(0, str(SRC))
    try:
        from loguru import logger

        logger.remove()
    except Exception:  # noqa: BLE001 - sin loguru el arnés sigue
        pass
    try:
        python_available()
        ensure_binaries(build=not args.no_build)
    except HarnessError as error:
        print(f"arnés: {error}", file=sys.stderr)
        return 2
    profile = args.profile
    if not (profile / "voice.json").exists():
        print(f"arnés: no hay perfil real en {profile}", file=sys.stderr)
        return 2
    report = Report()
    workspace = Workspace(profile, keep=args.keep)
    print(f"Perfil copiado a un temporal: {workspace.base}")
    print("(el contenido del perfil no se imprime ni se commitea)")
    try:
        if args.only in ("all", "artifacts"):
            artifacts.checks(workspace, report)
        if args.only in ("all", "swarm"):
            swarm.checks(workspace, report)
    except HarnessError as error:
        print(f"arnés: {error}", file=sys.stderr)
        return 2
    finally:
        workspace.close()
    print_report(report)
    return 0 if all(row.ok for row in report.rows) else 1


if __name__ == "__main__":
    raise SystemExit(main())
