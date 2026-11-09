"""Input lag con el menú abierto: la tarjeta no debe colgar el ratón de todo el sistema."""

from __future__ import annotations

import sys

import numpy as np
import pytest

from ezagent.voice.notch import drop as D
from ezagent.voice.notch import layered as L


# ------------------------------------------------- ¿hace falta el gancho global?
@pytest.mark.parametrize(("routing", "expected"), [(2, True), (1, False), (0, False)])
def test_la_rueda_sigue_al_puntero_solo_con_enrutado_2(monkeypatch, routing, expected):
    monkeypatch.setattr(L, "_IS_WIN", True)
    monkeypatch.setattr(L, "_read_wheel_routing", lambda: routing)
    assert L.wheel_follows_pointer() is expected


def test_sin_el_valor_en_el_registro_windows_10_usa_el_de_fabrica(monkeypatch):
    monkeypatch.setattr(L, "_IS_WIN", True)
    monkeypatch.setattr(L, "_read_wheel_routing", lambda: None)
    monkeypatch.setattr(L, "_registry_readable", lambda: True)
    monkeypatch.setattr(sys, "getwindowsversion", lambda: type("V", (), {"major": 10})(), raising=False)
    assert L.wheel_follows_pointer() is True
    monkeypatch.setattr(sys, "getwindowsversion", lambda: type("V", (), {"major": 6})(), raising=False)
    assert L.wheel_follows_pointer() is False              # Windows 7/8: no hay enrutado


def test_si_no_se_puede_leer_el_registro_se_conserva_el_gancho(monkeypatch):
    monkeypatch.setattr(L, "_IS_WIN", True)
    monkeypatch.setattr(L, "_read_wheel_routing", lambda: None)
    monkeypatch.setattr(L, "_registry_readable", lambda: False)
    monkeypatch.setattr(sys, "getwindowsversion", lambda: type("V", (), {"major": 11})(), raising=False)
    assert L.wheel_follows_pointer() is False


def test_fuera_de_windows_no_hay_enrutado(monkeypatch):
    monkeypatch.setattr(L, "_IS_WIN", False)
    assert L.wheel_follows_pointer() is False


# ------------------------------------------------- quién instala el gancho
class _Recorder:
    def __init__(self, monkeypatch) -> None:
        self.calls = 0
        monkeypatch.setattr(L.LayeredWindow, "_install_mouse_hook", self._hook)

    def _hook(self, *_args) -> None:
        self.calls += 1


def test_la_tarjeta_no_instala_el_gancho_si_windows_ya_enruta_la_rueda(monkeypatch):
    rec = _Recorder(monkeypatch)
    monkeypatch.setattr(L, "wheel_follows_pointer", lambda: True)
    D.CardWindow(None, 0, 0, 10, 10)._install_mouse_hook()
    assert rec.calls == 0


def test_la_tarjeta_conserva_el_gancho_como_respaldo_si_no_hay_enrutado(monkeypatch):
    rec = _Recorder(monkeypatch)
    monkeypatch.setattr(L, "wheel_follows_pointer", lambda: False)
    D.CardWindow(None, 0, 0, 10, 10)._install_mouse_hook()
    assert rec.calls == 1


def test_la_barra_nunca_instala_el_gancho(monkeypatch):
    rec = _Recorder(monkeypatch)
    monkeypatch.setattr(L, "wheel_follows_pointer", lambda: False)
    D.DropWindow(None, 0, 0, 10, 10)._install_mouse_hook()
    assert rec.calls == 0


def test_la_rueda_nativa_llega_a_la_misma_funcion_que_el_gancho():
    """Sin gancho, WM_MOUSEWHEEL (wndproc) y el gancho convergen en card_wheel."""
    seen: list[tuple] = []
    ui = type("UI", (), {"card_wheel": lambda self, x, y, d: seen.append((x, y, d)) or True})()
    win = D.CardWindow(ui, 100, 200, 10, 10)
    assert win.on_wheel(5, 6, -120) is True              # camino nativo (wndproc)
    assert win.on_global_wheel(105, 206, -120) is True   # camino del gancho (pantalla → ventana)
    assert seen == [(5, 6, -120), (5, 6, -120)]


# ------------------------------------------------- el cuadro de la tarjeta
def test_el_cuerpo_de_la_tarjeta_se_cachea_y_es_de_solo_lectura():
    D.render_card_body.cache_clear()
    a = D.render_card_body(90, 120, 1.0, "light", fresh=True)
    b = D.render_card_body(90, 120, 1.0, "light", fresh=True)
    assert a is b and D.render_card_body.cache_info().hits == 1
    assert not a.flags.writeable
    with pytest.raises(ValueError):
        a[0, 0, 0] = 1.0                                  # nadie puede ensuciar la caché
    c = D.render_card_body(90, 121, 1.0, "light", fresh=True)
    assert c is not a and c.shape[0] == 121               # otra altura, otra entrada


def test_pegar_el_cuerpo_en_el_lienzo_no_toca_la_caja():
    body = D.render_card_body(60, 40, 1.0, "dark", fresh=True)
    before = body.copy()
    buf = np.zeros((50, 60, 4), np.float32)
    buf[5:45] = body                                      # lo que hace render_card
    buf[..., :3] *= 0.5
    assert np.array_equal(body, before)
