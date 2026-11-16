"""Utilidades del arnés de compatibilidad Python ↔ Rust (F7-03).

Nunca imprime contenido del perfil: las comparaciones son en memoria y el
informe solo muestra nombres de artefacto, dirección y conteos.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "src"
RUST = ROOT / "rust"
EXE = ".exe" if os.name == "nt" else ""
PROBE = RUST / "target" / "debug" / f"ezagent_compat{EXE}"
EZS = RUST / "target" / "debug" / f"ezs{EXE}"
TARGETS = [PROBE, EZS]


class HarnessError(RuntimeError):
    """Fallo del arnés (no del port)."""


@dataclass
class Row:
    artifact: str
    direction: str
    ok: bool
    detail: str = ""


class Report:
    def __init__(self) -> None:
        self.rows: list[Row] = []

    def add(self, artifact: str, direction: str, ok: bool, detail: str = "") -> None:
        self.rows.append(Row(artifact, direction, bool(ok), detail))

    def compare(self, artifact: str, direction: str, py, rs, note: str = "") -> bool:
        difference = first_difference(jsonable(py), jsonable(rs))
        if difference is None:
            self.add(artifact, direction, True, note)
            return True
        self.add(artifact, direction, False, f"diferencia en {difference}")
        return False


def jsonable(value):
    """Tuplas → listas y Path → texto, como viajaría por JSON."""
    if isinstance(value, dict):
        return {str(key): jsonable(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [jsonable(item) for item in value]
    if isinstance(value, set):
        return sorted(jsonable(item) for item in value)
    if isinstance(value, Path):
        return str(value)
    return value


def first_difference(left, right, path: str = "") -> str | None:
    """Ruta de la primera diferencia estructural; nunca devuelve valores."""
    where = path or "<raíz>"
    if isinstance(left, dict) and isinstance(right, dict):
        for key in sorted(set(left) | set(right)):
            if key not in left:
                return f"{where}.{key} (solo Rust)"
            if key not in right:
                return f"{where}.{key} (solo Python)"
            found = first_difference(left[key], right[key], f"{where}.{key}")
            if found is not None:
                return found
        return None
    if isinstance(left, list) and isinstance(right, list):
        if len(left) != len(right):
            return f"{where} (longitud {len(left)} ≠ {len(right)})"
        for index, (one, other) in enumerate(zip(left, right)):
            found = first_difference(one, other, f"{where}[{index}]")
            if found is not None:
                return found
        return None
    if isinstance(left, dict) or isinstance(right, dict):
        return f"{where} (uno es objeto y el otro no)"
    if isinstance(left, list) or isinstance(right, list):
        return f"{where} (uno es lista y el otro no)"
    if type(left) is not type(right):
        return f"{where} ({type(left).__name__} ≠ {type(right).__name__})"
    return None if left == right else where


def read_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


class Case:
    """Una copia desechable del perfil: `root/EzAgent` y `root/Local`."""

    def __init__(self, root: Path) -> None:
        self.root = root
        self.home = root / "EzAgent"
        self.local = root / "Local"
        self._probes = 0

    def activate(self) -> None:
        """Deja el proceso de Python viendo la misma copia que el de Rust."""
        os.environ["APPDATA"] = str(self.root)
        os.environ["LOCALAPPDATA"] = str(self.local)
        for name in list(os.environ):
            if name.startswith(("LLM_", "WHISPER_", "EZS_")) or name in (
                "EZ_LLM_CONFIG",
                "EZ_OUTPUT_DEVICE_INDEX",
            ):
                os.environ.pop(name, None)

    def env(self) -> dict[str, str]:
        env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith(("LLM_", "WHISPER_", "EZS_"))
            and key not in ("EZ_LLM_CONFIG", "EZ_OUTPUT_DEVICE_INDEX")
        }
        env["APPDATA"] = str(self.root)
        env["LOCALAPPDATA"] = str(self.local)
        env["PYTHONPATH"] = str(SRC)
        return env

    def probe(self, command: str, *flags: str):
        self._probes += 1
        out = self.root / f"probe-{command}-{self._probes}.json"
        args = [
            str(PROBE),
            command,
            "--home",
            str(self.home),
            "--out",
            str(out),
            *flags,
        ]
        result = subprocess.run(
            args,
            cwd=ROOT,
            env=self.env(),
            capture_output=True,
            text=True,
            timeout=120,
        )
        if not out.exists():
            raise HarnessError(
                f"{command}: sin salida (código {result.returncode}) {result.stderr[-400:]}"
            )
        data = read_json(out)
        if "error" in data:
            raise HarnessError(f"{command}: {data['error']}")
        return data


class Workspace:
    """Copia de trabajo del perfil real, en un temporal propio."""

    def __init__(self, profile: Path, keep: bool = False) -> None:
        self.profile = profile
        self.keep = keep
        self.base = Path(tempfile.mkdtemp(prefix="ezcompat-"))
        self.cases: list[Case] = []

    def case(self, name: str) -> Case:
        root = self.base / name
        if root.exists():
            shutil.rmtree(root)
        root.mkdir(parents=True)
        shutil.copytree(self.profile, root / "EzAgent")
        case = Case(root)
        case.activate()
        self.cases.append(case)
        return case

    def close(self) -> None:
        if not self.keep:
            shutil.rmtree(self.base, ignore_errors=True)


def ensure_binaries(build: bool) -> None:
    missing = [path for path in TARGETS if not path.exists()]
    if build or missing:
        result = subprocess.run(
            [
                "cargo",
                "build",
                "-p",
                "ez-app",
                "--bin",
                "ezagent_compat",
                "-p",
                "ez-swarm",
                "--bin",
                "ezs",
            ],
            cwd=RUST,
            capture_output=True,
            text=True,
            timeout=1800,
        )
        if result.returncode != 0:
            raise HarnessError(f"cargo build falló: {result.stderr[-800:]}")
    missing = [path for path in TARGETS if not path.exists()]
    if missing:
        names = ", ".join(str(path) for path in missing)
        raise HarnessError(f"faltan binarios Rust compilados: {names}")


def python_available() -> None:
    try:
        import ezagent  # noqa: F401
    except Exception as exc:  # noqa: BLE001
        raise HarnessError(
            f"este Python no puede importar ezagent ({exc}); usa el .venv del repo"
        ) from exc
    if sys.version_info < (3, 11):
        raise HarnessError("hace falta Python 3.11 o superior")
