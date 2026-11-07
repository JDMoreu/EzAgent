"""Atajos configurables desde Ajustes."""

from __future__ import annotations

import queue
from types import SimpleNamespace

import pytest

from ezagent.voice.app import HOTKEY_KEYS, VoiceApp
from ezagent.voice.notch import settings as S


def test_los_atajos_estan_en_el_menu_y_tienen_etiqueta():
    for key in HOTKEY_KEYS:
        assert key in S.EDITABLE_KEYS
        assert key in S.TEXT_KEYS
        assert key in S.LABELS
        assert key in S.DEFAULT_VALUES


def test_no_se_permite_repetir_atajos():
    values = {key: "" for key in HOTKEY_KEYS}
    values["hotkey"] = "ctrl+alt+space"
    values["mute_hotkey"] = " ctrl + alt + space "
    with pytest.raises(ValueError):
        VoiceApp._check_hotkey_duplicates(values)


def test_atajos_unicos_se_aceptan():
    values = {key: "" for key in HOTKEY_KEYS}
    values["hotkey"] = "ctrl+alt+space"
    values["mute_hotkey"] = "ctrl+alt+m"
    VoiceApp._check_hotkey_duplicates(values)


def test_build_hotkeys_usa_ids_unicos_y_respeta_los_specs():
    app = VoiceApp.__new__(VoiceApp)
    app.cfg = SimpleNamespace(
        hotkey="ctrl+alt+space",
        cancel_hotkey="ctrl+alt+e",
        repeat_hotkey="ctrl+alt+r",
        mute_hotkey="ctrl+alt+m",
        menu_hotkey="ctrl+alt+o",
    )
    app.hotkey_events = queue.Queue()
    hotkeys = VoiceApp._build_hotkeys(app)
    ids = [hotkey.hk_id for hotkey in hotkeys]
    assert len(ids) == len(set(ids))
    specs = {hotkey.spec for hotkey in hotkeys}
    assert "ctrl+alt+space" in specs
    assert "ctrl+alt+e" in specs
    assert "ctrl+alt+c" in specs
    assert "ctrl+alt+y" in specs


def test_live_setting_valido_reload():
    app = VoiceApp.__new__(VoiceApp)
    app.cfg = SimpleNamespace(
        hotkey="ctrl+alt+space",
        cancel_hotkey="ctrl+alt+e",
        repeat_hotkey="ctrl+alt+r",
        mute_hotkey="ctrl+alt+m",
        menu_hotkey="ctrl+alt+o",
    )
    calls: list[str] = []
    app._reload_hotkeys = lambda: calls.append("reload")
    VoiceApp._apply_live_setting(app, "hotkey", "ctrl+alt+h")
    assert app.cfg.hotkey == "ctrl+alt+h"
    assert calls == ["reload"]


def test_live_setting_invalido_no_cambia_ni_reload():
    app = VoiceApp.__new__(VoiceApp)
    app.cfg = SimpleNamespace(
        hotkey="ctrl+alt+space",
        cancel_hotkey="ctrl+alt+e",
        repeat_hotkey="ctrl+alt+r",
        mute_hotkey="ctrl+alt+m",
        menu_hotkey="ctrl+alt+o",
    )
    calls: list[str] = []
    app._reload_hotkeys = lambda: calls.append("reload")
    VoiceApp._apply_live_setting(app, "hotkey", "ctrl+nada")
    assert app.cfg.hotkey == "ctrl+alt+space"
    assert calls == []


def test_reload_para_los_antiguos_y_arranca_los_nuevos():
    class _Hotkey:
        def __init__(self, name):
            self.name = name
            self.stopped = False
            self.started = False

        def stop(self, wait=False):
            self.stopped = wait

        def start(self):
            self.started = True

    app = VoiceApp.__new__(VoiceApp)
    old = _Hotkey("old")
    new = _Hotkey("new")
    app.hotkeys = [old]
    app._build_hotkeys = lambda: [new]
    scheduled: list[tuple] = []
    app.root = SimpleNamespace(
        after=lambda delay, callback: scheduled.append((delay, callback))
    )
    app._report_hotkeys = lambda: None
    VoiceApp._reload_hotkeys(app)
    assert old.stopped is True
    assert new.started is True
    assert app.hotkeys == [new]
    assert scheduled and scheduled[0][0] == 150
