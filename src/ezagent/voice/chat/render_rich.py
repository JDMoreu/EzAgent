"""Bloques ricos del chat: tablas, imágenes y tarjetas de documento (con vista previa).

Lo que sale de aquí es lo que hace del chat algo más que texto: comparativas en tablas,
imágenes que el agente genera o encuentra y documentos que crea (Word, PDF, Excel…).
Nada se abre por su cuenta: una tarjeta ofrece «Abrir» y «Mostrar» y decide el usuario.
"""

from __future__ import annotations

import time
from urllib.parse import urlparse

from PIL import Image, ImageChops, ImageDraw

from . import markdown as md
from . import media
from .render_base import (
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

CELL_PAD_X = 10.0
CELL_PAD_Y = 7.0
CELL_MIN = 48.0
CARD_H = 64.0
PREVIEW_MAX = 190.0
IMAGE_MAX_H = 340.0
_MONTHS = ("ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic")
# Color de la pastilla de cada familia de documento (tono cálido o frío, sin gritar).
_FAMILY = {
    ".pdf": (214, 76, 70),
    ".doc": (58, 112, 214),
    ".docx": (58, 112, 214),
    ".odt": (58, 112, 214),
    ".xls": (40, 150, 96),
    ".xlsx": (40, 150, 96),
    ".ods": (40, 150, 96),
    ".csv": (40, 150, 96),
    ".ppt": (222, 120, 50),
    ".pptx": (222, 120, 50),
    ".odp": (222, 120, 50),
}


# -------------------------------------------------------------------- tablas
def _natural(ctx: Ctx, spans, key: str) -> float:
    return max((sum(run.width for run in ln) for ln in wrap(ctx, spans, key, 1e9)), default=0.0)


def _column_widths(ctx: Ctx, table: md.Table) -> list[float]:
    n = len(table.header)
    natural = [CELL_MIN] * n
    for row, key in [(table.header, "cellbold"), *((r, "cell") for r in table.rows)]:
        for index, cell in enumerate(row):
            natural[index] = max(natural[index], _natural(ctx, cell, key))
    pad = 2 * CELL_PAD_X
    widths = [w + pad for w in natural]
    excess = sum(widths) - ctx.width
    # Con muchas columnas el mínimo cómodo no cabe: se baja hasta repartir el ancho a partes iguales.
    floor = min(CELL_MIN + pad, ctx.width / n)
    while excess > 0.5:
        room = [max(0.0, w - floor) for w in widths]
        if sum(room) <= 0.01:
            break
        take = min(excess, sum(room))
        widths = [w - take * r / sum(room) for w, r in zip(widths, room, strict=True)]
        excess -= take
    return widths


def layout_table(table: md.Table, ctx: Ctx) -> tuple[float, Painter]:
    widths = _column_widths(ctx, table)
    rows = [(table.header, "cellbold"), *((r, "cell") for r in table.rows)]
    wrapped = [
        [wrap(ctx, cell, key, widths[i] - 2 * CELL_PAD_X) for i, cell in enumerate(row)]
        for row, key in rows
    ]
    heights = []
    for (_, key), cells in zip(rows, wrapped, strict=True):
        heights.append(max(text_height(ctx, len(c), key) for c in cells) + 2 * CELL_PAD_Y)
    total = sum(heights)

    def paint(img: Image.Image, x: float, y: float) -> None:
        w = sum(widths)
        round_rect(
            img, ctx, (x, y, x + w, y + total), 9.0, ctx.color("BODY", 0.35), ctx.color("LINE")
        )
        top = y
        for r, ((_, key), cells) in enumerate(zip(rows, wrapped, strict=True)):
            if r == 0:
                _band(img, ctx, x, top, w, heights[r], "TRACK", 0.40, first=True)
            elif r % 2 == 0:
                _band(img, ctx, x, top, w, heights[r], "TRACK", 0.14, first=False)
            if r:
                line(img, ctx, x + 1, top, x + w - 1, top + 1, ctx.color("LINE"))
            left = x
            for c, lines in enumerate(cells):
                _paint_cell(img, ctx, lines, key, table.aligns[c], left, top, widths[c], r == 0)
                left += widths[c]
            top += heights[r]

    return total, paint


def _band(img, ctx, x, y, w, h, color, alpha, first) -> None:
    """Franja de fondo de una fila (la primera con las esquinas de arriba redondeadas)."""
    layer = Image.new("RGBA", (ctx.px(w) - 2, ctx.px(h)), ctx.color(color, alpha))
    if first:
        mask = Image.new("L", layer.size, 0)
        ImageDraw.Draw(mask).rounded_rectangle(
            (0, 0, layer.width - 1, layer.height + ctx.px(12)), radius=ctx.px(8), fill=255
        )
        layer.putalpha(ImageChops.multiply(mask, layer.getchannel("A")))
    img.alpha_composite(layer, (ctx.px(x) + 1, ctx.px(y)))


def _paint_cell(img, ctx, lines, key, align, left, top, width, header) -> None:
    inner = width - 2 * CELL_PAD_X
    color = ctx.color("TEXT" if header else "TEXT", 1.0 if header else 0.92)
    shifted = []
    for runs in lines:
        used = sum(run.width for run in runs)
        offset = {"left": 0.0, "center": (inner - used) / 2, "right": inner - used}[align]
        shifted.append(
            [type(run)(run.x + max(0.0, offset), run.text, run.piece, run.width) for run in runs]
        )
    paint_lines(img, ctx, shifted, key, left + CELL_PAD_X, top + CELL_PAD_Y, color)


# ------------------------------------------------------------------ imágenes
def _placeholder(ctx: Ctx, title: str, detail: str) -> tuple[float, Painter]:
    def paint(img: Image.Image, x: float, y: float) -> None:
        round_rect(
            img,
            ctx,
            (x, y, x + ctx.width, y + 54),
            9.0,
            ctx.color("BODY", 0.35),
            ctx.color("LINE"),
        )
        _tile(img, ctx, x + 12, y + 11, 32, "IMG", (120, 130, 150))
        draw_text_at(img, ctx, x + 56, y + 23, title, "smallbold", ctx.color("TEXT_2"))
        draw_text_at(
            img,
            ctx,
            x + 56,
            y + 40,
            _clip(ctx, detail, ctx.width - 72, "small"),
            "small",
            ctx.color("TEXT_3"),
        )

    return 54.0, paint


def layout_image(block: md.Image, ctx: Ctx) -> tuple[float, Painter]:
    if not md.is_local_target(block.src):
        host = urlparse(block.src).netloc or block.src
        return _placeholder(ctx, "Imagen de la web", f"No se descarga sola · {host}")
    path = media.resolve_target(block.src, ctx.base_dirs)
    info = media.file_info(path, ctx.roots)
    if info is None:
        return _placeholder(ctx, "No encuentro la imagen", block.src)
    if not info.allowed:
        return _placeholder(ctx, "Imagen fuera de las carpetas permitidas", info.name)
    image = media.load_image(info)
    if image is None:
        return _placeholder(ctx, "No puedo mostrar esta imagen", info.name)
    return _image_block(ctx, block, info, image)


def _image_block(ctx: Ctx, block: md.Image, info: media.FileInfo, image: Image.Image):
    natural_w, natural_h = image.width / ctx.scale, image.height / ctx.scale
    k = min(1.0, ctx.width / natural_w, IMAGE_MAX_H / natural_h)
    w, h = max(8.0, natural_w * k), max(8.0, natural_h * k)
    caption = block.alt.strip()
    extra = 20.0 if caption else 0.0

    def paint(img: Image.Image, x: float, y: float) -> None:
        pw, ph = ctx.px(w), ctx.px(h)
        shown = image.resize((pw, ph), Image.LANCZOS)
        mask = Image.new("L", (pw * 4, ph * 4), 0)
        ImageDraw.Draw(mask).rounded_rectangle(
            (0, 0, pw * 4 - 1, ph * 4 - 1), radius=ctx.px(10) * 4, fill=255
        )
        shown.putalpha(
            ImageChops.multiply(mask.resize((pw, ph), Image.LANCZOS), shown.getchannel("A"))
        )
        img.alpha_composite(shown, (ctx.px(x), ctx.px(y)))
        round_rect(img, ctx, (x, y, x + w, y + h), 10.0, None, ctx.color("LINE"))
        if caption:
            text = _clip(ctx, caption, ctx.width, "small")
            draw_text_at(img, ctx, x, y + h + 15, text, "small", ctx.color("TEXT_3"))
        ctx.hits.append(Hit("image", (x, y, x + w, y + h), info))

    return h + extra, paint


# ----------------------------------------------------------------- documentos
def _clip(ctx: Ctx, text: str, width: float, key: str) -> str:
    """Recorta ``text`` con «…» para que quepa en ``width``."""
    if ctx.length(text, key) <= width:
        return text
    while text and ctx.length(text + "…", key) > width:
        text = text[:-1]
    return text.rstrip() + "…"


def _tile(img: Image.Image, ctx: Ctx, x: float, y: float, size: float, label: str, rgb) -> None:
    """Pastilla cuadrada con la extensión del archivo."""
    round_rect(img, ctx, (x, y, x + size, y + size), 9.0, (*rgb, 46), (*rgb, 150))
    tw = ctx.length(label, "micro")
    draw_text_at(img, ctx, x + (size - tw) / 2, y + size / 2 + 3.6, label, "micro", (*rgb, 255))


def _date(mtime: float) -> str:
    t = time.localtime(mtime)
    return f"{t.tm_mday} {_MONTHS[t.tm_mon - 1]} {t.tm_year}"


def _button(img, ctx, x, y, label, primary, w) -> tuple[float, float, float, float]:
    box = (x, y, x + w, y + 26)
    if primary:
        round_rect(img, ctx, box, 8.0, ctx.color("MINT_BTN"))
        color = ctx.color("MINT_INK")
    else:
        round_rect(img, ctx, box, 8.0, None, ctx.color("TEXT_3", 0.6))
        color = ctx.color("TEXT_2")
    tw = ctx.length(label, "smallbold")
    draw_text_at(img, ctx, x + (w - tw) / 2, y + 17.5, label, "smallbold", color)
    return box


def layout_document(block: md.Document, ctx: Ctx) -> tuple[float, Painter]:
    path = media.resolve_target(block.target, ctx.base_dirs)
    info = media.file_info(path, ctx.roots)
    if info is None:
        return _missing_card(ctx, block)
    preview = media.text_preview(info)
    sub, sub_h = (None, 0.0)
    if preview is not None:
        sub, sub_h = _preview_image(ctx, *preview)
    height = CARD_H + (sub_h + 12.0 if sub is not None else 0.0)

    def paint(img: Image.Image, x: float, y: float) -> None:
        round_rect(
            img,
            ctx,
            (x, y, x + ctx.width, y + height),
            11.0,
            ctx.color("BODY", 0.35),
            ctx.color("LINE"),
        )
        label = info.ext.lstrip(".").upper()[:4] or "ARCH"
        _tile(img, ctx, x + 12, y + 12, 40, label, _FAMILY.get(info.ext, (120, 130, 150)))
        right = _card_buttons(img, ctx, info, x, y)
        title = block.label or info.name
        draw_text_at(
            img,
            ctx,
            x + 64,
            y + 28,
            _clip(ctx, title, right - x - 76, "bold"),
            "bold",
            ctx.color("TEXT"),
        )
        sub_text = f"{label} · {media.human_size(info.size)} · {_date(info.mtime)}"
        if not info.allowed:
            sub_text = "Fuera de las carpetas permitidas"
        draw_text_at(
            img,
            ctx,
            x + 64,
            y + 46,
            _clip(ctx, sub_text, right - x - 76, "small"),
            "small",
            ctx.color("TEXT_3"),
        )
        if sub is not None:
            img.alpha_composite(sub, (ctx.px(x + 12), ctx.px(y + CARD_H)))

    return height, paint


def _card_buttons(img, ctx, info: media.FileInfo, x: float, y: float) -> float:
    """Botones de la derecha; devuelve la ``x`` donde empiezan (para recortar el título)."""
    right = x + ctx.width - 12
    if not info.allowed:
        return right
    show_w, open_w = 64.0, 56.0
    bx = right - show_w
    ctx.hits.append(Hit("reveal", _button(img, ctx, bx, y + 19, "Mostrar", False, show_w), info))
    if info.openable:
        bx -= open_w + 8
        ctx.hits.append(Hit("open", _button(img, ctx, bx, y + 19, "Abrir", True, open_w), info))
    return bx - 8


def _missing_card(ctx: Ctx, block: md.Document) -> tuple[float, Painter]:
    def paint(img: Image.Image, x: float, y: float) -> None:
        round_rect(
            img,
            ctx,
            (x, y, x + ctx.width, y + 54),
            11.0,
            ctx.color("BODY", 0.25),
            ctx.color("LINE"),
        )
        _tile(img, ctx, x + 12, y + 11, 32, "?", (150, 130, 110))
        draw_text_at(
            img, ctx, x + 56, y + 23, "No encuentro este archivo", "smallbold", ctx.color("TEXT_2")
        )
        draw_text_at(
            img,
            ctx,
            x + 56,
            y + 40,
            _clip(ctx, block.target, ctx.width - 72, "small"),
            "small",
            ctx.color("TEXT_3"),
        )

    return 54.0, paint


def _preview_image(ctx: Ctx, text: str, more: bool) -> tuple[Image.Image | None, float]:
    """Vista previa del contenido: markdown dibujado en una caja de alto limitado."""
    from .render import render_blocks

    inner = ctx.child(ctx.width - 24 - 20)
    body, height = render_blocks(md.parse(text + ("\n\n…" if more else "")), inner)
    shown = min(height, PREVIEW_MAX)
    w, h = ctx.px(ctx.width - 24), ctx.px(shown + 20)
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    box = Ctx(ctx.scale, ctx.pal, ctx.width - 24)
    round_rect(out, box, (0, 0, ctx.width - 24, shown + 20), 8.0, ctx.color("BODY", 0.6))
    out.alpha_composite(body.crop((0, 0, body.width, ctx.px(shown))), (ctx.px(10), ctx.px(10)))
    if height > PREVIEW_MAX:  # desvanecido al final: «hay más»
        fade = Image.linear_gradient("L").resize((w, ctx.px(34)))
        veil = Image.new("RGBA", (w, ctx.px(34)), ctx.color("BODY", 0.9))
        veil.putalpha(ImageChops.multiply(fade, veil.getchannel("A")))
        out.alpha_composite(veil, (0, h - ctx.px(34)))
    return out, shown + 20
