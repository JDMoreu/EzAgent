"""Texto del notch renderizado con Pillow.

Cinco estilos cerrados (``FS_*`` de :mod:`tokens`), fuentes Geist del proyecto,
anti-alias nativo de FreeType y composición sobre un buffer float32 RGBA
premultiplicado. El texto se rasteriza directamente a tamaño físico
``tamaño_lógico * scale``: no se escala un bitmap lógico, por lo que se mantiene
nítido a 100, 150 y 200 % DPI.

Coordenadas:
- ``x`` e ``y`` son lógicos y ``y`` es el borde superior de la caja de tinta.
  # DECISION: anclar por la tinta (no por la línea tipográfica) simplifica
  # centrar la palabra de estado en su zona de 64 x 14 px.
- ``align`` mueve el ancla horizontal: ``left`` usa ``x`` como borde izquierdo,
  ``center`` como centro y ``right`` como borde derecho.
- ``tracking`` se expresa en ``em`` (0.1 = 0.1 * tamaño de fuente).
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Final, Literal

import numpy as np
from PIL import Image, ImageDraw, ImageFont

from .tokens import (
    FONT_FALLBACKS,
    FONT_FILES,
    FONT_STYLES,
    Color,
    FontStyle,
)

Align = Literal["left", "center", "right"]
_ALIGNS: Final[tuple[str, ...]] = ("left", "center", "right")
_CACHE_LIMIT: Final[int] = 512


@dataclass(slots=True)
class _TextBitmap:
    """Máscara de tinta recortada y sus dimensiones físicas."""

    alpha: np.ndarray          # (h, w) uint8
    width_px: int              # avance aunque no haya tinta (p. ej. espacios)
    line_h_px: int


_CACHE: dict[
    tuple[str, FontStyle, Color, float, float, str], _TextBitmap
] = {}
_FONT_CACHE: dict[tuple[str, int, float], ImageFont.FreeTypeFont | ImageFont.ImageFont] = {}
_FONT_PATH_CACHE: dict[tuple[str, int], object] = {}


def _resolve_style(style: str | FontStyle) -> FontStyle:
    if isinstance(style, FontStyle):
        return style
    key = style.strip().upper()
    if not key.startswith("FS_"):
        key = "FS_" + key
    try:
        return FONT_STYLES[key]
    except KeyError as exc:
        raise ValueError(f"estilo de texto desconocido: {style!r}") from exc


def _resolve_tracking(style: FontStyle, tracking: float | None) -> float:
    value = style.tracking_em if tracking is None else float(tracking)
    if not math.isfinite(value):
        raise ValueError("tracking debe ser un número finito")
    return value


def _font_path(style: FontStyle) -> object | None:
    key = (style.family, style.weight)
    if key in _FONT_PATH_CACHE:
        return _FONT_PATH_CACHE[key]
    project = FONT_FILES.get(style.family, {}).get(style.weight)
    candidates = [project] if project is not None else []
    candidates.extend(FONT_FALLBACKS.get(style.family, {}).get(style.weight, ()))
    found: object | None = None
    for candidate in candidates:
        if candidate is not None and candidate.exists():
            found = candidate
            break
    _FONT_PATH_CACHE[key] = found
    return found


def _load_font(style: FontStyle, scale: float) -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
    size_px = max(1.0, style.size * scale)
    key = (style.family, style.weight, size_px)
    cached = _FONT_CACHE.get(key)
    if cached is not None:
        return cached
    path = _font_path(style)
    try:
        if path is None:
            raise OSError("fuente no encontrada")
        font = ImageFont.truetype(str(path), size_px)
    except Exception:
        # DECISION: si faltan Geist y los respaldos del sistema, Pillow usa su
        # fuente interna. Es preferible a romper el render del notch.
        try:
            font = ImageFont.load_default(size=max(1, round(size_px)))
        except TypeError:  # Pillow antiguo no acepta ``size``.
            font = ImageFont.load_default()
    _FONT_CACHE[key] = font
    return font


def _render_bitmap(
    text: str,
    style: FontStyle,
    scale: float,
    tracking: float,
) -> _TextBitmap:
    font = _load_font(style, scale)
    tracking_px = tracking * style.size * scale
    ascent, descent = font.getmetrics()
    line_h_px = max(1, int(round(ascent + descent)))
    pad = max(2, int(round(2.0 * scale)))

    if not text:
        return _TextBitmap(np.zeros((0, 0), np.uint8), 0, line_h_px)

    if abs(tracking_px) > 1e-6:
        advances = [float(font.getlength(char)) for char in text]
        total = sum(advances) + tracking_px * (len(text) - 1)
        canvas_w = max(1, int(math.ceil(total)) + 2 * pad)
        canvas_h = line_h_px + 2 * pad
        image = Image.new("L", (canvas_w, canvas_h), 0)
        draw = ImageDraw.Draw(image)
        pen = float(pad)
        for char, advance in zip(text, advances, strict=False):
            draw.text((pen, float(pad)), char, font=font, fill=255)
            pen += advance + tracking_px
    else:
        total = float(font.getlength(text))
        canvas_w = max(1, int(math.ceil(total)) + 2 * pad)
        canvas_h = line_h_px + 2 * pad
        image = Image.new("L", (canvas_w, canvas_h), 0)
        ImageDraw.Draw(image).text(
            (float(pad), float(pad)), text, font=font, fill=255
        )

    mask = np.asarray(image, dtype=np.uint8)
    rows, cols = np.nonzero(mask)
    if rows.size == 0:
        # Texto compuesto solo por espacios: conserva el avance para alinear.
        return _TextBitmap(
            np.zeros((0, 0), np.uint8),
            max(0, int(round(total))),
            line_h_px,
        )
    cropped = mask[
        int(rows.min()) : int(rows.max()) + 1,
        int(cols.min()) : int(cols.max()) + 1,
    ]
    return _TextBitmap(cropped.copy(), max(1, int(round(total))), line_h_px)


def _bitmap_for(
    text: str,
    style: FontStyle,
    color: Color,
    scale: float,
    tracking: float,
    align: str,
) -> _TextBitmap:
    key = (text, style, tuple(color), float(scale), float(tracking), align)
    cached = _CACHE.get(key)
    if cached is not None:
        return cached
    bitmap = _render_bitmap(text, style, scale, tracking)
    if len(_CACHE) >= _CACHE_LIMIT:
        # Los estilos y estados son pocos; una eviction FIFO basta.
        _CACHE.pop(next(iter(_CACHE)))
    _CACHE[key] = bitmap
    return bitmap


def text_size(
    text: str,
    style: str | FontStyle,
    scale: float = 1.0,
    tracking: float | None = None,
    align: Align = "left",
) -> tuple[float, float]:
    """Tamaño de la tinta en píxeles lógicos ``(ancho, alto)``.

    ``scale`` es el factor DPI (``dpi / 96``). Para texto sin tinta (solo
    espacios) devuelve el avance y el alto de línea tipográfico.
    """
    if scale <= 0:
        raise ValueError("scale debe ser mayor que cero")
    if align not in _ALIGNS:
        raise ValueError(f"alineación desconocida: {align!r}")
    resolved = _resolve_style(style)
    track = _resolve_tracking(resolved, tracking)
    display = text.upper() if resolved.uppercase else text
    bitmap = _bitmap_for(display, resolved, (0, 0, 0), scale, track, align)
    if bitmap.alpha.size:
        h, w = bitmap.alpha.shape
    else:
        w, h = bitmap.width_px, bitmap.line_h_px
    return (w / scale, h / scale)


def draw_text(
    buf: np.ndarray,
    text: str,
    style: str | FontStyle,
    color: Color,
    x: float,
    y: float,
    scale: float,
    align: Align = "left",
    tracking: float | None = None,
    alpha: float = 1.0,
) -> tuple[float, float]:
    """Dibuja texto sobre ``buf`` y devuelve el tamaño lógico dibujado.

    ``buf`` debe ser ``(H, W, 4)`` float32 premultiplicado con canales 0..1.
    ``color`` es RGB o RGBA 0..255. El alfa adicional de ``alpha`` permite
    fundidos sin volver a rasterizar.
    """
    if scale <= 0:
        raise ValueError("scale debe ser mayor que cero")
    if align not in _ALIGNS:
        raise ValueError(f"alineación desconocida: {align!r}")
    if buf.ndim != 3 or buf.shape[2] != 4 or buf.dtype != np.float32:
        raise TypeError("buf debe ser float32 con forma (H, W, 4)")
    if not 0.0 <= alpha <= 1.0:
        raise ValueError("alpha debe estar en [0, 1]")

    resolved = _resolve_style(style)
    track = _resolve_tracking(resolved, tracking)
    display = text.upper() if resolved.uppercase else text
    color_alpha = (color[3] / 255.0) if len(color) == 4 else 1.0
    draw_alpha = alpha * color_alpha
    if draw_alpha <= 0.0 or not display:
        return (0.0, 0.0)

    bitmap = _bitmap_for(
        display, resolved, tuple(color), scale, track, align
    )
    if bitmap.alpha.size == 0:
        width = bitmap.width_px / scale
        height = bitmap.line_h_px / scale
        return (width, height)

    mask_h, mask_w = bitmap.alpha.shape
    x_px = x * scale
    y_px = y * scale
    if align == "left":
        x0 = int(round(x_px))
    elif align == "center":
        x0 = int(round(x_px - mask_w / 2.0))
    else:
        x0 = int(round(x_px - mask_w))
    y0 = int(round(y_px))

    dst_h, dst_w = buf.shape[:2]
    sx0 = max(0, -x0)
    sy0 = max(0, -y0)
    dx0 = max(0, x0)
    dy0 = max(0, y0)
    copy_w = min(mask_w - sx0, dst_w - dx0)
    copy_h = min(mask_h - sy0, dst_h - dy0)
    if copy_w <= 0 or copy_h <= 0:
        return (mask_w / scale, mask_h / scale)

    src_alpha = (
        bitmap.alpha[sy0 : sy0 + copy_h, sx0 : sx0 + copy_w].astype(np.float32)
        / 255.0
    )
    a = src_alpha * draw_alpha
    rgb = np.asarray(color[:3], dtype=np.float32) / 255.0
    region = buf[dy0 : dy0 + copy_h, dx0 : dx0 + copy_w]
    one_minus = 1.0 - a
    region[..., :3] = (
        rgb[None, None, :] * a[..., None] + region[..., :3] * one_minus[..., None]
    )
    region[..., 3] = a + region[..., 3] * one_minus
    return (mask_w / scale, mask_h / scale)


def clear_cache() -> None:
    """Vacía las cachés. Debe llamarse al cambiar de DPI o de tema."""
    _CACHE.clear()
    _FONT_CACHE.clear()
    _FONT_PATH_CACHE.clear()


__all__ = ["Align", "clear_cache", "draw_text", "text_size"]
