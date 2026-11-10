"""Campo de texto del chat: varias líneas, cursor, selección y pegado, sin dibujo ni Win32.

La ventana le pasa las teclas y él decide qué cambia; así se puede probar entero. Los
caracteres de control no entran (ni siquiera pegados) y el texto tiene un tope.
"""

from __future__ import annotations

import re
from typing import Final, Literal

MAX_CHARS: Final[int] = 20_000
Move = Literal["left", "right", "up", "down", "home", "end"]
_CONTROL = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]")
_WORD = re.compile(r"\w", re.UNICODE)


def sanitize(text: str) -> str:
    """Normaliza saltos de línea, tabuladores y quita caracteres de control."""
    clean = str(text or "").replace("\r\n", "\n").replace("\r", "\n").replace("\t", "    ")
    return _CONTROL.sub("", clean)


class TextInput:
    """Texto editable con cursor y selección (entre ``anchor`` y ``cursor``)."""

    def __init__(self, text: str = "") -> None:
        self.text = ""
        self.cursor = 0
        self.anchor: int | None = None
        if text:
            self.insert(text)

    # ------------------------------------------------------------ selección
    def selection(self) -> tuple[int, int] | None:
        if self.anchor is None or self.anchor == self.cursor:
            return None
        return (min(self.anchor, self.cursor), max(self.anchor, self.cursor))

    def selected_text(self) -> str:
        span = self.selection()
        return self.text[span[0] : span[1]] if span else ""

    def select_all(self) -> None:
        self.anchor, self.cursor = 0, len(self.text)

    def _delete_selection(self) -> bool:
        span = self.selection()
        self.anchor = None
        if span is None:
            return False
        self.text = self.text[: span[0]] + self.text[span[1] :]
        self.cursor = span[0]
        return True

    # -------------------------------------------------------------- edición
    def insert(self, text: str) -> None:
        """Escribe o pega ``text`` en el cursor (sustituye la selección)."""
        clean = sanitize(text)
        if not clean:
            return
        self._delete_selection()
        room = MAX_CHARS - len(self.text)
        if room <= 0:
            return
        clean = clean[:room]
        self.text = self.text[: self.cursor] + clean + self.text[self.cursor :]
        self.cursor += len(clean)

    def newline(self) -> None:
        self.insert("\n")

    def backspace(self, word: bool = False) -> None:
        if self._delete_selection() or self.cursor == 0:
            return
        start = self._word_left(self.cursor) if word else self.cursor - 1
        self.text = self.text[:start] + self.text[self.cursor :]
        self.cursor = start

    def delete(self, word: bool = False) -> None:
        if self._delete_selection() or self.cursor >= len(self.text):
            return
        end = self._word_right(self.cursor) if word else self.cursor + 1
        self.text = self.text[: self.cursor] + self.text[end:]

    def clear(self) -> None:
        self.text, self.cursor, self.anchor = "", 0, None

    def take(self) -> str:
        """El texto para enviar (sin espacios sobrantes); el campo queda vacío."""
        value = self.text.strip()
        self.clear()
        return value

    def set_text(self, text: str) -> None:
        self.clear()
        self.insert(text)

    # ----------------------------------------------------------- movimiento
    def move(self, key: Move, select: bool = False, word: bool = False) -> None:
        """Mueve el cursor; con ``select`` extiende la selección, sin ella la deshace."""
        if select:
            if self.anchor is None:
                self.anchor = self.cursor
        elif self.selection() is not None and key in ("left", "right"):
            span = self.selection()
            self.cursor = span[0] if key == "left" else span[1]  # type: ignore[index]
            self.anchor = None
            return
        else:
            self.anchor = None
        self.cursor = self._target(key, word)

    def _target(self, key: Move, word: bool) -> int:
        text, pos = self.text, self.cursor
        if key == "left":
            return self._word_left(pos) if word else max(0, pos - 1)
        if key == "right":
            return self._word_right(pos) if word else min(len(text), pos + 1)
        start = text.rfind("\n", 0, pos) + 1
        if key == "home":
            return start
        if key == "end":
            end = text.find("\n", pos)
            return len(text) if end < 0 else end
        col = pos - start
        if key == "up":
            if start == 0:
                return 0
            prev_start = text.rfind("\n", 0, start - 1) + 1
            return min(prev_start + col, start - 1)
        nxt = text.find("\n", pos)  # down
        if nxt < 0:
            return len(text)
        end = text.find("\n", nxt + 1)
        end = len(text) if end < 0 else end
        return min(nxt + 1 + col, end)

    def _word_left(self, pos: int) -> int:
        while pos > 0 and not _WORD.match(self.text[pos - 1]):
            pos -= 1
        while pos > 0 and _WORD.match(self.text[pos - 1]):
            pos -= 1
        return pos

    def _word_right(self, pos: int) -> int:
        n = len(self.text)
        while pos < n and not _WORD.match(self.text[pos]):
            pos += 1
        while pos < n and _WORD.match(self.text[pos]):
            pos += 1
        return pos

    # ------------------------------------------------------------- consulta
    def line_col(self) -> tuple[int, int]:
        """``(línea, columna)`` del cursor, contando líneas lógicas (separadas por salto)."""
        before = self.text[: self.cursor]
        return before.count("\n"), len(before) - (before.rfind("\n") + 1)

    @property
    def empty(self) -> bool:
        return not self.text.strip()
