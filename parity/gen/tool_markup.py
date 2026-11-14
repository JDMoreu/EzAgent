"""Fixtures de `ezagent.voice.tool_markup` (F1-02).

El marcado interno de herramienta se corta en cuanto aparece, aunque el stream lo
parta en trozos. Cada caso recorre el guard con una lista de fragmentos.

* ``feed``  entrada ``{"chunks": [str, ...]}`` -> ``{"text": str, "tripped": bool}``
* ``flush`` entrada ``{"chunks": [str, ...]}`` -> ``{"text": str, "flush": str,
  "flush_again": str, "tripped": bool}``
"""

from __future__ import annotations

from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "tool_markup"

MARKERS = [
    "<｜dsml｜",
    "<|dsml|",
    "<dsml",
    "｜dsml｜",
    "<｜tool▁call",
    "<｜tool_call",
    "<|tool_call",
    "<tool_call>",
    "<tool_calls>",
    "<function_call",
    "<function_calls>",
    "[tool_calls]",
    "<|python_tag|>",
]

DSML = '<｜DSML｜ calls>\n<｜DSML｜ invoke name="delegate_to_agent">'


def _split(text: str, size: int) -> list[str]:
    return [text[index : index + size] for index in range(0, len(text), size)]


def _feed_cases() -> list[list[str]]:
    cases: list[list[str]] = [
        [],
        [""],
        ["", "", ""],
        ["Hola, ", "¿qué tal? a < b y <b>negrita</b>."],
        ["Hola mundo"],
        ["Listo. " + DSML],
        ["Listo. " + DSML + "<｜DSML｜ parameter>x"],
        ["<tool_call>{}"],
        ["Antes <tool_calls> después"],
        ["Va. <tool_call>{}", "más basura"],
        ["Mira <", "｜"],
        ["Mira <｜", "D"],
        ["<", "｜", "tool", "_call", ">"],
        ["<", "|", "python", "_tag", "|>"],
        ["[tool_", "calls]"],
        ["texto con [tool_calls] aquí", " y más"],
        ["a < b > c", " <3 ", "<= 4"],
        ["ñandú ☕ y 中文"],
        ["🙂" * 20],
        ["x" * 500, " <dsml", ">"],
        ["<DSML>"],
        ["<Tool_Call>"],
        ["<TOOL_CALLS>"],
        ["<｜TOOL▁CALL"],
        ["<function_call", ">x"],
        ["<function_calls>"],
        ["<｜dsml｜", ' invoke name="x">'],
        ["prefijo <", "|dsml| fin"],
        ["a", "b", "c", "d", "e", "<tool_call>x", "z"],
        ["sin marcador ", "pero < y | sueltos"],
        [DSML, DSML],
        ["<t", "o", "o", "l", "_", "c", "a", "l", "l", ">"],
        ["resultado: 1 < 2 y 3 > 2"],
    ]
    for size in (1, 2, 3, 4, 5, 6, 7, 8, 13):
        cases.append(_split("Listo. " + DSML + "<｜DSML｜ parameter>x", size))
    for marker in MARKERS:
        cases.append(["Listo. ", marker, " resto"])
        cases.append(_split("Antes " + marker.upper() + " después", 2))
        cases.append(_split(marker, 3))
    return cases


def _flush_cases() -> list[list[str]]:
    cases: list[list[str]] = [
        [],
        [""],
        ["Mira <"],
        ["Mira <｜"],
        ["Mira <|"],
        ["Mira <｜d"],
        ["Mira <｜ds"],
        ["Mira <tool_"],
        ["Mira <tool_c"],
        ["Mira <|python_tag"],
        ["Mira [tool_c"],
        ["abc<"],
        ["texto normal"],
        ["texto normal", " y más"],
        ["Va. <tool_call>{}"],
        ["Va. <tool_call>{}", "basura"],
        ["<dsml"],
        ["<｜tool▁call"],
        ["<function_call"],
        ["Hola, ", "¿qué tal?"],
        ["ñandú ☕ <"],
        ["🙂 <🙂"],
        ["a < b"],
        ["1 < 2 > 0"],
        ["x" * 300 + " <"],
    ]
    for size in (1, 2, 3, 5, 8):
        cases.append(_split("Listo. " + DSML + "<resto>", size))
    return cases


def _guard(chunks: list[str]) -> tuple[Any, str]:
    from ezagent.voice.tool_markup import ToolMarkupGuard

    guard = ToolMarkupGuard()
    return guard, "".join(guard.feed(chunk) for chunk in chunks)


def generate(out: FixtureFile) -> None:
    for index, chunks in enumerate(_feed_cases(), start=1):
        guard, text = _guard(chunks)
        out.add(
            f"tool_markup/feed/{index:03d}",
            "feed",
            {"chunks": chunks},
            {"text": text, "tripped": guard.tripped},
        )

    for index, chunks in enumerate(_flush_cases(), start=1):
        guard, text = _guard(chunks)
        released = guard.flush()
        again = guard.flush()
        out.add(
            f"tool_markup/flush/{index:03d}",
            "flush",
            {"chunks": chunks},
            {
                "text": text,
                "flush": released,
                "flush_again": again,
                "tripped": guard.tripped,
            },
        )


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
