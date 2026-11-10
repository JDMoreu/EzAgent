"""Cabecera, historial y estado vacío de la vista del chat (separados de ``chat_view``).

``HeaderMixin`` se mezcla en ``ChatView`` y usa su estado (``g``, ``model``, ``hover``, ``mode``…).
"""

from __future__ import annotations

import time

import numpy as np

from .chat_footer import BTN, PAD
from .icons import draw_icon
from .menu import _fill_circle, _fill_round_rect
from .text import draw_text  # noqa: F401  (los métodos usan ``self._text``)
from .tokens import FS_MICRO, FS_ROW, FS_SECONDARY, FS_TITLE

HEADER_H = 56.0
ORB_D = 28.0  # diámetro (px lógicos) del orbe vivo que acompaña al título
_MONTHS = ("ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic")


def _date(t: float) -> str:
    lt = time.localtime(t)
    return f"{lt.tm_mday} {_MONTHS[lt.tm_mon - 1]} {lt.tm_year}"


class HeaderMixin:
    """Cabecera con botones, lista de conversaciones y pantalla de bienvenida."""

    def _draw_empty(self, buf, top: float, bottom: float) -> None:
        mid = (top + bottom) / 2 - 18
        cx = self.g.width / 2
        self._text(
            buf, "¿En qué te ayudo?", FS_TITLE, self._color("TEXT"), cx, mid - 14, align="center"
        )
        lines = (
            "Escribe aquí lo que necesites.",
            "Puedo resumir, comparar y mostrarte",
            "documentos e imágenes.",
        )
        for i, line in enumerate(lines):
            self._text(
                buf,
                line,
                FS_SECONDARY,
                self._color("TEXT_3"),
                cx,
                mid + 14 + i * 18,
                align="center",
            )
        hint = f"{self.hotkey_label} abre y cierra el chat"
        self._text(buf, hint, FS_MICRO, self._color("TEXT_3", 0.8), cx, mid + 82, align="center")

    # -------------------------------------------------------------- historial
    def _draw_history(self, buf, top: float, bottom: float) -> None:
        sessions = self.model.sessions()
        if not sessions:
            self._text(
                buf,
                "Aún no hay conversaciones guardadas.",
                FS_ROW,
                self._color("TEXT_3"),
                PAD,
                top + 24,
            )
            return
        y = top + 8 - self.scroll
        total = 8.0
        for info in sessions:
            rect = (8.0, y, self.g.width - 8, y + 58)
            total += 60
            if rect[3] > top and rect[1] < bottom:
                if self.hover == f"session:{info.session}":
                    _fill_round_rect(buf, *rect, 12.0, self._color("GROUP_HOVER")[:3], self.g.scale)
                title = info.title if len(info.title) < 44 else info.title[:43] + "…"
                self._text(buf, title, FS_ROW, self._color("TEXT"), 20, y + 11)
                meta = f"{_date(info.started)} · {info.messages} mensajes"
                self._text(buf, meta, FS_MICRO, self._color("TEXT_3"), 20, y + 35)
                self._hit(rect, "session", info.session)
            y += 60
        self._max_scroll = max(0.0, total - (bottom - top))
        self.scroll = max(0.0, min(self.scroll, self._max_scroll))

    # ------------------------------------------------------------- cabecera
    def _draw_header(self, buf) -> None:
        g, sc = self.g, self.g.scale
        cy = HEADER_H / 2
        viewing = self.model.viewing is not None
        back = self.mode == "history" or viewing
        x = PAD
        self.orb = None
        if back:
            self._icon_button(buf, "chevron-left", PAD + BTN / 2 - 6, cy, "back")
            x = PAD + BTN + 2
        else:  # el orbe vivo se pinta encima (el panel lo anima); aquí solo se le deja el sitio
            self.orb = (PAD + ORB_D / 2, cy, ORB_D)
            x = PAD + ORB_D + 10
        title = (
            "Historial"
            if self.mode == "history"
            else ("Conversación anterior" if viewing else "Chat")
        )
        if self.status and not back:
            self._text(buf, title, FS_TITLE, self._color("TEXT"), x, cy - 15)
            self._text(buf, self.status, FS_MICRO, self._color("MINT"), x, cy + 4)
        else:
            self._text(buf, title, FS_TITLE, self._color("TEXT"), x, cy - 9)
        right = g.width - PAD - BTN / 2 + 6
        for name, key, active in (
            ("close", "close", False),
            ("plus", "new", False),
            ("clock", "history", self.mode == "history"),
            ("speaker", "voice", self.model.voice_replies),
        ):
            self._icon_button(buf, name, right, cy, key, active)
            right -= BTN + 2
        line = np.asarray(self._color("LINE"), np.float32) / 255.0
        buf[
            round((HEADER_H - 1) * sc) : round(HEADER_H * sc),
            round(PAD * sc) : round((g.width - PAD) * sc),
        ] = np.concatenate([line[:3] * line[3], [line[3]]])

    def _icon_button(
        self, buf, name: str, cx: float, cy: float, key: str, active: bool = False
    ) -> None:
        rect = (cx - BTN / 2, cy - BTN / 2, cx + BTN / 2, cy + BTN / 2)
        hovered = self.hover == key
        if hovered or active:
            tone = "MINT" if active else "GROUP_HOVER"
            _fill_circle(
                buf, cx, cy, BTN, self._color(tone)[:3], self.g.scale, 0.18 if active else 1.0
            )
        color = self._color("MINT" if active else ("TEXT" if hovered else "TEXT_2"))[:3]
        draw_icon(buf, name, cx, cy, 18.0, color, self.g.scale)
        self._hit(rect, key)
