"""El orbe: la interfaz de EzAgent (sin texto, solo movimiento y color).

Sustituye a la isla/notch. Es una ventana por capas transparente con una esfera
suave y luminosa, sin bordes duros, con color que fluye por dentro:

* **respira** y se deforma muy poco cuando está en calma;
* **se expande con la voz** (la tuya o la de EzAgent) y las manchas de color se
  agrandan y aceleran;
* **tiembla sutilmente** mientras capta sonido y **vuelve a su forma base** al
  callar;
* cambia de paleta según el estado: espera, escucha, piensa, habla, pregunta,
  error y silencio.

Al acercar el puntero brotan dos gotas pequeñas (micrófono y ajustes); una
confirmación pendiente hace brotar ✕ y ✓. Los ajustes se abren en una tarjeta
propia junto al orbe. El recorte de clics se calcula por geometría: todo lo
demás deja pasar el ratón.

Tiene el mismo contrato que ``Notch`` (``island.py``) para que ``app.py`` no
cambie. La animación vive en ``DropModel`` (pura, sin Win32) para probarla.
"""

from __future__ import annotations

import math
import threading
import time

import numpy as np
from PIL import Image, ImageDraw

from . import layered as _shell
from .anim import animations_enabled
from .state import UiState
from .tokens import FLARE, H_ISLAND, R_MENU
from .tokens import W as MENU_W
from .tokens import palette as token_palette

# Lienzo lógico (px a 96 DPI) y geometría de la gota.
LW, LH = 260, 170
CX, CY = LW / 2.0, LH / 2.0
BUD_R = 17.0          # radio de las gotas pequeñas
BUD_X = 56.0          # separación de las gotas pequeñas al centro
HIT_R = 54.0          # el puntero «cerca» hace brotar las gotas pequeñas
SS = 2                # supersampling de la tarjeta
BUD_SS = 3            # supersampling de las gotas pequeñas (el orbe no lo necesita)
HOVER_HOLD = 0.7      # s que las gotas pequeñas siguen tras retirar el puntero

DARK = {
    "ink": (0x14, 0x12, 0x10), "text2": (0xC2, 0xBA, 0xAE), "text3": (0x9C, 0x94, 0x88),
    "signal": (0xD4, 0xF2, 0x5A), "signal_dim": (0x8C, 0xA0, 0x3F), "flow": (0x5E, 0xEA, 0xC4),
    "agent": (0xA8, 0xA1, 0xFF), "attention": (0xFF, 0xB2, 0x3E), "fault": (0xFF, 0x5D, 0x52),
    "line": (0xF4, 0xEF, 0xE8), "white": (255, 255, 255),
}
LIGHT = {
    "ink": (0xFA, 0xF8, 0xF5), "text2": (0x4E, 0x48, 0x40), "text3": (0x6F, 0x67, 0x5C),
    "signal": (0x4D, 0x6B, 0x00), "signal_dim": (0x6F, 0x8A, 0x1F), "flow": (0x0C, 0x91, 0x7E),
    "agent": (0x5B, 0x4F, 0xD6), "attention": (0xA3, 0x5F, 0x00), "fault": (0xC8, 0x24, 0x1A),
    "line": (0x1A, 0x17, 0x14), "white": (255, 255, 255),
}

# Estado visual (de UiState) -> pose de la gota.
POSE_OF = {
    "off": "idle", "idle": "idle", "listen": "listen", "think": "think",
    "speak": "speak", "notice": "done", "mute": "mute", "error": "error",
}


def level_from_rms(rms: float) -> float:
    """RMS lineal (0..~0.3) -> 0..1 perceptual."""
    try:
        value = max(0.0, float(rms or 0.0))
    except (TypeError, ValueError):
        return 0.0
    return min(1.0, (value ** 0.6) * 3.2)


def mix(a, b, t: float) -> tuple[float, float, float]:
    t = max(0.0, min(1.0, t))
    return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t)


class Spring:
    """Muelle amortiguado (k, c, masa 1) con paso fijo de 1/120 s."""

    def __init__(self, x: float = 0.0, k: float = 300.0, c: float = 18.0) -> None:
        self.x = float(x)
        self.v = 0.0
        self.t = float(x)
        self.k = k
        self.c = c

    def step(self, dt: float, instant: bool = False) -> float:
        if instant:
            self.x, self.v = self.t, 0.0
            return self.x
        n = max(1, math.ceil(dt * 120))
        h = dt / n
        for _ in range(n):
            self.v += (-self.k * (self.x - self.t) - self.c * self.v) * h
            self.x += self.v * h
        return self.x


RMAX = 44.0     # semiancho lógico del parche donde se calcula el orbe
GRID = 64       # resolución del parche (se escala suavemente al tamaño real)


def orb_palette(pose: str, p: dict) -> list[tuple[float, float, float]]:
    """Cinco colores que se mezclan por dentro del orbe según el estado."""
    s, f, a, w, x = p["signal"], p["flow"], p["agent"], p["attention"], p["fault"]
    ink, white, t3 = p["ink"], p["white"], p["text3"]
    if pose == "idle":
        return [mix(s, ink, .30), mix(f, ink, .25), mix(s, a, .35), mix(s, ink, .15), mix(a, ink, .40)]
    if pose == "listen":
        return [s, f, mix(s, white, .55), mix(s, f, .5), mix(f, white, .30)]
    if pose == "think":
        return [w, a, mix(w, x, .5), mix(a, w, .4), mix(w, white, .25)]
    if pose in ("speak", "done"):
        return [s, f, a, mix(s, f, .5), mix(f, a, .4)]
    if pose == "ask":
        return [w, mix(w, white, .35), mix(w, x, .3), mix(w, white, .55), w]
    if pose == "error":
        return [x, mix(x, white, .30), mix(x, ink, .30), x, mix(x, w, .30)]
    if pose == "sleep":
        return [mix(t3, ink, .45), mix(t3, ink, .55), mix(t3, ink, .40), mix(t3, ink, .50), mix(t3, ink, .35)]
    return [t3, mix(t3, ink, .20), mix(t3, ink, .35), t3, mix(t3, white, .10)]   # mute


# Posición y velocidad de las cinco manchas: (ωx, ωy, fase x, fase y)
BLOBS = ((.9, 1.3, 0.0, 1.7), (1.1, .8, 2.1, .6), (.7, 1.5, 4.0, 2.3), (1.3, .9, 1.2, 3.4), (.8, 1.1, 5.2, .9))

# Radio base por pose (px lógicos) y cuánto crece con la voz.
BASE_R = {"idle": 24.0, "listen": 25.0, "think": 25.0, "speak": 26.0, "done": 25.0,
          "ask": 26.0, "error": 22.0, "mute": 15.0, "sleep": 17.0}
GROW = {"listen": 9.0, "speak": 11.0}


class DropModel:
    """Animación pura del orbe: sin Win32, sin Pillow, probable en aislamiento.

    El contorno respira y se deforma poco (modos 2–4, lentos). Con sonido se
    expande y aparece un temblor fino (modos 5 y 7, rápidos) que se apaga al
    callar, y la esfera vuelve a su forma base.
    """

    def __init__(self) -> None:
        self.R = 24.0
        self.m = {"a2": .035, "a3": .03, "a4": .02, "tr": 0.0}
        self.ph = [0.0, 1.3, 2.6]          # fases lentas del contorno (k = 2, 3, 4)
        self.trph = [0.0, 0.0]             # fases rápidas del temblor (k = 5, 7)
        self.blob_ph = 0.0
        self.cols = [[float(v) for v in c] for c in orb_palette("idle", DARK)]
        self.bright = .85
        self.lvl = 0.0
        self.peak = 0.008
        self.bud = {n: Spring(0.0) for n in ("mic", "gear", "no", "yes")}
        self.pose = "idle"
        self.shift = 0.0
        self.pose_since = 0.0
        self.ts = 0.0
        self.cy_offset = 0.0
        self.jitter = 0.0
        self.speed = .35

    # ------------------------------------------------------------ entrada
    def update(
        self, now: float, dt: float, pose: str, rms: float, palette: dict,
        hover: bool, asking: bool, menu_open: bool, reduced: bool,
    ) -> None:
        if pose != self.pose:
            self.pose = pose
            self.pose_since = now
        ts = self.ts = now - self.pose_since

        # Nivel con ganancia automática: un micrófono flojo usa todo el rango.
        r = max(0.0, float(rms or 0.0))
        if pose in ("listen", "speak"):
            self.peak = max(0.008, self.peak * 0.9985, r)
            target = min(1.0, (r / self.peak) ** 0.8)
        else:
            self.peak = max(0.008, self.peak * 0.9995)
            target = 0.0
        tau = 0.05 if target > self.lvl else 0.25     # ataque rápido, vuelta suave
        self.lvl += (target - self.lvl) * (1.0 - math.exp(-dt / tau))
        lv = self.lvl

        # Temblor: aparece al captar sonido y se desvanece al callar.
        sounding = pose in ("listen", "speak") and lv > .04
        tr_t = min(1.0, .35 + lv) if sounding else 0.0
        self.m["tr"] += (tr_t - self.m["tr"]) * (1.0 - math.exp(-dt / (.05 if tr_t > self.m["tr"] else .3)))

        base = BASE_R.get(pose, 24.0)
        breath = 0.0 if reduced else math.sin(now * math.tau / 4.2) * .7 + math.sin(now * math.tau / 7.3 + 1.3) * .4
        tgt_r = base + breath + GROW.get(pose, 0.0) * lv
        a = {"a2": .035, "a3": .03, "a4": .02}
        spd = .35
        shift = 0.0
        if pose == "listen":
            a = {"a2": .03 + lv * .07, "a3": .03 + lv * .06, "a4": .02 + lv * .04}
            spd = .8 + lv * 1.6
        elif pose == "think":
            a = {"a2": .06, "a3": .07, "a4": .04}
            spd = 1.6
        elif pose == "speak":
            a = {"a2": .03 + lv * .10, "a3": .03 + lv * .09, "a4": .02 + lv * .06}
            spd = .9 + lv * 1.8
        elif pose == "ask":
            a = {"a2": .04, "a3": .035, "a4": .02}
            spd = .9
            tgt_r += math.sin(now * math.tau / 1.2) * 1.4
        elif pose == "mute":
            a = {"a2": .012, "a3": .01, "a4": 0.0}
            spd = .15
        elif pose == "sleep":
            a = {"a2": .02, "a3": .015, "a4": .01}
            spd = .2
        elif pose == "error":
            shift = math.sin(ts * 45) * 4 * max(0.0, 1 - ts / .5)
            spd = .5
        elif pose == "done":
            tgt_r += math.exp(-ts * 4) * math.cos(ts * 14) * 5
            spd = 1.2

        k = 1.0 if reduced else 1.0 - math.exp(-dt / .14)
        for key in ("a2", "a3", "a4"):
            self.m[key] += (a[key] - self.m[key]) * k
        self.speed += (spd - self.speed) * (1.0 if reduced else 1.0 - math.exp(-dt / .35))
        if not reduced:
            self.ph[0] += dt * self.speed * .9
            self.ph[1] -= dt * self.speed * .7
            self.ph[2] += dt * self.speed * 1.1
            self.trph[0] += dt * 21.0
            self.trph[1] -= dt * 27.0
            self.blob_ph += dt * self.speed
        self.R += (tgt_r - self.R) * (1.0 if reduced else 1.0 - math.exp(-dt / .09))
        self.shift = 0.0 if reduced else shift
        self.cy_offset = 0.0 if reduced else math.sin(now * .7) * 1.2
        self.jitter = 0.0 if reduced else self.m["tr"] * .35 * math.sin(now * 47.0)

        bright_t = {"idle": .85, "mute": .7, "sleep": .6}.get(pose, 1.0 + .22 * lv)
        if pose == "done":
            bright_t += max(0.0, 1 - ts * 2.2) * .45
        self.bright += (bright_t - self.bright) * (1.0 if reduced else 1.0 - math.exp(-dt / .15))
        pal = orb_palette(pose, palette)
        kc = 1.0 - math.exp(-dt / (.12 if reduced else .2))
        for i in range(5):
            for j in range(3):
                self.cols[i][j] += (pal[i][j] - self.cols[i][j]) * kc

        show_pair = hover or menu_open
        self.bud["mic"].t = 1.0 if show_pair and not asking else 0.0
        self.bud["gear"].t = 1.0 if show_pair and not asking else 0.0
        self.bud["no"].t = 1.0 if asking else 0.0
        self.bud["yes"].t = 1.0 if asking else 0.0
        for spring in self.bud.values():
            spring.step(dt, instant=reduced)

    # ------------------------------------------------------------- forma
    def contour(self, th: float) -> float:
        """Factor del radio en el ángulo ``th`` (1.0 = círculo perfecto)."""
        f = 1.0
        for kk, key, i in ((2, "a2", 0), (3, "a3", 1), (4, "a4", 2)):
            f += self.m[key] * math.sin(kk * th + self.ph[i])
        f += self.m["tr"] * (.018 * math.sin(5 * th + self.trph[0]) + .014 * math.sin(7 * th + self.trph[1]))
        return f

    def blob(self) -> list[tuple[float, float]]:
        """Contorno de 72 puntos en coordenadas lógicas del lienzo."""
        cx, cy = CX + self.shift, CY + self.cy_offset
        r0 = self.R + self.jitter
        pts = []
        for i in range(72):
            th = i / 72 * math.tau
            r = r0 * self.contour(th)
            pts.append((cx + r * math.cos(th), cy + r * math.sin(th)))
        return pts

    # -------------------------------------------------------- geometría
    def bud_center(self, name: str) -> tuple[float, float, float]:
        """(x, y, progreso) de una gota pequeña en coordenadas lógicas."""
        p = self.bud[name].x
        side = -1.0 if name in ("mic", "no") else 1.0
        return (CX + side * BUD_X * p, CY, p)

    def bud_at(self, x: float, y: float) -> str | None:
        """Gota pequeña visible bajo el punto (lógico), si hay alguna."""
        for name in ("mic", "gear", "no", "yes"):
            bx, by, p = self.bud_center(name)
            if p > 0.6 and (x - bx) ** 2 + (y - by) ** 2 <= (BUD_R * 1.05) ** 2:
                return name
        return None

    def on_body(self, x: float, y: float) -> bool:
        """¿Está el punto sobre el cuerpo del orbe (con un margen para agarrarlo)?"""
        r = self.R + 6
        return (x - CX) ** 2 + (y - CY) ** 2 <= r * r

    @staticmethod
    def near(x: float, y: float) -> bool:
        return (x - CX) ** 2 + (y - CY) ** 2 <= HIT_R ** 2


_BASIS: dict[int, tuple] = {}


def _basis(n: int):
    """Mallas y bases trigonométricas del parche (se calculan una sola vez)."""
    b = _BASIS.get(n)
    if b is None:
        ax = (np.arange(n, dtype=np.float32) + .5) / n * 2 - 1
        nx, ny = np.meshgrid(ax, ax)
        r = np.sqrt(nx * nx + ny * ny)
        th = np.arctan2(ny, nx)
        sc = {k: (np.sin(k * th), np.cos(k * th)) for k in (2, 3, 4, 5, 7)}
        b = (nx, ny, r, sc)
        _BASIS[n] = b
    return b


def render_orb(model: DropModel, n: int = GRID) -> np.ndarray:
    """Parche RGBA (n×n, recto) del orbe: manchas de color + borde suave."""
    nx, ny, r, sc = _basis(n)
    r0 = (model.R + model.jitter) / RMAX
    edge = np.full_like(r, 1.0)
    for kk, key, i in ((2, "a2", 0), (3, "a3", 1), (4, "a4", 2)):
        s, c = sc[kk]
        edge += model.m[key] * (s * math.cos(model.ph[i]) + c * math.sin(model.ph[i]))
    tr = model.m["tr"]
    if tr > .002:
        s5, c5 = sc[5]
        s7, c7 = sc[7]
        edge += tr * (.018 * (s5 * math.cos(model.trph[0]) + c5 * math.sin(model.trph[0]))
                      + .014 * (s7 * math.cos(model.trph[1]) + c7 * math.sin(model.trph[1])))
    edge *= r0

    # Cinco manchas gaussianas que recorren el interior; con la voz crecen y aceleran.
    lv = model.lvl
    amp = (.34 + .26 * lv) * r0 / .9
    sig2 = ((.5 + .16 * lv) * r0 / .9) ** 2
    acc = np.zeros((n, n, 3), np.float32)
    sw = np.zeros((n, n), np.float32)
    for i, (wx, wy, px, py) in enumerate(BLOBS):
        bx = math.cos(model.blob_ph * wx + px) * amp
        by = math.sin(model.blob_ph * wy + py) * amp
        w = np.exp(-((nx - bx) ** 2 + (ny - by) ** 2) / sig2)
        sw += w
        acc += w[..., None] * np.asarray(model.cols[i], np.float32)
    mixk = np.clip(sw * 1.1, 0.0, 1.0)[..., None]
    base = np.asarray(model.cols[0], np.float32) * .22
    rgb = acc / (sw[..., None] + 1e-6) * mixk + base * (1.0 - mixk)

    e = np.clip(r / np.maximum(edge, 1e-3), 0.0, 1.0)
    rgb *= (model.bright * (.80 + .30 * (1.0 - e) ** 1.6))[..., None]      # centro más luminoso

    # Borde suave: la opacidad cae poco a poco; no hay línea ni contorno duro.
    t = np.clip((r - (edge + .02)) / (-.16 - .02), 0.0, 1.0)
    alpha = t * t * (3.0 - 2.0 * t)
    out = np.empty((n, n, 4), np.uint8)
    out[..., :3] = np.clip(rgb, 0, 255).astype(np.uint8)
    out[..., 3] = (alpha * 255.0).astype(np.uint8)
    return out


# ------------------------------------------------------------------ dibujo
def _stroke(d: ImageDraw.ImageDraw, pts, width: float, fill) -> None:
    d.line(pts, fill=fill, width=max(1, round(width)), joint="curve")
    r = width / 2.0
    for x, y in (pts[0], pts[-1]):
        d.ellipse((x - r, y - r, x + r, y + r), fill=fill)


def _glyph(d, name: str, cx: float, cy: float, u: float, color, width: float,
           muted: bool = False, fault=None) -> None:
    """Glifos en un cuadro de 24 unidades escalado por ``u`` px/unidad."""
    def P(px, py):
        return (cx + (px - 12) * u, cy + (py - 12) * u)

    w = max(1, round(width))
    if name == "mic":
        d.rounded_rectangle((*P(9, 3), *P(15, 14)), radius=3 * u, outline=color, width=w)
        d.arc((*P(5, 4), *P(19, 18)), 0, 180, fill=color, width=w)
        _stroke(d, [P(12, 18), P(12, 21)], width, color)
        if muted:
            _stroke(d, [P(4, 4), P(20, 20)], width * 1.15, fault)
    elif name == "gear":
        r = 3 * u
        c = P(12, 12)
        d.ellipse((c[0] - r, c[1] - r, c[0] + r, c[1] + r), outline=color, width=w)
        for k in range(8):
            ang = k * math.pi / 4
            a = (c[0] + math.cos(ang) * 6.6 * u, c[1] + math.sin(ang) * 6.6 * u)
            b = (c[0] + math.cos(ang) * 9.2 * u, c[1] + math.sin(ang) * 9.2 * u)
            _stroke(d, [a, b], width, color)
    elif name == "no":
        _stroke(d, [P(6, 6), P(18, 18)], width, color)
        _stroke(d, [P(18, 6), P(6, 18)], width, color)
    elif name == "yes":
        _stroke(d, [P(5, 12.5), P(9.5, 17), P(19, 7)], width, color)


class _Canvas:
    """Lienzo RGBA donde cada forma se dibuja en su propia capa y se *compone*.

    ``ImageDraw`` sobre una imagen RGBA sustituye los píxeles en vez de
    mezclarlos; con transparencias hay que componer.
    """

    def __init__(self, w: int, h: int) -> None:
        self.img = Image.new("RGBA", (w, h), (0, 0, 0, 0))

    def custom(self, box, fn, ss: int = 1) -> None:
        """Dibuja con ``fn(d, ox, oy)`` en la caja ``box`` = (x0, y0, x1, y1).

        Con ``ss`` > 1 la capa se dibuja a ``ss``× (fn debe multiplicar sus
        coordenadas por ``ss``) y se reduce antes de componerla.
        """
        x0, y0 = max(0, int(box[0])), max(0, int(box[1]))
        x1, y1 = min(self.img.width, int(box[2]) + 1), min(self.img.height, int(box[3]) + 1)
        if x1 <= x0 or y1 <= y0:
            return
        layer = Image.new("RGBA", ((x1 - x0) * ss, (y1 - y0) * ss), (0, 0, 0, 0))
        fn(ImageDraw.Draw(layer), x0, y0)
        if ss > 1:
            layer = layer.convert("RGBa").reduce(ss).convert("RGBA")
        self.img.alpha_composite(layer, (x0, y0))

    def paste_orb(self, patch: np.ndarray, cx: float, cy: float, half_px: float) -> None:
        """Escala el parche del orbe con suavidad y lo compone centrado en (cx, cy)."""
        size = max(2, round(half_px * 2))
        tile = Image.fromarray(patch, "RGBA").resize((size, size), Image.BICUBIC)
        x0, y0 = round(cx - half_px), round(cy - half_px)
        sx0, sy0 = max(0, -x0), max(0, -y0)
        sx1, sy1 = min(size, self.img.width - x0), min(size, self.img.height - y0)
        if sx1 <= sx0 or sy1 <= sy0:
            return
        if (sx0, sy0, sx1, sy1) != (0, 0, size, size):
            tile = tile.crop((sx0, sy0, sx1, sy1))
        self.img.alpha_composite(tile, (x0 + sx0, y0 + sy0))


def render_model(model: DropModel, w: int, h: int, scale: float, palette: dict,
                 muted: bool, in_call: bool, menu_open: bool, now: float | None = None) -> np.ndarray:
    """Dibuja un fotograma BGRA premultiplicado de ``w``×``h`` px físicos."""
    k = scale
    cv = _Canvas(w, h)
    ink, line = palette["ink"], palette["line"]
    pose = model.pose
    cx = (CX + model.shift) * k
    cy = (CY + model.cy_offset) * k

    # El orbe: color que fluye dentro de una esfera de borde suave (sin halo, anillos ni brillo).
    cv.paste_orb(render_orb(model), cx, cy, RMAX * k)

    if pose == "mute" or (muted and pose in ("idle", "listen")):
        sx0, sy0, sx1, sy1 = cx - 10 * k, cy + 10 * k, cx + 10 * k, cy - 10 * k
        cv.custom((cx - 14 * k, cy - 14 * k, cx + 14 * k, cy + 14 * k),
                  lambda d, ox, oy: _stroke(
                      d, [((sx0 - ox) * BUD_SS, (sy0 - oy) * BUD_SS), ((sx1 - ox) * BUD_SS, (sy1 - oy) * BUD_SS)],
                      2.2 * k * BUD_SS, (*palette["fault"], 255)),
                  ss=BUD_SS)

    # Gotas pequeñas (botones): micrófono, ajustes, ✕ y ✓
    for name in ("mic", "gear", "no", "yes"):
        bx, by, p = model.bud_center(name)
        if p < 0.03:
            continue
        pr = max(0.2, p)
        rad = BUD_R * pr * k
        alpha = int(255 * max(0.0, min(1.0, p * 1.4)))
        bcx, bcy = bx * k, by * k
        if name == "mic":
            gcol = palette["fault"] if muted else palette["text2"]
        elif name == "gear":
            gcol = palette["signal"] if menu_open else palette["text2"]
        elif name == "yes":
            gcol = palette["signal"]
        else:
            gcol = palette["text2"]

        def bud(d, ox, oy, bcx=bcx, bcy=bcy, rad=rad, alpha=alpha, gcol=gcol, pr=pr, name=name):
            q = BUD_SS
            x, y, rr = (bcx - ox) * q, (bcy - oy) * q, rad * q
            d.ellipse((x - rr, y - rr, x + rr, y + rr), fill=(*ink, alpha),
                      outline=(*line, int(alpha * .22)), width=max(1, round(1.0 * k * q)))
            _glyph(d, name, x, y, (16 / 24) * k * q * pr, (*gcol, alpha), 1.7 * k * q * pr,
                   muted=(name == "mic" and muted), fault=(*palette["fault"], alpha))

        cv.custom((bcx - rad - 3, bcy - rad - 3, bcx + rad + 3, bcy + rad + 3), bud, ss=BUD_SS)

    arr = np.asarray(cv.img.convert("RGBa"))
    return np.ascontiguousarray(arr[..., [2, 1, 0, 3]])


def render_card_body(w: int, h: int, scale: float, theme: str) -> np.ndarray:
    """Cuerpo redondeado de la tarjeta de ajustes: float32 RGBA premultiplicado (0..1)."""
    pal = token_palette(theme)
    body = tuple(pal["BODY"][:3])
    line = pal["LINE"]
    img = Image.new("RGBA", (w * SS, h * SS), (0, 0, 0, 0))
    d = ImageDraw.Draw(img, "RGBA")
    x0 = FLARE * scale * SS
    d.rounded_rectangle((x0, 0, w * SS - x0 - 1, h * SS - 1), radius=R_MENU * scale * SS,
                        fill=(*body, 255), outline=(*line[:3], int(line[3])), width=max(1, round(SS * scale)))
    pm = img.convert("RGBa")
    if SS > 1:
        pm = pm.reduce(SS)
    return np.asarray(pm).astype(np.float32) / 255.0


# ----------------------------------------------------------------- ventanas
class DropWindow(_shell.LayeredWindow):
    def __init__(self, ui: "DropUI", x: int, y: int, w: int, h: int) -> None:
        super().__init__(int(w), int(h), int(x), int(y), click_through=False, title="drop")
        self.ui = ui
        self._interval = 1.0 / 30.0

    def render(self):
        return self.ui.render_frame()

    def _install_mouse_hook(self) -> None:
        """La gota no necesita el gancho global de ratón (solo es para la rueda
        del menú): instalarlo hacía que cada movimiento del sistema esperase a
        este hilo."""
        return None

    def _alpha_at(self, x, y):
        """Recorte de clics por geometría: ni un píxel «casi invisible»."""
        s = float(self.ui.s)
        lx, ly = x / s, y / s
        m = self.ui.model
        if m.on_body(lx, ly) or m.bud_at(lx, ly) is not None:
            return 255
        return super()._alpha_at(x, y)

    def _logical(self, x: int, y: int) -> tuple[float, float]:
        s = float(self.ui.s)
        return (x / s, y / s)

    def on_pointer_down(self, x: int, y: int) -> bool:
        return self.ui.pointer_down(*self._logical(x, y))

    def on_pointer_up(self, x: int, y: int) -> bool:
        return self.ui.pointer_up(*self._logical(x, y))

    def cursor_hand(self, x: int, y: int) -> bool:
        lx, ly = self._logical(x, y)
        return self.ui.model.bud_at(lx, ly) is not None

    def on_dpi_changed(self, dpi: int) -> None:
        self.ui.set_scale(max(1.0, dpi / 96.0))

    def clamp_position(self, x: int, y: int) -> tuple[int, int]:
        return self.ui.clamp_position(x, y)

    def on_window_drag_end(self, x: int, y: int) -> None:
        self.ui.drag_end(x, y)


class CardWindow(_shell.LayeredWindow):
    """Ventana de la tarjeta de ajustes; aloja el ``Menu`` existente."""

    def __init__(self, ui: "DropUI", x: int, y: int, w: int, h: int) -> None:
        super().__init__(int(w), int(h), int(x), int(y), click_through=False, title="card")
        self.ui = ui
        self._interval = 1.0 / 60.0

    def render(self):
        return self.ui.render_card()

    def clamp_position(self, x: int, y: int) -> tuple[int, int]:
        return (self.x, self.y)

    def on_pointer_down(self, x: int, y: int) -> bool:
        return self.ui.card_pointer("down", x, y)

    def on_pointer_move(self, x: int, y: int) -> bool:
        return self.ui.card_pointer("move", x, y)

    def on_pointer_up(self, x: int, y: int) -> bool:
        return self.ui.card_pointer("up", x, y)

    def on_wheel(self, x: int, y: int, delta: int) -> bool:
        return self.ui.card_wheel(x, y, delta)

    def on_global_wheel(self, x: int, y: int, delta: int) -> bool:
        return self.ui.card_wheel(x - self.x, y - self.y, delta)

    def cursor_hand(self, x: int, y: int) -> bool:
        return self.ui.card_hand(x, y)

    def on_key(self, vk: int, ctrl: bool) -> bool:
        return self.ui.card_text("key", vk, ctrl)

    def on_char(self, char: str) -> bool:
        return self.ui.card_text("char", char)

    def on_paste(self, text: str) -> bool:
        return self.ui.card_text("paste", text)

    def on_focus_lost(self) -> None:
        self.ui.card_focus_lost()

    def on_dpi_changed(self, dpi: int) -> None:
        self.ui.set_scale(max(1.0, dpi / 96.0))


def _system_light() -> bool:
    try:
        import winreg

        key = winreg.OpenKey(
            winreg.HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        )
        try:
            value, _ = winreg.QueryValueEx(key, "AppsUseLightTheme")
            return bool(int(value))
        finally:
            winreg.CloseKey(key)
    except Exception:
        return False


class DropUI:
    """Controlador de la gota; la app solo habla con esta clase."""

    def __init__(self, root=None, position: tuple[int, int] | None = None, cfg=None) -> None:
        self.root = root
        self.cfg = cfg
        scale = 1.0
        try:
            if root is not None:
                scale = max(1.0, float(root.winfo_fpixels("1i")) / 96.0)
        except Exception:
            scale = 1.0
        self.s = scale
        self._position = position
        self._ui = UiState()
        self.model = DropModel()
        self._win: DropWindow | None = None
        self._visible = False
        self._state = "idle"
        self._rms = 0.0
        self._mic_level = 0.0
        self._muted = False
        self._in_call = False
        self._menu_open = False
        self._asking = False
        self._on_yes = None
        self._on_no = None
        self._hover_until = 0.0
        self._pressed: str | None = None
        self._theme_setting = "auto"
        self._theme_cache = ("dark", 0.0)
        self._last = time.monotonic()
        self._reduced = not animations_enabled()
        self._reduced_at = 0.0
        # tarjeta de ajustes
        self._menu = None
        self._card: CardWindow | None = None
        self._card_above = True
        self._card_lock = threading.RLock()
        self._pending_save: bool | None = None
        # Callbacks de la app (mismo contrato que Notch).
        self.on_action = lambda action: None
        self.on_position_change = lambda x, y: None
        self.on_hide = lambda: None
        self.on_live_setting = lambda key, value: None
        self.on_save_settings = lambda values: None
        self.on_test_voice = lambda values: None
        self.on_menu_action = lambda action: None
        self.menu_settings_provider = lambda: None

    # ---------------------------------------------------------------- ventana
    def _size(self) -> tuple[int, int]:
        return (round(LW * self.s), round(LH * self.s))

    def _default_position(self) -> tuple[int, int]:
        wa = _shell.work_area()
        w, h = self._size()
        return (wa[2] - w - round(8 * self.s), wa[3] - h + round(36 * self.s))

    def _ensure_window(self) -> bool:
        if self._win is None:
            w, h = self._size()
            x, y = self._position if self._position else self._default_position()
            x, y = self.clamp_position(x, y)
            self._win = DropWindow(self, x, y, w, h)
        return self._win.start()

    def show(self, state: str = "idle") -> None:
        self._visible = True
        self._ui.set_app_state(state)
        self._ui.set_visible(True)
        if self._ensure_window():
            self._win.show()

    def hide(self, close_eyes: bool = True) -> None:
        self._visible = False
        self._close_card_now()
        if self._win is not None:
            self._win.hide()

    def close(self) -> None:
        self._close_card_now()
        if self._win is not None:
            self._win.stop()

    # ----------------------------------------------------------------- estado
    def set_state(self, state: str) -> None:
        self._state = state
        self._ui.set_app_state(state)

    def set_muted(self, muted: bool) -> None:
        self._muted = bool(muted)
        self._ui.set_muted(self._muted)

    def set_in_call(self, in_call: bool) -> None:
        self._in_call = bool(in_call)
        self._ui.set_in_call(self._in_call)

    def set_mic_level(self, level: float) -> None:
        try:
            self._mic_level = max(0.0, float(level or 0.0))
        except (TypeError, ValueError):
            self._mic_level = 0.0

    def draw(self, rms: float, state: str | None = None, wave_data=()) -> None:
        self._rms = float(rms or 0.0)
        if state is not None:
            self.set_state(state)

    def notice(self) -> None:
        self._ui.notice(time.monotonic())

    def set_theme(self, theme: str) -> None:
        self._theme_setting = theme if theme in ("auto", "dark", "light") else "auto"
        self._theme_cache = ("dark", 0.0)
        menu = self._menu
        if menu is not None:
            try:
                menu.set_theme(self._theme_name())
            except Exception:
                pass

    def set_scale(self, scale: float) -> None:
        self.s = max(1.0, float(scale))
        if self._win is not None:
            w, h = self._size()
            self._win.resize(w, h)
        if self._menu is not None:
            try:
                self._menu.set_scale(self.s)
            except Exception:
                pass

    # ------------------------------------------------------------ confirmación
    def show_confirmation(self, summary: str, strong: bool, on_yes, on_no) -> None:
        """La pregunta se dice por voz; aquí solo brotan ✕ y ✓ (sin texto)."""
        self._on_yes, self._on_no = on_yes, on_no
        self._asking = True
        if self._menu_open:
            self._close_menu()
        if not self._visible:
            self.show("thinking")

    def hide_confirmation(self) -> None:
        self._asking = False
        self._on_yes = self._on_no = None

    # ------------------------------------------------------------------- tema
    def _palette(self) -> dict:
        if self._theme_setting == "dark":
            return DARK
        if self._theme_setting == "light":
            return LIGHT
        value, at = self._theme_cache
        now = time.monotonic()
        if now - at > 3.0:
            value = "light" if _system_light() else "dark"
            self._theme_cache = (value, now)
        return LIGHT if value == "light" else DARK

    def _theme_name(self) -> str:
        return "light" if self._palette() is LIGHT else "dark"

    # ------------------------------------------------------------------ dibujo
    def _pose(self, now: float) -> str:
        if self._asking:
            return "ask"
        shown = POSE_OF.get(self._ui.update(now), "idle")
        if shown == "idle" and not self._in_call:
            return "sleep"
        return shown

    def _cursor_logical(self) -> tuple[float, float] | None:
        win = self._win
        if win is None:
            return None
        try:
            sx, sy = win._screen_cursor()
        except Exception:
            return None
        return ((sx - win.x) / self.s, (sy - win.y) / self.s)

    def render_frame(self) -> np.ndarray:
        now = time.monotonic()
        dt = min(0.05, max(0.0, now - self._last))
        self._last = now
        if now - self._reduced_at > 2.0:
            self._reduced = not animations_enabled()
            self._reduced_at = now
        cur = self._cursor_logical()
        near = False
        if cur is not None:
            near = DropModel.near(*cur) or self.model.bud_at(*cur) is not None
        if near:
            self._hover_until = now + HOVER_HOLD
        hover = now < self._hover_until
        palette = self._palette()
        pose = self._pose(now)
        self.model.update(now, dt, pose, self._rms, palette, hover, self._asking,
                          self._menu_open, self._reduced)
        if self._win is not None:
            busy = pose not in ("idle", "sleep", "mute") or hover or self._asking
            self._win._interval = 1.0 / (60.0 if busy else 24.0)
        w, h = self._size()
        return render_model(self.model, w, h, self.s, palette, self._muted, self._in_call,
                            self._menu_open, now)

    # ----------------------------------------------------------------- puntero
    def pointer_down(self, x: float, y: float) -> bool:
        name = self.model.bud_at(x, y)
        if name is None:
            return False
        self._pressed = name
        return True

    def pointer_up(self, x: float, y: float) -> bool:
        name, self._pressed = self._pressed, None
        if name is None or self.model.bud_at(x, y) != name:
            return True
        try:
            if name == "mic":
                self.on_action("mute" if self._in_call else "listen")
            elif name == "gear":
                self.on_action("panel")
            elif name == "yes" and self._on_yes:
                self._on_yes()
            elif name == "no" and self._on_no:
                self._on_no()
        except Exception:
            pass
        return True

    def clamp_position(self, x: int, y: int) -> tuple[int, int]:
        """Mantiene la gota (no todo el lienzo) dentro del área de trabajo."""
        wa = _shell.work_area()
        w, h = self._size()
        mx, my = round((CX - 40) * self.s), round((CY - 40) * self.s)
        return (
            max(wa[0] - mx, min(int(x), wa[2] - w + mx)),
            max(wa[1] - my, min(int(y), wa[3] - h + my)),
        )

    def drag_end(self, x: int, y: int) -> None:
        if self.cfg is not None:
            try:
                self.cfg.save_key("drop_x", int(x))
                self.cfg.save_key("drop_y", int(y))
            except Exception:
                pass
        self.on_position_change(int(x), int(y))
        if self._menu_open:
            self._place_card()

    # ----------------------------------------------- tarjeta de ajustes (Menu)
    def open_menu(self) -> None:
        if not self._asking and not self._menu_open:
            self.toggle_menu()

    def toggle_menu(self) -> None:
        if self._asking:
            return
        if self._menu_open:
            self._close_menu()
            return
        try:
            settings = self.menu_settings_provider()
        except Exception:
            settings = None
        if settings is None:
            return
        menu = self._ensure_menu(settings)
        if menu is None or self._win is None:
            return
        now = time.monotonic()
        try:
            menu.set_theme(self._theme_name())
            menu.set_scale(self.s)
            menu.set_reduced_motion(self._reduced)
            menu.set_available_height(self._card_geometry()[2])
            menu.open(now)
        except Exception:
            return
        self._menu_open = True
        self._place_card(create=True)

    def _close_menu(self) -> None:
        menu = self._menu
        if menu is not None:
            try:
                menu.close(time.monotonic())
            except Exception:
                self._menu_closed()

    def _ensure_menu(self, settings):
        if self._menu is None:
            try:
                from .menu import Menu

                screen_h = 1080.0
                try:
                    mon = _shell.monitor_rect()
                    screen_h = (mon[3] - mon[1]) / self.s
                except Exception:
                    pass
                self._menu = Menu(
                    settings, scale=self.s, theme=self._theme_name(), screen_h=screen_h,
                    on_live_change=self._live_setting, on_save=self._save_settings,
                    on_test=self._test_voice, on_hide=self._hide_from_menu,
                    on_close=self._menu_closed, on_text_edit=self._menu_text_edit,
                    on_action=self._menu_action,
                )
            except Exception:
                self._menu = None
        return self._menu

    def _card_geometry(self) -> tuple[bool, int, float]:
        """(abre hacia arriba, x ancla física, alto disponible lógico)."""
        win = self._win
        gx = win.x + CX * self.s
        gy = win.y + CY * self.s
        wa = _shell.work_area()
        gap = 40 * self.s
        above_space = (gy - gap) - wa[1]
        below_space = wa[3] - (gy + gap)
        above = above_space >= below_space
        space = above_space if above else below_space
        return above, int(gx), max(160.0, space / self.s - 12)

    def _place_card(self, create: bool = False) -> None:
        menu, win = self._menu, self._win
        if menu is None or win is None:
            return
        above, gx, avail = self._card_geometry()
        self._card_above = above
        try:
            menu.set_available_height(avail)
            vh = float(menu.viewport_height)
        except Exception:
            vh = 400.0
        w = round(MENU_W * self.s)
        h = round((vh + 4) * self.s)
        wa = _shell.work_area()
        x = max(wa[0] + round(8 * self.s), min(gx - w // 2, wa[2] - w - round(8 * self.s)))
        gy = win.y + CY * self.s
        gap = 40 * self.s
        y = int(gy - gap - h) if above else int(gy + gap)
        with self._card_lock:
            if self._card is None:
                if not create:
                    return
                self._card = CardWindow(self, x, y, w, h)
                self._card.start()
            else:
                self._card.resize(w, h)
                self._card.set_pos(x, y)
            self._card.show()

    def _close_card_now(self) -> None:
        menu = self._menu
        if menu is not None and self._menu_open:
            try:
                menu.close(time.monotonic())
            except Exception:
                pass
        self._menu_open = False
        with self._card_lock:
            card, self._card = self._card, None
        if card is not None:
            card.stop()

    def _menu_closed(self) -> None:
        """El menú terminó de plegarse: se retira la ventana (y su gancho de rueda)."""
        self._menu_open = False
        with self._card_lock:
            card, self._card = self._card, None
        if card is not None:
            card.stop()

    def _menu_text_edit(self, active: bool) -> None:
        card = self._card
        if card is not None:
            try:
                card.set_keyboard_enabled(bool(active))
            except Exception:
                pass

    def _live_setting(self, key: str, value) -> None:
        try:
            self.on_live_setting(key, value)
        except Exception:
            pass

    def _save_settings(self, values: dict):
        try:
            result = self.on_save_settings(values)
        except Exception:
            return False
        return None if result is None else bool(result)

    def _test_voice(self, values: dict) -> None:
        try:
            self.on_test_voice(values)
        except Exception:
            pass

    def _hide_from_menu(self) -> None:
        try:
            self.on_hide()
        except Exception:
            pass

    def _menu_action(self, action: str) -> None:
        try:
            self.on_menu_action(str(action))
        except Exception:
            pass

    def set_testing(self, testing: bool) -> None:
        if self._menu is not None:
            try:
                self._menu.set_testing(bool(testing))
            except Exception:
                pass

    def set_save_result(self, ok: bool) -> None:
        self._pending_save = bool(ok)

    def refresh_menu_settings(self) -> None:
        if self._menu is None:
            return
        try:
            settings = self.menu_settings_provider()
            if settings is not None:
                self._menu.set_settings(settings)
        except Exception:
            pass

    def render_card(self) -> np.ndarray:
        card, menu = self._card, self._menu
        if card is None or menu is None:
            return np.zeros((1, 1, 4), np.uint8)
        s = self.s
        w, h = card.w, card.h
        out = np.zeros((h, w, 4), np.uint8)
        now = time.monotonic()
        pending, self._pending_save = self._pending_save, None
        if pending is not None:
            try:
                menu.on_applied(pending)
            except Exception:
                pass
        try:
            menu.set_input_level(self._mic_level, muted=self._muted,
                                 active=(self._in_call and self._state == "listening"))
            menu.update(now)
            mh = float(menu.height(now))
        except Exception:
            return out
        if self._card is None:           # se cerró durante update()
            return out
        bh = min(h, round(mh * s))
        if bh < 3:
            return out
        y0 = round(H_ISLAND * s)
        buf = np.zeros((max(y0 + bh, round((H_ISLAND + menu.viewport_height) * s) + 4), w, 4), np.float32)
        buf[y0:y0 + bh] = render_card_body(w, bh, s, self._theme_name())
        try:
            menu.draw(buf, now)
        except Exception:
            return out
        crop = np.clip(buf[y0:y0 + bh], 0.0, 1.0)
        arr = (crop * 255.0 + 0.5).astype(np.uint8)[..., [2, 1, 0, 3]]
        if self._card_above:
            out[h - bh:h] = arr
        else:
            out[0:bh] = arr
        try:
            card._interval = 1.0 / (60.0 if menu.is_animating(now) else 30.0)
        except Exception:
            pass
        return out

    def _card_point(self, x: float, y: float) -> tuple[float, float, bool]:
        s = self.s
        card = self._card
        menu = self._menu
        if card is None or menu is None:
            return (0.0, 0.0, False)
        mh = float(menu.height(time.monotonic()))
        lx, ly = x / s, y / s
        top = (card.h / s - mh) if self._card_above else 0.0
        ry = ly - top
        return (lx, ry + H_ISLAND, 0.0 <= ry <= mh)

    def card_pointer(self, kind: str, x: int, y: int) -> bool:
        menu = self._menu
        if menu is None:
            return False
        mx, my, inside = self._card_point(x, y)
        now = time.monotonic()
        try:
            if kind == "down":
                if not inside:
                    return True
                menu.pointer_down(mx, my, now)
            elif kind == "move":
                menu.pointer_move(mx, my, now)
            else:
                menu.pointer_up(mx, my, now)
        except Exception:
            pass
        return True

    def card_wheel(self, x: int, y: int, delta: int) -> bool:
        menu = self._menu
        if menu is None:
            return False
        mx, my, inside = self._card_point(x, y)
        if not inside:
            return False
        try:
            return bool(menu.wheel(mx, my, delta, time.monotonic()))
        except Exception:
            return False

    def card_hand(self, x: int, y: int) -> bool:
        menu = self._menu
        if menu is None:
            return False
        mx, my, inside = self._card_point(x, y)
        if not inside:
            return False
        try:
            return menu.hit(mx, my, time.monotonic()) is not None
        except Exception:
            return False

    def card_text(self, kind: str, *args) -> bool:
        menu = self._menu
        if menu is None or not self._menu_open:
            return False
        try:
            if kind == "key":
                return bool(menu.text_key(*args))
            if kind == "char":
                return bool(menu.text_input(*args))
            return bool(menu.paste_text(*args))
        except Exception:
            return False

    def card_focus_lost(self) -> None:
        menu = self._menu
        if menu is not None:
            try:
                menu.finish_text_edit(commit=True)
            except Exception:
                pass


def create_ui(root, cfg):
    """Crea la interfaz elegida en ``cfg.ui_style`` («drop» o «notch»)."""
    style = str(getattr(cfg, "ui_style", "drop") or "drop").lower()
    if style == "drop":
        x = getattr(cfg, "drop_x", None)
        y = getattr(cfg, "drop_y", None)
        pos = (int(x), int(y)) if isinstance(x, int) and isinstance(y, int) else None
        try:
            return DropUI(root, position=pos, cfg=cfg)
        except Exception:
            pass
    from .island import Notch

    nx = getattr(cfg, "notch_x", None)
    ny = getattr(cfg, "notch_y", None)
    return Notch(root, position=(nx, ny) if isinstance(nx, int) and isinstance(ny, int) else None)
