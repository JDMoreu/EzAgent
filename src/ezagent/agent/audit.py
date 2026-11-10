"""Registro de auditoría JSONL append-only con redacción de secretos.

Una línea por llamada a herramienta. Nunca se guardan contenidos de archivos:
las claves ``content``, ``text``, ``data`` y ``output`` se sustituyen por su
longitud. Las claves de API y los tokens se redactan.
"""

from __future__ import annotations

import json
import logging
import os
import re
import threading
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)

_CONTENT_KEYS = {"content", "text", "data", "output"}
_SECRET_KEY_PARTS = ("api_key", "apikey", "token", "password", "passwd", "secret")
_SK_PATTERN = re.compile(r"\bsk-[A-Za-z0-9][A-Za-z0-9_-]{2,}")
_BEARER_PATTERN = re.compile(r"(?i)\bBearer\s+[A-Za-z0-9._~+/=-]+")


def _redact_text(text: str) -> str:
    text = _SK_PATTERN.sub("[REDACTED]", text)
    text = _BEARER_PATTERN.sub("Bearer [REDACTED]", text)
    return text


def _content_size(value: Any) -> int:
    # DECISION: los textos cuentan caracteres; los contenedores, los
    # caracteres de su serialización JSON; el resto, su representación.
    if value is None:
        return 0
    if isinstance(value, str):
        return len(value)
    if isinstance(value, (bytes, bytearray)):
        return len(value)
    if isinstance(value, (list, tuple, dict, set)):
        try:
            return len(json.dumps(value, ensure_ascii=False, default=str))
        except (TypeError, ValueError):
            return len(str(value))
    return len(str(value))


class AuditLog:
    """Registro JSONL con cerrojo de hilos y rotación por tamaño."""

    def __init__(self, path: Path, *, max_bytes: int = 5_000_000, backups: int = 5) -> None:
        self.path = Path(path)
        self.max_bytes = max_bytes
        self.backups = backups
        # DECISION: EzAgent es un único proceso; el cerrojo de hilos evita
        # líneas entrelazadas sin añadir bloqueos de archivo multiplataforma.
        self._lock = threading.Lock()

    def log(
        self,
        *,
        turn_id: str,
        tool: str,
        level: int,
        args: dict[str, Any],
        decision: str,
        taint: bool,
        result_ok: bool,
        ms: float,
    ) -> None:
        record = {
            "ts": datetime.now(timezone.utc).isoformat(timespec="milliseconds"),
            "turn_id": turn_id,
            "tool": tool,
            "level": int(level),
            "args": self.redact(args),
            "decision": decision,
            "taint": bool(taint),
            "result_ok": bool(result_ok),
            "ms": round(float(ms), 1),
        }
        line = json.dumps(record, ensure_ascii=False, default=str) + "\n"
        with self._lock:
            self._rotate_if_needed(len(line.encode("utf-8")))
            try:
                self.path.parent.mkdir(parents=True, exist_ok=True)
                with self.path.open("a", encoding="utf-8", newline="\n") as handle:
                    handle.write(line)
            except OSError as exc:
                logger.warning("No se pudo escribir la auditoría en %s: %s", self.path, exc)

    def redact(self, value: Any) -> Any:
        """Copia ``value`` sustituyendo contenidos y secretos."""
        if isinstance(value, dict):
            redacted: dict[str, Any] = {}
            for key, item in value.items():
                name = str(key)
                normalized = name.lower().replace("-", "_")
                if normalized in _CONTENT_KEYS:
                    redacted[f"{normalized}_chars"] = _content_size(item)
                    continue
                if any(part in normalized for part in _SECRET_KEY_PARTS):
                    redacted[name] = "[REDACTED]"
                    continue
                redacted[name] = self.redact(item)
            return redacted
        if isinstance(value, (list, tuple)):
            return [self.redact(item) for item in value]
        if isinstance(value, str):
            return _redact_text(value)
        return value

    def _backup_path(self, index: int) -> Path:
        return self.path.with_name(f"{self.path.name}.{index}")

    def _rotate_if_needed(self, incoming_bytes: int) -> None:
        if self.backups <= 0:
            return
        try:
            current = self.path.stat().st_size
        except OSError:
            return
        if current + incoming_bytes <= self.max_bytes:
            return
        # Los backups se desplazan con os.replace; el más antiguo se
        # descarta al ser sobrescrito por el siguiente.
        for index in range(self.backups - 1, 0, -1):
            source = self._backup_path(index)
            if not source.exists():
                continue
            try:
                os.replace(source, self._backup_path(index + 1))
            except OSError as exc:
                logger.warning("No se pudo rotar %s: %s", source, exc)
        try:
            if self.path.exists():
                os.replace(self.path, self._backup_path(1))
        except OSError as exc:
            logger.warning("No se pudo rotar %s: %s", self.path, exc)
