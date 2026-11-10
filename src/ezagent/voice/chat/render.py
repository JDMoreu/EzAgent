"""Dibuja un mensaje del chat (markdown) como imagen: texto, listas, código, tablas, imágenes y documentos.

``render_message`` es el punto de entrada: parsea, maqueta y pinta una vez; la ventana solo
recorta y compone el resultado. Las zonas pulsables (copiar código, abrir un documento…) salen
en ``Rendered.hits``, en píxeles lógicos relativos al mensaje.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from PIL import Image, ImageDraw

from ..notch.tokens import Color
from . import markdown as md
from .render_base import (
    PARA_GAP,
    Ctx,
    Hit,
    Painter,
    draw_text_at,
    line,
    paint_lines,
    round_rect,
    text_height,
    wrap,
)

CODE_LINE: float = 17.5
CODE_HEAD: float = 24.0
CODE_PAD: float = 12.0
LIST_STEP: float = 18.0
LIST_MARKER: float = 18.0


@dataclass(frozen=True, slots=True)
class Rendered:
    """Un mensaje ya dibujado."""

    image: Image.Image  # RGBA (sin premultiplicar), a tamaño físico
    height: float  # alto lógico
    hits: tuple[Hit, ...]
    plain: str  # el texto sin formato, para copiarlo


# ------------------------------------------------------------------- bloques
def _heading(block: md.Heading, ctx: Ctx) -> tuple[float, Painter]:
    key = {1: "h1", 2: "h2"}.get(block.level, "h3")
    lines = wrap(ctx, block.spans, key, ctx.width)

    def paint(img: Image.Image, x: float, y: float) -> None:
        paint_lines(img, ctx, lines, key, x, y, ctx.color("TEXT"))

    return text_height(ctx, len(lines), key), paint


def _paragraph(block: md.Paragraph, ctx: Ctx) -> tuple[float, Painter]:
    lines = wrap(ctx, block.spans, "body", ctx.width)

    def paint(img: Image.Image, x: float, y: float) -> None:
        paint_lines(img, ctx, lines, "body", x, y, ctx.color("TEXT"))

    return text_height(ctx, len(lines), "body"), paint


def _list_item(block: md.ListItem, ctx: Ctx) -> tuple[float, Painter]:
    indent = 4.0 + block.level * LIST_STEP
    lines = wrap(ctx, block.spans, "body", ctx.width - indent - LIST_MARKER)
    step = ctx.size("body") * 1.5

    def paint(img: Image.Image, x: float, y: float) -> None:
        cy = y + step / 2
        mx = x + indent
        if block.checked is not None:
            _checkbox(img, ctx, mx, cy, block.checked)
        elif block.marker == "•":
            r = 2.2
            ImageDraw.Draw(img, "RGBA").ellipse(
                (ctx.px(mx + 4 - r), ctx.px(cy - r), ctx.px(mx + 4 + r), ctx.px(cy + r)),
                fill=ctx.color("TEXT_3"),
            )
        else:
            draw_text_at(img, ctx, mx, cy + 5, block.marker, "body", ctx.color("TEXT_3"))
        paint_lines(img, ctx, lines, "body", x + indent + LIST_MARKER, y, ctx.color("TEXT"))

    return text_height(ctx, len(lines), "body"), paint


def _checkbox(img: Image.Image, ctx: Ctx, x: float, cy: float, checked: bool) -> None:
    box = (x, cy - 6.5, x + 13, cy + 6.5)
    if checked:
        round_rect(img, ctx, box, 4.0, ctx.color("MINT"))
        draw = ImageDraw.Draw(img, "RGBA")
        pts = [(x + 3.2, cy + 0.2), (x + 5.6, cy + 2.8), (x + 10, cy - 2.6)]
        draw.line(
            [(ctx.px(a), ctx.px(b)) for a, b in pts], fill=ctx.color("MINT_INK"), width=ctx.px(1.6)
        )
    else:
        round_rect(img, ctx, box, 4.0, None, ctx.color("TEXT_3", 0.9))


def _quote(block: md.Quote, ctx: Ctx) -> tuple[float, Painter]:
    lines = wrap(ctx, block.spans, "body", ctx.width - 16)
    height = text_height(ctx, len(lines), "body")

    def paint(img: Image.Image, x: float, y: float) -> None:
        round_rect(img, ctx, (x, y + 2, x + 3, y + height - 2), 1.5, ctx.color("MINT", 0.7))
        paint_lines(img, ctx, lines, "body", x + 14, y, ctx.color("TEXT_2"))

    return height, paint


def _rule(ctx: Ctx) -> tuple[float, Painter]:
    def paint(img: Image.Image, x: float, y: float) -> None:
        line(img, ctx, x, y + 5, x + ctx.width, y + 6, ctx.color("LINE"))

    return 12.0, paint


def _code_lines(ctx: Ctx, text: str) -> list[str]:
    """Líneas de código; las muy largas se parten en vez de salirse de la caja."""
    chars = max(8, int((ctx.width - 2 * CODE_PAD) / max(1.0, ctx.length("M", "code"))))
    out: list[str] = []
    for raw in text.expandtabs(4).split("\n"):
        out += [raw[i : i + chars] for i in range(0, len(raw), chars)] or [""]
    return out


def _code(block: md.Code, ctx: Ctx) -> tuple[float, Painter]:
    rows = _code_lines(ctx, block.text)
    height = CODE_HEAD + len(rows) * CODE_LINE + CODE_PAD

    def paint(img: Image.Image, x: float, y: float) -> None:
        box = (x, y, x + ctx.width, y + height)
        round_rect(img, ctx, box, 9.0, ctx.color("BODY", 0.55), ctx.color("LINE"))
        if block.lang:
            draw_text_at(
                img, ctx, x + CODE_PAD, y + 16, block.lang.upper(), "micro", ctx.color("TEXT_3")
            )
        label = "Copiar"
        tw = ctx.length(label, "micro")
        draw_text_at(
            img, ctx, x + ctx.width - CODE_PAD - tw, y + 16, label, "micro", ctx.color("MINT")
        )
        ctx.hits.append(
            Hit(
                "copy_code",
                (x + ctx.width - CODE_PAD - tw - 8, y, x + ctx.width, y + CODE_HEAD),
                block.text,
            )
        )
        for index, row in enumerate(rows):
            baseline = y + CODE_HEAD + (index + 0.5) * CODE_LINE + 4.2
            draw_text_at(img, ctx, x + CODE_PAD, baseline, row, "code", ctx.color("TEXT"))

    return height, paint


# ------------------------------------------------------------------ maqueta
def layout_block(block: md.Block, ctx: Ctx) -> tuple[float, Painter]:
    """Alto lógico de un bloque y la función que lo pinta en ``(x, y)``."""
    from . import render_rich as rich

    if isinstance(block, md.Heading):
        return _heading(block, ctx)
    if isinstance(block, md.Paragraph):
        return _paragraph(block, ctx)
    if isinstance(block, md.ListItem):
        return _list_item(block, ctx)
    if isinstance(block, md.Quote):
        return _quote(block, ctx)
    if isinstance(block, md.Code):
        return _code(block, ctx)
    if isinstance(block, md.Table):
        return rich.layout_table(block, ctx)
    if isinstance(block, md.Image):
        return rich.layout_image(block, ctx)
    if isinstance(block, md.Document):
        return rich.layout_document(block, ctx)
    return _rule(ctx)


def _gap(prev: md.Block | None, block: md.Block) -> float:
    if prev is None:
        return 0.0
    if isinstance(prev, md.ListItem) and isinstance(block, md.ListItem):
        return 3.0
    if isinstance(block, md.Heading):
        return PARA_GAP + 6.0
    if isinstance(prev, md.Heading):
        return 4.0
    return PARA_GAP


def render_blocks(blocks: list[md.Block], ctx: Ctx) -> tuple[Image.Image, float]:
    """Imagen RGBA con todos los bloques apilados y su alto lógico."""
    placed: list[tuple[float, Painter]] = []
    y = 0.0
    prev: md.Block | None = None
    for block in blocks:
        y += _gap(prev, block)
        height, painter = layout_block(block, ctx)
        placed.append((y, painter))
        y += height
        prev = block
    total = max(y, 1.0)
    img = Image.new("RGBA", (max(1, ctx.px(ctx.width)), max(1, ctx.px(total))), (0, 0, 0, 0))
    for top, painter in placed:
        painter(img, 0.0, top)
    return img, total


def render_message(
    text: str,
    pal: dict[str, Color],
    width: float,
    scale: float = 1.0,
    roots: tuple[Path, ...] | None = None,
    base_dirs: tuple[Path, ...] = (),
) -> Rendered:
    """Parsea y dibuja ``text`` en una columna de ``width`` px lógicos."""
    blocks = md.parse(text)
    ctx = Ctx(scale=scale, pal=pal, width=width, roots=roots, base_dirs=base_dirs)
    image, height = render_blocks(blocks, ctx)
    return Rendered(image, height, tuple(ctx.hits), md.plain_text(blocks))
