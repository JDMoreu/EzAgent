"""«Guardar» se activa con cualquier cambio, también con apariencia y escucha."""

from __future__ import annotations

from types import SimpleNamespace

import pytest

from ezagent.voice.notch.menu_layout import GROUPS
from ezagent.voice.notch.settings import (
    DEFAULT_VALUES,
    EDITABLE_KEYS,
    INSTANT_KEYS,
    SLIDERS,
    TRACKED_KEYS,
    MenuSettings,
)

# Cada fila instantánea con un valor distinto del predeterminado.
CHANGES = {
    "ui_layout": "circle",
    "ui_scale": 1.4,
    "ui_motion": "reduced",
    "theme": "dark",
    "theme_base": "light",
    "theme_primary": "#123456",
    "theme_secondary": "#234567",
    "theme_accent": "#345678",
    "theme_background": "#456789",
    "show_advanced": True,
    "close_action": "hangup",
    "voice_wake": False,
    "wake_phrase": "Hola EzAgent",
}


def _settings() -> MenuSettings:
    return MenuSettings.from_app(dict(DEFAULT_VALUES), devices=[], voices=None)


def test_every_instant_key_is_tracked_and_has_a_test_value() -> None:
    assert set(INSTANT_KEYS) <= set(TRACKED_KEYS)
    assert set(EDITABLE_KEYS) <= set(TRACKED_KEYS)
    assert set(CHANGES) == set(INSTANT_KEYS)


@pytest.mark.parametrize("key", sorted(CHANGES))
def test_changing_an_appearance_setting_activates_save(key: str) -> None:
    settings = _settings()
    assert not settings.dirty()
    settings.set(key, CHANGES[key])
    assert settings.dirty(), key


@pytest.mark.parametrize("key", sorted(CHANGES))
def test_saving_clears_it_and_restoring_brings_it_back(key: str) -> None:
    settings = _settings()
    before = settings.saved_snapshot()
    settings.set(key, CHANGES[key])
    snapshot = settings.snapshot()
    assert key in snapshot  # el paquete de guardado lleva la apariencia
    settings.mark_saved(snapshot)
    assert not settings.dirty()
    settings.restore_saved(before)  # un guardado fallido deja el cambio pendiente
    assert settings.dirty()


def test_putting_a_value_back_deactivates_save_again() -> None:
    settings = _settings()
    original = settings.get("ui_scale")
    settings.set("ui_scale", 1.4)
    assert settings.dirty()
    settings.set("ui_scale", original)
    assert not settings.dirty()


def test_every_visible_row_with_a_real_setting_can_activate_save() -> None:
    """Ninguna fila de Ajustes (salvo botones, medidor y vista previa) queda fuera."""
    non_settings = {"theme_preview", "input_meter", "test_voice", "repeat_last", "new_conversation"}
    untracked = []
    for _group, rows in GROUPS:
        for key, kind in rows:
            if kind == "action" or key in non_settings:
                continue
            if key not in TRACKED_KEYS and key not in {
                "llm_profile",
                "theme_preset",
                "llm_model_pick",
            }:
                untracked.append(key)
    assert untracked == []
    assert "min_rms" in TRACKED_KEYS and "min_rms" in SLIDERS


# ------------------------------------------------------------------- app
def _app_stub(saved: list):
    cfg = SimpleNamespace(save_key=lambda key, value: saved.append((key, value)), theme="light")
    notch = SimpleNamespace(set_bar_scale=lambda value: None, set_theme=lambda value: None)
    return SimpleNamespace(cfg=cfg, notch=notch, _push_custom_colors=lambda: None)


def test_live_changes_from_settings_are_not_written_until_save() -> None:
    app = pytest.importorskip("ezagent.voice.app")
    saved: list = []
    stub = _app_stub(saved)
    app.VoiceApp._apply_instant_setting(stub, "ui_scale", 1.4, persist=False)
    assert stub.cfg.ui_scale == 1.4  # se aplica en vivo
    assert saved == []  # pero no se escribe todavía


def test_other_callers_still_persist_immediately() -> None:
    app = pytest.importorskip("ezagent.voice.app")
    saved: list = []
    stub = _app_stub(saved)
    app.VoiceApp._apply_instant_setting(stub, "ui_scale", 1.4)
    assert saved == [("ui_scale", 1.4)]


def test_menu_live_changes_go_through_the_non_persisting_path() -> None:
    app = pytest.importorskip("ezagent.voice.app")
    calls: list = []
    stub = SimpleNamespace(
        _apply_instant_setting=lambda key, value, push=True, persist=True: calls.append(
            (key, persist)
        )
    )
    app.VoiceApp._apply_live_setting(stub, "ui_scale", 1.4)
    app.VoiceApp._apply_live_setting(stub, "theme", "dark")
    assert calls == [("ui_scale", False), ("theme", False)]


@pytest.mark.parametrize(
    ("key", "value"),
    [
        ("tts_model_variant", "fp32"),
        ("stt_engine", "cpu"),
        ("llm_api_style", "anthropic"),
        ("llm_reasoning_effort", "high"),
        ("input_device_index", 1),
        ("voice", "ef_dora"),
        ("llm_model", "otro-modelo"),
        ("api_key", "clave"),
        ("hotkey", "ctrl+alt+j"),
    ],
)
def test_selectors_texts_and_hotkeys_activate_save(key: str, value) -> None:
    devices = [{"index": 0, "name": "Mic A"}, {"index": 1, "name": "Mic B"}]
    settings = MenuSettings.from_app(
        dict(DEFAULT_VALUES, input_device_index=0), devices=devices, voices=None
    )
    assert not settings.dirty()
    settings.set(key, value)
    assert settings.dirty(), key
