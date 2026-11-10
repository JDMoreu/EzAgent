"""Un contexto de modelo por conversación del chat.

El historial que ve el LLM (mensajes, resumen) vive en el ``Messenger``. Al pasar de una
conversación a otra, el contexto de la que se deja se guarda aquí y el de la que se retoma se
restaura: así cada chat sigue donde se quedó, sin mezclar lo que se habló en los demás.

Si de una conversación no hay contexto en memoria (p. ej. tras reiniciar la app), se reconstruye
con el texto guardado en el historial; se pierde el detalle de las herramientas, no la charla.
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Any, Final, Protocol

from loguru import logger

from .session import ChatModel
from .store import ChatEntry, as_messages

# DECISION: 8 contextos cubren la lista visible; los demás se reconstruyen del historial.
MAX_SAVED: Final[int] = 8


class ContextHolder(Protocol):
    """Lo que se le pide al ``Messenger``: exportar e importar su contexto."""

    def reset_history(self) -> None: ...

    def export_context(self) -> tuple[list[dict[str, Any]], str, str]: ...

    def import_context(
        self, messages: list[dict[str, Any]], summary: str = "", last_user: str = ""
    ) -> None: ...


@dataclass(slots=True)
class SavedContext:
    messages: list[dict[str, Any]]
    summary: str = ""
    last_user: str = ""


@dataclass(slots=True)
class ChatContexts:
    """Reparte el contexto del modelo entre las conversaciones del chat."""

    chat: ChatModel
    holder: Callable[[], ContextHolder | None]
    saved: dict[int, SavedContext] = field(default_factory=dict)

    # ------------------------------------------------------------ internos
    def _remember(self, session: int, saved: SavedContext) -> None:
        """Deja ``session`` como el contexto más reciente y expulsa el más antiguo."""
        self.saved.pop(session, None)
        self.saved[session] = saved
        while len(self.saved) > MAX_SAVED:
            current = self.chat.current_session
            victim = next((s for s in self.saved if s != current), None)
            if victim is None:  # nunca se expulsa la conversación actual
                break
            del self.saved[victim]

    def _park_current(self) -> None:
        """Guarda el contexto de la conversación que se está dejando."""
        holder, session = self.holder(), self.chat.current_session
        if holder is None or session is None:
            return
        messages, summary, last_user = holder.export_context()
        self._remember(session, SavedContext(list(messages), summary, last_user))

    def _load(self, session: int) -> None:
        holder = self.holder()
        if holder is None:
            return
        found = self.saved.pop(session, None)
        if found is not None:
            holder.import_context(found.messages, found.summary, found.last_user)
            self._remember(session, found)
            return
        self.rebuild(session)

    # ------------------------------------------------------------ operaciones
    def rebuild(self, session: int | None = None) -> None:
        """Reconstruye el contexto de una conversación con lo guardado en el historial."""
        holder = self.holder()
        store = self.chat.store
        if holder is None or store is None:
            return
        entries: list[ChatEntry] = store.entries(session)
        messages = as_messages(entries)
        last_user = next((e.text for e in reversed(entries) if e.role == "user"), "")
        holder.import_context(messages, "", last_user)

    def new_chat(self) -> None:
        """Conversación nueva: la actual se guarda aparte y el modelo empieza de cero."""
        self._park_current()
        holder = self.holder()
        if holder is not None:
            holder.reset_history()
        self.chat.new_session()

    def switch(self, session: int) -> bool:
        """Retoma otra conversación con su propio contexto. ``False`` si no se pudo."""
        current = self.chat.current_session
        if current is None or session == current:
            return False
        self._park_current()
        if not self.chat.switch_session(session):
            self.saved.pop(current, None)  # no cambió nada: el modelo conserva su contexto
            return False
        self._load(session)
        logger.info("Chat: conversación {} retomada.", session)
        return True

    def delete(self, session: int) -> bool:
        """Borra una conversación y su contexto; si era la actual, el modelo empieza de cero."""
        was_current = session == self.chat.current_session
        if not self.chat.delete_session(session):
            return False
        self.saved.pop(session, None)
        holder = self.holder()
        if was_current and holder is not None:
            holder.reset_history()
        return True
