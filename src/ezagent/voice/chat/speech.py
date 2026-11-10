"""La versión hablada de una respuesta del chat.

Una respuesta del chat es markdown: tablas, código, enlaces… Leerla tal cual por voz sería
ruido («asterisco asterisco…»). Aquí se saca lo que sí se puede decir —las frases— y se
avisa de lo que se queda en pantalla («el detalle está en el chat»).
"""

from __future__ import annotations

import re
from typing import Final

from . import markdown as md

MAX_SPOKEN_CHARS: Final[int] = 460
MAX_LIST_ITEMS: Final[int] = 5
SCREEN_NOTE: Final[str] = "El detalle está en el chat."
_SENTENCE_END = re.compile(r"[.!?…:]$")
_SENTENCES = re.compile(r"(?<=[.!?…])\s+")


def _text(spans: tuple[md.Span, ...]) -> str:
    return " ".join("".join(span.text for span in spans).split())


def _sentence(text: str) -> str:
    text = text.strip()
    if text and not _SENTENCE_END.search(text):
        text += "."
    return text


def spoken_version(markdown: str, max_chars: int = MAX_SPOKEN_CHARS) -> str:
    """Texto para decir en voz alta; vacío si no hay nada que decir."""
    parts: list[str] = []
    rich = False
    items = 0
    for block in md.parse(markdown):
        if isinstance(block, (md.Heading, md.Paragraph, md.Quote)):
            parts.append(_sentence(_text(block.spans)))
        elif isinstance(block, md.ListItem):
            items += 1
            if items <= MAX_LIST_ITEMS:
                parts.append(_sentence(_text(block.spans)))
            else:
                rich = True
        elif isinstance(block, (md.Code, md.Table, md.Image, md.Document)):
            rich = True
    spoken, clipped = _fit(" ".join(p for p in parts if p), max_chars)
    if spoken and (rich or clipped):
        spoken += " " + SCREEN_NOTE
    elif not spoken and rich:
        spoken = "Te lo dejé en el chat."
    return spoken


def _fit(text: str, limit: int) -> tuple[str, bool]:
    """Recorta en una frontera de frase para no cortar a media palabra."""
    if len(text) <= limit:
        return text, False
    kept = ""
    for sentence in _SENTENCES.split(text):
        candidate = (kept + " " + sentence).strip()
        if len(candidate) > limit:
            break
        kept = candidate
    if not kept:  # una sola frase larguísima: se corta en un espacio
        kept = text[:limit].rsplit(" ", 1)[0].rstrip(",;:") + "."
    return kept, True
