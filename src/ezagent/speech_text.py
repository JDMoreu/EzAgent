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


# --------------------------------------------------------------------------
# Dinero, tarifas, signos, rangos, fracciones y tablas: lo que más suena a robot si se lee tal cual
# --------------------------------------------------------------------------
_CURRENCIES = {
    "$": ("dólar", "dólares"),
    "US$": ("dólar", "dólares"),
    "USD": ("dólar", "dólares"),
    "€": ("euro", "euros"),
    "EUR": ("euro", "euros"),
    "£": ("libra", "libras"),
    "MXN": ("peso mexicano", "pesos mexicanos"),
    "COP": ("peso colombiano", "pesos colombianos"),
    "ARS": ("peso argentino", "pesos argentinos"),
    "CLP": ("peso chileno", "pesos chilenos"),
    "PEN": ("sol", "soles"),
    "BRL": ("real", "reales"),
}
_AMOUNT = r"\d{1,3}(?:[.,]\d{3})+(?:[.,]\d{1,2})?|\d+(?:[.,]\d{1,2})?"
_MAGNITUDE = r"(?:\s?(?:k|K|M|mil|millones|millón))?"
_CODES = "USD|EUR|MXN|COP|ARS|CLP|PEN|BRL"
_MONEY_BEFORE = re.compile(
    rf"(?P<cur>US\$|\$|€|£)\s?(?P<amt>{_AMOUNT})(?P<mag>{_MAGNITUDE})(?![\w])"
)
_MONEY_AFTER = re.compile(
    rf"(?<![\w.,])(?P<amt>{_AMOUNT})(?P<mag>{_MAGNITUDE})\s?(?P<cur>{_CODES}|US\$|\$|€|£)(?![A-Za-z])"
)
_RATES = {
    "mes": "al mes",
    "mo": "al mes",
    "month": "al mes",
    "año": "al año",
    "anio": "al año",
    "yr": "al año",
    "year": "al año",
    "día": "al día",
    "dia": "al día",
    "day": "al día",
    "semana": "a la semana",
    "week": "a la semana",
    "trimestre": "al trimestre",
    "hora": "por hora",
    "hr": "por hora",
    "minuto": "por minuto",
    "min": "por minuto",
    "segundo": "por segundo",
    "usuario": "por usuario",
    "user": "por usuario",
    "persona": "por persona",
    "licencia": "por licencia",
    "asiento": "por asiento",
    "seat": "por asiento",
    "dispositivo": "por dispositivo",
    "token": "por token",
    "tokens": "por token",
    "mensaje": "por mensaje",
    "llamada": "por llamada",
}
_RATE = re.compile(
    r"(?<=[\w%)])\s*/\s*(" + "|".join(sorted(_RATES, key=len, reverse=True)) + r")\b", re.IGNORECASE
)
_RATE_SHORT = re.compile(r"(?<=\d)\s*/\s*([hs])\b")
_PER_TIME_UNITS = {"KB": "kilobytes", "MB": "megabytes", "GB": "gigabytes", "TB": "terabytes"}
_PER_TIME = re.compile(
    r"\b(KB|MB|GB|TB|bytes?|bits?|tokens?|req|requests?|peticiones|mensajes|palabras|caracteres|"
    r"líneas|filas|frames?)\s*/\s*([sh])\b"
)
_FRACTIONS = {
    2: ("medio", "medios"),
    3: ("tercio", "tercios"),
    4: ("cuarto", "cuartos"),
    5: ("quinto", "quintos"),
    8: ("octavo", "octavos"),
    10: ("décimo", "décimos"),
}
_LETTER = "A-Za-zÁÉÍÓÚÜáéíóúüñÑ"


def _split_amount(amount: str) -> tuple[str, str | None]:
    """``("1200", "50")`` para «1.200,50» o «1,200.50»; ``("1200", None)`` para «1,200»."""
    if "." in amount and "," in amount:
        decimal = "." if amount.rfind(".") > amount.rfind(",") else ","
    elif "." in amount or "," in amount:
        sep = "." if "." in amount else ","
        parts = amount.split(sep)
        decimal = sep if len(parts) == 2 and len(parts[1]) in (1, 2) else ""
    else:
        decimal = ""
    if decimal:
        whole, frac = amount.rsplit(decimal, 1)
        return re.sub(r"[.,]", "", whole), frac
    return re.sub(r"[.,]", "", amount), None


def _apocope(words: str) -> str:
    """«uno» antes de un sustantivo es «un» («veintiuno» → «veintiún»): un dólar, veintiún euros."""
    if words.endswith("veintiuno"):
        return words[: -len("veintiuno")] + "veintiún"
    return words[:-1] if words.endswith(" uno") or words == "uno" else words


def _money_words(amount: str, magnitude: str, symbol: str) -> str:
    singular, plural = _CURRENCIES[symbol.upper() if symbol.isalpha() else symbol]
    whole, cents = _split_amount(amount)
    mag = magnitude.strip().lower()
    number = int(whole) if whole.isdigit() else None
    words = _apocope(_int_words(number)) if number is not None else whole
    if mag and cents and number is not None:  # «2.5M»: los decimales son parte de la cifra
        words = f"{_int_words(number)} coma {_fraction_words(cents)}"
        number, cents = -1, None
    if mag in ("k", "mil"):
        text = f"{words if number != 1 else ''} mil {plural}".replace("  ", " ").strip()
    elif mag in ("m", "millones", "millón"):
        text = f"{'un millón' if number == 1 else words + ' millones'} de {plural}"
    else:
        name = singular if (number == 1 and not cents) else plural
        text = f"{words} {name}"
    if cents:
        cents = (cents + "0")[:2]
        if cents != "00":
            text += f" con {cents}"
    return text


def _normalize_money(value: str) -> str:
    value = _MONEY_BEFORE.sub(lambda m: _money_words(m["amt"], m["mag"], m["cur"]), value)
    return _MONEY_AFTER.sub(lambda m: _money_words(m["amt"], m["mag"], m["cur"]), value)


def _normalize_rates(value: str) -> str:
    """«/mes» se dice «al mes»: la barra nunca se lee."""
    value = _RATE.sub(lambda m: " " + _RATES[m.group(1).lower()], value)
    value = _PER_TIME.sub(
        lambda m: (
            f"{_PER_TIME_UNITS.get(m.group(1), m.group(1))} por "
            + ("hora" if m.group(2) == "h" else "segundo")
        ),
        value,
    )
    return _RATE_SHORT.sub(lambda m: " por hora" if m.group(1) == "h" else " por segundo", value)


def _fraction(match: re.Match[str]) -> str:
    num, den = int(match.group(1)), int(match.group(2))
    if (num, den) == (24, 7):
        return "todos los días, a todas horas"
    if den == 0:
        return match.group(0)
    names = _FRACTIONS.get(den)
    if names and 0 < num < den:
        word = names[0] if num == 1 else names[1]
        return f"{'un' if num == 1 else _int_words(num)} {word}"
    return f"{match.group(1)} entre {match.group(2)}"


def _iso_date(match: re.Match[str]) -> str:
    year, month, day = (int(match.group(i)) for i in (1, 2, 3))
    if not (1 <= month <= 12 and 1 <= day <= 31):
        return match.group(0)
    return f"{_int_words(day)} de {_MONTHS[month - 1]} de {_int_words(year)}"


def _normalize_symbols(value: str) -> str:
    """Signos, rangos, fracciones y símbolos sueltos que sonarían a robot."""
    value = value.replace("\u2212", "-").replace("\u2013", "-").replace("\u2014", " - ")
    value = re.sub(r"\b(\d{4})-(\d{2})-(\d{2})\b", _iso_date, value)
    value = re.sub(
        r"(?<![\w.,/-])(?:\d{3}-\d{4}|\d{2,4}(?:-\d{2,4}){2,3})(?![\w/-])", _phone, value
    )
    value = re.sub(
        r"(?<![\w.,/-])(\d+(?:[.,]\d+)?)\s*-\s*(\d+(?:[.,]\d+)?)(?![\w/-])", _range, value
    )
    value = re.sub(r"(?<![\w+])\+(?=\d)", "más ", value)
    value = re.sub(r"(?<![\w/\d])(\d{1,3})/(\d{1,3})(?![\w/])", _fraction, value)
    value = re.sub(r"\by\s?/\s?o\b", "y o", value, flags=re.IGNORECASE)
    value = re.sub(rf"(?<=[{_LETTER}])\s?/\s?(?=[{_LETTER}])", " o ", value)
    value = re.sub(r"\s-\s", ", ", value)
    for symbol, words in (
        ("×", " por "),
        ("≈", " aproximadamente "),
        ("≥", " mayor o igual que "),
        ("≤", " menor o igual que "),
        ("…", "..."),
        (" = ", " igual a "),
        ("~~", ""),
    ):
        value = value.replace(symbol, words)
    value = re.sub(r"~(?=\d)", "aproximadamente ", value)
    value = re.sub(r"#(?=\d)", "número ", value)
    value = re.sub(r"\b(\d+(?:[.,]\d+)?)\s?[kK]\b", r"\1 mil", value)
    value = re.sub(r"(?<![\w*])\*{1,2}(\S[^*\n]*?)\*{1,2}(?![\w*])", r"\1", value)
    return value.replace("*", " ")


def _phone(match: re.Match[str]) -> str:
    """555-1234 → «cinco cinco cinco, uno dos tres cuatro»: un teléfono se dice cifra a cifra."""
    return ", ".join(_digit_words(group) for group in match.group(0).split("-"))


def _range(match: re.Match[str]) -> str:
    left, right = match.group(1), match.group(2)
    if len(left) == 3 and len(right) == 4 and left.isdigit() and right.isdigit():
        return match.group(0)  # 555-1234 es un teléfono, no un rango
    return f"{left} a {right}"


def _flatten_tables(value: str) -> str:
    """Una tabla de markdown se dice fila por fila: «Norte, 120, 142», no «barra Norte barra…»."""
    lines: list[str] = []
    for line in value.split("\n"):
        if line.count("|") >= 2:
            if re.fullmatch(r"\s*\|?[\s:|-]+\|?\s*", line) and "-" in line:
                continue
            cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
            text = ", ".join(cell for cell in cells if cell)
            lines.append(text if text.endswith((".", "!", "?")) else text + ".")
        else:
            lines.append(line)
    return "\n".join(lines)


async def normalize_for_speech(text: str, _text_type: str = "sentence") -> str:
    """Limpia el texto para que el TTS lo pronuncie con naturalidad."""
    value = text.strip()
    if not value:
        return value

    # Markdown estructural y enlaces.
    value = re.sub(r"!\[([^\]]*)\]\([^)]*\)", r"\1", value)
    value = re.sub(r"\[([^\]]+)\]\([^)]*\)", r"\1", value)
    value = re.sub(r"https?://\S+", "", value)
    value = _flatten_tables(value)
    value = re.sub(r"^\s*#{1,6}\s*", "", value, flags=re.MULTILINE)
    value = re.sub(r"^\s*[-*•]\s+", "", value, flags=re.MULTILINE)
    value = re.sub(r"^\s*\d+[.)]\s+", "", value, flags=re.MULTILINE)
    value = value.replace("`", "").replace("**", "").replace("__", "")

    # Emojis y símbolos que no se pronuncian bien.
    value = _EMOJI.sub("", value)
    # El dinero y las tarifas van antes que el resto de símbolos: «$20/mes» → «20 dólares al mes».
    value = _normalize_money(value)
    value = _normalize_rates(value)
    value = value.replace("&", " y ").replace("%", " por ciento")
    value = value.replace("€", " euros").replace("£", " libras")
    value = value.replace("$", " dólares")
    value = value.replace("°C", " grados centígrados").replace("°", " grados")
    value = value.replace(" y/o ", " y o ")

    for pattern, replacement in _ABBREVIATIONS:
        value = re.sub(pattern, replacement, value)
    for pattern, replacement in _UNITS:
        value = re.sub(pattern, replacement, value)

    value = _normalize_symbols(value)
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
