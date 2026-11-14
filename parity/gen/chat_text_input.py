"""Fixtures de `ezagent.voice.chat.text_input` (F1-02).

El campo de texto se ejercita como una máquina de estados: se parte de un texto y
se aplican operaciones, registrando el estado después de cada una.

* ``sanitize``  entrada ``{"text": str}`` -> ``{"text": str}``
* resto de métodos (``insert``, ``backspace``, ``delete``, ``move``, ``take``,
  ``set_text``, ``clear``, ``newline``, ``select_all``, ``selection``,
  ``selected_text``, ``line_col``, ``empty``): entrada ``{"text": str,
  "ops": [...]}`` -> ``{"steps": [...], "final": {...}}``; cada paso guarda
  ``text/cursor/anchor/selection/selected/line/col/empty`` y el valor devuelto.

Operaciones: ``insert`` (text), ``newline``, ``backspace``/``delete`` (word),
``clear``, ``take``, ``set_text`` (text), ``select_all``, ``move``
(key/select/word), ``selection``, ``selected_text``, ``line_col``, ``empty``.
"""

from __future__ import annotations

from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "chat_text_input"


def _move(key: str, select: bool = False, word: bool = False) -> dict[str, Any]:
    return {"op": "move", "key": key, "select": select, "word": word}


def _script(text: str, ops: list[dict[str, Any]]) -> dict[str, Any]:
    return {"text": text, "ops": ops}


def _snapshot(field: Any) -> dict[str, Any]:
    selection = field.selection()
    line, col = field.line_col()
    return {
        "text": field.text,
        "cursor": field.cursor,
        "anchor": field.anchor,
        "selection": None if selection is None else list(selection),
        "selected": field.selected_text(),
        "line": line,
        "col": col,
        "empty": field.empty,
    }


def _apply(field: Any, op: dict[str, Any]) -> Any:
    name = op["op"]
    if name == "insert":
        return field.insert(op["text"])
    if name == "newline":
        return field.newline()
    if name == "backspace":
        return field.backspace(op.get("word", False))
    if name == "delete":
        return field.delete(op.get("word", False))
    if name == "clear":
        return field.clear()
    if name == "take":
        return field.take()
    if name == "set_text":
        return field.set_text(op["text"])
    if name == "select_all":
        return field.select_all()
    if name == "move":
        return field.move(op["key"], op.get("select", False), op.get("word", False))
    if name == "selection":
        span = field.selection()
        return None if span is None else list(span)
    if name == "selected_text":
        return field.selected_text()
    if name == "line_col":
        return list(field.line_col())
    if name == "empty":
        return field.empty
    raise ValueError(f"operación desconocida: {name}")


def _run(script: dict[str, Any]) -> dict[str, Any]:
    from ezagent.voice.chat.text_input import TextInput

    field = TextInput(script["text"]) if script["text"] else TextInput()
    steps: list[dict[str, Any]] = []
    for op in script["ops"]:
        result = _apply(field, op)
        step: dict[str, Any] = {"op": op["op"], **_snapshot(field), "result": result}
        steps.append(step)
    return {"steps": steps, "final": _snapshot(field)}


SANITIZE_CASES = [
    "",
    "texto normal",
    "a\r\nb\rc\td",
    "a\x00b",
    "\x00\x01\x02\x03\x04\x05\x06\x07\x08",
    "a\x0bb\x0cc",
    "\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b",
    "fin\x7f",
    "conserva\x80\x9f",
    "\t\t",
    "\r\n\r\n",
    "\n",
    "  con espacios  ",
    "ñandú ☕ 🙂",
    "emoji 🙂🚀 y 中文",
    "tab\tentre\tpalabras",
    "uno\rdos\r\ntres",
    "\x1b[31mrojo\x1b[0m",
    "linea\n",
    "x" * 300,
    "\u2028 separador de línea",
    "comillas \" y ' y \\",
    "a\x0b\x0c\x7f b",
    None,
]  # fmt: skip


def _insert_cases() -> list[dict[str, Any]]:
    rows = [
        ("", [], "hola"),
        ("hola", [_move("left")], "X"),
        ("hola", [_move("home")], "X"),
        ("abc", [_move("home"), _move("right", select=True), _move("right", select=True)], "Z"),
        ("hola mundo", [{"op": "select_all"}], "adiós"),
        ("", [], "a\r\nb\rc\td\x00e\x1b[31m"),
        ("", [], "  espacios  "),
        ("", [], "\n"),
        ("", [], "ñandú ☕ 🙂"),
        ("", [], ""),
        ("", [], "\x00\x07"),
        ("abc", [_move("end")], ""),
        ("", [], "x" * 100),
        ("hola", [_move("home")], "\t"),
        ("a", [], "b"),
        ("a\nb", [_move("up")], "X"),
        ("uno", [_move("left", word=True)], " dos"),
        ("texto", [{"op": "take"}], "nuevo"),
        ("viejo", [{"op": "set_text", "text": "base"}], "!"),
        ("", [], "línea1\nlínea2"),
        ("abcdef", [_move("home"), _move("right"), _move("right")], "XY"),
        ("abc", [_move("home"), _move("right", select=True)], "Q"),
        ("", [], "0123456789" * 10),
        ("abc", [_move("home"), _move("left"), _move("left")], "Z"),
        ("", [], "x" * 20_001),
    ]
    return [_script(text, [*ops, {"op": "insert", "text": value}]) for text, ops, value in rows]


def _newline_cases() -> list[dict[str, Any]]:
    rows = [
        ("", []),
        ("a", []),
        ("a", [_move("home")]),
        ("a", [_move("left")]),
        ("abc", [_move("home"), _move("right")]),
        ("a\nb", [_move("up")]),
        ("a\nb", [_move("home")]),
        ("a\nb", [_move("end")]),
        ("uno dos", [_move("left", word=True)]),
        ("", [{"op": "select_all"}]),
        ("hola mundo", [{"op": "select_all"}]),
        ("hola mundo", [{"op": "select_all"}, _move("left")]),
        ("ñandú", [_move("home")]),
        ("🙂🙂", [_move("left")]),
        ("  ", []),
        ("\n", []),
        ("a\n\nb", [_move("up")]),
        ("uno", [{"op": "take"}]),
        ("línea", [_move("end")]),
        ("tab\tsep", [_move("left", word=True)]),
    ]
    return [_script(text, [*ops, {"op": "newline"}]) for text, ops in rows]


def _backspace_cases() -> list[dict[str, Any]]:
    rows = [
        ("", [], False),
        ("abc", [], False),
        ("abc", [_move("home")], False),
        ("abc", [_move("left")], False),
        ("abc", [_move("left")], True),
        ("resume el informe, por favor", [], True),
        ("resume el informe, por favor", [], False),
        ("uno dos  tres", [], True),
        ("uno dos  tres", [_move("left", word=True)], True),
        ("ñandú ☕", [], True),
        ("ñandú ☕", [], False),
        ("🙂🙂", [], False),
        ("e\u0301", [], False),
        ("a\nb", [], False),
        ("a\nb", [_move("home")], False),
        ("hola mundo", [{"op": "select_all"}], False),
        ("hola mundo", [{"op": "select_all"}], True),
        ("hola mundo", [_move("home"), _move("right", select=True)], True),
        ("   ", [], True),
        ("123 456", [], True),
        ("snake_case_name", [], True),
        ("x" * 40, [_move("left", word=True)], True),
    ]
    return [_script(text, [*ops, {"op": "backspace", "word": word}]) for text, ops, word in rows]


def _delete_cases() -> list[dict[str, Any]]:
    rows = [
        ("", [], False),
        ("abc", [_move("home")], False),
        ("abc", [], False),
        ("uno dos tres", [_move("home")], True),
        ("uno dos tres", [], True),
        ("uno dos tres", [_move("left", word=True)], True),
        ("uno dos tres", [_move("left")], False),
        ("ñandú ☕", [_move("home")], True),
        ("🙂🙂", [_move("home")], False),
        ("e\u0301x", [_move("home")], False),
        ("a\nb", [_move("home")], False),
        ("a\nb", [_move("up")], False),
        ("hola mundo", [{"op": "select_all"}], False),
        ("hola mundo", [{"op": "select_all"}], True),
        ("hola mundo", [_move("home"), _move("right", select=True)], True),
        ("   ", [_move("home")], True),
        ("123 456", [_move("home")], True),
        ("snake_case_name", [_move("home")], True),
        ("x" * 40, [_move("home"), _move("right", word=True)], True),
        ("", [_move("home")], True),
        ("a", [_move("home")], False),
        ("dos\nlíneas", [_move("up")], False),
    ]
    return [_script(text, [*ops, {"op": "delete", "word": word}]) for text, ops, word in rows]


def _move_cases() -> list[dict[str, Any]]:
    texts = ["abcdef", "primera\nsegunda línea\ntercera", "ñandú ☕", "uno dos  tres", ""]
    rows: list[dict[str, Any]] = []
    for key in ("left", "right", "up", "down", "home", "end"):
        for select, word in ((False, False), (True, False), (False, True), (True, True)):
            text = texts[len(rows) % len(texts)]
            prep = [_move("end")] if len(rows) % 2 else []
            rows.append((text, [*prep, _move(key, select, word)]))
    return [_script(text, ops) for text, ops in rows]


def _simple_cases(op: str, texts: list[str]) -> list[dict[str, Any]]:
    return [_script(text, [{"op": op}]) for text in texts]


SIMPLE_TEXTS = [
    "",
    "   ",
    "hola",
    "  hola  ",
    "hola\n",
    "  \n hola \n ",
    "una línea",
    "a\nb\nc",
    "ñandú ☕",
    "🙂",
    "con\ttabulador",
    "a\r\nb",
    "x" * 50,
    "0",
    "\n",
    "linea uno\nlinea dos\n",
    "muchos   espacios",
    "fin.",
    "TODO: pendiente",
    "  dos  ",
]

LINE_COL_CASES = [
    ("", []),
    ("", [_move("end")]),
    ("abc", []),
    ("abc", [_move("home")]),
    ("abc", [_move("left")]),
    ("a\nb", []),
    ("a\nb", [_move("home")]),
    ("a\nb", [_move("up")]),
    ("primera\nsegunda línea\ntercera", []),
    ("primera\nsegunda línea\ntercera", [_move("up")]),
    ("primera\nsegunda línea\ntercera", [_move("up"), _move("up")]),
    ("primera\nsegunda línea\ntercera", [_move("home")]),
    ("a\n\nb", [_move("up")]),
    ("a\n\nb", [_move("up"), _move("up")]),
    ("uno\ndos", [_move("down")]),
    ("uno\ndos", [_move("down"), _move("down")]),
    ("ñandú\n☕", [_move("home")]),
    ("🙂🙂\n🙂", [_move("up")]),
    ("\n", [_move("home")]),
    ("a\nb\nc", [_move("up"), _move("up")]),
    ("larga línea\nd\n", [_move("up")]),
]


def _set_text_cases() -> list[dict[str, Any]]:
    rows = [
        ("viejo", "nuevo"),
        ("", ""),
        ("algo", ""),
        ("", "texto"),
        ("hola mundo", "adiós"),
        ("a\nb", "c\nd"),
        ("texto", "con\ttabulador"),
        ("texto", "con\x00control"),
        ("texto", "ñandú ☕"),
        ("texto", "🙂"),
        ("x", "y" * 100),
        ("línea", "otra línea"),
        ("a", "\n"),
        ("con espacios", "  bordes  "),
        ("123", "456"),
        ("con\nsalto", "sin salto"),
        ("", " "),
        ("abc", "abc"),
        ("fin", "fin."),
        ("con espacios", "  "),
    ]
    return [
        _script(old, [{"op": "set_text", "text": new}, {"op": "insert", "text": "!"}])
        for old, new in rows
    ]


QUERY_SELECTION_CASES = [
    ("abcdef", [_move("home"), _move("right", select=True), _move("right", select=True)]),
    ("abcdef", [_move("home"), _move("right", select=True)]),
    ("abcdef", [_move("home"), _move("end", select=True)]),
    ("abcdef", [_move("end"), _move("left", select=True)]),
    ("abcdef", [{"op": "select_all"}]),
    ("abcdef", [{"op": "select_all"}, _move("left", select=True)]),
    ("abcdef", [_move("home"), _move("right", select=True), _move("right")]),
    ("abcdef", [_move("home"), _move("right", select=True), _move("left")]),
    ("a\nb", [_move("home"), _move("down", select=True)]),
    ("a\nb", [_move("end"), _move("up", select=True)]),
    ("ñandú ☕", [_move("home"), _move("right", select=True, word=True)]),
    ("uno dos", [_move("home"), _move("right", select=True, word=True)]),
    ("uno dos", [_move("end"), _move("left", select=True, word=True)]),
    ("", [{"op": "select_all"}]),
    ("", [_move("home", select=True)]),
    ("abc", [{"op": "select_all"}, {"op": "insert", "text": "z"}]),
    ("abc", [{"op": "select_all"}, {"op": "backspace"}]),
    ("abc", [{"op": "select_all"}, {"op": "delete"}]),
    ("abc", [{"op": "select_all"}, {"op": "take"}]),
    ("ñandú", [_move("home"), _move("end", select=True)]),
]


def generate(out: FixtureFile) -> None:
    from ezagent.voice.chat.text_input import sanitize

    for index, text in enumerate(SANITIZE_CASES, start=1):
        out.add(
            f"chat_text_input/sanitize/{index:03d}",
            "sanitize",
            {"text": text},
            {"text": sanitize(text)},
        )

    groups: list[tuple[str, list[dict[str, Any]]]] = [
        ("insert", _insert_cases()),
        ("newline", _newline_cases()),
        ("backspace", _backspace_cases()),
        ("delete", _delete_cases()),
        ("move", _move_cases()),
        ("take", _simple_cases("take", SIMPLE_TEXTS)),
        ("clear", _simple_cases("clear", SIMPLE_TEXTS)),
        ("select_all", _simple_cases("select_all", SIMPLE_TEXTS)),
        ("set_text", _set_text_cases()),
        (
            "selection",
            [_script(text, [*ops, {"op": "selection"}]) for text, ops in QUERY_SELECTION_CASES],
        ),
        (
            "selected_text",
            [_script(text, [*ops, {"op": "selected_text"}]) for text, ops in QUERY_SELECTION_CASES],
        ),
        ("line_col", [_script(text, [*ops, {"op": "line_col"}]) for text, ops in LINE_COL_CASES]),
        (
            "empty",
            [_script(text, [*ops, {"op": "empty"}]) for text, ops in LINE_COL_CASES],
        ),
    ]
    for fn_name, scripts in groups:
        for index, script in enumerate(scripts, start=1):
            out.add(
                f"chat_text_input/{fn_name}/{index:03d}",
                fn_name,
                script,
                _run(script),
            )


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
