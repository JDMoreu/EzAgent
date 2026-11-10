"""La marca de EzAgent: unas comillas hechas de dos gotas de «Marea».

La comilla de apertura (“) es lo que ocurre cuando le hablas a EzAgent: tu frase
entre comillas es el argumento que se entrega a un agente en la terminal
(``claude "haz esto"``). Sus dos gotas están llenas del mismo fluido vivo que
anima la barra (``notch/flow.py``), sobre un cuadrado de grafito azulado. La
silueta es la parte propia de la marca: a 16 px siguen leyéndose dos comas
curvas, algo que ningún orbe o forma de onda de «asistente de voz» hace.

``scripts/make_icons.py`` genera los archivos a partir de aquí; la geometría de
las gotas es única y sirve tanto para el ráster como para el SVG.
"""

from __future__ import annotations

import math
from functools import lru_cache

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

from .notch.flow import FlowField, colors_for

INK = (0x07, 0x16, 0x1B)  # grafito azulado, abajo del cuadrado
INK_TOP = (0x0D, 0x2A, 0x31)  # el mismo, un poco más claro arriba
_TILE_RADIUS = 0.22  # esquinas del cuadrado, en fracción del lado

# Fotograma del fluido: ola diagonal de menta sobre verde azulado profundo.
_FRAME_SEED = 7
_FRAME_TIME = 1.5
_FRAME_SCALE = 0.32

# Geometría de una gota en coordenadas unitarias del cuadrado (y crece hacia abajo).
_DROP_R = 0.125  # radio de la cabeza
_DROP_SPREAD = 0.28  # separación entre las cabezas
_DROP_Y = 0.60  # altura de las cabezas
_DROP_TAIL = (0.075, -0.30)  # de la cabeza a la punta de la cola
_DROP_BEND = (-0.115, -0.20)  # punto de control de la curva de la cola
_DROP_TAPER = 0.80  # cuánto se afina la cola (0 = tubo, 1 = punta)
_STEPS = 220


def _curve(i: int, head: tuple[float, float]) -> tuple[float, float, float]:
    """Punto ``i`` de la cola (bézier cuadrática) y su fracción ``t``."""
    t = i / _STEPS
    hx, hy = head
    tx, ty = hx + _DROP_TAIL[0], hy + _DROP_TAIL[1]
    bx, by = hx + _DROP_BEND[0], hy + _DROP_BEND[1]
    x = (1 - t) ** 2 * hx + 2 * (1 - t) * t * bx + t * t * tx
    y = (1 - t) ** 2 * hy + 2 * (1 - t) * t * by + t * t * ty
    return x, y, t


def _radius(t: float) -> float:
    return _DROP_R * (1.0 - _DROP_TAPER * t**0.85)


def _heads() -> list[tuple[float, float]]:
    half = _DROP_SPREAD / 2 + 0.005
    return [(0.5 - half, _DROP_Y), (0.5 + half, _DROP_Y)]


def _drops_mask(side: int) -> Image.Image:
    """Máscara de las dos gotas: círculos que se afinan a lo largo de la curva."""
    m = Image.new("L", (side, side), 0)
    dr = ImageDraw.Draw(m)
    for head in _heads():
        for i in range(_STEPS + 1):
            x, y, t = _curve(i, head)
            rr = _radius(t) * side
            cx, cy = x * side, y * side
            dr.ellipse((cx - rr, cy - rr, cx + rr, cy + rr), fill=255)
    return m


@lru_cache(maxsize=8)
def drops_coverage(size: int) -> np.ndarray:
    """Cobertura (0–1, float32) de las gotas recortadas a su caja, en ``size``×``size``.

    Es la silueta sola, sin cuadrado: sirve para llenarla con el fluido vivo de
    la ventana de ajustes. La caja es cuadrada y está centrada en el centro
    *visual* de las gotas, así que se alinea con texto o iconos sin retoques.
    El resultado se reutiliza; no lo modifiques.
    """
    mask = _drops_mask(int(round(8 * size / 0.56)))
    left, top, right, bottom = mask.getbbox() or (0, 0, mask.width, mask.height)
    # DECISION: 4 % de aire alrededor de la silueta para que el borde no se recorte.
    half = max(right - left, bottom - top) / 2.0 * 1.04
    cx, cy = (left + right) / 2.0, (top + bottom) / 2.0
    box = (round(cx - half), round(cy - half), round(cx + half), round(cy + half))
    crop = mask.crop(box).resize((size, size), Image.LANCZOS)
    out = np.asarray(crop, np.float32) / 255.0
    out.setflags(write=False)
    return out


def _fluid(size: int) -> np.ndarray:
    """Un fotograma del fluido en un cuadrado ``size``×``size`` (RGB float)."""
    field = FlowField("circle", seed=_FRAME_SEED)
    field.scale = _FRAME_SCALE
    for _ in range(int(_FRAME_TIME * 30)):
        field.step(1 / 30, 0.0, "idle")
    axis = np.linspace(-1.0, 1.0, size, dtype=np.float32)
    gx, gy = np.meshgrid(axis, axis)
    return field.render(gx, gy, "light", colors_for("light", "idle"))


def _tile(side: int) -> Image.Image:
    """Cuadrado redondeado de grafito con un degradado vertical muy suave."""
    k = np.linspace(0.0, 1.0, side, dtype=np.float32)[:, None, None]
    rows = np.asarray(INK_TOP, np.float32) * (1 - k) + np.asarray(INK, np.float32) * k
    base = Image.fromarray(np.broadcast_to(rows, (side, side, 3)).astype(np.uint8), "RGB")
    mask = Image.new("L", (side, side), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        (0, 0, side - 1, side - 1), radius=int(_TILE_RADIUS * side), fill=255
    )
    out = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    out.paste(base, (0, 0), mask)
    return out


def mark_image(size: int = 256, tile: bool = True) -> Image.Image:
    """La marca en un lienzo cuadrado RGBA; sin ``tile`` queda sobre transparente."""
    big = 1024
    mask = _drops_mask(big * 2).resize((big, big), Image.LANCZOS)
    fluid = Image.fromarray(np.clip(_fluid(big), 0, 255).astype(np.uint8), "RGB")
    img = _tile(big) if tile else Image.new("RGBA", (big, big), (0, 0, 0, 0))
    img.paste(fluid, (0, 0), mask)

    # Brillo leve arriba, recortado a las gotas: la luz de vidrio de la barra.
    shine = Image.new("L", (big, big), 0)
    ImageDraw.Draw(shine).ellipse((0.1 * big, -0.15 * big, 0.9 * big, 0.38 * big), fill=38)
    shine = shine.filter(ImageFilter.GaussianBlur(0.04 * big))
    shine = Image.fromarray(
        (np.asarray(shine, np.float32) * np.asarray(mask, np.float32) / 255).astype(np.uint8)
    )
    img.paste(Image.new("RGBA", (big, big), (255, 255, 255, 255)), (0, 0), shine)
    return img.resize((size, size), Image.LANCZOS)


def _drop_path(head: tuple[float, float], units: float) -> str:
    """Contorno SVG de una gota: lados de la cola, punta y cabeza como círculos."""
    left: list[tuple[float, float]] = []
    right: list[tuple[float, float]] = []
    n = 48
    for j in range(n + 1):
        i = round(j * _STEPS / n)
        x, y, t = _curve(i, head)
        x2, y2, _ = _curve(min(i + 1, _STEPS), head)
        x1, y1, _ = _curve(max(i - 1, 0), head)
        ang = math.atan2(y2 - y1, x2 - x1) + math.pi / 2
        r = _radius(t)
        left.append(((x + r * math.cos(ang)) * units, (y + r * math.sin(ang)) * units))
        right.append(((x - r * math.cos(ang)) * units, (y - r * math.sin(ang)) * units))
    pts = left + right[::-1]
    d = "M" + " L".join(f"{px:.2f} {py:.2f}" for px, py in pts) + " Z"
    tip_x, tip_y, tip_t = _curve(_STEPS, head)
    tip = f'<circle cx="{tip_x * units:.2f}" cy="{tip_y * units:.2f}" r="{_radius(tip_t) * units:.2f}"/>'
    cap = (
        f'<circle cx="{head[0] * units:.2f}" cy="{head[1] * units:.2f}" r="{_DROP_R * units:.2f}"/>'
    )
    return f'<path d="{d}"/>{cap}{tip}'


def _svg() -> str:
    units = 48.0
    drops = "".join(_drop_path(h, units) for h in _heads())
    r = _TILE_RADIUS * units
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" width="256" height="256">\n'
        "  <defs>\n"
        '    <linearGradient id="tile" x1="0" y1="0" x2="0" y2="1">\n'
        f'      <stop offset="0" stop-color="#{INK_TOP[0]:02X}{INK_TOP[1]:02X}{INK_TOP[2]:02X}"/>\n'
        f'      <stop offset="1" stop-color="#{INK[0]:02X}{INK[1]:02X}{INK[2]:02X}"/>\n'
        "    </linearGradient>\n"
        '    <linearGradient id="marea" gradientUnits="userSpaceOnUse" x1="10" y1="12" x2="40" y2="36">\n'
        '      <stop offset="0" stop-color="#3E9FB0"/>\n'
        '      <stop offset="0.45" stop-color="#1FAE9F"/>\n'
        '      <stop offset="1" stop-color="#A8E6C8"/>\n'
        "    </linearGradient>\n"
        "  </defs>\n"
        f'  <rect width="48" height="48" rx="{r:.2f}" fill="url(#tile)"/>\n'
        f'  <g fill="url(#marea)">{drops}</g>\n'
        "</svg>\n"
    )


SVG = _svg()
