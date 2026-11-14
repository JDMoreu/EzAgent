"""Iconos del HUD: una sola familia y un solo rasterizador para barra, chat y ajustes.

Every glyph is described on a 24 x 24 grid as strokes (polylines, arcs, outlines)
plus a few fills. The rasterizer is analytic: each physical pixel gets the
coverage of its distance to the geometry (``r - d + 0.5`` clamped), so edges are
anti-aliased exactly once at the final size. There is no supersample-and-shrink
pass and no thickened font glyph, which is what made the old icons look soft.

# DECISION: Segoe Fluent Icons is no longer used. Mixing its outlines with the
# hand-drawn fallback gave two families with different terminals, and thickening
# the glyph with ``stroke_width`` before a LANCZOS + BOX downscale destroyed its
# hinting. One vector family keeps the HUD, chat and settings consistent.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import Final

import numpy as np

from .tokens import ICON_GRID, ICON_SMALL_THRESHOLD, ICON_STROKE, ICON_STROKE_SMALL, RGB, RGBA

Color = RGB | RGBA
Point = tuple[float, float]

ICON_NAMES: Final[tuple[str, ...]] = (
    "microphone",
    "microphone-slash",
    "phone",
    "hangup",
    "chevron-down",
    "chevron-up",
    "minus",
    "check",
    "speaker",
    "speaker-slash",
    "close",
    "sparkle",
    "mic-line",
    "voice-line",
    "chip",
    "sliders",
    "copy",
    "plus",
    "clock",
    "send",
    "stop",
    "chevron-left",
    "trash",
    "menu",
    # Barra y actividad.
    "chat",
    "waves",
    "alert",
    "compose",
)

# Alias tolerantes con los nombres que ya usa el HUD actual.
_ALIASES: Final[dict[str, str]] = {
    "mic": "microphone",
    "mic-off": "microphone-slash",
    "mic_off": "microphone-slash",
    "microphone-off": "microphone-slash",
    "call": "phone",
    "chevron": "chevron-down",
    "chevron_down": "chevron-down",
    "chevron_up": "chevron-up",
    "dash": "minus",
    "volume": "speaker",
    "volume-off": "speaker-slash",
    "speaker-off": "speaker-slash",
    "mute": "speaker-slash",
    "gear": "menu",
}

# DECISION: grid points belong to the drawing, not to the design tokens.
_PHONE: Final[tuple[Point, ...]] = (
    (7.0, 3.8),
    (9.6, 4.3),
    (10.0, 6.8),
    (8.2, 8.0),
    (15.0, 15.2),
    (16.5, 13.5),
    (19.4, 14.0),
    (20.2, 16.8),
    (18.2, 20.2),
    (15.4, 19.8),
    (3.8, 8.0),
    (3.8, 5.2),
    (5.2, 3.8),
    (7.0, 3.8),
)
_SLASH_FROM: Final[float] = 4.0
_SLASH_SPAN: Final[float] = 16.0
_SLASH_GAP: Final[float] = 1.2  # erase radius around the slash, in stroke widths

_MASKS: dict[tuple, np.ndarray] = {}
_CACHE: dict[tuple, np.ndarray] = {}
_CACHE_LIMIT: Final[int] = 512


# ------------------------------------------------------------------ geometry
@dataclass
class _Pen:
    """Records a glyph on the 24-unit grid; nothing is rasterized here."""

    segs: list[tuple[float, float, float, float]] = field(default_factory=list)
    fills: list[tuple[float, float, float, float, float]] = field(default_factory=list)

    def line(self, *pts: Point) -> None:
        for (x0, y0), (x1, y1) in zip(pts, pts[1:], strict=False):
            self.segs.append((x0, y0, x1, y1))
        if len(pts) == 1:
            self.segs.append((*pts[0], *pts[0]))

    def loop(self, *pts: Point) -> None:
        self.line(*pts, pts[0])

    def arc(self, box: tuple[float, float, float, float], start: float, end: float) -> None:
        """Elliptic arc; angles in degrees, clockwise from 3 o'clock (screen y down)."""
        x0, y0, x1, y1 = box
        cx, cy, rx, ry = (x0 + x1) / 2, (y0 + y1) / 2, (x1 - x0) / 2, (y1 - y0) / 2
        steps = max(6, int(abs(end - start) / 6))
        pts = []
        for i in range(steps + 1):
            a = math.radians(start + (end - start) * i / steps)
            pts.append((cx + rx * math.cos(a), cy + ry * math.sin(a)))
        self.line(*pts)

    def ellipse(self, box: tuple[float, float, float, float]) -> None:
        self.arc(box, 0.0, 360.0)

    def rrect(self, box: tuple[float, float, float, float], r: float) -> None:
        x0, y0, x1, y1 = box
        r = min(r, (x1 - x0) / 2, (y1 - y0) / 2)
        pts: list[Point] = []
        for cx, cy, a0 in (
            (x1 - r, y0 + r, -90.0),
            (x1 - r, y1 - r, 0.0),
            (x0 + r, y1 - r, 90.0),
            (x0 + r, y0 + r, 180.0),
        ):
            for i in range(7):
                a = math.radians(a0 + 90.0 * i / 6)
                pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
        self.loop(*pts)

    def fill_rrect(self, box: tuple[float, float, float, float], r: float) -> None:
        self.fills.append((*box, r))


def _mic(p: _Pen) -> None:
    p.rrect((8.8, 2.7, 15.2, 13.4), 3.2)
    p.arc((4.7, 5.6, 19.3, 18.3), 0.0, 180.0)
    p.line((12.0, 18.2), (12.0, 21.0))
    p.line((8.8, 21.0), (15.2, 21.0))


def _speaker(p: _Pen) -> None:
    p.loop((4.0, 9.3), (8.2, 9.3), (12.9, 4.7), (12.9, 19.3), (8.2, 14.7), (4.0, 14.7))
    p.arc((12.3, 7.6, 18.2, 16.4), -52.0, 52.0)
    p.arc((12.3, 4.8, 21.3, 19.2), -52.0, 52.0)


def _phone(p: _Pen, rotation: float = 0.0) -> None:
    a = math.radians(rotation)
    ca, sa = math.cos(a), math.sin(a)
    p.line(
        *(
            (12.0 + (x - 12.0) * ca - (y - 12.0) * sa, 12.0 + (x - 12.0) * sa + (y - 12.0) * ca)
            for x, y in _PHONE
        )
    )


def _chip(p: _Pen) -> None:
    p.rrect((6.4, 6.4, 17.6, 17.6), 2.4)
    for t in (9.6, 14.4):
        p.line((t, 3.2), (t, 6.4))
        p.line((t, 17.6), (t, 20.8))
        p.line((3.2, t), (6.4, t))
        p.line((17.6, t), (20.8, t))


def _sliders(p: _Pen) -> None:
    for y, knob in ((7.0, 15.0), (12.0, 9.0), (17.0, 13.5)):
        p.line((4.5, y), (knob - 2.2, y))
        p.line((knob + 2.2, y), (19.5, y))
        p.ellipse((knob - 2.1, y - 2.1, knob + 2.1, y + 2.1))


def _trash(p: _Pen) -> None:
    p.line((4.8, 7.0), (19.2, 7.0))
    p.line((9.4, 7.0), (9.4, 4.8), (14.6, 4.8), (14.6, 7.0))
    p.line((6.6, 7.0), (7.5, 18.4), (8.6, 19.4), (15.4, 19.4), (16.5, 18.4), (17.4, 7.0))
    p.line((10.2, 10.6), (10.2, 16.0))
    p.line((13.8, 10.6), (13.8, 16.0))


def _copy(p: _Pen) -> None:
    p.line((5.4, 15.4), (5.4, 6.4), (6.8, 5.0), (14.6, 5.0), (15.8, 6.2), (15.8, 8.2))
    p.rrect((8.4, 8.4, 19.0, 19.2), 2.4)


def _compose(p: _Pen) -> None:
    """Hoja con lápiz: «nueva conversación» (la hoja se abre arriba a la derecha)."""
    p.line((11.6, 5.0), (7.8, 5.0))
    p.arc((4.8, 5.0, 10.8, 11.0), 270.0, 180.0)
    p.line((4.8, 8.0), (4.8, 16.2))
    p.arc((4.8, 13.2, 10.8, 19.2), 180.0, 90.0)
    p.line((7.8, 19.2), (16.2, 19.2))
    p.arc((13.2, 13.2, 19.2, 19.2), 90.0, 0.0)
    p.line((19.2, 16.2), (19.2, 12.4))
    p.loop((10.2, 13.8), (10.8, 11.0), (17.4, 4.4), (19.6, 6.6), (13.0, 13.2))


def _chat(p: _Pen) -> None:
    p.rrect((3.0, 3.8, 21.0, 17.2), 5.0)
    p.line((8.0, 17.2), (6.4, 21.2), (12.6, 17.2))
    p.line((7.6, 8.6), (16.4, 8.6))
    p.line((7.6, 12.4), (13.4, 12.4))


_SIMPLE: Final[dict[str, tuple[tuple[Point, ...], ...]]] = {
    "chevron-down": (((6.8, 9.2), (12.0, 14.4), (17.2, 9.2)),),
    "chevron-up": (((6.8, 14.4), (12.0, 9.2), (17.2, 14.4)),),
    "chevron-left": (((14.8, 6.6), (9.4, 12.0), (14.8, 17.4)),),
    "minus": (((6.4, 12.0), (17.6, 12.0)),),
    "check": (((5.2, 12.4), (9.8, 17.0), (18.8, 6.8)),),
    "close": (((7.0, 7.0), (17.0, 17.0)), ((17.0, 7.0), (7.0, 17.0))),
    "plus": (((12.0, 5.2), (12.0, 18.8)), ((5.2, 12.0), (18.8, 12.0))),
    "send": (((12.0, 19.0), (12.0, 6.0)), ((6.2, 11.6), (12.0, 5.8), (17.8, 11.6))),
    "menu": tuple(((4.8, y), (19.2, y)) for y in (6.5, 12.0, 17.5)),
    "voice-line": tuple(
        ((x - 1.0, 12.0 - h), (x - 1.0, 12.0 + h))
        for x, h in ((5.0, 2.6), (9.0, 6.0), (13.0, 8.6), (17.0, 5.0), (21.0, 2.2))
    ),
    "waves": tuple(
        ((x, 12.0 - h), (x, 12.0 + h))
        for x, h in ((5.8, 2.8), (9.9, 7.0), (14.1, 4.9), (18.2, 2.0))
    ),
}


def _glyph(name: str) -> _Pen:
    p = _Pen()
    if name in _SIMPLE:
        for pts in _SIMPLE[name]:
            p.line(*pts)
    elif name in ("microphone", "microphone-slash"):
        _mic(p)
    elif name in ("speaker", "speaker-slash"):
        _speaker(p)
    elif name in ("phone", "hangup"):
        _phone(p, 135.0 if name == "hangup" else 0.0)
    elif name == "sparkle":
        p.loop(
            *((12.0, 3.4), (14.1, 9.9), (20.6, 12.0), (14.1, 14.1)),
            *((12.0, 20.6), (9.9, 14.1), (3.4, 12.0), (9.9, 9.9)),
        )
    elif name == "mic-line":
        p.rrect((9.0, 3.4, 15.0, 13.4), 3.0)
        p.arc((5.4, 6.0, 18.6, 18.0), 0.0, 180.0)
        p.line((12.0, 18.0), (12.0, 20.6))
    elif name == "clock":
        p.ellipse((4.0, 4.0, 20.0, 20.0))
        p.line((12.0, 7.6), (12.0, 12.0), (15.4, 14.0))
    elif name == "alert":
        p.ellipse((3.6, 3.6, 20.4, 20.4))
        p.line((12.0, 7.6), (12.0, 12.8))
        p.line((12.0, 16.3))  # a dot: a zero-length capsule
    elif name == "stop":
        p.fill_rrect((7.0, 7.0, 17.0, 17.0), 2.2)
    else:
        {
            "chip": _chip,
            "sliders": _sliders,
            "trash": _trash,
            "copy": _copy,
            "chat": _chat,
            "compose": _compose,
        }[name](p)
    return p


# ---------------------------------------------------------------- rasterizer
def _grid(pixels: int) -> tuple[np.ndarray, np.ndarray]:
    """Pixel centres of a ``pixels``² canvas, in grid units."""
    k = pixels / ICON_GRID
    axis = (np.arange(pixels, dtype=np.float32) + 0.5) / k
    gx, gy = np.meshgrid(axis, axis)
    return gx.ravel(), gy.ravel()


def segment_distance(px: np.ndarray, py: np.ndarray, segs: np.ndarray) -> np.ndarray:
    """Distance (grid units) from each point to the nearest segment."""
    best = np.full(px.shape, np.inf, dtype=np.float32)
    for chunk in range(0, len(segs), 48):
        s = segs[chunk : chunk + 48]
        ax, ay, bx, by = (s[:, i][None, :] for i in range(4))
        dx, dy = bx - ax, by - ay
        length = np.maximum(dx * dx + dy * dy, 1e-9)
        rx, ry = px[:, None] - ax, py[:, None] - ay
        t = np.clip((rx * dx + ry * dy) / length, 0.0, 1.0)
        qx, qy = rx - t * dx, ry - t * dy
        best = np.minimum(best, np.sqrt(qx * qx + qy * qy).min(axis=1))
    return best


def _rrect_distance(px: np.ndarray, py: np.ndarray, box: tuple[float, ...]) -> np.ndarray:
    x0, y0, x1, y1, r = box
    hx, hy = (x1 - x0) / 2 - r, (y1 - y0) / 2 - r
    qx = np.abs(px - (x0 + x1) / 2) - hx
    qy = np.abs(py - (y0 + y1) / 2) - hy
    outside = np.hypot(np.maximum(qx, 0.0), np.maximum(qy, 0.0))
    return outside + np.minimum(np.maximum(qx, qy), 0.0) - r


def _coverage(distance_px: np.ndarray, radius_px: float) -> np.ndarray:
    return np.clip(radius_px - distance_px + 0.5, 0.0, 1.0)


def _stroke_px(size: float, scale: float) -> float:
    stroke = ICON_STROKE_SMALL if size < ICON_SMALL_THRESHOLD else ICON_STROKE
    return max(1.0, stroke * scale)


def _rasterize(pen: _Pen, pixels: int, stroke_px: float) -> np.ndarray:
    px, py = _grid(pixels)
    k = pixels / ICON_GRID
    cover = np.zeros(px.shape, dtype=np.float32)
    if pen.segs:
        d = segment_distance(px, py, np.asarray(pen.segs, dtype=np.float32))
        cover = _coverage(d * k, stroke_px / 2.0)
    for box in pen.fills:
        cover = np.maximum(cover, _coverage(_rrect_distance(px, py, box) * k, 0.0))
    return cover.reshape(pixels, pixels)


def _slash_points(progress: float) -> tuple[Point, Point]:
    end = _SLASH_FROM + _SLASH_SPAN * max(0.0, min(1.0, progress))
    return (_SLASH_FROM, _SLASH_FROM), (end, end)


def icon_mask(name: str, size: float, scale: float, pixels: int | None = None) -> np.ndarray:
    """Coverage (0..1, float32) of ``name`` at ``size`` logical px.

    ``pixels`` forces the physical side (callers use it to keep parity with a
    sprite so the glyph centre lands on the same pixel grid). The ``*-slash``
    names include their slash and the gap around it.
    """
    name = _ALIASES.get(name, name)
    if name not in ICON_NAMES:
        raise ValueError(f"icono desconocido: {name!r}")
    if size <= 0 or scale <= 0:
        raise ValueError("size y scale deben ser mayores que cero")
    side = int(pixels) if pixels else max(1, int(round(size * scale)))
    stroke = _stroke_px(size, scale)
    key = (name, side, round(stroke, 3))
    mask = _MASKS.get(key)
    if mask is None:
        mask = _rasterize(_glyph(name), side, stroke)
        if name.endswith("-slash"):
            gap, line = slash_masks(size, scale, 1.0, side)
            mask = np.maximum(mask * (1.0 - gap), line)
        _remember(_MASKS, key, mask)
    return mask


def slash_masks(
    size: float, scale: float, progress: float, pixels: int | None = None
) -> tuple[np.ndarray, np.ndarray]:
    """(gap, slash) masks of the diagonal that crosses out a glyph, drawn to ``progress``."""
    side = int(pixels) if pixels else max(1, int(round(size * scale)))
    stroke = _stroke_px(size, scale)
    key = ("/slash", side, round(stroke, 3), round(progress, 3))
    cached = _MASKS.get(key)
    if cached is not None:
        return cached[0], cached[1]
    a, b = _slash_points(progress)
    px, py = _grid(side)
    d = segment_distance(px, py, np.asarray([(*a, *b)], dtype=np.float32)) * (side / ICON_GRID)
    gap = _coverage(d, stroke * _SLASH_GAP).reshape(side, side)
    line = _coverage(d, stroke / 2.0).reshape(side, side)
    _remember(_MASKS, key, np.stack([gap, line]))
    return gap, line


def _remember(cache: dict, key: tuple, value: np.ndarray) -> None:
    if len(cache) >= _CACHE_LIMIT:
        cache.pop(next(iter(cache)))
    cache[key] = value


# --------------------------------------------------------------- composition
def _rgba(color: Color) -> tuple[float, float, float, float]:
    alpha = (color[3] / 255.0) if len(color) == 4 else 1.0
    return (color[0] / 255.0, color[1] / 255.0, color[2] / 255.0, alpha)


def colorize(mask: np.ndarray, color: Color) -> np.ndarray:
    """Coverage mask → premultiplied float32 RGBA sprite."""
    alpha = mask.astype(np.float32)
    r, g, b, ca = _rgba(color)
    sprite = np.empty((*mask.shape, 4), dtype=np.float32)
    sprite[..., 0] = r * ca * alpha
    sprite[..., 1] = g * ca * alpha
    sprite[..., 2] = b * ca * alpha
    sprite[..., 3] = ca * alpha
    return sprite


def _sprite(name: str, color: Color, size: float, scale: float) -> np.ndarray:
    key = (_ALIASES.get(name, name), tuple(color), float(size), float(scale))
    cached = _CACHE.get(key)
    if cached is None:
        cached = colorize(icon_mask(name, size, scale), color)
        _remember(_CACHE, key, cached)
    return cached


def draw_icon(
    buf: np.ndarray,
    name: str,
    cx: float,
    cy: float,
    size: float,
    color: Color,
    scale: float,
    alpha: float = 1.0,
) -> None:
    """Dibuja ``name`` centrado en ``(cx, cy)`` sobre un buffer float32 RGBA.

    ``size`` es el lado lógico de la caja de 24 x 24; ``cx``/``cy`` también son
    lógicos. ``color`` admite RGB o RGBA 0..255 y ``alpha`` permite fundidos.
    """
    if buf.ndim != 3 or buf.shape[2] != 4 or buf.dtype != np.float32:
        raise TypeError("buf debe ser float32 con forma (H, W, 4)")
    if not 0.0 <= alpha <= 1.0:
        raise ValueError("alpha debe estar en [0, 1]")
    if alpha <= 0.0:
        return

    sprite = _sprite(name, color, size, scale)
    sh, sw = sprite.shape[:2]
    x0 = int(round(cx * scale - sw / 2.0))
    y0 = int(round(cy * scale - sh / 2.0))

    dst_h, dst_w = buf.shape[:2]
    sx0, sy0 = max(0, -x0), max(0, -y0)
    dx0, dy0 = max(0, x0), max(0, y0)
    copy_w = min(sw - sx0, dst_w - dx0)
    copy_h = min(sh - sy0, dst_h - dy0)
    if copy_w <= 0 or copy_h <= 0:
        return

    src = sprite[sy0 : sy0 + copy_h, sx0 : sx0 + copy_w]
    src_a = src[..., 3:4] * alpha
    dst = buf[dy0 : dy0 + copy_h, dx0 : dx0 + copy_w]
    dst[..., :3] = src[..., :3] * alpha + dst[..., :3] * (1.0 - src_a)
    dst[..., 3:4] = src_a + dst[..., 3:4] * (1.0 - src_a)


def clear_cache() -> None:
    _CACHE.clear()
    _MASKS.clear()


__all__ = [
    "ICON_NAMES",
    "clear_cache",
    "colorize",
    "draw_icon",
    "icon_mask",
    "segment_distance",
    "slash_masks",
]
