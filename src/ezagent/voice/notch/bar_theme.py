"""Colours shared by the bar and its settings card: one light and one dark theme.

The surfaces follow the «Marea» identity of the bar: a white with a hint of
mint in light, a deep teal night in dark, and a teal accent in both (deep teal
on light, bright aqua on dark). The card body carries a very soft vertical
gradient (a little more mint at the top) so it feels less flat.

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
        "BODY": (250, 253, 252),
        "GROUP": (237, 246, 244),
        "GROUP_HOVER": (224, 239, 236),
        "TRACK": (199, 224, 219),
        "TEXT": (16, 34, 38),
        "TEXT_2": (56, 82, 86),
        "TEXT_3": (78, 102, 106),
        "LINE": (16, 34, 38, 20),
        "MINT": (12, 112, 117),
        "MINT_BTN": (12, 112, 117),
        "MINT_INK": (255, 255, 255),
        "WORD_MINT": (12, 112, 117),
    },
    "dark": {
        "BODY": (13, 22, 26),
        "GROUP": (20, 33, 38),
        "GROUP_HOVER": (28, 45, 51),
        "TRACK": (44, 70, 76),
        "TEXT": (228, 242, 240),
        "TEXT_2": (172, 196, 194),
        "TEXT_3": (138, 164, 163),
        "LINE": (228, 242, 240, 24),
        "MINT": (94, 226, 196),
        "MINT_BTN": (94, 226, 196),
        "MINT_INK": (6, 34, 30),
        "WORD_MINT": (94, 226, 196),
    },
}
# Top of the card body gradient (the bottom is BODY).
BODY_TOP: dict[str, Color] = {"light": (238, 249, 246), "dark": (17, 30, 34)}


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
