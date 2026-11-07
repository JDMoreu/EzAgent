"""Números hablados en la confirmación fuerte."""

from __future__ import annotations

import threading
import time

import pytest

from ezagent.agent.confirm import ConfirmationManager


def _ask_with(text: str, number: int) -> bool:
    manager = ConfirmationManager(
        accept=["sí", "si", "confirmo", "adelante", "dale"],
        reject=["no", "cancela", "para"],
        timeout_s=2.0,
    )
    result: dict[str, bool] = {}
    thread = threading.Thread(
        target=lambda: result.setdefault(
            "value", manager.ask("¿Confirmas?", strong=True, require_number=number)
        )
    )
    thread.start()
    for _ in range(100):
        if manager.pending:
            break
        time.sleep(0.01)
    assert manager.pending
    manager.submit_text(text)
    thread.join(timeout=1.0)
    return bool(result.get("value"))


@pytest.mark.parametrize(
    ("text", "number"),
    [
        ("confirmo doce", 12),
        ("confirmo treinta y dos", 32),
        ("confirmo ciento veinte", 120),
        ("confirmo mil doscientos", 1200),
        ("confirmo 7", 7),
    ],
)
def test_acepta_el_numero_hablado(text: str, number: int):
    assert _ask_with(text, number) is True


def test_un_numero_distinto_rechaza():
    assert _ask_with("confirmo doce", 11) is False
