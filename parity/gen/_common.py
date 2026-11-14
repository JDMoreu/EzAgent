"""Base común de los generadores del oráculo de paridad (F1-01).

Reglas que todo generador hereda de aquí: reloj congelado, semilla fija, zona
horaria fija, `HOME`/`APPDATA`/`LOCALAPPDATA` en un directorio temporal, sin red
y salida determinista (mismo hash en dos corridas).

Uso típico en `parity/gen/<dominio>.py`:

    from _common import FixtureFile, run

    DOMAIN = "speech_text"

    def generate(out: FixtureFile) -> None:
        out.add("speech_text/0001", "normalize_for_kokoro", {"text": "12,50"}, {"text": "doce coma cincuenta"})

    if __name__ == "__main__":
        run(DOMAIN, generate)
"""

from __future__ import annotations

import contextlib
import hashlib
import json
import os
import pathlib
import random
import sys
import tempfile
import time
from collections.abc import Callable, Iterator
from typing import Any

ROOT = pathlib.Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "parity" / "fixtures"
GOLDEN = ROOT / "parity" / "golden"

FIXED_TS = 1735689600.0  # 2025-01-01T00:00:00Z
FIXED_TZ = "UTC"
FIXED_SEED = 20251113


@contextlib.contextmanager
def frozen_environment() -> Iterator[pathlib.Path]:
    """Reloj, zona, semilla y perfil temporal. Nada de red ni de rutas reales."""
    original_time = time.time
    original_localtime = time.localtime
    original_gmtime = time.gmtime
    original_environ = dict(os.environ)
    original_tz = os.environ.get("TZ")
    try:
        with tempfile.TemporaryDirectory(prefix="ezparity-") as tmp:
            home = pathlib.Path(tmp)
            for name in ("HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA", "TMP", "TEMP"):
                os.environ[name] = str(home)
            os.environ["TZ"] = FIXED_TZ
            time.tzset() if hasattr(time, "tzset") else None
            time.time = lambda: FIXED_TS  # type: ignore[assignment]
            time.localtime = lambda secs=None: original_localtime(FIXED_TS if secs is None else secs)
            time.gmtime = lambda secs=None: original_gmtime(FIXED_TS if secs is None else secs)
            random.seed(FIXED_SEED)
            try:
                import numpy as np

                np.random.seed(FIXED_SEED % (2**32))
            except Exception:
                pass
            yield home
    finally:
        time.time = original_time  # type: ignore[assignment]
        time.localtime = original_localtime
        time.gmtime = original_gmtime
        os.environ.clear()
        os.environ.update(original_environ)
        if original_tz is None:
            os.environ.pop("TZ", None)


def dumps(value: Any) -> str:
    """`json.dumps` del proyecto: ASCII, sin ordenar claves, separadores de Python."""
    return json.dumps(value, ensure_ascii=True, separators=(", ", ": "))


class FixtureFile:
    """Un `.jsonl` por dominio, con el orden y el formato de §4.1."""

    def __init__(self, domain: str, base: pathlib.Path | None = None) -> None:
        self.domain = domain
        self.base = base or FIXTURES / domain
        self.cases: list[dict[str, Any]] = []
        self.seen: set[str] = set()

    def add(
        self,
        case: str,
        fn_name: str,
        input_data: Any,
        output: Any,
        error: str | None = None,
    ) -> None:
        if case in self.seen:
            raise ValueError(f"caso repetido: {case}")
        self.seen.add(case)
        self.cases.append(
            {"case": case, "fn": fn_name, "input": input_data, "output": output, "error": error}
        )

    def add_error(self, case: str, fn_name: str, input_data: Any, error: str) -> None:
        self.add(case, fn_name, input_data, None, error)

    def write(self) -> pathlib.Path:
        if not self.cases:
            raise ValueError(f"{self.domain}: no hay casos")
        ordered = sorted(self.cases, key=lambda item: item["case"])
        path = self.base.with_suffix(".jsonl")
        path.parent.mkdir(parents=True, exist_ok=True)
        text = "\n".join(dumps(item) for item in ordered) + "\n"
        path.write_text(text, encoding="utf-8", newline="\n")
        return path

    def write_extra(self, name: str, text: str) -> pathlib.Path:
        path = self.base.parent / f"{self.base.name}.{name}"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8", newline="\n")
        return path


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    digest.update(path.read_bytes())
    return digest.hexdigest()


def run(domain: str, generate: Callable[[FixtureFile], None]) -> int:
    with frozen_environment():
        fixtures = FixtureFile(domain)
        generate(fixtures)
        path = fixtures.write()
    print(f"{domain}: {len(fixtures.cases)} casos -> {path.relative_to(ROOT)}")
    return 0


def main_guard(domain: str, generate: Callable[[FixtureFile], None]) -> None:
    sys.exit(run(domain, generate))
