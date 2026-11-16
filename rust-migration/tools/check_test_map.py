"""Comprueba que las referencias de un mapa de pruebas Python → Rust existan.

Uso: python rust-migration/tools/check_test_map.py <raíz del repo> <csv> [<csv>...]

Para cada fila: `estado` válido; `nota` obligatoria en `no_aplica`; y cada
referencia de `rust_test_path` existe: `ruta.rs::nombre` → el archivo existe y
define `fn nombre`; `parity/...#algo` → el archivo existe. Imprime los fallos y
sale con 1 si hay alguno.
"""

from __future__ import annotations

import csv
import re
import sys
from pathlib import Path

STATES = {"portada", "paridad", "nueva", "no_aplica"}


RUST_REF = re.compile(r"(rust/crates/[^\s()\[\],;|]+\.rs)::([A-Za-z0-9_:]+)")
DATA_REF = re.compile(r"(parity/[^\s()\[\],;|#]+)(?:#([^\s()\[\],;|]+))?")


def check_references(root: Path, text: str) -> list[str]:
    """Cada `ruta.rs::nombre` y cada `parity/archivo#caso` del texto."""
    problems = []
    for path, name in RUST_REF.findall(text):
        file = root / path
        if not file.is_file():
            problems.append(f"no existe {path}")
            continue
        name = name.split("::")[-1]
        body = file.read_text(encoding="utf-8", errors="replace")
        if name and not re.search(rf"\bfn\s+{re.escape(name)}\b", body):
            problems.append(f"{path} no define fn {name}")
    for path, case in DATA_REF.findall(text):
        file = root / path
        if not file.exists():
            problems.append(f"no existe {path}")
            continue
        if case and file.is_file():
            body = file.read_text(encoding="utf-8", errors="replace")
            if case not in body:
                problems.append(f"{path} no contiene el caso {case}")
    return problems


def main() -> int:
    root = Path(sys.argv[1])
    failures = 0
    for csv_path in sys.argv[2:]:
        rows = list(csv.DictReader(open(csv_path, encoding="utf-8")))
        for row in rows:
            test = row.get("python_test_id", "")
            state = (row.get("estado") or "").strip()
            if state not in STATES:
                print(f"{test}: estado no válido «{state}»")
                failures += 1
                continue
            if state == "no_aplica" and not (row.get("nota") or "").strip():
                print(f"{test}: no_aplica sin nota")
                failures += 1
            if state != "no_aplica" and not (row.get("rust_test_path") or "").strip():
                print(f"{test}: {state} sin referencia Rust")
                failures += 1
            for problem in check_references(root, row.get("rust_test_path") or ""):
                print(f"{test}: {problem}")
                failures += 1
        print(f"{csv_path}: {len(rows)} filas")
    print(f"fallos: {failures}")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
