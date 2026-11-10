"""Voz natural: precios, tarifas, tablas, signos y rangos se dicen como una persona los diría."""

from __future__ import annotations

import pytest

from ezagent.speech_text import normalize_for_speech


async def say(text: str) -> str:
    return await normalize_for_speech(text)


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        # el caso reportado: «20 barra inclinada a mes»
        ("Cuesta $20/mes.", "Cuesta veinte dólares al mes."),
        ("El plan anual: $200/año", "El plan anual: doscientos dólares al año"),
        ("Son US$ 15 por usuario/mes", "Son quince dólares por usuario al mes"),
        ("20 USD/mes", "veinte dólares al mes"),
        ("€9.99/mes", "nueve euros con noventa y nueve al mes"),
        ("£5 a la semana", "cinco libras a la semana"),
        ("$3/día y $2/hora", "tres dólares al día y dos dólares por hora"),
        ("$12 por usuario/mes", "doce dólares por usuario al mes"),
    ],
)
async def test_prices_and_rates_are_spoken_not_spelled(raw: str, expected: str) -> None:
    assert await say(raw) == expected


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        ("$1", "un dólar"),
        ("$21", "veintiún dólares"),
        ("$31 y $101", "treinta y un dólares y ciento un dólares"),
        ("1 USD", "un dólar"),
        ("$1,200", "mil doscientos dólares"),
        ("$1.200", "mil doscientos dólares"),
        ("$1,200.50", "mil doscientos dólares con cincuenta"),
        ("$1.200,50", "mil doscientos dólares con cincuenta"),
        ("$20.5", "veinte dólares con cincuenta"),
        ("$20.00", "veinte dólares"),
        ("$1M", "un millón de dólares"),
        ("$2M", "dos millones de dólares"),
        ("€2.5M", "dos coma cinco millones de euros"),
        ("$10k", "diez mil dólares"),
        ("$1k", "mil dólares"),
        ("500 MXN", "quinientos pesos mexicanos"),
    ],
)
async def test_amounts_agree_in_number_and_read_separators_correctly(
    raw: str, expected: str
) -> None:
    assert await say(raw) == expected


async def test_a_markdown_table_is_read_row_by_row() -> None:
    table = "| Plan | Precio | Notas |\n|:--|--:|:-:|\n| Básico | $5/mes | **ok** |\n| Pro | $12/mes | +18 % |"
    spoken = await say(table)
    assert spoken == (
        "Plan, Precio, Notas.\n"
        "Básico, cinco dólares al mes, ok.\n"
        "Pro, doce dólares al mes, más dieciocho por ciento."
    )
    assert "|" not in spoken and "-" not in spoken and "barra" not in spoken


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        ("subió +18 %", "subió más dieciocho por ciento"),
        ("bajó −6 %", "bajó menos seis por ciento"),
        ("entre 10-20 dólares", "entre diez a veinte dólares"),
        ("de 2020–2024", "de dos mil veinte a dos mil veinticuatro"),
        ("Rinde ~20 %", "Rinde aproximadamente veinte por ciento"),
        ("nota #3", "nota número tres"),
        ("a × b", "a por b"),
        ("x ≥ y", "x mayor o igual que y"),
    ],
)
async def test_signs_ranges_and_symbols(raw: str, expected: str) -> None:
    assert await say(raw) == expected


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        ("Llama al 555-1234", "Llama al cinco cinco cinco, uno dos tres cuatro"),
        ("55-1234-5678", "cinco cinco, uno dos tres cuatro, cinco seis siete ocho"),
        ("2026-11-10", "diez de noviembre de dos mil veintiséis"),
        ("10/11/2026", "diez de noviembre de dos mil veintiséis"),
        ("abierto 24/7", "abierto todos los días, a todas horas"),
        ("1/2 y 3/4 y 5/8", "un medio y tres cuartos y cinco octavos"),
        ("7/9", "siete entre nueve"),
    ],
)
async def test_phones_dates_and_fractions(raw: str, expected: str) -> None:
    assert await say(raw) == expected


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        ("120 km/h", "ciento veinte kilómetros por hora"),
        ("50 MB/s", "cincuenta megabytes por segundo"),
        ("5 tokens/s", "cinco tokens por segundo"),
        ("20 peticiones/h", "veinte peticiones por hora"),
        ("3/s", "tres por segundo"),
        ("entrada/salida", "entrada o salida"),
        ("Windows/Linux", "Windows o Linux"),
        ("y/o", "y o"),
    ],
)
async def test_units_and_slashes_between_words(raw: str, expected: str) -> None:
    assert await say(raw) == expected


async def test_no_slash_or_pipe_or_currency_symbol_survives_in_a_realistic_answer() -> None:
    answer = (
        "Resumen de precios:\n\n| Plan | Mensual | Anual |\n|---|--:|--:|\n"
        "| Free | $0/mes | $0/año |\n| Pro | $20/mes | $200/año |\n| Equipo | $15/usuario/mes | $150/usuario/año |\n\n"
        "Pro sube +25 % respecto a 2025 (de $16 a $20) y el soporte es 24/7."
    )
    spoken = await say(answer)
    for forbidden in ("/", "|", "$", "%", "+", "**", "  "):
        assert forbidden not in spoken, (forbidden, spoken)
    for expected in (
        "veinte dólares al mes",
        "doscientos dólares al año",
        "quince dólares por usuario al mes",
    ):
        assert expected in spoken


@pytest.mark.parametrize(
    "plain",
    [
        "Hola, ¿cómo estás?",
        "Son las tres y media.",
        "Mañana a las 10:30 nos vemos.",
        "No hay nada que normalizar aquí.",
        "El archivo se llama resumen final.",
    ],
)
async def test_ordinary_sentences_pass_through_untouched_except_numbers(plain: str) -> None:
    spoken = await say(plain)
    assert "/" not in spoken and "|" not in spoken
    assert spoken.startswith(plain.split()[0])


async def test_text_without_symbols_is_not_mangled() -> None:
    assert await say("Gracias por esperar, ya terminé.") == "Gracias por esperar, ya terminé."
    assert await say("") == ""
    assert await say("   ") == ""
