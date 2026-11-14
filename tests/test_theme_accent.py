"""Tema personalizado: su paleta tiñe chat, ajustes, botones y avisos con buen contraste."""

from __future__ import annotations

import colorsys
from types import SimpleNamespace

import numpy as np
import pytest

from ezagent.voice.hud import tokens
from ezagent.voice.hud.a11y import contrast
from ezagent.voice.hud.flow import PRESETS
from ezagent.voice.hud.theme_accent import derive


@pytest.fixture(autouse=True)
def _factory_palette():
    tokens.set_themed(None)
    yield
    tokens.set_themed(None)


def _hue(rgb) -> float:
    return colorsys.rgb_to_hls(*(c / 255 for c in rgb[:3]))[0]


@pytest.mark.parametrize("preset", sorted(PRESETS))
@pytest.mark.parametrize("theme", ["light", "dark"])
def test_every_preset_keeps_readable_accents(preset: str, theme: str) -> None:
    background, primary, _secondary, _accent = PRESETS[preset]
    pal = derive(primary, background)[theme]
    assert contrast(pal["MINT"], pal["BODY"]) >= 4.5
    assert contrast(pal["MINT_INK"], pal["MINT_BTN"]) >= 4.5
    assert contrast(pal["TEXT"], pal["BODY"]) >= 7.0  # el texto principal no se resiente


def test_the_accent_takes_the_primary_hue_and_surfaces_the_background_hue() -> None:
    pal = derive("#7A6FF0", "#2E2A6B")["dark"]  # lavanda
    assert abs(_hue(pal["MINT"]) - _hue((0x7A, 0x6F, 0xF0))) < 0.02
    assert abs(_hue(pal["GROUP"]) - _hue((0x2E, 0x2A, 0x6B))) < 0.03
    assert "WARM" not in pal and "ERR" not in pal  # ámbar y rojo no se tocan


def test_bad_colours_leave_the_factory_palette() -> None:
    assert derive("no-es-hex", "#000000") is None


def test_palette_overrides_apply_and_clear_with_a_new_version() -> None:
    start = tokens.palette_version()
    factory = tokens.palette("dark")["MINT"]
    tokens.set_themed(derive("#E0564F", "#5B1A3A"))  # atardecer
    assert tokens.palette_version() == start + 1
    assert tokens.palette("dark")["MINT"] != factory
    assert tokens.palette("dark")["ERR"] == tokens.DARK["ERR"]
    tokens.set_themed(derive("#E0564F", "#5B1A3A"))  # lo mismo: no invalida cachés
    assert tokens.palette_version() == start + 1
    tokens.set_themed(None)
    assert tokens.palette("dark") is tokens.DARK and tokens.palette_version() == start + 2


def test_choosing_the_custom_theme_tints_the_bar_buttons() -> None:
    from ezagent.voice.hud.drop import DropUI, button_sprite

    plain = np.asarray(button_sprite("call", "dark", 1.0, call=True)).astype(int)
    ui = DropUI(None, cfg=None)
    ui.set_theme("custom")
    ui.set_custom_colors("#7A6FF0", "#C9B8FF", "#FF9EC4", "#2E2A6B")
    tinted = np.asarray(button_sprite("call", "dark", 1.0, call=True)).astype(int)
    centre = tuple(s // 2 for s in tinted.shape[:2])
    r, g, b = tinted[centre][:3]
    assert b > g and (plain[centre][:3] != tinted[centre][:3]).any()  # lavanda, no menta
    ui.set_theme("dark")
    assert tokens.palette("dark") is tokens.DARK  # otro tema: vuelve la paleta de fábrica


def test_the_orbit_and_the_chat_follow_the_palette(tmp_path) -> None:
    import sys
    from pathlib import Path

    from ezagent.voice.hud import marks

    tokens.set_themed(derive("#2E7D4F", "#13361F"))  # bosque
    assert marks.ink("dark")["mint"] == tuple(tokens.palette("dark")["MINT"][:3])

    sys.path.insert(0, str(Path(__file__).parent))
    from test_chat_view import make

    from ezagent.voice.hud.bar_theme import card_palette

    _, view = make(tmp_path)
    before = dict(view.g.pal)
    tokens.set_themed(derive("#7A6FF0", "#2E2A6B"))
    view._cache["x"] = SimpleNamespace()  # algo en caché
    view.configure(view.g.width, view.g.height, view.g.scale, view.g.theme, card_palette("dark"))
    assert view.g.pal != before and "x" not in view._cache  # otros colores: mensajes rehechos
