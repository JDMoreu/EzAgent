"""Fixtures de `ezagent.voice.chat.trace` (F1-02).

``t`` se construye con ``time.mktime`` a partir de la hora de pared, de modo que
la fecha esperada no dependa de la zona horaria de la máquina que genera.

* ``stamp``     entrada ``{"t": float}`` -> ``{"stamp": str}``
* ``stamp_for`` entrada ``{"role": ..., "t": float, "streaming": bool}``
  (los campos ausentes se omiten) -> ``{"stamp": str}``
"""

from __future__ import annotations

import time
from datetime import datetime
from types import SimpleNamespace
from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "chat_trace"

# (year, month, day, hour, minute, second) en hora local.
WALL_CLOCKS = [
    (2026, 1, 15, 12, 0, 0),
    (2026, 2, 15, 12, 0, 0),
    (2026, 3, 15, 12, 0, 0),
    (2026, 4, 15, 12, 0, 0),
    (2026, 5, 15, 12, 0, 0),
    (2026, 6, 15, 12, 0, 0),
    (2026, 7, 15, 12, 0, 0),
    (2026, 8, 15, 12, 0, 0),
    (2026, 9, 15, 12, 0, 0),
    (2026, 10, 15, 12, 0, 0),
    (2026, 11, 13, 16, 46, 12),
    (2026, 12, 31, 23, 59, 59),
    (2026, 11, 5, 9, 5, 7),
    (2026, 11, 1, 0, 0, 0),
    (2026, 11, 30, 23, 0, 0),
    (2024, 2, 29, 12, 0, 0),
    (1999, 12, 31, 23, 59, 59),
    (1970, 1, 1, 12, 0, 0),
    (2038, 1, 19, 3, 14, 7),
    (2099, 7, 4, 6, 30, 0),
    (2000, 1, 1, 0, 0, 0),
    (2026, 11, 13, 23, 59, 59),
    (2026, 11, 13, 12, 0, 0),
]

STAMP_FOR_CASES: list[dict[str, Any]] = [
    {"role": "user", "wall": (2026, 11, 13, 16, 46, 17), "streaming": False},
    {"role": "assistant", "wall": (2026, 11, 13, 16, 46, 17), "streaming": False},
    {"role": "assistant", "wall": (2026, 11, 13, 16, 46, 17), "streaming": True},
    {"role": "user", "wall": (2026, 11, 13, 16, 46, 17), "streaming": True},
    {"role": "system", "wall": (2026, 11, 13, 16, 46, 17), "streaming": False},
    {"role": "tool", "wall": (2026, 11, 13, 16, 46, 17), "streaming": False},
    {"role": "", "wall": (2026, 11, 13, 16, 46, 17), "streaming": False},
    {"role": "developer", "wall": (2026, 11, 13, 16, 46, 17), "streaming": False},
    {"role": "usuario", "wall": (2026, 11, 13, 16, 46, 17), "streaming": False},
    {"role": "USER", "wall": (2026, 11, 13, 16, 46, 17), "streaming": False},
    {"role": "user", "wall": (2026, 1, 1, 0, 0, 0), "streaming": False},
    {"role": "user", "wall": (2026, 12, 31, 23, 59, 59), "streaming": False},
    {"role": "assistant", "wall": (2024, 2, 29, 12, 0, 0), "streaming": False},
    {"role": "assistant", "wall": (2026, 11, 5, 9, 5, 7), "streaming": False},
    {"role": "user", "wall": (2026, 11, 13, 16, 46, 17), "streaming": 0},
    {"role": "user", "wall": (2026, 11, 13, 16, 46, 17), "streaming": 1},
    {"role": "user", "wall": (2026, 11, 13, 16, 46, 17), "streaming": None},
    {"role": "user", "wall": (2026, 11, 13, 16, 46, 17)},
    {"role": "assistant", "wall": (2026, 11, 13, 16, 46, 17)},
    {"role": "user", "wall": (1970, 1, 1, 12, 0, 0), "streaming": False},
    {"role": "assistant", "wall": (2038, 1, 19, 3, 14, 7), "streaming": False},
]


def _epoch(wall: tuple[int, ...]) -> float:
    """Instante que ``time.localtime`` devuelve con esa hora de pared.

    Se busca el punto fijo con el mismo reloj que usa ``stamp`` para que la
    entrada y la salida del caso sean coherentes en cualquier zona horaria.
    """
    target = datetime(*wall)
    try:
        moment = time.mktime((*wall, 0, 0, -1))
    except (OverflowError, ValueError):
        moment = 0.0
    for _ in range(5):
        now = time.localtime(moment)[:6]
        if tuple(now) == wall:
            return moment
        moment += (target - datetime(*now)).total_seconds()
    return moment


def _message(case: dict[str, Any]) -> Any:
    fields: dict[str, Any] = {"role": case["role"]}
    if "wall" in case:
        fields["t"] = _epoch(case["wall"])
    if "streaming" in case:
        fields["streaming"] = case["streaming"]
    return SimpleNamespace(**fields)


def _add_stamp_cases(out: FixtureFile, stamp: Any) -> None:
    for index, wall in enumerate(WALL_CLOCKS, start=1):
        moment = _epoch(wall)
        out.add(
            f"chat_trace/stamp/{index:03d}",
            "stamp",
            {"t": moment},
            {"stamp": stamp(moment)},
        )
    for index, wall in enumerate([(2026, 11, 13, 16, 46, 12), (2026, 1, 1, 0, 0, 0)], start=1):
        moment = _epoch(wall)
        for suffix, value in (("frac05", moment + 0.5), ("frac99", moment + 0.99)):
            out.add(
                f"chat_trace/stamp/{index:02d}{suffix}",
                "stamp",
                {"t": value},
                {"stamp": stamp(value)},
            )
    for index, wall in enumerate([(2026, 11, 13, 16, 46, 12), (1999, 12, 31, 23, 59, 59)], start=1):
        moment = int(_epoch(wall))
        out.add(
            f"chat_trace/stamp/int{index:02d}",
            "stamp",
            {"t": moment},
            {"stamp": stamp(moment)},
        )
    for index, bad in enumerate([None, "2026-11-13T16:46:12"], start=1):
        try:
            stamp(bad)  # type: ignore[arg-type]
        except (TypeError, ValueError) as error:
            out.add_error(f"chat_trace/stamp/error{index:02d}", "stamp", {"t": bad}, str(error))


def _add_stamp_for_cases(out: FixtureFile, stamp_for: Any) -> None:
    for index, case in enumerate(STAMP_FOR_CASES, start=1):
        payload = {key: value for key, value in case.items() if key != "wall"}
        if "wall" in case:
            payload["t"] = _epoch(case["wall"])
        out.add(
            f"chat_trace/stamp_for/{index:03d}",
            "stamp_for",
            payload,
            {"stamp": stamp_for(_message(case))},
        )
    missing_t = SimpleNamespace(role="user", streaming=False)
    try:
        stamp_for(missing_t)
    except AttributeError as error:
        out.add_error(
            "chat_trace/stamp_for/error01",
            "stamp_for",
            {"role": "user", "streaming": False},
            str(error),
        )
    for index, bad in enumerate([None, "x"], start=1):
        message = SimpleNamespace(role="user", t=bad, streaming=False)
        try:
            stamp_for(message)
        except (TypeError, ValueError) as error:
            out.add_error(
                f"chat_trace/stamp_for/error{index + 1:02d}",
                "stamp_for",
                {"role": "user", "t": bad, "streaming": False},
                str(error),
            )


def generate(out: FixtureFile) -> None:
    from ezagent.voice.chat.trace import stamp, stamp_for

    _add_stamp_cases(out, stamp)
    _add_stamp_for_cases(out, stamp_for)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
