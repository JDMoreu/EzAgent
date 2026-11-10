"""Atajos en ajustes: captura de la combinación pulsada, en vivo y sin escribir texto."""

from __future__ import annotations

import numpy as np
import pytest

from ezagent.voice.hotkeys import parse_hotkey
from ezagent.voice.notch import hotkey_field as hk
from ezagent.voice.notch import settings_window as sw
from ezagent.voice.notch.menu import Menu
from ezagent.voice.notch.settings import DEFAULT_VALUES, LABELS, MenuSettings

VK_E, VK_K, VK_F8, VK_SPACE, VK_ESC, VK_CTRL, VK_SEMICOLON = 69, 75, 119, 32, 27, 17, 186


# ------------------------------------------------------------ lógica pura
def test_a_modifier_alone_waits_for_the_final_key() -> None:
    assert hk.press(VK_CTRL, frozenset({"ctrl"})).kind == "pending"


def test_esc_cancels_and_cannot_be_recorded() -> None:
    assert hk.press(VK_ESC, frozenset()).kind == "cancel"
    assert hk.press(VK_ESC, frozenset({"ctrl"})).kind == "reject"  # Ctrl+Esc abre Inicio


@pytest.mark.parametrize(
    ("vk", "mods", "spec"),
    [
        (VK_K, {"ctrl", "alt"}, "ctrl+alt+k"),
        (VK_SPACE, {"alt", "ctrl"}, "ctrl+alt+space"),
        (VK_K, {"shift", "ctrl", "win"}, "ctrl+shift+win+k"),
        (VK_F8, set(), "f8"),
    ],
)
def test_a_complete_combination_is_closed_in_a_fixed_order(vk, mods, spec) -> None:
    event = hk.press(vk, frozenset(mods))
    assert (event.kind, event.spec) == ("done", spec)


@pytest.mark.parametrize(
    ("vk", "mods"), [(VK_K, set()), (VK_K, {"shift"}), (VK_SEMICOLON, {"ctrl"})]
)
def test_combinations_that_would_break_typing_or_have_no_name_are_rejected(vk, mods) -> None:
    event = hk.press(vk, frozenset(mods))
    assert event.kind == "reject" and event.reason


def test_everything_the_field_records_is_registrable_by_the_app() -> None:
    sample = [*range(65, 91), *range(48, 58), *range(112, 136), 32, 13, 9, 8, 46, 45]
    sample += [36, 35, 33, 34, 38, 40, 37, 39]
    for vk in sample:
        spec = hk.press(vk, frozenset({"ctrl", "alt"})).spec
        mods, key = parse_hotkey(spec)
        assert key == vk, spec
        assert mods & 0x0001 and mods & 0x0002


def test_default_hotkeys_read_back_as_keycaps() -> None:
    assert hk.parts_for("ctrl+alt+space") == ["Ctrl", "Alt", "Espacio"]
    assert hk.parts_for("control+shift+F5") == ["Ctrl", "Mayús", "F5"]
    for key in hk.HOTKEY_KEYS:
        assert hk.canonical(DEFAULT_VALUES[key]) is not None
        assert key in LABELS


def test_conflicts_ignore_order_case_and_aliases() -> None:
    current = {"hotkey": "ctrl+alt+space", "cancel_hotkey": "ctrl+alt+e"}
    labels = {"hotkey": "Llamada", "cancel_hotkey": "Colgar"}
    assert hk.conflict("Alt+Control+E", current, "hotkey", labels) == "Colgar"
    assert hk.conflict("ctrl+alt+e", current, "cancel_hotkey", labels) is None
    assert hk.conflict("ctrl+alt+q", current, "hotkey", labels) is None


# ------------------------------------------------------------------- menú
def _menu() -> tuple[Menu, list]:
    events: list = []
    menu = Menu(
        MenuSettings.from_app(dict(DEFAULT_VALUES), devices=[], voices=None),
        scale=1.0,
        theme="dark",
        on_live_change=lambda key, value: events.append(("live", key, value)),
        on_text_edit=lambda active: events.append(("text", active)),
        on_hotkey_capture=lambda active: events.append(("capture", active)),
    )
    menu.set_reduced_motion(True)
    cw, vh = sw.content_size(760, 620)
    menu.set_width(cw)
    menu.set_max_height(vh)
    menu.set_groups(sw.section_groups("system"), "Sistema", 0.0)
    menu.open(0.0)
    menu.update(1.0)
    return menu, events


def test_clicking_a_hotkey_row_starts_capturing_and_frees_the_global_hotkeys() -> None:
    menu, events = _menu()
    menu.begin_text_edit("hotkey", 1.0)
    assert menu.editing_key == "hotkey" and menu._capturing
    assert ("capture", True) in events and ("text", True) in events


def test_pressing_a_combination_applies_it_live_and_stops_capturing() -> None:
    menu, events = _menu()
    menu.begin_text_edit("hotkey", 1.0)
    menu.modifier_source = lambda: frozenset({"ctrl", "alt"})
    assert menu.text_key(VK_K) is True
    assert menu.settings.get("hotkey") == "ctrl+alt+k"
    assert not menu._capturing and menu.editing_key is None
    assert ("live", "hotkey", "ctrl+alt+k") in events
    assert events[-1] == ("capture", False)


def test_a_duplicate_is_refused_and_the_field_keeps_listening() -> None:
    menu, events = _menu()
    taken = menu.settings.get("cancel_hotkey")
    menu.begin_text_edit("hotkey", 1.0)
    mods, key = hk.canonical(taken)
    menu.modifier_source = lambda: mods
    menu.text_key(ord(key.upper()))
    assert menu._capturing
    assert menu.settings.get("hotkey") == DEFAULT_VALUES["hotkey"]
    assert menu._capture_msg is not None and "Ya la usa" in menu._capture_msg[0]
    assert not any(event[0] == "live" for event in events)


def test_esc_cancels_without_changes_and_typing_is_swallowed() -> None:
    menu, events = _menu()
    menu.begin_text_edit("repeat_hotkey", 1.0)
    before = menu.settings.get("repeat_hotkey")
    menu.modifier_source = lambda: frozenset()
    assert menu.text_input("x") is True and menu.paste_text("abc") is True
    assert menu.text_key(VK_ESC) is True
    assert not menu._capturing
    assert menu.settings.get("repeat_hotkey") == before
    assert events[-1] == ("capture", False)


def test_closing_the_menu_ends_the_capture() -> None:
    menu, events = _menu()
    menu.begin_text_edit("mute_hotkey", 1.0)
    menu.close(1.5)
    assert not menu._capturing and events[-1] == ("capture", False)


def _frame(menu: Menu, now: float) -> np.ndarray:
    buf = np.zeros((int(menu.viewport_height) + 200, int(menu.width), 4), np.float32)
    menu.draw(buf, now)
    return buf


def test_every_state_of_the_field_draws() -> None:
    menu, _events = _menu()
    idle = _frame(menu, 1.0)
    menu.begin_text_edit("hotkey", 1.0)
    menu.modifier_source = lambda: frozenset()
    waiting = _frame(menu, 1.1)
    menu.modifier_source = lambda: frozenset({"ctrl", "shift"})
    menu.update(1.2)
    holding = _frame(menu, 1.2)
    menu._capture_msg = ("Combínala con Ctrl, Alt o Win", 9.0)
    refused = _frame(menu, 1.3)
    for frame in (idle, waiting, holding, refused):
        assert frame[..., 3].max() > 0
    assert not np.array_equal(idle, waiting)
    assert not np.array_equal(waiting, holding)
    assert not np.array_equal(holding, refused)
