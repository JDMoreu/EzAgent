"""Campo de texto del chat: edición, selección, movimiento y pegado seguro."""

from __future__ import annotations

from ezagent.voice.chat.text_input import MAX_CHARS, TextInput, sanitize


def test_typing_and_cursor() -> None:
    field = TextInput()
    field.insert("hola")
    field.move("left")
    field.insert("X")
    assert field.text == "holXa" and field.cursor == 4


def test_backspace_and_delete() -> None:
    field = TextInput("abcd")
    field.move("left")
    field.backspace()
    assert field.text == "abd" and field.cursor == 2
    field.delete()
    assert field.text == "ab"
    field.delete()  # al final no hace nada
    TextInput().backspace()  # ni al principio


def test_word_movement_and_word_deletion() -> None:
    field = TextInput("resume el informe, por favor")
    field.move("left", word=True)
    assert field.text[field.cursor :] == "favor"
    field.move("left", word=True)
    assert field.text[field.cursor :] == "por favor"
    field.move("right", word=True)
    assert field.text[: field.cursor].endswith("por")
    field.backspace(word=True)
    assert field.text == "resume el informe,  favor"


def test_delete_word_forward() -> None:
    field = TextInput("uno dos tres")
    field.move("home")
    field.delete(word=True)
    assert field.text == " dos tres"


def test_selection_is_replaced_by_typing_and_deleted_by_backspace() -> None:
    field = TextInput("hola mundo")
    field.select_all()
    assert field.selected_text() == "hola mundo"
    field.insert("adiós")
    assert field.text == "adiós" and field.selection() is None
    field.select_all()
    field.backspace()
    assert field.text == "" and field.cursor == 0


def test_shift_movement_extends_the_selection() -> None:
    field = TextInput("abcdef")
    field.move("home")
    field.move("right", select=True)
    field.move("right", select=True)
    assert field.selected_text() == "ab"
    field.move("left", select=True)
    assert field.selected_text() == "a"
    field.move("right")  # sin Mayús, se deshace al borde derecho
    assert field.selection() is None and field.cursor == 1


def test_plain_arrow_collapses_the_selection_to_its_edge() -> None:
    field = TextInput("abcdef")
    field.move("home")
    for _ in range(3):
        field.move("right", select=True)
    field.move("left")
    assert field.cursor == 0 and field.selection() is None


def test_multiline_navigation() -> None:
    field = TextInput("primera\nsegunda línea\ntercera")
    assert field.line_col() == (2, 7)
    field.move("up")
    assert field.line_col() == (1, 7)
    field.move("home")
    assert field.line_col() == (1, 0)
    field.move("end")
    assert field.line_col() == (1, 13)
    field.move("up")
    assert field.line_col() == (0, 7)  # la columna se recorta a la línea corta
    field.move("up")
    assert field.cursor == 0
    field.move("down")
    field.move("down")
    assert field.line_col()[0] == 2
    field.move("down")
    assert field.cursor == len(field.text)


def test_enter_inserts_a_line_break() -> None:
    field = TextInput("a")
    field.newline()
    field.insert("b")
    assert field.text == "a\nb"


def test_take_returns_the_trimmed_text_and_empties_the_field() -> None:
    field = TextInput("  hola  \n")
    assert field.take() == "hola" and field.text == "" and field.empty
    assert TextInput("   \n  ").empty


def test_pasting_cleans_control_characters_and_line_endings() -> None:
    assert sanitize("a\r\nb\rc\td\x00e\x1b[31m") == "a\nb\nc    de[31m"
    field = TextInput()
    field.insert("uno\x00\x07 dos\r\ntres")
    assert field.text == "uno dos\ntres"


def test_unicode_survives() -> None:
    field = TextInput("añadir café ☕ y ñandú")
    assert field.text == "añadir café ☕ y ñandú"
    field.backspace()
    assert field.text.endswith("ñand")


def test_the_text_has_a_cap_even_when_pasting_a_lot() -> None:
    field = TextInput()
    field.insert("x" * (MAX_CHARS + 5000))
    assert len(field.text) == MAX_CHARS
    field.insert("más")
    assert len(field.text) == MAX_CHARS
    field.move("home")
    field.delete()
    field.insert("yz")  # al borrar, vuelve a haber hueco para uno
    assert len(field.text) == MAX_CHARS


def test_set_text_replaces_everything() -> None:
    field = TextInput("viejo")
    field.set_text("nuevo")
    assert field.text == "nuevo" and field.cursor == 5
