"""Trazabilidad del chat: fecha y hora de cada mensaje.

Pure helpers (no UI). A reply's time is the moment it was complete, which is
also the time stored in the history, so a reopened conversation shows the same
times it showed live.
"""

from __future__ import annotations

import time
from typing import Any

_MONTHS = ("ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic")


def stamp(t: float) -> str:
    """«13 nov 2026 · 16:46:12» en la hora local."""
    lt = time.localtime(t)
    return (
        f"{lt.tm_mday} {_MONTHS[lt.tm_mon - 1]} {lt.tm_year} · "
        f"{lt.tm_hour:02d}:{lt.tm_min:02d}:{lt.tm_sec:02d}"
    )


def stamp_for(message: Any) -> str:
    """Fecha y hora de un mensaje del usuario o de EzAgent; "" mientras se escribe."""
    if getattr(message, "role", "") not in ("user", "assistant"):
        return ""
    if getattr(message, "streaming", False):
        return ""
    return stamp(float(message.t))


__all__ = ["stamp", "stamp_for"]
