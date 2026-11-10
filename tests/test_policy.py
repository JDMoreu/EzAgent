"""decide(): tabla de verdad completa de la política de permisos."""

from __future__ import annotations

import itertools

import pytest

from ezagent.agent.policy import Decision, decide


def oracle(level, tainted, bulk, always, confirm_nd, threshold=5) -> str:
    """Reimplementación independiente de la sección 9.2 del contrato."""
    if level >= 3:
        return "deny"
    if always:
        return "confirm_strong" if bulk > threshold else "confirm"
    if bulk > threshold:
        return "confirm_strong"
    if tainted and level >= 1:
        return "confirm"
    if level == 2 and confirm_nd:
        return "confirm"
    return "allow"


def test_coincide_con_el_oraculo_en_todo_el_espacio_de_entradas() -> None:
    combos = itertools.product(
        range(0, 5), (False, True), (0, 1, 5, 6, 50), (False, True), (False, True)
    )
    count = 0
    for level, tainted, bulk, always, confirm_nd in combos:
        got = decide(level, tainted, bulk, always, confirm_nd)
        assert got.action == oracle(level, tainted, bulk, always, confirm_nd), (
            level,
            tainted,
            bulk,
            always,
            confirm_nd,
        )
        count += 1
    assert count == 5 * 2 * 5 * 2 * 2


@pytest.mark.parametrize("level", [3, 4, 99])
def test_nivel_3_o_mas_siempre_se_deniega_pase_lo_que_pase(level: int) -> None:
    for tainted, bulk, always, confirm_nd in itertools.product(
        (False, True), (0, 100), (False, True), (False, True)
    ):
        assert decide(level, tainted, bulk, always, confirm_nd).action == "deny"


def test_el_borrado_nunca_se_permite_sin_confirmar() -> None:
    for level, tainted, bulk, confirm_nd in itertools.product(
        (0, 1, 2), (False, True), (0, 1, 5, 6, 1000), (False, True)
    ):
        action = decide(level, tainted, bulk, True, confirm_nd).action
        assert action in {"confirm", "confirm_strong"}, (level, tainted, bulk)


def test_el_umbral_masivo_es_estrictamente_mayor() -> None:
    assert decide(1, False, 5, False, False).action == "allow"
    assert decide(1, False, 6, False, False).action == "confirm_strong"
    assert decide(1, False, 3, False, False, bulk_threshold=2).action == "confirm_strong"
    assert decide(1, False, 2, False, False, bulk_threshold=2).action == "allow"
    # Con umbral 0 cualquier operación con elementos pasa a confirmación fuerte.
    assert decide(0, False, 1, False, False, bulk_threshold=0).action == "confirm_strong"


def test_contenido_no_confiable_solo_endurece_las_acciones_con_efecto() -> None:
    assert decide(0, True, 0, False, False).action == "allow"  # lectura pura
    assert decide(1, True, 0, False, False).action == "confirm"
    assert decide(2, True, 0, False, False).action == "confirm"


def test_nivel_2_solo_pide_confirmacion_si_el_usuario_lo_activo() -> None:
    assert decide(2, False, 0, False, False).action == "allow"
    assert decide(2, False, 0, False, True).action == "confirm"
    # El interruptor no afecta a niveles menores.
    assert decide(1, False, 0, False, True).action == "allow"


def test_el_nivel_devuelto_es_el_de_la_herramienta_o_2_si_confirma() -> None:
    assert decide(0, False, 0, False, False) == Decision("allow", 0, "Permitido")
    assert decide(1, True, 0, False, False).level == 1
    assert decide(0, False, 99, False, False).level == 2
    assert decide(1, False, 0, True, False).level == 2
    assert decide(3, False, 0, False, False).level == 3


def test_toda_decision_explica_su_motivo() -> None:
    for args in itertools.product(range(0, 5), (False, True), (0, 9), (False, True), (False, True)):
        assert decide(*args).reason
