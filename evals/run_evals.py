"""Lanza los evals de promptfoo con el entorno del proyecto.

Uso (desde la raíz del repo)::

    uv run python evals/run_evals.py offline      # contrato del prompt, sin red ni clave
    uv run python evals/run_evals.py live         # llama al LLM configurado en .env / llm.json
    uv run python evals/run_evals.py live -- --filter-pattern "delegaci"

Todo lo que va después de ``--`` se pasa tal cual a ``promptfoo eval``.
El código de salida es el de promptfoo: 0 = todo verde, 100 = hubo fallos.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path

from dotenv import load_dotenv

HERE = Path(__file__).resolve().parent
ENTRYPOINT = HERE / "node_modules" / "promptfoo" / "dist" / "src" / "entrypoint.js"
CONFIGS = {"offline": "promptfooconfig.offline.yaml", "live": "promptfooconfig.yaml"}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("suite", choices=sorted(CONFIGS), nargs="?", default="offline")
    parser.add_argument("passthrough", nargs=argparse.REMAINDER, help="argumentos extra para promptfoo")
    args = parser.parse_args(argv)

    if not ENTRYPOINT.is_file():
        print("Falta promptfoo. Ejecuta: npm install --prefix evals", file=sys.stderr)
        return 2

    load_dotenv(HERE.parent / ".env")
    env = dict(os.environ)
    env["PROMPTFOO_PYTHON"] = sys.executable  # el intérprete del proyecto, con ezagent instalado
    env["PROMPTFOO_DISABLE_TELEMETRY"] = "1"
    env["PROMPTFOO_DISABLE_UPDATE"] = "1"
    env["PYTHONIOENCODING"] = "utf-8"

    output = HERE / "output"
    output.mkdir(exist_ok=True)
    extra = [a for a in args.passthrough if a != "--"]
    command = [
        "node", str(ENTRYPOINT), "eval",
        "-c", CONFIGS[args.suite],
        "--no-cache", "--no-table",
        "-o", str(output / f"{args.suite}.json"),
        *extra,
    ]
    return subprocess.run(command, cwd=HERE, env=env, check=False).returncode  # noqa: S603


if __name__ == "__main__":
    raise SystemExit(main())
