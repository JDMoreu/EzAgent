"""La marca de EzAgent: un orbe de cielo con nubes, el mismo lenguaje que la barra.

Un círculo con un degradado de azul cielo (más intenso arriba, más claro abajo),
dos nubes blancas suaves en la parte baja y un brillo leve arriba a la
izquierda. Se lee bien desde 16 px: a ese tamaño queda un círculo azul con la
base blanca. Se usa en el icono de la aplicación y de la bandeja;
``scripts/make_icons.py`` genera los archivos a partir de aquí.
"""

from __future__ import annotations

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

SKY_TOP = (0x3F, 0x6E, 0xEB)
SKY_BOTTOM = (0xA9, 0xC4, 0xFF)
CLOUD = (0xFF, 0xFF, 0xFF)
INK = (0x11, 0x16, 0x26)  # fondo opcional del cuadro (azul noche)

# Nubes en un cuadro de 48 unidades: (cx, cy, rx, ry).
_CLOUDS = ((17.0, 35.0, 11.0, 7.5), (31.0, 37.5, 12.5, 7.0), (24.0, 31.5, 7.5, 6.0))
_ORB = (24.0, 24.0, 20.0)

SVG = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" width="256" height="256">
  <defs>
    <linearGradient id="sky" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#3F6EEB"/>
      <stop offset="1" stop-color="#A9C4FF"/>
    </linearGradient>
    <clipPath id="orb"><circle cx="24" cy="24" r="20"/></clipPath>
    <filter id="soft"><feGaussianBlur stdDeviation="1.2"/></filter>
  </defs>
  <circle cx="24" cy="24" r="20" fill="url(#sky)"/>
  <g clip-path="url(#orb)" fill="#FFFFFF" filter="url(#soft)">
    <ellipse cx="17" cy="35" rx="11" ry="7.5"/>
    <ellipse cx="31" cy="37.5" rx="12.5" ry="7"/>
    <ellipse cx="24" cy="31.5" rx="7.5" ry="6"/>
  </g>
  <ellipse cx="17" cy="14" rx="7" ry="4" fill="#FFFFFF" fill-opacity="0.22"/>
</svg>
"""


def mark_image(size: int = 256, tile: bool = False) -> Image.Image:
    """La marca en un lienzo cuadrado RGBA; con ``tile`` va sobre un cuadrado azul noche."""
    big = 1024
    u = big / 48.0
    cx, cy, r = (v * u for v in _ORB)

    # Cielo: degradado vertical dentro del orbe.
    t = np.clip((np.arange(big, dtype=np.float32) - (cy - r)) / (2 * r), 0.0, 1.0)[:, None]
    top, bottom = np.asarray(SKY_TOP, np.float32), np.asarray(SKY_BOTTOM, np.float32)
    rows = top * (1.0 - t) + bottom * t
    sky = np.broadcast_to(rows[:, None, :], (big, big, 3)).astype(np.uint8)
    img = Image.fromarray(sky, "RGB").convert("RGBA")

    # Nubes suaves y un brillo arriba a la izquierda.
    clouds = Image.new("L", (big, big), 0)
    d = ImageDraw.Draw(clouds)
    for x, y, rx, ry in _CLOUDS:
        d.ellipse(((x - rx) * u, (y - ry) * u, (x + rx) * u, (y + ry) * u), fill=255)
    clouds = clouds.filter(ImageFilter.GaussianBlur(1.2 * u))
    img.paste(Image.new("RGBA", (big, big), (*CLOUD, 255)), (0, 0), clouds)
    shine = Image.new("L", (big, big), 0)
    ImageDraw.Draw(shine).ellipse((10 * u, 10 * u, 24 * u, 18 * u), fill=56)
    shine = shine.filter(ImageFilter.GaussianBlur(1.5 * u))
    img.paste(Image.new("RGBA", (big, big), (*CLOUD, 255)), (0, 0), shine)

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
