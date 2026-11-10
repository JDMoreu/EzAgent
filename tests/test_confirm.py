"""ConfirmationManager: la confirmación es el último freno ante acciones de riesgo."""

from __future__ import annotations

import threading
import time

import pytest

from ezagent.agent.confirm import ConfirmationManager

ACCEPT = ["sí", "si", "confirmo", "adelante", "dale"]
REJECT = ["no", "cancela", "para"]


def make(timeout: float = 5.0, on_ask=None) -> ConfirmationManager:
    return ConfirmationManager(accept=ACCEPT, reject=REJECT, timeout_s=timeout, on_ask=on_ask)


class Asker:
    """Lanza ``ask`` en otro hilo, como hace el hilo de herramientas."""

    def __init__(self, manager: ConfirmationManager, **kwargs) -> None:
        self.manager = manager
        self.result: bool | None = None
        self.thread = threading.Thread(
            target=lambda: setattr(self, "result", manager.ask("¿Borro?", **kwargs)),
            daemon=True,
        )
        self.thread.start()
        deadline = time.monotonic() + 2
        while not manager.pending and time.monotonic() < deadline:
            time.sleep(0.001)
        assert manager.pending, "la confirmación nunca quedó pendiente"

    def join(self) -> bool:
        self.thread.join(5)
        assert not self.thread.is_alive(), "ask() no devolvió"
        assert self.result is not None
        return self.result


def answer(text: str, **ask_kwargs) -> bool:
    manager = make()
    asker = Asker(manager, **ask_kwargs)
    returned = manager.submit_text(text)
    final = asker.join()
    assert returned is final  # lo que devuelve submit_text es lo que ve ask
    assert not manager.pending
    return final


@pytest.mark.parametrize(
    "text", ["sí", "Sí", "SÍ", "si", "dale", "adelante", "sí, adelante", "Dale, hazlo"]
)
def test_respuestas_afirmativas_aceptan(text: str) -> None:
    assert answer(text) is True


@pytest.mark.parametrize(
    "text",
    [
        "no",
        "No.",
        "cancela",
        "para",
        "",
        "quizás",
        "hmm no sé",
        "daleee",  # palabra completa: no es «dale»
        "sin duda",  # «sin» no es «si»
    ],
)
def test_respuestas_negativas_o_ambiguas_rechazan(text: str) -> None:
    assert answer(text) is False


@pytest.mark.parametrize("text", ["no, dale", "dale, no", "sí pero cancela", "para, sí"])
def test_si_aparece_una_palabra_de_rechazo_gana_el_rechazo(text: str) -> None:
    assert answer(text) is False


def test_confirmacion_fuerte_exige_la_palabra_confirmo() -> None:
    for text in ["sí", "dale", "adelante", "ok", "confirma"]:
        assert answer(text, strong=True) is False, text
    assert answer("confirmo", strong=True) is True
    assert answer("Sí, confirmo", strong=True) is True


def test_confirmacion_fuerte_con_negacion_rechaza() -> None:
    assert answer("no confirmo", strong=True) is False


@pytest.mark.parametrize(
    ("text", "expected", "required"),
    [
        ("confirmo 7", True, 7),
        ("confirmo siete", True, 7),
        ("confirmo 8", False, 7),
        ("confirmo seis", False, 7),
        ("confirmo", False, 7),  # falta el número
        ("siete", False, 7),  # falta la palabra
        ("confirmo treinta y dos", True, 32),
        ("confirmo treinta y tres", False, 32),
        ("confirmo cien", True, 100),
        ("confirmo ciento veinte", True, 120),
        ("confirmo mil doscientos", True, 1200),
        ("confirmo mil", True, 1000),
        ("confirmo veintiuno", True, 21),
        ("confirmo 12 elementos", True, 12),
    ],
)
def test_confirmacion_fuerte_con_numero(text: str, expected: bool, required: int) -> None:
    assert answer(text, strong=True, require_number=required) is expected


def test_sin_confirmacion_pendiente_submit_devuelve_none() -> None:
    manager = make()
    assert manager.submit_text("sí") is None
    manager.submit_click(True)  # no debe lanzar
    manager.cancel()
    assert not manager.pending


def test_timeout_equivale_a_rechazo_y_libera_el_estado() -> None:
    manager = make(timeout=0.05)
    began = time.monotonic()
    assert manager.ask("¿Borro?") is False
    assert time.monotonic() - began < 2
    assert not manager.pending
    assert manager.submit_text("sí") is None  # respuesta tardía: se ignora


def test_timeout_cero_rechaza_de_inmediato() -> None:
    assert make(timeout=0).ask("¿Borro?") is False


def test_un_clic_resuelve() -> None:
    for clicked in (True, False):
        manager = make()
        asker = Asker(manager)
        manager.submit_click(clicked)
        assert asker.join() is clicked


def test_cancelar_rechaza() -> None:
    manager = make()
    asker = Asker(manager)
    manager.cancel("el usuario habló de otra cosa")
    assert asker.join() is False
    assert not manager.pending


def test_no_se_encolan_confirmaciones_la_segunda_se_rechaza() -> None:
    manager = make()
    first = Asker(manager)
    assert manager.ask("otra cosa") is False  # inmediata, no espera el timeout
    assert manager.pending  # y no pisa la primera
    manager.submit_text("sí")
    assert first.join() is True


def test_se_puede_preguntar_de_nuevo_tras_resolver() -> None:
    manager = make()
    for text, expected in [("sí", True), ("no", False), ("dale", True)]:
        asker = Asker(manager)
        manager.submit_text(text)
        assert asker.join() is expected


def test_on_ask_recibe_resumen_y_fuerza() -> None:
    seen: list[tuple[str, bool]] = []
    manager = make(on_ask=lambda summary, strong: seen.append((summary, strong)))
    asker = Asker(manager, strong=True)
    manager.submit_text("confirmo")
    asker.join()
    assert seen == [("¿Borro?", True)]


def test_un_fallo_de_la_interfaz_no_tumba_la_confirmacion() -> None:
    def boom(summary: str, strong: bool) -> None:
        raise RuntimeError("ui rota")

    manager = make(on_ask=boom)
    asker = Asker(manager)
    manager.submit_text("sí")
    assert asker.join() is True


def test_el_plazo_cuenta_desde_la_pregunta_no_desde_el_callback() -> None:
    def slow(summary: str, strong: bool) -> None:
        time.sleep(0.3)

    manager = make(timeout=0.2, on_ask=slow)
    began = time.monotonic()
    assert manager.ask("¿Borro?") is False
    assert time.monotonic() - began < 0.6


def test_respuestas_concurrentes_solo_resuelven_una_vez() -> None:
    manager = make()
    asker = Asker(manager)
    results: list[bool | None] = []
    threads = [
        threading.Thread(target=lambda t=t: results.append(manager.submit_text(t)))
        for t in ["sí", "no", "dale", "cancela"] * 5
    ]
    for t in threads:
        t.start()
    for t in threads:
        t.join(5)
    resolved = [r for r in results if r is not None]
    assert len(resolved) == 1
    assert asker.join() is resolved[0]
