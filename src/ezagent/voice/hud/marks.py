"""Marcas sobre la forma de la barra: la órbita de actividad.

(El micrófono en pausa no se marca aquí: ``drop.render_model`` vuelve gris la
forma y la encoge un poco, sin iconos ni bordes de color.)

The orbit replaces the old numbered badge, whose number switched meaning
between "running" and "unread". One mark per task (up to five), in priority
order, never text:

* sphere: short arcs clustered at the top-right of the orbit;
* pill: short dashes centred under the shape.

Each state differs in more than colour (``DESIGN.md``: «nunca solo color»):

=============  ======  ====================  =======================
state          shape   colour                motion / weight
=============  ======  ====================  =======================
working        dash    mint                  breathes
needs_you      dash    amber                 steady, thicker
failed         dash    red                   steady
waiting        dash    neutral               steady, dimmed
stale          dash    neutral               steady, faint
done (unseen)  dot     mint                  steady
=============  ======  ====================  =======================
"""

from __future__ import annotations

import math

import numpy as np
from PIL import Image

from ..activity import DONE, FAILED, NEEDS_YOU, STALE, WAITING, WORKING
from .icons import segment_distance
from .tokens import palette, palette_version

ORBIT_GAP = 4.5  # separación entre la forma y la órbita (px lógicos)
MARK_T = 2.4  # grosor de una marca
SLOT_DEG = 15.0  # largo de un arco en la esfera
GAP_DEG = 8.0
PILL_DASH = 8.0  # largo de un guion bajo la píldora
PILL_GAP = 5.0
HALO = 1.3  # anillo de contraste con el escritorio
BREATH_SECS = 1.6
_BREATH_STEPS = 10

INK = {
    "light": {
        "mint": (12, 112, 117),
        "amber": (196, 122, 0),
        "red": (200, 36, 26),
        "muted": (78, 102, 106),
        "halo": (255, 255, 255, 190),
    },
    "dark": {
        "mint": (94, 226, 196),
        "amber": (255, 192, 82),
        "red": (255, 93, 82),
        "muted": (138, 164, 163),
        "halo": (6, 12, 14, 150),
    },
}


# estado → (forma, tinta, grosor relativo, opacidad)
def ink(theme: str) -> dict:
    """Tintas de las marcas: menta y neutro siguen a la paleta (también la personalizada)."""
    theme = "light" if theme == "light" else "dark"
    pal = palette(theme)
    out = dict(INK[theme])
    out["mint"] = tuple(pal["MINT"][:3])
    out["muted"] = tuple(pal["TEXT_3"][:3])
    return out


_STYLE = {
    WORKING: ("dash", "mint", 1.0, 1.0),
    NEEDS_YOU: ("dash", "amber", 1.3, 1.0),
    FAILED: ("dash", "red", 1.0, 1.0),
    WAITING: ("dash", "muted", 1.0, 0.75),
    STALE: ("dash", "muted", 1.0, 0.45),
    DONE: ("dot", "mint", 1.5, 1.0),
}

_TILES: dict[tuple, Image.Image] = {}


def breath(now: float, reduced: bool) -> float:
    """Opacidad de las marcas «trabajando», cuantizada para reutilizar caché."""
    if reduced:
        return 1.0
    wave = 0.5 + 0.5 * math.sin(2.0 * math.pi * now / BREATH_SECS)
    return round((0.45 + 0.55 * wave) * _BREATH_STEPS) / _BREATH_STEPS


def mark_paths(layout: str, count: int, w: float, h: float) -> list[list[tuple[float, float]]]:
    """Polilíneas de cada hueco (px lógicos, relativas al centro de la forma)."""
    paths: list[list[tuple[float, float]]] = []
    if layout == "circle":
        radius = w / 2 + ORBIT_GAP + MARK_T / 2
        total = count * SLOT_DEG + (count - 1) * GAP_DEG
        start = -45.0 - total / 2
        for i in range(count):
            a0 = start + i * (SLOT_DEG + GAP_DEG)
            pts = []
            for k in range(6):
                a = math.radians(a0 + SLOT_DEG * k / 5)
                pts.append((radius * math.cos(a), radius * math.sin(a)))
            paths.append(pts)
        return paths
    y = h / 2 + ORBIT_GAP + MARK_T / 2
    total = count * PILL_DASH + (count - 1) * PILL_GAP
    for i in range(count):
        x0 = -total / 2 + i * (PILL_DASH + PILL_GAP)
        paths.append([(x0, y), (x0 + PILL_DASH, y)])
    return paths


def _segments(pts: list[tuple[float, float]], dot: bool) -> np.ndarray:
    if dot:
        mid = (
            pts[len(pts) // 2]
            if len(pts) > 2
            else (
                (pts[0][0] + pts[-1][0]) / 2,
                (pts[0][1] + pts[-1][1]) / 2,
            )
        )
        return np.asarray([(*mid, *mid)], dtype=np.float32)
    return np.asarray([(*a, *b) for a, b in zip(pts, pts[1:], strict=False)], dtype=np.float32)


def orbit_tile(
    layout: str,
    states: tuple[str, ...],
    w: float,
    h: float,
    scale: float,
    theme: str,
    pulse: float = 1.0,
) -> Image.Image | None:
    """Capa RGBA centrada en la forma con las marcas de ``states`` (None si no hay).

    ``w`` y ``h`` son las medidas lógicas actuales de la forma (con su crecimiento).
    """
    states = tuple(s for s in states if s in _STYLE)
    if not states:
        return None
    theme = "light" if theme == "light" else "dark"
    key = (
        layout,
        states,
        round(w, 1),
        round(h, 1),
        round(scale, 3),
        theme,
        pulse,
        palette_version(),
    )
    tile = _TILES.get(key)
    if tile is not None:
        return tile
    reach = ORBIT_GAP + MARK_T * 1.6 + HALO + 2
    side_w = math.ceil((w + 2 * reach) * scale)
    side_h = math.ceil((h + 2 * reach) * scale)
    xs = (np.arange(side_w, dtype=np.float32) + 0.5) / scale - side_w / scale / 2
    ys = (np.arange(side_h, dtype=np.float32) + 0.5) / scale - side_h / scale / 2
    gx, gy = (g.ravel() for g in np.meshgrid(xs, ys))
    out = np.zeros((side_h * side_w, 4), dtype=np.float32)
    tints = ink(theme)
    halo = np.asarray(tints["halo"], np.float32) / 255.0
    for state, pts in zip(states, mark_paths(layout, len(states), w, h), strict=False):
        shape, tone, weight, alpha = _STYLE[state]
        if state == WORKING:
            alpha *= pulse
        d = segment_distance(gx, gy, _segments(pts, shape == "dot")) * scale
        r = MARK_T * weight * scale / 2
        for cover, rgba in (
            (np.clip(r + HALO * scale - d + 0.5, 0, 1) * halo[3] * alpha, halo[:3]),
            (np.clip(r - d + 0.5, 0, 1) * alpha, np.asarray(tints[tone], np.float32) / 255),
        ):
            a = cover[:, None]
            out[:, :3] = rgba[None, :] * a + out[:, :3] * (1 - a)
            out[:, 3:] = a + out[:, 3:] * (1 - a)
    data = (np.clip(out, 0, 1) * 255 + 0.5).astype(np.uint8).reshape(side_h, side_w, 4)
    tile = Image.fromarray(data, "RGBa").convert("RGBA")
    if len(_TILES) > 96:
        _TILES.pop(next(iter(_TILES)))
    _TILES[key] = tile
    return tile


__all__ = ["INK", "breath", "ink", "mark_paths", "orbit_tile"]
