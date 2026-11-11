"""Cabecera, historial y estado vacío de la vista del chat (separados de ``chat_view``).

``HeaderMixin`` se mezcla en ``ChatView`` y usa su estado (``g``, ``model``, ``hover``, ``mode``…).
"""

from __future__ import annotations

import time

import numpy as np

from .chat_footer import BTN, PAD
from .icons import draw_icon
from .menu import _fill_circle, _fill_round_rect, _fit_text
from .settings_window import composite_over
from .text import draw_text  # noqa: F401  (los métodos usan ``self._text``)
from .tokens import FS_MICRO, FS_ROW, FS_SECONDARY, FS_TITLE

HEADER_H = 56.0
DELETE_SECS = 0.18  # fila del historial que se desvanece al borrarla
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

    # -------------------------------------------------------------- historial
    def _draw_history(self, buf, top: float, bottom: float, now: float) -> None:
        sessions = list(self.model.sessions())
        ghost = self._deleting
        if ghost is not None:
            elapsed = now - ghost[2]
            # Si el borrado no llegó a ocurrir (o ya pasó el fundido), no hay fila fantasma.
            if elapsed >= DELETE_SECS or any(row.session == ghost[1].session for row in sessions):
                self._deleting = ghost = None
            else:
                sessions.insert(min(ghost[0], len(sessions)), ghost[1])
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
        busy = self.model.busy
        current = self.model.current_session
        y = top + 8 - self.scroll
        total = 8.0
        if busy:  # no se cambia ni se borra nada mientras el modelo responde
            self._text(
                buf,
                "Detén la respuesta para cambiar de conversación.",
                FS_MICRO,
                self._color("TEXT_3"),
                PAD,
                y + 4,
            )
            y += 22
            total += 22
        progress = 0.0
        if ghost is not None:
            raw = max(0.0, min(1.0, (now - ghost[2]) / DELETE_SECS))
            progress = raw * raw * (3.0 - 2.0 * raw)
        for info in sessions:
            fading = ghost is not None and info.session == ghost[1].session
            row_h = 60.0 * (1.0 - progress) if fading else 60.0
            total += row_h
            if row_h > 1.0 and y + row_h > top and y < bottom:
                if fading:
                    self._draw_ghost_row(
                        buf, info, y, row_h, busy, info.session == current, 1.0 - progress
                    )
                else:
                    self._draw_history_row(buf, info, y, busy, info.session == current)
            y += row_h
        self._max_scroll = max(0.0, total - (bottom - top))
        self.scroll = max(0.0, min(self.scroll, self._max_scroll))

    def _draw_ghost_row(
        self, buf, info, y: float, height: float, busy: bool, is_current: bool, alpha: float
    ) -> None:
        """Fila ya borrada: se encoge y se desvanece mientras las de abajo suben."""
        rows = max(1, round(height * self.g.scale))
        tmp = np.zeros((rows, buf.shape[1], 4), np.float32)
        self._draw_history_row(tmp, info, 0.0, busy, is_current, interactive=False)
        composite_over(buf, tmp * alpha, 0, round(y * self.g.scale))

    def _draw_history_row(
        self, buf, info, y: float, busy: bool, is_current: bool, interactive: bool = True
    ) -> None:
        w = self.g.width
        asking = self._pending_delete == info.session
        right = w - 8 - 24  # centro del último botón de la fila
        rect = (8.0, y, w - 8 - (84 if asking else 44), y + 58)
        if interactive and self.hover == f"session:{info.session}" and not busy:
            _fill_round_rect(
                buf, 8.0, y, w - 8, y + 58, 12.0, self._color("GROUP_HOVER")[:3], self.g.scale
            )
        tone = "TEXT_3" if busy else "TEXT"
        if asking:
            self._text(buf, "¿Eliminar esta conversación?", FS_ROW, self._color("ERR"), 20, y + 20)
            self._row_button(buf, "check", right - 40, y + 29, "delete_yes", info.session, "ERR")
            self._row_button(buf, "close", right, y + 29, "delete_no", info.session, "TEXT_2")
            return
        title = info.title if len(info.title) < 40 else info.title[:39] + "…"
        self._text(buf, title, FS_ROW, self._color(tone), 20, y + 11)
        meta = f"{_date(info.started)} · {info.messages} mensajes"
        self._text(buf, meta, FS_MICRO, self._color("TEXT_3"), 20, y + 35)
        if is_current:
            self._text(
                buf, "ACTUAL", FS_MICRO, self._color("MINT"), right - 16, y + 35, align="right"
            )
        if not busy and interactive:
            self._hit(rect, "session", info.session)
            self._row_button(buf, "trash", right, y + 29, "delete_ask", info.session, "TEXT_3")

    def _row_button(
        self, buf, icon: str, cx: float, cy: float, kind: str, session: int, tone: str
    ) -> None:
        """Botón redondo de una fila del historial (borrar, confirmar, cancelar)."""
        hovered = self.hover == f"{kind}:{session}"
        if hovered:
            _fill_circle(buf, cx, cy, BTN, self._color("GROUP_HOVER")[:3], self.g.scale)
        color = self._color("ERR" if hovered and kind == "delete_ask" else tone)[:3]
        draw_icon(buf, icon, cx, cy, 18.0, color, self.g.scale)
        self._hit((cx - BTN / 2, cy - BTN / 2, cx + BTN / 2, cy + BTN / 2), kind, session)

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
        title = "Historial" if self.mode == "history" else ("Anterior" if viewing else "EzAgent")
        if self.status and not back:
            self._text(buf, title, FS_TITLE, self._color("TEXT"), x, cy - 15)
            available = g.width - PAD - 4 * (BTN + 2) - x
            status = _fit_text(self.status, FS_SECONDARY, available, sc)
            self._text(buf, status, FS_SECONDARY, self._color("MINT"), x, cy + 4)
        else:
            self._text(buf, title, FS_TITLE, self._color("TEXT"), x, cy - 9)
        right = g.width - PAD - BTN / 2 + 6
        # DECISION: conversation actions belong in the header; audio controls
        # stay beside the composer so compact windows retain a readable title.
        for name, key, active, alert in (
            ("minus", "close", False, False),
            ("plus", "new", False, False),
            ("clock", "history", self.mode == "history", False),
            ("sliders", "settings", False, False),
        ):
            self._icon_button(buf, name, right, cy, key, active, alert)
            right -= BTN + 2
        line = np.asarray(self._color("LINE"), np.float32) / 255.0
        buf[
            round((HEADER_H - 1) * sc) : round(HEADER_H * sc),
            round(PAD * sc) : round((g.width - PAD) * sc),
        ] = np.concatenate([line[:3] * line[3], [line[3]]])

    def _icon_button(
        self,
        buf,
        name: str,
        cx: float,
        cy: float,
        key: str,
        active: bool = False,
        alert: bool = False,
    ) -> None:
        rect = (cx - BTN / 2, cy - BTN / 2, cx + BTN / 2, cy + BTN / 2)
        hovered = self.hover == key
        if hovered or active:
            tone = "MINT" if active else "GROUP_HOVER"
            _fill_circle(
                buf, cx, cy, BTN, self._color(tone)[:3], self.g.scale, 0.18 if active else 1.0
            )
        color = self._color(
            "ERR" if alert else ("MINT" if active else ("TEXT" if hovered else "TEXT_2"))
        )[:3]
        draw_icon(buf, name, cx, cy, 18.0, color, self.g.scale)
        self._hit(rect, key)
