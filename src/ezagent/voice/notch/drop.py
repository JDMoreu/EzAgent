"""La gota: la interfaz de EzAgent (sin texto, solo movimiento).

Sustituye a la isla/notch. Es una ventana por capas transparente de 220×140 px
lógicos con una gota que respira siempre, cambia de forma según el estado y, al
acercar el puntero, hace brotar dos gotas pequeñas (micrófono y ajustes). Una
confirmación pendiente hace brotar ✕ y ✓ en su lugar. Los píxeles totalmente
transparentes dejan pasar el clic al programa de debajo.

Tiene el mismo contrato que ``Notch`` (``island.py``) para que ``app.py`` no
cambie. Los ajustes reutilizan por ahora el menú de la isla antigua, que se abre
junto a la gota; reescribirlo con este estilo es el siguiente paso.

La animación vive en ``DropModel`` (pura, sin Win32) para poder probarla.
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

# Lienzo lógico (px a 96 DPI) y geometría de la gota.
LW, LH = 220, 140
CX, CY = LW / 2.0, LH / 2.0
BUD_R = 17.0          # radio de las gotas pequeñas
BUD_X = 52.0          # separación de las gotas pequeñas al centro
HIT_R = 50.0          # zona sensible alrededor de la gota
SS = 2                # supersampling del dibujo
HOVER_HOLD = 0.7      # s que las gotas pequeñas siguen tras retirar el puntero

DARK = {
    "ink": (0x14, 0x12, 0x10), "text2": (0xC2, 0xBA, 0xAE), "text3": (0x9C, 0x94, 0x88),
    "signal": (0xD4, 0xF2, 0x5A), "signal_dim": (0x8C, 0xA0, 0x3F),
    "agent": (0xA8, 0xA1, 0xFF), "attention": (0xFF, 0xB2, 0x3E), "fault": (0xFF, 0x5D, 0x52),
    "line": (0xF4, 0xEF, 0xE8),
}
LIGHT = {
    "ink": (0xFA, 0xF8, 0xF5), "text2": (0x4E, 0x48, 0x40), "text3": (0x6F, 0x67, 0x5C),
    "signal": (0x4D, 0x6B, 0x00), "signal_dim": (0x6F, 0x8A, 0x1F),
    "agent": (0x5B, 0x4F, 0xD6), "attention": (0xA3, 0x5F, 0x00), "fault": (0xC8, 0x24, 0x1A),
    "line": (0x1A, 0x17, 0x14),
}

# Estado visual (de UiState) -> pose de la gota y color.
POSE_OF = {
    "off": "idle", "idle": "idle", "listen": "listen", "think": "think",
    "speak": "speak", "notice": "done", "mute": "mute", "error": "error",
}
COLOR_OF = {
    "idle": "signal_dim", "listen": "signal", "think": "attention", "speak": "signal",
    "done": "signal", "mute": "text3", "error": "fault", "ask": "attention",
    "sleep": "text3",
}


def level_from_rms(rms: float) -> float:
    """RMS lineal (0..~0.3) -> 0..1 perceptual."""
    try:
        value = max(0.0, float(rms or 0.0))
    except (TypeError, ValueError):
        return 0.0
    return min(1.0, (value ** 0.6) * 3.2)


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


class DropModel:
    """Animación pura de la gota: sin Win32, sin Pillow, probable en aislamiento."""

    def __init__(self) -> None:
        self.R = 11.0
        self.m = {"a2": .07, "a3": .055, "a4": .035, "a5": .02, "sx": 1.0, "sy": 1.0, "spd": .9}
        self.ph = [0.0, 0.0]
        self.color = [float(v) for v in DARK["signal_dim"]]
        self.lvl = 0.0
        self.rings: list[float] = []
        self.last_ring = 0.0
        self.bud = {n: Spring(0.0) for n in ("mic", "gear", "no", "yes")}
        self.pose = "idle"
        self.shift = 0.0
        self.pose_since = 0.0
        self.cy_offset = 0.0

    # ------------------------------------------------------------ entrada
    def update(
        self, now: float, dt: float, pose: str, rms: float, palette: dict,
        hover: bool, asking: bool, menu_open: bool, reduced: bool,
    ) -> None:
        if pose != self.pose:
            self.pose = pose
            self.pose_since = now
        ts = now - self.pose_since
        # nivel con ataque 45 ms y caída 200 ms
        target = level_from_rms(rms) if pose in ("listen", "speak") else 0.0
        tau = 0.045 if target > self.lvl else 0.2
        self.lvl += (target - self.lvl) * (1.0 - math.exp(-dt / tau))

        a = {"a2": .07, "a3": .055, "a4": .035, "a5": .02}
        tgt_r = 11.0 + math.sin(now * math.tau / 4.0) * .6
        sx = sy = 1.0
        spd = .9
        shift = 0.0
        lv = self.lvl
        if pose == "listen":
            tgt_r = 13 + lv * 5
            a = {"a2": .05 + lv * .10, "a3": .04 + lv * .09, "a4": .03, "a5": .02}
            spd = 2.2
        elif pose == "think":
            tgt_r = 11
            a = {"a2": .09, "a3": .10, "a4": .06, "a5": .03}
            spd = 3.2
        elif pose == "speak":
            tgt_r = 12 + lv * 3.5
            a = {"a2": .03 + lv * .16, "a3": .03 + lv * .14, "a4": .02 + lv * .09, "a5": .015 + lv * .06}
            spd = 3.0
        elif pose == "ask":
            sy = .9 + math.sin(now * math.tau / 1.2) * .06
            sx = 1.07
            tgt_r = 12
            a = {"a2": .06, "a3": .05, "a4": .03, "a5": .02}
            spd = 1.6
        elif pose == "mute":
            tgt_r = 7
            a = {"a2": .02, "a3": .015, "a4": 0.0, "a5": 0.0}
            spd = .5
        elif pose == "sleep":
            tgt_r = 7.5 + math.sin(now * math.tau / 5.0) * .4
            a = {"a2": .03, "a3": .02, "a4": .01, "a5": 0.0}
            spd = .4
        elif pose == "error":
            shift = math.sin(ts * 45) * 3 * max(0.0, 1 - ts / .5)
            tgt_r = 10
        elif pose == "done":
            tgt_r = 11 + math.exp(-ts * 4) * math.cos(ts * 14) * 4

        k = 1.0 if reduced else 1.0 - math.exp(-dt / .14)
        for key in ("a2", "a3", "a4", "a5"):
            self.m[key] += (a[key] - self.m[key]) * k
        self.m["sx"] += (sx - self.m["sx"]) * k
        self.m["sy"] += (sy - self.m["sy"]) * k
        self.m["spd"] += (spd - self.m["spd"]) * (1.0 if reduced else 1.0 - math.exp(-dt / .35))
        if not reduced:
            self.ph[0] += self.m["spd"] * dt
            self.ph[1] += self.m["spd"] * dt * .8
        self.R += (tgt_r - self.R) * (1.0 if reduced else 1.0 - math.exp(-dt / .09))
        self.shift = 0.0 if reduced else shift
        self.cy_offset = 0.0 if reduced else math.sin(now * .7) * 1.6

        tc = palette[COLOR_OF.get(pose, "signal_dim")]
        kc = 1.0 - math.exp(-dt / (.12 if reduced else .07))
        for i in range(3):
            self.color[i] += (tc[i] - self.color[i]) * kc

        if pose == "listen" and not reduced and now - self.last_ring > .34:
            self.rings.append(now)
            self.last_ring = now
        self.rings = [t for t in self.rings if now - t < .9]

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

    @staticmethod
    def near(x: float, y: float) -> bool:
        return (x - CX) ** 2 + (y - CY) ** 2 <= HIT_R ** 2


# ------------------------------------------------------------------ dibujo
def _stroke(d: ImageDraw.ImageDraw, pts, width: float, fill) -> None:
    d.line(pts, fill=fill, width=max(1, round(width)), joint="curve")
    r = width / 2.0
    for x, y in (pts[0], pts[-1]):
        d.ellipse((x - r, y - r, x + r, y + r), fill=fill)


def _glyph(d, name: str, cx: float, cy: float, u: float, color, width: float, muted: bool = False, fault=None) -> None:
    """Glifos en un cuadro de 24 unidades escalado por ``u`` px/unidad."""
    def P(px, py):
        return (cx + (px - 12) * u, cy + (py - 12) * u)

    if name == "mic":
        d.rounded_rectangle((*P(9, 3), *P(15, 14)), radius=3 * u, outline=color, width=max(1, round(width)))
        d.arc((*P(5, 4), *P(19, 18)), 0, 180, fill=color, width=max(1, round(width)))
        _stroke(d, [P(12, 18), P(12, 21)], width, color)
        if muted:
            _stroke(d, [P(4, 4), P(20, 20)], width * 1.15, fault)
    elif name == "gear":
        r = 3 * u
        c = P(12, 12)
        d.ellipse((c[0] - r, c[1] - r, c[0] + r, c[1] + r), outline=color, width=max(1, round(width)))
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


def render_model(model: DropModel, w: int, h: int, scale: float, palette: dict,
                 muted: bool, in_call: bool, menu_open: bool) -> np.ndarray:
    """Dibuja un fotograma BGRA premultiplicado de ``w``×``h`` px físicos."""
    k = scale * SS
    img = Image.new("RGBA", (w * SS, h * SS), (0, 0, 0, 0))
    d = ImageDraw.Draw(img, "RGBA")
    ink = palette["ink"]
    line = palette["line"]
    # Zona sensible casi invisible: sin ella el hover y el arrastre no la detectan.
    r = HIT_R * k
    d.ellipse((CX * k - r, CY * k - r, CX * k + r, CY * k + r), fill=(*ink, 4))

    col = tuple(int(v) for v in model.color)
    pose = model.pose
    # anillos al escuchar
    now = time.monotonic()
    for t0 in model.rings:
        q = (now - t0) / .9
        if 0 <= q <= 1:
            rr = (model.R + 4 + q * 26) * k
            d.ellipse((CX * k - rr, (CY + model.cy_offset) * k - rr, CX * k + rr, (CY + model.cy_offset) * k + rr),
                      outline=(*col, int(130 * (1 - q))), width=max(1, round(1.3 * k)))
    pts = [(x * k, y * k) for x, y in model.blob()]
    d.polygon(pts, fill=(*col, 235 if pose == "idle" else 255))
    d.line(pts + [pts[0]], fill=(*ink, 100), width=max(1, round(1.0 * k)))
    if pose in ("mute",) or (muted and pose in ("idle", "listen")):
        cxs, cys = (CX + model.shift) * k, (CY + model.cy_offset) * k
        _stroke(d, [(cxs - 8 * k, cys + 8 * k), (cxs + 8 * k, cys - 8 * k)], 2 * k, (*palette["fault"], 255))

    # gotas pequeñas
    for name in ("mic", "gear", "no", "yes"):
        bx, by, p = model.bud_center(name)
        if p < 0.03:
            continue
        pr = max(0.2, p)
        rad = BUD_R * pr * k
        alpha = int(255 * max(0.0, min(1.0, p * 1.4)))
        cx_, cy_ = bx * k, by * k
        d.ellipse((cx_ - rad, cy_ - rad, cx_ + rad, cy_ + rad),
                  fill=(*ink, alpha), outline=(*line, int(alpha * .22)), width=max(1, round(1.0 * k)))
        if name == "mic":
            gcol = palette["fault"] if muted else palette["text2"]
        elif name == "gear":
            gcol = palette["signal"] if menu_open else palette["text2"]
        elif name == "yes":
            gcol = palette["signal"]
        else:
            gcol = palette["text2"]
        _glyph(d, name, cx_, cy_, (16 / 24) * k * pr, (*gcol, alpha), 1.7 * k * pr,
               muted=(name == "mic" and muted), fault=(*palette["fault"], alpha))

    pm = img.convert("RGBa")
    if SS > 1:
        pm = pm.reduce(SS)
    arr = np.asarray(pm)
    return np.ascontiguousarray(arr[..., [2, 1, 0, 3]])


# ----------------------------------------------------------------- ventana
class DropWindow(_shell.LayeredWindow):
    def __init__(self, ui: "DropUI", x: int, y: int, w: int, h: int) -> None:
        super().__init__(int(w), int(h), int(x), int(y), click_through=False, title="drop")
        self.ui = ui
        self._interval = 1.0 / 30.0

    def render(self):
        return self.ui.render_frame()

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
        self._lock = threading.RLock()
        self._ui = UiState()
        self.model = DropModel()
        self._win: DropWindow | None = None
        self._visible = False
        self._state = "idle"
        self._rms = 0.0
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
        self._legacy = None
        self._legacy_opened_at = 0.0
        # Callbacks de la app (mismo contrato que Notch).
        self.on_action = lambda action: None
        self.on_position_change = lambda x, y: None
        self.on_hide = lambda: None
        self._forward = {}

    # ------------------------------------------------ menú de ajustes (isla antigua)
    def _legacy_ui(self):
        if self._legacy is None:
            from .island import Notch

            self._legacy = Notch(self.root, position=None)
            self._legacy.on_action = lambda a: self.on_action(a)
            self._legacy.on_hide = lambda: self.on_hide()
            for name, value in self._forward.items():
                setattr(self._legacy, name, value)
            self._legacy.set_theme(self._theme_setting)
            self._legacy.set_muted(self._muted)
            self._legacy.set_in_call(self._in_call)
        return self._legacy

    def _forwarded(name: str):  # noqa: N805 - fábrica de propiedades
        def getter(self):
            return self._forward.get(name)

        def setter(self, value):
            self._forward[name] = value
            if self._legacy is not None:
                setattr(self._legacy, name, value)

        return property(getter, setter)

    menu_settings_provider = _forwarded("menu_settings_provider")
    on_live_setting = _forwarded("on_live_setting")
    on_save_settings = _forwarded("on_save_settings")
    on_test_voice = _forwarded("on_test_voice")
    on_menu_action = _forwarded("on_menu_action")
    del _forwarded

    def _place_legacy(self, leg) -> None:
        """Coloca la isla antigua (solo para el menú) pegada a la gota."""
        if self._win is None:
            return
        gx = self._win.x + int(CX * self.s)
        gy = self._win.y + int(CY * self.s)
        # El menú se centra en la isla y mide ``W``: se desplaza para que
        # quepa en el área de trabajo aunque la gota esté en una esquina.
        wa = _shell.work_area()
        half = leg.W // 2 + round(8 * self.s)
        gx = max(wa[0] + half, min(gx, wa[2] - half))
        x = gx - leg.W_FLOAT // 2
        y = gy - int(20 * self.s)
        if leg._island is None:
            leg._position = (x, y)
        else:
            try:
                leg._island.set_pos(*leg._island.clamp_position(x, y))
                leg._sync_anchor_from_window()
            except Exception:
                pass

    def open_menu(self) -> None:
        if self._asking or self._menu_open:
            return
        self.toggle_menu()

    def toggle_menu(self) -> None:
        if self._asking:
            return
        leg = self._legacy_ui()
        if self._menu_open or leg._island_open():
            leg.toggle_menu()
            self._menu_open = False
            self._schedule_legacy_close()
            return
        self._place_legacy(leg)
        leg.show("idle")
        leg.toggle_menu()
        self._menu_open = True
        self._legacy_opened_at = time.monotonic()
        self._schedule_legacy_close()

    def _schedule_legacy_close(self) -> None:
        """Oculta la isla antigua cuando el menú termina de plegarse."""
        if self.root is None:
            return

        def poll():
            leg = self._legacy
            if leg is None:
                return
            try:
                still_open = leg._island_open()
            except Exception:
                still_open = False
            if still_open:
                self._menu_open = True
                self.root.after(250, poll)
                return
            if time.monotonic() - self._legacy_opened_at < 0.6:
                self.root.after(150, poll)
                return
            self._menu_open = False
            try:
                leg.hide(close_eyes=False)
            except Exception:
                pass

        self.root.after(300, poll)

    # --------------------------------------------------------------- ventana
    def _size(self) -> tuple[int, int]:
        return (round(LW * self.s), round(LH * self.s))

    def _default_position(self) -> tuple[int, int]:
        wa = _shell.work_area()
        w, h = self._size()
        return (wa[2] - w - round(24 * self.s), wa[3] - h - round(4 * self.s))

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
        if self._win is not None:
            self._win.hide()
        if self._legacy is not None:
            try:
                self._legacy.hide(close_eyes=False)
            except Exception:
                pass
        self._menu_open = False

    def close(self) -> None:
        if self._legacy is not None:
            try:
                self._legacy.close()
            except Exception:
                pass
        if self._win is not None:
            self._win.stop()

    # ----------------------------------------------------------------- estado
    def set_state(self, state: str) -> None:
        self._state = state
        self._ui.set_app_state(state)
        if self._win is not None:
            self._win.invalidate()

    def set_muted(self, muted: bool) -> None:
        self._muted = bool(muted)
        self._ui.set_muted(self._muted)
        if self._legacy is not None:
            self._legacy.set_muted(self._muted)

    def set_in_call(self, in_call: bool) -> None:
        self._in_call = bool(in_call)
        self._ui.set_in_call(self._in_call)
        if self._legacy is not None:
            self._legacy.set_in_call(self._in_call)

    def set_mic_level(self, level: float) -> None:
        if self._legacy is not None:
            self._legacy.set_mic_level(level)

    def draw(self, rms: float, state: str | None = None, wave_data=()) -> None:
        self._rms = float(rms or 0.0)
        if state is not None:
            self.set_state(state)

    def notice(self) -> None:
        self._ui.notice(time.monotonic())

    def set_testing(self, testing: bool) -> None:
        if self._legacy is not None:
            self._legacy.set_testing(testing)

    def set_save_result(self, ok: bool) -> None:
        if self._legacy is not None:
            self._legacy.set_save_result(ok)

    def refresh_menu_settings(self) -> None:
        if self._legacy is not None:
            self._legacy.refresh_menu_settings()

    def set_theme(self, theme: str) -> None:
        self._theme_setting = theme if theme in ("auto", "dark", "light") else "auto"
        self._theme_cache = ("dark", 0.0)
        if self._legacy is not None:
            self._legacy.set_theme(self._theme_setting)

    def set_scale(self, scale: float) -> None:
        self.s = max(1.0, float(scale))
        if self._win is not None:
            w, h = self._size()
            self._win.resize(w, h)

    # ------------------------------------------------------------ confirmación
    def show_confirmation(self, summary: str, strong: bool, on_yes, on_no) -> None:
        """La pregunta se dice por voz; aquí solo brotan ✕ y ✓ (sin texto)."""
        self._on_yes, self._on_no = on_yes, on_no
        self._asking = True
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
            self._win._interval = 1.0 / (60.0 if busy else 30.0)
        w, h = self._size()
        return render_model(self.model, w, h, self.s, palette, self._muted, self._in_call, self._menu_open)

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
        mx, my = round((CX - 36) * self.s), round((CY - 36) * self.s)
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
