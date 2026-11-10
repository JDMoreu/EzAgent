"""Ventana de ajustes: secciones, filas avanzadas, geometría y tamaño de la barra."""

from __future__ import annotations

from types import SimpleNamespace

import pytest

from ezagent.voice.config import VoiceConfig
from ezagent.voice.notch import settings_window as sw
from ezagent.voice.notch.drop import DropUI, layout_size
from ezagent.voice.notch.menu import Menu
from ezagent.voice.notch.menu_layout import (
    ADVANCED_ROWS,
    CUSTOM_THEME_ROWS,
    GROUP_LABELS,
    GROUPS,
    SECTIONS,
)
from ezagent.voice.notch.settings import (
    DEFAULT_VALUES,
    INSTANT_KEYS,
    LABELS,
    SLIDERS,
    UI_SCALE_MAX,
    UI_SCALE_MIN,
    MenuSettings,
    normalize_setting,
)

WORK = (0, 0, 1920, 1040)


def _settings(**overrides) -> MenuSettings:
    values = dict(DEFAULT_VALUES)
    values.update(overrides)
    return MenuSettings.from_app(values, devices=[], voices=None)


def _row_keys() -> list[str]:
    return [key for _name, rows in GROUPS for key, _kind in rows]


# ------------------------------------------------------------- estructura
def test_five_sections_cover_every_group_once() -> None:
    assert len(SECTIONS) == 5
    grouped = [group for _id, _title, groups in SECTIONS for group in groups]
    assert sorted(grouped) == sorted(name for name, _rows in GROUPS)
    assert len(grouped) == len(set(grouped))
    assert all(name in GROUP_LABELS for name in grouped)


def test_section_groups_resolve_and_fallback() -> None:
    for name, _title in sw.sections():
        groups = sw.section_groups(name)
        assert groups and all(rows for _g, rows in groups)
    assert sw.section_groups("no-existe") == sw.section_groups("appearance")


def test_every_row_has_one_label_and_is_unique() -> None:
    keys = _row_keys()
    assert len(keys) == len(set(keys))
    for key in keys:
        assert key in LABELS, key


def test_hidden_row_sets_only_name_real_rows() -> None:
    keys = set(_row_keys())
    assert ADVANCED_ROWS <= keys
    assert CUSTOM_THEME_ROWS <= keys
    assert not ADVANCED_ROWS & CUSTOM_THEME_ROWS


def test_instant_keys_match_app() -> None:
    app = pytest.importorskip("ezagent.voice.app")
    assert tuple(app.INSTANT_SETTINGS) == tuple(INSTANT_KEYS)
    assert "ui_scale" in INSTANT_KEYS and "show_advanced" in INSTANT_KEYS


def test_config_has_new_fields() -> None:
    cfg = VoiceConfig()
    assert cfg.ui_scale == 1.0
    assert cfg.show_advanced is False
    assert (cfg.settings_x, cfg.settings_y, cfg.settings_w, cfg.settings_h) == (None,) * 4


# ----------------------------------------------------------------- avanzado
def _visible_rows(menu: Menu) -> set[str]:
    return {key for _g, rows in menu._shown_groups() for key, _kind in rows}


def test_advanced_rows_are_folded_until_asked() -> None:
    menu = Menu(_settings(), scale=1.0)
    menu.set_groups(sw.section_groups("models"), "Modelos")
    folded = _visible_rows(menu)
    assert not folded & ADVANCED_ROWS
    assert {"stt_model", "llm_profile", "llm_model", "api_key"} <= folded

    menu = Menu(_settings(), scale=1.0)
    menu.set_groups(sw.section_groups("system"), "Sistema")
    assert not _visible_rows(menu) & ADVANCED_ROWS

    menu = Menu(_settings(show_advanced=True), scale=1.0)
    menu.set_groups(sw.section_groups("system"), "Sistema")
    assert ADVANCED_ROWS <= _visible_rows(menu)


def test_advanced_rows_live_only_next_to_their_switch() -> None:
    """El interruptor está en Sistema: lo que activa tiene que verse ahí, no en otra sección."""
    for name, _title in sw.sections():
        if name == "system":
            continue
        menu = Menu(_settings(show_advanced=True), scale=1.0)
        menu.set_groups(sw.section_groups(name), sw.section_title(name))
        assert not _visible_rows(menu) & ADVANCED_ROWS, name


def test_custom_theme_rows_only_with_custom_theme() -> None:
    menu = Menu(_settings(), scale=1.0)
    menu.set_groups(sw.section_groups("appearance"), "Aspecto")
    assert not _visible_rows(menu) & CUSTOM_THEME_ROWS
    menu = Menu(_settings(theme="custom"), scale=1.0)
    menu.set_groups(sw.section_groups("appearance"), "Aspecto")
    assert CUSTOM_THEME_ROWS <= _visible_rows(menu)


# -------------------------------------------------------------- tamaño barra
def test_ui_scale_setting_is_percent_and_clamped() -> None:
    settings = _settings(ui_scale=1.0)
    assert settings.format("ui_scale") == "100 %"
    assert normalize_setting("ui_scale", 9) == pytest.approx(UI_SCALE_MAX)
    assert normalize_setting("ui_scale", 0.1) == pytest.approx(UI_SCALE_MIN)
    assert normalize_setting("ui_scale", 1.33) == pytest.approx(1.35)
    assert "ui_scale" in SLIDERS


@pytest.mark.parametrize("layout", ["pill", "circle"])
def test_bar_scale_resizes_window_and_keeps_aspect(layout: str) -> None:
    ui = DropUI(None, cfg=SimpleNamespace(ui_layout=layout, ui_scale=1.0))
    base_w, base_h = ui._size()
    ui.set_bar_scale(2.0)
    assert ui.bar_scale == pytest.approx(2.0)
    big_w, big_h = ui._size()
    lw, lh = layout_size(layout)
    assert (big_w, big_h) == (round(lw * 2.0), round(lh * 2.0))
    assert big_w == pytest.approx(base_w * 2, abs=1) and big_h == pytest.approx(base_h * 2, abs=1)
    ui.set_bar_scale(0.0)
    assert ui.bar_scale == pytest.approx(UI_SCALE_MIN)


def test_bar_scale_does_not_touch_settings_scale() -> None:
    ui = DropUI(None, cfg=SimpleNamespace(ui_layout="pill", ui_scale=1.5))
    assert ui.bar_scale == pytest.approx(1.5)
    assert ui.bs == pytest.approx(ui.s * 1.5)


def test_bar_hit_testing_follows_scale() -> None:
    ui = DropUI(None, cfg=SimpleNamespace(ui_layout="pill", ui_scale=1.0))
    ui.set_bar_scale(2.0)
    fx, fy = ui.model.positions()["pill"][:2]
    # Con la escala doble el centro de la forma sigue siendo forma en coordenadas lógicas.
    assert ui.model.item_at(fx, fy) == "pill"


# --------------------------------------------------------------- geometría
def test_resize_edges_zones() -> None:
    w, h = 760.0, 620.0
    assert sw.resize_edges(w / 2, h / 2, w, h) is None
    assert sw.resize_edges(w - sw.BODY_INSET - 2, h / 2, w, h) == (False, False, True, False)
    assert sw.resize_edges(w / 2, h - 2, w, h) == (False, False, False, True)
    assert sw.resize_edges(w - sw.BODY_INSET - 4, h - 4, w, h) == (False, False, True, True)
    assert sw.resize_edges(sw.BODY_INSET + 4, 4, w, h) == (True, True, False, False)
    assert sw.resize_edges(1.0, h / 2, w, h) is None  # margen transparente del cuerpo
    assert sw.resize_edges(-50.0, h / 2, w, h) is None


def test_cursor_for_edges() -> None:
    assert sw.cursor_for(None) == 0
    assert sw.cursor_for((False, False, True, False)) == sw.IDC_SIZEWE
    assert sw.cursor_for((False, False, False, True)) == sw.IDC_SIZENS
    assert sw.cursor_for((False, False, True, True)) == sw.IDC_SIZENWSE
    assert sw.cursor_for((True, False, False, True)) == sw.IDC_SIZENESW


def test_resized_rect_keeps_opposite_edge() -> None:
    limits = (560, 400, 1900, 1000)
    start = (500, 300, 760, 620)
    assert sw.resized_rect((False, False, True, True), start, (100, 50), limits) == (
        500,
        300,
        860,
        670,
    )
    # Borde izquierdo: el derecho (x + w) no se mueve.
    x, y, w, h = sw.resized_rect((True, False, False, False), start, (-80, 0), limits)
    assert (x + w, y, h) == (500 + 760, 300, 620) and w == 840
    # Mínimo y máximo.
    x, y, w, h = sw.resized_rect((True, True, False, False), start, (900, 900), limits)
    assert (w, h) == (560, 400) and (x + w, y + h) == (500 + 760, 300 + 620)
    assert sw.resized_rect((False, False, True, True), start, (9000, 9000), limits)[2:] == (
        1900,
        1000,
    )


def test_clamp_size_and_position() -> None:
    assert sw.clamp_size(100, 100, WORK, 1.0) == (sw.MIN_W, sw.MIN_H)
    assert sw.clamp_size(9999, 9999, WORK, 1.0) == (1920 - 16, 1040 - 16)
    w, h = 760, 620
    x, y = sw.clamp_position(-9000, -9000, w, h, WORK, 1.0)
    assert x + w >= sw.KEEP_VISIBLE and y == 0
    x, y = sw.clamp_position(9000, 9000, w, h, WORK, 1.0)
    assert x <= 1920 - sw.KEEP_VISIBLE and y <= 1040 - sw.KEEP_VISIBLE
    assert sw.clamp_position(300, 200, w, h, WORK, 1.0) == (300, 200)
    assert sw.centered(WORK, w, h) == ((1920 - w) // 2, (1040 - h) // 2)


def test_content_size_matches_window() -> None:
    cw, vh = sw.content_size(760, 620)
    assert cw == 760 - sw.SIDEBAR_W and vh == 620 - sw.WINDOW_EXTRA_H


# -------------------------------------------------------------------- dibujo
@pytest.mark.parametrize("size", [(sw.MIN_W, sw.MIN_H), (sw.DEFAULT_W, sw.DEFAULT_H), (1200, 800)])
@pytest.mark.parametrize("section", [name for name, _t in sw.sections()])
def test_card_renders_at_any_size(section: str, size: tuple[float, float]) -> None:
    lw, lh = size
    settings = _settings(show_advanced=True, theme="custom")
    menu = Menu(settings, scale=1.0, theme="dark")
    cw, vh = sw.content_size(lw, lh)
    menu.set_reduced_motion(True)
    menu.set_width(cw)
    menu.set_max_height(vh)
    menu.set_groups(sw.section_groups(section), sw.section_title(section), 0.0)
    menu.open(0.0)
    ui = DropUI(None, cfg=None)
    ui._menu, ui._menu_open, ui._section = menu, True, section
    ui._theme_setting = "dark"
    ui._card = SimpleNamespace(w=round(lw), h=round(lh), x=0, y=0, _interval=0.0)
    frame = None
    for step in range(1, 6):
        menu.update(step * 0.2)
        frame = ui.render_card()
    assert frame.shape == (round(lh), round(lw), 4)
    assert frame[..., 3].max() > 0


def test_menu_resize_updates_dimensions_immediately() -> None:
    menu = Menu(_settings(), scale=1.0)
    menu.set_reduced_motion(True)
    menu.set_groups(sw.section_groups("voice"), "Voz")
    menu.set_width(500)
    menu.set_max_height(500)
    menu.open(0.0)
    menu.update(1.0)
    menu.resize(700, 300, 2.0)
    menu.update(2.0)
    assert menu.width == 700
    assert menu.viewport_height == 300
    assert menu.height(2.0) == pytest.approx(300)


# ------------------------------------------------------------ Ctrl + rueda
class _FakeUser32:
    def __init__(self, ctrl: bool) -> None:
        self.ctrl = ctrl

    def GetAsyncKeyState(self, vk: int) -> int:  # noqa: N802 - API de Win32
        return 0x8000 if (vk == 0x11 and self.ctrl) else 0


@pytest.mark.parametrize(
    ("ctrl", "delta", "expected"), [(True, 120, 1.05), (True, -120, 0.95), (False, 120, 1.0)]
)
def test_ctrl_wheel_changes_bar_size(monkeypatch, ctrl: bool, delta: int, expected: float) -> None:
    from ezagent.voice.notch import drop

    monkeypatch.setattr(drop._shell, "_u32", _FakeUser32(ctrl))
    lives: list[tuple[str, float]] = []
    ui = DropUI(None, cfg=SimpleNamespace(ui_layout="pill", ui_scale=1.0))
    ui.on_live_setting = lambda key, value: lives.append((key, value))
    fx, fy = ui.model.positions()["pill"][:2]
    handled = ui.bar_wheel(fx, fy, delta)
    assert handled is ctrl
    assert ui.bar_scale == pytest.approx(expected)
    assert bool(lives) is (ctrl and expected != 1.0)


def test_wheel_outside_the_bar_is_ignored(monkeypatch) -> None:
    from ezagent.voice.notch import drop

    monkeypatch.setattr(drop._shell, "_u32", _FakeUser32(True))
    ui = DropUI(None, cfg=SimpleNamespace(ui_layout="pill", ui_scale=1.0))
    assert ui.bar_wheel(-500.0, -500.0, 120) is False
    assert ui.bar_scale == 1.0
