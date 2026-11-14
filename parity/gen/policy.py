"""Fixtures de la política de permisos (dominio `policy`, F1-03).

`decide` es total: para cualquier combinación de argumentos devuelve una
`Decision`. La matriz cubre todos los niveles de herramienta, el taint del
turno, el modo de confirmación y los umbrales de operación masiva, más los
bordes del umbral (`bulk == bulk_threshold` no es masivo).
"""

from __future__ import annotations

from dataclasses import asdict
from typing import Any

from _common import FixtureFile, main_guard

from ezagent.agent.policy import decide

DOMAIN = "policy"

# Valores de `bulk` alrededor del umbral de fábrica (5): por debajo, justo en
# el umbral y por encima.
BULK_VALUES: tuple[int, ...] = (0, 5, 6)


def _run(payload: dict[str, Any]) -> dict[str, Any]:
    return asdict(decide(**payload))


def _payload(
    tool_level: int,
    tainted: bool,
    bulk: int,
    always_confirm: bool,
    confirm_nondestructive: bool,
    bulk_threshold: int = 5,
) -> dict[str, Any]:
    return {
        "tool_level": tool_level,
        "tainted": tainted,
        "bulk": bulk,
        "always_confirm": always_confirm,
        "confirm_nondestructive": confirm_nondestructive,
        "bulk_threshold": bulk_threshold,
    }


def generate(out: FixtureFile) -> None:
    index = 0
    for tool_level in range(0, 5):
        for tainted in (False, True):
            for bulk in BULK_VALUES:
                for always_confirm in (False, True):
                    for confirm_nondestructive in (False, True):
                        index += 1
                        payload = _payload(
                            tool_level,
                            tainted,
                            bulk,
                            always_confirm,
                            confirm_nondestructive,
                        )
                        out.add(
                            f"policy/decide/{index:04d}",
                            "decide",
                            payload,
                            _run(payload),
                        )

    # Umbral explícito: mismo `bulk` con umbrales distintos cambia la decisión.
    for threshold in (0, 1, 5, 10):
        for bulk in (0, 1, 5, 6, 11):
            index += 1
            payload = _payload(1, False, bulk, True, False, threshold)
            out.add(f"policy/decide/{index:04d}", "decide", payload, _run(payload))

    # Valores fuera de rango y negativos: la función no valida, solo decide.
    for tool_level, bulk in ((-1, -1), (0, 10**6), (2, -3), (3, 0)):
        index += 1
        payload = _payload(tool_level, True, bulk, False, True)
        out.add(f"policy/decide/{index:04d}", "decide", payload, _run(payload))


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
