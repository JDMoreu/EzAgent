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
from dataclasses import dataclass, replace
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
    seq: int = 0  # orden persistido (no depende del reloj); 0 solo si se crea a mano


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
        self._by_session: dict[int, list[ChatEntry]] = {}
        self._starts: dict[int, float] = {}
        self._activity: dict[int, int] = {}
        self._seq = 0
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
        for index, line in enumerate(lines, start=1):
            if not line.strip():
                continue
            try:
                self._ingest(json.loads(line), index)
            except (ValueError, TypeError, KeyError):
                bad += 1
        if bad:
            logger.warning("Historial del chat: {} línea(s) ilegibles omitidas.", bad)

    def _ingest(self, data: dict, index: int) -> None:
        session = int(data["session"])
        seq = _seq_of(data, index)
        self._seq = max(self._seq, seq)
        self._activity[session] = max(self._activity.get(session, 0), seq)
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
        entry = ChatEntry(
            t=float(data.get("t", 0.0)),
            session=session,
            role=role,  # type: ignore[arg-type]
            source=source if source in ("chat", "voice", "agent") else "chat",  # type: ignore[arg-type]
            text=str(data["text"]),
            seq=seq,
        )
        self._entries.append(entry)
        self._by_session.setdefault(session, []).append(entry)

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

    def _next_seq(self) -> int:
        self._seq += 1
        return self._seq

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
                seq=self._next_seq(),
            )
            self._starts.setdefault(entry.session, entry.t)
            self._entries.append(entry)
            self._by_session.setdefault(entry.session, []).append(entry)
            self._activity[entry.session] = entry.seq
            self._write(
                {
                    "v": _VERSION,
                    "t": round(entry.t, 3),
                    "session": entry.session,
                    "role": role,
                    "source": source,
                    "text": clean,
                    "seq": entry.seq,
                }
            )
        return entry

    def new_session(self) -> int:
        """Empieza otra conversación (las anteriores se conservan)."""
        with self._lock:
            self._session += 1
            now = time.time()
            seq = self._next_seq()
            self._starts[self._session] = now
            self._activity[self._session] = seq
            self._write(
                {
                    "v": _VERSION,
                    "t": round(now, 3),
                    "event": "session",
                    "session": self._session,
                    "seq": seq,
                }
            )
            return self._session

    def resume(self, session: int) -> bool:
        """Retoma una conversación anterior como la actual (lo que se escriba se añade a ella)."""
        with self._lock:
            if session == self._session:
                return True
            if session not in self._by_session:
                return False
            self._session = session
            seq = self._next_seq()
            self._activity[session] = seq
            self._write(
                {
                    "v": _VERSION,
                    "t": round(time.time(), 3),
                    "event": "resume",
                    "session": session,
                    "seq": seq,
                }
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
            rewritten = self._rewrite(kept, starts)
            if rewritten is None:
                return False
            entries, activity = rewritten
            self._entries, self._starts, self._activity = entries, starts, activity
            self._by_session = _index(entries)
            self._seq = max(activity.values(), default=0)
            return True

    def _rewrite(
        self, entries: list[ChatEntry], starts: dict[int, float]
    ) -> tuple[list[ChatEntry], dict[int, int]] | None:
        """Reescribe el archivo con ``entries``: a un temporal y luego ``os.replace`` (atómico).

        Devuelve las entradas y la actividad con su ``seq`` renumerado, o ``None`` si falla.
        """
        lines: list[str] = []
        rewritten: list[ChatEntry] = []
        activity: dict[int, int] = {}
        seq = 0
        for session in sorted(starts):
            seq += 1
            activity[session] = seq
            lines.append(
                json.dumps(
                    {
                        "v": _VERSION,
                        "t": round(starts[session], 3),
                        "event": "session",
                        "session": session,
                        "seq": seq,
                    },
                    ensure_ascii=False,
                )
            )
        for entry in entries:
            seq += 1
            activity[entry.session] = seq
            entry = replace(entry, seq=seq)
            rewritten.append(entry)
            lines.append(
                json.dumps(
                    {
                        "v": _VERSION,
                        "t": round(entry.t, 3),
                        "session": entry.session,
                        "role": entry.role,
                        "source": entry.source,
                        "text": entry.text,
                        "seq": seq,
                    },
                    ensure_ascii=False,
                )
            )
        if self._session != max(starts, default=self._session):  # la actual no es la última
            seq += 1
            activity[self._session] = seq
            lines.append(
                json.dumps(
                    {
                        "v": _VERSION,
                        "t": round(time.time(), 3),
                        "event": "resume",
                        "session": self._session,
                        "seq": seq,
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
            return None
        return rewritten, activity

    # -------------------------------------------------------------- lectura
    @property
    def session(self) -> int:
        return self._session

    def entries(self, session: int | None = None) -> list[ChatEntry]:
        """Mensajes de una conversación (la actual por defecto), en orden."""
        with self._lock:
            wanted = self._session if session is None else session
            return list(self._by_session.get(wanted, []))

    def recent(self, limit: int = 200) -> list[ChatEntry]:
        """Los últimos ``limit`` mensajes de todas las conversaciones."""
        with self._lock:
            return list(self._entries[-max(0, limit) :])

    def sessions(self) -> list[SessionInfo]:
        """Conversaciones con mensajes, de la usada más recientemente a la más antigua."""
        with self._lock:
            out: list[SessionInfo] = []
            for session, items in self._by_session.items():
                if not items:
                    continue
                first_user = next((e.text for e in items if e.role == "user"), items[0].text)
                out.append(
                    SessionInfo(
                        session=session,
                        started=self._starts.get(session, items[0].t),
                        messages=len(items),
                        title=_title(first_user),
                    )
                )
            # La actividad es una secuencia persistida: retomar una antigua la sube sin
            # depender del reloj ni de empates.
            out.sort(
                key=lambda info: (self._activity.get(info.session, 0), info.session),
                reverse=True,
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


def _seq_of(data: dict, index: int) -> int:
    """``seq`` guardado o, en archivos antiguos sin él, el índice de línea."""
    try:
        return int(data["seq"])
    except (KeyError, TypeError, ValueError):
        return index


def _index(entries: Iterable[ChatEntry]) -> dict[int, list[ChatEntry]]:
    by_session: dict[int, list[ChatEntry]] = {}
    for entry in entries:
        by_session.setdefault(entry.session, []).append(entry)
    return by_session


def _title(text: str, limit: int = 60) -> str:
    flat = " ".join(str(text).split())
    return flat if len(flat) <= limit else flat[: limit - 1].rstrip() + "…"


def as_messages(entries: Iterable[ChatEntry]) -> list[dict[str, str]]:
    """Mensajes en el formato del modelo (para reanudar una conversación)."""
    return [{"role": e.role, "content": e.text} for e in entries if e.role in ("user", "assistant")]
