"""Fixtures de `ezagent.voice.chat.markdown` (F1-02).

El parser no dibuja ni toca el disco: clasifica. Cada bloque se serializa con
``type`` y sus campos, y cada trozo con ``text/bold/italic/code/link``.

* ``parse_inline``    entrada ``{"text": str}`` -> ``{"spans": [...]}``
* ``parse``           entrada ``{"text": str}`` -> ``{"blocks": [...]}``
* ``plain_text``      entrada ``{"markdown": str}`` -> ``{"text": str}``
  (``plain_text(parse(markdown))``: el texto que se copia del mensaje)
* ``clean_target``    entrada ``{"target": str}`` -> ``{"target": str}``
* ``is_local_target`` entrada ``{"target": str}`` -> ``{"local": bool}``
"""

from __future__ import annotations

from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "chat_markdown"

INLINE_CASES = [
    "hola mundo",
    "",
    "   ",
    "**negrita**",
    "__negrita__",
    "*cursiva*",
    "_cursiva_",
    "**fuerte y _suave_**",
    "***triple***",
    "`código`",
    "``doble ` tick``",
    "`**no es negrita**`",
    "esto **no cierra",
    "un `código sin cerrar",
    "un [enlace](sin cerrar",
    "mira [la **guía**](https://ejemplo.com/a) ya",
    r"abre [el archivo](<C:\a b\x.pdf>) ya",
    '[t](https://x.y "un título")',
    r"\*no cursiva\* y \# no título",
    "usa snake_case_name aquí",
    "2 * 3 * 4",
    "a * b * c",
    "<b>hola</b> <script>alert(1)</script>",
    "emoji 🙂 y ñandú ☕",
    "**a** **b**",
    "[[]](x) y [z](w)",
    "* no es lista *",
    "**`code bold`**",
    "_a_b",
    "pre **bold `code`** post",
    "[a](b) [c]",
    "código `a|b` y **c**",
    "a\\\\b y \\`escape\\`",
    "texto con | pipes | y **negrita**",
]

DOCUMENT_CASES = [
    "",
    "\n\n   \n",
    "# Título",
    "###### seis",
    "####### siete",
    "#sin espacio",
    "# Título ###",
    "## **negrita** en título",
    "Un párrafo\nque sigue.\n\ny otro",
    "- uno\n  - anidado\n- dos",
    "1. primero\n2) segundo",
    "- [x] hecho\n- [ ] falta\n- [X] mayúscula",
    "* asterisco\n+ más",
    "- uno\n  continuación",
    "- a\n  - b\n    - c\n      - d\n        - e\n          - f",
    "\t- con tabulador\n    - cuatro espacios",
    "- [ ]",
    "1) sin espacio",
    "```python\nprint('hola')\nx = 1\n```",
    "```sql\nSELECT 1;\nSELECT 2",
    "```\n# no título\n**no negrita**\n| a | b |\n```",
    "~~~js\nconst a = 1;\n~~~",
    "````md\n```\nanidado\n```\n````",
    "```\n",
    "> una cita\n> larga",
    ">cita sin espacio\n\n> otra",
    "---",
    "***",
    "___",
    "- - -",
    "| Plan | Precio | Notas |\n|:--|--:|:-:|\n| Básico | 5 | **ok** |\n| Pro | 12 |",
    "| a | b |\n|---|---|\n| x \\| y | z |",
    "a | b\n--- | ---\n1 | 2",
    "usa a | b para unir",
    "| solo |\n|---|",
    "Mira:\n\n![gráfico de ventas](C:\\datos\\ventas.png)\n\nlisto",
    '![logo](https://ejemplo.com/a.png "El logo")',
    r"![gráfico](<C:\Users\Ana Pérez\ventas por región.png>)",
    "[Informe trimestral](C:\\Users\\Ana Pérez\\Informe Q3.docx)",
    "- [A](C:\\a.pdf)\n- [B](C:\\b.pdf)",
    "[sitio](https://ejemplo.com)",
    "abre [el informe](C:\\a.pdf) cuando puedas",
    "[Informe](<C:\\ruta con espacios\\x.docx>)",
    "a\r\nb\rc",
    "<b>hola</b> <script>alert(1)</script>",
    "*" * 500,
    "|||\n|---|",
    "# ñandú ☕ 🙂\n\ntexto **ñ** y `código`",
    "línea con\ttabulador",
    "    código indentado",
]

TARGET_CASES = [
    "  x  ",
    "<x>",
    "< x >",
    "<>",
    "",
    "   ",
    r"C:\Users\Ana Pérez\Informe Q3.docx",
    r"<C:\Users\Ana Pérez\Informe Q3.docx>",
    "https://ejemplo.com/a",
    " <https://x.y> ",
    "x>",
    "<x",
    "ñandú ☕",
    "<ñandú ☕>",
    "\n x \n",
    "a\nb",
    "<<x>>",
    ">x<",
    "C:/docs/informe.docx",
    "~/informe.pdf",
    'ruta con "comillas"',
    "file:///C:/x",
]

LOCAL_CASES = [
    r"C:\x",
    "c:/x",
    r"\\servidor\x",
    "file:///C:/x",
    "~/x",
    "./x",
    "../x",
    "/x",
    "https://x.com",
    "http://x.com",
    "mailto:a@b.c",
    "ejemplo.com/x",
    "",
    "   ",
    "C:x",
    "c:x",
    r"\x",
    "//x",
    "~/",
    ".",
    "..",
    "file:x",
    "FILE:///x",
    "Café/x",
    " /x",
]


def _span(span: Any) -> dict[str, Any]:
    return {
        "text": span.text,
        "bold": span.bold,
        "italic": span.italic,
        "code": span.code,
        "link": span.link,
    }


def _spans(spans: tuple[Any, ...]) -> list[dict[str, Any]]:
    return [_span(span) for span in spans]


def _block(block: Any) -> dict[str, Any]:
    from ezagent.voice.chat import markdown as md

    if isinstance(block, md.Heading):
        return {"type": "heading", "level": block.level, "spans": _spans(block.spans)}
    if isinstance(block, md.Paragraph):
        return {"type": "paragraph", "spans": _spans(block.spans)}
    if isinstance(block, md.ListItem):
        return {
            "type": "list_item",
            "level": block.level,
            "marker": block.marker,
            "checked": block.checked,
            "spans": _spans(block.spans),
        }
    if isinstance(block, md.Quote):
        return {"type": "quote", "spans": _spans(block.spans)}
    if isinstance(block, md.Code):
        return {"type": "code", "lang": block.lang, "text": block.text}
    if isinstance(block, md.Table):
        return {
            "type": "table",
            "header": [_spans(cell) for cell in block.header],
            "rows": [[_spans(cell) for cell in row] for row in block.rows],
            "aligns": list(block.aligns),
        }
    if isinstance(block, md.Rule):
        return {"type": "rule"}
    if isinstance(block, md.Image):
        return {"type": "image", "alt": block.alt, "src": block.src}
    if isinstance(block, md.Document):
        return {"type": "document", "label": block.label, "target": block.target}
    raise TypeError(f"bloque desconocido: {type(block).__name__}")


def _blocks(markdown: str) -> list[dict[str, Any]]:
    from ezagent.voice.chat import markdown as md

    return [_block(block) for block in md.parse(markdown)]


def _add_parse_cases(out: FixtureFile, md: Any) -> None:
    for index, text in enumerate(INLINE_CASES, start=1):
        out.add(
            f"chat_markdown/parse_inline/{index:03d}",
            "parse_inline",
            {"text": text},
            {"spans": _spans(md.parse_inline(text))},
        )
    out.add(
        "chat_markdown/parse_inline/100",
        "parse_inline",
        {"text": None},
        {"spans": _spans(md.parse_inline(None))},
    )
    for index, text in enumerate(DOCUMENT_CASES, start=1):
        out.add(
            f"chat_markdown/parse/{index:03d}",
            "parse",
            {"text": text},
            {"blocks": _blocks(text)},
        )
    out.add(
        "chat_markdown/parse/100",
        "parse",
        {"text": None},
        {"blocks": _blocks(None)},
    )


def _add_text_cases(out: FixtureFile, md: Any) -> None:
    for index, text in enumerate(DOCUMENT_CASES, start=1):
        out.add(
            f"chat_markdown/plain_text/{index:03d}",
            "plain_text",
            {"markdown": text},
            {"text": md.plain_text(md.parse(text))},
        )
    for index, target in enumerate(TARGET_CASES, start=1):
        out.add(
            f"chat_markdown/clean_target/{index:03d}",
            "clean_target",
            {"target": target},
            {"target": md.clean_target(target)},
        )
    for index, target in enumerate(LOCAL_CASES, start=1):
        out.add(
            f"chat_markdown/is_local_target/{index:03d}",
            "is_local_target",
            {"target": target},
            {"local": md.is_local_target(target)},
        )
    for fn_name, call in (
        ("clean_target", md.clean_target),
        ("is_local_target", md.is_local_target),
    ):
        try:
            call(None)
        except AttributeError as error:
            out.add_error(f"chat_markdown/{fn_name}/error01", fn_name, {"target": None}, str(error))


def generate(out: FixtureFile) -> None:
    from ezagent.voice.chat import markdown as md

    _add_parse_cases(out, md)
    _add_text_cases(out, md)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
