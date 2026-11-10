"""Geometría y curvas del chat: tamaño del panel, a dónde crece, bordes para redimensionar y easing.

Todo es puro (sin Win32 ni dibujo) para poder probarlo entero. ``chat_ui`` lo usa para colocar y
animar la ventana.
"""

from __future__ import annotations

from typing import Final

import numpy as np

PANEL_W: Final[float] = 440.0
PANEL_H: Final[float] = 640.0
MIN_W: Final[float] = 340.0
MIN_H: Final[float] = 380.0
MARGIN: Final[float] = 16.0  # aire alrededor del panel para la sombra (px lógicos)
SCREEN_PAD: Final[float] = 12.0  # separación mínima al borde de la pantalla
PANEL_RADIUS: Final[float] = 22.0
GRIP: Final[float] = 8.0  # grosor de la zona de agarre de los bordes (px lógicos)
MORPH_SECS: Final[float] = 0.40
FADE_SECS: Final[float] = 0.14  # con movimiento reducido

Rect = tuple[float, float, float, float]
Edges = tuple[bool, bool, bool, bool]  # (izquierda, arriba, derecha, abajo)

# Cursores de Windows (IDC_*) para cada borde.
IDC_SIZEWE: Final[int] = 32644
IDC_SIZENS: Final[int] = 32645
IDC_SIZENWSE: Final[int] = 32642
IDC_SIZENESW: Final[int] = 32643


# ------------------------------------------------------------------- curvas
def smoothstep(x: float) -> float:
    x = max(0.0, min(1.0, x))
    return x * x * (3.0 - 2.0 * x)


def ease(x: float) -> float:
    """Aceleración y frenado suaves (cúbica): arranca con calma y se asienta sin golpe."""
    x = max(0.0, min(1.0, x))
    return 4 * x**3 if x < 0.5 else 1.0 - (-2 * x + 2) ** 3 / 2


def ease_out_back(x: float, overshoot: float = 0.75) -> float:
    """Llega, se pasa un pelín (~3 %) y se asienta: el panel «cae» en su sitio con vida."""
    x = max(0.0, min(1.0, x))
    c3 = overshoot + 1.0
    return 1.0 + c3 * (x - 1.0) ** 3 + overshoot * (x - 1.0) ** 2


def lerp(a: float, b: float, k: float) -> float:
    return a + (b - a) * k


def lerp_rect(a: Rect, b: Rect, k: float) -> Rect:
    return (lerp(a[0], b[0], k), lerp(a[1], b[1], k), lerp(a[2], b[2], k), lerp(a[3], b[3], k))


# ----------------------------------------------------------------- tamaño
def panel_limits(work: tuple[int, int, int, int], scale: float) -> tuple[float, float]:
    """Tamaño lógico máximo del panel: toda la pantalla menos la separación a los bordes."""
    return (
        max(MIN_W, (work[2] - work[0]) / scale - 2 * SCREEN_PAD),
        max(MIN_H, (work[3] - work[1]) / scale - 2 * SCREEN_PAD),
    )


def panel_size(
    work: tuple[int, int, int, int], scale: float, saved: tuple[float, float] | None = None
) -> tuple[float, float]:
    """Tamaño lógico del panel: el recordado o el de fábrica, siempre entre el mínimo y la pantalla."""
    max_w, max_h = panel_limits(work, scale)
    want_w, want_h = saved if saved else (PANEL_W, PANEL_H)
    return (max(MIN_W, min(float(want_w), max_w)), max(MIN_H, min(float(want_h), max_h)))


def panel_rect(
    form: Rect,
    work: tuple[int, int, int, int],
    scale: float,
    saved: tuple[float, float] | None = None,
) -> Rect:
    """Rectángulo físico del panel: crece desde la forma hacia donde haya más espacio.

    Centrado en la forma y pegado a ella por el lado que mira al centro de la pantalla (una barra
    abajo despliega hacia arriba, una arriba hacia abajo); siempre dentro de la pantalla.
    """
    pw, ph = (v * scale for v in panel_size(work, scale, saved))
    pad = SCREEN_PAD * scale
    cx = (form[0] + form[2]) / 2
    x0 = max(work[0] + pad, min(cx - pw / 2, work[2] - pad - pw))
    mid = (work[1] + work[3]) / 2
    if (form[1] + form[3]) / 2 > mid:  # barra en la mitad de abajo: crece hacia arriba
        y1 = min(form[3], work[3] - pad)
        y0 = max(work[1] + pad, y1 - ph)
        y1 = y0 + ph
    else:
        y0 = max(work[1] + pad, form[1])
        y1 = min(work[3] - pad, y0 + ph)
        y0 = y1 - ph
    return (x0, y0, x0 + pw, y0 + ph)


def rounded_coverage(w: int, h: int, radius: float, inset: float = 0.0) -> np.ndarray:
    """Cobertura (0..1) suave de un rectángulo redondeado ``w``×``h`` (distancia con signo)."""
    w, h = max(2, w), max(2, h)
    r = max(0.0, min(radius, min(w, h) / 2.0)) - inset
    xs = np.abs(np.arange(w, dtype=np.float32) + 0.5 - w / 2)[None, :]
    ys = np.abs(np.arange(h, dtype=np.float32) + 0.5 - h / 2)[:, None]
    dx = np.maximum(xs - (w / 2 - inset - r), 0.0)
    dy = np.maximum(ys - (h / 2 - inset - r), 0.0)
    dist = np.sqrt(dx * dx + dy * dy) - r
    return np.clip(0.5 - dist, 0.0, 1.0)


# ------------------------------------------------------------ redimensionar
def edges_at(x: float, y: float, w: float, h: float, grip: float = GRIP) -> Edges | None:
    """Bordes que agarra el punto ``(x, y)`` de un panel ``w``×``h`` (``None`` si no toca ninguno).

    Las esquinas agarran dos bordes a la vez. Coordenadas lógicas relativas al panel.
    """
    if not (-1.0 <= x <= w + 1.0 and -1.0 <= y <= h + 1.0):
        return None
    edges = (x <= grip, y <= grip, x >= w - grip, y >= h - grip)
    return edges if any(edges) else None


def edge_cursor(edges: Edges) -> int:
    """Cursor de Windows para esos bordes (flechas horizontal, vertical o diagonal)."""
    left, top, right, bottom = edges
    if (left or right) and (top or bottom):
        return IDC_SIZENWSE if (left and top) or (right and bottom) else IDC_SIZENESW
    return IDC_SIZEWE if (left or right) else IDC_SIZENS


def resize_rect(
    edges: Edges,
    rect: Rect,
    dx: float,
    dy: float,
    work: tuple[int, int, int, int],
    scale: float,
) -> Rect:
    """Rectángulo físico del panel tras arrastrar ``edges`` ``(dx, dy)`` píxeles.

    El borde opuesto queda fijo; el tamaño se acota entre el mínimo y la pantalla (con su
    separación a los bordes), y nunca se sale del área de trabajo.
    """
    left, top, right, bottom = edges
    x0, y0, x1, y1 = rect
    pad = SCREEN_PAD * scale
    min_w, min_h = MIN_W * scale, MIN_H * scale
    lo_x, hi_x = work[0] + pad, work[2] - pad
    lo_y, hi_y = work[1] + pad, work[3] - pad
    if left:
        x0 = max(lo_x, min(x0 + dx, x1 - min_w))
    if right:
        x1 = min(hi_x, max(x1 + dx, x0 + min_w))
    if top:
        y0 = max(lo_y, min(y0 + dy, y1 - min_h))
    if bottom:
        y1 = min(hi_y, max(y1 + dy, y0 + min_h))
    return (x0, y0, x1, y1)
