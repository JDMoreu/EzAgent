"""Medidor de entrada: toda la pista es el rango del umbral (se arrastra en dB)."""

from __future__ import annotations

import math

import pytest

from ezagent.voice.notch.menu import SLIDER_TRACK_X0, SLIDER_TRACK_X1, Menu
from ezagent.voice.notch.settings import (
    DEFAULT_VALUES,
    METER_DB_CEIL,
    METER_DB_FLOOR,
    SLIDERS,
    MenuSettings,
    db_to_rms,
)


def _menu(**overrides) -> Menu:
    values = dict(DEFAULT_VALUES)
    values.update(overrides)
    return Menu(MenuSettings.from_app(values, devices=[], voices=None), scale=1.0)


def _db(rms: float) -> float:
    return 20.0 * math.log10(rms)


def _drag_to(menu: Menu, fraction: float) -> float:
    x = SLIDER_TRACK_X0 + fraction * (SLIDER_TRACK_X1 - SLIDER_TRACK_X0)
    menu._set_meter_from_x(x, now=1.0)
    return float(menu.settings.get("min_rms"))


def test_slider_range_is_the_whole_meter_scale() -> None:
    spec = SLIDERS["min_rms"]
    assert _db(spec.minimum) == pytest.approx(METER_DB_FLOOR)
    assert _db(spec.maximum) == pytest.approx(METER_DB_CEIL)


def test_both_ends_of_the_track_reach_the_ends_of_the_scale() -> None:
    menu = _menu()
    assert _db(_drag_to(menu, 0.0)) == pytest.approx(METER_DB_FLOOR, abs=0.05)
    assert _db(_drag_to(menu, 1.0)) == pytest.approx(METER_DB_CEIL, abs=0.05)


def test_thumb_ends_up_under_the_pointer() -> None:
    menu = _menu()
    span = METER_DB_CEIL - METER_DB_FLOOR
    for fraction in (0.05, 0.25, 0.5, 0.75, 0.95):
        rms = _drag_to(menu, fraction)
        drawn = menu._meter_fraction(rms)
        assert drawn == pytest.approx(fraction, abs=0.5 / span)


def test_movement_is_uniform_in_db_not_in_amplitude() -> None:
    menu = _menu()
    low = _db(_drag_to(menu, 0.10))
    high = _db(_drag_to(menu, 0.90))
    assert high - low == pytest.approx(0.8 * (METER_DB_CEIL - METER_DB_FLOOR), abs=1.0)


def test_default_threshold_is_inside_the_range_and_survives_quantizing() -> None:
    settings = MenuSettings.from_app(dict(DEFAULT_VALUES), devices=[], voices=None)
    assert settings.get("min_rms") == pytest.approx(DEFAULT_VALUES["min_rms"], rel=0.06)
    assert db_to_rms(METER_DB_FLOOR) < DEFAULT_VALUES["min_rms"] < db_to_rms(METER_DB_CEIL)
