"""Vista del chat: dibujo, acciones del ratón y del teclado, scroll, entrada y confirmaciones."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pytest

from ezagent.voice.chat import markdown as md
from ezagent.voice.chat.render_base import Ctx, wrap
from ezagent.voice.chat.session import ChatModel
from ezagent.voice.chat.store import ChatStore
from ezagent.voice.notch import chat_view as cv
from ezagent.voice.notch.bar_theme import card_palette
from ezagent.voice.notch.chat_view import Action, ChatView

W, H = 440.0, 600.0


def make(tmp_path: Path, theme: str = "dark", scale: float = 1.0, roots=None):
    model = ChatModel(ChatStore(tmp_path / "chat.jsonl"))
    view = ChatView(model, roots=roots)
    view.configure(W, H, scale, theme, card_palette(theme))
    return model, view


def kinds(view: ChatView) -> set[str]:
    return {kind for _rect, kind, _data in view._hits}


def center(view: ChatView, kind: str, data=None) -> tuple[float, float]:
    for (x0, y0, x1, y1), k, d in view._hits:
        if k == kind and (data is None or d == data):
            return (x0 + x1) / 2, (y0 + y1) / 2
    raise AssertionError(f"no hay zona «{kind}»: {sorted(kinds(view))}")


def click(view: ChatView, kind: str, data=None, now: float = 1.0):
    view.render(now)
    return view.pointer_down(*center(view, kind, data))


def reply(model: ChatModel, user: str, answer: str, source: str = "chat") -> None:
    model.add_user(user, source)  # type: ignore[arg-type]
    msg = model.begin_assistant(source)  # type: ignore[arg-type]
    model.append(msg.id, answer)
    model.finish(msg.id)


# ------------------------------------------------------------------ dibujo
@pytest.mark.parametrize("scale", [1.0, 1.5, 2.0])
@pytest.mark.parametrize("theme", ["dark", "light"])
def test_the_panel_renders_at_the_right_size(tmp_path: Path, scale: float, theme: str) -> None:
    model, view = make(tmp_path, theme, scale)
    reply(model, "hola", "¡Hola! ¿En qué te ayudo?")
    frame = view.render(1.0)
    assert frame.shape == (round(H * scale), round(W * scale), 4)
    assert frame[..., 3].max() > 0.9 and frame.dtype == np.float32


def test_the_empty_state_invites_to_write(tmp_path: Path) -> None:
    _, view = make(tmp_path)
    assert view.render(1.0)[..., 3].max() > 0
    assert {"close", "voice", "mic", "settings", "history", "new", "send", "input"} <= kinds(view)


def test_header_buttons_exist_in_every_state(tmp_path: Path) -> None:
    model, view = make(tmp_path)
    reply(model, "a", "b")
    view.render(1.0)
    assert {"close", "voice", "mic", "settings", "history", "new"} <= kinds(view)
    view.mode = "history"
    view.render(1.0)
    assert {"close", "voice", "mic", "settings", "history", "new", "back"} <= kinds(view)


def test_a_streaming_answer_shows_the_typing_dots_until_text_arrives(tmp_path: Path) -> None:
    model, view = make(tmp_path)
    model.add_user("hola")
    msg = model.begin_assistant()
    empty = view.render(1.0)
    model.append(msg.id, "Hola")
    filled = view.render(1.0 + 0.2)
    assert not np.array_equal(empty, filled)
    assert view.animating(1.0)  # mientras llega la respuesta, la ventana repinta


def test_captions_never_show_under_the_header(tmp_path: Path, monkeypatch) -> None:
    model, view = make(tmp_path)
    for i in range(6):
        reply(model, f"pregunta {i}", "respuesta " * 40)
    drawn: list[tuple[str, float]] = []
    original = view._text

    def spy(buf, text, style, color, x, y, align="left"):
        drawn.append((text, y))
        return original(buf, text, style, color, x, y, align)

    monkeypatch.setattr(view, "_text", spy)
    view.render(1.0)
    view.scroll = 130.0
    view.render(1.0)
    captions = [y for text, y in drawn if text.startswith("EZAGENT")]
    assert captions and min(captions) >= cv.HEADER_H - 1


# ---------------------------------------------------------------- regresión
def test_a_table_header_is_never_split_by_a_rounding_error() -> None:
    (table,) = md.parse(
        "| Región | Q2 | Q3 | Cambio |\n|:--|--:|--:|:-:|\n| Norte | 1 | 2 | **+18 %** |"
    )
    for scale in (1.0, 1.25, 1.5, 1.75, 2.0, 2.5):
        ctx = Ctx(scale, card_palette("dark"), 408.0)
        from ezagent.voice.chat import render_rich as rr

        widths = rr._column_widths(ctx, table)
        for i, cell in enumerate(table.header):
            lines = wrap(ctx, cell, "cellbold", widths[i] - 2 * rr.CELL_PAD_X)
            assert len(lines) == 1, (scale, i)


# ------------------------------------------------------------------ ratón
def test_close_and_new_and_voice_buttons(tmp_path: Path) -> None:
    model, view = make(tmp_path)
    assert click(view, "close") == Action("close")
    assert click(view, "new") == Action("new")
    assert click(view, "voice") == Action("voice", True)
    model.set_voice_replies(True)
    assert click(view, "voice") == Action("voice", False)


def test_the_mic_button_toggles_the_same_pause_as_the_bar(tmp_path: Path) -> None:
    model, view = make(tmp_path)
    assert click(view, "mic") == Action("mic", True)
    model.set_mic_muted(True)
    view.render(1.0)  # el icono tachado se dibuja sin romper
    assert click(view, "mic") == Action("mic", False)


def test_the_menu_button_opens_settings_from_the_chat(tmp_path: Path) -> None:
    _, view = make(tmp_path)
    assert click(view, "settings") == Action("settings")


def test_sending_from_the_button_takes_the_text(tmp_path: Path) -> None:
    _, view = make(tmp_path)
    view.paste("resume el informe")
    assert click(view, "send") == Action("send", "resume el informe")
    assert view.input.text == ""
    assert click(view, "send") is None  # vacío: no envía


def test_while_busy_the_button_stops_the_answer(tmp_path: Path) -> None:
    model, view = make(tmp_path)
    model.add_user("x")
    model.begin_assistant()
    assert click(view, "stop") == Action("stop")
    view.paste("otra cosa")
    assert click(view, "stop") == Action(
        "stop"
    )  # el botón sigue siendo «detener» mientras responde


def test_history_lists_conversations_and_opens_one(tmp_path: Path) -> None:
    model, view = make(tmp_path)
    reply(model, "presupuesto de marzo", "ok")
    old = model.store.session
    model.new_session()
    reply(model, "otra cosa", "vale")
    click(view, "history")
    assert view.mode == "history"
    view.render(1.0)
    assert {"session", "back"} <= kinds(view)
    assert click(view, "session", old) == Action("switch", old)  # se retoma, no solo se mira
    assert view.mode == "chat"
    click(view, "history")
    click(view, "back")
    assert view.mode == "chat" and model.viewing is None


def test_empty_history_says_so(tmp_path: Path) -> None:
    _, view = make(tmp_path)
    click(view, "history")
    assert view.render(1.0)[..., 3].max() > 0 and "session" not in kinds(view)


def test_hover_and_hand_cursor(tmp_path: Path) -> None:
    _, view = make(tmp_path)
    view.render(1.0)
    x, y = center(view, "close")
    assert view.pointer_move(x, y) and view.hover == "close"
    assert not view.pointer_move(x, y)  # sin cambio no repinta
    assert view.cursor_is_hand(x, y)
    assert view.pointer_move(1.0, 1.0) and view.hover is None


def test_clicking_the_input_places_the_caret(tmp_path: Path) -> None:
    _, view = make(tmp_path)
    view.paste("hola mundo")
    view.render(1.0)
    box = view._input_box
    view.pointer_down(box[0] + 16, box[1] + 20)
    assert view.input.cursor == 0
    view.render(1.0)
    view.pointer_down(box[2] - 60, box[1] + 20)
    assert view.input.cursor == len("hola mundo")


# ------------------------------------------------- mensajes: copiar y abrir
def test_assistant_messages_can_be_copied_as_plain_text(tmp_path: Path) -> None:
    model, view = make(tmp_path)
    reply(model, "hola", "Un **resumen** con `código`")
    assert click(view, "copy_msg", "Un resumen con código") == Action(
        "copy", "Un resumen con código"
    )


def test_code_blocks_documents_and_images_give_actions(tmp_path: Path) -> None:
    (tmp_path / "Informe Q3.docx").write_bytes(b"x" * 100)
    (tmp_path / "foto.png").write_bytes(b"")
    from PIL import Image

    Image.new("RGB", (40, 20), (200, 30, 30)).save(tmp_path / "foto.png")
    model, view = make(tmp_path, roots=(tmp_path,))
    reply(
        model,
        "dame cosas",
        f"```sql\nSELECT 1;\n```\n\n![f]({tmp_path / 'foto.png'})\n\n[Informe]({tmp_path / 'Informe Q3.docx'})",
    )
    assert click(view, "copy_code") == Action("copy", "SELECT 1;")
    act = click(view, "open")
    assert act and act.kind == "open" and act.data.name == "Informe Q3.docx"
    act = click(view, "reveal")
    assert act and act.kind == "reveal"
    act = click(view, "image")
    assert act and act.kind == "open" and act.data.name == "foto.png"


def test_user_bubbles_hug_the_right_edge(tmp_path: Path) -> None:
    model, view = make(tmp_path)
    model.add_user("hola")
    _, (msg,) = model.snapshot()
    entry = view._entry(msg, 1.0)
    assert entry.x + entry.arr.shape[1] / view.g.scale == pytest.approx(W - cv.PAD, abs=1.0)
    assert entry.arr.shape[1] / view.g.scale < W * 0.5  # una frase corta no ocupa todo el ancho


# ------------------------------------------------------------ confirmaciones
def test_a_pending_confirmation_shows_yes_no_buttons(tmp_path: Path) -> None:
    model, view = make(tmp_path)
    reply(model, "borra todo", "Necesito confirmación.")
    model.ask_confirm("Mover 12 archivos a la papelera", True)
    assert click(view, "confirm_yes") == Action("confirm", True)
    assert click(view, "confirm_no") == Action("confirm", False)
    model.clear_confirm()
    view.render(1.0)
    assert "confirm_yes" not in kinds(view)


# ------------------------------------------------------------------ teclado
def test_enter_sends_and_shift_enter_adds_a_line(tmp_path: Path) -> None:
    _, view = make(tmp_path)
    for ch in "hola":
        view.char(ch)
    assert view.key(13, shift=True) is None
    view.paste("segunda línea")
    assert view.input.text == "hola\nsegunda línea"
    assert view.key(13) == Action("send", "hola\nsegunda línea")
    assert view.key(13) is None  # vacío


def test_control_characters_are_not_typed(tmp_path: Path) -> None:
    _, view = make(tmp_path)
    for ch in ("\r", "\n", "\x08", "\x1b", "\x7f", ""):
        view.char(ch)
    assert view.input.text == ""
    view.char("ñ")
    assert view.input.text == "ñ"


def test_escape_clears_then_leaves_history_then_closes(tmp_path: Path) -> None:
    model, view = make(tmp_path)
    view.paste("borrador")
    assert view.key(27) is None and view.input.text == ""
    view.mode = "history"
    assert view.key(27) is None and view.mode == "chat"
    reply(model, "x", "y")
    old = model.store.session
    model.new_session()
    model.view_session(old)
    assert view.key(27) is None and model.viewing is None
    assert view.key(27) == Action("close")


def test_selection_copy_and_cut_go_through_actions(tmp_path: Path) -> None:
    _, view = make(tmp_path)
    view.paste("hola mundo")
    assert view.key(67, ctrl=True) is None  # sin selección no copia
    assert view.key(65, ctrl=True) is None
    assert view.key(67, ctrl=True) == Action("copy", "hola mundo")
    assert view.input.text == "hola mundo"
    assert view.key(88, ctrl=True) == Action("copy", "hola mundo")
    assert view.input.text == ""


def test_editing_keys(tmp_path: Path) -> None:
    _, view = make(tmp_path)
    view.paste("uno dos")
    view.key(37)
    view.key(8)
    assert view.input.text == "uno ds"  # el cursor estaba al final: borra la «o»
    view.key(36)
    view.key(46)
    assert view.input.text == "no ds"
    view.key(8, ctrl=True)  # al principio no hace nada
    view.key(35, shift=True)  # Mayús+Fin selecciona hasta el final
    assert view.input.selected_text() == "no ds"


def test_dropped_files_become_cards_when_sent(tmp_path: Path) -> None:
    doc = tmp_path / "Mi informe.docx"
    doc.write_bytes(b"x" * 500)
    model, view = make(tmp_path, roots=(tmp_path,))
    view.paste("revisa esto")
    view.add_files([str(doc), ""])
    action = view.key(13)
    assert action and action.kind == "send" and "Mi informe.docx" in action.data
    model.add_user(action.data)
    view.render(1.0)
    assert {"open", "reveal"} <= kinds(view)


# -------------------------------------------------------------------- scroll
def _long_chat(tmp_path: Path):
    model, view = make(tmp_path)
    for i in range(12):
        reply(model, f"pregunta {i}", f"respuesta número {i} " * 30)
    view.render(1.0)
    return model, view


def test_the_chat_opens_pinned_to_the_last_message_and_scroll_is_clamped(tmp_path: Path) -> None:
    _, view = _long_chat(tmp_path)
    assert view.scroll == 0.0 and view._max_scroll > 300
    view.wheel(120 * 1000)
    view.render(1.0)
    assert view.scroll == pytest.approx(view._max_scroll)
    view.wheel(-120 * 1000)
    assert view.scroll == 0.0


def test_new_messages_keep_following_the_end_unless_you_scrolled_up(tmp_path: Path) -> None:
    model, view = _long_chat(tmp_path)
    reply(model, "otra", "más texto " * 50)
    view.render(1.0)
    assert view.scroll == 0.0
    view.wheel(240)
    before = view.scroll
    reply(model, "otra más", "y más " * 50)
    view.render(1.0)
    assert view.scroll == before > 0  # no te arrastra si estás leyendo algo anterior


def test_page_keys_scroll_the_messages(tmp_path: Path) -> None:
    _, view = _long_chat(tmp_path)
    view.key(33)
    assert view.scroll > 0
    view.key(34)
    assert view.scroll == 0.0


def test_the_wheel_in_history_clamps_too(tmp_path: Path) -> None:
    model, view = make(tmp_path)
    for i in range(14):
        reply(model, f"tema {i}", "ok")
        model.new_session()
    view.mode = "history"
    view.render(1.0)
    view.wheel(120 * 1000)
    view.render(1.0)
    assert 0 < view.scroll <= view._max_scroll


# ----------------------------------------------------------- entrada y caché
def test_the_input_grows_with_its_lines_up_to_a_limit(tmp_path: Path) -> None:
    _, view = make(tmp_path)
    one = view._footer_height()
    view.paste("a\nb\nc")
    three = view._footer_height()
    view.paste("\n".join("x" for _ in range(30)))
    many = view._footer_height()
    assert one < three < many
    assert many - one <= cv.INPUT_LINE * (cv.INPUT_MAX_LINES - 1) + 1


def test_long_text_wraps_instead_of_overflowing_the_box(tmp_path: Path) -> None:
    _, view = make(tmp_path)
    view.paste("palabra " * 60)
    lines = view._wrap_input(W - 2 * cv.PAD - 12 - cv.BTN - 14)
    assert len(lines) > 3
    assert all(b >= a for a, b in lines)
    assert lines[0][0] == 0 and lines[-1][1] == len(view.input.text)


def test_break_handles_very_long_words_and_blank_lines() -> None:
    ctx = Ctx(1.0, card_palette("dark"), 200.0)
    pieces = cv._break(ctx, "a" * 120, 100.0)
    assert len(pieces) > 1 and pieces[0][0] == 0 and pieces[-1][1] == 120
    assert cv._break(ctx, "", 100.0) == [(0, 0)]


def test_streaming_entries_are_not_rebuilt_on_every_chunk(tmp_path: Path, monkeypatch) -> None:
    model, view = make(tmp_path)
    model.add_user("hola")
    msg = model.begin_assistant()
    built = []
    original = view._build_entry
    monkeypatch.setattr(view, "_build_entry", lambda m: (built.append(len(m.text)), original(m))[1])
    now = 10.0
    for _ in range(30):
        model.append(msg.id, "palabra ")
        view.render(now)
        now += 0.01  # 30 trozos en 300 ms
    assert 1 <= len([b for b in built if b]) <= 6
    model.finish(msg.id)
    view.render(now)
    assert built[-1] == len("palabra " * 30) - 1  # al terminar se dibuja el texto final completo


def test_changing_size_or_theme_rebuilds_the_messages(tmp_path: Path) -> None:
    model, view = make(tmp_path)
    reply(model, "a", "b")
    view.render(1.0)
    assert view._cache
    view.configure(W + 40, H, 1.0, "dark", card_palette("dark"))
    assert not view._cache
    view.render(1.0)
    view.configure(W + 40, H, 1.0, "dark", card_palette("dark"))
    assert view._cache  # sin cambios no se tira la caché


# ------------------------------------------------- copiar lo escrito y las notas
def test_what_the_user_typed_can_be_copied_exactly_as_typed(tmp_path: Path) -> None:
    model, view = make(tmp_path)
    typed = "Compara **A** con `B`\ny también C"
    model.add_user(typed)
    assert click(view, "copy_msg", typed) == Action("copy", typed)  # sin interpretar el markdown


def record_drawing(view, monkeypatch) -> tuple[list[str], list[str]]:
    """Qué iconos y qué textos dibuja la vista (para comprobar el botón de copiar)."""
    icons: list[str] = []
    texts: list[str] = []
    real_icon, real_text = cv.draw_icon, view._text
    monkeypatch.setattr(
        cv,
        "draw_icon",
        lambda buf, name, *a, **k: (icons.append(name), real_icon(buf, name, *a, **k))[1],
    )
    monkeypatch.setattr(
        view,
        "_text",
        lambda buf, text, *a, **k: (texts.append(text), real_text(buf, text, *a, **k))[1],
    )
    return icons, texts


def test_every_message_has_a_copy_icon_not_a_text_label(tmp_path: Path, monkeypatch) -> None:
    model, view = make(tmp_path)
    reply(model, "hola", "respuesta")
    icons, texts = record_drawing(view, monkeypatch)
    view.render(1.0)
    assert icons.count("copy") == 2  # el del usuario y el del asistente
    assert "COPIAR" not in texts and "COPIADO" not in texts


def test_the_copy_icon_turns_into_a_check_for_a_moment(tmp_path: Path, monkeypatch) -> None:
    model, view = make(tmp_path)
    reply(model, "hola", "respuesta")
    icons, _ = record_drawing(view, monkeypatch)
    view.flash_copied("hola", now=10.0)
    view.render(10.5)
    assert icons.count("check") == 1 and icons.count("copy") == 1  # solo el copiado cambia
    icons.clear()
    view.render(12.0)  # pasó el aviso
    assert "check" not in icons and icons.count("copy") == 2
    assert view.animating(10.5) and not view.animating(12.0)


def test_voice_messages_say_so_next_to_the_copy_icon(tmp_path: Path, monkeypatch) -> None:
    model, view = make(tmp_path)
    model.add_user("dicho en voz alta", "voice")
    icons, texts = record_drawing(view, monkeypatch)
    view.render(1.0)
    assert "POR VOZ" in texts and "copy" in icons


def test_the_copy_zone_is_just_the_icon(tmp_path: Path) -> None:
    model, view = make(tmp_path)
    reply(model, "hola", "respuesta")
    view.render(1.0)
    zones = [r for r, k, _ in view._hits if k == "copy_msg"]
    assert len(zones) == 2 and all(r[2] - r[0] <= 32 for r in zones)


def test_context_notes_are_drawn_centred_without_copy_or_caption(tmp_path: Path) -> None:
    model, view = make(tmp_path)
    reply(model, "hola", "respuesta")
    model.add_note("Conversación compactada: lo anterior queda resumido.")
    _, messages = model.snapshot()
    note = view._entry(messages[-1], 1.0)
    assert note.caption_hit is None and note.arr[..., 3].max() > 0
    assert note.height < 40
    view.render(1.0)
    assert [d for _r, k, d in view._hits if k == "copy_msg"] == ["hola", "respuesta"]  # la nota no


def test_a_note_that_wraps_stays_inside_the_column(tmp_path: Path) -> None:
    model, view = make(tmp_path)
    model.add_note("Conversación compactada. " * 12)
    _, (msg,) = model.snapshot()
    entry = view._entry(msg, 1.0)
    assert entry.height > 30
    assert entry.x + entry.arr.shape[1] / view.g.scale <= W
