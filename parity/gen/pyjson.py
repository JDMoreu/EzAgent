"""Fixtures de `json.dumps` (dominio `pyjson`, F2-01).

Fija el texto exacto que escribe Python para que `ez_core::pyjson` lo reproduzca:
separadores, `ensure_ascii`, formato de flotantes y orden de claves.
"""

from __future__ import annotations

import json

from _common import FixtureFile, main_guard

DOMAIN = "pyjson"


def _dump(value: object, **options: object) -> str:
    return json.dumps(value, **options)  # type: ignore[arg-type]


CASES: list[tuple[object, dict[str, object]]] = [
    ({}, {}),
    ([], {}),
    ({"a": 1}, {}),
    ({"b": 1, "a": 2}, {}),
    ({"z": [1, 2, 3], "a": {"n": None}}, {}),
    (None, {}),
    (True, {}),
    (False, {}),
    ("texto plano", {}),
    ("tildes: ñáéíóú", {}),
    ("comillas \" y \\ barras", {}),
    ("emoji 🙂🚀", {}),
    ("salto\nde línea\ttabulador", {}),
    (0.1, {}),
    (1.0, {}),
    (-0.0, {}),
    (1e-5, {}),
    (1e-7, {}),
    (1.5e20, {}),
    (123456789012345678, {}),
    (0.30000000000000004, {}),
    ({"latencia": 0.000123, "tts": 1.05}, {}),
    ({"clave con espacios": "valor"}, {}),
    ([{"case": "x", "fn": "y"}], {}),
    ({"ts": 1735689600.5, "kind": "turn"}, {"ensure_ascii": True}),
    ({"nombre": "José"}, {"ensure_ascii": True}),
    ({"nombre": "José"}, {"ensure_ascii": False}),
    ({"a": 1, "b": 2}, {"indent": 2}),
    ({"a": [1, {"b": 2}]}, {"indent": 2, "sort_keys": True}),
    ([1, 2, 3], {"separators": (",", ":")}),
    ({"a": "ñ"}, {"separators": (",", ":")}),
    (["a", "b"], {"indent": 4}),
    ({"n": 1.0e-05, "m": 2.5}, {"indent": 2}),
    ({"unicode": "á\nb"}, {"indent": 2, "ensure_ascii": True}),
    ("a" * 300, {}),
    ({"lista": list(range(20))}, {"indent": 1}),
]


def generate(out: FixtureFile) -> None:
    for index, (value, options) in enumerate(CASES, start=1):
        case = f"pyjson/{index:04d}"
        out.add(
            case,
            "dumps",
            {"value": value, "options": options},
            {"text": _dump(value, **options)},
        )

    # round-trip de los archivos del proyecto: nada se pierde al releer
    samples = [
        {"kind": "turn", "vad_wait_ms": 1297.0, "stt_ready_ms": 125.0},
        {"voice": "ef_dora", "speed": 1.1, "silence_secs": 1.3},
        {"ok": True, "items": [{"id": "a", "n": 1}], "none": None},
    ]
    for index, sample in enumerate(samples, start=1):
        text = _dump(sample)
        out.add(
            f"pyjson/rt{index:03d}",
            "round_trip",
            {"value": sample},
            {"text": text, "back": json.loads(text)},
        )


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
