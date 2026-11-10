"""Composición del panel del chat: forma, contenido, sombra, orbe vivo y línea de estado.

``PaintMixin`` se mezcla en ``ChatPanel`` (``chat_ui``) y usa su estado: ``ui``, ``anim``,
``window``, ``view``, ``_cache``, ``_base`` y ``_act``. Todo se compone en el lienzo de la
ventana, donde el panel siempre está a ``MARGIN`` del borde.
"""

from __future__ import annotations

import math
from typing import TYPE_CHECKING

import numpy as np

from .bar_theme import card_palette
from .chat_geometry import (
    MARGIN,
    PANEL_RADIUS,
    Rect,
    ease_out_back,
    lerp,
    lerp_rect,
    rounded_coverage,
    smoothstep,
)
from .chat_header import HEADER_H
from .chat_view import ChatView

if TYPE_CHECKING:
    from .chat_ui import ChatWindow


def shape_progress(p: float, opening: bool) -> float:
    """Progreso de la forma: al abrir se pasa un poco (~2 %) y se asienta; al cerrar, directo.

    El rebote entra poco a poco (peso 0 hacia la mitad) para que reabrir a mitad de un cierre
    continúe sin saltos.
    """
    if not opening or p >= 1.0:
        return p
    weight = smoothstep((p - 0.45) / 0.35)
    return p + (ease_out_back(p, 1.2) - p) * weight


def row_reveal(rows: int, p: float, opening: bool) -> np.ndarray:
    """Cuánto se ve cada fila del contenido (0..1): de arriba abajo al abrir, y se va antes al cerrar."""
    base, span, stagger = (0.46, 0.28, 0.22) if opening else (0.52, 0.20, 0.20)
    f = np.linspace(0.0, 1.0, max(1, rows), dtype=np.float32)
    x = np.clip((p - (base + stagger * f)) / span, 0.0, 1.0)
    return x * x * (3.0 - 2.0 * x)


class PaintMixin:
    """Dibujo del panel (forma, contenido, orbe y línea)."""

    def _stable(self, window: ChatWindow, view: ChatView, now: float) -> np.ndarray:
        """Con el panel quieto: el cuadro se cachea y solo se repinta el orbe y la línea."""
        key = (self._sig(view, now), window.w, window.h, self.ui.s)
        if self._base is None or self._base[0] != key:
            self._base = (key, self._compose(window, view, 1.0, now, live=False))
        frame = self._base[1]
        pw = window.w - 2 * self._margin
        layers = self._live_layers(pw, window.h, now, 1.0, np.ones(1, np.float32))
        if not layers:
            return frame
        out = frame.copy()
        for x0, y0, arr in layers:
            self._blend_u8(out, arr, x0, y0)
        return out

    def _sig(self, view: ChatView, now: float, ticking: bool = True) -> tuple:
        m = view.model
        return (
            m.version,
            round(view.scroll, 1),
            view.hover,
            view.input.text,
            view.input.cursor,
            view.input.anchor,
            view.mode,
            view.pending_delete,
            view.status,
            view.flash_token,
            int((now - view._focus_t0) * 2) % 2 if ticking else 0,
            int(now * 30) if ticking and view.animating(now) else 0,
            view.g.theme,
            round(view.g.width),
            round(view.g.height),
        )

    def _content(self, view: ChatView, now: float, ticking: bool = True) -> np.ndarray:
        sig = self._sig(view, now, ticking)
        if self._cache is None or self._cache[0] != sig:
            self._cache = (sig, view.render(now))
        return self._cache[1]

    def _compose(
        self, window: ChatWindow, view: ChatView, p: float, now: float, live: bool
    ) -> np.ndarray:
        s = self.ui.s
        canvas = np.zeros((window.h, window.w, 4), np.float32)
        panel = self._panel_local()
        opening = self.anim.state != "closing"
        form = self._form_local()
        reduced = bool(self.ui._reduced)
        fly = form is not None and not reduced
        if fly:
            assert form is not None
            rect = lerp_rect(form, panel, shape_progress(p, opening))
            form_r = (
                min(form[2] - form[0], form[3] - form[1]) / 2
                if self.ui.layout == "circle"
                else (form[3] - form[1]) / 2
            )
            radius = lerp(form_r, PANEL_RADIUS * s, min(1.0, p))
            solid, alpha_k = smoothstep((p - 0.4) / 0.55), 1.0
            shadow_k = smoothstep((p - 0.6) / 0.4)
        else:  # fundido (movimiento reducido o el panel se movió lejos de la barra)
            grow = 0.95 + 0.05 * p if not reduced else 1.0
            cx, cy = (panel[0] + panel[2]) / 2, (panel[1] + panel[3]) / 2
            hw, hh = (panel[2] - panel[0]) / 2 * grow, (panel[3] - panel[1]) / 2 * grow
            rect = (cx - hw, cy - hh, cx + hw, cy + hh)
            radius = PANEL_RADIUS * s
            solid, alpha_k, shadow_k = 1.0, smoothstep(p), p
        self._paint_shape(canvas, rect, radius, solid, alpha_k, shadow_k, now)
        x0, y0, x1, y1 = (round(v) for v in rect)
        rows = max(1, y1 - y0)
        mask = (
            row_reveal(rows, p, opening)
            if fly
            else np.full(rows, alpha_k if reduced else smoothstep((p - 0.3) / 0.7), np.float32)
        )
        if mask.max() > 0.01:
            lift = 0 if not (fly and opening) else round((1.0 - float(mask.mean())) * 10 * s)
            self._paint_content(
                canvas, view, rect, radius, mask, lift, now, self.anim.state == "open"
            )
            if live:
                for lx, ly, arr in self._live_layers(x1 - x0, rows, now, 1.0, mask, x0, y0 + lift):
                    self._composite(canvas, arr, lx, ly)
        # BGRA uint8 en el sitio: ahorra tres copias del tamaño del lienzo por cuadro.
        np.multiply(canvas, 255.0, out=canvas)
        canvas += 0.5
        np.clip(canvas, 0.0, 255.0, out=canvas)
        return canvas.astype(np.uint8)[..., [2, 1, 0, 3]]

    def _paint_shape(
        self,
        canvas,
        rect: Rect,
        radius: float,
        solid: float,
        alpha_k: float,
        shadow_k: float,
        now: float,
    ) -> None:
        x0, y0, x1, y1 = (round(v) for v in rect)
        w, h = x1 - x0, y1 - y0
        if w < 2 or h < 2:
            return
        cov = rounded_coverage(w, h, radius)
        body = np.asarray(self.ui.chat_body_rgb(), np.float32) / 255.0
        fluid = self.ui.chat_fluid(w, h, now) if solid < 0.999 else None
        rgb = body[None, None, :] * solid + (
            fluid if fluid is not None else body[None, None, :]
        ) * (1.0 - solid)
        alpha = cov * alpha_k
        self._shadow_under(canvas, rect, radius, shadow_k * alpha_k)
        if fluid is None:  # cuerpo liso: se premultiplica en una sola pasada
            rgb = np.broadcast_to(body, (h, w, 3)) * alpha[..., None]
        else:
            rgb *= alpha[..., None]
        self._composite_over(canvas, rgb, alpha, x0, y0)
        # DECISION: el aro se pinta cuando el cuerpo ya es opaco; a media transformación no se
        # distingue del fluido y rehacerlo entero costaba un cuadro grande.
        if solid < 0.999:
            return
        ring = np.clip(cov - rounded_coverage(w, h, radius, inset=max(1.0, self.ui.s)), 0.0, 1.0)
        line = np.asarray(card_palette(self.ui._theme_name())["LINE"], np.float32) / 255.0
        la = ring * line[3] * 2.2 * solid * alpha_k
        self._composite_frame(
            canvas,
            np.concatenate([line[None, None, :3] * la[..., None], la[..., None]], axis=2),
            x0,
            y0,
            int(np.ceil(radius + max(1.0, self.ui.s))) + 2,
        )

    def _shadow_under(self, canvas, rect: Rect, radius: float, k: float) -> None:
        k *= 0.55
        if k <= 0.01:
            return
        x0, y0, x1, y1 = (round(v) for v in rect)
        w, h = x1 - x0, y1 - y0
        grow = round(MARGIN * self.ui.s * 0.8)
        shift = round(self.ui.s * 5)
        for step in range(3):
            g = grow * (step + 1) // 3
            cov = rounded_coverage(w + 2 * g, h + 2 * g, radius + g)
            # La sombra es negra: basta con oscurecer; el cuerpo tapa su centro y solo se
            # compone el marco que asoma (con las esquinas redondeadas y el desplazamiento).
            band = int(0.3 * (radius + g)) + g + shift + 2
            self._shade_frame(canvas, cov * (k * 0.10), x0 - g, y0 - g + shift, band)

    @staticmethod
    def _composite_over(canvas, rgb, alpha, x0: int, y0: int) -> None:
        """Compone ``rgb`` (ya premultiplicado por ``alpha``) y ``alpha`` sobre el lienzo."""
        h, w = alpha.shape[:2]
        sx0, sy0 = max(0, -x0), max(0, -y0)
        dx0, dy0 = max(0, x0), max(0, y0)
        w -= sx0
        h -= sy0
        cw = min(w, canvas.shape[1] - dx0)
        ch = min(h, canvas.shape[0] - dy0)
        if cw <= 0 or ch <= 0:
            return
        a = alpha[sy0 : sy0 + ch, sx0 : sx0 + cw, None]
        region = canvas[dy0 : dy0 + ch, dx0 : dx0 + cw]
        region *= 1.0 - a
        region[..., :3] += rgb[sy0 : sy0 + ch, sx0 : sx0 + cw]
        region[..., 3:] += a

    @staticmethod
    def _shade_frame(canvas, alpha, x0: int, y0: int, band: int) -> None:
        """Oscurece el marco de ``band`` px que ocupa ``alpha`` (su centro queda tapado)."""
        h, w = alpha.shape[:2]
        band = min(max(1, band), h, w)
        if band * 2 >= h or band * 2 >= w:  # marco casi todo el lienzo: una sola pasada
            PaintMixin._shade(canvas, alpha, x0, y0)
            return
        for yy in (slice(0, band), slice(h - band, h)):
            PaintMixin._shade(canvas, alpha[yy], x0, y0 + yy.start)
        mid = slice(band, h - band)
        for xx in (slice(0, band), slice(w - band, w)):
            PaintMixin._shade(canvas, alpha[mid, xx], x0 + xx.start, y0 + band)

    @staticmethod
    def _shade(canvas, alpha, x0: int, y0: int) -> None:
        h, w = alpha.shape[:2]
        sx0, sy0 = max(0, -x0), max(0, -y0)
        dx0, dy0 = max(0, x0), max(0, y0)
        cw = min(w - sx0, canvas.shape[1] - dx0)
        ch = min(h - sy0, canvas.shape[0] - dy0)
        if cw <= 0 or ch <= 0:
            return
        a = alpha[sy0 : sy0 + ch, sx0 : sx0 + cw, None]
        region = canvas[dy0 : dy0 + ch, dx0 : dx0 + cw]
        region[..., :3] *= 1.0 - a  # negro: no añade color
        region[..., 3:] = a + region[..., 3:] * (1.0 - a)

    @staticmethod
    def _composite_frame(canvas, src, x0: int, y0: int, band: int) -> None:
        """Compone solo el marco de ``band`` px de ``src`` (su centro queda tapado o es cero)."""
        h, w = src.shape[:2]
        band = min(max(1, band), h, w)
        if band * 2 >= h or band * 2 >= w:  # marco casi todo el lienzo: una sola pasada
            PaintMixin._composite(canvas, src, x0, y0)
            return
        for yy in (slice(0, band), slice(h - band, h)):
            PaintMixin._composite(canvas, src[yy], x0, y0 + yy.start)
        mid = slice(band, h - band)
        for xx in (slice(0, band), slice(w - band, w)):
            PaintMixin._composite(canvas, src[mid, xx], x0 + xx.start, y0 + band)

    def _paint_content(
        self,
        canvas,
        view: ChatView,
        rect: Rect,
        radius: float,
        mask: np.ndarray,
        lift: int,
        now: float,
        ticking: bool = True,
    ) -> None:
        content = self._content(view, now, ticking)
        x0, y0, x1, y1 = (round(v) for v in rect)
        w, h = x1 - x0, y1 - y0
        cov = rounded_coverage(w, h, radius)
        part = content[:h, :w]
        rows = min(part.shape[0], mask.shape[0])
        m = cov[:rows, : part.shape[1], None] * mask[:rows, None, None]
        self._composite(canvas, part[:rows] * m, x0, y0 + lift)

    # ------------------------------------------------- orbe vivo y línea de estado
    def _live_layers(
        self,
        pw: int,
        ph: int,
        now: float,
        gain: float,
        mask: np.ndarray,
        ox: int | None = None,
        oy: int | None = None,
    ) -> list[tuple[int, int, np.ndarray]]:
        """Capas premultiplicadas ``(x, y, RGBA)`` del orbe y la línea; ``ox/oy`` = esquina del panel."""
        view = self.view
        if view is None or view.orb is None:
            return []
        s = self.ui.s
        if ox is None or oy is None:
            ox = oy = self._margin
        cx, cy, diam = view.orb
        row = min(mask.shape[0] - 1, max(0, int(cy * s)))
        k = float(mask[row]) * gain
        if k <= 0.01:
            return []
        d = max(6, round(diam * s))  # tamaño fijo: lo único que se mueve es el fluido de dentro
        fluid = self.ui.chat_fluid(d, d, now)
        if fluid is None:
            return []
        alpha = rounded_coverage(d, d, d / 2)[..., None]
        layer = np.concatenate([fluid * alpha, alpha], axis=2).astype(np.float32) * k
        out = [(round(ox + cx * s - d / 2), round(oy + cy * s - d / 2), layer)]
        line = self._status_line(pw, now, k, ox, oy)
        if line is not None:
            out.append(line)
        return out

    def _status_line(
        self, pw: int, now: float, k: float, ox: int, oy: int
    ) -> tuple[int, int, np.ndarray] | None:
        """Una raya fina bajo la cabecera con el color del fluido y una onda que la recorre."""
        act = self._act * k
        if act <= 0.02:
            return None
        s = self.ui.s
        pad = round(16 * s)
        w, h = max(2, pw - 2 * pad), max(2, round(2 * s))
        fluid = self.ui.chat_fluid(w, h, now)
        if fluid is None:
            return None
        x = np.linspace(0.0, 1.0, w, dtype=np.float32)
        phase = 0.0 if self.ui._reduced else now * 0.9
        wave = 0.5 + 0.5 * np.sin(2 * math.pi * (x * 1.6 - phase))
        alpha = (0.18 + 0.82 * wave**2) * act * 0.9
        edge = np.minimum(1.0, np.minimum(x, 1.0 - x) * 12.0)  # se funde en los extremos
        alpha = (alpha * edge)[None, :, None] * np.ones((h, 1, 1), np.float32)
        arr = np.concatenate([fluid * alpha, alpha], axis=2).astype(np.float32)
        return (ox + pad, oy + round((HEADER_H - 1.5) * s), arr)

    @staticmethod
    def _blend_u8(frame: np.ndarray, src: np.ndarray, x0: int, y0: int) -> None:
        """``src`` (RGBA premultiplicado float) sobre un cuadro BGRA uint8, recortado a sus límites."""
        sx0, sy0 = max(0, -x0), max(0, -y0)
        dx0, dy0 = max(0, x0), max(0, y0)
        w = min(src.shape[1] - sx0, frame.shape[1] - dx0)
        h = min(src.shape[0] - sy0, frame.shape[0] - dy0)
        if w <= 0 or h <= 0:
            return
        top = src[sy0 : sy0 + h, sx0 : sx0 + w]
        region = frame[dy0 : dy0 + h, dx0 : dx0 + w]
        under = region[..., [2, 1, 0, 3]].astype(np.float32) / 255.0
        mixed = np.clip(top + under * (1.0 - top[..., 3:4]), 0.0, 1.0)
        region[...] = (mixed * 255.0 + 0.5).astype(np.uint8)[..., [2, 1, 0, 3]]

    @staticmethod
    def _composite(canvas: np.ndarray, src: np.ndarray, x0: int, y0: int) -> None:
        """``src`` (premultiplicado) sobre ``canvas``, recortado a sus límites."""
        sx0, sy0 = max(0, -x0), max(0, -y0)
        dx0, dy0 = max(0, x0), max(0, y0)
        w = min(src.shape[1] - sx0, canvas.shape[1] - dx0)
        h = min(src.shape[0] - sy0, canvas.shape[0] - dy0)
        if w <= 0 or h <= 0:
            return
        top = src[sy0 : sy0 + h, sx0 : sx0 + w]
        region = canvas[dy0 : dy0 + h, dx0 : dx0 + w]
        region *= 1.0 - top[..., 3:4]  # en el sitio: sin temporales del tamaño del panel
        region += top
