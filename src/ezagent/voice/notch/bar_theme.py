"""Colours shared by the bar and its settings card: one light and one dark theme.

The card used the old island palette (warm off-white, warm brown and a lime
accent). It now takes fresh, airy surfaces that match the bar: a white with a
hint of sky in light, a night blue in dark, and a sky-blue accent in both. The
card body carries a very soft vertical gradient (a little more sky at the top)
so it feels less flat.

Contrast targets (WCAG): text and accent text >= 4.5:1 on the card surfaces,
accent buttons with their ink >= 4.5:1. ``tests/test_menu_appearance.py``
checks them.
"""

from __future__ import annotations

from .tokens import Color
from .tokens import palette as token_palette

CARD_RADIUS = 22  # rounder than the old 18 px card: softer, more organic

_FRESH: dict[str, dict[str, Color]] = {
    "light": {
        "BODY": (250, 252, 255),
        "GROUP": (238, 244, 253),
        "GROUP_HOVER": (227, 236, 251),
        "TRACK": (205, 218, 240),
        "TEXT": (22, 30, 52),
        "TEXT_2": (66, 78, 108),
        "TEXT_3": (98, 110, 140),
        "LINE": (22, 30, 52, 20),
        "MINT": (37, 90, 210),
        "MINT_BTN": (47, 95, 208),
        "MINT_INK": (255, 255, 255),
        "WORD_MINT": (37, 90, 210),
    },
    "dark": {
        "BODY": (17, 22, 38),
        "GROUP": (26, 33, 54),
        "GROUP_HOVER": (35, 44, 70),
        "TRACK": (52, 64, 98),
        "TEXT": (232, 238, 250),
        "TEXT_2": (176, 188, 214),
        "TEXT_3": (140, 153, 184),
        "LINE": (232, 238, 250, 24),
        "MINT": (141, 180, 255),
        "MINT_BTN": (141, 180, 255),
        "MINT_INK": (14, 22, 44),
        "WORD_MINT": (141, 180, 255),
    },
}
# Top of the card body gradient (the bottom is BODY).
BODY_TOP: dict[str, Color] = {"light": (241, 246, 255), "dark": (22, 29, 50)}


def _theme(theme: str) -> str:
    return "light" if theme == "light" else "dark"


def card_overrides(theme: str) -> dict[str, Color]:
    """Keys that replace the token palette in the card for ``theme``."""
    return dict(_FRESH[_theme(theme)])


def card_palette(theme: str) -> dict[str, Color]:
    """Full card palette: token palette plus the fresh surfaces."""
    base = dict(token_palette(_theme(theme)))
    base.update(card_overrides(theme))
    return base


def body_top(theme: str) -> Color:
    return BODY_TOP[_theme(theme)]


def _luminance(rgb: Color) -> float:
    def channel(v: float) -> float:
        c = v / 255.0
        return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4

    r, g, b = (channel(float(x)) for x in rgb[:3])
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(a: Color, b: Color) -> float:
    """WCAG contrast ratio between two colours."""
    la, lb = _luminance(a), _luminance(b)
    hi, lo = max(la, lb), min(la, lb)
    return (hi + 0.05) / (lo + 0.05)
