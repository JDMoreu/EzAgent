"""Ventana de ajustes: centrada, con secciones a la izquierda y controles a la derecha.

La tarjeta que se desplegaba desde ☰ era estrecha y estaba atada al espacio que
quedaba sobre la barra. La ventana se abre en el centro de la pantalla con más
alto y una barra lateral de secciones (Apariencia, Micrófono, Fin de turno…);
a la derecha se ve solo la sección elegida, dibujada por el ``Menu`` de siempre
(mismos controles, edición en línea y guardado). «–» la oculta.

Módulo puro de geometría y dibujo (numpy, sin Win32): ``drop.py`` lo usa para
componer el cuadro y repartir los clics entre la barra lateral y el menú.
"""

from __future__ import annotations

import math

import numpy as np

from .flow import FlowField, colors_for
from .menu import _fill_circle, _fill_round_rect
from .menu_layout import GROUPS, SECTION_TITLES, SIDEBAR_ORDER
from .text import draw_text
from .tokens import FS_MICRO, FS_ROW, FS_TITLE

SIDEBAR_W = 188.0  # px lógicos de la barra lateral
# Ancho del contenido: más que los 320 px de la tarjeta para que etiqueta y
# valor no se pisen (proveedor, dirección de la API, modelos...).
CONTENT_W = 440.0
CONTENT_MAX_H = 600.0  # alto lógico máximo del contenido (la sección)
ITEMS_TOP = 86.0
ITEM_H = 34.0
ITEM_GAP = 3.0
ITEM_X0 = 14.0
ITEM_X1 = SIDEBAR_W - 8.0
ITEM_RADIUS = 10.0
ORB_D = 30.0
ORB_CX, ORB_CY = 33.0, 40.0


def sections() -> list[tuple[str, str]]:
    """``(id, título)`` de cada sección, en el orden de la barra lateral."""
    known = {name for name, _rows in GROUPS}  # «Colores» vive dentro de Apariencia
    order = [name for name in SIDEBAR_ORDER if name in known]
    order += [name for name, _rows in GROUPS if name not in order]
    return [(name, SECTION_TITLES.get(name, name.title())) for name in order]


def section_groups(name: str) -> tuple[tuple[str, tuple[tuple[str, str], ...]], ...]:
    """Grupos que muestra el menú para la sección ``name`` (o el primero)."""
    for group in GROUPS:
        if group[0] == name:
            return (group,)
    return (GROUPS[0],)


def section_title(name: str) -> str:
    return SECTION_TITLES.get(name, name.title())


def window_size(viewport_h: float) -> tuple[float, float]:
    """Tamaño lógico de la ventana para un alto de contenido dado."""
    return (SIDEBAR_W + CONTENT_W, float(viewport_h) + 4.0)


def centered(work: tuple[int, int, int, int], w: int, h: int) -> tuple[int, int]:
    """Posición física que centra una ventana ``w``×``h`` en el área de trabajo."""
    left, top, right, bottom = work
    x = left + max(0, (right - left - w) // 2)
    y = top + max(0, (bottom - top - h) // 2)
    return (int(x), int(y))


def item_rect(index: int) -> tuple[float, float, float, float]:
    y0 = ITEMS_TOP + index * (ITEM_H + ITEM_GAP)
    return (ITEM_X0, y0, ITEM_X1, y0 + ITEM_H)


def item_at(x: float, y: float) -> str | None:
    """Sección bajo el punto lógico ``(x, y)`` de la ventana, si la hay."""
    if not ITEM_X0 <= x <= ITEM_X1:
        return None
    for index, (name, _title) in enumerate(sections()):
        _x0, y0, _x1, y1 = item_rect(index)
        if y0 <= y <= y1:
            return name
    return None


def composite_over(dst: np.ndarray, src: np.ndarray, x0: int, y0: int) -> None:
    """``src`` sobre ``dst`` (float32 RGBA premultiplicado) en ``(x0, y0)``."""
    h = min(src.shape[0], dst.shape[0] - y0)
    w = min(src.shape[1], dst.shape[1] - x0)
    if h <= 0 or w <= 0:
        return
    region = dst[y0 : y0 + h, x0 : x0 + w]
    top = src[:h, :w]
    region[...] = top + region * (1.0 - top[..., 3:4])


class Sidebar:
    """Barra lateral: marca viva, título y lista de secciones."""

    def __init__(self) -> None:
        self._orb = FlowField("circle", seed=7)
        self._last = 0.0

    def draw(
        self,
        buf: np.ndarray,
        scale: float,
        pal: dict,
        theme: str,
        selected: str,
        hover: str | None,
        now: float,
        reduced: bool = False,
    ) -> None:
        dt = 0.0 if self._last == 0.0 else min(0.1, max(0.0, now - self._last))
        self._last = now
        self._orb.step(dt, 0.0, "idle", reduced)
        self._draw_orb(buf, scale, theme)
        draw_text(buf, "EzAgent", FS_TITLE, pal["TEXT"], 56.0, 26.0, scale)
        draw_text(buf, "Ajustes", FS_MICRO, pal["TEXT_3"], 56.0, 47.0, scale)
        for index, (name, title) in enumerate(sections()):
            x0, y0, x1, y1 = item_rect(index)
            if name == selected:
                _fill_round_rect(buf, x0, y0, x1, y1, ITEM_RADIUS, pal["MINT"], scale, 0.14)
                color = pal["MINT"]
            else:
                if name == hover:
                    _fill_round_rect(buf, x0, y0, x1, y1, ITEM_RADIUS, pal["GROUP_HOVER"], scale)
                color = pal["TEXT_2"]
            if name == selected:
                _fill_circle(buf, x0 + 12.0, (y0 + y1) / 2.0, 6.0, pal["MINT"], scale)
            draw_text(buf, title, FS_ROW, color, x0 + 24.0, y0 + 8.0, scale)
        # Separador vertical entre la barra lateral y el contenido.
        line = pal["LINE"]
        x = int(round(SIDEBAR_W * scale)) - 1
        alpha = (line[3] / 255.0) if len(line) == 4 else 0.1
        rgb = np.asarray(line[:3], np.float32) / 255.0
        top, bottom = int(16 * scale), buf.shape[0] - int(16 * scale)
        col = buf[top:bottom, x]
        col[...] = np.concatenate([rgb * alpha, [alpha]]) + col * (1.0 - alpha)

    def _draw_orb(self, buf: np.ndarray, scale: float, theme: str) -> None:
        """Un orbe pequeño de Marea: la marca viva en la ventana."""
        size = max(8, int(round(ORB_D * scale)))
        axis = np.linspace(-1.0, 1.0, size, dtype=np.float32)
        gx, gy = np.meshgrid(axis, axis)
        rgb = np.clip(self._orb.render(gx, gy, theme, colors_for(theme, "idle")), 0, 255) / 255.0
        dist = np.sqrt(gx * gx + gy * gy)
        coverage = np.clip((1.0 - dist) * size / 2.0 + 0.5, 0.0, 1.0)[..., None]
        orb = np.concatenate([rgb * coverage, coverage], axis=2).astype(np.float32)
        x0 = int(round((ORB_CX - ORB_D / 2.0) * scale))
        y0 = int(round((ORB_CY - ORB_D / 2.0) * scale))
        composite_over(buf, orb, x0, y0)


def fade(buf: np.ndarray, amount: float) -> None:
    """Multiplica el cuadro entero (premultiplicado) por ``amount`` (fundido)."""
    k = max(0.0, min(1.0, float(amount)))
    if k < 1.0:
        buf *= k * k * (3.0 - 2.0 * k) if not math.isclose(k, 0.0) else 0.0
