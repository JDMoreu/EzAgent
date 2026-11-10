"""Historial del chat: un archivo JSONL de solo añadir, legible y editable a mano.

Guarda lo que se escribe y lo que se habla (``source``), agrupado en
conversaciones (``session``). Nunca se borra nada: «nueva conversación» solo
añade una marca, y el historial completo sigue consultable.
"""

from __future__ import annotations

import json
import os
import threading
import time
from collections.abc import Iterable
from dataclasses import dataclass
from pathlib import Path
from typing import Final, Literal

from loguru import logger

Role = Literal["user", "assistant", "system"]
Source = Literal["chat", "voice", "agent"]

MAX_ENTRY_CHARS: Final[int] = 200_000
_VERSION: Final[int] = 1


def default_chat_path() -> Path:
    """``%APPDATA%/EzAgent/history/chat.jsonl`` (junto a ``voice.json`` y ``memory``)."""
    base = os.getenv("APPDATA") or os.getenv("XDG_CONFIG_HOME") or str(Path.home() / ".config")
    return Path(base) / "EzAgent" / "history" / "chat.jsonl"


@dataclass(frozen=True, slots=True)
class ChatEntry:
    """Un mensaje del historial."""

    t: float  # epoch, segundos
    session: int
    role: Role
    source: Source
    text: str


@dataclass(frozen=True, slots=True)
class SessionInfo:
    """Resumen de una conversación para la lista del historial."""

    session: int
    started: float
    messages: int
    title: str  # primer mensaje del usuario, recortado


class ChatStore:
    """Historial persistente y seguro entre hilos."""

    def __init__(self, path: Path | None = None) -> None:
        self.path = Path(path) if path is not None else default_chat_path()
        self._lock = threading.RLock()
        self._entries: list[ChatEntry] = []
        self._starts: dict[int, float] = {}
        self._session = 1
        self._load()

    # ---------------------------------------------------------------- carga
    def _load(self) -> None:
        try:
            lines = self.path.read_text(encoding="utf-8", errors="replace").splitlines()
        except FileNotFoundError:
            return
        except OSError as exc:
            logger.warning("No pude leer el historial del chat: {}", exc)
            return
        bad = 0
        for line in lines:
            if not line.strip():
                continue
            try:
                self._ingest(json.loads(line))
            except (ValueError, TypeError, KeyError):
                bad += 1
        if bad:
            logger.warning("Historial del chat: {} línea(s) ilegibles omitidas.", bad)

    def _ingest(self, data: dict) -> None:
        session = int(data["session"])
        self._starts.setdefault(session, float(data.get("t", 0.0)))
        self._session = max(self._session, session)
        if data.get("event") == "session":
            return
        role = str(data["role"])
        if role not in ("user", "assistant", "system"):
            raise ValueError("rol desconocido")
        source = str(data.get("source", "chat"))
        self._entries.append(
            ChatEntry(
                t=float(data.get("t", 0.0)),
                session=session,
                role=role,  # type: ignore[arg-type]
                source=source if source in ("chat", "voice", "agent") else "chat",  # type: ignore[arg-type]
                text=str(data["text"]),
            )
        )

    # ------------------------------------------------------------ escritura
    def _write(self, record: dict) -> None:
        try:
            self.path.parent.mkdir(parents=True, exist_ok=True)
            line = json.dumps(record, ensure_ascii=False)
            with self.path.open("a", encoding="utf-8", newline="\n") as handle:
                handle.write(line + "\n")
        except OSError as exc:
            # El chat sigue funcionando en memoria aunque el disco falle.
            logger.warning("No pude guardar el historial del chat: {}", exc)

    def append(
        self,
        role: Role,
        text: str,
        source: Source = "chat",
        t: float | None = None,
    ) -> ChatEntry:
        """Añade un mensaje a la conversación actual y lo guarda."""
        clean = str(text or "")[:MAX_ENTRY_CHARS]
        with self._lock:
            entry = ChatEntry(
                t=time.time() if t is None else float(t),
                session=self._session,
                role=role,
                source=source,
                text=clean,
            )
            self._starts.setdefault(entry.session, entry.t)
            self._entries.append(entry)
            self._write(
                {
                    "v": _VERSION,
                    "t": round(entry.t, 3),
                    "session": entry.session,
                    "role": role,
                    "source": source,
                    "text": clean,
                }
            )
        return entry

    def new_session(self) -> int:
        """Empieza otra conversación (las anteriores se conservan)."""
        with self._lock:
            self._session += 1
            now = time.time()
            self._starts[self._session] = now
            self._write(
                {"v": _VERSION, "t": round(now, 3), "event": "session", "session": self._session}
            )
            return self._session

    # -------------------------------------------------------------- lectura
    @property
    def session(self) -> int:
        return self._session

    def entries(self, session: int | None = None) -> list[ChatEntry]:
        """Mensajes de una conversación (la actual por defecto), en orden."""
        with self._lock:
            wanted = self._session if session is None else session
            return [entry for entry in self._entries if entry.session == wanted]

    def recent(self, limit: int = 200) -> list[ChatEntry]:
        """Los últimos ``limit`` mensajes de todas las conversaciones."""
        with self._lock:
            return list(self._entries[-max(0, limit) :])

    def sessions(self) -> list[SessionInfo]:
        """Conversaciones con mensajes, de la más reciente a la más antigua."""
        with self._lock:
            out: list[SessionInfo] = []
            for session in sorted({e.session for e in self._entries}, reverse=True):
                items = [e for e in self._entries if e.session == session]
                first_user = next((e.text for e in items if e.role == "user"), items[0].text)
                out.append(
                    SessionInfo(
                        session=session,
                        started=self._starts.get(session, items[0].t),
                        messages=len(items),
                        title=_title(first_user),
                    )
                )
            return out

    def search(self, query: str, limit: int = 50) -> list[ChatEntry]:
        """Mensajes que contienen ``query`` (sin distinguir mayúsculas), los más recientes primero."""
        needle = str(query or "").strip().casefold()
        if not needle:
            return []
        with self._lock:
            found = [e for e in reversed(self._entries) if needle in e.text.casefold()]
        return found[: max(0, limit)]

    def __len__(self) -> int:
        return len(self._entries)


def _title(text: str, limit: int = 60) -> str:
    flat = " ".join(str(text).split())
    return flat if len(flat) <= limit else flat[: limit - 1].rstrip() + "…"


def as_messages(entries: Iterable[ChatEntry]) -> list[dict[str, str]]:
    """Mensajes en el formato del modelo (para reanudar una conversación)."""
    return [{"role": e.role, "content": e.text} for e in entries if e.role in ("user", "assistant")]
