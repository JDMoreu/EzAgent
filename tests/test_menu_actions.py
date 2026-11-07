"""Acciones del menú que abren archivos del usuario."""

from __future__ import annotations

import os
from pathlib import Path

from ezagent.voice import app as app_module
from ezagent.voice.app import VoiceApp


def test_abrir_system_md_lo_crea_y_lo_lanza(tmp_path, monkeypatch):
    voice_path = tmp_path / "voice.json"
    monkeypatch.setattr(app_module, "config_path", lambda: voice_path)
    opened: list[str] = []
    monkeypatch.setattr(
        os, "startfile", lambda path: opened.append(str(path))
    )
    app = VoiceApp.__new__(VoiceApp)
    app.agent = None
    VoiceApp._open_system_md(app)
    target = tmp_path / "system.md"
    assert target.exists()
    assert "# Identidad" in target.read_text(encoding="utf-8")
    assert opened == [str(target)]


def test_menu_action_open_system_md(tmp_path, monkeypatch):
    calls: list[str] = []
    app = VoiceApp.__new__(VoiceApp)
    app._open_system_md = lambda: calls.append("open")
    VoiceApp._menu_action_ui(app, "open_system_md")
    assert calls == ["open"]


def test_abrir_voice_json_lo_crea_y_lo_lanza(tmp_path, monkeypatch):
    config_file = tmp_path / "voice.json"
    opened: list[str] = []
    monkeypatch.setattr(
        os, "startfile", lambda path: opened.append(str(path))
    )
    app = VoiceApp.__new__(VoiceApp)
    app.config_file = config_file
    VoiceApp._open_voice_json(app)
    assert config_file.exists()
    assert opened == [str(config_file)]
