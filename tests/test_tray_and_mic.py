"""Bandeja del sistema, micrófono dinámico y píldora sobre la taskbar."""

from __future__ import annotations

import os
import subprocess
import sys
import threading
from types import SimpleNamespace

import pytest

from ezagent.voice import tray as tray_module
from ezagent.voice.app import IDLE, VoiceApp
from ezagent.voice.notch import layered
from ezagent.voice.notch.drop import DropUI


def _notch() -> DropUI:
    return DropUI(root=None)


def test_clic_en_mic_apagado_pide_reactivar_la_escucha():
    notch = _notch()
    actions: list[str] = []
    notch.on_action = actions.append
    notch.set_in_call(False)
    notch._activate("mic")
    assert actions == ["listen"]


def test_clic_en_mic_activo_pide_mute():
    notch = _notch()
    actions: list[str] = []
    notch.on_action = actions.append
    notch.set_in_call(True)
    notch._activate("mic")
    assert actions == ["mute"]


@pytest.mark.skipif(os.name != "nt", reason="Win32")
def test_set_pos_reafirma_el_topmost_sin_nozorder(monkeypatch):
    calls: list[tuple] = []

    class _User32:
        @staticmethod
        def SetWindowPos(*args):
            calls.append(args)
            return 1

    win = object.__new__(layered.LayeredWindow)
    win.x = 10
    win.y = 20
    win.hwnd = 123
    win._lock = threading.RLock()
    win._dirty = False
    monkeypatch.setattr(layered, "_u32", _User32())
    win.set_pos(10, 20)
    assert calls
    flags = calls[0][-1]
    assert not (flags & layered._SWP_NOZORDER)
    assert calls[0][1] is layered._HWND_TOPMOST


class _FakeMenuItem:
    def __init__(self, text, action, default=False):
        self.text = text
        self.action = action
        self.default = default


class _FakeMenu:
    SEPARATOR = object()

    def __init__(self, *items):
        self.items = items


class _FakeIcon:
    def __init__(self, name, image, title, menu):
        self.name = name
        self.image = image
        self.title = title
        self.menu = menu
        self.stopped = False

    def run(self):
        return None

    def stop(self):
        self.stopped = True


def _fake_pystray():
    return SimpleNamespace(
        Menu=_FakeMenu,
        MenuItem=_FakeMenuItem,
        Icon=_FakeIcon,
    )


def test_tray_construye_el_menu_y_despacha(monkeypatch):
    monkeypatch.setattr(tray_module, "_pystray", _fake_pystray())
    actions: list[str] = []
    icon = tray_module.TrayIcon(actions.append)
    assert icon.start() is True
    assert icon.running
    menu = icon._icon.menu
    labels = [
        item.text for item in menu.items if hasattr(item, "text")
    ]
    assert labels == [
        "Configurar",
        "Cambiar forma (círculo / píldora)",
        "Reiniciar EzAgent",
        "Cerrar",
    ]
    for item in menu.items:
        if hasattr(item, "action"):
            item.action(None, None)
    assert actions == ["config", "shape", "restart", "quit"]
    icon.stop()
    assert icon._icon is None


def test_tray_sin_pystray_no_rompe(monkeypatch):
    monkeypatch.setattr(tray_module, "_pystray", None)
    icon = tray_module.TrayIcon(lambda action: None)
    assert icon.start() is False
    assert icon.available is False


def test_icono_de_respaldo_es_cuadrado():
    image = tray_module.load_icon_image(None)
    assert image.size == (tray_module.ICON_SIZE, tray_module.ICON_SIZE)


def test_tray_config_muestra_el_notch_y_abre_ajustes():
    app = VoiceApp.__new__(VoiceApp)
    app._notch_hidden = True
    app.state = IDLE
    app.in_call = False
    calls: list[tuple] = []

    class _Cfg:
        notch_hidden = True

        @staticmethod
        def save_key(key, value):
            calls.append(("save", key, value))

    class _Notch:
        def show(self, state):
            calls.append(("show", state))

        def open_menu(self):
            calls.append(("menu",))

    app.cfg = _Cfg()
    app.notch = _Notch()
    VoiceApp._tray_action_ui(app, "config")
    assert app._notch_hidden is False
    assert calls == [
        ("save", "notch_hidden", False),
        ("show", IDLE),
        ("menu",),
    ]


def test_tray_cierra_y_reinicia():
    app = VoiceApp.__new__(VoiceApp)
    calls: list[str] = []

    class _Root:
        def quit(self):
            calls.append("quit")

    app.root = _Root()
    app._restart_app = lambda: calls.append("restart")
    VoiceApp._tray_action_ui(app, "restart")
    VoiceApp._tray_action_ui(app, "quit")
    assert calls == ["restart", "quit"]


def test_mic_apagado_reactiva_la_escucha_y_quita_el_mute():
    app = VoiceApp.__new__(VoiceApp)
    app.muted = True
    app.in_call = False
    calls: list[tuple] = []
    app._set_mic_muted = lambda value: calls.append(("muted", value))
    app._start_call = lambda: calls.append(("start",))
    app._set_state = lambda state: calls.append(("state", state))
    VoiceApp._resume_listening_from_mic(app)
    assert calls == [("muted", False), ("start",)]


def test_notch_action_listen_delega_en_el_mic():
    app = VoiceApp.__new__(VoiceApp)
    calls: list[str] = []
    app._resume_listening_from_mic = lambda: calls.append("listen")
    VoiceApp._notch_action_ui(app, "listen")
    assert calls == ["listen"]


def test_spawn_restart_en_codigo_fuente(monkeypatch):
    app = VoiceApp.__new__(VoiceApp)
    calls: list[tuple] = []

    def fake_popen(command, **kwargs):
        calls.append((command, kwargs))
        return object()

    monkeypatch.setattr(subprocess, "Popen", fake_popen)
    monkeypatch.setattr(sys, "frozen", False, raising=False)
    assert VoiceApp._spawn_restart(app) is True
    command, kwargs = calls[0]
    assert command[0] == sys.executable
    assert command[1:3] == ["-m", "ezagent.voice.app"]
    assert kwargs["close_fds"] is True
