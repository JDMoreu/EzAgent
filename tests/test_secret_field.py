"""Clave API en ajustes: nunca se pinta; un botón copia el valor al portapapeles."""

from __future__ import annotations

import numpy as np
import pytest

from ezagent.voice.notch import settings_window as sw
from ezagent.voice.notch.icons import draw_icon
from ezagent.voice.notch.menu import COPIED_SECS, Menu
from ezagent.voice.notch.settings import DEFAULT_VALUES, MenuSettings

KEY = "sk-test-SECRET-0123456789"


def _menu(api_key: str = KEY, copies: list[str] | None = None, ok: bool = True) -> Menu:
    values = dict(DEFAULT_VALUES, api_key=api_key)
    log = copies if copies is not None else []

    def on_copy(text: str) -> bool:
        log.append(text)
        return ok

    menu = Menu(
        MenuSettings.from_app(values, devices=[], voices=None),
        scale=1.0,
        theme="dark",
        on_copy=on_copy,
    )
    menu.set_reduced_motion(True)
    menu.set_width(sw.content_size(760, 620)[0])
    menu.set_max_height(sw.content_size(760, 620)[1])
    menu.set_groups(sw.section_groups("models"), "Modelos", 0.0)
    menu.open(0.0)
    menu.update(1.0)
    return menu


def _button_center(menu: Menu) -> tuple[float, float]:
    row = menu._layout()["api_key"]
    return menu._copy_rect(row.y0 - menu.scroll_offset).center


def _draw(menu: Menu, now: float = 1.0) -> list[str]:
    """Dibuja el menú y devuelve todos los textos que pidió pintar."""
    drawn: list[str] = []
    original = menu._text_at

    def spy(layer, text, *args, **kwargs):
        drawn.append(str(text))
        return original(layer, text, *args, **kwargs)

    menu._text_at = spy  # type: ignore[method-assign]
    buf = np.zeros((int(menu.viewport_height) + 200, int(menu.width), 4), np.float32)
    menu.draw(buf, now)
    return drawn


def test_clicking_the_button_copies_the_exact_key() -> None:
    copies: list[str] = []
    menu = _menu(copies=copies)
    x, y = _button_center(menu)
    hit = menu.hit(x, y, 1.0)
    assert hit is not None and hit.kind == "copy" and hit.key == "api_key"
    menu.pointer_down(x, y, 1.0)
    menu.pointer_up(x, y, 1.0)
    assert copies == [KEY]


def test_button_shows_a_check_for_a_moment_then_goes_back() -> None:
    menu = _menu()
    x, y = _button_center(menu)
    menu.pointer_down(x, y, 1.0)
    menu.pointer_up(x, y, 1.0)
    assert "api_key" in menu._copied
    assert 1.0 < menu._copied["api_key"] <= 1.0 + COPIED_SECS + 1e-6
    # Mientras dura el check el menú pide repintar; pasado el tiempo, ya no por esto.
    assert menu.is_animating(1.0 + COPIED_SECS / 2)
    assert not menu.is_animating(1.0 + COPIED_SECS + 1.0)


def test_failed_copy_does_not_show_the_check() -> None:
    menu = _menu(ok=False)
    x, y = _button_center(menu)
    menu.pointer_down(x, y, 1.0)
    menu.pointer_up(x, y, 1.0)
    assert menu._copied == {}


def test_no_button_without_a_key() -> None:
    copies: list[str] = []
    menu = _menu(api_key="", copies=copies)
    x, y = _button_center(menu)
    hit = menu.hit(x, y, 1.0)
    assert hit is None or hit.kind != "copy"
    menu.pointer_down(x, y, 1.0)
    menu.pointer_up(x, y, 1.0)
    assert copies == []


def test_the_key_is_never_drawn_as_text() -> None:
    menu = _menu()
    texts = _draw(menu)
    assert not any(KEY in text or KEY[-4:] in text for text in texts)
    assert not any("Configurada" in text for text in texts)


def test_the_key_is_not_drawn_while_editing_either() -> None:
    menu = _menu()
    menu.begin_text_edit("api_key", 1.0)
    menu.paste_text("sk-another-SECRET-value")
    texts = _draw(menu, 1.1)
    assert not any("SECRET" in text for text in texts)


def test_empty_key_shows_a_plain_placeholder() -> None:
    texts = _draw(_menu(api_key=""))
    assert "Sin clave" in texts


def test_copy_icon_has_ink() -> None:
    buf = np.zeros((40, 40, 4), np.float32)
    draw_icon(buf, "copy", 20.0, 20.0, 16.0, (255, 255, 255), 1.0)
    assert buf[..., 3].max() > 0.5
    with pytest.raises(ValueError):
        draw_icon(buf, "no-existe", 20.0, 20.0, 16.0, (255, 255, 255), 1.0)
