"""Tema personalizado: la paleta elegida tiñe también chat, ajustes, botones y avisos.

The shipped Marea palette (``tokens.LIGHT`` / ``tokens.DARK``) is re-tinted, not
replaced, so contrast and hierarchy survive any preset:

* surfaces and secondary text keep their lightness and their (small) saturation
  but take the hue of the palette's background colour, the way Marea tints
  everything teal;
* the accent (the «mint» keys) takes the hue of the primary colour, and its
  lightness is nudged until it reads at 4.5:1 on the body of each theme; the
  ink drawn on accent buttons is checked the same way;
* warm and error colours keep their meaning and are left alone.
"""

from __future__ import annotations

import colorsys
from typing import Final

from .a11y import contrast
from .tokens import DARK, LIGHT, Color

SURFACE_KEYS: Final = ("BODY", "GROUP", "GROUP_HOVER", "TRACK", "TEXT", "TEXT_2", "TEXT_3")
LINE_KEYS: Final = ("LINE", "LINE_2")
ACCENT_KEYS: Final = ("MINT", "MINT_BTN", "WORD_MINT", "EYE_IDLE", "WORD_IDLE")
BODY_TOP: Final[dict[str, Color]] = {"light": (238, 249, 246), "dark": (17, 30, 34)}
MIN_CONTRAST: Final = 4.5  # texto de acento y su tinta sobre botones
DARK_ACCENT_CONTRAST: Final = 6.0  # en oscuro el acento debe brillar, no solo leerse
ACCENT_START: Final = 0.62  # luminosidad desde la que se busca: el acento más vivo que se lee


def parse_hex(value: str) -> tuple[int, int, int] | None:
    text = str(value or "").strip().lstrip("#")
    if len(text) != 6:
        return None
    try:
        return (int(text[0:2], 16), int(text[2:4], 16), int(text[4:6], 16))
    except ValueError:
        return None


def _hls(rgb: tuple[int, ...]) -> tuple[float, float, float]:
    return colorsys.rgb_to_hls(*(c / 255.0 for c in rgb[:3]))


def _rgb(h: float, light: float, s: float) -> tuple[int, int, int]:
    r, g, b = colorsys.hls_to_rgb(h % 1.0, max(0.0, min(1.0, light)), max(0.0, min(1.0, s)))
    return (round(r * 255), round(g * 255), round(b * 255))


def _rehue(color: Color, hue: float) -> Color:
    """El mismo color con otro tono: conserva luminosidad, saturación y alfa."""
    _, light, sat = _hls(color)
    out = _rgb(hue, light, sat)
    return (*out, color[3]) if len(color) == 4 else out


def _readable(
    hue: float,
    sat: float,
    start: float,
    against: Color,
    darker: bool,
    target: float = MIN_CONTRAST,
) -> Color:
    """Acento con ``hue``/``sat``: desde ``start``, el primero que llega a ``target`` sobre ``against``."""
    light = start
    for _ in range(60):
        color = _rgb(hue, light, sat)
        if contrast(color, against[:3]) >= target:
            return color
        light += -0.01 if darker else 0.01
    return _rgb(hue, 0.15 if darker else 0.85, sat)


def derive(primary: str, background: str) -> dict[str, dict[str, Color]] | None:
    """Sobrescrituras de la paleta para ``light`` y ``dark``; None si los colores no valen."""
    main, ground = parse_hex(primary), parse_hex(background)
    if main is None or ground is None:
        return None
    accent_hue, _, accent_sat = _hls(main)
    surface_hue = _hls(ground)[0]
    accent_sat = max(0.45, min(0.85, accent_sat))
    out: dict[str, dict[str, Color]] = {}
    for theme, base in (("light", LIGHT), ("dark", DARK)):
        pal: dict[str, Color] = {k: _rehue(base[k], surface_hue) for k in SURFACE_KEYS + LINE_KEYS}
        dark = theme == "dark"
        target = DARK_ACCENT_CONTRAST if dark else MIN_CONTRAST
        accent = _readable(
            accent_hue, accent_sat, ACCENT_START, pal["BODY"], darker=not dark, target=target
        )
        for key in ACCENT_KEYS:
            pal[key] = accent
        if dark:  # tinta oscura sobre el acento brillante, del mismo tono
            pal["MINT_INK"] = _readable(accent_hue, 0.6, 0.12, accent, darker=True)
        else:
            pal["MINT_INK"] = (255, 255, 255)
        pal["BODY_TOP"] = _rehue(BODY_TOP[theme], surface_hue)
        out[theme] = pal
    return out


__all__ = ["derive", "parse_hex"]
