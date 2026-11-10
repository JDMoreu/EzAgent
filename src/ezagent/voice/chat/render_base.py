"""Base del dibujo del chat: contexto, fuentes, formas suaves y texto con estilos.

Todo se maqueta en píxeles lógicos y se pinta a tamaño físico (``× scale``), igual
que el resto del notch, para que el texto salga nítido a cualquier DPI. Los mensajes
se dibujan una vez en un ``PIL.Image`` RGBA y la ventana solo los compone.
"""

from __future__ import annotations

import re
from collections.abc import Callable, Iterable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Final

from PIL import Image, ImageChops, ImageDraw, ImageFont

from ..notch.text import _load_font
from ..notch.tokens import Color, FontStyle
from .markdown import Span

# Estilos de texto del chat: la misma familia (Geist) que el resto de la interfaz.
STYLES: Final[dict[str, FontStyle]] = {
    "body": FontStyle("CHAT_BODY", "Geist", 14, 400),
    "bold": FontStyle("CHAT_BOLD", "Geist", 14, 600),
    "code": FontStyle("CHAT_CODE", "GeistMono", 12.5, 400),
    "h1": FontStyle("CHAT_H1", "Geist", 19, 600),
    "h2": FontStyle("CHAT_H2", "Geist", 16.5, 600),
    "h3": FontStyle("CHAT_H3", "Geist", 14.5, 600),
    "cell": FontStyle("CHAT_CELL", "Geist", 13, 400),
    "cellbold": FontStyle("CHAT_CELLBOLD", "Geist", 13, 600),
    "small": FontStyle("CHAT_SMALL", "Geist", 12, 400),
    "smallbold": FontStyle("CHAT_SMALLBOLD", "Geist", 12, 600),
    "micro": FontStyle("CHAT_MICRO", "GeistMono", 10.5, 500),
}
LINE_K: Final[float] = 1.5  # interlineado
PARA_GAP: Final[float] = 8.0  # espacio entre bloques
_SPLIT = re.compile(r"(\s+)")


@dataclass(frozen=True, slots=True)
class Hit:
    """Zona pulsable de un mensaje, en píxeles lógicos relativos a su esquina."""

    kind: str  # «copy_code» | «open» | «reveal» | «image»
    rect: tuple[float, float, float, float]
    data: object = None


@dataclass(slots=True)
class Ctx:
    """Todo lo que necesita el dibujo: tamaño, colores y qué archivos se pueden mostrar."""

    scale: float
    pal: dict[str, Color]
    width: float  # ancho útil lógico
    roots: tuple[Path, ...] | None = None  # carpetas de lectura permitidas (None = todas)
    base_dirs: tuple[Path, ...] = ()
    hits: list[Hit] = field(default_factory=list)
    _fonts: dict[str, ImageFont.FreeTypeFont | ImageFont.ImageFont] = field(default_factory=dict)

    def font(self, key: str) -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
        font = self._fonts.get(key)
        if font is None:
            font = self._fonts[key] = _load_font(STYLES[key], self.scale)
        return font

    def size(self, key: str) -> float:
        return STYLES[key].size

    def px(self, value: float) -> int:
        return int(round(value * self.scale))

    def length(self, text: str, key: str) -> float:
        """Avance lógico de ``text`` con la fuente ``key``."""
        return float(self.font(key).getlength(text)) / self.scale

    def color(self, name: str, alpha: float | None = None) -> tuple[int, int, int, int]:
        """Color de la paleta; sin ``alpha`` conserva su transparencia propia (p. ej. ``LINE``)."""
        entry = self.pal[name]
        rgb = tuple(entry[:3])
        if alpha is None:
            return (*rgb, int(entry[3]) if len(entry) > 3 else 255)  # type: ignore[return-value]
        return (*rgb, max(0, min(255, round(255 * alpha))))  # type: ignore[return-value]

    def child(self, width: float) -> Ctx:
        """Contexto para dibujar dentro de una caja más estrecha (vista previa, celdas)."""
        # Las zonas pulsables de una caja anidada no son del mensaje: lista propia.
        return Ctx(self.scale, self.pal, width, self.roots, self.base_dirs, [], self._fonts)


Painter = Callable[[Image.Image, float, float], None]  # (imagen, x lógica, y lógica)


# ---------------------------------------------------------------- formas suaves
def round_rect(
    img: Image.Image,
    ctx: Ctx,
    box: tuple[float, float, float, float],
    radius: float,
    fill: tuple[int, int, int, int] | None = None,
    outline: tuple[int, int, int, int] | None = None,
) -> None:
    """Rectángulo redondeado con bordes suaves (máscara a 4× reducida)."""
    x0, y0, x1, y1 = (ctx.px(v) for v in box)
    w, h = max(1, x1 - x0), max(1, y1 - y0)
    ss = 4
    r = ctx.px(radius) * ss
    ring = ctx.px(1) * ss

    def shape(inset: int) -> Image.Image:
        mask = Image.new("L", (w * ss, h * ss), 0)
        ImageDraw.Draw(mask).rounded_rectangle(
            (inset, inset, w * ss - 1 - inset, h * ss - 1 - inset),
            radius=max(0, r - inset),
            fill=255,
        )
        return mask.resize((w, h), Image.LANCZOS)

    outer = shape(0)
    inner = shape(ring) if outline else outer
    for color, mask in (
        (fill, inner),
        (outline, ImageChops.subtract(outer, inner) if outline else None),
    ):
        if color is None or mask is None:
            continue
        mask = ImageChops.multiply(mask, Image.new("L", (w, h), color[3]))
        layer = Image.new("RGBA", (w, h), color[:3] + (255,))
        layer.putalpha(mask)
        img.alpha_composite(layer, (x0, y0))


def line(
    img: Image.Image,
    ctx: Ctx,
    x0: float,
    y0: float,
    x1: float,
    y1: float,
    color: tuple[int, int, int, int],
) -> None:
    ImageDraw.Draw(img, "RGBA").rectangle(
        (ctx.px(x0), ctx.px(y0), max(ctx.px(x0), ctx.px(x1) - 1), max(ctx.px(y0), ctx.px(y1) - 1)),
        fill=color,
    )


def draw_text_at(
    img: Image.Image,
    ctx: Ctx,
    x: float,
    baseline: float,
    text: str,
    key: str,
    color: tuple[int, int, int, int],
    italic: bool = False,
) -> None:
    """Texto con su línea base en ``baseline`` (la cursiva se inclina)."""
    font = ctx.font(key)
    px, py = ctx.px(x), ctx.px(baseline)
    if not italic:
        ImageDraw.Draw(img, "RGBA").text((px, py), text, font=font, fill=color, anchor="ls")
        return
    ascent, descent = font.getmetrics()
    w = int(font.getlength(text)) + ascent
    mask = Image.new("L", (w, ascent + descent + 2), 0)
    ImageDraw.Draw(mask).text((0, ascent), text, font=font, fill=255, anchor="ls")
    skew = 0.2
    mask = mask.transform(
        mask.size, Image.AFFINE, (1, skew, -skew * ascent, 0, 1, 0), Image.BICUBIC
    )
    layer = Image.new("RGBA", mask.size, color[:3] + (255,))
    layer.putalpha(ImageChops.multiply(mask, Image.new("L", mask.size, color[3])))
    img.alpha_composite(layer, (px, py - ascent))


# ------------------------------------------------------------- texto con estilos
@dataclass(slots=True)
class _Piece:
    text: str
    key: str
    italic: bool
    code: bool
    link: bool
    space: bool = False


def _pieces(spans: Iterable[Span], base: str) -> list[_Piece]:
    out: list[_Piece] = []
    for span in spans:
        if span.code:
            out.append(_Piece(span.text, "code", False, True, span.link is not None))
            continue
        key = "bold" if span.bold or base == "bold" else base
        for part in _SPLIT.split(span.text):
            if part:
                out.append(
                    _Piece(part, key, span.italic, False, span.link is not None, part.isspace())
                )
    return out


@dataclass(slots=True)
class _Run:
    x: float
    text: str
    piece: _Piece
    width: float


def _piece_width(ctx: Ctx, piece: _Piece) -> float:
    pad = 6.0 if piece.code else 0.0
    return ctx.length(piece.text, piece.key) + pad


def wrap(ctx: Ctx, spans: Iterable[Span], base: str, width: float) -> list[list[_Run]]:
    """Reparte el texto en líneas de como mucho ``width``; cada línea es una lista de tramos."""
    lines: list[list[_Run]] = [[]]
    x = 0.0

    def place(piece: _Piece, w: float) -> None:
        nonlocal x
        if x + w > width and lines[-1]:
            while lines[-1] and lines[-1][-1].piece.space:
                lines[-1].pop()  # sin espacios colgando al final de la línea
            lines.append([])
            x = 0.0
        lines[-1].append(_Run(x, piece.text, piece, w))
        x += w

    for piece in _pieces(spans, base):
        w = _piece_width(ctx, piece)
        if piece.space:
            if lines[-1]:
                lines[-1].append(_Run(x, piece.text, piece, w))
                x += w
        elif w > width and not piece.code and len(piece.text) > 1:
            for chunk in _split_word(ctx, piece, width):  # palabra más ancha que la caja
                place(chunk, _piece_width(ctx, chunk))
        else:
            place(piece, w)
    while lines and not lines[-1] and len(lines) > 1:
        lines.pop()
    return lines


def _split_word(ctx: Ctx, piece: _Piece, width: float) -> list[_Piece]:
    """Parte una palabra demasiado larga en trozos que quepan en ``width``."""
    chunks: list[_Piece] = []
    current = ""
    for char in piece.text:
        if current and ctx.length(current + char, piece.key) > width:
            chunks.append(_Piece(current, piece.key, piece.italic, piece.code, piece.link))
            current = ""
        current += char
    if current:
        chunks.append(_Piece(current, piece.key, piece.italic, piece.code, piece.link))
    return chunks


def text_height(ctx: Ctx, nlines: int, base: str) -> float:
    return nlines * ctx.size(base) * LINE_K


def paint_lines(
    img: Image.Image,
    ctx: Ctx,
    lines: list[list[_Run]],
    base: str,
    x0: float,
    y0: float,
    color: tuple[int, int, int, int],
) -> None:
    """Pinta las líneas de ``wrap`` desde ``(x0, y0)`` (esquina superior izquierda)."""
    step = ctx.size(base) * LINE_K
    for index, runs in enumerate(lines):
        baseline = y0 + index * step + step * 0.5 + ctx.size(base) * 0.34
        for run in runs:
            if run.piece.space:
                continue
            tone = color
            x = x0 + run.x
            if run.piece.code:
                size = ctx.size("code")
                round_rect(
                    img,
                    ctx,
                    (x, baseline - size * 1.02, x + run.width, baseline + size * 0.34),
                    4.0,
                    ctx.color("TRACK", 0.55),
                )
                x += 3.0
                tone = ctx.color("TEXT")
            elif run.piece.link:
                tone = ctx.color("MINT")
            draw_text_at(img, ctx, x, baseline, run.text, run.piece.key, tone, run.piece.italic)
            if run.piece.link:
                line(img, ctx, x, baseline + 2.0, x + run.width, baseline + 3.0, tone)
