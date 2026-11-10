"""Markdown para el chat: texto del agente -> bloques que ``chat_ui`` sabe dibujar.

Es un subconjunto pensado para respuestas de un asistente: títulos, párrafos,
**negrita**, *cursiva*, `código`, listas (anidadas y de tareas), tablas, citas,
código en bloque, reglas, imágenes y enlaces a archivos locales.

Lo que llega del modelo es dato no confiable: no se interpreta HTML y el parser
no toca el disco. Solo clasifica; ``media`` decide qué se puede mostrar o abrir.

Tolera el texto a medias del streaming: un bloque de código sin cerrar sigue
siendo código y un énfasis sin cerrar queda como texto.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Literal

# ------------------------------------------------------------------- inline


@dataclass(frozen=True, slots=True)
class Span:
    """Un trozo de texto con su estilo."""

    text: str
    bold: bool = False
    italic: bool = False
    code: bool = False
    link: str | None = None


# ------------------------------------------------------------------- bloques


@dataclass(frozen=True, slots=True)
class Heading:
    level: int
    spans: tuple[Span, ...]


@dataclass(frozen=True, slots=True)
class Paragraph:
    spans: tuple[Span, ...]


@dataclass(frozen=True, slots=True)
class ListItem:
    level: int  # 0 = primer nivel
    marker: str  # «•» o «1.»
    spans: tuple[Span, ...]
    checked: bool | None = None  # lista de tareas: True/False; None si no lo es


@dataclass(frozen=True, slots=True)
class Quote:
    spans: tuple[Span, ...]


@dataclass(frozen=True, slots=True)
class Code:
    lang: str
    text: str


@dataclass(frozen=True, slots=True)
class Table:
    header: tuple[tuple[Span, ...], ...]
    rows: tuple[tuple[tuple[Span, ...], ...], ...]
    aligns: tuple[Literal["left", "center", "right"], ...]


@dataclass(frozen=True, slots=True)
class Rule:
    pass


@dataclass(frozen=True, slots=True)
class Image:
    """``![texto](origen)``: el origen puede ser local o web; ``media`` decide."""

    alt: str
    src: str


@dataclass(frozen=True, slots=True)
class Document:
    """Una línea que es solo un enlace a un archivo local: se muestra como tarjeta."""

    label: str
    target: str


Block = Heading | Paragraph | ListItem | Quote | Code | Table | Rule | Image | Document


_TOKEN = re.compile(
    r"""
    (?P<esc>\\[\\`*_{}\[\]()\#+\-.!|>~])
  | (?P<code>`+)
  | (?P<bold>\*\*(?=\S)|__(?=\S))
  | (?P<italic>\*(?=[^\s*])|(?<![\w])_(?=[^\s_]))
  | (?P<link>\[)
    """,
    re.VERBOSE,
)
_LINK_TAIL = re.compile(r"\]\(\s*(<[^>\n]+>|[^)\s]+)(?:\s+\"[^\"]*\")?\s*\)")


def clean_target(target: str) -> str:
    """El destino sin los «<>» opcionales ni espacios sobrantes."""
    text = target.strip()
    if text.startswith("<") and text.endswith(">"):
        text = text[1:-1].strip()
    return text


def parse_inline(text: str) -> tuple[Span, ...]:
    """Parte una línea en trozos con estilo; el énfasis sin cerrar queda literal."""
    return tuple(_merge(_inline(str(text), False, False, None)))


def _inline(text: str, bold: bool, italic: bool, link: str | None) -> list[Span]:
    out: list[Span] = []
    pos = 0
    plain_from = 0

    def flush(end: int) -> None:
        if end > plain_from:
            out.append(Span(text[plain_from:end], bold, italic, False, link))

    while pos < len(text):
        match = _TOKEN.search(text, pos)
        if match is None:
            break
        start = match.start()
        kind = match.lastgroup
        if kind == "esc":
            flush(start)
            out.append(Span(match.group()[1], bold, italic, False, link))
            pos = plain_from = match.end()
        elif kind == "code":
            ticks = match.group()
            close = text.find(ticks, match.end())
            if close < 0:
                pos = match.end()
                continue
            flush(start)
            inner = text[match.end() : close]
            inner = inner.strip() if inner.strip() else inner
            out.append(Span(inner, bold, italic, True, link))
            pos = plain_from = close + len(ticks)
        elif kind in ("bold", "italic"):
            marker = match.group()
            close = _find_close(text, marker, match.end())
            if close < 0:
                pos = match.end()
                continue
            flush(start)
            inner = text[match.end() : close]
            is_bold = kind == "bold"
            out += _inline(inner, bold or is_bold, italic or not is_bold, link)
            pos = plain_from = close + len(marker)
        else:  # link
            end = _link_end(text, match.end())
            tail = _LINK_TAIL.match(text, end) if end >= 0 else None
            if tail is None:
                pos = match.end()
                continue
            flush(start)
            out += _inline(text[match.end() : end], bold, italic, clean_target(tail.group(1)))
            pos = plain_from = tail.end()
    flush(len(text))
    return out


def _find_close(text: str, marker: str, start: int) -> int:
    """Posición del cierre de un énfasis: el mismo marcador, sin espacio antes."""
    pos = start
    while True:
        close = text.find(marker, pos)
        if close < 0:
            return -1
        before = text[close - 1] if close > start else ""
        after = text[close + len(marker)] if close + len(marker) < len(text) else ""
        escaped = close > 0 and text[close - 1] == "\\"
        # «_» dentro de una palabra (snake_case) no cierra; «*» sí.
        word_underscore = marker.startswith("_") and after.isalnum()
        if close > start and not before.isspace() and not escaped and not word_underscore:
            return close
        pos = close + 1


def _link_end(text: str, start: int) -> int:
    """Índice del «]» que cierra el texto de un enlace (admite corchetes anidados)."""
    depth = 1
    for index in range(start, len(text)):
        char = text[index]
        if char == "[":
            depth += 1
        elif char == "]":
            depth -= 1
            if depth == 0:
                return index
    return -1


def _merge(spans: list[Span]) -> list[Span]:
    out: list[Span] = []
    for span in spans:
        if not span.text:
            continue
        if out and not span.code and not out[-1].code and _same_style(out[-1], span):
            out[-1] = Span(out[-1].text + span.text, span.bold, span.italic, False, span.link)
        else:
            out.append(span)
    return out


def _same_style(a: Span, b: Span) -> bool:
    return (a.bold, a.italic, a.link) == (b.bold, b.italic, b.link)


# ------------------------------------------------------------------- bloques
_HEADING = re.compile(r"^ {0,3}(#{1,6})\s+(.*?)\s*#*\s*$")
_FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})\s*([\w+#.-]*)\s*$")
_RULE = re.compile(r"^ {0,3}([-*_])(?:\s*\1){2,}\s*$")
_BULLET = re.compile(r"^(\s*)([-*+])\s+(.*)$")
_ORDERED = re.compile(r"^(\s*)(\d{1,9})[.)]\s+(.*)$")
_TASK = re.compile(r"^\[([ xX])\]\s+(.*)$")
_QUOTE = re.compile(r"^ {0,3}>\s?(.*)$")
# Destino de un enlace: «<ruta con espacios>», una ruta local (que en Windows suele tener
# espacios: «C:\Users\Ana Pérez\x.docx») o cualquier cosa sin espacios.
_TARGET = (
    r"(<[^>\n]+>"
    r"|(?:[A-Za-z]:[\\/]|\\\\|file:|~[\\/]|\.{1,2}[\\/]|/)[^)\n]*?"
    r"|[^)\s]+)"
)
_TITLE = r"(?:\s+\"[^\"]*\")?"
_IMAGE_LINE = re.compile(r"^\s*!\[([^\]]*)\]\(\s*" + _TARGET + _TITLE + r"\s*\)\s*$")
_LINK_LINE = re.compile(r"^\s*(?:[-*+]\s+)?\[([^\]]+)\]\(\s*" + _TARGET + _TITLE + r"\s*\)\s*$")
_TABLE_SEP = re.compile(r"^\s*\|?\s*:?-{1,}:?\s*(\|\s*:?-{1,}:?\s*)*\|?\s*$")
_LOCAL = re.compile(r"^(?:[A-Za-z]:[\\/]|\\\\|file:|~[\\/]|\.{1,2}[\\/]|/)")


def is_local_target(target: str) -> bool:
    """¿Parece una ruta de archivo local (y no una dirección web)?"""
    return bool(_LOCAL.match(target.strip()))


def parse(text: str) -> list[Block]:
    """Convierte markdown en bloques. Nunca falla: lo que no entiende es un párrafo."""
    lines = str(text or "").replace("\r\n", "\n").replace("\r", "\n").split("\n")
    blocks: list[Block] = []
    para: list[str] = []
    index = 0

    def end_paragraph() -> None:
        if para:
            blocks.append(Paragraph(parse_inline(" ".join(part.strip() for part in para))))
            para.clear()

    while index < len(lines):
        line = lines[index]
        if not line.strip():
            end_paragraph()
            index += 1
            continue
        fence = _FENCE.match(line)
        if fence:
            end_paragraph()
            index = _read_code(lines, index, fence, blocks)
            continue
        if _try_single_line(line, blocks, end_paragraph):
            index += 1
            continue
        if _is_table_start(lines, index):
            end_paragraph()
            index = _read_table(lines, index, blocks)
            continue
        quote = _QUOTE.match(line)
        if quote:
            end_paragraph()
            index = _read_quote(lines, index, blocks)
            continue
        item = _list_item(line)
        if item is not None:
            end_paragraph()
            index = _read_list(lines, index, blocks)
            continue
        para.append(line)
        index += 1
    end_paragraph()
    return blocks


def _try_single_line(line: str, blocks: list[Block], end_paragraph) -> bool:
    heading = _HEADING.match(line)
    if heading:
        end_paragraph()
        blocks.append(Heading(len(heading.group(1)), parse_inline(heading.group(2))))
        return True
    if _RULE.match(line):
        end_paragraph()
        blocks.append(Rule())
        return True
    image = _IMAGE_LINE.match(line)
    if image:
        end_paragraph()
        blocks.append(Image(image.group(1).strip(), clean_target(image.group(2))))
        return True
    link = _LINK_LINE.match(line)
    if link and is_local_target(clean_target(link.group(2))):
        end_paragraph()
        blocks.append(Document(link.group(1).strip(), clean_target(link.group(2))))
        return True
    return False


def _read_code(lines: list[str], index: int, fence: re.Match[str], blocks: list[Block]) -> int:
    marker = fence.group(1)
    body: list[str] = []
    index += 1
    while index < len(lines):
        closing = lines[index].strip()
        if closing.startswith(marker[0] * len(marker)) and set(closing) == {marker[0]}:
            index += 1
            break
        body.append(lines[index])
        index += 1
    blocks.append(Code(fence.group(2), "\n".join(body)))
    return index


def _split_row(line: str) -> list[str]:
    text = line.strip()
    if text.startswith("|"):
        text = text[1:]
    if text.endswith("|") and not text.endswith("\\|"):
        text = text[:-1]
    cells, current, escaped = [], [], False
    for char in text:
        if escaped:
            current.append(char)
            escaped = False
        elif char == "\\":
            escaped = True
            current.append(char)
        elif char == "|":
            cells.append("".join(current).strip())
            current = []
        else:
            current.append(char)
    cells.append("".join(current).strip())
    return cells


def _is_table_start(lines: list[str], index: int) -> bool:
    if index + 1 >= len(lines) or "|" not in lines[index]:
        return False
    sep = lines[index + 1]
    return bool(_TABLE_SEP.match(sep)) and "-" in sep and "|" in sep


def _align_of(cell: str) -> Literal["left", "center", "right"]:
    cell = cell.strip()
    if cell.startswith(":") and cell.endswith(":") and len(cell) > 1:
        return "center"
    return "right" if cell.endswith(":") else "left"


def _read_table(lines: list[str], index: int, blocks: list[Block]) -> int:
    header = _split_row(lines[index])
    aligns = tuple(_align_of(cell) for cell in _split_row(lines[index + 1]))
    width = len(header)
    rows: list[tuple[tuple[Span, ...], ...]] = []
    index += 2
    while index < len(lines) and lines[index].strip() and "|" in lines[index]:
        cells = _split_row(lines[index])[:width]
        cells += [""] * (width - len(cells))
        rows.append(tuple(parse_inline(cell) for cell in cells))
        index += 1
    aligns = (aligns + ("left",) * width)[:width]
    blocks.append(Table(tuple(parse_inline(cell) for cell in header), tuple(rows), aligns))
    return index


def _read_quote(lines: list[str], index: int, blocks: list[Block]) -> int:
    parts: list[str] = []
    while index < len(lines):
        match = _QUOTE.match(lines[index])
        if not match:
            break
        parts.append(match.group(1).strip())
        index += 1
    blocks.append(Quote(parse_inline(" ".join(part for part in parts if part))))
    return index


def _list_item(line: str) -> tuple[int, str, str] | None:
    """``(nivel, marcador, texto)`` si la línea abre un elemento de lista."""
    bullet = _BULLET.match(line)
    if bullet and not _RULE.match(line):
        return (len(bullet.group(1).expandtabs(4)) // 2, "•", bullet.group(3))
    ordered = _ORDERED.match(line)
    if ordered:
        return (len(ordered.group(1).expandtabs(4)) // 2, ordered.group(2) + ".", ordered.group(3))
    return None


def _read_list(lines: list[str], index: int, blocks: list[Block]) -> int:
    pending: list[tuple[int, str, str]] = []
    while index < len(lines):
        line = lines[index]
        item = _list_item(line)
        if item is not None:
            pending.append(item)
        elif line.strip() and line.startswith((" ", "\t")) and pending:
            level, marker, text = pending[-1]  # continuación del elemento anterior
            pending[-1] = (level, marker, text + " " + line.strip())
        else:
            break
        index += 1
    base = min(level for level, _, _ in pending)
    for level, marker, text in pending:
        task = _TASK.match(text)
        checked = None
        if task:
            checked = task.group(1).lower() == "x"
            text = task.group(2)
        # Los niveles se normalizan: saltarse un sangrado no deja huecos raros.
        blocks.append(ListItem(min(level - base, 4), marker, parse_inline(text), checked))
    return index


def plain_text(blocks: list[Block]) -> str:
    """El texto sin formato (para copiar un mensaje al portapapeles)."""
    out: list[str] = []
    for block in blocks:
        if isinstance(block, (Heading, Paragraph, Quote)):
            out.append("".join(span.text for span in block.spans))
        elif isinstance(block, ListItem):
            mark = {True: "[x] ", False: "[ ] ", None: ""}[block.checked]
            out.append(
                "  " * block.level + f"{block.marker} {mark}" + "".join(s.text for s in block.spans)
            )
        elif isinstance(block, Code):
            out.append(block.text)
        elif isinstance(block, Table):
            for row in (block.header, *block.rows):
                out.append(" | ".join("".join(s.text for s in cell) for cell in row))
        elif isinstance(block, Image):
            out.append(f"[imagen: {block.alt or block.src}]")
        elif isinstance(block, Document):
            out.append(f"{block.label} ({block.target})")
    return "\n".join(out)


__all__ = [
    "Block",
    "Code",
    "Document",
    "Heading",
    "Image",
    "ListItem",
    "Paragraph",
    "Quote",
    "Rule",
    "Span",
    "Table",
    "clean_target",
    "is_local_target",
    "parse",
    "parse_inline",
    "plain_text",
]
