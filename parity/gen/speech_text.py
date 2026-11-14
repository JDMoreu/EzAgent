"""Fixtures de normalización de texto para TTS (dominio `speech_text`, F1-02).

`normalize_for_speech` y `normalize_for_kokoro` son asíncronas y totales para
cualquier `str`: el único error posible es recibir algo que no sea texto, y ese
caso queda fijado al final del archivo.
"""

from __future__ import annotations

import asyncio
from collections.abc import Callable, Coroutine
from typing import Any

from _common import FixtureFile, main_guard

from ezagent.speech_text import normalize_for_kokoro, normalize_for_speech

DOMAIN = "speech_text"

# Un solo corpus: cada texto se pasa por las dos funciones públicas.
CORPUS: list[str] = [
    # --- básicos y espacios -------------------------------------------------
    "",
    "   ",
    "\n\t ",
    "Hola mundo",
    "Texto con  dos  espacios",
    "  espacios  alrededor  ",
    "Hola\n\n\nmundo",
    "a  ,  b",
    "punto  .  final  .",
    "Listo.",
    # --- números y cardinales ----------------------------------------------
    "Tenemos 3 tazas y 42 platos",
    "0",
    "-7",
    "7",
    "21",
    "31",
    "100",
    "101",
    "1000",
    "1999",
    "2000",
    "2026",
    "1000000",
    "1000000000",
    "1234567890",
    "123456789",
    "-1234567890",
    "12.345.678",
    "1.234",
    "3.141",
    "3.14159",
    "3,14159",
    "0,5",
    "-0,25",
    "12,50",
    "1.234,56",
    "-1.234,56",
    "1,234.56",
    "2,5",
    "2.5",
    "10.",
    "1..2",
    "1e5",
    "0,0001",
    "999999999999",
    "El total fue 1.250 unidades y 3,75 de media",
    # --- dinero -------------------------------------------------------------
    "$20",
    "20 $",
    "$0",
    "$1",
    "$21",
    "$31",
    "$1000000",
    "$20/mes",
    "€15",
    "15 €",
    "15,50 €",
    "€0,01",
    "€1.234.567,89",
    "US$ 1.000",
    "2.5M USD",
    "$1k",
    "$2,5k",
    "€3.5M",
    "£30",
    "1200 MXN",
    "35 COP",
    "500 ARS",
    "3 CLP",
    "10 PEN",
    "7 BRL",
    "1 millón de euros",
    "un millón de dólares",
    "Cuesta 25 USD/año",
    "Sale a 25 €/hora",
    "Paga 20 $ por usuario",
    "Son 19,99 € más IVA",
    # --- horas y fechas -----------------------------------------------------
    "9:00",
    "15:30",
    "23:15",
    "0:05",
    "12:45",
    "a las 7:30 de la mañana",
    "18:00 h",
    "1/1/2026",
    "31/12/1999",
    "5/6/07",
    "2026-11-13",
    "13-11-2026",
    "32/13/2026",
    "2026-13-01",
    "Reunión el 3/4/2026 a las 9:15",
    # --- ordinales ----------------------------------------------------------
    "1º",
    "2ª",
    "3er",
    "10º",
    "21er",
    "1ª vez y 2º intento",
    "el 3º y el 4ª",
    # --- unidades -----------------------------------------------------------
    "5 km",
    "120 km/h",
    "30 °C",
    "-5°C",
    "80 kg",
    "15 cm",
    "3 mm",
    "250 ml",
    "8 GB",
    "512 MB",
    "300 ms",
    "2 s",
    "10km",
    "5 Kg",
    "2 h",
    "Recorrimos 12 km en 2 h a 6 km/h",
    # --- abreviaturas -------------------------------------------------------
    "El Sr. Pérez y la Sra. Gómez",
    "La Dra. Ruiz y el Dr. House",
    "EE. UU.",
    "p. ej. esto",
    "etc.",
    "aprox. 20",
    "n.º 5",
    "N.º 7",
    "núm. 3",
    "pág. 12",
    "tel. 555-1234",
    "a. C.",
    "d. C.",
    "Ud. puede",
    "Uds. pueden",
    "Av. Reforma",
    "C/ Mayor 5",
    "Sra. y Srta. llegaron",
    # --- URLs, markdown y tablas -------------------------------------------
    "https://example.com/ruta?x=1",
    "Mira https://ezagent.dev y dime",
    "[enlace](https://example.com)",
    "![imagen](foto.png)",
    "**negrita** y __subrayado__",
    "`código`",
    "# Título",
    "## Subtítulo",
    "1. uno\n2. dos",
    "- item\n- otro",
    "• viñeta",
    "| Norte | 120 | 142 |",
    "| a | b |\n|---|---|\n| 1 | 2 |",
    "| x | y |\n| :-: | --: |\n| 1 | 2 |\nfin",
    # --- emoji y unicode ----------------------------------------------------
    "Listo 🙂",
    "🚀 despegue",
    "Hecho ✅ y 👍",
    "Familia 👨‍👩‍👧",
    "Bandera 🇲🇽",
    "Corazón ❤️",
    "Flecha → 5",
    "emoji 🙂🚀 entre palabras",
    "cafe\u0301 con leche",
    "A\u0303",
    "ñandú en el Ñire",
    "canción de José Ángel",
    "¿Qué? ¡Sí!",
    "«comillas» y “otras”",
    "İstanbul",
    # --- símbolos -----------------------------------------------------------
    "5 × 3",
    "≈ 10",
    "≥ 5",
    "≤ 3",
    "…",
    "a = b",
    "~~tachado~~",
    "~50",
    "#12",
    "10k",
    "*uno*",
    "* dos *",
    "a*b",
    "50%",
    "A & B",
    "y/o",
    "sí/no",
    "él/ella",
    "opción A/B/C",
    "1/2",
    "3/4",
    "2/3",
    "1/8",
    "9/10",
    "1/0",
    "24/7",
    "5/7",
    "10/100",
    "3-4-5",
    "10-20",
    "1.5-2.5",
    "2020-2024",
    "+34 600 123 456",
    "+5 grados",
    "-5 grados",
    "un 20-30% más",
    "555-1234",
    "llama al 555-1234 ya",
    "plan 2 meses - sin costo",
    # --- tarifas por tiempo -------------------------------------------------
    "10 €/mes",
    "5 GB/s",
    "100 req/s",
    "20 tokens/s",
    "50 palabras/min",
    "3/s",
    "2/h",
    "1,5 GB/s",
    "1000 peticiones/h",
    "$20 per user",
    # --- texto realista -----------------------------------------------------
    "El plan cuesta 19,99 €/mes y el anual 199 €/año.",
    "Quedan 3 días: 2/3 del equipo",
    "El Sr. López pidió 2 kg de café por 25,50 € y dejó propina.",
    "EzAgent está listo",
    "ezagent funciona",
    "EZAGENT",
    "Escríbeme a https://ezagent.dev o al tel. 555-1234.",
    "Resumen: 1.200 usuarios, 98,5% de disponibilidad y 3 h de soporte.",
    "Del 1/3/2026 al 31/5/2026: 90 días, aprox. 12 semanas.",
    "Compra 1º: 2 GB x $15; compra 2ª: 500 MB x $5.",
    "El 45% de los 1.000 encuestados prefiere la opción A/B.",
]


def _run(fn: Callable[[str], Coroutine[Any, Any, str]], text: str, text_type: str | None) -> str:
    if text_type is None:
        return asyncio.run(fn(text))
    return asyncio.run(fn(text, text_type))


def _error_of(fn: Callable[..., Coroutine[Any, Any, str]], value: object) -> str:
    try:
        asyncio.run(fn(value))  # type: ignore[arg-type]
    except Exception as error:  # el oráculo es el mensaje real de Python
        return str(error)
    raise AssertionError(f"{fn.__name__} no falló con {value!r}")


FUNCTIONS: list[tuple[str, Callable[..., Coroutine[Any, Any, str]]]] = [
    ("normalize_for_speech", normalize_for_speech),
    ("normalize_for_kokoro", normalize_for_kokoro),
]


def generate(out: FixtureFile) -> None:
    for fn_name, fn in FUNCTIONS:
        for index, text in enumerate(CORPUS, start=1):
            case = f"speech_text/{fn_name}/{index:04d}"
            out.add(case, fn_name, {"text": text}, {"text": _run(fn, text, None)})
        for index, text_type in enumerate(("sentence", "word", "paragraph"), start=1):
            text = "El Sr. Pérez pagó $20/mes."
            case = f"speech_text/{fn_name}/type{index:02d}"
            out.add(
                case,
                fn_name,
                {"text": text, "text_type": text_type},
                {"text": _run(fn, text, text_type)},
            )
        out.add_error(
            f"speech_text/{fn_name}/error01",
            fn_name,
            {"text": None},
            _error_of(fn, None),
        )


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
