"""La marca de EzAgent: una «e» dibujada de un solo trazo que sale como un hilo y
termina en un punto violeta (el agente). Se usa en el icono de la aplicación y
de la bandeja; ``scripts/make_icons.py`` genera los archivos a partir de aquí.
"""

from __future__ import annotations

from PIL import Image, ImageDraw

INK = (0x14, 0x12, 0x10)
SIGNAL = (0xD4, 0xF2, 0x5A)
AGENT = (0xA8, 0xA1, 0xFF)

# Geometría en un cuadro de 48 unidades (la misma del SVG de la marca).
_STROKE = 3.4
_CENTER = (22.0, 24.0)
_RADIUS = 11.0
_BAR = ((11.0, 24.0), (33.0, 24.0))
_TAIL = ((29.78, 31.78), (34.0, 35.2), (38.5, 34.8), (43.0, 30.6))   # Bézier cúbica
_DOT = (43.0, 30.6, 3.0)

SVG = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" width="256" height="256">
  <rect width="48" height="48" rx="11" fill="#141210"/>
  <g transform="translate(-3 0)" fill="none" stroke="#D4F25A" stroke-width="3.4" stroke-linecap="round" stroke-linejoin="round">
    <path d="M11 24H33A11 11 0 1 0 29.78 31.78C34 35.2 38.5 34.8 43 30.6"/>
  </g>
  <circle cx="40" cy="30.6" r="3" fill="#A8A1FF"/>
</svg>
"""


def _bezier(p0, p1, p2, p3, n: int = 24):
    pts = []
    for i in range(n + 1):
        t = i / n
        mt = 1 - t
        pts.append((
            mt ** 3 * p0[0] + 3 * mt * mt * t * p1[0] + 3 * mt * t * t * p2[0] + t ** 3 * p3[0],
            mt ** 3 * p0[1] + 3 * mt * mt * t * p1[1] + 3 * mt * t * t * p2[1] + t ** 3 * p3[1],
        ))
    return pts


def mark_image(size: int = 256, tile: bool = True) -> Image.Image:
    """La marca en un lienzo cuadrado RGBA; con ``tile`` va sobre un cuadrado tinta."""
    big = 1024
    u = big / 48.0
    img = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    if tile:
        d.rounded_rectangle((0, 0, big - 1, big - 1), radius=int(11 * u), fill=(*INK, 255))
    dx = -3.0  # centra visualmente la «e» con su cola dentro del tile

    def P(x, y):
        return ((x + dx) * u, y * u)

    w = _STROKE * u
    r = w / 2.0

    def cap(pt):
        x, y = P(*pt)
        d.ellipse((x - r, y - r, x + r, y + r), fill=(*SIGNAL, 255))

    # barra
    d.line([P(*_BAR[0]), P(*_BAR[1])], fill=(*SIGNAL, 255), width=round(w))
    cap(_BAR[0])
    # cuenco: arco de 45° a 360° (sentido horario en pantalla), abierto abajo a la derecha
    cx, cy = P(*_CENTER)
    rr = _RADIUS * u
    d.arc((cx - rr, cy - rr, cx + rr, cy + rr), 45, 360, fill=(*SIGNAL, 255), width=round(w))
    cap(_TAIL[0])
    # cola: el hilo que sale hacia el agente
    tail = [P(*p) for p in _bezier(*_TAIL)]
    d.line(tail, fill=(*SIGNAL, 255), width=round(w), joint="curve")
    cap(_TAIL[-1])
    # punto del agente
    x, y = P(_DOT[0], _DOT[1])
    dr = _DOT[2] * u
    d.ellipse((x - dr, y - dr, x + dr, y + dr), fill=(*AGENT, 255))
    return img.resize((size, size), Image.LANCZOS)
