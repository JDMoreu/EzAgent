"""Fixtures de la frase de reactivación (dominio `wake`, F1-08).

Cubre `normalize`, `wake_forms`, `is_wake_phrase`, `utterance_secs` y
`worth_transcribing` de `ezagent.voice.wake`, con los textos literales y los
errores reales del Python (`py-baseline`).

Los PCM se describen con un objeto (`input.pcm`) para no repetir megabytes de
bytes en el JSONL:

* ``{"kind": "hex", "hex": "0100..."}`` — bytes exactos.
* ``{"kind": "pattern", "bytes": n, "pattern": "ramp", "seed": s}`` — el byte
  ``j`` vale ``(seed + j) % 256``.
"""

from __future__ import annotations

from typing import Any

from _common import FixtureFile, main_guard

from ezagent.voice.wake import (
    DEFAULT_WAKE_PHRASE,
    is_wake_phrase,
    normalize,
    utterance_secs,
    wake_forms,
    worth_transcribing,
)

DOMAIN = "wake"


def _pcm(spec: dict[str, Any]) -> bytes:
    kind = spec["kind"]
    if kind == "hex":
        return bytes.fromhex(spec["hex"])
    if kind == "pattern":
        size = int(spec["bytes"])
        step = int(spec.get("seed", 0))
        return bytes((step + index) % 256 for index in range(size))
    raise ValueError(f"pcm desconocido: {kind}")


NORMALIZE_CASES: list[str] = [
    "",
    "   ",
    "\t\n",
    "EzAgent, escúchame!!",
    "ÉL NIÑO",
    "ÁÉÍÓÚÜÑ",
    "áéíóúüñ",
    "A,,,b",
    "¿Qué? ¡Ya!",
    "🙂",
    "a🙂b",
    "a 🙂 b",
    "A\t\nB",
    "  múltiples   espacios  ",
    "İstanbul",
    "Straße",
    "ﬁ",
    "①",
    "½",
    "ⅷ",
    "a-b_c",
    "a.b,c;d",
    "¿¡!?",
    "mañana",
    "pingüino",
    "café",
    "naïve",
    "Ångström",
    "Œuf",
    "Ægir",
    "Ø",
    "Łódź",
    "Здравствуйте",
    "こんにちは",
    "中文",
    "a\u00a0b",
    "a\u2003b",
    "a\u200bb",
    "a\u2028b",
    "a\u2029b",
    "a\u001cb",
    "a\u001db",
    "a\u001eb",
    "a\u001fb",
    "a\x00b",
    "EZAGENT",
    "EzAgent",
    "ez agent",
    "easy agent",
    "e z agent",
    "isi agent",
    "isiagent",
    "esagent",
    "es agent",
    "eazy agent",
    "easy agents",
    "ezagents",
    "abc123",
    "123",
    "0",
    "a1b2",
    "A1 B2",
    "a\u0301",
    "e\u0301",
    "n\u0303",
    "ñ",
    "ß",
    "ẞ",
    "ﬁle",
    "e\u0301\u0301",
    "  a  b  ",
    "a\nb",
    "a\r\nb",
    "a\u000bb",
    "a\u000cb",
    "ezagent,   escúchame   por   favor",
    "¡EzAgent! ¿Escúchame?",
    "E-Z-Agent",
    "easy-agent",
    "a🙂🙃b",
    "🙂🙃",
    " \u00a0 ",
    "ÑANDÚ",
    "café\r\nmañana",
    "١٢٣",
    "ＡＢＣ",
    "ｆｕｌｌ",
]

WAKE_FORMS_CASES: list[str] = [
    DEFAULT_WAKE_PHRASE,
    "",
    "   ",
    "hola",
    "a trabajar",
    "EzAgent",
    "EzAgent, escúchame",
    "oye ezagent escuchame",
    "  EzAgent,  A TRABAJAR  ",
    "easy agent despierta",
    "EzAgent escúchame por favor",
    "abre la escucha",
    "despierta",
    "vuelve",
    "escúchame",
    "EzAgent, a trabajar ya",
    "a trabajar ezagent",
    "e z agent, hola asistente",
    "isiagent, abre la escucha",
    "¡EzAgent, escúchame!",
    "EzAgent, escúchame; y luego trabaja",
    "EzAgent\n escúchame",
    "áéí",
    "🙂",
    "hola asistente",
    "hola asistente por favor",
    "EZAGENT ESCUCHAME",
    "ezagents",
    "easy agents",
    "hola",
    "ezagent ezagent",
    "ezagent oye",
    "oye ezagent",
    "hola ezagent escuchame ya",
    "por favor ezagent escuchame",
]

IS_WAKE_CASES: list[tuple[str, str]] = [
    ("EzAgent, escúchame", DEFAULT_WAKE_PHRASE),
    ("ezagent escuchame", DEFAULT_WAKE_PHRASE),
    ("EZAGENT ESCUCHAME", DEFAULT_WAKE_PHRASE),
    ("Escúchame, EzAgent", DEFAULT_WAKE_PHRASE),
    ("  EzAgent   escúchame ", DEFAULT_WAKE_PHRASE),
    ("Oye EzAgent, escúchame por favor", DEFAULT_WAKE_PHRASE),
    ("hey ezagent escúchame por favor", DEFAULT_WAKE_PHRASE),
    ("Ey, EzAgent, despierta.", DEFAULT_WAKE_PHRASE),
    ("Hola, EzAgent escúchame ya", DEFAULT_WAKE_PHRASE),
    ("hola ez agent vuelve ya", DEFAULT_WAKE_PHRASE),
    ("EZ AGENT ABRE LA ESCUCHA", DEFAULT_WAKE_PHRASE),
    ("¡EzAgent, escúchame!", DEFAULT_WAKE_PHRASE),
    ("E Z Agent, escúchame", DEFAULT_WAKE_PHRASE),
    ("easy agents despierta", DEFAULT_WAKE_PHRASE),
    ("isiagent escuchame", DEFAULT_WAKE_PHRASE),
    ("isi agent vuelve", DEFAULT_WAKE_PHRASE),
    ("esagent abre la escucha", DEFAULT_WAKE_PHRASE),
    ("es agent despierta", DEFAULT_WAKE_PHRASE),
    ("eazy agent escuchame", DEFAULT_WAKE_PHRASE),
    ("easyagent escuchame", DEFAULT_WAKE_PHRASE),
    ("ezagents despierta", DEFAULT_WAKE_PHRASE),
    ("ÉZ ÁGENT, ESCÚCHAME", DEFAULT_WAKE_PHRASE),
    ("e-z agent, escúchame", DEFAULT_WAKE_PHRASE),
    ("¡EZ ÁGENT! ¿ESCÚCHAME?", DEFAULT_WAKE_PHRASE),
    ("Oye oye EzAgent escúchame por favor", DEFAULT_WAKE_PHRASE),
    ("hey ey hola ezagent escuchame", DEFAULT_WAKE_PHRASE),
    ("hola ezagent escuchame ya ya", DEFAULT_WAKE_PHRASE),
    ("ezagent escuchame por favor porfa", DEFAULT_WAKE_PHRASE),
    ("despierta ezagent", DEFAULT_WAKE_PHRASE),
    ("vuelve ez agent", DEFAULT_WAKE_PHRASE),
    ("abre la escucha, easy agent", DEFAULT_WAKE_PHRASE),
    ("EzAgent", DEFAULT_WAKE_PHRASE),
    ("ezagent escucha", DEFAULT_WAKE_PHRASE),
    ("escúchame", DEFAULT_WAKE_PHRASE),
    ("hola", DEFAULT_WAKE_PHRASE),
    ("vuelve", DEFAULT_WAKE_PHRASE),
    ("despierta", DEFAULT_WAKE_PHRASE),
    ("oye", DEFAULT_WAKE_PHRASE),
    ("", DEFAULT_WAKE_PHRASE),
    ("   ", DEFAULT_WAKE_PHRASE),
    ("🙂", DEFAULT_WAKE_PHRASE),
    ("hola cómo estás", DEFAULT_WAKE_PHRASE),
    ("EzAgent, escúchame esta canción que me gusta mucho", DEFAULT_WAKE_PHRASE),
    ("le dije a ezagent escúchame", DEFAULT_WAKE_PHRASE),
    ("EzAgent no me escuches", DEFAULT_WAKE_PHRASE),
    ("ezagent escuchame y borra todo", DEFAULT_WAKE_PHRASE),
    ("voy a hablar con el agente", DEFAULT_WAKE_PHRASE),
    ("ezagent escuchame por favor", DEFAULT_WAKE_PHRASE),
    ("hola ezagent escuchame", DEFAULT_WAKE_PHRASE),
    ("oye ezagent", DEFAULT_WAKE_PHRASE),
    ("ezagent oye", DEFAULT_WAKE_PHRASE),
    ("ezagent, por favor", DEFAULT_WAKE_PHRASE),
    ("hola asistente", "hola asistente"),
    ("hola asistente", "Hola, asistente"),
    ("EzAgent, hola asistente", "hola asistente"),
    ("ezagent hola asistente", "hola asistente"),
    ("hola asistente ezagent", "hola asistente"),
    ("hola", "hola"),
    ("ezagent hola", "hola"),
    ("ezagent, hola", "hola"),
    ("", ""),
    ("EzAgent, escúchame", ""),
    ("hola", ""),
    ("EzAgent, a trabajar", "EzAgent, a trabajar"),
    ("ezagent a trabajar", "EzAgent, a trabajar"),
    ("a trabajar ezagent", "EzAgent, a trabajar"),
    ("ezagent, escúchame", "EzAgent, a trabajar"),
    ("a trabajar", "EzAgent, a trabajar"),
    ("", "a trabajar"),
    ("ezagent a trabajar", "a trabajar"),
    ("a trabajar ezagent", "a trabajar"),
    ("a trabajar", "a trabajar"),
    ("ezagent", "EzAgent"),
    ("ezagent", "EzAgent, escúchame"),
    ("ezagent escuchame", "EzAgent"),
    ("ezagent vuelve", "ezagent"),
    ("ezagent despierta", "ezagent"),
    ("ezagent abre la escucha", "ezagent"),
    ("hola ezagent escuchame", "ezagent"),
    ("ezagent hola", "ezagent"),
    ("escuchame ezagent", "ezagent"),
    ("EzAgent, escúchame, y nada más", DEFAULT_WAKE_PHRASE),
    ("hola ezagent escuchame por favor ya", DEFAULT_WAKE_PHRASE),
    ("por favor ezagent escuchame", DEFAULT_WAKE_PHRASE),
    ("ezagent escuchame porfa", DEFAULT_WAKE_PHRASE),
    ("ezagent escuchame ya", DEFAULT_WAKE_PHRASE),
    ("ezagent escuchame ya ya", DEFAULT_WAKE_PHRASE),
    ("oye oye oye ezagent escuchame", DEFAULT_WAKE_PHRASE),
    ("hola hola ezagent escuchame", DEFAULT_WAKE_PHRASE),
    ("hey ezagent despierta por favor", DEFAULT_WAKE_PHRASE),
    ("EzAgent escúchame\t", DEFAULT_WAKE_PHRASE),
    ("EzAgent escúchame\n", DEFAULT_WAKE_PHRASE),
    ("EzAgent\u00a0escúchame", DEFAULT_WAKE_PHRASE),
    ("EzAgent\u2003escúchame", DEFAULT_WAKE_PHRASE),
    ("E.Z. Agent despierta", DEFAULT_WAKE_PHRASE),
    ("easy agents, despierta", DEFAULT_WAKE_PHRASE),
    ("easy-agents despierta", DEFAULT_WAKE_PHRASE),
    ("isi agents despierta", DEFAULT_WAKE_PHRASE),
    ("e z agents despierta", DEFAULT_WAKE_PHRASE),
    ("ezagent abre la escucha por favor", DEFAULT_WAKE_PHRASE),
    ("abre la escucha ezagent porfa", DEFAULT_WAKE_PHRASE),
]

UTTERANCE_CASES: list[tuple[dict[str, Any], int]] = [
    ({"kind": "hex", "hex": ""}, 16000),
    ({"kind": "hex", "hex": ""}, 0),
    ({"kind": "hex", "hex": "00"}, 16000),
    ({"kind": "hex", "hex": "0000"}, 16000),
    ({"kind": "pattern", "bytes": 1000, "pattern": "ramp"}, 16000),
    ({"kind": "pattern", "bytes": 32000, "pattern": "ramp"}, 16000),
    ({"kind": "pattern", "bytes": 32000, "pattern": "ramp"}, 8000),
    ({"kind": "pattern", "bytes": 32000, "pattern": "ramp"}, 44100),
    ({"kind": "pattern", "bytes": 32001, "pattern": "ramp"}, 16000),
    ({"kind": "pattern", "bytes": 96000, "pattern": "ramp"}, 16000),
    ({"kind": "pattern", "bytes": 960000, "pattern": "ramp"}, 16000),
    ({"kind": "pattern", "bytes": 2, "pattern": "ramp"}, 1),
    ({"kind": "pattern", "bytes": 2, "pattern": "ramp"}, 2),
    ({"kind": "pattern", "bytes": 3, "pattern": "ramp"}, 3),
    ({"kind": "pattern", "bytes": 2, "pattern": "ramp"}, 0),
    ({"kind": "pattern", "bytes": 2, "pattern": "ramp"}, -16000),
    ({"kind": "pattern", "bytes": 32000, "pattern": "ramp"}, -1),
]

WORTH_CASES: list[tuple[dict[str, Any], int]] = [
    ({"kind": "hex", "hex": ""}, 16000),
    ({"kind": "pattern", "bytes": 11199, "pattern": "ramp"}, 16000),
    ({"kind": "pattern", "bytes": 11200, "pattern": "ramp"}, 16000),
    ({"kind": "pattern", "bytes": 11201, "pattern": "ramp"}, 16000),
    ({"kind": "pattern", "bytes": 95999, "pattern": "ramp"}, 16000),
    ({"kind": "pattern", "bytes": 96000, "pattern": "ramp"}, 16000),
    ({"kind": "pattern", "bytes": 96001, "pattern": "ramp"}, 16000),
    ({"kind": "pattern", "bytes": 96002, "pattern": "ramp"}, 16000),
    ({"kind": "pattern", "bytes": 5600, "pattern": "ramp"}, 8000),
    ({"kind": "pattern", "bytes": 5599, "pattern": "ramp"}, 8000),
    ({"kind": "pattern", "bytes": 48000, "pattern": "ramp"}, 48000),
    ({"kind": "pattern", "bytes": 48001, "pattern": "ramp"}, 48000),
    ({"kind": "pattern", "bytes": 30870, "pattern": "ramp"}, 44100),
    ({"kind": "pattern", "bytes": 30869, "pattern": "ramp"}, 44100),
    ({"kind": "pattern", "bytes": 264600, "pattern": "ramp"}, 44100),
    ({"kind": "pattern", "bytes": 264601, "pattern": "ramp"}, 44100),
    ({"kind": "pattern", "bytes": 2085, "pattern": "ramp"}, 3000),
    ({"kind": "pattern", "bytes": 2084, "pattern": "ramp"}, 3000),
    ({"kind": "pattern", "bytes": 2, "pattern": "ramp"}, 0),
    ({"kind": "pattern", "bytes": 2, "pattern": "ramp"}, -16000),
    ({"kind": "pattern", "bytes": 32000, "pattern": "ramp"}, 1),
    ({"kind": "pattern", "bytes": 32000, "pattern": "ramp"}, 1000000),
]


def _add_normalize(out: FixtureFile) -> None:
    for index, text in enumerate(NORMALIZE_CASES, start=1):
        out.add(
            f"wake/normalize/{index:04d}",
            "normalize",
            {"text": text},
            {"text": normalize(text)},
        )
    out.add(
        "wake/normalize/null",
        "normalize",
        {"text": None},
        {"text": normalize(None)},  # type: ignore[arg-type]
    )


def _add_wake_forms(out: FixtureFile) -> None:
    for index, phrase in enumerate(WAKE_FORMS_CASES, start=1):
        out.add(
            f"wake/wake_forms/{index:04d}",
            "wake_forms",
            {"phrase": phrase},
            {"forms": sorted(wake_forms(phrase))},
        )
    out.add(
        "wake/wake_forms/null",
        "wake_forms",
        {"phrase": None},
        {"forms": sorted(wake_forms(None))},  # type: ignore[arg-type]
    )


def _add_is_wake(out: FixtureFile) -> None:
    for index, (text, phrase) in enumerate(IS_WAKE_CASES, start=1):
        out.add(
            f"wake/is_wake_phrase/{index:04d}",
            "is_wake_phrase",
            {"text": text, "phrase": phrase},
            {"wake": is_wake_phrase(text, phrase)},
        )


def _add_utterance(out: FixtureFile) -> None:
    for index, (spec, rate) in enumerate(UTTERANCE_CASES, start=1):
        pcm = _pcm(spec)
        out.add(
            f"wake/utterance_secs/{index:04d}",
            "utterance_secs",
            {"pcm": spec, "rate": rate},
            {"secs": utterance_secs(pcm, rate)},
        )


def _add_worth(out: FixtureFile) -> None:
    for index, (spec, rate) in enumerate(WORTH_CASES, start=1):
        pcm = _pcm(spec)
        out.add(
            f"wake/worth_transcribing/{index:04d}",
            "worth_transcribing",
            {"pcm": spec, "rate": rate},
            {"worth": worth_transcribing(pcm, rate)},
        )


def generate(out: FixtureFile) -> None:
    _add_normalize(out)
    _add_wake_forms(out)
    _add_is_wake(out)
    _add_utterance(out)
    _add_worth(out)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
