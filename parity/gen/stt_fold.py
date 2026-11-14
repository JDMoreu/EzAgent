"""Fixtures de plegado de mayúsculas en el diccionario (dominio `stt_fold`).

`str.casefold()` no es `str.lower()`: `Straße`/`STRASSE`, sigma final y
ligaduras colapsan igual para Python. Estas reglas violan la unicidad si Rust
usa `to_lowercase()`.
"""

from __future__ import annotations

from _common import FixtureFile, main_guard

DOMAIN = "stt_fold"

RULES = [
    ("Straße", "calle"),
    ("STRASSE", "avenida"),
    ("Σ", "sigma"),
    ("ς", "sigma final"),
    ("İstanbul", "estambul"),
    ("ﬀ", "efe doble"),
]

TEXTS = [
    "la Straße está lejos",
    "LA STRASSE está lejos",
    "la strasse está lejos",
    "Σ y ς son sigmas",
    "İstanbul y Estambul",
    "el ﬀ raro",
    "texto sin reglas",
]


def generate(out: FixtureFile) -> None:
    from ezagent.voice import stt_dictionary

    for index, (source, target) in enumerate(RULES, start=1):
        out.add(
            f"stt_fold/parse/{index:02d}",
            "parse_rules",
            {"value": f"{source} → {target}"},
            {"rules": [list(pair) for pair in stt_dictionary.parse_rules(f"{source} → {target}")]},
        )

    duplicates = [
        ("Straße → uno; STRASSE → dos", "casefold aleman"),
        ("Σ → uno; ς → dos", "casefold sigma"),
        ("ﬀ → uno; ff → dos", "casefold ligadura"),
        ("İstanbul → uno; i̇stanbul → dos", "casefold turco"),
    ]
    for index, (value, label) in enumerate(duplicates, start=1):
        try:
            parsed = stt_dictionary.parse_rules(value)
            payload = {"ok": True, "rules": [list(pair) for pair in parsed]}
            error = None
        except Exception as caught:
            payload = None
            error = str(caught)
        out.add(f"stt_fold/dup/{index:02d}/{label.replace(' ', '-')}", "parse_rules", {"value": value}, payload, error)

    # sin duplicados a proposito: aqui se mide el reemplazo, no la unicidad
    dictionary = "Straße → calle; Code → código"
    for index, text in enumerate(TEXTS, start=1):
        out.add(
            f"stt_fold/correct/{index:02d}",
            "correct_transcription",
            {"text": text, "dictionary": dictionary},
            {"text": stt_dictionary.correct_transcription(text, dictionary)},
        )

    overlapping = "Open Code → OpenCode; Open → abrir; Code → código"
    for index, text in enumerate(
        ["Open Code abierto", "abrir el Code", "open code y code", "OPEN CODE"],
        start=1,
    ):
        out.add(
            f"stt_fold/overlap/{index:02d}",
            "correct_transcription",
            {"text": text, "dictionary": overlapping},
            {"text": stt_dictionary.correct_transcription(text, overlapping)},
        )

    hint_cases = ["", " ", "prompt del usuario"]
    for index, prompt in enumerate(hint_cases, start=1):
        for dictionary_value in ("", "Straße → calle; STRASSE → avenida"):
            try:
                hint = stt_dictionary.transcription_hint(prompt, dictionary_value, ["OpenCode", "claude_code"])
                payload, error = {"text": hint}, None
            except Exception as caught:
                payload, error = None, str(caught)
            suffix = "vacio" if not dictionary_value else "dup"
            out.add(
                f"stt_fold/hint/{index:02d}-{suffix}",
                "transcription_hint",
                {"prompt": prompt, "dictionary": dictionary_value, "agents": ["OpenCode", "claude_code"]},
                payload,
                error,
            )


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
