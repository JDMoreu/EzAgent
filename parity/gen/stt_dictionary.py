"""Fixtures del diccionario de correcciones de STT (dominio `stt_dictionary`, F1-02).

Cubre las cuatro funciones públicas: `parse_rules`, `normalize_rules`,
`transcription_hint` y `correct_transcription`. Los errores se fijan con el
mensaje real de `ValueError`, que es lo que ve el usuario en ajustes.
"""

from __future__ import annotations

from collections.abc import Callable, Iterable
from typing import Any

from _common import FixtureFile, main_guard

from ezagent.voice.stt_dictionary import (
    correct_transcription,
    normalize_rules,
    parse_rules,
    transcription_hint,
)

DOMAIN = "stt_dictionary"


def _many(count: int) -> str:
    return "; ".join(f"origen{index} → destino{index}" for index in range(1, count + 1))


def _error_of(fn: Callable[..., Any], *args: object, **kwargs: object) -> str:
    try:
        fn(*args, **kwargs)
    except Exception as error:  # el oráculo es el mensaje real de Python
        return str(error)
    raise AssertionError(f"{fn.__name__} no falló con {args!r}")


PARSE_OK: list[str] = [
    "",
    "   ",
    ";",
    "\n\n",
    "hola → hello",
    "hola→hello",
    "hola => hello",
    "  hola   →   hello  ",
    "a → b; c → d",
    "a → b\nc → d",
    "a → b\r\nc → d",
    "Exprés → Expres",
    "ÑANDÚ → NANDU",
    "emoji 🙂 → smile",
    "kubernetes → Kubernetes",
    "dos palabras → dos palabras unidas",
    "a → b → c",
    "; ;b → c",
    "¿qué? → qué",
    "uno, dos → 1, 2",
    "origen con  espacios   internos → destino",
    _many(100),
    "a → " + "b" * 11996,
]

PARSE_ERRORS: list[str] = [
    "sin flecha",
    "→ sin origen",
    "origen → ",
    "a → b; c",
    "x → y; X → z",
    "Hola Mundo → x; hola   mundo → y",
    _many(101),
    "x" * 12001,
]

NORMALIZE_OK: list[str] = [
    "",
    "   ",
    "hola→hello",
    " a  →  b ; c=>d ",
    "a → b\nc → d",
    "Exprés → Expres; ÑANDÚ → NANDU",
    "a → b → c",
    "a=>b;c→d",
    "  a  →  b  \n  c  →  d  ",
    "origen → destino → extra",
    "ÑANDÚ → NANDU",
    "dos  palabras → dos palabras unidas",
    "x → y; ; z → w",
    "k → v\r\nm → n",
    "uno, dos → 1, 2",
    "emoji 🙂 → smile",
    "origen con  espacios   internos → destino",
    _many(100),
]

NORMALIZE_ERRORS: list[str] = [
    "sin flecha",
    "a → b; c",
    "x → y; X → z",
    "x" * 12001,
]

HINT_CASES: list[tuple[str, str, list[str]]] = [
    ("texto previo", "", []),
    ("  texto  ", "a → b", []),
    ("", "", []),
    ("   ", "", []),
    ("\n", "", []),
    ("", "hola → hello", []),
    ("", "a → uno; b → dos", []),
    ("", "x →   dos   palabras", []),
    ("", "claude code → Claude Code", []),
    ("", "herdr → Herdr; ezagent → EzAgent", []),
    ("", "largo → " + "y" * 100, []),
    ("", "", ["mi_agente"]),
    ("", "", ["a", "a", "", "   "]),
    ("", "", ["3", "7"]),
    ("", "", ["nombre_" + "z" * 80]),
    ("", "", ["a__b"]),
    ("", "", ["Herdr"]),
    ("", "", ["herdr", "HERDR"]),
    ("", _many(30), []),
    ("", "; ".join(f"regla{index} → " + "x" * 80 for index in range(1, 12)), []),
    ("", "; ".join(f"regla{index} → " + str(index) * 80 for index in range(1, 12)), []),
    ("", "uno → uno, dos", []),
]

HINT_ERRORS: list[tuple[str, str, list[str]]] = [
    ("", "sin flecha", []),
    ("", "x" * 12001, []),
    ("", "x → y; X → z", []),
]

CORRECT_CASES: list[tuple[str, str]] = [
    ("", ""),
    ("texto igual", ""),
    ("hola", "hola → hello"),
    ("Hola", "hola → hello"),
    ("HOLA mundo", "hola → hello"),
    ("hola mundo", "hola mundo → saludo"),
    ("Hola   Mundo", "hola mundo → saludo"),
    ("a b", "a → 1; a b → 2"),
    ("sol y solar", "sol → sun"),
    ("ÑANDÚ en el campo", "ñandú → nandu"),
    ("hola, mundo.", "hola → hello; mundo → world"),
    ("c++ es rápido", "c++ → C mas mas"),
    ("a.b y axb", "a.b → A_B"),
    ("precio", "precio → $1"),
    ("estoy 🙂 hoy", "🙂 → feliz"),
    ("new york city", "new york → NY"),
    ("sol sol sol", "sol → sun"),
    ("uno dos", "uno → dos; dos → tres"),
    ("   ", "x → y"),
    ("gúugel y Gúugel", "gúugel → google"),
    ("EzAgent abrió ezagent", "azure → azure"),
    ("token tokenizado", "token → clave"),
]

CORRECT_ERRORS: list[tuple[str, str]] = [
    ("hola", "sin flecha"),
    ("hola", "x → y; X → z"),
]


def _add_parse(out: FixtureFile) -> None:
    for index, value in enumerate(PARSE_OK, start=1):
        out.add(
            f"stt_dictionary/parse/{index:04d}",
            "parse_rules",
            {"value": value},
            {"rules": [list(pair) for pair in parse_rules(value)]},
        )
    for index, value in enumerate(PARSE_ERRORS, start=1):
        out.add_error(
            f"stt_dictionary/parse/err{index:02d}",
            "parse_rules",
            {"value": value},
            _error_of(parse_rules, value),
        )


def _add_normalize(out: FixtureFile) -> None:
    for index, value in enumerate(NORMALIZE_OK, start=1):
        out.add(
            f"stt_dictionary/normalize/{index:04d}",
            "normalize_rules",
            {"value": value},
            {"text": normalize_rules(value)},
        )
    for index, value in enumerate(NORMALIZE_ERRORS, start=1):
        out.add_error(
            f"stt_dictionary/normalize/err{index:02d}",
            "normalize_rules",
            {"value": value},
            _error_of(normalize_rules, value),
        )


def _hint(prompt: str, dictionary: str, agents: Iterable[str]) -> str:
    return transcription_hint(prompt, dictionary, agents)


def _add_hint(out: FixtureFile) -> None:
    for index, (prompt, dictionary, agents) in enumerate(HINT_CASES, start=1):
        out.add(
            f"stt_dictionary/hint/{index:04d}",
            "transcription_hint",
            {"prompt": prompt, "dictionary": dictionary, "agents": agents},
            {"text": _hint(prompt, dictionary, agents)},
        )
    for index, (prompt, dictionary, agents) in enumerate(HINT_ERRORS, start=1):
        out.add_error(
            f"stt_dictionary/hint/err{index:02d}",
            "transcription_hint",
            {"prompt": prompt, "dictionary": dictionary, "agents": agents},
            _error_of(_hint, prompt, dictionary, agents),
        )


def _add_correct(out: FixtureFile) -> None:
    for index, (text, dictionary) in enumerate(CORRECT_CASES, start=1):
        out.add(
            f"stt_dictionary/correct/{index:04d}",
            "correct_transcription",
            {"text": text, "dictionary": dictionary},
            {"text": correct_transcription(text, dictionary)},
        )
    for index, (text, dictionary) in enumerate(CORRECT_ERRORS, start=1):
        out.add_error(
            f"stt_dictionary/correct/err{index:02d}",
            "correct_transcription",
            {"text": text, "dictionary": dictionary},
            _error_of(correct_transcription, text, dictionary),
        )


def generate(out: FixtureFile) -> None:
    _add_parse(out)
    _add_normalize(out)
    _add_hint(out)
    _add_correct(out)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
