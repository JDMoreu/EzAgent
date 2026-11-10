"""Sección Avanzado: el interruptor dice qué hace y los botones abren los archivos reales."""

from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

import numpy as np
import pytest

from ezagent.voice.notch import settings_window as sw
from ezagent.voice.notch.menu import ACTION_KEYS, Menu
from ezagent.voice.notch.menu_layout import ADVANCED_ROWS, GROUP_LABELS, GROUPS
from ezagent.voice.notch.settings import DEFAULT_VALUES, LABELS, MenuSettings

FILE_ACTIONS = ("open_config_dir", "open_system_md", "open_permissions", "open_voice_json")


def _menu(show_advanced: bool) -> Menu:
    values = dict(DEFAULT_VALUES, show_advanced=show_advanced)
    menu = Menu(MenuSettings.from_app(values, devices=[], voices=None), scale=1.0, theme="dark")
    menu.set_reduced_motion(True)
    cw, vh = sw.content_size(760, 620)
    menu.set_width(cw)
    menu.set_max_height(vh)
    menu.set_groups(sw.section_groups("system"), "Sistema", 0.0)
    menu.open(0.0)
    menu.update(1.0)
    return menu


def _texts(menu: Menu) -> list[str]:
    drawn: list[str] = []
    original = menu._text_at

    def spy(layer, text, *args, **kwargs):
        drawn.append(str(text))
        return original(layer, text, *args, **kwargs)

    menu._text_at = spy  # type: ignore[method-assign]
    buf = np.zeros((int(menu.viewport_height) + 200, int(menu.width), 4), np.float32)
    menu.draw(buf, 1.0)
    return drawn


def test_the_toggle_says_how_many_settings_it_unlocks() -> None:
    count = len(ADVANCED_ROWS)
    assert f"+{count} ajustes" in _texts(_menu(False))
    assert f"{count} visibles" in _texts(_menu(True))


def test_files_group_lists_every_file_action_inside_system() -> None:
    rows = dict(GROUPS)["files"]
    assert tuple(key for key, _kind in rows) == FILE_ACTIONS
    assert "files" in GROUP_LABELS
    names = [name for name, _rows in sw.section_groups("system")]
    assert names[:2] == ["hotkeys", "advanced"] and names[-1] == "files"
    assert {"adv_listen", "adv_voice", "adv_stt", "adv_llm"} <= set(names)
    for key in FILE_ACTIONS:
        assert key in ACTION_KEYS and key in LABELS


def test_every_file_action_is_handled_by_the_app() -> None:
    app = pytest.importorskip("ezagent.voice.app")
    called: list[str] = []
    stub = SimpleNamespace(
        _open_config_dir=lambda: called.append("open_config_dir"),
        _open_system_md=lambda: called.append("open_system_md"),
        _open_permissions=lambda: called.append("open_permissions"),
        _open_voice_json=lambda: called.append("open_voice_json"),
    )
    for action in FILE_ACTIONS:
        app.VoiceApp._menu_action_ui(stub, action)
    assert called == list(FILE_ACTIONS)


def _runtime(app_dir: Path, permissions: Path | None = None):
    config = SimpleNamespace(app_dir=app_dir, permissions_path=permissions)
    return SimpleNamespace(config=config)


def test_permissions_opens_the_file_the_agent_runtime_reads(monkeypatch, tmp_path: Path) -> None:
    app = pytest.importorskip("ezagent.voice.app")
    permissions = tmp_path / "permissions.toml"
    permissions.write_text("[general]\n", encoding="utf-8")
    opened: list[str] = []
    monkeypatch.setattr(app.os, "startfile", opened.append, raising=False)
    stub = SimpleNamespace(agent=_runtime(tmp_path, permissions))
    app.VoiceApp._open_permissions(stub)
    assert opened == [str(permissions)]


def test_a_missing_permissions_file_is_neither_created_nor_opened(
    monkeypatch, tmp_path: Path
) -> None:
    app = pytest.importorskip("ezagent.voice.app")
    opened: list[str] = []
    monkeypatch.setattr(app.os, "startfile", opened.append, raising=False)
    missing = tmp_path / "permissions.toml"
    stub = SimpleNamespace(agent=_runtime(tmp_path, missing))
    app.VoiceApp._open_permissions(stub)
    assert opened == [] and not missing.exists()


def test_the_data_folder_is_the_agent_app_dir(monkeypatch, tmp_path: Path) -> None:
    app = pytest.importorskip("ezagent.voice.app")
    opened: list[str] = []
    monkeypatch.setattr(app.os, "startfile", opened.append, raising=False)
    stub = SimpleNamespace(agent=_runtime(tmp_path))
    stub._app_dir = lambda: app.VoiceApp._app_dir(stub)
    app.VoiceApp._open_config_dir(stub)
    assert opened == [str(tmp_path)]
