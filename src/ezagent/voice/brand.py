"""La marca de EzAgent: un orbe de «Marea», el mismo gradiente vivo de la barra.

Un círculo lleno con un fotograma del fluido de ``notch/flow.py`` (verde azulado,
aguamarina y menta, con un filamento melocotón) y un brillo leve arriba. A 16 px
se lee como un círculo verde azulado con luz clara. Se usa en el icono de la
aplicación y de la bandeja; ``scripts/make_icons.py`` genera los archivos a
partir de aquí.
"""

from __future__ import annotations

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

from .notch.flow import FlowField, colors_for

INK = (0x0B, 0x1F, 0x24)  # fondo opcional del cuadro (verde azulado muy oscuro)
# Fotograma elegido para el icono: formas grandes (una ola diagonal de menta
# sobre verde azulado profundo) que se leen bien hasta a 16 px.
_FRAME_SEED = 7
_FRAME_TIME = 1.5
_FRAME_SCALE = 0.32
_ORB = (24.0, 24.0, 20.0)  # en un cuadro de 48 unidades

SVG = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" width="256" height="256">
  <defs>
    <linearGradient id="deep" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#0E6F7C"/>
      <stop offset="0.55" stop-color="#1FAE9F"/>
      <stop offset="1" stop-color="#8FE3C0"/>
    </linearGradient>
    <clipPath id="orb"><circle cx="24" cy="24" r="20"/></clipPath>
    <filter id="soft"><feGaussianBlur stdDeviation="3"/></filter>
  </defs>
  <circle cx="24" cy="24" r="20" fill="url(#deep)"/>
  <g clip-path="url(#orb)" filter="url(#soft)">
    <ellipse cx="31" cy="29" rx="10" ry="6" fill="#8FE3C0" fill-opacity="0.85"/>
    <ellipse cx="16" cy="31" rx="9" ry="7" fill="#0E6F7C" fill-opacity="0.7"/>
    <ellipse cx="27" cy="21" rx="5" ry="2.6" fill="#FFC9B0" fill-opacity="0.75"/>
  </g>
  <ellipse cx="18" cy="13" rx="8" ry="3.5" fill="#FFFFFF" fill-opacity="0.18"/>
</svg>
"""


def _fluid(size: int) -> np.ndarray:
    """Un fotograma del fluido en un cuadrado ``size``×``size`` (RGB float)."""
    field = FlowField("circle", seed=_FRAME_SEED)
    field.scale = _FRAME_SCALE
    for _ in range(int(_FRAME_TIME * 30)):
        field.step(1 / 30, 0.0, "idle")
    axis = np.linspace(-1.0, 1.0, size, dtype=np.float32)
    gx, gy = np.meshgrid(axis, axis)
    return field.render(gx, gy, "light", colors_for("light", "idle"))


def mark_image(size: int = 256, tile: bool = False) -> Image.Image:
    """La marca en un lienzo cuadrado RGBA; con ``tile`` va sobre un cuadrado oscuro."""
    big = 512
    u = big / 48.0
    cx, cy, r = (v * u for v in _ORB)
    span = int(round(2 * r))
    orb = Image.fromarray(np.clip(_fluid(span), 0, 255).astype(np.uint8), "RGB")
    img = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    img.paste(orb, (int(cx - r), int(cy - r)))

    # Brillo leve arriba: la misma luz de vidrio que la barra.
    shine = Image.new("L", (big, big), 0)
    ImageDraw.Draw(shine).ellipse((10 * u, 9.5 * u, 26 * u, 16.5 * u), fill=46)
    shine = shine.filter(ImageFilter.GaussianBlur(1.6 * u))
    img.paste(Image.new("RGBA", (big, big), (255, 255, 255, 255)), (0, 0), shine)

    # Recorte exacto al círculo.
    mask = Image.new("L", (big, big), 0)
    ImageDraw.Draw(mask).ellipse((cx - r, cy - r, cx + r, cy + r), fill=255)
    out = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    if tile:
        ImageDraw.Draw(out).rounded_rectangle(
            (0, 0, big - 1, big - 1), radius=int(11 * u), fill=(*INK, 255)
        )
    out.paste(img, (0, 0), mask)
    return out.resize((size, size), Image.LANCZOS)
