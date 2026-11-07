"""Pruebas de la fila de confirmación del notch (sin ventana real)."""

from __future__ import annotations

from ezagent.voice.notch.confirm import CONFIRM_H, CONFIRM_W
from ezagent.voice.notch.island import Notch
from ezagent.voice.notch.tokens import H_ISLAND


def _visible_notch() -> Notch:
    notch = Notch(None)
    notch._visible = True
    notch._ui.set_visible(True)
    notch._ui.set_base("idle")
    return notch


def test_la_isla_crece_con_el_resumen_y_los_botones():
    notch = _visible_notch()
    notch.show_confirmation(
        "Voy a mover 12 archivos a Facturas. ¿Confirmas?",
        True,
        lambda: None,
        lambda: None,
    )
    notch._tab_w.value = CONFIRM_W
    frame = notch.render_frame()
    assert frame.shape == (int(H_ISLAND + CONFIRM_H), int(CONFIRM_W), 4)
    assert notch.confirmation_visible
    # El cuerpo del panel es opaco y los botones están dentro.
    assert frame[int(H_ISLAND) + 10, int(CONFIRM_W) // 2, 3] == 255


def test_el_clic_en_si_activa_el_callback_y_cierra_la_fila():
    notch = _visible_notch()
    calls: list[str] = []
    notch.show_confirmation(
        "¿Confirmas?",
        False,
        lambda: calls.append("yes"),
        lambda: calls.append("no"),
    )
    notch._tab_w.value = CONFIRM_W
    no, yes = notch._confirm_panel.buttons()
    x = (yes.x0 + yes.x1) / 2.0
    y = H_ISLAND + (yes.y0 + yes.y1) / 2.0
    assert notch.pointer_down(x, y)
    assert notch.pointer_up(x, y)
    assert calls == ["yes"]
    assert not notch.confirmation_visible


def test_el_clic_en_no_activa_el_rechazo():
    notch = _visible_notch()
    calls: list[str] = []
    notch.show_confirmation(
        "¿Confirmas?",
        False,
        lambda: calls.append("yes"),
        lambda: calls.append("no"),
    )
    notch._tab_w.value = CONFIRM_W
    no, _yes = notch._confirm_panel.buttons()
    x = (no.x0 + no.x1) / 2.0
    y = H_ISLAND + (no.y0 + no.y1) / 2.0
    notch.pointer_down(x, y)
    notch.pointer_up(x, y)
    assert calls == ["no"]


def test_el_menu_no_se_abre_mientras_hay_confirmacion():
    notch = _visible_notch()
    notch.show_confirmation("¿Confirmas?", False, lambda: None, lambda: None)
    notch._tab_w.value = CONFIRM_W
    notch.toggle_menu()
    assert notch._menu is None
    assert not notch._menu_open


def test_ocultar_la_confirmacion_vuelve_al_ancho_compacto():
    notch = _visible_notch()
    notch.show_confirmation("¿Confirmas?", False, lambda: None, lambda: None)
    notch._tab_w.value = CONFIRM_W
    assert notch.render_frame().shape[1] == int(CONFIRM_W)
    notch.hide_confirmation()
    notch._tab_w.value = notch._compact_layout_width()
    frame = notch.render_frame()
    assert not notch.confirmation_visible
    assert frame.shape == (int(H_ISLAND), notch._compact_width_px(), 4)
