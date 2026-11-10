"""Iconos del notch.

Se usa Segoe Fluent Icons si está instalada y verificada; si no, el mismo
módulo dibuja todos los glifos a mano sobre la rejilla de 24 x 24. En ambos
casos el sprite final es RGBA premultiplicado, con trazo óptico cercano a
1,8 px lógicos (1,6 px por debajo de 18 px) y extremos redondeados.

# DECISION: Segoe Fluent Icons es la ruta principal porque se verificaron sus
# códigos y se ve mejor a 15-18 px que un contorno propio. El trazado vectorial
# queda siempre disponible si la fuente falta o falla. Los glifos de fuente se
# engrosan de forma óptica para acercarse a 1,8/1,6 px lógicos.
"""

from __future__ import annotations

import math
from pathlib import Path
from typing import Final, Iterable

import numpy as np
from PIL import Image, ImageDraw, ImageFont

from .tokens import (
    ICON_GRID,
    ICON_SMALL_THRESHOLD,
    ICON_SSAA,
    ICON_STROKE,
    ICON_STROKE_SMALL,
    RGB,
    RGBA,
)

Color = RGB | RGBA
Point = tuple[float, float]
Points = tuple[Point, ...]

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
    # Siempre vectoriales: iconos de la barra lateral de ajustes y de ventana.
    "close",
    "sparkle",
    "mic-line",
    "voice-line",
    "chip",
    "sliders",
)

# Los iconos de esta lista no usan la fuente Segoe: así los cinco de la barra
# lateral comparten el mismo trazo y las mismas terminaciones.
_VECTOR_ONLY: Final[frozenset[str]] = frozenset(
    {"close", "sparkle", "mic-line", "voice-line", "chip", "sliders"}
)

# Alias tolerantes con los nombres que ya usa el notch actual.
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
}

_ICON_FONT_CANDIDATES: Final[tuple[Path, ...]] = (
    Path("C:/Windows/Fonts/SegoeIcons.ttf"),
    Path("C:/Windows/Fonts/segmdl2.ttf"),
)

_GLYPHS: Final[dict[str, int]] = {
    "microphone": 0xE720,
    "microphone-slash": 0xE720,
    "phone": 0xE717,
    "hangup": 0xE717,       # el teléfono se gira 135° al dibujarlo
    "chevron-down": 0xE70D,
    "chevron-up": 0xE70E,
    "minus": 0xE738,
    "check": 0xE73E,
    "speaker": 0xE767,
}

_CACHE: dict[tuple[str, Color, float, float], np.ndarray] = {}
_FONT_CACHE: dict[tuple[str, int], ImageFont.FreeTypeFont] = {}
_ICON_FONT_PATH: Path | None = None
_FONT_RESOLVED = False

# DECISION: los puntos del respaldo vectorial se expresan en la rejilla de
# 24 x 24 y forman parte del dibujo, no de los tokens de diseño. Mantenerlos
# aquí evita un tokens.py ilegible.
_PHONE: Points = (
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


# --------------------------------------------------------------- utilidades
def _rgba(color: Color) -> tuple[float, float, float, float]:
    alpha = (color[3] / 255.0) if len(color) == 4 else 1.0
    return (color[0] / 255.0, color[1] / 255.0, color[2] / 255.0, alpha)


def _colorize(mask: np.ndarray, color: Color) -> np.ndarray:
    """Convierte una máscara alfa en sprite RGBA premultiplicado."""
    alpha = mask.astype(np.float32)
    r, g, b, ca = _rgba(color)
    sprite = np.empty((*mask.shape, 4), dtype=np.float32)
    sprite[..., 0] = r * ca * alpha
    sprite[..., 1] = g * ca * alpha
    sprite[..., 2] = b * ca * alpha
    sprite[..., 3] = ca * alpha
    return sprite


def _caps(
    draw: ImageDraw.ImageDraw, points: Iterable[Point], radius: float
) -> None:
    for x, y in points:
        draw.ellipse(
            (x - radius, y - radius, x + radius, y + radius), fill=255
        )


# ------------------------------------------------------- respaldo vectorial
def _path(
    draw: ImageDraw.ImageDraw,
    points: Iterable[Point],
    width: float,
    k: float,
    *,
    closed: bool = False,
) -> None:
    pts = [(x * k, y * k) for x, y in points]
    if closed and pts and pts[0] != pts[-1]:
        pts.append(pts[0])
    if len(pts) < 2:
        return
    width_i = max(1, int(round(width)))
    draw.line(pts, fill=255, width=width_i, joint="curve")
    _caps(draw, pts, width_i / 2.0)


def _arc(
    draw: ImageDraw.ImageDraw,
    bbox: tuple[float, float, float, float],
    start: float,
    end: float,
    width: float,
    k: float,
) -> None:
    x0, y0, x1, y1 = (value * k for value in bbox)
    width_i = max(1, int(round(width)))
    draw.arc((x0, y0, x1, y1), start=start, end=end, fill=255, width=width_i)
    cx = (x0 + x1) / 2.0
    cy = (y0 + y1) / 2.0
    rx = (x1 - x0) / 2.0
    ry = (y1 - y0) / 2.0
    ends = []
    for deg in (start, end):
        rad = math.radians(deg)
        ends.append((cx + rx * math.cos(rad), cy + ry * math.sin(rad)))
    _caps(draw, ends, width_i / 2.0)


def _mic(draw: ImageDraw.ImageDraw, width: float, k: float) -> None:
    width_i = max(1, int(round(width)))
    draw.rounded_rectangle(
        (8.8 * k, 2.7 * k, 15.2 * k, 13.4 * k),
        radius=3.2 * k,
        outline=255,
        width=width_i,
    )
    _arc(draw, (4.7, 5.6, 19.3, 18.3), 0.0, 180.0, width, k)
    _path(draw, ((12.0, 18.2), (12.0, 21.0)), width, k)
    _path(draw, ((8.8, 21.0), (15.2, 21.0)), width, k)


def _mic_slash(
    draw: ImageDraw.ImageDraw, width: float, k: float
) -> None:
    """Micrófono tachado con un hueco alrededor de la diagonal.

    El hueco separa la barra del propio micrófono; sin él, al pintar todo en
    rojo la diagonal se funde con el glifo y el mute deja de leerse.
    """
    _mic(draw, width, k)
    start, end = 4.0 * k, 20.0 * k
    gap = max(1, int(round(width * 2.4)))
    line_width = max(1, int(round(width)))
    draw.line((start, start, end, end), fill=0, width=gap)
    draw.line((start, start, end, end), fill=255, width=line_width)
    radius = line_width / 2.0
    for x, y in ((start, start), (end, end)):
        draw.ellipse((x - radius, y - radius, x + radius, y + radius), fill=255)


def _phone(
    draw: ImageDraw.ImageDraw, width: float, k: float, rotation: float = 0.0
) -> None:
    points: Points = _PHONE
    if rotation:
        a = math.radians(rotation)
        ca, sa = math.cos(a), math.sin(a)
        points = tuple(
            (
                12.0 + (x - 12.0) * ca - (y - 12.0) * sa,
                12.0 + (x - 12.0) * sa + (y - 12.0) * ca,
            )
            for x, y in _PHONE
        )
    _path(draw, points, width, k, closed=True)


def _draw_vector(
    name: str, width: float, draw: ImageDraw.ImageDraw, k: float
) -> None:
    if name == "microphone":
        _mic(draw, width, k)
    elif name == "microphone-slash":
        _mic_slash(draw, width, k)
    elif name == "phone":
        _phone(draw, width, k)
    elif name == "hangup":
        _phone(draw, width, k, rotation=135.0)
    elif name == "chevron-down":
        _path(draw, ((6.8, 9.2), (12.0, 14.4), (17.2, 9.2)), width, k)
    elif name == "chevron-up":
        _path(draw, ((6.8, 14.4), (12.0, 9.2), (17.2, 14.4)), width, k)
    elif name == "minus":
        _path(draw, ((6.4, 12.0), (17.6, 12.0)), width, k)
    elif name == "check":
        _path(draw, ((5.2, 12.4), (9.8, 17.0), (18.8, 6.8)), width, k)
    elif name == "close":
        _path(draw, ((7.0, 7.0), (17.0, 17.0)), width, k)
        _path(draw, ((17.0, 7.0), (7.0, 17.0)), width, k)
    elif name == "sparkle":
        _path(
            draw,
            (
                (12.0, 3.4), (14.1, 9.9), (20.6, 12.0), (14.1, 14.1),
                (12.0, 20.6), (9.9, 14.1), (3.4, 12.0), (9.9, 9.9),
            ),
            width,
            k,
            closed=True,
        )
    elif name == "mic-line":
        draw.rounded_rectangle(
            (9.0 * k, 3.4 * k, 15.0 * k, 13.4 * k),
            radius=3.0 * k,
            outline=255,
            width=max(1, int(round(width))),
        )
        _arc(draw, (5.4, 6.0, 18.6, 18.0), 0.0, 180.0, width, k)
        _path(draw, ((12.0, 18.0), (12.0, 20.6)), width, k)
    elif name == "voice-line":
        for x, half in ((5.0, 2.6), (9.0, 6.0), (13.0, 8.6), (17.0, 5.0), (21.0, 2.2)):
            _path(draw, ((x - 1.0, 12.0 - half), (x - 1.0, 12.0 + half)), width, k)
    elif name == "chip":
        draw.rounded_rectangle(
            (6.4 * k, 6.4 * k, 17.6 * k, 17.6 * k),
            radius=2.4 * k,
            outline=255,
            width=max(1, int(round(width))),
        )
        for t in (9.6, 14.4):
            _path(draw, ((t, 3.2), (t, 6.4)), width, k)
            _path(draw, ((t, 17.6), (t, 20.8)), width, k)
            _path(draw, ((3.2, t), (6.4, t)), width, k)
            _path(draw, ((17.6, t), (20.8, t)), width, k)
    elif name == "sliders":
        for y, knob in ((7.0, 15.0), (12.0, 9.0), (17.0, 13.5)):
            _path(draw, ((4.5, y), (knob - 2.2, y)), width, k)
            _path(draw, ((knob + 2.2, y), (19.5, y)), width, k)
            r = 2.1 * k
            draw.ellipse(
                (knob * k - r, y * k - r, knob * k + r, y * k + r),
                outline=255,
                width=max(1, int(round(width))),
            )
    elif name == "speaker":
        _path(
            draw,
            (
                (4.0, 9.3),
                (8.2, 9.3),
                (12.9, 4.7),
                (12.9, 19.3),
                (8.2, 14.7),
                (4.0, 14.7),
            ),
            width,
            k,
            closed=True,
        )
        _arc(draw, (12.3, 7.6, 18.2, 16.4), -52.0, 52.0, width, k)
        _arc(draw, (12.3, 4.8, 21.3, 19.2), -52.0, 52.0, width, k)
    else:
        raise ValueError(f"icono desconocido: {name!r}")


def _make_vector_sprite(
    name: str,
    color: Color,
    size: float,
    scale: float,
) -> np.ndarray:
    """Respaldo dibujado a mano, coloreado a tamaño físico."""
    physical = max(1, int(round(size * scale)))
    ss = max(1, ICON_SSAA)
    pixels = physical * ss
    stroke = ICON_STROKE_SMALL if size < ICON_SMALL_THRESHOLD else ICON_STROKE
    stroke_ss = stroke * scale * ss
    k = pixels / ICON_GRID

    mask = Image.new("L", (pixels, pixels), 0)
    draw = ImageDraw.Draw(mask)
    _draw_vector(name, stroke_ss, draw, k)
    small = mask.resize((physical, physical), resample=Image.Resampling.BOX)
    return _colorize(np.asarray(small, dtype=np.float32) / 255.0, color)


# ----------------------------------------------------- fuente de iconos
def _icon_font_path() -> Path | None:
    """Primera fuente de iconos disponible, o ``None`` para usar el respaldo."""
    global _ICON_FONT_PATH, _FONT_RESOLVED
    if not _FONT_RESOLVED:
        _ICON_FONT_PATH = next(
            (path for path in _ICON_FONT_CANDIDATES if path.exists()),
            None,
        )
        _FONT_RESOLVED = True
    return _ICON_FONT_PATH


def _icon_font(path: Path, pixel_size: int) -> ImageFont.FreeTypeFont:
    key = (str(path), pixel_size)
    font = _FONT_CACHE.get(key)
    if font is None:
        font = ImageFont.truetype(str(path), pixel_size)
        _FONT_CACHE[key] = font
    return font


def _optimal_stroke_width(
    font_px: float, size: float, scale: float
) -> int:
    """Grosor extra en px de supermuestreo para acercarse al trazo nominal."""
    stroke = (
        ICON_STROKE_SMALL if size < ICON_SMALL_THRESHOLD else ICON_STROKE
    )
    desired = stroke * scale * ICON_SSAA
    # El trazo nativo de Segoe Fluent es ~1,17 px a un tamaño de fuente de 24.
    native = 1.17 * (font_px / ICON_GRID)
    extra = max(0.0, desired - native)
    return max(0, int(round(extra / 2.0)))


def _font_mask(name: str, size: float, scale: float) -> np.ndarray | None:
    """Máscara del glifo Segoe; ``None`` si no hay fuente disponible."""
    path = _icon_font_path()
    if path is None:
        return None
    physical = max(1, int(round(size * scale)))
    ss = max(1, ICON_SSAA)
    pixels = physical * ss
    font_px = max(1, int(round(pixels * 0.96)))
    font = _icon_font(path, font_px)
    stroke_width = _optimal_stroke_width(font_px, size, scale)

    canvas = Image.new("L", (pixels * 2, pixels * 2), 0)
    ImageDraw.Draw(canvas).text(
        (pixels // 2, pixels // 2),
        chr(_GLYPHS[name]),
        font=font,
        fill=255,
        stroke_width=stroke_width,
        stroke_fill=255,
    )
    bbox = canvas.getbbox()
    if bbox is None:
        return None
    glyph = canvas.crop(bbox)
    if name == "hangup":
        # Pillow gira en sentido antihorario; -135 = 135° horarios.
        glyph = glyph.rotate(
            -135.0, resample=Image.Resampling.BICUBIC, expand=True
        )
        bbox = glyph.getbbox()
        if bbox is None:
            return None
        glyph = glyph.crop(bbox)

    limit = max(1, int(round(pixels * 0.94)))
    longest = max(glyph.size)
    if longest > limit:
        factor = limit / float(longest)
        glyph = glyph.resize(
            (
                max(1, int(round(glyph.width * factor))),
                max(1, int(round(glyph.height * factor))),
            ),
            Image.Resampling.LANCZOS,
        )

    final = Image.new("L", (pixels, pixels), 0)
    final.paste(
        glyph,
        ((pixels - glyph.width) // 2, (pixels - glyph.height) // 2),
    )

    if name == "microphone-slash":
        overlay = ImageDraw.Draw(final)
        start = int(round(pixels * 0.16))
        end = int(round(pixels * 0.84))
        stroke = (
            ICON_STROKE_SMALL if size < ICON_SMALL_THRESHOLD else ICON_STROKE
        )
        width = max(1, int(round(stroke * scale * ICON_SSAA)))
        gap = max(1, int(round(width * 2.4)))
        overlay.line((start, start, end, end), fill=0, width=gap)
        overlay.line((start, start, end, end), fill=255, width=width)
        radius = width / 2.0
        for x, y in ((start, start), (end, end)):
            overlay.ellipse(
                (x - radius, y - radius, x + radius, y + radius), fill=255
            )
    small = final.resize(
        (physical, physical), resample=Image.Resampling.BOX
    )
    return np.asarray(small, dtype=np.float32) / 255.0


def _make_sprite(
    name: str,
    color: Color,
    size: float,
    scale: float,
) -> np.ndarray:
    """Glifo Segoe si está disponible; si no, respaldo vectorial."""
    mask = None
    if name in _GLYPHS and name not in _VECTOR_ONLY:
        try:
            mask = _font_mask(name, size, scale)
        except Exception:
            mask = None
    if mask is not None:
        return _colorize(mask, color)
    return _make_vector_sprite(name, color, size, scale)


def _sprite(name: str, color: Color, size: float, scale: float) -> np.ndarray:
    name = _ALIASES.get(name, name)
    if name not in ICON_NAMES:
        raise ValueError(f"icono desconocido: {name!r}")
    if size <= 0 or scale <= 0:
        raise ValueError("size y scale deben ser mayores que cero")
    key = (name, tuple(color), float(size), float(scale))
    cached = _CACHE.get(key)
    if cached is None:
        cached = _make_sprite(name, color, size, scale)
        _CACHE[key] = cached
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
    La caché usa (nombre, color, tamaño, escala) porque el grosor óptico depende
    del tamaño lógico, no solo de sus píxeles físicos.
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
    sx0 = max(0, -x0)
    sy0 = max(0, -y0)
    dx0 = max(0, x0)
    dy0 = max(0, y0)
    copy_w = min(sw - sx0, dst_w - dx0)
    copy_h = min(sh - sy0, dst_h - dy0)
    if copy_w <= 0 or copy_h <= 0:
        return

    src = sprite[sy0 : sy0 + copy_h, sx0 : sx0 + copy_w]
    src_a = src[..., 3:4] * alpha
    src_c = src[..., :3] * alpha
    dst = buf[dy0 : dy0 + copy_h, dx0 : dx0 + copy_w]
    dst[..., :3] = src_c + dst[..., :3] * (1.0 - src_a)
    dst[..., 3:4] = src[..., 3:4] * alpha + dst[..., 3:4] * (1.0 - src_a)


def clear_cache() -> None:
    """Vacía las cachés de sprites y glifos (DPI/tema/contenido)."""
    _CACHE.clear()
    _FONT_CACHE.clear()


__all__ = ["ICON_NAMES", "clear_cache", "draw_icon"]
