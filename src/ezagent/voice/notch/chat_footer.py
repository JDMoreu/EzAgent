"""Pie de la vista del chat: campo de texto con varias líneas, botón de enviar y confirmaciones.

Se separa de ``chat_view`` para que ningún módulo pase de 600 líneas. ``FooterMixin`` se mezcla
en ``ChatView`` y usa su estado (``g``, ``input``, ``model``, ``hover``…).
"""

from __future__ import annotations

import time

from ..chat import markdown as md
from ..chat.render_base import STYLES, Ctx, wrap
from .icons import draw_icon
from .menu import _fill_circle, _fill_round_rect, _stroke_round_rect
from .tokens import FS_MICRO, FS_ROW, FS_SECONDARY

PAD = 16.0
BTN = 32.0
INPUT_MIN = 46.0
INPUT_LINE = 21.0
INPUT_MAX_LINES = 6
HINT_H = 18.0


class FooterMixin:
    """Entrada de texto, enviar/detener y tarjeta de confirmación."""

    def _wrap_input(self, width: float) -> list[tuple[int, int]]:
        """Líneas visibles del campo: ``(inicio, fin)`` de cada una en el texto."""
        ctx = self._ctx(width)
        text, lines, start = self.input.text, [], 0
        for logical in text.split("\n"):
            lines += [(start + a, start + b) for a, b in _break(ctx, logical, width)]
            start += len(logical) + 1
        return lines or [(0, 0)]

    def _footer_height(self) -> float:
        width = self.g.width - 2 * PAD - 12 - BTN - 14
        self._input_lines = self._wrap_input(width)
        rows = min(len(self._input_lines), INPUT_MAX_LINES)
        box = max(INPUT_MIN, rows * INPUT_LINE + 24)
        extra = 0.0
        if self.model.confirm is not None:
            extra += 84.0
        if self.model.viewing is not None:
            extra += 30.0
        return box + HINT_H + 24 + extra

    def _draw_footer(self, buf, now: float, footer: float) -> None:
        g = self.g
        y = g.height - footer
        if self.model.viewing is not None:
            self._text(
                buf,
                "Estás viendo una conversación anterior. Escribe para volver a la actual.",
                FS_MICRO,
                self._color("TEXT_3"),
                PAD,
                y + 8,
            )
            y += 30
        if self.model.confirm is not None:
            self._draw_confirm(buf, y + 2)
            y += 84
        rows = min(len(self._input_lines), INPUT_MAX_LINES)
        box_h = max(INPUT_MIN, rows * INPUT_LINE + 24)
        box = (PAD, y + 6, g.width - PAD, y + 6 + box_h)
        self._input_box = box
        _fill_round_rect(buf, *box, 16.0, self._color("GROUP")[:3], g.scale)
        _stroke_round_rect(
            buf,
            *box,
            16.0,
            self._color("MINT" if not self.input.empty else "LINE")[:3],
            g.scale,
            0.7,
        )
        self._draw_input_text(buf, box, now)
        self._hit(box, "input")  # antes que el botón: gana la zona registrada al final
        self._draw_send(buf, box)
        hint = "Enter envía · Mayús+Enter salta de línea"
        self._text(buf, hint, FS_MICRO, self._color("TEXT_3", 0.85), PAD + 12, box[3] + 6)

    def _draw_input_text(self, buf, box, now: float) -> None:
        x0, y0 = box[0] + 14, box[1] + 12
        first = max(0, len(self._input_lines) - INPUT_MAX_LINES)
        if self.input.empty and not self.input.text:
            self._text(buf, "Escribe un mensaje…", FS_ROW, self._color("TEXT_3"), x0, y0 + 1)
        sel = self.input.selection()
        ctx = self._ctx(1000.0)
        for row, (a, b) in enumerate(self._input_lines[first:]):
            y = y0 + row * INPUT_LINE
            if sel and sel[0] < b + 1 and sel[1] > a:
                s0, s1 = max(sel[0], a), min(sel[1], b)
                x_a, x_b = (
                    ctx.length(self.input.text[a:s0], "body"),
                    ctx.length(self.input.text[a:s1], "body"),
                )
                _fill_round_rect(
                    buf,
                    x0 + x_a,
                    y - 1,
                    x0 + x_b + 3,
                    y + 19,
                    4.0,
                    self._color("MINT")[:3],
                    self.g.scale,
                    0.28,
                )
            self._text(buf, self.input.text[a:b], STYLES["body"], self._color("TEXT"), x0, y)
        self._draw_caret(buf, x0, y0, first, now)

    def _draw_caret(self, buf, x0: float, y0: float, first: int, now: float) -> None:
        if int((now - self._focus_t0) * 2.0) % 2:
            return
        cur = self.input.cursor
        for row, (a, b) in enumerate(self._input_lines):
            last = row == len(self._input_lines) - 1
            if a <= cur <= b and (
                cur < b or last or self.input.text[b : b + 1] == "\n" or cur == b
            ):
                if row < first:
                    return
                x = x0 + self._ctx(1000.0).length(self.input.text[a:cur], "body")
                y = y0 + (row - first) * INPUT_LINE
                _fill_round_rect(
                    buf, x, y, x + 1.6, y + 18, 0.8, self._color("MINT")[:3], self.g.scale
                )
                return

    def _draw_send(self, buf, box) -> None:
        cx, cy = box[2] - 14 - BTN / 2 + 6, box[3] - 8 - BTN / 2
        stop = self.model.busy
        ready = stop or not self.input.empty
        tone = "MINT_BTN" if ready else "TRACK"
        _fill_circle(
            buf,
            cx,
            cy,
            BTN,
            self._color(tone)[:3],
            self.g.scale,
            1.0 if not ready or self.hover != "send" else 0.88,
        )
        ink = self._color("MINT_INK" if ready else "TEXT_3")[:3]
        draw_icon(buf, "stop" if stop else "send", cx, cy, 17.0, ink, self.g.scale)
        self._hit(
            (cx - BTN / 2, cy - BTN / 2, cx + BTN / 2, cy + BTN / 2), "stop" if stop else "send"
        )

    def _draw_confirm(self, buf, y: float) -> None:
        card = self.model.confirm
        if card is None:
            return
        rect = (PAD, y, self.g.width - PAD, y + 76)
        _fill_round_rect(buf, *rect, 14.0, self._color("WARM")[:3], self.g.scale, 0.14)
        _stroke_round_rect(buf, *rect, 14.0, self._color("WARM")[:3], self.g.scale, 0.7)
        ctx = self._ctx(self.g.width - 2 * PAD - 28)
        lines = wrap(ctx, md.parse_inline(card.summary), "small", ctx.width)[:2]
        text = " ".join("".join(r.text for r in ln) for ln in lines)
        self._text(
            buf,
            text if len(text) < 90 else text[:89] + "…",
            FS_SECONDARY,
            self._color("TEXT"),
            PAD + 14,
            y + 12,
        )
        for i, (label, key) in enumerate((("Sí", "confirm_yes"), ("No", "confirm_no"))):
            bx = PAD + 14 + i * 92
            b = (bx, y + 40, bx + 82, y + 66)
            primary = i == 0
            _fill_round_rect(
                buf,
                *b,
                10.0,
                self._color("MINT_BTN" if primary else "TRACK")[:3],
                self.g.scale,
                0.9 if self.hover == key else 1.0,
            )
            self._text(
                buf,
                label,
                FS_SECONDARY,
                self._color("MINT_INK" if primary else "TEXT"),
                bx + 41,
                y + 46,
                align="center",
            )
            self._hit(b, key)

    def _place_caret(self, x: float, y: float) -> None:
        box = self._input_box
        first = max(0, len(self._input_lines) - INPUT_MAX_LINES)
        row = int((y - (box[1] + 12)) // INPUT_LINE)
        row = max(0, min(row, len(self._input_lines) - first - 1)) + first
        a, b = self._input_lines[row]
        ctx = self._ctx(1000.0)
        target = x - (box[0] + 14)
        best = a
        for i in range(a, b + 1):
            if ctx.length(self.input.text[a:i], "body") <= target + 3:
                best = i
        self.input.anchor = None
        self.input.cursor = best
        self._focus_t0 = time.monotonic()


def _break(ctx: Ctx, text: str, width: float) -> list[tuple[int, int]]:
    """Reparte una línea lógica en tramos que caben en ``width``: ``(inicio, fin)`` por tramo."""
    if not text:
        return [(0, 0)]
    out: list[tuple[int, int]] = []
    start = 0
    last_space = -1
    for i in range(len(text) + 1):
        if i < len(text) and text[i] == " ":
            last_space = i
        if i == len(text) or ctx.length(text[start : i + 1], "body") > width:
            if i == len(text):
                out.append((start, i))
                break
            cut = last_space + 1 if last_space >= start else i
            if cut <= start:
                cut = i if i > start else start + 1
            out.append((start, cut))
            start, last_space = cut, -1
    return out or [(0, len(text))]
