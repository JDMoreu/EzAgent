"""Rótulos de los mensajes del chat: «TÚ» / «EZAGENT» arriba y la fecha y hora debajo.

Separate from ``chat_view`` so that module does not grow further; the view
passes its own ``_text`` and colours.
"""

from __future__ import annotations

from collections.abc import Callable

from ..chat.trace import stamp_for
from .text import baseline_top, cap_height
from .tokens import FS_MICRO, FontStyle

FS_STAMP = FontStyle("CHAT_STAMP", "Geist", 11, 400)  # fecha y hora bajo el mensaje
STAMP_H = 16.0  # alto reservado bajo cada mensaje del usuario y de EzAgent
STAMP_INSET = 4.0  # la hora se alinea con el borde de la burbuja, dentro de su curva

Draw = Callable[[str, FontStyle, tuple, float, float, str], None]


def stamp_height(msg) -> float:
    """Alto de la línea de fecha y hora bajo ``msg`` (también mientras se escribe, sin saltos)."""
    return STAMP_H if getattr(msg, "role", "") in ("user", "assistant") else 0.0


def draw_label(
    draw: Draw, label: str, x: float, y: float, color, scale: float, right: bool
) -> None:
    """Rótulo sobre el mensaje, colocado por su línea base (la tilde de «TÚ» lo bajaba)."""
    base = y + 2 + cap_height(FS_MICRO, scale)
    draw(label, FS_MICRO, color, x, baseline_top(label, FS_MICRO, base, scale), _align(right))


def draw_stamp(draw: Draw, msg, x: float, width: float, y: float, color, scale: float) -> None:
    """Fecha y hora bajo el mensaje, alineadas con el borde de su burbuja.

    ``x`` y ``width`` son los de la burbuja; ``y`` es su borde inferior. Los mensajes del
    usuario (a la derecha) alinean la hora a la derecha.
    """
    text = stamp_for(msg)
    if not text:
        return
    right = getattr(msg, "role", "") == "user"
    base = y + 4 + cap_height(FS_STAMP, scale)
    x = x + width - STAMP_INSET if right else x + STAMP_INSET
    draw(text, FS_STAMP, color, x, baseline_top(text, FS_STAMP, base, scale), _align(right))


def _align(right: bool) -> str:
    return "right" if right else "left"


__all__ = ["FS_STAMP", "STAMP_H", "draw_label", "draw_stamp", "stamp_height"]
