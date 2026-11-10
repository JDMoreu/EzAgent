"""Historial del chat: un archivo JSONL de solo añadir, legible y editable a mano.

Guarda lo que se escribe y lo que se habla (``source``), agrupado en
conversaciones (``session``). «Nueva conversación» solo añade una marca y retomar una
anterior otra; el historial completo sigue consultable. Una conversación solo se borra si el
usuario lo pide (``delete_session``), y entonces el archivo se reescribe entero y de forma
atómica (archivo temporal + ``os.replace``), nunca con ``unlink``.
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
        event = data.get("event")
        if event == "resume":  # se retomó una conversación anterior: esa es la actual
            self._session = session
            return
        if event == "session":
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

    def resume(self, session: int) -> bool:
        """Retoma una conversación anterior como la actual (lo que se escriba se añade a ella)."""
        with self._lock:
            if session == self._session:
                return True
            if not any(e.session == session for e in self._entries):
                return False
            self._session = session
            self._write(
                {"v": _VERSION, "t": round(time.time(), 3), "event": "resume", "session": session}
            )
            return True

    def delete_session(self, session: int) -> bool:
        """Borra una conversación del historial (y del disco). ``False`` si no se pudo guardar.

        Si es la actual, queda vacía pero sigue siendo la actual (se numera igual).
        """
        with self._lock:
            kept = [e for e in self._entries if e.session != session]
            starts = {
                k: v for k, v in self._starts.items() if k != session or session == self._session
            }
            if len(kept) == len(self._entries) and session not in self._starts:
                return False
            if not self._rewrite(kept, starts):
                return False
            self._entries, self._starts = kept, starts
            return True

    def _rewrite(self, entries: list[ChatEntry], starts: dict[int, float]) -> bool:
        """Reescribe el archivo con ``entries``: a un temporal y luego ``os.replace`` (atómico)."""
        lines: list[str] = []
        for session in sorted(starts):
            lines.append(
                json.dumps(
                    {
                        "v": _VERSION,
                        "t": round(starts[session], 3),
                        "event": "session",
                        "session": session,
                    },
                    ensure_ascii=False,
                )
            )
        for e in entries:
            lines.append(
                json.dumps(
                    {
                        "v": _VERSION,
                        "t": round(e.t, 3),
                        "session": e.session,
                        "role": e.role,
                        "source": e.source,
                        "text": e.text,
                    },
                    ensure_ascii=False,
                )
            )
        if self._session != max(starts, default=self._session):  # la actual no es la última
            lines.append(
                json.dumps(
                    {
                        "v": _VERSION,
                        "t": round(time.time(), 3),
                        "event": "resume",
                        "session": self._session,
                    }
                )
            )
        tmp = self.path.with_name(self.path.name + ".tmp")
        try:
            self.path.parent.mkdir(parents=True, exist_ok=True)
            tmp.write_text(
                "\n".join(lines) + ("\n" if lines else ""), encoding="utf-8", newline="\n"
            )
            os.replace(tmp, self.path)
        except OSError as exc:
            logger.warning("No pude borrar la conversación del historial: {}", exc)
            return False
        return True

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
        """Conversaciones con mensajes, de la usada más recientemente a la más antigua."""
        with self._lock:
            out: list[tuple[float, SessionInfo]] = []
            for session in {e.session for e in self._entries}:
                items = [e for e in self._entries if e.session == session]
                first_user = next((e.text for e in items if e.role == "user"), items[0].text)
                info = SessionInfo(
                    session=session,
                    started=self._starts.get(session, items[0].t),
                    messages=len(items),
                    title=_title(first_user),
                )
                out.append((max(e.t for e in items), info))
            # La que se usó hace menos va primero (retomar una antigua la sube).
            out.sort(key=lambda pair: (pair[0], pair[1].session), reverse=True)
            return [info for _, info in out]

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
