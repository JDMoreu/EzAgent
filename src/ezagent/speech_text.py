"""Normalización del texto que se envía al TTS.

El LLM ya recibe instrucciones de estilo hablado, pero los modelos a veces
devuelven markdown, URLs, abreviaturas, números o símbolos. Este transform se
aplica solo al texto que va al sintetizador: la transcripción conserva el texto
original del LLM.

Kokoro pronuncia mejor las cifras y los símbolos cuando se escriben como se
leerían en voz alta ("2026" → "dos mil veintiséis", "15 €" → "quince euros").
"""

from __future__ import annotations

import re

from num2words import num2words

_ABBREVIATIONS = [
    (r"\bEE\.\s*UU\.", "Estados Unidos"),
    (r"\bSrta\.", "señorita"),
    (r"\bSra\.", "señora"),
    (r"\bSr\.", "señor"),
    (r"\bDra\.", "doctora"),
    (r"\bDr\.", "doctor"),
    (r"\bp\.\s*ej\.", "por ejemplo"),
    (r"\betc\.", "etcétera"),
    (r"\baprox\.", "aproximadamente"),
    (r"\bn\.\s*[ºo]", "número"),
    (r"\bN\.\s*[ºo]", "número"),
    (r"\bnúm\.", "número"),
    (r"\bpág\.", "página"),
    (r"\btel\.", "teléfono"),
    (r"\ba\.\s*C\.", "antes de Cristo"),
    (r"\bd\.\s*C\.", "después de Cristo"),
    (r"\bUd\.", "usted"),
    (r"\bUds\.", "ustedes"),
    (r"\bAv\.", "avenida"),
    (r"\bC/", "calle "),
]

_UNITS = [
    (r"(?<=\d)\s*km/h\b", " kilómetros por hora"),
    (r"(?<=\d)\s*m/s\b", " metros por segundo"),
    (r"(?<=\d)\s*°C\b", " grados centígrados"),
    (r"(?<=\d)\s*km\b", " kilómetros"),
    (r"(?<=\d)\s*kg\b", " kilogramos"),
    (r"(?<=\d)\s*cm\b", " centímetros"),
    (r"(?<=\d)\s*mm\b", " milímetros"),
    (r"(?<=\d)\s*ml\b", " mililitros"),
    (r"(?<=\d)\s*GB\b", " gigabytes"),
    (r"(?<=\d)\s*MB\b", " megabytes"),
    (r"(?<=\d)\s*ms\b", " milisegundos"),
    (r"(?<=\d)\s*s\b", " segundos"),
    (r"\bkm/h\b", "kilómetros por hora"),
    (r"\bkm\b", "kilómetros"),
    (r"\bkg\b", "kilogramos"),
    (r"\bcm\b", "centímetros"),
    (r"\bmm\b", "milímetros"),
    (r"\bml\b", "mililitros"),
    (r"\bGB\b", "gigabytes"),
    (r"\bMB\b", "megabytes"),
    (r"\bms\b", "milisegundos"),
]

_MONTHS = [
    "enero",
    "febrero",
    "marzo",
    "abril",
    "mayo",
    "junio",
    "julio",
    "agosto",
    "septiembre",
    "octubre",
    "noviembre",
    "diciembre",
]

_DIGITS = {
    "0": "cero",
    "1": "uno",
    "2": "dos",
    "3": "tres",
    "4": "cuatro",
    "5": "cinco",
    "6": "seis",
    "7": "siete",
    "8": "ocho",
    "9": "nueve",
}

_EMOJI = re.compile(
    "["
    "\U0001f300-\U0001faff"
    "\U00002600-\U000027bf"
    "\U0001f1e6-\U0001f1ff"
    "\U00002190-\U000021ff"
    "\U0000fe0f"
    "\U0000200d"
    "]+"
)


def _int_words(value: int) -> str:
    return num2words(value, lang="es")


def _digit_words(digits: str) -> str:
    return " ".join(_DIGITS[digit] for digit in digits)


def _fraction_words(fraction: str) -> str:
    if fraction.startswith("0"):
        return _digit_words(fraction)
    return _int_words(int(fraction))


def _cardinal(raw: str) -> str:
    negative = raw.startswith("-")
    digits = raw.lstrip("-").replace(".", "")
    if not digits.isdigit():
        return raw
    words = _digit_words(digits) if len(digits) >= 7 else _int_words(int(digits))
    return f"menos {words}" if negative else words


def _thousands(match: re.Match[str]) -> str:
    return _cardinal(match.group(0))


def _thousands_decimal(match: re.Match[str]) -> str:
    raw = match.group(0)
    negative = raw.startswith("-")
    body = raw.lstrip("-")
    integer, fraction = body.split(",", 1)
    words = f"{_int_words(int(integer.replace('.', '')))} coma {_fraction_words(fraction)}"
    return f"menos {words}" if negative else words


def _thousands_group(match: re.Match[str]) -> str:
    return _thousands_decimal(match) if "," in match.group(0) else _thousands(match)


def _decimal_comma(match: re.Match[str]) -> str:
    raw = match.group(0)
    negative = raw.startswith("-")
    body = raw.lstrip("-")
    integer, fraction = body.split(",", 1)
    words = f"{_int_words(int(integer.replace('.', '')))} coma {_fraction_words(fraction)}"
    return f"menos {words}" if negative else words


def _decimal_dot(match: re.Match[str]) -> str:
    raw = match.group(0)
    negative = raw.startswith("-")
    body = raw.lstrip("-")
    integer, fraction = body.split(".", 1)
    words = f"{_int_words(int(integer))} coma {_fraction_words(fraction)}"
    return f"menos {words}" if negative else words


def _integer(match: re.Match[str]) -> str:
    return _cardinal(match.group(0))


def _time(match: re.Match[str]) -> str:
    hour = int(match.group(1))
    minute = int(match.group(2))
    if minute == 0:
        return f"{_int_words(hour)} en punto"
    if minute == 15:
        return f"{_int_words(hour)} y cuarto"
    if minute == 30:
        return f"{_int_words(hour)} y media"
    return f"{_int_words(hour)} {_int_words(minute)}"


def _date(match: re.Match[str]) -> str:
    day = int(match.group(1))
    month = int(match.group(2))
    year = int(match.group(3))
    if not 1 <= month <= 12:
        return match.group(0)
    if year < 100:
        year += 2000
    return f"{_int_words(day)} de {_MONTHS[month - 1]} de {_int_words(year)}"


def _ordinal(match: re.Match[str]) -> str:
    number = int(match.group(1))
    suffix = match.group(2)
    word = num2words(number, lang="es", to="ordinal")
    if suffix == "ª" and word.endswith("o"):
        word = word[:-1] + "a"
    return word


def _ordinal_er(match: re.Match[str]) -> str:
    number = int(match.group(1))
    if number == 1:
        return "primer"
    return num2words(number, lang="es", to="ordinal")


def _normalize_numbers(value: str) -> str:
    value = re.sub(r"\b(\d{1,2})/(\d{1,2})/(\d{2,4})\b", _date, value)
    value = re.sub(r"\b(\d{1,2}):(\d{2})\b", _time, value)
    value = re.sub(r"\b(\d+)\s*([ºª])(?=\W|$)", _ordinal, value)
    value = re.sub(r"\b(\d+)\s*er\b", _ordinal_er, value)
    # Miles con separador de punto y, opcionalmente, decimales con coma.
    value = re.sub(
        r"(?<![\w.,])-?\d{1,3}(?:\.\d{3})+(?:,\d+)?(?![\w])",
        _thousands_group,
        value,
    )
    value = re.sub(r"(?<![\w.,])-?\d+,\d+(?![\w])", _decimal_comma, value)
    value = re.sub(r"(?<![\w.,])-?\d+\.\d{1,3}(?![\w])", _decimal_dot, value)
    value = re.sub(r"(?<![\w.,])-?\d+(?![\w])", _integer, value)
    return value


async def normalize_for_speech(text: str, _text_type: str = "sentence") -> str:
    """Limpia el texto para que el TTS lo pronuncie con naturalidad."""
    value = text.strip()
    if not value:
        return value

    # Markdown estructural y enlaces.
    value = re.sub(r"!\[([^\]]*)\]\([^)]*\)", r"\1", value)
    value = re.sub(r"\[([^\]]+)\]\([^)]*\)", r"\1", value)
    value = re.sub(r"https?://\S+", "", value)
    value = re.sub(r"^\s*#{1,6}\s*", "", value, flags=re.MULTILINE)
    value = re.sub(r"^\s*[-*•]\s+", "", value, flags=re.MULTILINE)
    value = re.sub(r"^\s*\d+[.)]\s+", "", value, flags=re.MULTILINE)
    value = value.replace("`", "").replace("**", "").replace("__", "")

    # Emojis y símbolos que no se pronuncian bien.
    value = _EMOJI.sub("", value)
    value = value.replace("&", " y ").replace("%", " por ciento")
    value = value.replace("€", " euros").replace("£", " libras")
    value = value.replace("$", " dólares")
    value = value.replace("°C", " grados centígrados").replace("°", " grados")
    value = value.replace(" y/o ", " y o ")

    for pattern, replacement in _ABBREVIATIONS:
        value = re.sub(pattern, replacement, value)
    for pattern, replacement in _UNITS:
        value = re.sub(pattern, replacement, value)

    value = _normalize_numbers(value)

    # Espacios y puntuación.
    value = re.sub(r"[ \t]+", " ", value)
    value = re.sub(r"\s+([,.;:!?])", r"\1", value)
    value = re.sub(r"\s{2,}", " ", value).strip()
    return value


async def normalize_for_kokoro(text: str, text_type: str = "sentence") -> str:
    """Normaliza y fija la pronunciación española de la marca."""
    value = await normalize_for_speech(text, text_type)
    return re.sub(r"\bEzAgent\b", "Es Agente", value, flags=re.IGNORECASE)
