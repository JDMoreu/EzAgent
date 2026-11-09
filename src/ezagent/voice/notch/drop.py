"""La barra de EzAgent: una forma fija con cielo y nubes dentro (sin texto).

Dos disposiciones (``ui_layout`` en ``voice.json``):

* ``pill``: ☰ a la izquierda de la cápsula de color; 🎤 y ✕ a la derecha;
* ``circle``: un círculo de color sobre una fila corta con los mismos botones.

**El contorno no se deforma nunca**: es fijo y nítido. Dentro hay un cielo con
nubes que tienen inercia (``clouds.py``): cuando hablas entra aire por abajo,
las empuja y las hincha; al callar vuelven despacio con un vaivén. Los colores
cambian con el estado (espera, escucha, piensa, habla, pregunta, error, pausa)
y con el tema (claro u oscuro). Los botones solo aparecen al acercar el
puntero: la forma crece un 4 % y nacen desde detrás de ella (``bar_motion.py``).

El micrófono tiene tres estados: escuchando, en pausa (tachado; solo atiende a
la frase de reactivación) y colgado. Al colgar, el micrófono se pliega y ✕ se
convierte en el botón de llamar. Una confirmación pendiente oculta ☰,
convierte 🎤 en ✓ y deja ✕ como «no». Los ajustes se abren en una tarjeta propia
que se despliega desde ☰. El recorte de clics se calcula por geometría: todo lo
que no es forma o botón deja pasar el ratón.

Expone el contrato que ``app.py`` espera de la interfaz (``show``, ``hide``,
``set_state``…) para que apenas
cambie. La animación vive en ``DropModel`` (pura, sin Win32) para probarla.
"""

from __future__ import annotations

import math
import threading
import time
from functools import lru_cache

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

from . import layered as _shell
from .anim import animations_enabled
from .bar_motion import Choreo, Spring, emerge
from .bar_theme import CARD_RADIUS, body_top, card_overrides, card_palette
from .clouds import CloudField, colors_for
from .state import UiState
from .tokens import FLARE, H_ISLAND, R_MENU
from .tokens import W as MENU_W
from .tokens import palette as token_palette

# ----------------------------------------------------------------- geometría
BTN = 36.0              # diámetro de los botones redondos
GAP = 8.0               # separación entre elementos de la fila
CALL_GAP = 10.0         # respiro extra de «llamar» cuando el micro está plegado
PAD = 12.0              # margen para sombras dentro de la ventana
PILL_W, PILL_H = 148.0, 38.0
CIRCLE_D = 58.0
ORB_GAP = 10.0          # separación entre el círculo y la fila de botones
HOVER_MARGIN = 22.0     # el puntero «cerca» de la barra hace aparecer los botones
DOUBLE_CLICK = 0.4      # s entre dos clics sobre la forma para cambiar pill ↔ circle
HOVER_HOLD = 0.7        # s que los botones siguen tras retirar el puntero
BUD_SS = 3              # supersampling de los sprites de los botones
GRID_CELL = 2.0         # px lógicos por celda del fluido (se escala suavemente)
SHADOW = 10.0           # margen de sombra de cada sprite
FPS_BUSY = 60.0         # mientras algo se mueve: botones, voz, crecimiento, iconos
FPS_CALM = 30.0         # en reposo; el fluido sigue fluyendo despacio

# DECISION: tres botones (☰ menú, 🎤 micrófono, ✕ colgar). «Repetir» pasa al
# menú y a la voz/atajo; «nueva conversación» vive en el menú con confirmación.
# Sin llamada el micrófono se pliega y ✕ se convierte en «llamar» (mismo botón).
NAMES = {"pill": ("gear", "pill", "mic", "hang"),
         "circle": ("gear", "mic", "hang")}
BUTTONS = ("gear", "mic", "hang")
# Orden de salida por grupos: del hueco más cercano a la forma al más lejano.
EMERGE_ORDER = {"pill": (("gear", "mic"), ("hang",)), "circle": (("mic",), ("gear", "hang"))}

LAYOUTS = ("pill", "circle")
# DECISION: los botones solo aparecen al acercar el puntero; «always» queda para
# las pruebas y no se ofrece en el menú.
MOTION_MODES = ("system", "full", "reduced")


def layout_size(layout: str) -> tuple[float, float]:
    """Tamaño lógico de la ventana según la disposición."""
    if layout == "circle":
        return (3 * BTN + 2 * GAP + 2 * PAD, PAD + CIRCLE_D + ORB_GAP + BTN + PAD)
    return (3 * BTN + PILL_W + 3 * GAP + 2 * PAD, 2 * PAD + max(BTN, PILL_H))


def row_center_y(layout: str) -> float:
    _, h = layout_size(layout)
    return (PAD + CIRCLE_D + ORB_GAP + BTN / 2) if layout == "circle" else h / 2


def slots(layout: str, mic: float = 1.0) -> dict[str, tuple[float, float]]:
    """Centro de cada elemento. ``mic`` (0..1) es el hueco del micrófono: sin
    llamada se pliega y ✕/llamar se desliza a su sitio; la forma no se mueve."""
    w, _ = layout_size(layout)
    cy = row_center_y(layout)
    mic = max(0.0, min(1.0, mic))
    out: dict[str, tuple[float, float]] = {}
    names = NAMES[layout]
    widths = [PILL_W if n == "pill" else (BTN * mic if n == "mic" else BTN) for n in names]
    # Al plegarse el micro, su espacio se convierte en un respiro más ancho
    # (GAP + CALL_GAP): «llamar» no queda pegado a la forma ni a ☰.
    gaps = [(GAP * mic + CALL_GAP * (1.0 - mic)) if a == "mic" else GAP
            for a, b in zip(names, names[1:], strict=False)]
    x = PAD
    if layout == "circle":                 # la fila se centra bajo el círculo
        x = w / 2 - (sum(widths) + sum(gaps)) / 2
    for i, (n, width) in enumerate(zip(names, widths, strict=True)):
        out[n] = (x + width / 2, cy)
        x += width + (gaps[i] if i < len(gaps) else 0.0)
    if layout == "circle":
        out["pill"] = (w / 2, PAD + CIRCLE_D / 2)
    return out


def form_center(layout: str) -> tuple[float, float]:
    """Centro de la forma de color (cápsula o círculo)."""
    return slots(layout)["pill"]


# --------------------------------------------------------------------- color
# Estado visual (de UiState) -> pose de la barra.
POSE_OF = {
    "off": "idle", "idle": "idle", "listen": "listen", "think": "think",
    "speak": "speak", "notice": "speak", "mute": "mute", "error": "error",
}
BRIGHT = {"mute": .9, "sleep": .97}

# Botones por tema: superficies frescas (blanco / azul noche) como la tarjeta.
UI = {
    "light": {"disc": (255, 255, 255, 245), "hover": (255, 255, 255, 255), "icon": (30, 36, 56),
              "fault": (214, 51, 61), "hang": (24, 30, 48, 255), "hang_icon": (255, 255, 255),
              "call": (47, 95, 208, 255), "call_icon": (255, 255, 255),
              "ring": (255, 255, 255, 56), "shadow": (30, 50, 110, 64)},
    "dark": {"disc": (24, 30, 46, 245), "hover": (34, 42, 62, 255), "icon": (232, 238, 250),
             "fault": (255, 120, 118), "hang": (232, 238, 250, 255), "hang_icon": (18, 22, 34),
             "call": (141, 180, 255, 255), "call_icon": (14, 22, 44),
             "ring": (232, 238, 250, 30), "shadow": (0, 0, 0, 120)},
}
# Color de «activo» (☰ con el menú abierto, ✓).
SIGNAL = {"light": (47, 95, 208), "dark": (141, 180, 255)}


# ------------------------------------------------------------ nubes y aire
_GRIDS: dict[str, tuple[np.ndarray, np.ndarray]] = {}


def _form_dims(layout: str) -> tuple[float, float]:
    return (CIRCLE_D, CIRCLE_D) if layout == "circle" else (PILL_W, PILL_H)


def _fluid_grid(layout: str) -> tuple[np.ndarray, np.ndarray]:
    """Cuadrícula gruesa en unidades de media altura de la forma (se calcula una vez)."""
    g = _GRIDS.get(layout)
    if g is None:
        w, h = _form_dims(layout)
        nx, ny = max(2, round(w / GRID_CELL)), max(2, round(h / GRID_CELL))
        unit = h / 2
        xs = (((np.arange(nx) + .5) * (w / nx) - w / 2) / unit).astype(np.float32)
        ys = (((np.arange(ny) + .5) * (h / ny) - h / 2) / unit).astype(np.float32)
        g = tuple(np.meshgrid(xs, ys))
        _GRIDS[layout] = g
    return g


def render_fluid(model: DropModel, now: float = 0.0) -> np.ndarray:
    """Interior de la forma (cielo y nubes): RGB uint8 en la cuadrícula gruesa."""
    gx, gy = _fluid_grid(model.layout)
    img = model.field.render(gx, gy, model.theme, model.cols)
    return np.clip(img * model.bright, 0, 255).astype(np.uint8)


_MASKS: dict[tuple, Image.Image] = {}


def form_mask(layout: str, scale: float) -> Image.Image:
    """Máscara exacta (modo L) de la forma a la resolución final: borde fijo y nítido."""
    key = (layout, round(scale * 100))
    m = _MASKS.get(key)
    if m is None:
        if layout == "circle":
            w = h = max(2, round(CIRCLE_D * scale))
            xs = np.arange(w, dtype=np.float32) + .5 - w / 2
            ys = np.arange(h, dtype=np.float32) + .5 - h / 2
            xx, yy = np.meshgrid(xs, ys)
            dist = np.sqrt(xx * xx + yy * yy) - w / 2
        else:
            w, h = max(2, round(PILL_W * scale)), max(2, round(PILL_H * scale))
            xs = np.arange(w, dtype=np.float32) + .5 - w / 2
            ys = np.arange(h, dtype=np.float32) + .5 - h / 2
            xx, yy = np.meshgrid(xs, ys)
            r = h / 2
            dx = np.maximum(np.abs(xx) - (w / 2 - r), 0.0)
            dist = np.sqrt(dx * dx + yy * yy) - r
        alpha = np.clip(.5 - dist, 0.0, 1.0)
        m = Image.fromarray((alpha * 255).astype(np.uint8), "L")
        if len(_MASKS) > 32:            # el crecimiento al acercar usa varias escalas
            _MASKS.clear()
        _MASKS[key] = m
    return m


# ------------------------------------------------------------------- modelo
class DropModel:
    """Animación pura de la barra: sin Win32, sin Pillow, probable en aislamiento.

    El contorno es fijo. Dentro hay un cielo con nubes (``clouds.CloudField``)
    que el aire de la voz empuja; fuera, el leve crecimiento al acercar el
    puntero y la coreografía de los botones. Sin llamada, el micrófono se
    pliega y ✕ se convierte en el botón de llamar.
    """

    def __init__(self, layout: str = "pill") -> None:
        self.layout = layout if layout in LAYOUTS else "pill"
        self.field = CloudField(self.layout)
        self.theme = "light"
        self.cols = colors_for("light", "idle").copy()
        self._colored = False
        self.bright = 1.0
        self.lvl = 0.0
        self.peak = 0.008
        self.pose = "idle"
        self.pose_since = 0.0
        self.ts = 0.0
        self.reduced = False
        self.asking = False
        self.choreo = Choreo(BUTTONS, shown=False)   # ocultos hasta acercar el puntero
        self.appear = self.choreo.appear                               # aparición de cada botón
        self.ask = Spring(0.0, 300.0, 26.0)                            # 🎤 → ✓ al preguntar
        self.mute_p = Spring(0.0, 420.0, 41.0)                         # raya del micro en pausa
        self.call = Spring(0.0, 300.0, 26.0)                           # ✕ → llamar al colgar
        self.mic_space = Spring(1.0, 260.0, 32.0)                      # hueco del micro en la fila

    @property
    def flow(self) -> float:
        """Tiempo interno de las nubes (no avanza con movimiento reducido)."""
        return self.field.t

    # ------------------------------------------------------------ entrada
    def update(
        self, now: float, dt: float, pose: str, rms: float, theme: str,
        hover: bool, asking: bool, menu_open: bool, reduced: bool, buttons: str = "hover",
        *, muted: bool = False, hover_item: str | None = None, pressed: str | None = None,
        in_call: bool = True,
    ) -> None:
        self.reduced = reduced
        self.asking = asking
        if pose != self.pose:
            self.pose = pose
            self.pose_since = now
        self.ts = now - self.pose_since
        if self.field.layout != self.layout:
            self.field = CloudField(self.layout)
        self._update_air(dt, pose, rms, theme, reduced)

        show = hover or asking or menu_open or buttons == "always"
        live = in_call or asking
        want = {"gear": show and not asking, "mic": show and live, "hang": show}
        self.choreo.set_targets(want, EMERGE_ORDER[self.layout], now, reduced)
        self.choreo.step(now, dt, reduced, grow=hover and not asking,
                         hover_item=hover_item, pressed=pressed)
        self.ask.t = 1.0 if asking else 0.0
        self.mute_p.t = 1.0 if muted else 0.0
        self.call.t = 0.0 if live else 1.0
        self.mic_space.t = 1.0 if live else 0.0
        for spring in (self.ask, self.mute_p, self.call, self.mic_space):
            if reduced:
                spring.fade(dt)
            else:
                spring.step(dt)

    def _update_air(self, dt: float, pose: str, rms: float, theme: str, reduced: bool) -> None:
        # Nivel con ganancia automática: un micrófono flojo usa todo el rango.
        r = max(0.0, float(rms or 0.0))
        if pose in ("listen", "speak"):
            self.peak = max(0.008, self.peak * 0.9985, r)
            target = min(1.0, (r / self.peak) ** 0.8)
        else:
            self.peak = max(0.008, self.peak * 0.9995)
            target = 0.0
        tau = 0.05 if target > self.lvl else 0.25
        self.lvl += (target - self.lvl) * (1.0 - math.exp(-dt / tau))
        self.field.step(dt, self.lvl, pose, reduced)

        bright_t = BRIGHT.get(pose, 1.0)
        if pose == "done":
            bright_t += max(0.0, 1 - self.ts * 2.2) * .2
        self.bright += (bright_t - self.bright) * (1.0 if reduced else 1.0 - math.exp(-dt / .15))

        self.theme = "light" if theme == "light" else "dark"
        # El primer fotograma ya sale con el color correcto (sin fundido desde el claro).
        kc = 1.0 if not self._colored else 1.0 - math.exp(-dt / (.12 if reduced else .25))
        self._colored = True
        self.cols += (colors_for(self.theme, pose) - self.cols) * kc

    def animating(self, now: float) -> bool:
        """¿Hay botones, crecimiento o iconos a medio camino? (pide 60 fps)."""
        springs = (self.ask, self.mute_p, self.call, self.mic_space)
        return self.choreo.animating(now) or not all(s.settled() for s in springs)

    # -------------------------------------------------------- geometría
    def positions(self) -> dict[str, tuple[float, float, float, float]]:
        """Para cada elemento: (cx, cy, escala, opacidad), en coordenadas lógicas.

        Los botones viajan desde el centro de la forma hasta su hueco; la forma
        nunca se mueve (su «escala» es el leve crecimiento al acercar).
        """
        sl = slots(self.layout, self.mic_space.x)
        origin = sl["pill"]
        out: dict[str, tuple[float, float, float, float]] = {
            "pill": (origin[0], origin[1], self.choreo.grow.x, 1.0)
        }
        for n in BUTTONS:
            cx, cy, sc, al = emerge(sl[n], origin, self.choreo.appear[n].x, self.reduced)
            out[n] = (cx, cy, sc * self.choreo.pointer[n].x, al)
        return out

    def item_at(self, x: float, y: float) -> str | None:
        """Botón visible bajo el punto, o ``'pill'`` si es la forma (para arrastrar)."""
        pos = self.positions()
        for name in BUTTONS:
            cx, cy, sc, al = pos[name]
            if sc > .6 and al > .5 and (x - cx) ** 2 + (y - cy) ** 2 <= (BTN / 2 * 1.05) ** 2:
                return name
        cx, cy, _, _ = pos["pill"]
        if self.layout == "circle":
            return "pill" if (x - cx) ** 2 + (y - cy) ** 2 <= (CIRCLE_D / 2) ** 2 else None
        r = PILL_H / 2
        dx = max(abs(x - cx) - (PILL_W / 2 - r), 0.0)
        return "pill" if dx * dx + (y - cy) ** 2 <= r * r else None

    def bar_rect(self) -> tuple[float, float, float, float]:
        """Rectángulo de forma y huecos de botones, con margen (para el «cerca»).

        Usa los huecos completos, no la posición animada: acercarse a donde
        estará un botón oculto también lo hace aparecer.
        """
        xs, ys = [], []
        for name, (cx, cy) in slots(self.layout).items():
            if name == "pill":
                half_w, half_h = ((PILL_W / 2, PILL_H / 2) if self.layout == "pill"
                                  else (CIRCLE_D / 2, CIRCLE_D / 2))
            else:
                half_w = half_h = BTN / 2
            xs += [cx - half_w, cx + half_w]
            ys += [cy - half_h, cy + half_h]
        return (min(xs) - HOVER_MARGIN, min(ys) - HOVER_MARGIN, max(xs) + HOVER_MARGIN, max(ys) + HOVER_MARGIN)

    def near(self, x: float, y: float) -> bool:
        x0, y0, x1, y1 = self.bar_rect()
        return x0 <= x <= x1 and y0 <= y <= y1


# ------------------------------------------------------------------- dibujo
def _stroke(d: ImageDraw.ImageDraw, pts, width: float, fill) -> None:
    d.line(pts, fill=fill, width=max(1, round(width)), joint="curve")
    r = width / 2.0
    for x, y in (pts[0], pts[-1]):
        d.ellipse((x - r, y - r, x + r, y + r), fill=fill)


def _glyph(d, name: str, cx: float, cy: float, u: float, color, width: float,
           muted: float = 0.0, fault=None) -> None:
    """Glifos en un cuadro de 24 unidades escalado por ``u`` px/unidad."""
    def P(px, py):
        return (cx + (px - 12) * u, cy + (py - 12) * u)

    w = max(1, round(width))
    if name == "mic":
        d.rounded_rectangle((*P(9, 3), *P(15, 14)), radius=3 * u, outline=color, width=w)
        d.arc((*P(5, 4), *P(19, 18)), 0, 180, fill=color, width=w)
        _stroke(d, [P(12, 18), P(12, 21)], width, color)
        if muted > 0.0:
            # La raya de la pausa se dibuja de arriba abajo según el progreso.
            end = 4 + 16 * min(1.0, float(muted))
            _stroke(d, [P(4, 4), P(end, end)], width * 1.15, fault)
    elif name == "gear":
        # DECISION: hamburguesa (tres rayas) en vez de rueda dentada: a 36 px un
        # engranaje de 8 dientes se leía como un sol («brillo»).
        for y in (6.5, 12, 17.5):
            _stroke(d, [P(5, y), P(19, y)], width, color)
    elif name == "repeat":
        d.arc((*P(4, 4), *P(20, 20)), -132.5, 180, fill=color, width=w)
        _stroke(d, [P(4, 4), P(4, 8.5), P(8.5, 8.5)], width, color)
    elif name in ("no", "hang"):
        _stroke(d, [P(6, 6), P(18, 18)], width, color)
        _stroke(d, [P(18, 6), P(6, 18)], width, color)
    elif name == "yes":
        _stroke(d, [P(5, 12.5), P(9.5, 17), P(19, 7)], width, color)
    elif name == "call":
        # Ondas de voz: «empezar a hablar» (sustituye a ✕ cuando no hay llamada).
        for x, half in ((5.8, 2.8), (9.9, 7.0), (14.1, 4.9), (18.2, 2.0)):
            _stroke(d, [P(x, 12 - half), P(x, 12 + half)], width * 1.3, color)


_SPRITES: dict[tuple, Image.Image] = {}


def button_sprite(glyph: str, theme: str, scale: float, *, hang: bool = False,
                  hover: bool = False, muted: float = 0.0, signal: bool = False,
                  call: bool = False) -> Image.Image:
    """Botón redondo con sombra suave (RGBA recto). Se dibuja una vez y se reutiliza.

    ``muted`` (0..1) es el progreso de la raya de pausa; se cuantiza en 8 pasos
    para que la animación reutilice sprites en caché.
    """
    muted = round(max(0.0, min(1.0, float(muted))) * 8) / 8
    theme = "light" if theme == "light" else "dark"
    key = (glyph, theme, round(scale * 100), hang, hover, muted, signal, call)
    sp = _SPRITES.get(key)
    if sp is not None:
        return sp
    ui = UI[theme]
    q = BUD_SS
    size = round((BTN + 2 * SHADOW) * scale)
    big = size * q
    c, r = big / 2, BTN * scale * q / 2
    shadow = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).ellipse((c - r, c - r + 3 * scale * q, c + r, c + r + 3 * scale * q), fill=ui["shadow"])
    shadow = shadow.filter(ImageFilter.GaussianBlur(4 * scale * q))
    disc = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    d = ImageDraw.Draw(disc)
    fill = ui["call"] if call else (ui["hang"] if hang else (ui["hover"] if hover else ui["disc"]))
    ring = ui["ring"] if (hang or call or theme == "dark") else None
    d.ellipse((c - r, c - r, c + r, c + r), fill=fill, outline=ring, width=max(1, round(1.0 * scale * q)))
    if call:
        icon = ui["call_icon"]
    elif hang:
        icon = ui["hang_icon"]
    else:
        icon = ui["fault"] if muted >= .5 else (SIGNAL[theme] if signal else ui["icon"])
    k = BTN / 46.0
    _glyph(d, glyph, c, c, (20 / 24) * k * scale * q * 0.95, (*icon, 255), 1.8 * k * scale * q,
           muted=muted, fault=(*ui["fault"], 255))
    out = Image.alpha_composite(shadow, disc).convert("RGBa").reduce(q).convert("RGBA")
    if len(_SPRITES) > 120:
        _SPRITES.clear()
    _SPRITES[key] = out
    return out


class _Canvas:
    """Lienzo RGBA donde cada elemento se compone sobre los demás."""

    def __init__(self, w: int, h: int) -> None:
        self.img = Image.new("RGBA", (w, h), (0, 0, 0, 0))

    def paste(self, tile: Image.Image, cx: float, cy: float, scale: float = 1.0, alpha: float = 1.0) -> None:
        if alpha <= 0.01 or scale <= 0.02:
            return
        if abs(scale - 1.0) > .02:
            tile = tile.resize((max(1, round(tile.width * scale)), max(1, round(tile.height * scale))), Image.BILINEAR)
        if alpha < .99:
            a = tile.getchannel("A").point(lambda v, f=alpha: int(v * f))
            tile = tile.copy()
            tile.putalpha(a)
        x0, y0 = round(cx - tile.width / 2), round(cy - tile.height / 2)
        sx0, sy0 = max(0, -x0), max(0, -y0)
        sx1, sy1 = min(tile.width, self.img.width - x0), min(tile.height, self.img.height - y0)
        if sx1 <= sx0 or sy1 <= sy0:
            return
        if (sx0, sy0, sx1, sy1) != (0, 0, tile.width, tile.height):
            tile = tile.crop((sx0, sy0, sx1, sy1))
        self.img.alpha_composite(tile, (x0 + sx0, y0 + sy0))


def render_model(model: DropModel, w: int, h: int, scale: float, theme: str,
                 muted: bool, in_call: bool, menu_open: bool, now: float | None = None,
                 hover_item: str | None = None) -> np.ndarray:
    """Dibuja un fotograma BGRA premultiplicado de ``w``×``h`` px físicos."""
    now = time.monotonic() if now is None else now
    cv = _Canvas(w, h)
    s = scale
    pos = model.positions()

    # 1) botones primero: al nacer quedan detrás de la forma y salen de ella
    ask = max(0.0, min(1.0, model.ask.x))
    mp = model.mute_p
    mute_progress = mp.x if (mp.t > 0 or not muted) else 1.0
    # Los lejanos primero: al salir pasan por detrás de los cercanos, no encima.
    draw_order = [n for group in reversed(EMERGE_ORDER[model.layout]) for n in group]
    for name in draw_order:
        cx, cy, sc, al = pos[name]
        if sc <= .02 or al <= .01:
            continue
        hov = hover_item == name
        if name == "gear":
            tiles = [(button_sprite("gear", theme, s, hover=hov, signal=menu_open), 1.0)]
        elif name == "hang":                # al preguntar, ✕ significa «no»
            call = 0.0 if model.asking else max(0.0, min(1.0, model.call.x))
            tiles = [(button_sprite("hang", theme, s, hang=True, hover=hov), 1.0 - call),
                     (button_sprite("call", theme, s, call=True, hover=hov), call)]
        else:  # mic / ✓
            tiles = [(button_sprite("mic", theme, s, hover=hov, muted=mute_progress), 1.0 - ask),
                     (button_sprite("yes", theme, s, hover=hov, signal=True), ask)]
        for tile, share in tiles:
            cv.paste(tile, cx * s, cy * s, sc, al * share)

    # 2) la forma: interior fluido + máscara exacta (contorno nítido a cualquier escala)
    fx, fy, grow, _ = pos["pill"]
    mask = form_mask(model.layout, s * grow)
    interior = Image.fromarray(render_fluid(model, now), "RGB").resize(mask.size, Image.BICUBIC)
    interior.putalpha(mask)
    cv.paste(interior, fx * s, fy * s)

    arr = np.asarray(cv.img.convert("RGBa"))
    return np.ascontiguousarray(arr[..., [2, 1, 0, 3]])


@lru_cache(maxsize=48)
def render_card_body(w: int, h: int, scale: float, theme: str, fresh: bool = False) -> np.ndarray:
    """Cuerpo redondeado de la tarjeta de ajustes: float32 RGBA premultiplicado (0..1).

    Con ``fresh`` usa las superficies de la barra (``bar_theme``); sin él, los tokens.

    Se cachea: con la tarjeta abierta el cuerpo no cambia y rehacerlo con Pillow en
    cada cuadro costaba ~17 ms (el 40 % del cuadro) en el hilo que atiende el ratón.
    El resultado es de solo lectura: quien lo use debe copiarlo, no modificarlo.
    """
    pal = card_palette(theme) if fresh else token_palette(theme)
    body = tuple(pal["BODY"][:3])
    line = pal["LINE"]
    ss = 2
    img = Image.new("RGBA", (w * ss, h * ss), (0, 0, 0, 0))
    d = ImageDraw.Draw(img, "RGBA")
    x0 = FLARE * scale * ss
    radius = (CARD_RADIUS if fresh else R_MENU) * scale * ss
    d.rounded_rectangle((x0, 0, w * ss - x0 - 1, h * ss - 1), radius=radius,
                        fill=(*body, 255), outline=(*line[:3], int(line[3])), width=max(1, round(ss * scale)))
    arr = np.asarray(img.convert("RGBa").reduce(ss)).astype(np.float32) / 255.0
    if fresh:
        # Degradado muy suave: un poco más de cielo arriba (premultiplicado por el alfa).
        top = np.asarray(body_top(theme), np.float32) / 255.0
        bottom = np.asarray(body, np.float32) / 255.0
        k = np.clip(np.arange(h, dtype=np.float32) / max(1.0, 160.0 * scale), 0.0, 1.0)[:, None, None]
        shift = (top - bottom) * (1.0 - k)     # se suma: el borde conserva su línea
        arr[..., :3] = np.clip(arr[..., :3] + shift * arr[..., 3:4], 0.0, 1.0)
    arr.setflags(write=False)
    return arr


# ----------------------------------------------------------------- ventanas
class DropWindow(_shell.LayeredWindow):
    def __init__(self, ui: DropUI, x: int, y: int, w: int, h: int) -> None:
        super().__init__(int(w), int(h), int(x), int(y), click_through=False, title="drop")
        self.ui = ui
        self._interval = 1.0 / 30.0

    def render(self):
        return self.ui.render_frame()

    def _install_mouse_hook(self) -> None:
        """La barra no necesita el gancho global de ratón (solo es para la rueda
        del menú): instalarlo hacía que cada movimiento del sistema esperase a
        este hilo."""
        return None

    def _alpha_at(self, x, y):
        """Recorte de clics por geometría: la forma y los botones son sólidos; el
        resto deja pasar el ratón."""
        s = float(self.ui.s)
        if self.ui.model.item_at(x / s, y / s) is not None:
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
        item = self.ui.model.item_at(*self._logical(x, y))
        return item is not None and item != "pill"

    def on_dpi_changed(self, dpi: int) -> None:
        self.ui.set_scale(max(1.0, dpi / 96.0))

    def clamp_position(self, x: int, y: int) -> tuple[int, int]:
        return self.ui.clamp_position(x, y)

    def on_window_drag_end(self, x: int, y: int) -> None:
        self.ui.drag_end(x, y)


class CardWindow(_shell.LayeredWindow):
    """Ventana de la tarjeta de ajustes; aloja el ``Menu`` existente."""

    def __init__(self, ui: DropUI, x: int, y: int, w: int, h: int) -> None:
        super().__init__(int(w), int(h), int(x), int(y), click_through=False, title="card")
        self.ui = ui
        self._interval = 1.0 / 60.0

    def _install_mouse_hook(self) -> None:
        """El gancho global solo hace falta si Windows no entrega la rueda a la
        ventana bajo el puntero. Instalado sin necesidad, cada movimiento de ratón
        del sistema esperaba a este hilo mientras dibujaba (input lag con el menú
        abierto); al cerrar la tarjeta el gancho se retiraba y todo volvía a ir bien."""
        if _shell.wheel_follows_pointer():
            return None
        super()._install_mouse_hook()

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
    """Controlador de la barra; la app solo habla con esta clase."""

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
        layout = str(getattr(cfg, "ui_layout", "pill") or "pill").lower()
        self.layout = layout if layout in LAYOUTS else "pill"
        self.buttons = "hover"
        motion = str(getattr(cfg, "ui_motion", "system") or "system").lower()
        self.motion = motion if motion in MOTION_MODES else "system"
        self._position = position
        self._ui = UiState()
        self.model = DropModel(self.layout)
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
        self._form_click = 0.0
        self._hover_item: str | None = None
        self._theme_setting = "auto"
        self._theme_cache = ("dark", 0.0)
        self._last = time.monotonic()
        self._reduced = self._motion_reduced()
        self._reduced_at = 0.0
        # tarjeta de ajustes
        self._menu = None
        self._card: CardWindow | None = None
        self._card_above = True
        self._card_lock = threading.RLock()
        self._pending_save: bool | None = None
        # Callbacks de la app.
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
        w, h = layout_size(self.layout)
        return (round(w * self.s), round(h * self.s))

    def _default_position(self) -> tuple[int, int]:
        """Abajo y con la forma de color centrada en la pantalla."""
        wa = _shell.work_area()
        _, h = self._size()
        fx, _ = form_center(self.layout)
        return (round((wa[0] + wa[2]) / 2 - fx * self.s), wa[3] - h - round(6 * self.s))

    def _motion_reduced(self) -> bool:
        if self.motion == "reduced":
            return True
        if self.motion == "full":
            return False
        return not animations_enabled()

    def set_motion(self, motion: str) -> None:
        self.motion = motion if motion in MOTION_MODES else "system"
        self._reduced = self._motion_reduced()
        self._reduced_at = time.monotonic()


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
        """La pregunta se dice por voz; aquí solo se muestran ✕ y ✓ (sin texto)."""
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
    def _theme_name(self) -> str:
        if self._theme_setting in ("dark", "light"):
            return self._theme_setting
        value, at = self._theme_cache
        now = time.monotonic()
        if now - at > 3.0:
            value = "light" if _system_light() else "dark"
            self._theme_cache = (value, now)
        return value

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
            self._reduced = self._motion_reduced()
            self._reduced_at = now
        cur = self._cursor_logical()
        self._hover_item = None
        if cur is not None:
            if self.model.near(*cur):
                self._hover_until = now + HOVER_HOLD
            item = self.model.item_at(*cur)
            self._hover_item = item if item not in (None, "pill") else None
        hover = now < self._hover_until
        pose = self._pose(now)
        self.model.update(now, dt, pose, self._rms, self._theme_name(), hover, self._asking,
                          self._menu_open, self._reduced, self.buttons, muted=self._muted,
                          hover_item=self._hover_item, pressed=self._pressed,
                          in_call=self._in_call)
        if self._win is not None:
            self._win._interval = 1.0 / self.target_fps(pose, hover, now)
        w, h = self._size()
        return render_model(self.model, w, h, self.s, self._theme_name(), self._muted,
                            self._in_call, self._menu_open, now, self._hover_item)

    def target_fps(self, pose: str, hover: bool, now: float) -> float:
        """60 fps fijos mientras algo se mueve; 30 en reposo (el fluido sigue vivo)."""
        busy = (pose not in ("idle", "sleep", "mute") or hover or self._asking
                or self.model.animating(now))
        return FPS_BUSY if busy else FPS_CALM

    # ------------------------------------------------------ forma: círculo ↔ píldora
    def set_layout(self, layout: str) -> None:
        """Cambia entre píldora y círculo sin mover la forma de color de sitio."""
        layout = layout if layout in LAYOUTS else "pill"
        if layout == self.layout:
            return
        win = self._win
        anchor = None
        if win is not None:
            fx, fy = form_center(self.layout)
            anchor = (win.x + fx * self.s, win.y + fy * self.s)
        self.layout = layout
        self.model.layout = layout
        if win is not None and anchor is not None:
            fx, fy = form_center(layout)
            nx, ny = self.clamp_position(round(anchor[0] - fx * self.s), round(anchor[1] - fy * self.s))
            w, h = self._size()
            win.resize(w, h)
            win.set_pos(nx, ny)
            self._position = (nx, ny)
            if self._menu_open:
                self._place_card()      # el menú sigue abierto junto a la nueva forma
        if self.cfg is not None:
            try:
                self.cfg.save_key("ui_layout", layout)
                if win is not None:
                    self.cfg.save_key("drop_x", int(win.x))
                    self.cfg.save_key("drop_y", int(win.y))
                    self.cfg.save_key("drop_layout", layout)
            except Exception:
                pass

    def toggle_layout(self) -> None:
        self.set_layout("circle" if self.layout == "pill" else "pill")

    # ----------------------------------------------------------------- puntero
    def pointer_down(self, x: float, y: float) -> bool:
        name = self.model.item_at(x, y)
        if name is None:
            return False
        if name == "pill":
            now = time.monotonic()
            if now - self._form_click < DOUBLE_CLICK:       # doble clic: cambia la forma
                self._form_click = 0.0
                self.toggle_layout()
                return True
            self._form_click = now
            return False                    # un clic en la forma arrastra la ventana
        self._pressed = name
        return True

    def pointer_up(self, x: float, y: float) -> bool:
        name, self._pressed = self._pressed, None
        if name is None or self.model.item_at(x, y) != name:
            return True
        self._activate(name)
        return True

    def _activate(self, name: str) -> None:
        asking = self._asking
        try:
            if name == "gear":
                if not asking:
                    self.on_action("panel")
            elif name == "mic":
                if asking:
                    if self._on_yes:
                        self._on_yes()
                else:
                    # En llamada alterna la pausa; colgado, empieza a escuchar.
                    self.on_action("mute" if self._in_call else "listen")
            elif name == "hang":
                if asking:
                    if self._on_no:
                        self._on_no()
                elif self._in_call:
                    self.on_action("hangup")
                else:
                    self.on_action("listen")    # el mismo botón vuelve a llamar
        except Exception:
            pass

    def clamp_position(self, x: int, y: int) -> tuple[int, int]:
        """Mantiene la barra dentro del área de trabajo."""
        wa = _shell.work_area()
        w, h = self._size()
        margin = round(PAD * self.s)
        return (
            max(wa[0] - margin, min(int(x), wa[2] - w + margin)),
            max(wa[1] - margin, min(int(y), wa[3] - h + margin)),
        )

    def drag_end(self, x: int, y: int) -> None:
        if self.cfg is not None:
            try:
                self.cfg.save_key("drop_x", int(x))
                self.cfg.save_key("drop_y", int(y))
                self.cfg.save_key("drop_layout", self.layout)
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
                self._menu.set_palette_overrides(
                    card_overrides
                )
            except Exception:
                self._menu = None
        return self._menu

    def _bar_box(self) -> tuple[float, float, float]:
        """(x central, arriba, abajo) de la barra en píxeles físicos de pantalla."""
        win = self._win
        w, h = layout_size(self.layout)
        if self.layout == "circle":
            top, bottom = PAD, h - PAD
        else:
            half = max(BTN, PILL_H) / 2
            top, bottom = h / 2 - half, h / 2 + half
        return (win.x + w / 2 * self.s, win.y + top * self.s, win.y + bottom * self.s)

    def _card_geometry(self) -> tuple[bool, int, float]:
        """(abre hacia arriba, x del borde izquierdo de ☰, alto disponible lógico)."""
        _, top, bottom = self._bar_box()
        gear_x, _ = slots(self.layout)["gear"]
        anchor = self._win.x + (gear_x - BTN / 2) * self.s
        wa = _shell.work_area()
        gap = 10 * self.s
        above_space = (top - gap) - wa[1]
        below_space = wa[3] - (bottom + gap)
        above = above_space >= below_space
        space = above_space if above else below_space
        return above, int(anchor), max(160.0, space / self.s - 12)

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
        # La tarjeta se despliega desde ☰: su cuerpo arranca alineado con el botón.
        x = max(wa[0] + round(8 * self.s), min(gx - round(FLARE * self.s), wa[2] - w - round(8 * self.s)))
        _, top, bottom = self._bar_box()
        gap = 10 * self.s
        y = int(top - gap - h) if above else int(bottom + gap)
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
        """«–» de la tarjeta: cierra solo el menú; la barra sigue visible."""
        self._close_menu()

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

    def update_llm_models(self, provider: str, models) -> None:
        """Lista real de modelos de la API para el menú abierto (hilo de red)."""
        menu = self._menu
        if menu is None:
            return
        try:
            menu.settings.set_llm_models(provider, models)
        except Exception:
            pass

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
        buf[y0:y0 + bh] = render_card_body(w, bh, s, self._theme_name(), fresh=True)
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
    """Crea la interfaz de la barra (``DropUI``) con la posición guardada, si vale."""
    x = getattr(cfg, "drop_x", None)
    y = getattr(cfg, "drop_y", None)
    layout = str(getattr(cfg, "ui_layout", "pill") or "pill").lower()
    # La posición solo vale si se guardó con esta misma disposición (otro tamaño de ventana).
    same = getattr(cfg, "drop_layout", "") == layout
    pos = (int(x), int(y)) if same and isinstance(x, int) and isinstance(y, int) else None
    return DropUI(root, position=pos, cfg=cfg)
