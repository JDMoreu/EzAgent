"""Corre todos los generadores de `parity/gen/` y escribe `parity/gen/manifest.json`.

Cada módulo `parity/gen/<dominio>.py` (sin `_`) expone `DOMAIN` y `generate`.
`--check` exige que el resultado coincida con el manifiesto guardado.

Uso:
    uv run python parity/gen/run_all.py
    uv run python parity/gen/run_all.py --check
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[1]
MANIFEST = HERE / "manifest.json"

sys.path.insert(0, str(HERE))

from _common import sha256  # noqa: E402


def generators() -> list[tuple[str, pathlib.Path]]:
    found = []
    for path in sorted(HERE.glob("*.py")):
        if path.name.startswith("_"):
            continue
        spec = importlib.util.spec_from_file_location(f"ezgen_{path.stem}", path)
        if spec is None or spec.loader is None:
            continue
        module = importlib.util.module_from_spec(spec)
        # dataclasses busca el modulo en sys.modules al crearse
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        if hasattr(module, "generate") and hasattr(module, "DOMAIN"):
            found.append((module.DOMAIN, path))
    return found


def run_one(domain: str, path: pathlib.Path) -> list[str]:
    import _common

    spec = importlib.util.spec_from_file_location(f"ezgen_{path.stem}", path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    with _common.frozen_environment():
        fixtures = _common.FixtureFile(module.DOMAIN)
        module.generate(fixtures)
        written = fixtures.write()
    produced = [str(written.relative_to(ROOT)).replace("\\", "/")]
    for extra in sorted((written.parent).glob(f"{written.stem}.*")):
        produced.append(str(extra.relative_to(ROOT)).replace("\\", "/"))
    return produced


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args(argv[1:])

    found = generators()
    if not found:
        print("no hay generadores en parity/gen/")
        return 1

    current: dict[str, str] = {}
    for domain, path in found:
        try:
            for relative in run_one(domain, path):
                current[relative] = sha256(ROOT / relative)
        except Exception as error:  # un dominio roto no debe ocultar los demás
            print(f"{domain}: FALLA {error}")
            return 1

    if args.check:
        if not MANIFEST.exists():
            print("falta parity/gen/manifest.json; corre sin --check primero")
            return 1
        stored = json.loads(MANIFEST.read_text(encoding="utf-8"))
        missing = sorted(set(stored) - set(current))
        extra = sorted(set(current) - set(stored))
        changed = sorted(name for name in stored if name in current and stored[name] != current[name])
        if missing or extra or changed:
            for name in missing:
                print(f"FALTA {name}")
            for name in extra:
                print(f"SOBRA {name}")
            for name in changed:
                print(f"CAMBIA {name}")
            return 1
        print(f"oraculo estable: {len(current)} archivos")
        return 0

    MANIFEST.write_text(
        json.dumps(dict(sorted(current.items())), indent=2) + "\n", encoding="utf-8", newline="\n"
    )
    print(f"manifiesto con {len(current)} archivos")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
