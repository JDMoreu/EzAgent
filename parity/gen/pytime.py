"""Fixtures de fechas (dominio `pytime`, F2-01).

Fija el texto exacto que produce Python para `datetime.fromtimestamp(...)`,
incluido el acarreo de microsegundos y el fallo con instantes negativos en
Windows (que el port debe reproducir, no corregir).
"""

from __future__ import annotations

import datetime

from _common import FixtureFile, main_guard

DOMAIN = "pytime"

POSITIVE = [
    0.0,
    0.5,
    1.0,
    0.000001,
    0.9999995,
    0.9999999,
    2.5,
    1735689600.0,
    1735689600.5,
    1735689600.123456,
    1735689600.000001,
    1735689600.9999995,
    1234567890.987654,
    1000000000.0000005,
    4102444800.0,
]

NEGATIVE = [-1.0, -0.5, -1735689600.0]


def _call(fn, value):
    try:
        return fn(value), None
    except Exception as error:  # Windows rechaza instantes negativos
        return None, f"{type(error).__name__}: {error}"


def generate(out: FixtureFile) -> None:
    functions = {
        "isoformat_local": lambda t: datetime.datetime.fromtimestamp(t).isoformat(),
        "isoformat_utc": lambda t: datetime.datetime.fromtimestamp(
            t, datetime.timezone.utc
        ).isoformat(),
        "isoformat_millis_utc": lambda t: datetime.datetime.fromtimestamp(
            t, datetime.timezone.utc
        ).isoformat(timespec="milliseconds"),
        "date_local": lambda t: datetime.datetime.fromtimestamp(t).date().isoformat(),
    }

    for name, fn in functions.items():
        for index, value in enumerate(POSITIVE, start=1):
            result, error = _call(fn, value)
            out.add(f"pytime/{name}/{index:03d}", name, {"t": value}, result, error)
        for index, value in enumerate(NEGATIVE, start=1):
            result, error = _call(fn, value)
            out.add(f"pytime/{name}/neg{index:02d}", name, {"t": value}, result, error)

    # `time.time()` y su escritura en JSON se comparan por forma, no por valor.
    out.add(
        "pytime/timestamp/001",
        "timestamp_shape",
        {},
        {"finite": True, "roughly_now": True},
        None,
    )


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
