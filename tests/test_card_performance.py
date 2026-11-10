"""Ventana de ajustes fluida: cuadros reutilizados, marca viva barata y arrastre sin repintar."""

from __future__ import annotations

import os
from types import SimpleNamespace

import numpy as np
import pytest

from ezagent.voice.notch import settings_window as sw
from ezagent.voice.notch.drop import CardWindow, DropUI
from ezagent.voice.notch.layered import LayeredWindow, paint_due
from ezagent.voice.notch.menu import Menu
from ezagent.voice.notch.settings import DEFAULT_VALUES, MenuSettings

LW, LH = 640, 520


def _ui(section: str = "voice") -> tuple[DropUI, Menu]:
    menu = Menu(MenuSettings.from_app(dict(DEFAULT_VALUES), devices=[], voices=None), theme="dark")
    cw, vh = sw.content_size(LW, LH)
    menu.set_reduced_motion(True)
    menu.set_width(cw)
    menu.set_max_height(vh)
    menu.set_groups(sw.section_groups(section), sw.section_title(section), 0.0)
    menu.open(0.0)
    menu.update(1.0)
    ui = DropUI(None, cfg=None)
    ui._menu, ui._menu_open, ui._section = menu, True, section
    ui._theme_setting = "dark"
    ui._card = SimpleNamespace(w=LW, h=LH, x=0, y=0, _interval=0.0)
    return ui, menu


def _settle(ui: DropUI, menu: Menu) -> None:
    for step in range(1, 12):  # deja terminar la animación de apertura
        menu.update(1.0 + step * 0.3)
        ui.render_card()


def _count_compositions(ui: DropUI) -> list[int]:
    calls = [0]
    original = ui._compose_card

    def spy(*args, **kwargs):
        calls[0] += 1
        return original(*args, **kwargs)

    ui._compose_card = spy  # type: ignore[method-assign]
    return calls


# ------------------------------------------------------------- reutilización
def test_a_still_card_is_not_recomposed() -> None:
    ui, menu = _ui()
    _settle(ui, menu)
    calls = _count_compositions(ui)
    for _ in range(10):
        ui.render_card()
    assert calls[0] == 0


def test_a_change_in_the_menu_recomposes_and_then_settles_again() -> None:
    ui, menu = _ui()
    _settle(ui, menu)
    calls = _count_compositions(ui)
    menu.settings.set("speed", 1.2)
    ui.render_card()
    assert calls[0] >= 1  # el cambio se ve
    _settle(ui, menu)  # (el pie «Guardar» se anima unos fotogramas)
    settled = calls[0]
    for _ in range(10):
        ui.render_card()
    assert calls[0] == settled  # y vuelve a reutilizarse el cuadro


def test_hover_in_the_sidebar_and_the_section_invalidate_the_cache() -> None:
    ui, menu = _ui()
    _settle(ui, menu)
    calls = _count_compositions(ui)
    ui._sidebar_hover = "models"
    ui.render_card()
    ui._section = "models"
    ui.render_card()
    assert calls[0] == 2


def test_the_cached_frame_is_never_modified_by_the_overlay() -> None:
    ui, menu = _ui()
    _settle(ui, menu)
    first = ui.render_card().copy()
    ui.render_card()
    cached = ui._card_cache[1].copy()
    ui.render_card()
    assert np.array_equal(ui._card_cache[1], cached)
    assert first.shape == cached.shape


def test_the_living_mark_moves_without_touching_the_rest_of_the_card() -> None:
    ui, menu = _ui()
    menu.set_reduced_motion(False)
    ui._reduced = False
    _settle(ui, menu)
    a = ui.render_card().astype(np.int16)
    for _ in range(40):
        ui.render_card()
    b = ui.render_card().astype(np.int16)
    changed = np.argwhere((a != b).any(axis=2))
    assert changed.size, "la marca debería animarse"
    y0, x0 = changed.min(axis=0)
    y1, x1 = changed.max(axis=0)
    # Todo el cambio cabe en el recuadro de la marca (al pie de la barra lateral).
    assert x1 < sw.SIDEBAR_W
    assert y0 > LH - 80
    assert (y1 - y0) <= sw.MARK_D + 2 and (x1 - x0) <= sw.MARK_D + 2


def test_a_still_frame_is_an_order_of_magnitude_cheaper_than_a_full_one() -> None:
    import time

    ui, menu = _ui()
    _settle(ui, menu)
    start = time.perf_counter()
    for _ in range(10):
        ui.render_card()
    still = (time.perf_counter() - start) / 10
    start = time.perf_counter()
    for _ in range(10):
        ui._card_cache = None
        ui._compose_card(menu, LW, LH, 1.0, "dark", 5.0, 400.0, 400.0)
    full = (time.perf_counter() - start) / 10
    assert still < full / 3  # holgado: en la práctica es ~10×


def test_the_height_and_opacity_of_the_menu_are_part_of_the_frame_key() -> None:
    """Abrir y cerrar anima la altura: cada valor distinto es un cuadro distinto."""
    ui, menu = _ui()
    _settle(ui, menu)
    before = menu.frame_key(9.0)
    calls = _count_compositions(ui)
    menu._height.value = float(menu._height.value) - 40.0
    assert menu.frame_key(9.0) != before
    ui.render_card()
    assert calls[0] == 1
    menu._content_alpha.value = 0.5
    assert menu.frame_key(9.0) != before


# --------------------------------------------------------------------- arrastre
def test_no_painting_while_a_lazy_window_is_dragged() -> None:
    kwargs = dict(visible=True, dirty=True, now=10.0, last_paint=0.0, interval=1 / 30)
    assert paint_due(dragging=False, **kwargs)
    assert not paint_due(dragging=True, **kwargs)


def test_painting_resumes_when_the_drag_ends_and_never_when_hidden() -> None:
    assert paint_due(True, False, False, now=1.0, last_paint=0.0, interval=1 / 30)
    assert not paint_due(True, False, False, now=0.001, last_paint=0.0, interval=1 / 30)
    assert not paint_due(False, False, True, now=1.0, last_paint=0.0, interval=1 / 30)


def test_only_the_settings_card_is_lazy() -> None:
    assert CardWindow.lazy_drag is True
    assert LayeredWindow.lazy_drag is False


@pytest.mark.skipif(os.name != "nt", reason="usa la API de ventanas de Windows")
@pytest.mark.parametrize(("lazy", "dirty"), [(True, False), (False, True)])
def test_moving_a_window_only_repaints_if_its_frame_depends_on_position(lazy, dirty) -> None:
    window = LayeredWindow(100, 100, click_through=False, title="t")
    window.lazy_drag = lazy
    window.hwnd = 0x7FFF0001  # identificador inválido: la llamada a Windows falla sin romper
    window._dirty = False
    window.set_pos(40, 60)
    assert (window.x, window.y) == (40, 60)
    assert window._dirty is dirty


@pytest.mark.skipif(os.name != "nt", reason="usa la API de ventanas de Windows")
def test_a_lazy_window_without_a_handle_still_marks_dirty_so_it_paints_in_place() -> None:
    window = LayeredWindow(100, 100, click_through=False, title="t")
    window.lazy_drag = True
    window._dirty = False
    window.set_pos(5, 5)
    assert window._dirty is True


def test_overlay_does_nothing_when_fully_faded() -> None:
    sidebar = sw.Sidebar()
    out = np.zeros((LH, 300, 4), np.uint8)
    sidebar.overlay_mark(out, 1.0, 1.0, opacity=0.0)
    assert not out.any()
    sidebar.overlay_mark(out, 1.0, 1.1, opacity=1.0)
    assert out.any()
