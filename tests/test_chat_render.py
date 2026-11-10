"""Dibujo del chat: texto, tablas, imágenes y documentos como imagen, con sus zonas pulsables."""

from __future__ import annotations

from pathlib import Path

import pytest
from PIL import Image

from ezagent.voice.chat import markdown as md
from ezagent.voice.chat import render_rich
from ezagent.voice.chat.render import render_message
from ezagent.voice.chat.render_base import Ctx, wrap
from ezagent.voice.notch.bar_theme import card_palette

WIDTH = 420.0


def _render(text: str, theme: str = "dark", scale: float = 1.0, roots=None, width: float = WIDTH):
    return render_message(text, card_palette(theme), width, scale, roots=roots)


def _kinds(rendered) -> list[str]:
    return [hit.kind for hit in rendered.hits]


@pytest.fixture
def files(tmp_path: Path) -> Path:
    Image.new("RGB", (800, 400), (30, 120, 130)).save(tmp_path / "ventas por región.png")
    (tmp_path / "Informe Q3.docx").write_bytes(b"x" * 5000)
    (tmp_path / "notas.md").write_text("# Notas\n\n- uno\n- dos", encoding="utf-8")
    (tmp_path / "datos.csv").write_text("a,b\n1,2\n3,4\n", encoding="utf-8")
    (tmp_path / "programa.exe").write_bytes(b"MZ")
    return tmp_path


# ----------------------------------------------------------------- texto
def test_the_image_matches_the_logical_size_and_scale() -> None:
    for scale in (1.0, 1.5, 2.0):
        rendered = _render("# Título\n\nUn párrafo.", scale=scale)
        assert rendered.image.width == round(WIDTH * scale)
        assert rendered.image.height == round(rendered.height * scale)
        assert rendered.image.getchannel("A").getbbox() is not None


def test_logical_height_does_not_depend_on_the_screen_scale() -> None:
    text = "Un párrafo bastante largo " * 12
    heights = [_render(text, scale=s).height for s in (1.0, 1.5, 2.0)]
    assert max(heights) - min(heights) <= 24.0  # el ajuste de línea puede variar un renglón


def test_both_themes_render_visible_text() -> None:
    for theme in ("dark", "light"):
        assert _render("**hola** mundo", theme).image.getchannel("A").getbbox() is not None


def test_empty_messages_render_without_failing() -> None:
    for text in ("", "   ", "\n\n"):
        rendered = _render(text)
        assert rendered.image.width > 0 and rendered.height >= 1.0 and rendered.hits == ()


def test_wrapped_lines_never_exceed_the_column() -> None:
    ctx = Ctx(scale=1.0, pal=card_palette("dark"), width=200.0)
    spans = md.parse_inline("palabras " * 40 + "`código largo` y **negrita** otra vez")
    for runs in wrap(ctx, spans, "body", 200.0):
        assert runs, "ninguna línea vacía"
        assert max(run.x + run.width for run in runs) <= 200.0 + 0.5
        assert not runs[-1].piece.space


def test_a_single_word_longer_than_the_column_does_not_hang() -> None:
    rendered = _render("a" * 400, width=160.0)
    assert rendered.height > 10


def test_long_code_lines_are_split_instead_of_overflowing() -> None:
    rendered = _render("```\n" + "x" * 300 + "\n```", width=240.0)
    assert rendered.image.width == 240
    assert rendered.height > 24 + 17.5 * 2  # más de una línea de código


def test_code_blocks_offer_a_copy_zone_with_the_code() -> None:
    rendered = _render("```python\nprint(1)\n```")
    (hit,) = rendered.hits
    assert hit.kind == "copy_code" and hit.data == "print(1)"
    x0, y0, x1, y1 = hit.rect
    assert 0 <= x0 < x1 <= WIDTH and 0 <= y0 < y1 <= rendered.height


def test_plain_text_is_available_for_copying_a_message() -> None:
    assert "hola mundo" in _render("**hola** mundo\n\n- a").plain


# ----------------------------------------------------------------- tablas
def test_a_table_never_overflows_even_with_many_columns() -> None:
    cols = 9
    head = "| " + " | ".join(f"Columna {i}" for i in range(cols)) + " |"
    sep = "|" + "---|" * cols
    row = "| " + " | ".join("dato largo de ejemplo" for _ in range(cols)) + " |"
    rendered = _render(f"{head}\n{sep}\n{row}\n{row}", width=360.0)
    assert rendered.image.width == 360
    ctx = Ctx(scale=1.0, pal=card_palette("dark"), width=360.0)
    (table,) = md.parse(f"{head}\n{sep}\n{row}")
    assert sum(render_rich._column_widths(ctx, table)) <= 360.0 + 1.0


def test_a_small_table_keeps_its_natural_width() -> None:
    ctx = Ctx(scale=1.0, pal=card_palette("dark"), width=WIDTH)
    (table,) = md.parse("| a | b |\n|---|---|\n| 1 | 2 |")
    assert sum(render_rich._column_widths(ctx, table)) < WIDTH / 2


def test_long_cell_text_wraps_and_makes_the_row_taller() -> None:
    short = _render("| a | b |\n|---|---|\n| x | y |", width=300.0)
    long = _render("| a | b |\n|---|---|\n| " + "texto " * 40 + " | y |", width=300.0)
    assert long.height > short.height + 20


# ---------------------------------------------------------------- imágenes
def test_a_local_image_is_shown_scaled_to_the_column_with_a_click_zone(files: Path) -> None:
    rendered = _render(f"![Ventas]({files / 'ventas por región.png'})", roots=(files,))
    (hit,) = rendered.hits
    assert hit.kind == "image" and hit.data.kind == "image"
    x0, y0, x1, y1 = hit.rect
    assert x1 - x0 <= WIDTH and (y1 - y0) <= render_rich.IMAGE_MAX_H
    assert (x1 - x0) / (y1 - y0) == pytest.approx(2.0, rel=0.02)  # conserva la proporción


def test_an_image_is_never_enlarged_beyond_its_size(files: Path) -> None:
    Image.new("RGB", (60, 30), (255, 0, 0)).save(files / "pequeña.png")
    (hit,) = _render(f"![x]({files / 'pequeña.png'})", roots=(files,)).hits
    assert hit.rect[2] - hit.rect[0] == pytest.approx(60, abs=1)


def test_remote_images_are_not_fetched_and_have_no_click_zone() -> None:
    rendered = _render("![logo](https://ejemplo.com/logo.png)")
    assert rendered.hits == () and rendered.height > 40


def test_images_outside_the_allowed_folders_are_not_shown(files: Path) -> None:
    other = files / "otra"
    other.mkdir()
    rendered = _render(f"![x]({files / 'ventas por región.png'})", roots=(other,))
    assert "image" not in _kinds(rendered)


def test_missing_and_broken_images_show_a_placeholder(files: Path) -> None:
    (files / "rota.png").write_bytes(b"no es una imagen")
    for target in (files / "no-existe.png", files / "rota.png"):
        rendered = _render(f"![x]({target})", roots=(files,))
        assert "image" not in _kinds(rendered) and rendered.height > 40


# --------------------------------------------------------------- documentos
def test_a_document_card_offers_open_and_reveal(files: Path) -> None:
    rendered = _render(f"[Informe]({files / 'Informe Q3.docx'})", roots=(files,))
    assert _kinds(rendered) == ["reveal", "open"]
    assert all(hit.data.name == "Informe Q3.docx" for hit in rendered.hits)
    for hit in rendered.hits:
        x0, y0, x1, y1 = hit.rect
        assert 0 <= x0 < x1 <= WIDTH and 0 <= y0 < y1 <= rendered.height


def test_paths_with_spaces_and_angle_brackets_are_documents(files: Path) -> None:
    plain = _render(f"[A]({files / 'Informe Q3.docx'})", roots=(files,))
    angle = _render(f"[A](<{files / 'Informe Q3.docx'}>)", roots=(files,))
    assert _kinds(plain) == _kinds(angle) == ["reveal", "open"]


def test_executables_can_be_revealed_but_never_opened(files: Path) -> None:
    rendered = _render(f"[Programa]({files / 'programa.exe'})", roots=(files,))
    assert _kinds(rendered) == ["reveal"]


def test_a_missing_document_has_no_buttons(files: Path) -> None:
    rendered = _render("[Perdido](C:/no/existe/nada.pdf)", roots=(files,))
    assert rendered.hits == ()


def test_documents_outside_the_allowed_folders_have_no_buttons_and_no_preview(files: Path) -> None:
    other = files / "otra"
    other.mkdir()
    inside = _render(f"[Notas]({files / 'notas.md'})", roots=(files,))
    outside = _render(f"[Notas]({files / 'notas.md'})", roots=(other,))
    assert outside.hits == () and outside.height < inside.height


def test_text_documents_show_a_preview_and_others_do_not(files: Path) -> None:
    text = _render(f"[Notas]({files / 'notas.md'})", roots=(files,))
    office = _render(f"[Informe]({files / 'Informe Q3.docx'})", roots=(files,))
    assert text.height > office.height + 40
    assert office.height == pytest.approx(render_rich.CARD_H)


def test_a_csv_preview_is_drawn_as_a_table(files: Path) -> None:
    csv = _render(f"[Datos]({files / 'datos.csv'})", roots=(files,))
    assert csv.height > render_rich.CARD_H + 60


def test_a_long_preview_is_capped(files: Path) -> None:
    (files / "largo.md").write_text(
        "\n\n".join(f"Párrafo número {i} con texto." for i in range(80)), encoding="utf-8"
    )
    rendered = _render(f"[Largo]({files / 'largo.md'})", roots=(files,))
    assert rendered.height <= render_rich.CARD_H + render_rich.PREVIEW_MAX + 20 + 12 + 1


def test_preview_content_cannot_create_clickable_zones(files: Path) -> None:
    other = files / "malo.md"
    other.write_text("```\ncódigo\n```\n\n![x](https://a.b/c.png)", encoding="utf-8")
    rendered = _render(f"[Malo]({other})", roots=(files,))
    assert "copy_code" not in _kinds(rendered)  # el contenido del archivo es dato, no interfaz


def test_a_message_mixing_everything_renders(files: Path) -> None:
    text = (
        "# Resumen\n\nTexto con **negrita**, *cursiva* y `código`.\n\n- a\n  - b\n\n"
        "| x | y |\n|---|---|\n| 1 | 2 |\n\n"
        f"![i]({files / 'ventas por región.png'})\n\n[d]({files / 'Informe Q3.docx'})\n\n"
        "```sql\nSELECT 1;\n```\n\n> cita\n\n---\n\nfin"
    )
    for theme in ("dark", "light"):
        rendered = _render(text, theme, 1.5, roots=(files,))
        assert rendered.height > 600
        assert {"copy_code", "image", "open", "reveal"} <= set(_kinds(rendered))
