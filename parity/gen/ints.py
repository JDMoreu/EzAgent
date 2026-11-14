"""Fixtures de `int()` (dominio `ints`, F2-01/F2-02).

`VoiceConfig._int` traga el error y devuelve el valor por defecto: el fixture
fija el resultado de `_int(value, default)` para todos los tipos que llegan de
un JSON, incluidos los que Python acepta y Rust rechazaría de forma ingenua
(guiones bajos, dígitos de ancho completo, espacios Unicode).
"""

from __future__ import annotations

from _common import FixtureFile, main_guard

DOMAIN = "ints"

DEFAULT = 7

VALUES = [
    True,
    False,
    0,
    1,
    -3,
    12.9,
    -12.9,
    0.0,
    -0.0,
    12.0,
    9007199254740993,
    None,
    [],
    {},
    "12",
    " 12 ",
    "+5",
    "-0",
    "007",
    "1_000",
    "1__0",
    "_1",
    "１２",
    "\u00a012\u00a0",
    "1e3",
    "0x10",
    "12.7",
    "",
    "   ",
    "１２３４",
    "9223372036854775807",
    "-9223372036854775808",
]


def generate(out: FixtureFile) -> None:
    from ezagent.voice.config import VoiceConfig

    for index, value in enumerate(VALUES, start=1):
        try:
            result = VoiceConfig._int(value, DEFAULT)
            out.add(f"ints/000{index:03d}" if index < 10 else f"ints/{index:04d}", "_int", {"value": value, "default": DEFAULT}, {"int": result}, None)
        except Exception as error:
            out.add(
                f"ints/{index:04d}",
                "_int",
                {"value": value, "default": DEFAULT},
                None,
                f"{type(error).__name__}: {error}",
            )


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
