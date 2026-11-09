"""Ventana de ajustes centrada: secciones, geometría y composición."""

from __future__ import annotations

import time
from types import SimpleNamespace

import numpy as np

from ezagent.voice.notch import drop as D
from ezagent.voice.notch import menu as M
from ezagent.voice.notch import settings as S
from ezagent.voice.notch import settings_window as SW
from ezagent.voice.notch.menu_layout import GROUPS, layout_menu


def test_una_seccion_por_grupo_con_titulo():
    names = [name for name, _title in SW.sections()]
    assert sorted(names) == sorted(name for name, _rows in GROUPS)  # todas, una vez
    assert names[:2] == ["appearance", "colors"]  # Colores, junto a Apariencia
    assert dict(SW.sections())["vad"] == "Fin de turno"
    assert SW.section_groups("llm")[0][0] == "llm"
    assert SW.section_groups("no-existe") == (GROUPS[0],)


def test_clic_en_la_barra_lateral_elige_seccion():
    x0, y0, x1, y1 = SW.item_rect(3)
    assert SW.item_at((x0 + x1) / 2, (y0 + y1) / 2) == SW.sections()[3][0]
    assert SW.item_at(SW.SIDEBAR_W + 40, (y0 + y1) / 2) is None  # zona de contenido
    assert SW.item_at((x0 + x1) / 2, 10) is None  # cabecera de la marca


def test_ventana_centrada_y_mas_amplia_que_la_tarjeta():
    w, h = SW.window_size(600)
    assert w == SW.SIDEBAR_W + SW.CONTENT_W and h == 604
    assert SW.CONTENT_W > 320  # más ancha que la tarjeta: etiqueta y valor no se pisan
    assert SW.centered((0, 0, 1920, 1040), 628, 604) == (646, 218)


def test_composicion_premultiplicada():
    dst = np.zeros((4, 4, 4), np.float32)
    dst[...] = (0.0, 0.0, 1.0, 1.0)
    src = np.zeros((2, 2, 4), np.float32)
    src[...] = (0.5, 0.0, 0.0, 0.5)
    SW.composite_over(dst, src, 1, 1)
    assert np.allclose(dst[1, 1], (0.5, 0.0, 0.5, 1.0))
    assert np.allclose(dst[0, 0], (0.0, 0.0, 1.0, 1.0))


def test_el_pie_queda_al_fondo_de_la_ventana():
    layout = layout_menu(None, 36 + 600, groups=SW.section_groups("vad"))
    assert layout["footer"].y1 == 36 + 600 - 12
    assert layout["group_vad"].y1 < layout["footer"].y0


def test_la_ventana_dibuja_barra_lateral_y_seccion():
    settings = S.MenuSettings.from_app({}, [], {"ef_dora": "Dora"})
    ui = D.DropUI()
    ui._theme_name = lambda: "dark"
    menu = ui._ensure_menu(settings)
    menu.set_scale(1.0)
    menu.set_max_height(SW.CONTENT_MAX_H)
    menu.set_available_height(900.0)
    menu.set_groups(SW.section_groups("appearance"), "")
    menu.open(time.monotonic())
    ui._menu_open = True
    lw, lh = SW.window_size(menu.viewport_height)
    ui._card = SimpleNamespace(w=round(lw), h=round(lh), _interval=0)
    frame = ui.render_card()
    assert frame.shape == (round(lh), round(lw), 4)
    # Cambiar de sección solo cambia lo que muestra el menú.
    ui.select_section("llm")
    assert menu.groups == ("llm",)
    assert ui._section == "llm"
    # Un clic en la barra lateral también cambia de sección.
    x0, y0, x1, y1 = SW.item_rect(0)
    ui.card_pointer("down", int((x0 + x1) / 2), int((y0 + y1) / 2))
    assert menu.groups == (GROUPS[0][0],)


def _window(section: str = "appearance"):
    settings = S.MenuSettings.from_app({}, [], {"ef_dora": "Dora"})
    ui = D.DropUI()
    ui._theme_name = lambda: "dark"
    live: list = []
    ui.on_live_setting = lambda key, value: live.append((key, value))
    menu = ui._ensure_menu(settings)
    menu.set_scale(1.0)
    menu.set_width(SW.CONTENT_W)
    menu.set_max_height(SW.CONTENT_MAX_H)
    menu.set_available_height(900.0)
    ui._section = section
    menu.set_groups(SW.section_groups(section), "")
    menu.open(time.monotonic() - 2.0)
    ui._menu_open = True
    lw, lh = SW.window_size(menu.viewport_height)
    ui._card = SimpleNamespace(w=round(lw), h=round(lh), _interval=0)
    return ui, menu, settings, live


def _click(ui, lx, ly):
    ui.card_pointer("down", int(lx), int(ly))
    ui.card_pointer("up", int(lx), int(ly))


def test_los_clics_llegan_a_los_controles_de_una_seccion():
    """Regresión: con una sola sección, cada clic lanzaba KeyError('test_voice')."""
    ui, menu, settings, live = _window("appearance")
    row = menu._layout()["theme"]
    _click(ui, SW.SIDEBAR_W + 150, (row.y0 + row.y1) / 2 - 36)
    assert menu._selector_key == "theme"
    # «–» (a la derecha, desplazado por el ancho extra) oculta la ventana.
    menu._selector_key = None
    header = menu._layout()["header"]
    _click(ui, SW.SIDEBAR_W + M.HIDE_CX + menu._dx, (header.y0 + header.y1) / 2 - 36)
    assert not menu.is_open()


def test_paleta_base_y_colores_propios():
    settings = S.MenuSettings.from_app({}, [], {})
    settings.set("theme_preset", "atardecer")
    assert settings.get("theme") == "custom"
    assert settings.custom_colors() == ("#E0564F", "#FFB36B", "#FFE08A", "#5B1A3A")
    settings.set("theme_primary", "#12ab9c")
    assert settings.get("theme_primary") == "#12AB9C"
    settings.set("theme_primary", "rojo")  # mal escrito: se conserva el anterior
    assert settings.get("theme_primary") == "#12AB9C"
    assert [o.value for o in settings.select_options("theme")][-1] == "custom"


def test_las_muestras_de_paleta_eligen_el_tema():
    ui, menu, settings, live = _window("colors")
    row = menu._layout()["theme_preview"]
    pills = menu._palette_pills(row.y0)
    value, _label, pill = pills[1]  # «Oscuro»
    _click(ui, SW.SIDEBAR_W + (pill.x0 + pill.x1) / 2, (pill.y0 + pill.y1) / 2 - 36)
    assert settings.get("theme") == value == "dark"
    assert ("theme", "dark") in live
    frame = ui.render_card()  # la sección de colores se dibuja sin fallar
    assert frame.shape[2] == 4


def test_la_barra_usa_la_paleta_propia_salvo_en_avisos():
    from ezagent.voice.notch.flow import colors_for, custom_palette

    palette = custom_palette("#E0564F", "#FFB36B", "#FFE08A", "#5B1A3A")
    assert palette is not None and palette.shape == (5, 3)
    assert custom_palette("nope", "#FFB36B", "#FFE08A", "#5B1A3A") is None
    m = D.DropModel()
    t = 1000.0
    for _ in range(180):
        t += 1 / 60
        m.update(t, 1 / 60, "idle", 0.0, "dark", False, False, False, False, custom=palette)
    assert np.abs(m.cols - palette).max() < 8
    for _ in range(180):
        t += 1 / 60
        m.update(t, 1 / 60, "error", 0.0, "dark", False, False, False, False, custom=palette)
    assert np.abs(m.cols - colors_for("dark", "error")).max() < 8  # el error sigue en rojo


def test_la_app_guarda_la_paleta_y_activa_el_tema_personalizado():
    from ezagent.voice.app import VoiceApp

    saved: dict = {}
    applied: list = []
    app = VoiceApp.__new__(VoiceApp)
    app.cfg = SimpleNamespace(
        theme="dark",
        theme_primary="#1FAE9F",
        theme_secondary="#8FE3C0",
        theme_accent="#FF8A65",
        theme_background="#0E6F7C",
        save_key=lambda key, value: saved.__setitem__(key, value),
    )
    app.notch = SimpleNamespace(
        set_theme=lambda value: applied.append(("theme", value)),
        set_custom_colors=lambda *colors: applied.append(("colors", colors)),
    )
    app._apply_live_setting("theme_preset", "bosque")
    assert app.cfg.theme == "custom" and saved["theme"] == "custom"
    assert saved["theme_primary"] == "#2E7D4F" and saved["theme_background"] == "#13361F"
    assert ("theme", "custom") in applied
    assert applied[-2][0] == "colors" or applied[-1][0] == "colors"
    app._apply_live_setting("theme_accent", "#abcdef")
    assert saved["theme_accent"] == "#ABCDEF"
