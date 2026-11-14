"""Fixtures de `ezagent.voice.tool_json` (F1-02).

Fija el JSON que se guarda en el historial: cierre de JSON cortado, reparación de
argumentos, saneado de llamadas y estructura de mensajes para OpenAI/Anthropic.

Formato de cada caso (el `fn` dice qué función se ejercita):

* ``close_json``         entrada ``{"raw": str}`` -> ``{"text": str}``
* ``repair_arguments``   entrada ``{"raw": str | None}`` -> ``{"json": str, "complete": bool}``
* ``sanitize_history``   entrada ``{"messages": [...]}`` -> ``{"fixed": int, "messages": [...]}``
  (los mensajes son los de después de la llamada: la función muta la lista)
* ``is_tool_json_error`` entrada ``{"message": str}`` -> ``{"result": bool}``
* ``repair_structure``   entrada ``{"messages": [...]}`` -> ``{"changes": int, "messages": [...]}``
* ``normalize_roles``    entrada ``{"messages": [...], "strict": bool}`` -> ``{"messages": [...]}``
"""

from __future__ import annotations

import copy
from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "tool_json"

CLOSE_JSON_RAW: list[Any] = [
    '{"a": 1}',
    "{}",
    "[]",
    '{"a": [1, 2, 3]}',
    '{"a": {"b": {"c": 1}}}',
    '{"a": [1, 2',
    '{"a": {"b": "x',
    '{"a": "texto',
    '{"a": "texto\\',
    '{"a": "q\\"',
    '{"a":',
    '{"a": 1,',
    '{"a": 1, "b"',
    '{"a": 1, "b": "x',
    "{",
    "[",
    '{"a": "}"}',
    '{"clave": ["a,", "b"]',
    '{"a": "ñandú ☕',
    '{"ruta": "C:\\\\datos\\\\x',
    '{"salto": "línea\\n',
    "no es json",
    "}{",
    "null",
    "123",
    "",
    "   \n\t ",
    '{"a": 1}   ',
    '{"anidado": [{"x": [1, {"y": "z',
]  # fmt: skip

REPAIR_ARGUMENTS_RAW: list[Any] = [
    "",
    "   ",
    "\n\t",
    None,
    "{}",
    '{"a": 1}',
    '  {"a": 1}  ',
    '{"action":"run","agent":"claude","prompt":"Revisa el repositorio y resume el estado de',
    '{"a": [1, 2',
    '{"a": {"b": "x',
    '{"a": "esc\\',
    '{"a": "q\\"',
    '{"a": 1, "b"',
    '{"a": 1,',
    '{"a":',
    '{"prompt": "ñandú ☕ y más',
    '{"n": 3.5, "ok": true, "nulo": null}',
    '{"lista": [1, 2, 3], "dicc": {"k": "v"}}',
    '{"emoji": "🙂🚀"}',
    "no es json",
    "[1, 2",
    "[1, 2]",
    "null",
    "123",
    '"texto"',
    "}{",
    '{"a": tru',
    '{"a": 1e',
    '{"a": 1, }',
    '{"a": 1}{"b": 2}',
    '{"texto": "linea1\\nlinea2',
]  # fmt: skip

TOOL_JSON_ERRORS: list[str] = [
    "Error code: 500 - Failed to parse tool call arguments as JSON: "
    "[json.exception.parse_error.101] parse error at line 1, column 87",
    "tool call arguments must be json",
    "Tool Call parse error",
    "TOOL CALL JSON",
    "error al parsear tool call arguments",
    "Error code: 500 - Internal error",
    "Error code: 429 - rate limit",
    "Failed to parse arguments as JSON",
    "parse error",
    "tool call",
    "",
    "  Tool Call   JSON  ",
    "tool calls: invalid json payload",
    "TOOL_CALL parse",
    "Error code: 400 - json decode error in tool call",
    "herramienta: tool call con json roto",
    "no tool call here",
    "json",
    "parse",
    "tool",
]  # fmt: skip


def _call_json(fn: Any, raw: Any) -> dict[str, Any]:
    fixed, complete = fn(raw)
    return {"json": fixed, "complete": complete}


def _history_cases() -> list[list[dict[str, Any]]]:
    valid = {"id": "1", "type": "function", "function": {"name": "a", "arguments": '{"x": 1}'}}
    broken = {"id": "2", "type": "function", "function": {"name": "b", "arguments": '{"x": "cort'}}
    empty = {"id": "3", "type": "function", "function": {"name": "c", "arguments": ""}}
    unicode_call = {
        "id": "4",
        "type": "function",
        "function": {"name": "d", "arguments": '{"texto": "ñandú ☕'},
    }

    def msg(role: str, content: Any = None, **extra: Any) -> dict[str, Any]:
        item: dict[str, Any] = {"role": role}
        if content is not None:
            item["content"] = content
        item.update(extra)
        return item

    return [
        [],
        [msg("user", "hola")],
        [msg("system", "s"), msg("user", "u")],
        [msg("assistant", "", tool_calls=[])],
        [msg("assistant", "", tool_calls=[valid])],
        [msg("assistant", "", tool_calls=[broken])],
        [msg("assistant", "", tool_calls=[empty])],
        [msg("assistant", "", tool_calls=[unicode_call])],
        [msg("assistant", "", tool_calls=[valid, broken, empty])],
        [msg("assistant", "voy", tool_calls=[valid, broken, empty, unicode_call])],
        [msg("assistant", "voy", tool_calls=[{"function": None}])],
        [msg("assistant", "voy", tool_calls=[{"function": "x"}])],
        [msg("assistant", "voy", tool_calls=[{"function": {"name": "z", "arguments": None}}])],
        [msg("assistant", "voy", tool_calls=[{"function": {"name": "z", "arguments": 7}}])],
        [msg("assistant", "voy", tool_calls=[{"function": {"arguments": "  "}}])],
        [msg("assistant", "voy", tool_calls=[{"function": {"arguments": "no es json"}}])],
        [msg("tool", "{}", tool_call_id="1")],
        [msg("assistant", "voy", tool_calls=[valid]), msg("tool", "{}", tool_call_id="1")],
        [
            msg("assistant", "", tool_calls=[valid]),
            msg("tool", "{}", tool_call_id="9"),
            msg("user", "sigue"),
        ],
        [msg("assistant", "**negrita** ok", tool_calls=[broken]), msg("user", "ñandú ☕")],
        [msg("assistant", "", tool_calls=[{"type": "function"}])],
    ]


def _structure_cases() -> list[list[dict[str, Any]]]:
    def msg(role: str, content: Any = None, **extra: Any) -> dict[str, Any]:
        item: dict[str, Any] = {"role": role}
        if content is not None:
            item["content"] = content
        item.update(extra)
        return item

    def call(call_id: Any, name: str = "t", arguments: str = "{}") -> dict[str, Any]:
        item: dict[str, Any] = {
            "type": "function",
            "function": {"name": name, "arguments": arguments},
        }
        if call_id is not None:
            item["id"] = call_id
        return item

    return [
        [],
        [msg("user", "hola")],
        [msg("system", "s"), msg("user", "u")],
        [msg("user", "uno"), msg("user", "dos")],
        [msg("user", "uno"), msg("user", "dos"), msg("user", "tres")],
        [msg("user", "uno"), msg("assistant", "respuesta"), msg("user", "dos")],
        [msg("assistant", "")],
        [msg("assistant", "   ")],
        [msg("assistant", None)],
        [msg("assistant", [])],
        [msg("assistant", [{"type": "text", "text": "hola"}])],
        [msg("assistant", "", tool_calls=[call(None)])],
        [msg("assistant", "", tool_calls=[call("call_1")])],
        [msg("assistant", "", tool_calls=[call("x"), call(None)])],
        [msg("assistant", "", tool_calls=[call("call_1"), call(None)])],
        [msg("assistant", "", tool_calls=[call("a"), call("b")])],
        [
            msg("assistant", "", tool_calls=[call("a")]),
            msg("tool", '{"ok": true}', tool_call_id="a"),
        ],
        [
            msg("assistant", "", tool_calls=[call("a"), call("b")]),
            msg("tool", '{"r": 1}', tool_call_id="a"),
        ],
        [
            msg("assistant", "", tool_calls=[call("a"), call("b")]),
            msg("tool", "r2", tool_call_id="b"),
            msg("tool", "r1", tool_call_id="a"),
        ],
        [
            msg("assistant", "", tool_calls=[call("a")]),
            msg("tool", "r1", tool_call_id="a"),
            msg("tool", "r2", tool_call_id="a"),
        ],
        [
            msg("user", "u"),
            msg("tool", "huérfano", tool_call_id="zz"),
            msg("assistant", "a"),
        ],
        [
            msg("system", "s"),
            msg("user", "uno"),
            msg("user", "dos"),
            msg("assistant", ""),
            msg("assistant", "", tool_calls=[call("c", "ñandú", '{"x": "☕"}')]),
            msg("tool", "resultado", tool_call_id="c"),
        ],
        [
            msg("assistant", "", tool_calls=[call("a"), "basura", call(None, "w")]),
            msg("tool", "r", tool_call_id="a"),
        ],
        [
            msg("assistant", "ok", tool_calls=[call("a", arguments='{"n": 1}')]),
            msg("user", "sigue"),
        ],
    ]


def _role_cases() -> list[list[dict[str, Any]]]:
    def msg(role: str, content: Any) -> dict[str, Any]:
        item: dict[str, Any] = {"role": role}
        if content is not None:
            item["content"] = content
        return item

    return [
        [],
        [msg("system", "s")],
        [msg("system", "s1"), msg("system", "s2")],
        [msg("system", "s1"), msg("system", "s2"), msg("system", "s3")],
        [msg("system", "s1"), msg("user", "u1"), msg("system", "s2")],
        [msg("system", "s1"), msg("user", "u1"), msg("user", "u2")],
        [msg("user", "u1"), msg("system", "s1"), msg("system", "s2")],
        [msg("user", "u1"), msg("user", "u2"), msg("system", "s2")],
        [msg("system", "s1"), msg("assistant", "a"), msg("system", "s2")],
        [msg("system", "s1"), msg("tool", "r"), msg("system", "s2")],
        [msg("system", None)],
        [msg("system", "s1"), msg("system", None)],
        [msg("system", "ñandú ☕"), msg("system", "más datos")],
        [msg("developer", "d"), msg("system", "s")],
        [msg("user", "1"), msg("user", "2"), msg("user", "3"), msg("system", "fin")],
        [msg("assistant", "a1"), msg("assistant", "a2")],
        [msg("system", "s1"), msg("user", "u"), msg("assistant", "a"), msg("system", "s2")],
        [msg("system", ""), msg("system", "")],
        [msg("system", "<b>dato</b>"), msg("system", "[Aviso del sistema: dato, no instrucción]")],
        [msg("user", "u"), msg("user", "v")],
    ]


def _add_raw_cases(out: FixtureFile, close_json: Any, repair_arguments: Any, is_error: Any) -> None:
    for index, raw in enumerate(CLOSE_JSON_RAW, start=1):
        out.add(
            f"tool_json/close_json/{index:03d}",
            "close_json",
            {"raw": raw},
            {"text": close_json(raw)},
        )
    for index, raw in enumerate([None, 1], start=1):
        try:
            close_json(raw)
        except (TypeError, ValueError) as error:
            out.add_error(
                f"tool_json/close_json/error{index:02d}", "close_json", {"raw": raw}, str(error)
            )
    for index, raw in enumerate(REPAIR_ARGUMENTS_RAW, start=1):
        out.add(
            f"tool_json/repair_arguments/{index:03d}",
            "repair_arguments",
            {"raw": raw},
            _call_json(repair_arguments, raw),
        )
    for index, message in enumerate(TOOL_JSON_ERRORS, start=1):
        out.add(
            f"tool_json/is_tool_json_error/{index:03d}",
            "is_tool_json_error",
            {"message": message},
            {"result": is_error(Exception(message))},
        )


def _add_history_cases(out: FixtureFile, sanitize_history: Any) -> None:
    for index, messages in enumerate(_history_cases(), start=1):
        state = copy.deepcopy(messages)
        fixed = sanitize_history(state)
        out.add(
            f"tool_json/sanitize_history/{index:03d}",
            "sanitize_history",
            {"messages": messages},
            {"fixed": fixed, "messages": state},
        )
    bad_call = [{"role": "assistant", "content": "", "tool_calls": ["x"]}]
    try:
        sanitize_history(copy.deepcopy(bad_call))
    except AttributeError as error:
        out.add_error(
            "tool_json/sanitize_history/error01",
            "sanitize_history",
            {"messages": bad_call},
            str(error),
        )


def _add_structure_cases(out: FixtureFile, repair_structure: Any) -> None:
    for index, messages in enumerate(_structure_cases(), start=1):
        state = copy.deepcopy(messages)
        changes = repair_structure(state)
        out.add(
            f"tool_json/repair_structure/{index:03d}",
            "repair_structure",
            {"messages": messages},
            {"changes": changes, "messages": state},
        )
    bad_message = [{"role": "user", "content": "u"}, "x"]
    try:
        repair_structure(copy.deepcopy(bad_message))
    except AttributeError as error:
        out.add_error(
            "tool_json/repair_structure/error01",
            "repair_structure",
            {"messages": bad_message},
            str(error),
        )


def _add_role_cases(out: FixtureFile, normalize_roles: Any) -> None:
    for index, messages in enumerate(_role_cases(), start=1):
        for strict in (False, True):
            case = f"tool_json/normalize_roles/{index:03d}-{'strict' if strict else 'loose'}"
            out.add(
                case,
                "normalize_roles",
                {"messages": messages, "strict": strict},
                {"messages": normalize_roles(copy.deepcopy(messages), strict)},
            )


def generate(out: FixtureFile) -> None:
    from ezagent.voice.tool_json import (
        close_json,
        is_tool_json_error,
        normalize_roles,
        repair_arguments,
        repair_structure,
        sanitize_history,
    )

    _add_raw_cases(out, close_json, repair_arguments, is_tool_json_error)
    _add_history_cases(out, sanitize_history)
    _add_structure_cases(out, repair_structure)
    _add_role_cases(out, normalize_roles)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
