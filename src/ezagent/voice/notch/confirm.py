"""Fila de confirmación del notch.

La isla crece con el menú para mostrar el resumen que ha generado el código y
dos botones, Sí y No. Este módulo solo pinta y resuelve el hit-test de la fila;
la geometría de la ventana y la silueta viven en :mod:`island` y
:mod:`silhouette`.

El resumen nunca lo escribe el LLM: llega ya construido desde
``agent.registry.ToolSpec.describe``.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Final, Literal

import numpy as np

from .menu import _fill_round_rect, _stroke_round_rect
from .text import draw_text, text_size
from .tokens import (
    FS_BODY,
    FS_MICRO,
    FS_SECONDARY,
    R_GROUP,
    Rect,
    palette,
)

CONFIRM_W: Final[float] = 300.0
CONFIRM_H: Final[float] = 88.0
CONFIRM_PAD: Final[float] = 16.0
CONFIRM_BUTTON_W: Final[float] = 72.0
CONFIRM_BUTTON_H: Final[float] = 28.0
CONFIRM_BUTTON_GAP: Final[float] = 8.0
CONFIRM_BUTTON_BOTTOM: Final[float] = 8.0
CONFIRM_SUMMARY_MAX_W: Final[float] = CONFIRM_W - CONFIRM_PAD * 2.0
CONFIRM_SUMMARY_LINE1: Final[float] = 12.0
CONFIRM_SUMMARY_LINE2: Final[float] = 30.0
CONFIRM_STRONG_LINE: Final[float] = 2.0
CONFIRM_SUMMARY_STRONG1: Final[float] = 18.0
CONFIRM_SUMMARY_STRONG2: Final[float] = 36.0
CONFIRM_BUTTON_Y: Final[float] = CONFIRM_H - CONFIRM_BUTTON_BOTTOM - CONFIRM_BUTTON_H

ConfirmAction = Literal["yes", "no"]


@dataclass(slots=True)
class _Pending:
    summary: str
    strong: bool
    on_yes: Callable[[], None]
    on_no: Callable[[], None]


def _wrap_lines(
    text: str,
    style,
    max_width: float,
    scale: float,
    max_lines: int = 2,
) -> list[str]:
    words = str(text or "").split()
    if not words:
        return []
    lines: list[str] = []
    current = ""
    for word in words:
        candidate = f"{current} {word}".strip()
        if not current or text_size(candidate, style, scale)[0] <= max_width:
            current = candidate
        else:
            lines.append(current)
            current = word
            if len(lines) == max_lines:
                break
    if current and len(lines) < max_lines:
        lines.append(current)
    if len(lines) > max_lines:
        lines = lines[:max_lines]
    # Si quedó texto fuera, la última línea termina en puntos suspensivos.
    rendered = " ".join(lines)
    if rendered and len(rendered) < len(" ".join(str(text or "").split())):
        last = lines[-1]
        while last and text_size(last + "…", style, scale)[0] > max_width:
            last = last[:-1].rstrip()
        lines[-1] = (last + "…") if last else "…"
    return lines


class ConfirmPanel:
    """Contenido de la fila de confirmación; no conoce la ventana."""

    def __init__(self, scale: float = 1.0, theme: str = "dark") -> None:
        self.scale = max(0.5, float(scale))
        self.theme = theme if theme in ("dark", "light") else "dark"
        self._pending: _Pending | None = None

    @property
    def visible(self) -> bool:
        return self._pending is not None

    @property
    def width(self) -> float:
        return CONFIRM_W

    @property
    def height(self) -> float:
        return CONFIRM_H

    def set_scale(self, scale: float) -> None:
        self.scale = max(0.5, float(scale))

    def set_theme(self, theme: str) -> None:
        self.theme = theme if theme in ("dark", "light") else "dark"

    def show(
        self,
        summary: str,
        strong: bool,
        on_yes: Callable[[], None],
        on_no: Callable[[], None],
    ) -> None:
        self._pending = _Pending(
            summary=str(summary or ""),
            strong=bool(strong),
            on_yes=on_yes,
            on_no=on_no,
        )

    def hide(self) -> None:
        self._pending = None

    def buttons(self) -> tuple[Rect, Rect]:
        """Rectángulos locales de los botones No y Sí."""
        total = CONFIRM_BUTTON_W * 2.0 + CONFIRM_BUTTON_GAP
        x0 = CONFIRM_W - CONFIRM_PAD - total
        no = Rect(
            x0,
            CONFIRM_BUTTON_Y,
            x0 + CONFIRM_BUTTON_W,
            CONFIRM_BUTTON_Y + CONFIRM_BUTTON_H,
        )
        yes = Rect(
            no.x1 + CONFIRM_BUTTON_GAP,
            CONFIRM_BUTTON_Y,
            no.x1 + CONFIRM_BUTTON_GAP + CONFIRM_BUTTON_W,
            CONFIRM_BUTTON_Y + CONFIRM_BUTTON_H,
        )
        return no, yes

    def hit(self, x: float, y: float) -> ConfirmAction | None:
        if self._pending is None:
            return None
        no, yes = self.buttons()
        if yes.x0 <= x <= yes.x1 and yes.y0 <= y <= yes.y1:
            return "yes"
        if no.x0 <= x <= no.x1 and no.y0 <= y <= no.y1:
            return "no"
        return None

    def activate(self, action: ConfirmAction) -> None:
        pending = self._pending
        if pending is None:
            return
        self._pending = None
        try:
            if action == "yes":
                pending.on_yes()
            else:
                pending.on_no()
        except Exception:
            pass

    def draw(
        self,
        layer: np.ndarray,
        origin_x: float,
        origin_y: float,
        *,
        alpha: float = 1.0,
    ) -> None:
        pending = self._pending
        if pending is None:
            return
        pal = palette(self.theme)
        scale = self.scale
        strength = max(0.0, min(1.0, float(alpha)))
        if strength <= 0.001:
            return
        if pending.strong:
            draw_text(
                layer,
                "CONFIRMACIÓN FUERTE",
                FS_MICRO,
                pal["WARM"],
                origin_x + CONFIRM_PAD,
                origin_y + CONFIRM_STRONG_LINE,
                scale,
                alpha=strength,
            )
            line1 = CONFIRM_SUMMARY_STRONG1
            line2 = CONFIRM_SUMMARY_STRONG2
        else:
            line1 = CONFIRM_SUMMARY_LINE1
            line2 = CONFIRM_SUMMARY_LINE2
        lines = _wrap_lines(
            pending.summary, FS_BODY, CONFIRM_SUMMARY_MAX_W, scale, 2
        )
        for index, line in enumerate(lines[:2]):
            draw_text(
                layer,
                line,
                FS_BODY,
                pal["TEXT"],
                origin_x + CONFIRM_PAD,
                origin_y + (line1 if index == 0 else line2),
                scale,
                alpha=strength,
            )
        no, yes = self.buttons()
        label_style = FS_SECONDARY
        # No: superficie elevada con hairline.
        _fill_round_rect(
            layer,
            origin_x + no.x0,
            origin_y + no.y0,
            origin_x + no.x1,
            origin_y + no.y1,
            R_GROUP,
            pal["GROUP"],
            scale,
            alpha=strength,
        )
        _stroke_round_rect(
            layer,
            origin_x + no.x0,
            origin_y + no.y0,
            origin_x + no.x1,
            origin_y + no.y1,
            R_GROUP,
            pal["LINE"],
            scale,
            alpha=strength,
        )
        draw_text(
            layer,
            "No",
            label_style,
            pal["TEXT_2"],
            origin_x + (no.x0 + no.x1) / 2.0,
            origin_y + (no.y0 + no.y1) / 2.0 - 7.0,
            scale,
            align="center",
            alpha=strength,
        )
        # Sí: acción principal mint.
        _fill_round_rect(
            layer,
            origin_x + yes.x0,
            origin_y + yes.y0,
            origin_x + yes.x1,
            origin_y + yes.y1,
            R_GROUP,
            pal["MINT_BTN"],
            scale,
            alpha=strength,
        )
        draw_text(
            layer,
            "Sí",
            label_style,
            pal["MINT_INK"],
            origin_x + (yes.x0 + yes.x1) / 2.0,
            origin_y + (yes.y0 + yes.y1) / 2.0 - 7.0,
            scale,
            align="center",
            alpha=strength,
        )


__all__ = [
    "CONFIRM_H",
    "CONFIRM_W",
    "ConfirmAction",
    "ConfirmPanel",
]
