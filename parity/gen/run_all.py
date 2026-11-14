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


GOLDEN = ROOT / "parity" / "golden"


def golden_files() -> dict[str, str]:
    """Cuadros dorados (PNG y JSON de estado): tambien son oraculo."""
    if not GOLDEN.exists():
        return {}
    found = {}
    for path in sorted(GOLDEN.rglob("*")):
        if path.is_file():
            found[str(path.relative_to(ROOT)).replace("\\", "/")] = sha256(path)
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


def actual_state() -> dict[str, str]:
    """Hash de lo que hay hoy en disco: fixtures y cuadros dorados."""
    found = {}
    for path in sorted((ROOT / "parity" / "fixtures").glob("*.jsonl")):
        found[str(path.relative_to(ROOT)).replace("\\", "/")] = sha256(path)
    found.update(golden_files())
    return found


def report_difference(expected: dict[str, str], current: dict[str, str]) -> bool:
    missing = sorted(set(expected) - set(current))
    extra = sorted(set(current) - set(expected))
    changed = sorted(
        name for name in expected if name in current and expected[name] != current[name]
    )
    for name in missing:
        print(f"FALTA {name}")
    for name in extra:
        print(f"SOBRA {name}")
    for name in changed:
        print(f"CAMBIA {name}")
    return bool(missing or extra or changed)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args(argv[1:])

    found = generators()
    if not found:
        print("no hay generadores en parity/gen/")
        return 1

    if args.check:
        if not MANIFEST.exists():
            print("falta parity/gen/manifest.json; corre sin --check primero")
            return 1
        stored: dict[str, str] = json.loads(MANIFEST.read_text(encoding="utf-8"))
        if not any(name.endswith(".png") for name in stored):
            print("FALLO el manifiesto no incluye los cuadros dorados; regeneralo")
            return 1
        # 1. lo commiteado tiene que estar intacto (bytes, no solo contenido)
        if report_difference(stored, actual_state()):
            print("FALLO el oraculo en disco no coincide con el manifiesto")
            return 1

    current: dict[str, str] = {}
    for domain, path in found:
        try:
            for relative in run_one(domain, path):
                current[relative] = sha256(ROOT / relative)
        except Exception as error:  # un dominio roto no debe ocultar los demás
            print(f"{domain}: FALLA {error}")
            return 1

    golden = golden_files()
    fixtures = len(current)
    current.update(golden)

    if args.check:
        # 2. regenerar tiene que dar exactamente lo mismo
        if report_difference(stored, current):
            print("FALLO la regeneracion no reproduce el manifiesto")
            return 1
        print(f"oraculo estable: {fixtures} fixtures + {len(golden)} archivos dorados")
        return 0

    MANIFEST.write_text(
        json.dumps(dict(sorted(current.items())), indent=2) + chr(10),
        encoding="utf-8",
        newline=chr(10),
    )
    print(f"manifiesto con {fixtures} fixtures + {len(golden)} archivos dorados")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
