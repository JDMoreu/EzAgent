"""Ventana de ajustes: movible, redimensionable y con cinco secciones.

Se abre en el centro de la pantalla (o donde se dejó la última vez) con una barra
lateral de secciones (Aspecto, Escucha, Voz, Modelos, Sistema); a la derecha se ve
solo la sección elegida, dibujada por el ``Menu`` de siempre (mismos controles,
edición en línea y guardado). Se arrastra desde cualquier zona sin controles, se
redimensiona desde los bordes y las esquinas, y un doble clic en la cabecera
devuelve el tamaño y la posición de fábrica.

Módulo puro de geometría y dibujo (numpy, sin Win32): ``drop.py`` lo usa para
componer el cuadro y repartir los clics entre la barra lateral, los bordes y el
menú.
"""

from __future__ import annotations

import math
from typing import Final

import numpy as np

from ..brand import drops_coverage
from .flow import FlowField, colors_for
from .icons import draw_icon
from .menu import _fill_circle, _fill_round_rect
from .menu_layout import GROUPS, SECTION_GROUPS, SECTION_TITLES, SIDEBAR_ORDER
from .text import draw_text
from .tokens import FLARE, FS_MICRO, FS_ROW, FontStyle

# ------------------------------------------------------------------ medidas
SIDEBAR_W: Final[float] = 184.0  # px lógicos de la barra lateral
DEFAULT_W: Final[float] = 760.0  # tamaño de fábrica de la ventana (lógico)
DEFAULT_H: Final[float] = 620.0
MIN_W: Final[float] = 560.0
MIN_H: Final[float] = 400.0
# Alto de la ventana por encima del alto visible del menú (borde inferior).
WINDOW_EXTRA_H: Final[float] = 4.0

# El cuerpo de la tarjeta se dibuja ``FLARE`` px dentro de la ventana (margen
# transparente a izquierda y derecha); los bordes de agarre viven dentro del cuerpo.
BODY_INSET: Final[float] = float(FLARE)
EDGE: Final[float] = 7.0  # grosor de la zona de agarre de un borde
CORNER: Final[float] = 20.0  # lado de la zona de agarre de una esquina
HEADER_H: Final[float] = 52.0  # franja superior: doble clic = tamaño de fábrica
KEEP_VISIBLE: Final[float] = 96.0  # px de la ventana que siempre quedan en pantalla

ITEMS_TOP: Final[float] = 64.0  # justo bajo la franja de cabecera (HEADER_H)
ITEM_H: Final[float] = 40.0
ITEM_GAP: Final[float] = 4.0
ITEM_X0: Final[float] = 14.0
ITEM_X1: Final[float] = SIDEBAR_W - 10.0
ITEM_RADIUS: Final[float] = 12.0
ICON_SIZE: Final[float] = 18.0
MARK_D: Final[float] = 22.0  # un poco mayor que ICON_SIZE: las gotas llevan aire alrededor
MARK_CX: Final[float] = ITEM_X0 + 22.0  # misma columna que los iconos de sección
MARK_FROM_BOTTOM: Final[float] = 30.0  # del centro de la marca al borde inferior de la ventana
FOOT_H: Final[float] = 40.0  # alto de la fila del pie: como un elemento de la lista
LABEL_CY: Final[float] = 26.0  # eje de «AJUSTES»: el mismo que el título y el ✕ del contenido

# ``draw_text`` recorta al contorno de la tinta: su ``y`` es la parte alta de las
# mayúsculas, no una línea de texto. Para centrar texto con un icono se centra la
# altura de mayúsculas (Geist y Geist Mono la tienen en ~0,714 em).
CAP_RATIO: Final[float] = 0.714

# DECISION: el nombre es una palabra-marca de dos pesos de Geist (la misma familia
# del sistema): «Ez» en semibold con el acento, «Agent» en medium con el texto.
WORD_EZ: Final[FontStyle] = FontStyle("WORD_EZ", "Geist", 15, 600, tracking_em=-0.01)
WORD_AGENT: Final[FontStyle] = FontStyle("WORD_AGENT", "Geist", 15, 500, tracking_em=-0.01)

SECTION_ICONS: Final[dict[str, str]] = {
    "appearance": "sparkle",
    "listening": "mic-line",
    "voice": "voice-line",
    "models": "chip",
    "system": "sliders",
}

# Cursores de Windows (IDC_*) para cada borde.
IDC_SIZEWE: Final[int] = 32644
IDC_SIZENS: Final[int] = 32645
IDC_SIZENWSE: Final[int] = 32642
IDC_SIZENESW: Final[int] = 32643

Edges = tuple[bool, bool, bool, bool]  # (izquierda, arriba, derecha, abajo)


# ---------------------------------------------------------------- secciones
def sections() -> list[tuple[str, str]]:
    """``(id, título)`` de cada sección, en el orden de la barra lateral."""
    return [(name, SECTION_TITLES.get(name, name.title())) for name in SIDEBAR_ORDER]


def section_groups(name: str) -> tuple[tuple[str, tuple[tuple[str, str], ...]], ...]:
    """Grupos que muestra el menú para la sección ``name`` (o la primera)."""
    wanted = SECTION_GROUPS.get(name) or SECTION_GROUPS[SIDEBAR_ORDER[0]]
    by_name = dict(GROUPS)
    return tuple((group, by_name[group]) for group in wanted if group in by_name)


def section_title(name: str) -> str:
    return SECTION_TITLES.get(name, name.title())


# ----------------------------------------------------------------- geometría
def content_size(win_w: float, win_h: float) -> tuple[float, float]:
    """``(ancho, alto visible)`` lógicos del menú dentro de una ventana ``win_w``×``win_h``."""
    return (max(1.0, float(win_w) - SIDEBAR_W), max(1.0, float(win_h) - WINDOW_EXTRA_H))


def window_size(viewport_h: float) -> tuple[float, float]:
    """Tamaño lógico de la ventana de fábrica para un alto de contenido dado."""
    return (DEFAULT_W, float(viewport_h) + WINDOW_EXTRA_H)


def clamp_size(
    w: float, h: float, work: tuple[int, int, int, int], scale: float
) -> tuple[float, float]:
    """Tamaño lógico acotado entre el mínimo y el área de trabajo (con margen)."""
    max_w = max(MIN_W, (work[2] - work[0]) / scale - 16.0)
    max_h = max(MIN_H, (work[3] - work[1]) / scale - 16.0)
    return (max(MIN_W, min(float(w), max_w)), max(MIN_H, min(float(h), max_h)))


def centered(work: tuple[int, int, int, int], w: int, h: int) -> tuple[int, int]:
    """Posición física que centra una ventana ``w``×``h`` en el área de trabajo."""
    left, top, right, bottom = work
    x = left + max(0, (right - left - w) // 2)
    y = top + max(0, (bottom - top - h) // 2)
    return (int(x), int(y))


def clamp_position(
    x: int, y: int, w: int, h: int, work: tuple[int, int, int, int], scale: float
) -> tuple[int, int]:
    """Mantiene a la vista una franja de la ventana (la cabecera siempre alcanzable)."""
    keep = round(KEEP_VISIBLE * scale)
    left, top, right, bottom = work
    nx = max(left - w + keep, min(int(x), right - keep))
    ny = max(top, min(int(y), bottom - keep))
    return (nx, ny)


def resize_edges(x: float, y: float, w: float, h: float) -> Edges | None:
    """Bordes que agarra el punto lógico ``(x, y)`` de una ventana ``w``×``h``.

    ``None`` si el punto no está en una zona de agarre. Las esquinas agarran dos
    bordes a la vez. El cuerpo empieza ``BODY_INSET`` px dentro de la ventana.
    """
    left, right = BODY_INSET, w - BODY_INSET
    if not (left - 1.0 <= x <= right + 1.0 and -1.0 <= y <= h + 1.0):
        return None
    edges = [
        x <= left + EDGE,  # izquierda
        y <= EDGE,  # arriba
        x >= right - EDGE,  # derecha
        y >= h - EDGE,  # abajo
    ]
    corner = [x <= left + CORNER, y <= CORNER, x >= right - CORNER, y >= h - CORNER]
    if (corner[0] or corner[2]) and (corner[1] or corner[3]):
        edges = [edge or near for edge, near in zip(edges, corner, strict=True)]
    return (edges[0], edges[1], edges[2], edges[3]) if any(edges) else None


def cursor_for(edges: Edges | None) -> int:
    """Cursor (IDC_*) para los bordes agarrados; 0 si no hay ninguno."""
    if edges is None:
        return 0
    left, top, right, bottom = edges
    horizontal = left or right
    vertical = top or bottom
    if horizontal and vertical:
        return IDC_SIZENWSE if (left == top) else IDC_SIZENESW
    if horizontal:
        return IDC_SIZEWE
    return IDC_SIZENS if vertical else 0


def resized_rect(
    edges: Edges,
    start: tuple[int, int, int, int],
    delta: tuple[int, int],
    limits: tuple[int, int, int, int],
) -> tuple[int, int, int, int]:
    """Nuevo ``(x, y, w, h)`` físico al arrastrar ``edges`` ``delta`` píxeles.

    ``start`` es el rectángulo al empezar; ``limits`` es ``(min_w, min_h, max_w,
    max_h)``. Los bordes izquierdo y superior mueven también el origen para que
    el borde opuesto no se desplace.
    """
    x0, y0, w0, h0 = start
    dx, dy = delta
    min_w, min_h, max_w, max_h = limits
    left, top, right, bottom = edges
    x, y, w, h = x0, y0, w0, h0
    if right:
        w = max(min_w, min(w0 + dx, max_w))
    elif left:
        w = max(min_w, min(w0 - dx, max_w))
        x = x0 + (w0 - w)
    if bottom:
        h = max(min_h, min(h0 + dy, max_h))
    elif top:
        h = max(min_h, min(h0 - dy, max_h))
        y = y0 + (h0 - h)
    return (x, y, w, h)


def in_header(x: float, y: float) -> bool:
    """¿Está el punto lógico en la franja superior de la ventana?"""
    return 0.0 <= y <= HEADER_H


# ---------------------------------------------------------------- barra lateral
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


def cap_top(center_y: float, size: float) -> float:
    """``y`` de ``draw_text`` para que las mayúsculas de ``size`` px queden centradas en ``center_y``."""
    return center_y - CAP_RATIO * size / 2.0


def _blend(seg: np.ndarray, line: tuple) -> None:
    """Mezcla el color de línea (premultiplicado) sobre un tramo de 1 px de ``buf``."""
    alpha = (line[3] / 255.0) if len(line) == 4 else 0.1
    color = np.concatenate([np.asarray(line[:3], np.float32) / 255.0 * alpha, [alpha]])
    seg[...] = color + seg * (1.0 - alpha)


def _hline(buf: np.ndarray, line: tuple, scale: float, y: float, x0: float, x1: float) -> None:
    """Línea horizontal de 1 px físicas en ``y``, de ``x0`` a ``x1`` (px lógicos)."""
    _blend(buf[int(round(y * scale)), int(round(x0 * scale)) : int(round(x1 * scale))], line)


def _vline(buf: np.ndarray, line: tuple, scale: float, x: float, y0: float, y1: float) -> None:
    """Línea vertical de 1 px físicas en ``x``, de ``y0`` a ``y1`` (px lógicos)."""
    _blend(buf[int(round(y0 * scale)) : int(round(y1 * scale)), int(round(x * scale))], line)


class Sidebar:
    """Barra lateral: marca viva, título y lista de secciones con icono."""

    def __init__(self) -> None:
        self._flow = FlowField("circle", seed=7)
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
        self._flow.step(dt, 0.0, "idle", reduced)
        # «Ajustes» arriba; abajo, una línea y debajo la marca con el nombre, pegados
        # al borde inferior.
        top = cap_top(LABEL_CY, FS_MICRO.size)
        draw_text(buf, "Ajustes", FS_MICRO, pal["TEXT_3"], ITEM_X0 + 13.0, top, scale)
        self._draw_brand(buf, scale, pal, buf.shape[0] / scale - MARK_FROM_BOTTOM)
        for index, (name, title) in enumerate(sections()):
            x0, y0, x1, y1 = item_rect(index)
            cy = (y0 + y1) / 2.0
            if name == selected:
                _fill_round_rect(buf, x0, y0, x1, y1, ITEM_RADIUS, pal["MINT"], scale, 0.14)
                color = pal["MINT"]
            else:
                if name == hover:
                    _fill_round_rect(buf, x0, y0, x1, y1, ITEM_RADIUS, pal["GROUP_HOVER"], scale)
                color = pal["TEXT_2"]
            draw_icon(
                buf, SECTION_ICONS.get(name, "sliders"), x0 + 22.0, cy, ICON_SIZE, color, scale
            )
            draw_text(buf, title, FS_ROW, color, x0 + 42.0, cap_top(cy, FS_ROW.size), scale)
        # Separador vertical entre la barra lateral y el contenido.
        _vline(buf, pal["LINE"], scale, SIDEBAR_W - 1.0 / scale, 16.0, buf.shape[0] / scale - 16.0)

    def _draw_brand(self, buf: np.ndarray, scale: float, pal: dict, cy: float) -> None:
        """Pie de la barra: línea, marca y palabra-marca, centradas en ``cy``."""
        _hline(buf, pal["LINE"], scale, cy - FOOT_H / 2.0, ITEM_X0, ITEM_X1)
        self._draw_mark(buf, scale, cy)
        x = ITEM_X0 + 42.0
        top = cap_top(cy, WORD_EZ.size)
        # El ancho devuelto es el de la tinta (sin márgenes laterales): +0,6 px de aire.
        x += draw_text(buf, "Ez", WORD_EZ, pal["MINT"], x, top, scale)[0] + 0.6
        draw_text(buf, "Agent", WORD_AGENT, pal["TEXT"], x, top, scale)

    def _draw_mark(self, buf: np.ndarray, scale: float, cy: float) -> None:
        """La marca de EzAgent (las gotas del icono) llena de Marea en movimiento."""
        size = max(8, int(round(MARK_D * scale)))
        # DECISION: malla de ±0.5 y paleta clara siempre, igual que el icono: así el
        # borde luminoso del círculo queda fuera de las gotas y la marca no cambia con el tema.
        axis = np.linspace(-0.5, 0.5, size, dtype=np.float32)
        gx, gy = np.meshgrid(axis, axis)
        rgb = np.clip(self._flow.render(gx, gy, "light", colors_for("light", "idle")), 0, 255)
        rgb = rgb / 255.0
        coverage = drops_coverage(size)[..., None]
        mark = np.concatenate([rgb * coverage, coverage], axis=2).astype(np.float32)
        x0 = int(round((MARK_CX - MARK_D / 2.0) * scale))
        y0 = int(round((cy - MARK_D / 2.0) * scale))
        composite_over(buf, mark, x0, y0)


def draw_grip(buf: np.ndarray, scale: float, pal: dict) -> None:
    """Seis puntos en triángulo en la esquina inferior derecha: «aquí se estira»."""
    h, w = buf.shape[:2]
    right = w / scale - BODY_INSET - 13.0
    bottom = h / scale - 13.0
    color = pal["TEXT_3"]
    for row in range(3):
        for col in range(3 - row):
            _fill_circle(buf, right - col * 5.0, bottom - row * 5.0, 2.2, color, scale, 0.55)


def fade(buf: np.ndarray, amount: float) -> None:
    """Multiplica el cuadro entero (premultiplicado) por ``amount`` (fundido)."""
    k = max(0.0, min(1.0, float(amount)))
    if k < 1.0:
        buf *= k * k * (3.0 - 2.0 * k) if not math.isclose(k, 0.0) else 0.0
