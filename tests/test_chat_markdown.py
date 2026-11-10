"""Markdown del chat: lo que escribe el agente se convierte en bloques dibujables."""

from __future__ import annotations

import pytest

from ezagent.voice.chat.markdown import (
    Code,
    Document,
    Heading,
    Image,
    ListItem,
    Paragraph,
    Quote,
    Rule,
    Span,
    Table,
    is_local_target,
    parse,
    parse_inline,
    plain_text,
)


def _text(spans) -> str:
    return "".join(span.text for span in spans)


# ------------------------------------------------------------------- inline
def test_plain_text_is_one_span() -> None:
    assert parse_inline("hola mundo") == (Span("hola mundo"),)


def test_bold_italic_and_code() -> None:
    spans = parse_inline("a **negrita** b *cursiva* c `código` d")
    assert [(s.text, s.bold, s.italic, s.code) for s in spans if s.bold or s.italic or s.code] == [
        ("negrita", True, False, False),
        ("cursiva", False, True, False),
        ("código", False, False, True),
    ]
    assert _text(spans) == "a negrita b cursiva c código d"


def test_nested_emphasis_and_underscores() -> None:
    spans = parse_inline("**fuerte y _suave_**")
    assert any(s.bold and s.italic and s.text == "suave" for s in spans)
    assert _text(parse_inline("usa snake_case_name aquí")) == "usa snake_case_name aquí"
    assert _text(parse_inline("2 * 3 * 4")) == "2 * 3 * 4"


def test_unclosed_emphasis_stays_literal() -> None:
    assert _text(parse_inline("esto **no cierra")) == "esto **no cierra"
    assert _text(parse_inline("un `código sin cerrar")) == "un `código sin cerrar"
    assert _text(parse_inline("un [enlace](sin cerrar")) == "un [enlace](sin cerrar"


def test_code_spans_do_not_interpret_markdown() -> None:
    (span,) = parse_inline("`**no es negrita**`")
    assert span.code and span.text == "**no es negrita**" and not span.bold


def test_links_keep_their_target_and_inner_style() -> None:
    spans = parse_inline("mira [la **guía**](https://ejemplo.com/a) ya")
    linked = [s for s in spans if s.link]
    assert [s.text for s in linked] == ["la ", "guía"]
    assert all(s.link == "https://ejemplo.com/a" for s in linked)
    assert linked[1].bold


def test_escapes() -> None:
    assert _text(parse_inline(r"\*no cursiva\* y \# no título")) == "*no cursiva* y # no título"


def test_html_is_never_interpreted() -> None:
    spans = parse_inline("<b>hola</b> <script>alert(1)</script>")
    assert _text(spans) == "<b>hola</b> <script>alert(1)</script>"
    assert not any(s.bold for s in spans)


# ------------------------------------------------------------------ bloques
def test_headings_and_paragraphs() -> None:
    blocks = parse("# Título\n\nUn párrafo\nque sigue.\n\n### Menor ###")
    assert isinstance(blocks[0], Heading) and blocks[0].level == 1
    assert isinstance(blocks[1], Paragraph) and _text(blocks[1].spans) == "Un párrafo que sigue."
    assert isinstance(blocks[2], Heading) and blocks[2].level == 3
    assert _text(blocks[2].spans) == "Menor"


def test_lists_ordered_nested_and_tasks() -> None:
    blocks = parse(
        "- uno\n  - anidado\n- dos\n\n1. primero\n2) segundo\n\n- [x] hecho\n- [ ] falta"
    )
    items = [b for b in blocks if isinstance(b, ListItem)]
    assert [(i.level, i.marker, _text(i.spans)) for i in items[:3]] == [
        (0, "•", "uno"),
        (1, "•", "anidado"),
        (0, "•", "dos"),
    ]
    assert [i.marker for i in items[3:5]] == ["1.", "2."]
    assert [(i.checked, _text(i.spans)) for i in items[5:]] == [(True, "hecho"), (False, "falta")]


def test_list_continuation_lines_join_the_item() -> None:
    (item,) = parse("- una línea\n  que continúa")
    assert _text(item.spans) == "una línea que continúa"


def test_code_block_with_language_and_unterminated_while_streaming() -> None:
    (block,) = parse("```python\nprint('hola')\nx = 1\n```")
    assert isinstance(block, Code) and block.lang == "python"
    assert block.text == "print('hola')\nx = 1"
    (partial,) = parse("```sql\nSELECT 1;\nSELECT 2")
    assert isinstance(partial, Code) and partial.text == "SELECT 1;\nSELECT 2"


def test_markdown_inside_code_blocks_is_left_alone() -> None:
    (block,) = parse("```\n# no título\n**no negrita**\n| a | b |\n```")
    assert isinstance(block, Code) and "# no título" in block.text


def test_quote_and_rule() -> None:
    blocks = parse("> una cita\n> larga\n\n---\n\ntexto")
    assert isinstance(blocks[0], Quote) and _text(blocks[0].spans) == "una cita larga"
    assert isinstance(blocks[1], Rule)
    assert isinstance(blocks[2], Paragraph)


def test_table_with_alignment_and_ragged_rows() -> None:
    md = "| Plan | Precio | Notas |\n|:--|--:|:-:|\n| Básico | 5 | **ok** |\n| Pro | 12 |\n"
    (table,) = parse(md)
    assert isinstance(table, Table)
    assert [_text(c) for c in table.header] == ["Plan", "Precio", "Notas"]
    assert table.aligns == ("left", "right", "center")
    assert [_text(c) for c in table.rows[0]] == ["Básico", "5", "ok"]
    assert table.rows[0][2][0].bold
    assert [_text(c) for c in table.rows[1]] == ["Pro", "12", ""]  # la fila corta se completa


def test_table_cells_can_hold_escaped_pipes() -> None:
    (table,) = parse("| a | b |\n|---|---|\n| x \\| y | z |")
    assert _text(table.rows[0][0]) == "x | y"


def test_a_pipe_in_normal_text_is_not_a_table() -> None:
    (block,) = parse("usa a | b para unir")
    assert isinstance(block, Paragraph)


def test_empty_or_odd_input_never_fails() -> None:
    assert parse("") == []
    assert parse(None) == []  # type: ignore[arg-type]
    assert parse("\n\n   \n") == []
    assert parse("|||\n|---|") is not None
    assert isinstance(parse("*" * 500)[0], (Paragraph, Rule))


# --------------------------------------------------- imágenes y documentos
def test_an_image_line_becomes_an_image_block() -> None:
    blocks = parse("Mira:\n\n![gráfico de ventas](C:\\datos\\ventas.png)\n\nlisto")
    assert blocks[1] == Image("gráfico de ventas", "C:\\datos\\ventas.png")
    assert isinstance(blocks[0], Paragraph) and isinstance(blocks[2], Paragraph)


def test_a_remote_image_is_still_an_image_block_media_decides_later() -> None:
    (block,) = parse("![logo](https://ejemplo.com/logo.png)")
    assert block == Image("logo", "https://ejemplo.com/logo.png")


@pytest.mark.parametrize(
    "target",
    [
        "C:\\docs\\informe.docx",
        "C:/docs/informe.docx",
        "file:///C:/docs/informe.docx",
        "~/informe.pdf",
    ],
)
def test_a_line_that_is_only_a_local_link_becomes_a_document(target: str) -> None:
    (block,) = parse(f"[Informe trimestral]({target})")
    assert block == Document("Informe trimestral", target)


def test_a_bullet_that_is_only_a_local_link_is_also_a_document() -> None:
    blocks = parse("- [A](C:\\a.pdf)\n- [B](C:\\b.pdf)")
    assert blocks == [Document("A", "C:\\a.pdf"), Document("B", "C:\\b.pdf")]


def test_a_web_link_alone_stays_a_paragraph_and_inline_links_stay_inline() -> None:
    (web,) = parse("[sitio](https://ejemplo.com)")
    assert isinstance(web, Paragraph) and web.spans[0].link == "https://ejemplo.com"
    (inline,) = parse("abre [el informe](C:\\a.pdf) cuando puedas")
    assert isinstance(inline, Paragraph)


@pytest.mark.parametrize(
    ("target", "local"),
    [
        ("C:\\x", True),
        ("c:/x", True),
        ("\\\\servidor\\x", True),
        ("file:///C:/x", True),
        ("~/x", True),
        ("./x", True),
        ("/x", True),
        ("https://x.com", False),
        ("http://x.com", False),
        ("mailto:a@b.c", False),
        ("ejemplo.com/x", False),
    ],
)
def test_local_target_detection(target: str, local: bool) -> None:
    assert is_local_target(target) is local


# ---------------------------------------------------------------- copiado
def test_plain_text_keeps_the_content_without_markup() -> None:
    text = plain_text(
        parse(
            "# T\n\nhola **mundo**\n\n- a\n- b\n\n| x | y |\n|---|---|\n| 1 | 2 |\n\n![f](C:\\f.png)"
        )
    )
    assert "T\nhola mundo\n• a\n• b\nx | y\n1 | 2" in text
    assert "[imagen: f]" in text and "**" not in text


# ---------------------------------------------- rutas de Windows con espacios
def test_windows_paths_with_spaces_are_documents() -> None:
    path = r"C:\Users\Ana Pérez\Documentos\Informe Q3.docx"
    assert parse(f"[Informe]({path})") == [Document("Informe", path)]


def test_angle_brackets_around_the_path_are_removed() -> None:
    path = r"C:\Users\Ana Pérez\Informe Q3.docx"
    assert parse(f"[Informe](<{path}>)") == [Document("Informe", path)]
    assert parse(f"![gráfico](<{path}>)") == [Image("gráfico", path)]


def test_images_with_spaces_in_the_path() -> None:
    path = r"C:\Users\Ana Pérez\ventas por región.png"
    assert parse(f"![x]({path})") == [Image("x", path)]


def test_a_title_after_a_web_target_is_ignored() -> None:
    (block,) = parse('![logo](https://ejemplo.com/a.png "El logo")')
    assert block == Image("logo", "https://ejemplo.com/a.png")


def test_inline_links_accept_angle_targets() -> None:
    spans = parse_inline(r"abre [el archivo](<C:\a b\x.pdf>) ya")
    assert [s.link for s in spans if s.link] == [r"C:\a b\x.pdf"]
