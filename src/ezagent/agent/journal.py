"""Diario de operaciones deshacibles.

Cada operación que modifica el disco guarda en ``%APPDATA%/EzAgent/journal/``
un archivo JSON con los pasos necesarios para revertirla. :meth:`Journal.undo_last`
revierte las últimas ``n`` operaciones, de la más reciente a la más antigua, y
las marca como deshechas.

Las operaciones de borrado no se registran: van directamente a la Papelera del
sistema y su restauración no es portable.

# DECISION: se usa un archivo JSON por operación (escritura atómica con
# ``os.replace``) en lugar de un único archivo con una lista. Así el diario no
# se corrompe completo si falla una escritura y el orden se obtiene del nombre,
# que empieza por la marca de tiempo en nanosegundos.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import tempfile
import threading
import time
import uuid
from collections.abc import Callable
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any

_SAFE_ID = re.compile(r"^[0-9A-Za-z_-]+$")


def _now_iso() -> str:
    return datetime.now().astimezone().isoformat(timespec="seconds")


def default_journal_dir() -> Path:
    """Carpeta por defecto del diario: ``%APPDATA%/EzAgent/journal`` (o XDG)."""
    base = os.getenv("APPDATA") or os.getenv("XDG_CONFIG_HOME")
    if base:
        return Path(base) / "EzAgent" / "journal"
    return Path.home() / ".config" / "EzAgent" / "journal"


def _send_to_trash(path: Path) -> None:
    """Envía una ruta a la Papelera. Import perezoso para pruebas/entornos sin el paquete."""
    try:
        from send2trash import send2trash
    except ImportError as exc:  # pragma: no cover - depende del entorno
        raise RuntimeError(
            "Falta el paquete send2trash; no se puede enviar a la Papelera."
        ) from exc
    send2trash(str(path))


@dataclass(frozen=True)
class UndoOutcome:
    """Resultado de revertir una entrada del diario."""

    entry_id: str
    kind: str
    summary: str
    ok: bool
    error: str | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "entry_id": self.entry_id,
            "kind": self.kind,
            "summary": self.summary,
            "ok": self.ok,
            "error": self.error,
        }


class Journal:
    """Diario en disco con la operación inversa de cada cambio."""

    def __init__(self, directory: Path | None = None) -> None:
        self._dir = Path(directory) if directory is not None else default_journal_dir()
        self._dir.mkdir(parents=True, exist_ok=True)
        self._lock = threading.Lock()
        self._sequence = 0

    @property
    def directory(self) -> Path:
        return self._dir

    # ------------------------------------------------------------------
    # Registro de operaciones
    # ------------------------------------------------------------------
    def record(
        self,
        kind: str,
        summary: str,
        steps: list[dict[str, Any]],
    ) -> str:
        """Guarda una entrada y devuelve su identificador."""
        if not steps:
            raise ValueError("La operación inversa no puede estar vacía.")
        with self._lock:
            self._sequence += 1
            # El nombre ordena por tiempo y, dentro del mismo instante, por
            # secuencia de esta instancia.
            entry_id = (
                f"{time.time_ns():020d}-{self._sequence:08d}-{uuid.uuid4().hex[:12]}"
            )
            entry = {
                "version": 1,
                "id": entry_id,
                "ts": _now_iso(),
                "kind": kind,
                "summary": summary,
                "steps": steps,
                "undone": False,
                "undone_at": None,
                "undo_error": None,
            }
            self._write_entry(entry)
        return entry_id

    def record_create(
        self,
        path: Path,
        *,
        kind: str = "create",
        summary: str = "",
    ) -> str:
        path = Path(path)
        return self.record(
            kind,
            summary or f"Crear «{path}»",
            [{"action": "trash", "path": str(path)}],
        )

    def record_copy(
        self,
        src: Path,
        dst: Path,
        *,
        overwrite: bool = False,
        backup: Path | None = None,
    ) -> str:
        src, dst = Path(src), Path(dst)
        if overwrite:
            if backup is None:
                raise ValueError("La copia con sobrescritura necesita una copia de seguridad.")
            steps = [
                {"action": "trash", "path": str(dst)},
                {"action": "move", "src": str(backup), "dst": str(dst)},
            ]
            summary = f"Copiar «{src}» sobre «{dst}»"
            kind = "copy_overwrite"
        else:
            steps = [{"action": "trash", "path": str(dst)}]
            summary = f"Copiar «{src}» en «{dst}»"
            kind = "copy"
        return self.record(kind, summary, steps)

    def record_move(
        self,
        src: Path,
        dst: Path,
        *,
        overwrite: bool = False,
        backup: Path | None = None,
    ) -> str:
        src, dst = Path(src), Path(dst)
        steps = [{"action": "move", "src": str(dst), "dst": str(src)}]
        if overwrite:
            if backup is None:
                raise ValueError("El movimiento con sobrescritura necesita una copia de seguridad.")
            steps.append({"action": "move", "src": str(backup), "dst": str(dst)})
            summary = f"Mover «{src}» sobre «{dst}»"
            kind = "move_overwrite"
        else:
            summary = f"Mover «{src}» a «{dst}»"
            kind = "move"
        return self.record(kind, summary, steps)

    def record_overwrite(self, target: Path, backup: Path, *, summary: str = "") -> str:
        target, backup = Path(target), Path(backup)
        return self.record(
            "overwrite",
            summary or f"Sobrescribir «{target}»",
            [{"action": "copy_back", "src": str(backup), "dst": str(target)}],
        )

    # ------------------------------------------------------------------
    # Deshacer
    # ------------------------------------------------------------------
    def undo_last(
        self,
        n: int = 1,
        *,
        validate: Callable[[Path], Path] | None = None,
    ) -> list[UndoOutcome]:
        """Revierte las últimas ``n`` operaciones no deshechas.

        ``validate`` recibe cada ruta guardada y devuelve la ruta resuelta y
        permitida; si lanza una excepción, la entrada se marca como deshecha
        con error y no se opera sobre el disco. Devuelve un resultado por
        entrada, de la más reciente a la más antigua.
        """
        if isinstance(n, bool) or not isinstance(n, int):
            raise ValueError("n debe ser un número entero.")
        if n < 1:
            raise ValueError("n debe ser mayor o igual que 1.")

        with self._lock:
            entries = [entry for entry in self._load_entries() if not entry.get("undone")]
            selected = entries[:n]
            outcomes: list[UndoOutcome] = []
            for entry in selected:
                outcome = self._undo_entry(entry, validate)
                outcomes.append(outcome)
                self._mark_undone(entry, outcome.error)
            return outcomes

    def _undo_entry(
        self,
        entry: dict[str, Any],
        validate: Callable[[Path], Path] | None,
    ) -> UndoOutcome:
        entry_id = str(entry.get("id", ""))
        kind = str(entry.get("kind", ""))
        summary = str(entry.get("summary", ""))
        steps = entry.get("steps")
        if not isinstance(steps, list) or not steps:
            return UndoOutcome(entry_id, kind, summary, False, "La entrada no tiene pasos inversos.")
        try:
            for step in steps:
                self._run_step(step, validate)
        except Exception as exc:
            return UndoOutcome(entry_id, kind, summary, False, str(exc))
        return UndoOutcome(entry_id, kind, summary, True, None)

    def _run_step(
        self,
        step: Any,
        validate: Callable[[Path], Path] | None,
    ) -> None:
        if not isinstance(step, dict):
            raise ValueError("Paso inverso inválido.")
        action = str(step.get("action", ""))
        if action == "trash":
            path = self._validated(step, "path", validate)
            if not path.exists() and not path.is_symlink():
                return
            _send_to_trash(path)
            return
        if action == "move":
            src = self._validated(step, "src", validate)
            dst = self._validated(step, "dst", validate)
            if not src.exists() and not src.is_symlink():
                raise FileNotFoundError(f"No existe «{src}» para devolverlo a su sitio.")
            if dst.exists() or dst.is_symlink():
                raise FileExistsError(f"Ya existe «{dst}»; no se sobrescribe al deshacer.")
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.move(str(src), str(dst))
            return
        if action == "copy_back":
            src = self._validated(step, "src", validate)
            dst = self._validated(step, "dst", validate)
            if not src.exists():
                raise FileNotFoundError(f"No existe la copia de seguridad «{src}».")
            self._restore_copy(src, dst)
            return
        raise ValueError(f"Acción inversa desconocida: {action}")

    @staticmethod
    def _validated(
        step: dict[str, Any],
        key: str,
        validate: Callable[[Path], Path] | None,
    ) -> Path:
        raw = step.get(key)
        if not raw:
            raise ValueError(f"Falta la ruta «{key}» en el paso inverso.")
        path = Path(str(raw))
        if validate is None:
            return path
        return validate(path)

    @staticmethod
    def _restore_copy(src: Path, dst: Path) -> None:
        dst.parent.mkdir(parents=True, exist_ok=True)
        fd, raw = tempfile.mkstemp(prefix=".ezagent-undo-", suffix=".tmp", dir=str(dst.parent))
        os.close(fd)
        tmp = Path(raw)
        try:
            shutil.copy2(src, tmp)
            os.replace(tmp, dst)
        except BaseException:
            try:
                if tmp.exists():
                    _send_to_trash(tmp)
            except Exception:
                pass
            raise

    # ------------------------------------------------------------------
    # Persistencia
    # ------------------------------------------------------------------
    def _load_entries(self) -> list[dict[str, Any]]:
        entries: list[dict[str, Any]] = []
        try:
            files = sorted(self._dir.glob("*.json"), reverse=True)
        except OSError:
            return entries
        for file in files:
            try:
                data = json.loads(file.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                # DECISION: una entrada corrupta se ignora; no bloquea el resto.
                continue
            if isinstance(data, dict) and isinstance(data.get("steps"), list):
                entries.append(data)
        return entries

    def _write_entry(self, entry: dict[str, Any]) -> None:
        entry_id = str(entry.get("id", ""))
        if not _SAFE_ID.match(entry_id):
            raise ValueError("Identificador de diario inválido.")
        data = json.dumps(entry, ensure_ascii=False, indent=2)
        fd, raw = tempfile.mkstemp(prefix=".entry-", suffix=".tmp", dir=str(self._dir))
        tmp = Path(raw)
        try:
            with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as handle:
                handle.write(data)
                handle.flush()
                os.fsync(handle.fileno())
            os.replace(tmp, self._dir / f"{entry_id}.json")
        except BaseException:
            try:
                if tmp.exists():
                    _send_to_trash(tmp)
            except Exception:
                pass
            raise

    def _mark_undone(self, entry: dict[str, Any], error: str | None) -> None:
        entry["undone"] = True
        entry["undone_at"] = _now_iso()
        entry["undo_error"] = error
        try:
            self._write_entry(entry)
        except (OSError, ValueError):
            # DECISION: si no se puede reescribir la entrada, el resultado de la
            # reversión ya se devolvió; no se lanza para no ocultar ese resultado.
            pass
