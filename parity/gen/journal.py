"""Fixtures del diario de operaciones deshacibles (dominio `journal`, F1-07).

Cubre `src/ezagent/agent/journal.py`.

Cada escenario ejecuta operaciones sobre un `Journal` en un directorio
temporal y devuelve:

- el resultado de cada operación (`UndoOutcome.to_dict` para `undo_last`),
- el texto exacto de cada archivo de entrada del diario,
- el estado del árbol de archivos sobre el que se deshace.

El reloj se congela en `2025-01-01T00:00:00+00:00` (UTC) y los identificadores
aleatorios (`<20 dígitos>-<8 dígitos>-<12 hex>`) se normalizan a `<id>`. Las
rutas absolutas del perfil temporal se normalizan a `<tmp>`.

`_send_to_trash` se sustituye por un movimiento a una carpeta temporal propia
para no llenar la Papelera real al regenerar el oráculo; el resto del flujo
(comprobar existencia y llamar al envío) es el del Python.
"""

from __future__ import annotations

import os
import re
import shutil
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "journal"

_ENTRY_ID = re.compile(r"\d{20}-\d{8}-[0-9a-f]{12}")
_FIXED = datetime(2025, 1, 1, 0, 0, 0, tzinfo=timezone.utc)


class _FrozenDatetime(datetime):
    @classmethod
    def now(cls, tz: Any = None) -> datetime:  # noqa: D102 - parche del oráculo
        if tz is None:
            return _FIXED.replace(tzinfo=None).replace(tzinfo=timezone.utc)
        return _FIXED.astimezone(tz)

    def astimezone(self, tz: Any = None) -> datetime:  # noqa: D102
        if tz is None:
            return self
        return super().astimezone(tz)


def _norm(value: Any, home: Path) -> Any:
    if isinstance(value, str):
        text = _ENTRY_ID.sub("<id>", value)
        # En un archivo JSON cada separador de Windows aparece duplicado.
        text = text.replace("\\\\", "/")
        for prefix in (home.as_posix(), str(home)):
            text = text.replace(prefix, "<tmp>")
        return text.replace("\\", "/")
    if isinstance(value, dict):
        return {key: _norm(item, home) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_norm(item, home) for item in value]
    return value


def _path(raw: Any, work: Path, home: Path) -> Path:
    text = str(raw)
    if text.startswith("<work>/"):
        return work / text[len("<work>/") :]
    if text.startswith("<home>/"):
        return home / text[len("<home>/") :]
    path = Path(text)
    return path if path.is_absolute() else work / path


def _steps(value: Any, work: Path, home: Path) -> Any:
    if isinstance(value, dict):
        return {
            key: (
                str(_path(item, work, home))
                if key in ("path", "src", "dst") and str(item) != ""
                else ("" if key in ("path", "src", "dst") else _steps(item, work, home))
            )
            for key, item in value.items()
        }
    if isinstance(value, list):
        return [_steps(item, work, home) for item in value]
    return value


def _write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")


def _tree(root: Path, *, skip: set[str], home: Path) -> dict[str, str]:
    result: dict[str, str] = {}
    if not root.is_dir():
        return result
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root)
        if relative.parts and relative.parts[0] in skip:
            continue
        key = relative.as_posix()
        if path.is_dir():
            result[key] = "<dir>"
        else:
            result[key] = path.read_text(encoding="utf-8", errors="replace")
    return _norm(result, home)


def _entry_files(directory: Path, home: Path) -> list[dict[str, str]]:
    files = []
    for path in sorted(directory.glob("*.json")):
        files.append(
            {
                "name": _ENTRY_ID.sub("<id>", path.name),
                "text": _norm(path.read_text(encoding="utf-8", errors="replace"), home),
            }
        )
    return files


class _Scenario:
    def __init__(self, out: FixtureFile, home: Path, case: str) -> None:
        from ezagent.agent.journal import Journal

        self.out = out
        self.home = home
        self.case = case
        self.work = home / "journal-cases" / case
        self.work.mkdir(parents=True, exist_ok=True)
        self.directory = self.work / "journal"
        self.journal = Journal(self.directory)
        self.ops: list[dict[str, Any]] = []
        self.results: list[Any] = []
        self.validated: list[str] = []
        self.trashed: list[str] = []
        self._original_trash = None

    def __enter__(self) -> "_Scenario":
        from ezagent.agent import journal as module

        def send_to_trash(path: Path) -> None:
            self.trashed.append(_norm(str(path), self.home))
            target = self.work / "_trash" / f"{len(self.trashed):03d}-{path.name}"
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.move(str(path), str(target))

        self._original_trash = module._send_to_trash
        module._send_to_trash = send_to_trash
        return self

    def __exit__(self, *_exception: Any) -> None:
        from ezagent.agent import journal as module

        module._send_to_trash = self._original_trash

    def validate(self, mode: str) -> Any:
        def check(path: Path) -> Path:
            self.validated.append(_norm(str(path), self.home))
            if mode == "reject":
                raise ValueError("fuera de la raíz permitida")
            if mode == "suffix":
                return path.with_name(path.name + ".ok")
            return path

        return check

    def run(self, op: dict[str, Any]) -> Any:
        self.ops.append(op)
        action = op["op"]
        journal = self.journal
        work, home = self.work, self.home
        try:
            if action == "write":
                _write(_path(op["name"], work, home), op.get("text", ""))
                return True
            if action == "mkdir":
                _path(op["name"], work, home).mkdir(parents=True, exist_ok=True)
                return True
            if action == "rm":
                path = _path(op["name"], work, home)
                if path.is_dir():
                    path.rmdir()
                elif path.exists():
                    path.unlink()
                return True
            if action == "save_entry":
                import json

                escaped = json.dumps(str(work))[1:-1]
                _write(
                    self.directory / op["name"],
                    op.get("text", "").replace("<work>", escaped),
                )
                return True
            if action == "reopen":
                journal = type(journal)(self.directory)
                self.journal = journal
                return str(journal.directory).replace(str(home), "<tmp>").replace("\\", "/")
            if action == "record":
                return journal.record(
                    op.get("kind", ""),
                    op.get("summary", ""),
                    _steps(op.get("steps", []), work, home),
                )
            if action == "record_create":
                return journal.record_create(
                    _path(op["path"], work, home),
                    kind=op.get("kind", "create"),
                    summary=op.get("summary", ""),
                )
            if action == "record_copy":
                return journal.record_copy(
                    _path(op["src"], work, home),
                    _path(op["dst"], work, home),
                    overwrite=op.get("overwrite", False),
                    backup=_path(op["backup"], work, home) if op.get("backup") else None,
                )
            if action == "record_move":
                return journal.record_move(
                    _path(op["src"], work, home),
                    _path(op["dst"], work, home),
                    overwrite=op.get("overwrite", False),
                    backup=_path(op["backup"], work, home) if op.get("backup") else None,
                )
            if action == "record_overwrite":
                return journal.record_overwrite(
                    _path(op["target"], work, home),
                    _path(op["backup"], work, home),
                    summary=op.get("summary", ""),
                )
            if action == "undo":
                validate = (
                    self.validate(op.get("validate", "none"))
                    if op.get("validate", "none") != "none"
                    else None
                )
                return [
                    outcome.to_dict()
                    for outcome in journal.undo_last(op.get("n", 1), validate=validate)
                ]
            if action == "load":
                return [dict(entry) for entry in journal._load_entries()]
            if action == "write_entry":
                journal._write_entry(op.get("entry", {}))
                return True
            raise ValueError(f"operación desconocida: {action}")
        except Exception as error:
            return {"error": f"{type(error).__name__}: {error}"}

    def finish(self) -> None:
        entries = self.journal._load_entries()
        output = {
            "results": _norm(self.results, self.home),
            "validated": self.validated,
            "trashed": self.trashed,
            "entries": _norm([dict(entry) for entry in entries], self.home),
            "entry_files": _entry_files(self.directory, self.home),
            "tree": _tree(self.work, skip={"journal", "_trash"}, home=self.home),
            "directory": _norm(str(self.directory), self.home),
            "directory_exists": self.directory.is_dir(),
        }
        self.out.add(f"journal/{self.case}", "Journal", {"ops": self.ops}, output)


def _scenario(out: FixtureFile, home: Path, case: str, ops: list[dict[str, Any]]) -> None:
    with _Scenario(out, home, case) as scenario:
        scenario.results = [scenario.run(op) for op in ops]
        scenario.finish()


def _scenarios(out: FixtureFile, home: Path) -> None:
    cases: list[tuple[str, list[dict[str, Any]]]] = [
        ("empty", []),
        (
            "record-create",
            [
                {"op": "write", "name": "a.txt", "text": "hola"},
                {"op": "record_create", "path": "<work>/a.txt"},
            ],
        ),
        (
            "undo-create",
            [
                {"op": "write", "name": "a.txt", "text": "hola"},
                {"op": "record_create", "path": "<work>/a.txt"},
                {"op": "undo"},
            ],
        ),
        (
            "record-copy",
            [
                {"op": "write", "name": "src.txt", "text": "origen"},
                {"op": "write", "name": "dst.txt", "text": "destino"},
                {"op": "record_copy", "src": "<work>/src.txt", "dst": "<work>/dst.txt"},
                {"op": "undo"},
            ],
        ),
        (
            "copy-overwrite",
            [
                {"op": "write", "name": "src.txt", "text": "origen"},
                {"op": "write", "name": "dst.txt", "text": "destino"},
                {"op": "write", "name": "dst.bak", "text": "respaldo"},
                {
                    "op": "record_copy",
                    "src": "<work>/src.txt",
                    "dst": "<work>/dst.txt",
                    "overwrite": True,
                    "backup": "<work>/dst.bak",
                },
                {"op": "undo"},
            ],
        ),
        (
            "copy-overwrite-error",
            [
                {"op": "write", "name": "src.txt", "text": "origen"},
                {
                    "op": "record_copy",
                    "src": "<work>/src.txt",
                    "dst": "<work>/dst.txt",
                    "overwrite": True,
                },
            ],
        ),
        (
            "copy-missing-dst-undo",
            [
                {"op": "write", "name": "src.txt", "text": "origen"},
                {"op": "record_copy", "src": "<work>/src.txt", "dst": "<work>/dst.txt"},
                {"op": "undo"},
            ],
        ),
        (
            "move",
            [
                {"op": "write", "name": "dst.txt", "text": "mudado"},
                {
                    "op": "record_move",
                    "src": "<work>/src.txt",
                    "dst": "<work>/dst.txt",
                },
                {"op": "undo"},
            ],
        ),
        (
            "move-overwrite",
            [
                {"op": "write", "name": "dst.txt", "text": "mudado"},
                {"op": "write", "name": "dst.bak", "text": "respaldo"},
                {
                    "op": "record_move",
                    "src": "<work>/src.txt",
                    "dst": "<work>/dst.txt",
                    "overwrite": True,
                    "backup": "<work>/dst.bak",
                },
                {"op": "undo"},
            ],
        ),
        (
            "move-overwrite-error",
            [
                {"op": "write", "name": "src.txt", "text": "origen"},
                {
                    "op": "record_move",
                    "src": "<work>/src.txt",
                    "dst": "<work>/dst.txt",
                    "overwrite": True,
                },
            ],
        ),
        (
            "overwrite",
            [
                {"op": "write", "name": "target.txt", "text": "nuevo"},
                {"op": "write", "name": "backup.txt", "text": "viejo"},
                {
                    "op": "record_overwrite",
                    "target": "<work>/target.txt",
                    "backup": "<work>/backup.txt",
                },
                {"op": "undo"},
            ],
        ),
        (
            "overwrite-custom-summary",
            [
                {"op": "write", "name": "target.txt", "text": "nuevo"},
                {"op": "write", "name": "backup.txt", "text": "viejo"},
                {
                    "op": "record_overwrite",
                    "target": "<work>/target.txt",
                    "backup": "<work>/backup.txt",
                    "summary": "Resumen propio con ñ",
                },
                {"op": "load"},
            ],
        ),
        (
            "overwrite-missing-backup",
            [
                {"op": "write", "name": "target.txt", "text": "nuevo"},
                {
                    "op": "record_overwrite",
                    "target": "<work>/target.txt",
                    "backup": "<work>/no.txt",
                },
                {"op": "undo"},
            ],
        ),
        (
            "undo-multiple",
            [
                {"op": "write", "name": "a.txt", "text": "a"},
                {"op": "write", "name": "b.txt", "text": "b"},
                {"op": "write", "name": "c.txt", "text": "c"},
                {"op": "record_create", "path": "<work>/a.txt", "summary": "Crear a"},
                {"op": "record_create", "path": "<work>/b.txt", "summary": "Crear b"},
                {"op": "record_create", "path": "<work>/c.txt", "summary": "Crear c"},
                {"op": "undo", "n": 2},
                {"op": "undo", "n": 1},
                {"op": "undo", "n": 1},
            ],
        ),
        (
            "undo-more-than-entries",
            [
                {"op": "write", "name": "a.txt", "text": "a"},
                {"op": "record_create", "path": "<work>/a.txt"},
                {"op": "undo", "n": 10},
            ],
        ),
        (
            "undo-invalid-n",
            [
                {"op": "undo", "n": True},
                {"op": "undo", "n": "1"},
                {"op": "undo", "n": 0},
                {"op": "undo", "n": -1},
                {"op": "undo", "n": 1.5},
            ],
        ),
        (
            "record-no-steps",
            [{"op": "record", "kind": "x", "summary": "sin pasos", "steps": []}],
        ),
        (
            "record-persist",
            [
                {"op": "record_create", "path": "<work>/a.txt"},
                {"op": "reopen"},
                {"op": "load"},
            ],
        ),
        (
            "entry-format",
            [
                {
                    "op": "record",
                    "kind": "custom",
                    "summary": "Resumen con «ñ» y 🙂",
                    "steps": [
                        {"action": "move", "src": "<work>/ñ.txt", "dst": "<work>/ñ-2.txt"},
                        {"action": "trash", "path": "<work>/x.txt"},
                    ],
                },
                {"op": "load"},
            ],
        ),
        (
            "unknown-action",
            [
                {"op": "write", "name": "x.txt", "text": "x"},
                {
                    "op": "record",
                    "kind": "x",
                    "summary": "x",
                    "steps": [{"action": "explode", "path": "<work>/x.txt"}],
                },
                {"op": "undo"},
            ],
        ),
        (
            "invalid-steps",
            [
                {"op": "record", "kind": "x", "summary": "paso texto", "steps": ["texto"]},
                {
                    "op": "record",
                    "kind": "x",
                    "summary": "sin ruta",
                    "steps": [{"action": "trash"}],
                },
                {
                    "op": "record",
                    "kind": "x",
                    "summary": "ruta vacía",
                    "steps": [{"action": "trash", "path": ""}],
                },
                {
                    "op": "record",
                    "kind": "x",
                    "summary": "otra acción",
                    "steps": [{"action": "trash", "path": "<work>/nada.txt"}],
                },
                {"op": "undo", "n": 3},
            ],
        ),
        (
            "move-errors",
            [
                {
                    "op": "record",
                    "kind": "x",
                    "summary": "no existe",
                    "steps": [{"action": "move", "src": "<work>/no.txt", "dst": "<work>/x.txt"}],
                },
                {"op": "write", "name": "origen.txt", "text": "o"},
                {"op": "write", "name": "destino.txt", "text": "d"},
                {
                    "op": "record",
                    "kind": "x",
                    "summary": "destino existe",
                    "steps": [
                        {"action": "move", "src": "<work>/origen.txt", "dst": "<work>/destino.txt"}
                    ],
                },
                {"op": "undo", "n": 2},
            ],
        ),
        (
            "copy-back-errors",
            [
                {
                    "op": "record",
                    "kind": "x",
                    "summary": "copia ausente",
                    "steps": [
                        {"action": "copy_back", "src": "<work>/no.txt", "dst": "<work>/x.txt"}
                    ],
                },
                {"op": "undo"},
            ],
        ),
        (
            "trash-missing",
            [
                {
                    "op": "record",
                    "kind": "x",
                    "summary": "ya no está",
                    "steps": [{"action": "trash", "path": "<work>/no.txt"}],
                },
                {"op": "undo"},
            ],
        ),
        (
            "validate-reject",
            [
                {"op": "write", "name": "a.txt", "text": "a"},
                {"op": "record_create", "path": "<work>/a.txt"},
                {"op": "undo", "validate": "reject"},
                {"op": "load"},
            ],
        ),
        (
            "validate-suffix",
            [
                {"op": "write", "name": "a.txt", "text": "a"},
                {"op": "record_create", "path": "<work>/a.txt"},
                {"op": "undo", "validate": "suffix"},
            ],
        ),
        (
            "validate-identity",
            [
                {"op": "write", "name": "a.txt", "text": "a"},
                {"op": "record_create", "path": "<work>/a.txt"},
                {"op": "undo", "validate": "identity"},
            ],
        ),
        (
            "corrupt-entries",
            [
                {"op": "write", "name": "x.txt", "text": "x"},
                {"op": "save_entry", "name": "0001-corrupt.json", "text": "{no es json"},
                {"op": "save_entry", "name": "0002-no-steps.json", "text": '{"steps": []}'},
                {"op": "save_entry", "name": "0003-wrong-steps.json", "text": '{"steps": "x"}'},
                {
                    "op": "save_entry",
                    "name": "0004-good.json",
                    "text": '{"id": "0004", "kind": "k", "summary": "buena", "steps": [{"action": "trash", "path": "<work>/x.txt"}], "undone": false}',
                },
                {"op": "undo", "n": 2},
                {"op": "load"},
            ],
        ),
        (
            "write-entry-invalid-id",
            [
                {
                    "op": "write_entry",
                    "entry": {"id": "bad/id", "steps": [{"action": "trash", "path": "x"}]},
                },
                {"op": "write_entry", "entry": {"id": "", "steps": []}},
                {"op": "write_entry", "entry": {"id": "ok_1", "steps": []}},
            ],
        ),
        (
            "undo-already-undone",
            [
                {"op": "write", "name": "a.txt", "text": "a"},
                {"op": "record_create", "path": "<work>/a.txt"},
                {"op": "undo"},
                {"op": "undo"},
            ],
        ),
        (
            "record-direct-dir",
            [
                {"op": "mkdir", "name": "carpeta"},
                {"op": "record_create", "path": "<work>/carpeta"},
                {"op": "undo"},
            ],
        ),
        (
            "record-summary-empty",
            [
                {"op": "write", "name": "a.txt", "text": "a"},
                {"op": "record_create", "path": "<work>/a.txt", "kind": "propio", "summary": ""},
                {"op": "load"},
            ],
        ),
        (
            "copy-steps",
            [
                {"op": "write", "name": "a.txt", "text": "a"},
                {"op": "write", "name": "b.txt", "text": "b"},
                {"op": "record_copy", "src": "<work>/a.txt", "dst": "<work>/b.txt"},
                {"op": "load"},
            ],
        ),
    ]
    for case, ops in cases:
        _scenario(out, home, case, ops)


def _default_dir_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.agent.journal import default_journal_dir

    original = dict(os.environ)
    try:
        for env in (
            {"APPDATA": None, "XDG_CONFIG_HOME": None},
            {"APPDATA": str(home / "AppData"), "XDG_CONFIG_HOME": None},
            {"APPDATA": "", "XDG_CONFIG_HOME": str(home / "xdg")},
            {"APPDATA": None, "XDG_CONFIG_HOME": str(home / "xdg")},
        ):
            for key, value in env.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value
            out.add(
                f"journal/default-dir/{len([case for case in out.cases if case['fn'] == 'default_journal_dir']) + 1:02d}",
                "default_journal_dir",
                _norm(env, home),
                {"path": _norm(str(default_journal_dir()), home)},
            )
    finally:
        os.environ.clear()
        os.environ.update(original)


def generate(out: FixtureFile) -> None:
    from ezagent.agent import journal as module

    home = Path(os.environ["HOME"])
    original = module.datetime
    original_ns = module.time.time_ns
    counter = [1_735_689_600_000_000_000]

    def clock_ns() -> int:
        counter[0] += 1_000_000
        return counter[0]

    module.datetime = _FrozenDatetime
    module.time.time_ns = clock_ns
    try:
        _default_dir_cases(out, home)
        _scenarios(out, home)
    finally:
        module.datetime = original
        module.time.time_ns = original_ns


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
