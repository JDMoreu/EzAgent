"""La gota: la interfaz de EzAgent (sin texto, solo movimiento y color).

Sustituye a la isla/notch. Es una ventana por capas transparente con una gota
líquida que respira siempre y cuenta lo que pasa con forma, color y ondas:

* **te escucha**: anillos que *entran* hacia la gota con tu voz y el cuerpo se
  hincha y aclara con el volumen;
* **piensa**: un cometa ámbar y violeta le da vueltas;
* **habla**: anillos que *salen* con cada sílaba y el color fluye entre lima y
  menta;
* **termina**: destello y chispas; **confirma**: late en ámbar; **error**: rojo.

Al acercar el puntero brotan dos gotas pequeñas (micrófono y ajustes); una
confirmación pendiente hace brotar ✕ y ✓. Los ajustes se abren en una tarjeta
propia junto a la gota. No dibuja zonas «casi invisibles»: el recorte de clics
se calcula por geometría y todo lo demás deja pasar el ratón.

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
SS = 2                # supersampling del dibujo
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
# Intensidad del halo por pose (0..1); crece con el volumen al escuchar/hablar.
HALO = {"idle": .22, "listen": .45, "think": .5, "speak": .5, "done": .8, "ask": .55,
        "error": .6, "mute": 0.0, "sleep": 0.0}


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


def target_color(pose: str, now: float, lvl: float, ts: float, p: dict):
    """Color objetivo de cada pose; se mueve con el tiempo para que «viva»."""
    if pose == "idle":
        return mix(p["signal_dim"], mix(p["flow"], p["ink"], .35), .5 + .5 * math.sin(now * .55))
    if pose == "listen":
        return mix(mix(p["signal"], p["flow"], .25 * lvl), p["white"], lvl * .4)
    if pose == "think":
        return mix(p["attention"], p["agent"], .5 + .5 * math.sin(now * 2.4))
    if pose == "speak":
        flow = .5 + .5 * math.sin(now * 3.2 + lvl * 3.0)
        return mix(mix(p["signal"], p["flow"], flow), p["white"], lvl * .25)
    if pose == "done":
        return mix(p["signal"], p["white"], max(0.0, 1 - ts * 2.2) * .8)
    if pose == "ask":
        return mix(p["attention"], p["white"], (.5 + .5 * math.sin(now * math.tau / 1.2)) * .25)
    if pose == "error":
        return p["fault"]
    if pose == "sleep":
        return mix(p["text3"], p["ink"], .3)
    return p["text3"]  # mute


class DropModel:
    """Animación pura de la gota: sin Win32, sin Pillow, probable en aislamiento."""

    def __init__(self) -> None:
        self.R = 15.0
        self.m = {"a2": .07, "a3": .055, "a4": .035, "a5": .02, "sx": 1.0, "sy": 1.0, "spd": .9}
        self.ph = [0.0, 0.0]
        self.color = [float(v) for v in DARK["signal_dim"]]
        self.lvl = 0.0
        self.peak = 0.008
        self.prev_lvl = 0.0
        self.halo = 0.0
        self.rings: list[tuple[float, str, float]] = []   # (t0, "in"|"out", fuerza)
        self.last_ring = 0.0
        self.bud = {n: Spring(0.0) for n in ("mic", "gear", "no", "yes")}
        self.pose = "idle"
        self.shift = 0.0
        self.pose_since = 0.0
        self.ts = 0.0
        self.cy_offset = 0.0
        self.comet = 0.0

    # ------------------------------------------------------------ entrada
    def update(
        self, now: float, dt: float, pose: str, rms: float, palette: dict,
        hover: bool, asking: bool, menu_open: bool, reduced: bool,
    ) -> None:
        if pose != self.pose:
            self.pose = pose
            self.pose_since = now
            if pose == "done" and not reduced:
                self.rings.append((now, "out", 1.0))
        ts = self.ts = now - self.pose_since

        # Nivel con ganancia automática: sin importar si el micrófono es flojo
        # o la salida es fuerte, la gota usa todo su rango.
        r = max(0.0, float(rms or 0.0))
        if pose in ("listen", "speak"):
            self.peak = max(0.008, self.peak * 0.9985, r)
            target = min(1.0, (r / self.peak) ** 0.8)
        else:
            self.peak = max(0.008, self.peak * 0.9995)
            target = 0.0
        tau = 0.045 if target > self.lvl else 0.2
        self.lvl += (target - self.lvl) * (1.0 - math.exp(-dt / tau))
        lv = self.lvl

        a = {"a2": .07, "a3": .055, "a4": .035, "a5": .02}
        tgt_r = 15.0 + math.sin(now * math.tau / 4.0) * .8
        sx = sy = 1.0
        spd = .9
        shift = 0.0
        if pose == "listen":
            tgt_r = 17 + lv * 9
            a = {"a2": .05 + lv * .12, "a3": .04 + lv * .10, "a4": .03 + lv * .05, "a5": .02}
            spd = 2.2 + lv * 1.5
        elif pose == "think":
            tgt_r = 15
            a = {"a2": .09, "a3": .10, "a4": .06, "a5": .03}
            spd = 3.2
        elif pose == "speak":
            tgt_r = 16 + lv * 8
            a = {"a2": .04 + lv * .24, "a3": .04 + lv * .20, "a4": .03 + lv * .14, "a5": .02 + lv * .10}
            spd = 3.0 + lv * 2.0
        elif pose == "ask":
            sy = .9 + math.sin(now * math.tau / 1.2) * .06
            sx = 1.07
            tgt_r = 16
            a = {"a2": .06, "a3": .05, "a4": .03, "a5": .02}
            spd = 1.6
        elif pose == "mute":
            tgt_r = 9
            a = {"a2": .02, "a3": .015, "a4": 0.0, "a5": 0.0}
            spd = .5
        elif pose == "sleep":
            tgt_r = 10 + math.sin(now * math.tau / 5.0) * .5
            a = {"a2": .03, "a3": .02, "a4": .01, "a5": 0.0}
            spd = .4
        elif pose == "error":
            shift = math.sin(ts * 45) * 4 * max(0.0, 1 - ts / .5)
            tgt_r = 13
        elif pose == "done":
            tgt_r = 15 + math.exp(-ts * 4) * math.cos(ts * 14) * 5

        k = 1.0 if reduced else 1.0 - math.exp(-dt / .14)
        for key in ("a2", "a3", "a4", "a5"):
            self.m[key] += (a[key] - self.m[key]) * k
        self.m["sx"] += (sx - self.m["sx"]) * k
        self.m["sy"] += (sy - self.m["sy"]) * k
        self.m["spd"] += (spd - self.m["spd"]) * (1.0 if reduced else 1.0 - math.exp(-dt / .35))
        if not reduced:
            self.ph[0] += self.m["spd"] * dt
            self.ph[1] += self.m["spd"] * dt * .8
            self.comet += dt * 3.4
        self.R += (tgt_r - self.R) * (1.0 if reduced else 1.0 - math.exp(-dt / .09))
        self.shift = 0.0 if reduced else shift
        self.cy_offset = 0.0 if reduced else math.sin(now * .7) * 1.8

        tc = target_color(pose, now, lv, ts, palette)
        kc = 1.0 - math.exp(-dt / (.12 if reduced else .07))
        for i in range(3):
            self.color[i] += (tc[i] - self.color[i]) * kc

        halo_t = HALO.get(pose, .2)
        if pose in ("listen", "speak"):
            halo_t = .4 + .6 * lv
        elif pose == "done":
            halo_t = max(.25, 1 - ts * 1.6)
        self.halo += (halo_t - self.halo) * (1.0 if reduced else 1.0 - math.exp(-dt / .18))

        # Anillos: hacia dentro con TU voz, hacia fuera con la de EzAgent.
        if not reduced:
            if pose == "listen" and lv > .18 and now - self.last_ring > .24:
                self.rings.append((now, "in", lv))
                self.last_ring = now
            elif pose == "speak" and lv > .32 >= self.prev_lvl and now - self.last_ring > .16:
                self.rings.append((now, "out", lv))
                self.last_ring = now
            elif pose == "ask" and now - self.last_ring > 1.2:
                self.rings.append((now, "out", .6))
                self.last_ring = now
        self.prev_lvl = lv
        self.rings = [t for t in self.rings if now - t[0] < 1.0]

        show_pair = hover or menu_open
        self.bud["mic"].t = 1.0 if show_pair and not asking else 0.0
        self.bud["gear"].t = 1.0 if show_pair and not asking else 0.0
        self.bud["no"].t = 1.0 if asking else 0.0
        self.bud["yes"].t = 1.0 if asking else 0.0
        for spring in self.bud.values():
            spring.step(dt, instant=reduced)

    # ------------------------------------------------------------- forma
    def blob(self) -> list[tuple[float, float]]:
        """Contorno de 72 puntos en coordenadas lógicas del lienzo."""
        R = self.R
        cx = CX + self.shift
        cy = CY + self.cy_offset
        pts = []
        for i in range(72):
            th = i / 72 * math.tau
            r = R
            for kk, key in ((2, "a2"), (3, "a3"), (4, "a4"), (5, "a5")):
                phase = self.ph[0] if kk % 2 else -self.ph[1]
                r += R * self.m[key] * math.sin(kk * th + phase + kk)
            pts.append((cx + r * math.cos(th) * self.m["sx"], cy + r * math.sin(th) * self.m["sy"]))
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
        """¿Está el punto sobre el cuerpo de la gota (con un margen para agarrarla)?"""
        r = self.R * 1.15 + 8
        return (x - CX) ** 2 + (y - CY) ** 2 <= r * r

    @staticmethod
    def near(x: float, y: float) -> bool:
        return (x - CX) ** 2 + (y - CY) ** 2 <= HIT_R ** 2


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


def _scaled(pts, cx, cy, f, dx=0.0, dy=0.0):
    return [(cx + (x - cx) * f + dx, cy + (y - cy) * f + dy) for x, y in pts]


_GLOW_GRIDS: dict[int, np.ndarray] = {}


class _Canvas:
    """Lienzo RGBA donde cada forma se dibuja en una capa propia y se *compone*.

    ``ImageDraw`` sobre una imagen RGBA sustituye los píxeles en vez de
    mezclarlos; con transparencias (halo, brillo, anillos) hay que componer.
    """

    def __init__(self, w: int, h: int) -> None:
        self.img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
        self._fx: Image.Image | None = None
        self._fxd: ImageDraw.ImageDraw | None = None

    def fx(self) -> ImageDraw.ImageDraw:
        """Capa compartida para anillos, cometa y chispas: se compone una sola vez."""
        if self._fx is None:
            self._fx = Image.new("RGBA", self.img.size, (0, 0, 0, 0))
            self._fxd = ImageDraw.Draw(self._fx)
        return self._fxd

    def flush_fx(self) -> None:
        if self._fx is not None:
            self.img.alpha_composite(self._fx)
            self._fx = self._fxd = None

    def _layer(self, x0: float, y0: float, x1: float, y1: float, fn) -> None:
        x0, y0 = max(0, int(x0)), max(0, int(y0))
        x1, y1 = min(self.img.width, int(x1) + 1), min(self.img.height, int(y1) + 1)
        if x1 <= x0 or y1 <= y0:
            return
        layer = Image.new("RGBA", (x1 - x0, y1 - y0), (0, 0, 0, 0))
        fn(ImageDraw.Draw(layer), x0, y0)
        self.img.alpha_composite(layer, (x0, y0))

    def ellipse(self, cx, cy, r, fill=None, outline=None, width=1) -> None:
        pad = width + 1
        self._layer(cx - r - pad, cy - r - pad, cx + r + pad, cy + r + pad,
                    lambda d, ox, oy: d.ellipse((cx - ox - r, cy - oy - r, cx - ox + r, cy - oy + r),
                                                fill=fill, outline=outline, width=max(1, int(width))))

    def polygon(self, pts, fill=None, outline_line=None, line_width=1) -> None:
        xs, ys = [p[0] for p in pts], [p[1] for p in pts]
        pad = line_width + 1

        def fn(d, ox, oy):
            local = [(x - ox, y - oy) for x, y in pts]
            if fill is not None:
                d.polygon(local, fill=fill)
            if outline_line is not None:
                d.line(local + [local[0]], fill=outline_line, width=max(1, int(line_width)))

        self._layer(min(xs) - pad, min(ys) - pad, max(xs) + pad, max(ys) + pad, fn)

    def glow(self, cx: float, cy: float, r_px: float, col, amount: float) -> None:
        """Halo: un único degradado radial (numpy) compuesto una sola vez."""
        extent = int(r_px * 2.9)
        size = 2 * extent + 1
        grid = _GLOW_GRIDS.get(extent)
        if grid is None:
            yy, xx = np.mgrid[0:size, 0:size].astype(np.float32)
            grid = np.sqrt((xx - extent) ** 2 + (yy - extent) ** 2)
            if len(_GLOW_GRIDS) > 12:
                _GLOW_GRIDS.clear()
            _GLOW_GRIDS[extent] = grid
        d = grid / max(r_px, 1.0)
        alpha = np.clip((2.9 - d) / 1.8, 0.0, 1.0) ** 1.6 * (120.0 * amount)
        patch = np.empty((size, size, 4), np.uint8)
        patch[..., 0], patch[..., 1], patch[..., 2] = col[0], col[1], col[2]
        patch[..., 3] = alpha.astype(np.uint8)
        x0, y0 = int(cx) - extent, int(cy) - extent
        # recorte a los límites del lienzo
        sx0, sy0 = max(0, -x0), max(0, -y0)
        sx1 = min(size, self.img.width - x0)
        sy1 = min(size, self.img.height - y0)
        if sx1 <= sx0 or sy1 <= sy0:
            return
        tile = Image.fromarray(patch[sy0:sy1, sx0:sx1], "RGBA")
        self.img.alpha_composite(tile, (x0 + sx0, y0 + sy0))

    def custom(self, box, fn) -> None:
        """Dibuja con ``fn(d, ox, oy)`` en la caja ``box`` = (x0, y0, x1, y1)."""
        self._layer(*box, fn)


def render_model(model: DropModel, w: int, h: int, scale: float, palette: dict,
                 muted: bool, in_call: bool, menu_open: bool, now: float | None = None) -> np.ndarray:
    """Dibuja un fotograma BGRA premultiplicado de ``w``×``h`` px físicos."""
    now = time.monotonic() if now is None else now
    k = scale * SS
    cv = _Canvas(w * SS, h * SS)
    ink, line, white = palette["ink"], palette["line"], palette["white"]
    pose = model.pose
    col = tuple(int(v) for v in model.color)
    R = model.R
    cx = (CX + model.shift) * k
    cy = (CY + model.cy_offset) * k

    # 1) halo: círculos concéntricos que se apilan en un resplandor suave
    if model.halo > .02:
        cv.glow(cx, cy, R * k, col, model.halo)

    # 2) anillos: entran con tu voz, salen con la de EzAgent
    for t0, kind, strength in model.rings:
        age = (now - t0) / (.7 if kind == "in" else .9)
        if not 0 <= age <= 1:
            continue
        if kind == "in":
            rr = (R + (1 - age) * (30 + 14 * strength)) * k
            alpha = int(190 * strength * age)
        else:
            rr = (R + 2 + age * (30 + 18 * strength)) * k
            alpha = int(190 * strength * (1 - age))
        cv.fx().ellipse((cx - rr, cy - rr, cx + rr, cy + rr), outline=(*col, max(0, alpha)),
                        width=max(1, round((1.8 - age) * k)))

    # 3) cometa al pensar: ámbar → violeta, con cola
    if pose == "think":
        fx = cv.fx()
        for i in range(11, -1, -1):
            ang = model.comet - i * .13
            rr = (R + 9) * k
            t = i / 12
            c = mix(palette["attention"], palette["agent"], t)
            px, py, rad = cx + math.cos(ang) * rr, cy + math.sin(ang) * rr, (3.4 - 2.4 * t) * k
            fx.ellipse((px - rad, py - rad, px + rad, py + rad),
                       fill=(int(c[0]), int(c[1]), int(c[2]), int(255 * (1 - t) ** 1.3)))

    cv.flush_fx()

    # 4) cuerpo líquido: borde oscuro, masa de color y brillo desplazado
    pts = [(x * k, y * k) for x, y in model.blob()]
    rim = tuple(int(v) for v in mix(col, ink, .45))
    hi = tuple(int(v) for v in mix(col, white, .55))
    alpha_main = 235 if pose == "idle" else 255
    cv.polygon(pts, fill=(*rim, alpha_main))
    cv.polygon(_scaled(pts, cx, cy, .9), fill=(*col, alpha_main))
    cv.polygon(_scaled(pts, cx, cy, .46, -.22 * R * k, -.3 * R * k), fill=(*hi, 120))
    cv.ellipse(cx - .36 * R * k, cy - .4 * R * k, .13 * R * k, fill=(*white, 150))
    cv.polygon(pts, outline_line=(*ink, 90), line_width=max(1, round(1.0 * k)))

    # 5) chispas al terminar
    if pose == "done" and model.ts < .9:
        for i in range(8):
            ang = i * math.tau / 8 + .3
            dist = (R + 4 + model.ts * 52) * k
            rad = max(.4, (2.6 - model.ts * 2.2)) * k
            a = int(255 * max(0.0, 1 - model.ts / .9))
            px, py = cx + math.cos(ang) * dist, cy + math.sin(ang) * dist
            cv.fx().ellipse((px - rad, py - rad, px + rad, py + rad), fill=(*palette["signal"], a))

    if pose == "mute" or (muted and pose in ("idle", "listen")):
        sx0, sy0, sx1, sy1 = cx - 10 * k, cy + 10 * k, cx + 10 * k, cy - 10 * k
        cv.custom((cx - 14 * k, cy - 14 * k, cx + 14 * k, cy + 14 * k),
                  lambda d, ox, oy: _stroke(d, [(sx0 - ox, sy0 - oy), (sx1 - ox, sy1 - oy)],
                                            2.2 * k, (*palette["fault"], 255)))

    # 6) gotas pequeñas
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
            x, y = bcx - ox, bcy - oy
            d.ellipse((x - rad, y - rad, x + rad, y + rad), fill=(*ink, alpha),
                      outline=(*line, int(alpha * .22)), width=max(1, round(1.0 * k)))
            _glyph(d, name, x, y, (16 / 24) * k * pr, (*gcol, alpha), 1.7 * k * pr,
                   muted=(name == "mic" and muted), fault=(*palette["fault"], alpha))

        cv.custom((bcx - rad - 3, bcy - rad - 3, bcx + rad + 3, bcy + rad + 3), bud)

    pm = cv.img.convert("RGBa")
    if SS > 1:
        pm = pm.reduce(SS)
    arr = np.asarray(pm)
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
