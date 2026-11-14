"""Fixtures de `ezagent.voice.chat.speech` (F1-02).

La respuesta del chat convertida a voz: frases, tope de caracteres, listas y el
aviso de que el detalle está en pantalla.

* ``spoken_version`` entrada ``{"markdown": str, "max_chars": int}`` ->
  ``{"text": str}``
"""

from __future__ import annotations

from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "chat_speech"

SENTENCES = ". ".join(f"Frase número {number}" for number in range(1, 41)) + "."
LONG_PARAGRAPH = " ".join(f"palabra{number}" for number in range(1, 200))

CASES: list[tuple[str, int]] = [
    ("", 460),
    ("   ", 460),
    ("\n\n", 460),
    ("hola mundo", 460),
    ("  hola  ", 460),
    ("# Título", 460),
    ("## Título\n\nUn párrafo.", 460),
    ("una frase. Otra frase.", 460),
    ("> una cita\n> larga", 460),
    ("- uno\n- dos\n- tres", 460),
    ("- uno\n- dos\n- tres\n- cuatro\n- cinco", 460),
    ("- uno\n- dos\n- tres\n- cuatro\n- cinco\n- seis", 460),
    ("1. primero\n2. segundo", 460),
    ("- [x] hecho\n- [ ] falta", 460),
    ("```\nprint('hola')\n```", 460),
    ("Antes\n\n```py\nx = 1\n```", 460),
    ("| a | b |\n|---|---|\n| 1 | 2 |", 460),
    ("![foto](C:\\a.png)", 460),
    ("[Informe](C:\\a.pdf)", 460),
    ("---", 460),
    ("---\n\ntexto", 460),
    ("mira [la guía](https://ejemplo.com) ya", 460),
    ("**negrita** y *cursiva* y `código`", 460),
    ("ñandú ☕ 🙂 y 中文", 460),
    ("Listo:", 460),
    ("¿Vamos?", 460),
    ("Espera…", 460),
    ("primera línea\nsegunda línea", 460),
    (SENTENCES, 460),
    (SENTENCES, 100),
    (SENTENCES, 40),
    (LONG_PARAGRAPH, 50),
    (LONG_PARAGRAPH, 460),
    ("hola.", 10),
    ("hola.", 0),
    ("hola.", 1),
    ("uno. dos. tres. cuatro.", 10),
    ("uno. dos. tres. cuatro.", 5),
    ("- a\n- b\n- c\n- d\n- e\n- f", 20),
    ("# T\n\ntexto\n\n- a\n- b\n\n```\ncódigo\n```\n\n| x | y |\n|---|---|\n| 1 | 2 |", 460),
    ("<b>hola</b> y <script>alert(1)</script>", 460),
    ("a\r\nb\rc", 460),
    ("", 0),
]

ERRORS: list[dict[str, Any]] = [
    {"markdown": "hola", "max_chars": None},
    {"markdown": "hola", "max_chars": "10"},
]


def generate(out: FixtureFile) -> None:
    from ezagent.voice.chat.speech import spoken_version

    for index, (markdown, max_chars) in enumerate(CASES, start=1):
        out.add(
            f"chat_speech/spoken_version/{index:03d}",
            "spoken_version",
            {"markdown": markdown, "max_chars": max_chars},
            {"text": spoken_version(markdown, max_chars)},
        )

    for index, case in enumerate(ERRORS, start=1):
        try:
            spoken_version(case["markdown"], case["max_chars"])
        except TypeError as error:
            out.add_error(
                f"chat_speech/spoken_version/error{index:02d}",
                "spoken_version",
                case,
                str(error),
            )


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
