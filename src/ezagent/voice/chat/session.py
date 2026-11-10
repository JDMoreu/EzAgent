"""Conversación en pantalla: lo que ve el chat y lo que la app le va contando.

Recoge los mensajes escritos *y* los hablados (``source``), así el chat muestra el hilo
completo aunque se use la voz, y los guarda en el historial. Es seguro entre hilos: el
LLM y el TTS lo alimentan desde sus hilos y la ventana lo lee con ``snapshot``.
"""

from __future__ import annotations

import threading
import time
from dataclasses import dataclass, field

from .store import ChatEntry, ChatStore, SessionInfo, Source


@dataclass(slots=True)
class ChatMessage:
    """Un mensaje visible. El del asistente crece mientras llega en streaming."""

    id: int
    role: str  # «user» | «assistant»
    text: str
    source: str = "chat"
    t: float = field(default_factory=time.time)
    streaming: bool = False
    error: bool = False


@dataclass(frozen=True, slots=True)
class ConfirmCard:
    """Una confirmación pendiente («¿Seguro?») que se responde con Sí / No en el chat."""

    summary: str
    strong: bool


class ChatModel:
    """Estado del chat. ``version`` sube con cada cambio para que la vista sepa si repintar."""

    def __init__(self, store: ChatStore | None = None, voice_replies: bool = False) -> None:
        self.store = store
        self.voice_replies = bool(voice_replies)
        self._lock = threading.RLock()
        self._messages: list[ChatMessage] = []
        self._next_id = 1
        self._version = 0
        self._busy = False
        self._confirm: ConfirmCard | None = None
        self._viewing: int | None = None
        if store is not None:
            for entry in store.entries():
                self._push(entry.role, entry.text, entry.source, entry.t)

    # ------------------------------------------------------------- internos
    def _bump(self) -> None:
        self._version += 1

    def _push(self, role: str, text: str, source: str, t: float | None = None) -> ChatMessage:
        message = ChatMessage(
            self._next_id, role, text, source, t if t is not None else time.time()
        )
        self._next_id += 1
        self._messages.append(message)
        return message

    def _find(self, message_id: int) -> ChatMessage | None:
        for message in reversed(self._messages):
            if message.id == message_id:
                return message
        return None

    # ----------------------------------------------------------- escritura
    def add_user(self, text: str, source: Source = "chat") -> ChatMessage:
        """El usuario dijo o escribió algo (se guarda en el historial)."""
        with self._lock:
            self._viewing = None  # al escribir se vuelve a la conversación actual
            message = self._push("user", text, source)
            if self.store is not None:
                self.store.append("user", text, source)
            self._bump()
            return message

    def add_note(self, text: str) -> ChatMessage:
        """Una nota de la propia app («conversación compactada»): se ve, pero no se guarda."""
        with self._lock:
            message = self._push("system", text, "agent")
            self._bump()
            return message

    def begin_assistant(self, source: Source = "chat") -> ChatMessage:
        """Empieza una respuesta vacía que irá llenándose."""
        with self._lock:
            message = self._push("assistant", "", source)
            message.streaming = True
            self._busy = True
            self._bump()
            return message

    def append(self, message_id: int, delta: str) -> None:
        with self._lock:
            message = self._find(message_id)
            if message is not None and delta:
                message.text += delta
                self._bump()

    def finish(self, message_id: int) -> None:
        """La respuesta terminó: se guarda en el historial."""
        with self._lock:
            message = self._find(message_id)
            if message is None:
                return
            message.streaming = False
            message.text = message.text.strip()
            if not message.text:  # nada que mostrar: no queda una burbuja vacía
                self._messages.remove(message)
            elif self.store is not None:
                self.store.append("assistant", message.text, message.source)  # type: ignore[arg-type]
            self._busy = any(m.streaming for m in self._messages)
            self._bump()

    def fail(self, message_id: int, text: str) -> None:
        """La respuesta falló: se muestra el motivo (sin guardarlo como respuesta)."""
        with self._lock:
            message = self._find(message_id)
            if message is None:
                return
            message.streaming = False
            message.error = True
            message.text = text
            self._busy = any(m.streaming for m in self._messages)
            self._bump()

    def cancel(self, message_id: int) -> None:
        """Se interrumpió la respuesta: se conserva lo que llegó (o se quita si estaba vacía)."""
        with self._lock:
            message = self._find(message_id)
            if message is None:
                return
            message.streaming = False
            if message.text.strip():
                message.text = message.text.strip()
                if self.store is not None:
                    self.store.append("assistant", message.text, message.source)  # type: ignore[arg-type]
            else:
                self._messages.remove(message)
            self._busy = any(m.streaming for m in self._messages)
            self._bump()

    # ------------------------------------------------------ confirmaciones
    def ask_confirm(self, summary: str, strong: bool) -> None:
        with self._lock:
            self._confirm = ConfirmCard(summary, strong)
            self._bump()

    def clear_confirm(self) -> None:
        with self._lock:
            if self._confirm is not None:
                self._confirm = None
                self._bump()

    # ------------------------------------------------------ conversaciones
    def new_session(self) -> None:
        """Empieza una conversación nueva (la anterior queda en el historial)."""
        with self._lock:
            self._messages.clear()
            self._busy = False
            self._confirm = None
            self._viewing = None
            if self.store is not None:
                self.store.new_session()
            self._bump()

    def sessions(self) -> list[SessionInfo]:
        return self.store.sessions() if self.store is not None else []

    def view_session(self, session: int | None) -> None:
        """Muestra una conversación pasada (solo lectura); ``None`` vuelve a la actual."""
        with self._lock:
            current = self.store.session if self.store is not None else None
            self._viewing = None if session in (None, current) else session
            self._bump()

    @property
    def viewing(self) -> int | None:
        return self._viewing

    def set_voice_replies(self, enabled: bool) -> None:
        with self._lock:
            if self.voice_replies != bool(enabled):
                self.voice_replies = bool(enabled)
                self._bump()

    # -------------------------------------------------------------- lectura
    @property
    def version(self) -> int:
        return self._version

    @property
    def busy(self) -> bool:
        return self._busy

    @property
    def confirm(self) -> ConfirmCard | None:
        return self._confirm

    def snapshot(self) -> tuple[int, tuple[ChatMessage, ...]]:
        """``(versión, mensajes)`` de lo que se está viendo, copiados (la vista no bloquea a nadie)."""
        with self._lock:
            if self._viewing is not None and self.store is not None:
                shown = [_from_entry(i, e) for i, e in enumerate(self.store.entries(self._viewing))]
            else:
                shown = self._messages
            return self._version, tuple(
                ChatMessage(m.id, m.role, m.text, m.source, m.t, m.streaming, m.error)
                for m in shown
            )

    def last_assistant_text(self) -> str:
        with self._lock:
            for message in reversed(self._messages):
                if message.role == "assistant" and not message.error and message.text:
                    return message.text
            return ""


def _from_entry(index: int, entry: ChatEntry) -> ChatMessage:
    return ChatMessage(-(index + 1), entry.role, entry.text, entry.source, entry.t)
