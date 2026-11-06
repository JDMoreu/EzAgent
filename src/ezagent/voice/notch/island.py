"""Isla flotante de EzAgent: ojos, controles y menú en una sola ventana.

La geometría, los tokens, los ojos y la silueta viven en módulos separados.
Este módulo es el controlador: traduce el estado lógico del pipeline a poses,
compone el fotograma BGRA premultiplicado y gestiona la ventana Win32.

Reglas que se respetan aquí:
- La ventana no se activa en reposo ni se mueve sola; la isla se acopla al
  borde superior o inferior, o flota como píldora, y el menú crece hacia el
  lado con espacio. Solo la edición de texto pide el teclado temporalmente.
- El menú vive en la MISMA ventana que la isla; no hay Toplevel, tooltip ni
  segunda ventana.
- El render no bloquea: no hay disco, red ni esperas dentro del fotograma.
"""

from __future__ import annotations

import math
import random
import time
from dataclasses import replace

import numpy as np
from PIL import Image

from . import layered as _shell
from .anim import EASE_IN, EASE_OUT, EASE_SPRING, LevelFollower, Tween, lerp_rgb
from .eyes import POSES, draw_eyes
from .icons import draw_icon
from .silhouette import render_side_surface, render_surface
from .state import UiState
from .tokens import (
    ARROW_BUTTON_COMPACT,
    ARROW_BUTTON_SIDE,
    BUTTON_PRESSED_SCALE,
    EYES_CX_COMPACT,
    EYES_CX_WIDE,
    EYES_CY,
    EYES_CY_SIDE,
    H_ISLAND,
    H_MENU,
    H_SIDE,
    ICON_ARROW_SIZE,
    ICON_MIC_SIZE,
    MIC_BUTTON_COMPACT,
    MIC_BUTTON_SIDE,
    Rect,
    SIDE_EDGE_MARGIN,
    SIDE_EYES_GAP,
    SNAP_EDGE,
    W_SIDE,
    T_BASE,
    T_CLOSE_EYES,
    T_CLOSE_RETRACT,
    T_FAST,
    T_MICRO,
    T_OPEN,
    T_SLOW,
    VOICE_ATTACK,
    VOICE_RELEASE,
    W,
    W_COMPACT,
    palette,
)

try:  # El menú es parte del notch; el fallback permite renderizar solo la isla.
    from .menu import Menu
    from .settings import MenuSettings
except Exception:  # pragma: no cover - instalaciones sin el módulo de menú
    Menu = None  # type: ignore[assignment]
    MenuSettings = None  # type: ignore[assignment]


# Un RMS de 0,020 ya se considera nivel máximo visual. Es una calibración de
# audio, no una medida de layout; se deja documentada y en un solo sitio.
RMS_FULL_SCALE = 0.020
RMS_GAMMA = 0.85


def _rgba(color: tuple[int, ...], alpha: float | None = None) -> np.ndarray:
    if len(color) == 4 and alpha is None:
        alpha = color[3] / 255.0
    if alpha is None:
        alpha = 1.0
    return np.asarray(color[:3], dtype=np.float32) / 255.0, float(alpha)


def _over(
    buf: np.ndarray,
    y0: int,
    y1: int,
    x0: int,
    x1: int,
    coverage: np.ndarray,
    color: tuple[int, ...],
    alpha: float = 1.0,
) -> None:
    if coverage.size == 0:
        return
    rgb, color_alpha = _rgba(color, None)
    a = np.clip(coverage * alpha * color_alpha, 0.0, 1.0)
    region = buf[y0:y1, x0:x1]
    one = 1.0 - a
    region[..., :3] = rgb[None, None, :] * a[..., None] + region[..., :3] * one[..., None]
    region[..., 3] = a + region[..., 3] * one


def _box_coverage(
    buf: np.ndarray,
    cx: float,
    cy: float,
    hw: float,
    hh: float,
    radius: float,
    scale: float,
) -> tuple[int, int, int, int, np.ndarray]:
    pad = 1.5
    x0 = max(0, int(math.floor((cx - hw - pad) * scale)))
    x1 = min(buf.shape[1], int(math.ceil((cx + hw + pad) * scale)))
    y0 = max(0, int(math.floor((cy - hh - pad) * scale)))
    y1 = min(buf.shape[0], int(math.ceil((cy + hh + pad) * scale)))
    if x1 <= x0 or y1 <= y0:
        empty = np.zeros((0, 0), np.float32)
        return 0, 0, 0, 0, empty
    ys, xs = np.mgrid[y0:y1, x0:x1].astype(np.float32)
    px = (xs + 0.5) / scale
    py = (ys + 0.5) / scale
    qx = np.abs(px - cx) - max(hw - radius, 0.0)
    qy = np.abs(py - cy) - max(hh - radius, 0.0)
    d = (
        np.hypot(np.maximum(qx, 0.0), np.maximum(qy, 0.0))
        + np.minimum(np.maximum(qx, qy), 0.0)
        - radius
    )
    cov = np.clip(0.5 - d, 0.0, 1.0).astype(np.float32)
    return y0, y1, x0, x1, cov


def fill_round_rect(
    buf: np.ndarray,
    x: float,
    y: float,
    w: float,
    h: float,
    radius: float,
    color: tuple[int, ...],
    scale: float,
    alpha: float = 1.0,
) -> None:
    if w <= 0 or h <= 0 or alpha <= 0:
        return
    cx, cy = x + w / 2.0, y + h / 2.0
    y0, y1, x0, x1, cov = _box_coverage(buf, cx, cy, w / 2.0, h / 2.0, radius, scale)
    if cov.size:
        _over(buf, y0, y1, x0, x1, cov, color, alpha)


def fill_circle(
    buf: np.ndarray,
    cx: float,
    cy: float,
    radius: float,
    color: tuple[int, ...],
    scale: float,
    alpha: float = 1.0,
) -> None:
    fill_round_rect(buf, cx - radius, cy - radius, radius * 2, radius * 2, radius, color, scale, alpha)


def draw_capsule_line(
    buf: np.ndarray,
    x0: float,
    y0: float,
    x1: float,
    y1: float,
    width: float,
    color: tuple[int, ...],
    scale: float,
    alpha: float = 1.0,
) -> None:
    """Segmento con extremos redondeados, en coordenadas lógicas."""
    pad = width + 1.5
    bx0 = max(0, int(math.floor((min(x0, x1) - pad) * scale)))
    bx1 = min(buf.shape[1], int(math.ceil((max(x0, x1) + pad) * scale)))
    by0 = max(0, int(math.floor((min(y0, y1) - pad) * scale)))
    by1 = min(buf.shape[0], int(math.ceil((max(y0, y1) + pad) * scale)))
    if bx1 <= bx0 or by1 <= by0:
        return
    ys, xs = np.mgrid[by0:by1, bx0:bx1].astype(np.float32)
    px = (xs + 0.5) / scale
    py = (ys + 0.5) / scale
    dx, dy = x1 - x0, y1 - y0
    length2 = dx * dx + dy * dy
    if length2 <= 1e-9:
        d = np.hypot(px - x0, py - y0) - width / 2.0
    else:
        t = np.clip(((px - x0) * dx + (py - y0) * dy) / length2, 0.0, 1.0)
        d = np.hypot(px - (x0 + t * dx), py - (y0 + t * dy)) - width / 2.0
    cov = np.clip(0.5 - d, 0.0, 1.0).astype(np.float32)
    _over(buf, by0, by1, bx0, bx1, cov, color, alpha)


def draw_chevron(
    buf: np.ndarray,
    cx: float,
    cy: float,
    size: float,
    color: tuple[int, ...],
    scale: float,
    angle_deg: float = 0.0,
    alpha: float = 1.0,
) -> None:
    """Chevrón vectorial, sin fondo ni borde, que gira con el menú."""
    half = size / 2.0
    points = [(-half, -half * 0.45), (0.0, half * 0.45), (half, -half * 0.45)]
    a = math.radians(angle_deg)
    ca, sa = math.cos(a), math.sin(a)
    rot = [(cx + x * ca - y * sa, cy + x * sa + y * ca) for x, y in points]
    stroke = 1.8 if size >= 18 else 1.6
    for p, q in zip(rot, rot[1:]):
        draw_capsule_line(buf, p[0], p[1], q[0], q[1], stroke, color, scale, alpha)


class _PoseAnimator:
    """Interpola la pose de ojos: números con muelle, color y forma aparte."""

    def __init__(self, theme: str, animations: bool = True):
        pose = POSES["idle"]
        self.theme = theme
        self.name = "idle"
        self.animations = animations
        self._target = pose
        self.w = Tween(pose.w)
        self.h = Tween(pose.h)
        self.rot = Tween(pose.rot)
        self.gap = Tween(pose.gap)
        self.glow = Tween(pose.glow)
        self.wave_w = Tween(pose.wave_w)
        self.wave_alpha = Tween(0.0 if pose.wave_w <= 0.0 else 1.0)
        self.gain = Tween(pose.gain)
        self.color = tuple(palette(theme)[pose.color_key][:3])
        self._color0 = self.color
        self._color1 = self.color
        self._color_t0 = 0.0
        self._color_dur = 0.0
        self.shape = pose.shape
        self._shape_from = pose.shape
        self._shape_to = pose.shape
        self._shape_t0 = 0.0
        self._shape_dur = 0.0

    def set_theme(self, theme: str) -> None:
        self.theme = theme
        self._color0 = self._color1
        self._color1 = tuple(palette(theme)[self._target.color_key][:3])
        self._color_t0 = 0.0
        self._color_dur = 0.0
        self.color = self._color1

    def set_pose(self, name: str, now: float) -> None:
        target = POSES.get(name, POSES["idle"])
        if name == self.name and self._target is target:
            return
        self.name = name
        self._target = target
        # Movimiento reducido: fundido de 120 ms EASE_OUT (spec 8.2).
        dur = T_BASE if self.animations else T_MICRO
        ease = EASE_SPRING if self.animations else EASE_OUT
        for tween, value in (
            (self.w, target.w),
            (self.h, target.h),
            (self.rot, target.rot),
            (self.gap, target.gap),
            (self.glow, target.glow),
            (self.gain, target.gain),
        ):
            tween.to(value, now, dur, ease)
        if target.wave_w > 0.0:
            wave_dur = T_SLOW if self.animations else T_MICRO
            wave_ease = EASE_SPRING if self.animations else EASE_OUT
            self.wave_w.to(target.wave_w, now, wave_dur, wave_ease)
            self.wave_alpha.to(
                1.0, now, 0 if not self.animations else 200, EASE_OUT
            )
        else:
            wave_dur = T_FAST if self.animations else T_MICRO
            wave_ease = EASE_IN if self.animations else EASE_OUT
            self.wave_w.to(0.0, now, wave_dur, wave_ease)
            self.wave_alpha.to(
                0.0, now, 0 if not self.animations else 180, EASE_IN
            )
        target_color = tuple(palette(self.theme)[target.color_key][:3])
        if target_color != self._color1:
            self._color0 = self.color
            self._color1 = target_color
            self._color_t0 = now
            self._color_dur = (
                T_FAST if self.animations else T_MICRO
            ) / 1000.0
        if target.shape != self._shape_to:
            self._shape_from = self.shape
            self._shape_to = target.shape
            self._shape_t0 = now
            self._shape_dur = T_MICRO / 1000.0

    def update(self, now: float) -> None:
        for tween in (
            self.w,
            self.h,
            self.rot,
            self.gap,
            self.glow,
            self.wave_w,
            self.wave_alpha,
            self.gain,
        ):
            tween.update(now)
        if self._color_dur > 0:
            k = min(1.0, max(0.0, (now - self._color_t0) / self._color_dur))
            self.color = lerp_rgb(self._color0, self._color1, k)
            if k >= 1.0:
                self._color_dur = 0.0
        else:
            self.color = self._color1
        if self._shape_dur > 0:
            k = min(1.0, max(0.0, (now - self._shape_t0) / self._shape_dur))
            if k >= 1.0:
                self.shape = self._shape_to
                self._shape_dur = 0.0
        else:
            self.shape = self._shape_to

    def current(self):
        target = self._target
        return type(target)(
            w=self.w.value,
            h=self.h.value,
            rot=self.rot.value,
            gap=self.gap.value,
            glow=self.glow.value,
            wave_w=self.wave_w.value,
            shape=self._shape_to,
            color_key=target.color_key,
            gain=self.gain.value,
        )

    def shape_alphas(self, now: float) -> tuple[float, float]:
        if self._shape_dur <= 0:
            return (1.0, 0.0) if self._shape_to == "capsule" else (0.0, 1.0)
        k = min(1.0, max(0.0, (now - self._shape_t0) / self._shape_dur))
        from_arc = self._shape_from == "arc"
        # k=0 -> forma antigua; k=1 -> forma nueva.
        arc = (1.0 - k) if from_arc else k
        return (1.0 - arc, arc)

    def animating(self) -> bool:
        if self._color_dur > 0 or self._shape_dur > 0:
            return True
        return any(
            t.active
            for t in (
                self.w,
                self.h,
                self.rot,
                self.gap,
                self.glow,
                self.wave_w,
                self.gain,
            )
        )


class _Blink:
    def __init__(self) -> None:
        self.next_at = 0.0
        self.start = 0.0
        self.active = False
        self.value = 1.0

    def update(self, now: float, allowed: bool) -> float:
        if not allowed:
            self.active = False
            self.value = 1.0
            if self.next_at == 0.0:
                self.next_at = now + random.uniform(4.5, 6.5)
            return 1.0
        if not self.active:
            if self.next_at == 0.0:
                self.next_at = now + random.uniform(4.5, 6.5)
            if now >= self.next_at:
                self.active = True
                self.start = now
        if self.active:
            t = now - self.start
            if t < 0.165:
                self.value = 1.0 - 0.92 * (t / 0.165)
            elif t < 0.330:
                self.value = 0.08 + 0.92 * ((t - 0.165) / 0.165)
            else:
                self.active = False
                self.value = 1.0
                self.next_at = now + random.uniform(4.5, 6.5)
        return self.value


class IslandWindow(_shell.LayeredWindow):
    """Ventana layered única de la isla y su menú."""

    def __init__(self, overlay: "Notch", x: int, y: int, w: int, h: int):
        super().__init__(int(w), int(h), int(x), int(y),
                         click_through=False, title="ezagent-notch")
        self.ov = overlay
        self.max_height = overlay.H_MAX
        self._interval = 1.0 / 60.0

    def render(self):
        return self.ov.render_frame()

    # ---- puntero: coordenadas físicas de cliente -> lógicas
    def _logical(self, x: int, y: int) -> tuple[float, float]:
        s = float(self.ov.s)
        return (x / s, y / s)

    def on_pointer_down(self, x: int, y: int) -> bool:
        lx, ly = self._logical(x, y)
        return self.ov.pointer_down(lx, ly)

    def on_pointer_move(self, x: int, y: int) -> bool:
        lx, ly = self._logical(x, y)
        return self.ov.pointer_move(lx, ly)

    def on_pointer_up(self, x: int, y: int) -> bool:
        lx, ly = self._logical(x, y)
        return self.ov.pointer_up(lx, ly)

    def on_wheel(self, x: int, y: int, delta: int) -> bool:
        lx, ly = self._logical(x, y)
        return self.ov.wheel(lx, ly, delta)

    def cursor_hand(self, x: int, y: int) -> bool:
        lx, ly = self._logical(x, y)
        return self.ov.cursor_hand(lx, ly)

    def on_key(self, vk: int, ctrl: bool) -> bool:
        return self.ov.key_event(vk, ctrl)

    def on_char(self, char: str) -> bool:
        return self.ov.char_event(char)

    def on_paste(self, text: str) -> bool:
        return self.ov.paste_event(text)

    def on_focus_lost(self) -> None:
        self.ov.focus_lost()

    def on_global_wheel(self, x: int, y: int, delta: int) -> bool:
        return self.ov.global_wheel(x, y, delta)

    def on_dpi_changed(self, dpi: int) -> None:
        self.ov.set_scale(max(1.0, dpi / 96.0))

    def clamp_position(self, x: int, y: int) -> tuple[int, int]:
        return self.ov.clamp_position(x, y)

    def on_window_drag_start(self, x: int, y: int) -> None:
        self.ov.window_drag_start(x, y)

    def on_window_drag_move(self, x: int, y: int) -> None:
        self.ov.window_drag_move(x, y)

    def on_window_drag_end(self, x: int, y: int) -> None:
        self.ov.window_drag_end(x, y)

    def open(self) -> bool:
        return self.start()

    def hide(self) -> None:
        super().hide()

    def destroy(self) -> None:
        self.stop()


class Notch:
    """Controlador de la isla; la app solo habla con esta clase."""

    FRAME_MS = 16

    def __init__(self, root=None):
        self.root = root
        scale = 1.0
        try:
            if root is not None:
                scale = max(1.0, float(root.winfo_fpixels("1i")) / 96.0)
        except Exception:
            scale = 1.0
        self.s = scale
        self.W = round(W * scale)                 # ancho del menú abierto
        self.W_COMPACT = round(W_COMPACT * scale)  # ancho de la isla/pill
        self.W_SIDE = round(W_SIDE * scale)
        self.H_ISLAND = round(H_ISLAND * scale)
        self.H_SIDE = round(H_SIDE * scale)
        self.H_MAX = round((H_ISLAND + H_MENU) * scale)
        self.BODY_W = self.W_COMPACT
        self.BODY_H = self.H_ISLAND

        self._island: IslandWindow | None = None
        # Ancla = centro X y borde superior de la isla en pantalla. El menú
        # puede crecer hacia abajo o hacia arriba sin mover la isla.
        self._anchor_x = 0.0
        self._anchor_y = 0.0
        self._attachment = "float"
        self._menu_above = False
        self._menu_available: float | None = None
        self._dragging = False
        self._visible = False
        self._ui = UiState()
        self._state = "idle"
        self._rms = 0.0
        self._mic_level = 0.0
        self._level = LevelFollower(VOICE_ATTACK, VOICE_RELEASE)
        self._theme_setting = "auto"
        self._theme = "dark"
        self._animations = self._prefers_animations()
        self._pose = _PoseAnimator(self._theme, self._animations)
        self._blink = _Blink()
        self._show_t0: float | None = None
        self._hide_t0: float | None = None
        self._hide_eyes = True
        self._hide_pending = False
        self._hide_pending_eyes = True
        self._last_frame = 0.0
        self._hover: str | None = None
        self._pressed: str | None = None
        self._pressed_inside = False
        self._arrow = Tween(0.0)
        # Ancho de la pestaña: 169 cerrada, 320 abierta. El menú y la isla
        # crecen juntos para leerse como una sola pieza.
        self._tab_w = Tween(float(W_COMPACT))
        self._menu = None
        self._menu_open = False
        self._dirty = False
        self._pending_save_result: bool | None = None
        self._last_pose_name = "idle"
        self._pose_t0 = 0.0

        self.on_action = lambda action: None
        self.menu_settings_provider = lambda: None
        self.on_save_settings = lambda values: False
        self.on_live_setting = lambda key, value: None
        self.on_test_voice = lambda values: None
        self.on_hide = lambda: None

    # ------------------------------------------------------------- utilidades
    @staticmethod
    def _prefers_animations() -> bool:
        try:
            from .anim import animations_enabled

            return animations_enabled()
        except Exception:
            return True

    def _resolve_theme(self) -> str:
        if self._theme_setting in ("dark", "light"):
            return self._theme_setting
        try:
            import winreg

            key = winreg.OpenKey(
                winreg.HKEY_CURRENT_USER,
                r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            )
            try:
                value, _ = winreg.QueryValueEx(key, "AppsUseLightTheme")
                return "light" if int(value) else "dark"
            finally:
                winreg.CloseKey(key)
        except Exception:
            return "dark"

    def set_theme(self, theme: str) -> None:
        theme = theme if theme in ("auto", "dark", "light") else "auto"
        self._theme_setting = theme
        resolved = self._resolve_theme()
        if resolved != self._theme:
            self._theme = resolved
            self._pose.set_theme(resolved)
            self._invalidate()

    def set_scale(self, scale: float) -> None:
        scale = max(1.0, float(scale))
        if abs(scale - self.s) < 0.001:
            return
        self.s = scale
        self.W = round(W * scale)
        self.W_COMPACT = round(W_COMPACT * scale)
        self.W_SIDE = round(W_SIDE * scale)
        self.H_ISLAND = round(H_ISLAND * scale)
        self.H_SIDE = round(H_SIDE * scale)
        self.H_MAX = round((H_ISLAND + H_MENU) * scale)
        self.BODY_W = self.W_COMPACT
        self.BODY_H = self.H_ISLAND
        from . import eyes as _eyes
        from . import icons as _icons
        from . import silhouette as _sil
        from . import text as _text

        _text.clear_cache()
        _icons.clear_cache()
        _sil.clear_cache()
        if self._island is not None:
            self._island.max_height = self.H_MAX
            self._island.resize(self.W_COMPACT, self.H_ISLAND)
        if self._menu is not None:
            try:
                self._menu.set_scale(scale)
            except Exception:
                pass
        self._invalidate()

    # ------------------------------------------------------------- ciclo de vida
    def _create_window(self) -> bool:
        if self._island is None:
            try:
                mon = _shell.monitor_rect()
                width = int(mon[2] - mon[0])
                x = int(mon[0]) + (width - self.W_COMPACT) // 2
                y = int(mon[1])
            except Exception:
                x = 200
                y = 0
            self._island = IslandWindow(
                self, x, y, self.W_COMPACT, self.H_ISLAND
            )
            self._anchor_x = x + self.W_COMPACT / 2.0
            self._anchor_y = float(y)
            self._attachment = self._attachment_for_rect(
                x, y, self.W_COMPACT, self.H_ISLAND
            )
        if not self._island.open():
            print("[ezagent-voice] Notch: no se pudo abrir la isla", flush=True)
            self._visible = False
            return False
        return True

    def show(self, state: str = "idle") -> None:
        now = time.monotonic()
        self._state = state
        self._ui.set_app_state(state)
        self._ui.set_visible(True)
        self._hide_t0 = None
        if not self._visible:
            self._visible = True
            self._hide_t0 = None
            self._pose_t0 = now
            if self._animations:
                self._show_t0 = now
                self._pose.set_pose("open", now)
                self._last_pose_name = "open"
            else:
                self._show_t0 = None
                self._pose.set_pose("idle", now)
                self._last_pose_name = "idle"
            if not self._create_window():
                return
        if self._island is not None:
            self._island.show()
            self._invalidate()

    def hide(self, close_eyes: bool = True) -> None:
        if not self._visible:
            return
        now = time.monotonic()
        if self._island_open():
            # A-05 manda: la isla espera a que el menú termine de plegarse.
            self._hide_pending = True
            self._hide_pending_eyes = bool(close_eyes)
            try:
                self._menu.close(now)
            except Exception:
                pass
            self._show_t0 = None
            if close_eyes:
                self._pose.set_pose("close", now)
                self._last_pose_name = "close"
            self._invalidate()
            return
        if self._menu is not None:
            try:
                self._menu.close(now)
            except Exception:
                pass
        self._begin_hide(close_eyes, now)

    def _begin_hide(self, close_eyes: bool, now: float) -> None:
        self._arrow.to(0.0, now, T_BASE, EASE_OUT)
        self._tab_w.to(
            float(W_COMPACT), now,
            0 if not self._animations else T_CLOSE_RETRACT,
            EASE_IN,
        )
        self._show_t0 = None
        if not self._animations:
            # Movimiento reducido: la isla se oculta sin deslizamiento.
            self._visible = False
            self._hide_t0 = None
            self._hide_pending = False
            self._ui.set_visible(False)
            if self._island is not None:
                self._island.hide()
            return
        self._hide_t0 = now
        self._hide_eyes = bool(close_eyes)
        if close_eyes:
            self._pose.set_pose("close", now)
            self._last_pose_name = "close"
        self._invalidate()

    def close(self) -> None:
        if self._island is not None:
            self._island.destroy()
            self._island = None
        self._visible = False

    def _log_state(self) -> None:
        if self._island is None:
            return
        frame = getattr(self._island, "_frame", None)
        alpha = (
            f"alpha_max={int(frame[..., 3].max())}"
            if frame is not None
            else "frame=None"
        )
        print(
            f"[ezagent-voice] Notch: hwnd={self._island.hwnd} "
            f"pos=({self._island.x},{self._island.y}) "
            f"size=({self._island.w},{self._island.h}) "
            f"visible={self._island.is_visible()} {alpha}",
            flush=True,
        )

    def _invalidate(self) -> None:
        if self._island is not None:
            self._island.invalidate()

    # ------------------------------------------------------------- geometría
    def _monitor_bottom(self, x: float, y: float) -> tuple[tuple[int, int, int, int], int]:
        mon = _shell.monitor_rect(int(x), int(y))
        work = _shell.work_area()
        # En el monitor primario, el borde inferior útil es el de la barra de
        # tareas; con la barra auto-oculta, ``work_area`` devuelve la pantalla
        # completa, así que se usa el borde superior real de la barra.
        inside_primary = mon[0] >= work[0] and mon[2] <= work[2]
        bottom = work[3] if inside_primary else mon[3]
        try:
            tb = _shell.taskbar_rect()
        except Exception:
            tb = None
        if tb and tb[1] > mon[1] + (mon[3] - mon[1]) // 2:
            bottom = min(bottom, int(tb[1]))
        return mon, bottom

    def _is_side_attachment(self, attachment: str | None = None) -> bool:
        value = getattr(self, "_attachment", "float") if attachment is None else attachment
        return value in ("left", "right")

    def _compact_size(self) -> tuple[float, float]:
        """Tamaño lógico de la isla cerrada según el acople actual."""
        if self._is_side_attachment():
            return float(W_SIDE), float(H_SIDE)
        return float(W_COMPACT), float(H_ISLAND)

    def _side_window_geometry(
        self, menu_h: float
    ) -> tuple[float, float, float, float, float, float]:
        """``(ancho, alto, panel_x, tab_x, tab_y, progreso)`` lógicos."""
        if menu_h <= 0.5:
            return (
                float(W_SIDE),
                float(H_SIDE),
                0.0,
                0.0,
                0.0,
                0.0,
            )
        viewport = float(H_MENU)
        if self._menu is not None:
            try:
                viewport = max(1.0, float(self._menu.viewport_height))
            except Exception:
                pass
        progress = min(1.0, max(0.0, float(menu_h) / viewport))
        window_w = float(W_SIDE) + float(W) * progress
        window_h = max(float(H_SIDE), float(menu_h))
        panel_x = float(W_SIDE) if self._attachment == "left" else 0.0
        tab_x = 0.0 if self._attachment == "left" else window_w - float(W_SIDE)
        tab_y = (window_h - float(H_SIDE)) / 2.0
        return window_w, window_h, panel_x, tab_x, tab_y, progress

    def _attachment_for(self, anchor_y: float) -> str:
        """Compatibilidad con la geometría histórica de la píldora horizontal."""
        return self._attachment_for_rect(
            self._anchor_x - self.W_COMPACT / 2.0,
            anchor_y,
            self.W_COMPACT,
            self.H_ISLAND,
        )

    def _attachment_for_rect(
        self, x: float, y: float, w: float, h: float
    ) -> str:
        """Borde más cercano; empate o esquina: superior."""
        mon, work_bottom = self._monitor_bottom(x + w / 2.0, y + h / 2.0)
        snap = SNAP_EDGE * self.s
        distances = {
            "top": float(y - mon[1]),
            "bottom": float(work_bottom - (y + h)),
            "left": float(x - mon[0]),
            "right": float(mon[2] - (x + w)),
        }
        for edge in ("top", "bottom", "left", "right"):
            if distances[edge] <= snap:
                return edge
        return "float"

    def clamp_position(self, x: int, y: int) -> tuple[int, int]:
        """Mantiene la ventana dentro del monitor, permitiendo los bordes."""
        island = getattr(self, "_island", None)
        if island is not None:
            w, h = island.w, island.h
        else:
            cw, ch = self._compact_size()
            w, h = int(round(cw * self.s)), int(round(ch * self.s))
        mon, work_bottom = self._monitor_bottom(x + w / 2.0, y + h / 2.0)
        nx = min(max(int(x), mon[0]), max(mon[0], mon[2] - w))
        ny = min(max(int(y), mon[1]), max(mon[1], work_bottom - h))
        return nx, ny

    def _sync_anchor_from_window(self) -> None:
        if self._island is None:
            return
        self._anchor_x = self._island.x + self._island.w / 2.0
        if self._is_side_attachment():
            self._anchor_y = self._island.y + self._island.h / 2.0
        else:
            self._anchor_y = float(self._island.y)

    def _snap_compact(
        self,
        attachment: str,
        x: float,
        y: float,
        center_y: float | None = None,
    ) -> tuple[int, int]:
        """Posición física final de la isla cerrada para un acople."""
        mon, work_bottom = self._monitor_bottom(
            self._anchor_x, center_y if center_y is not None else self._anchor_y
        )
        if attachment == "top":
            y = float(mon[1])
            x = min(max(x, mon[0]), max(mon[0], mon[2] - self.W_COMPACT))
        elif attachment == "bottom":
            y = float(work_bottom - self.H_ISLAND)
            x = min(max(x, mon[0]), max(mon[0], mon[2] - self.W_COMPACT))
        elif attachment in ("left", "right"):
            center = (
                float(center_y)
                if center_y is not None
                else float(y) + self.H_SIDE / 2.0
            )
            y = min(
                max(
                    center - self.H_SIDE / 2.0,
                    mon[1] + SIDE_EDGE_MARGIN * self.s,
                ),
                max(
                    mon[1] + SIDE_EDGE_MARGIN * self.s,
                    work_bottom - self.H_SIDE - SIDE_EDGE_MARGIN * self.s,
                ),
            )
            if attachment == "left":
                x = float(mon[0])
            else:
                x = float(mon[2] - self.W_SIDE)
        else:
            x = min(max(x, mon[0]), max(mon[0], mon[2] - self.W_COMPACT))
            y = min(max(y, mon[1]), max(mon[1], work_bottom - self.H_ISLAND))
        return int(round(x)), int(round(y))

    def window_drag_start(self, x: int, y: int) -> None:
        # No cambia la forma todavía: un clic sin arrastre no debe convertir la
        # isla en pill. El cambio ocurre en el primer desplazamiento real.
        self._dragging = False

    def window_drag_move(self, x: int, y: int) -> None:
        self._dragging = True
        self._sync_anchor_from_window()
        if self._island is not None:
            previous = self._attachment
            self._attachment = self._attachment_for_rect(
                self._island.x,
                self._island.y,
                self._island.w,
                self._island.h,
            )
            if previous != self._attachment:
                if self._is_side_attachment():
                    self._anchor_y = self._island.y + self._island.h / 2.0
                else:
                    self._anchor_y = float(self._island.y)
        self._invalidate()

    def window_drag_end(self, x: int, y: int) -> None:
        self._dragging = False
        self._sync_anchor_from_window()
        if self._island is None:
            return
        self._attachment = self._attachment_for_rect(
            self._island.x,
            self._island.y,
            self._island.w,
            self._island.h,
        )
        if self._is_side_attachment():
            self._anchor_y = self._island.y + self._island.h / 2.0
        else:
            self._anchor_y = float(self._island.y)
        nx, ny = self._snap_compact(
            self._attachment,
            float(self._island.x),
            float(self._island.y),
            float(self._island.y + self._island.h / 2.0),
        )
        self._island.set_pos(nx, ny)
        if self._is_side_attachment():
            self._anchor_x = nx + self.W_SIDE / 2.0
            self._anchor_y = ny + self.H_SIDE / 2.0
        else:
            self._anchor_x = nx + self.W_COMPACT / 2.0
            self._anchor_y = float(ny)
        self._invalidate()

    # ------------------------------------------------------- teclado del menú
    def _menu_text_edit(self, active: bool) -> None:
        """Pide o devuelve el teclado al editar un campo del menú."""
        try:
            if self._island is not None:
                self._island.set_keyboard_enabled(bool(active))
        except Exception:
            pass
        self._invalidate()

    def key_event(self, vk: int, ctrl: bool = False) -> bool:
        menu = self._menu
        if menu is None or not self._menu_open:
            return False
        try:
            return bool(menu.text_key(vk, ctrl))
        except Exception:
            return False

    def char_event(self, char: str) -> bool:
        menu = self._menu
        if menu is None or not self._menu_open:
            return False
        try:
            return bool(menu.text_input(char))
        except Exception:
            return False

    def paste_event(self, text: str) -> bool:
        menu = self._menu
        if menu is None or not self._menu_open:
            return False
        try:
            return bool(menu.paste_text(text))
        except Exception:
            return False

    def focus_lost(self) -> None:
        menu = self._menu
        if menu is not None:
            try:
                menu.finish_text_edit(commit=True)
            except Exception:
                pass

    def menu_geometry(self) -> tuple[bool, float]:
        """Devuelve (menú arriba, alto disponible lógico)."""
        if self._island is None:
            return False, float(H_MENU)
        self._sync_anchor_from_window()
        mon, work_bottom = self._monitor_bottom(self._anchor_x, self._anchor_y)
        if self._is_side_attachment():
            # El panel lateral se limita por la altura del área útil; crece
            # hacia dentro, no arriba/abajo.
            half = min(
                self._anchor_y - mon[1],
                work_bottom - self._anchor_y,
            ) / self.s - SIDE_EDGE_MARGIN
            available = max(160.0, 2.0 * half)
            return False, available
        tab_bottom = self._anchor_y + self.H_ISLAND
        below = max(0.0, work_bottom - tab_bottom) / self.s
        above = max(0.0, self._anchor_y - mon[1]) / self.s
        menu_above = above > below
        available = max(160.0, (above if menu_above else below) - 8.0)
        return menu_above, available

    # ------------------------------------------------------------- estado
    def set_state(self, state: str) -> None:
        self._state = state
        self._ui.set_app_state(state)
        if state not in ("idle", "error", "cancelled") and not self._visible:
            self.show(state)
        self._invalidate()

    def set_muted(self, muted: bool) -> None:
        self._ui.set_muted(bool(muted))
        self._invalidate()

    def set_in_call(self, in_call: bool) -> None:
        self._ui.set_in_call(bool(in_call))
        self._invalidate()

    def set_menu_open(self, open_: bool) -> None:
        self._menu_open = bool(open_)
        self._ui.set_menu_open(bool(open_))
        self._invalidate()

    def set_mic_level(self, level: float) -> None:
        """Nivel RMS del micrófono para el medidor del menú."""
        try:
            self._mic_level = max(0.0, float(level or 0.0))
        except (TypeError, ValueError):
            self._mic_level = 0.0

    def draw(self, rms: float, state: str | None = None, wave_data=()) -> None:
        self._rms = float(rms or 0.0)
        if state is not None:
            self.set_state(state)
        self._invalidate()

    def set_testing(self, testing: bool) -> None:
        if self._menu is not None:
            try:
                self._menu.set_testing(bool(testing))
            except Exception:
                pass
        self._invalidate()

    def set_save_result(self, ok: bool) -> None:
        """El guardado se aplica en Tk; el resultado llega al menú aquí."""
        self._pending_save_result = bool(ok)
        self._invalidate()

    def refresh_menu_settings(self) -> None:
        """Recarga perfil y opciones después de guardar un proveedor nuevo."""
        if self._menu is None:
            return
        try:
            settings = self.menu_settings_provider()
            if settings is not None:
                self._menu.set_settings(settings)
                self._invalidate()
        except Exception:
            pass

    def notice(self) -> None:
        self._ui.notice(time.monotonic())
        self._invalidate()

    # ------------------------------------------------------------- menú
    def _island_open(self) -> bool:
        return bool(self._menu is not None and getattr(self._menu, "is_open", lambda: False)())

    def toggle_menu(self) -> None:
        now = time.monotonic()
        if self._island_open():
            try:
                self._menu.close(now)
            except Exception:
                pass
            self._menu_open = False
            self._ui.set_menu_open(False)
            self._arrow.to(0.0, now, T_BASE, EASE_OUT)
            self._tab_w.to(
                float(W_COMPACT), now,
                0 if not self._animations else T_CLOSE_RETRACT,
                EASE_IN,
            )
            self._invalidate()
            return
        if not self._visible:
            self.show("idle")
        # La spec 8.2/10.1 pide releer movimiento reducido y tema al abrir.
        self._animations = self._prefers_animations()
        self._pose.animations = self._animations
        self.set_theme(self._theme_setting)
        if Menu is None or MenuSettings is None:
            return
        menu_above, available = self.menu_geometry()
        self._menu_above = menu_above
        self._menu_available = available
        try:
            settings = self.menu_settings_provider()
        except Exception as exc:  # noqa: BLE001
            print(f"[ezagent-voice] Menú: no pude leer ajustes: {exc}", flush=True)
            return
        if settings is None:
            return
        if self._menu is None:
            try:
                screen_h = 1080.0
                try:
                    mon = _shell.monitor_rect()
                    screen_h = (mon[3] - mon[1]) / self.s
                except Exception:
                    pass
                self._menu = Menu(
                    settings,
                    scale=self.s,
                    theme=self._theme,
                    screen_h=screen_h,
                    on_live_change=self._live_setting,
                    on_save=self._save_settings,
                    on_test=self._test_voice,
                    on_hide=self._hide_from_menu,
                    on_close=self._menu_closed,
                    on_text_edit=self._menu_text_edit,
                )
            except Exception as exc:  # noqa: BLE001
                print(f"[ezagent-voice] Menú: no pude crearlo: {exc}", flush=True)
                self._menu = None
                return
        try:
            self._menu.set_theme(self._theme)
            self._menu.set_scale(self.s)
            self._menu.set_reduced_motion(not self._animations)
            try:
                self._menu.set_available_height(available)
            except Exception:
                pass
            self._menu.open(now)
        except Exception as exc:  # noqa: BLE001
            print(f"[ezagent-voice] Menú: no pude abrirlo: {exc}", flush=True)
            return
        self._menu_open = True
        self._ui.set_menu_open(True)
        self._arrow.to(180.0, now, T_BASE, EASE_OUT)
        self._tab_w.to(
            float(W), now,
            0 if not self._animations else T_SLOW,
            EASE_OUT,
        )
        self._invalidate()

    def _menu_closed(self) -> None:
        self._menu_open = False
        self._ui.set_menu_open(False)
        self._menu_text_edit(False)
        now = time.monotonic()
        self._arrow.to(0.0, now, T_BASE, EASE_OUT)
        self._tab_w.to(
            float(W_COMPACT), now,
            0 if not self._animations else T_CLOSE_RETRACT,
            EASE_IN,
        )
        if self._hide_pending:
            self._hide_pending = False
            self._begin_hide(self._hide_pending_eyes, now)
        self._invalidate()

    def _live_setting(self, key: str, value) -> None:
        try:
            self.on_live_setting(key, value)
        except Exception as exc:  # noqa: BLE001
            print(f"[ezagent-voice] Ajuste en vivo {key}: {exc}", flush=True)
        self._invalidate()

    def _save_settings(self, values: dict) -> bool | None:
        try:
            result = self.on_save_settings(values)
        except Exception as exc:  # noqa: BLE001
            print(f"[ezagent-voice] Guardar ajustes: {exc}", flush=True)
            return False
        self._invalidate()
        # None = aceptado y pendiente; el resultado real llega por
        # set_save_result() desde el hilo Tk.
        return None if result is None else bool(result)

    def _test_voice(self, values: dict) -> None:
        try:
            self.on_test_voice(values)
        except Exception as exc:  # noqa: BLE001
            print(f"[ezagent-voice] Probar voz: {exc}", flush=True)

    def _hide_from_menu(self) -> None:
        try:
            self.on_hide()
        except Exception:
            pass

    def menu_height(self, now: float) -> float:
        if self._menu is None:
            return 0.0
        try:
            h = float(self._menu.height(now))
        except Exception:
            h = 0.0
        return max(0.0, min(float(H_MENU), h))

    # ------------------------------------------------------------- puntero
    def _current_tab_width(self) -> float:
        if self.menu_height(time.monotonic()) <= 0.5:
            return float(W_COMPACT)
        return min(float(W), max(float(W_COMPACT), float(self._tab_w.value)))

    def _control_layout(
        self, tab_w: float | None = None
    ) -> tuple[Rect, float, Rect]:
        """Devuelve (mic, centro de ojos, flecha) para el ancho actual."""
        if tab_w is None:
            tab_w = self._current_tab_width()
        span = float(W - W_COMPACT)
        progress = 0.0 if span <= 0 else (tab_w - W_COMPACT) / span
        progress = min(1.0, max(0.0, progress))
        eyes_cx = EYES_CX_COMPACT + (EYES_CX_WIDE - EYES_CX_COMPACT) * progress
        arrow_cx = tab_w - 31.0
        arrow = Rect(arrow_cx - 12.0, 6.0, arrow_cx + 12.0, 30.0)
        return MIC_BUTTON_COMPACT, eyes_cx, arrow

    @staticmethod
    def _reflect_side_rect(rect: Rect) -> Rect:
        return Rect(
            float(W_SIDE) - rect.x1,
            rect.y0,
            float(W_SIDE) - rect.x0,
            rect.y1,
        )

    def _side_control_layout(self) -> tuple[Rect, float, float, Rect]:
        """``(mic, cx_ojos, cy_ojos, flecha)`` para la píldora lateral."""
        mic = MIC_BUTTON_SIDE
        arrow = ARROW_BUTTON_SIDE
        if self._attachment == "right":
            mic = self._reflect_side_rect(mic)
            arrow = self._reflect_side_rect(arrow)
        return mic, float(W_SIDE) / 2.0, float(EYES_CY_SIDE), arrow

    def _hit_button(
        self, x: float, y: float, tab_w: float | None = None
    ) -> str | None:
        if self._is_side_attachment():
            mic, _eyes_cx, _eyes_cy, arrow = self._side_control_layout()
        else:
            mic, _eyes_cx, arrow = self._control_layout(tab_w)
        for name, rect in (("mic", mic), ("arrow", arrow)):
            if rect.x0 <= x <= rect.x1 and rect.y0 <= y <= rect.y1:
                return name
        return None

    def _pointer_geometry(
        self, x: float, y: float
    ) -> tuple[bool, tuple[float, float], tuple[float, float], bool, float]:
        menu_h = self.menu_height(time.monotonic())
        menu_visible = menu_h > 0.5
        if self._is_side_attachment():
            window_w, _window_h, panel_x, tab_x, tab_y, _p = (
                self._side_window_geometry(menu_h)
            )
            tab_w = float(W_SIDE)
            in_tab = (
                tab_x <= x <= tab_x + tab_w
                and tab_y <= y <= tab_y + H_SIDE
            )
            menu_coords = (
                (x - panel_x, y + H_ISLAND)
                if menu_visible
                else (x, y)
            )
            return (
                in_tab,
                (x - tab_x, y - tab_y),
                menu_coords,
                menu_visible,
                tab_w,
            )
        window_w = W if menu_visible else W_COMPACT
        tab_w = (
            min(float(W), max(float(W_COMPACT), float(self._tab_w.value)))
            if menu_visible
            else float(W_COMPACT)
        )
        tab_x = (window_w - tab_w) / 2.0
        tab_y = menu_h if (menu_visible and self._menu_above) else 0.0
        in_tab = (
            tab_x <= x <= tab_x + tab_w
            and tab_y <= y <= tab_y + H_ISLAND
        )
        menu_coords = (
            (x, y + H_ISLAND) if (menu_visible and self._menu_above) else (x, y)
        )
        return (
            in_tab,
            (x - tab_x, y - tab_y),
            menu_coords,
            menu_visible,
            tab_w,
        )

    def pointer_down(self, x: float, y: float) -> bool:
        in_tab, (lx, ly), (mx, my), menu_visible, tab_w = self._pointer_geometry(x, y)
        if in_tab:
            if self._menu is not None:
                try:
                    self._menu.finish_text_edit(commit=True)
                except Exception:
                    pass
            action = self._hit_button(lx, ly, tab_w)
            if action is None:
                # Con el menú abierto, el fondo de la isla no arrastra.
                return menu_visible
            self._pressed = action
            self._pressed_inside = True
            self._set_hover(action)
            self._invalidate()
            return True
        if menu_visible:
            try:
                self._menu.pointer_down(mx, my, time.monotonic())
            except Exception:
                pass
            return True
        return False

    def pointer_move(self, x: float, y: float) -> bool:
        in_tab, (lx, ly), (mx, my), menu_visible, tab_w = self._pointer_geometry(x, y)
        if in_tab:
            action = self._hit_button(lx, ly, tab_w)
            if action != self._hover:
                self._set_hover(action)
            if self._pressed is not None:
                self._pressed_inside = action == self._pressed
            return True
        if menu_visible:
            try:
                self._menu.pointer_move(mx, my, time.monotonic())
            except Exception:
                pass
            if self._hover is not None:
                self._set_hover(None)
            return True
        return False

    def pointer_up(self, x: float, y: float) -> bool:
        in_tab, (lx, ly), (mx, my), menu_visible, tab_w = self._pointer_geometry(x, y)
        if self._pressed is not None:
            action = self._pressed
            _hit = self._hit_button(lx, ly, tab_w)
            inside = in_tab and _hit == action
            self._pressed = None
            self._pressed_inside = False
            if inside:
                self._activate(action)
                self._set_hover(action)
            self._invalidate()
            return True
        if menu_visible:
            if not in_tab:
                try:
                    self._menu.pointer_up(mx, my, time.monotonic())
                except Exception:
                    pass
            return True
        return False

    def wheel(self, x: float, y: float, delta: int) -> bool:
        in_tab, _, (mx, my), menu_visible, tab_w = self._pointer_geometry(x, y)
        if menu_visible and not in_tab:
            try:
                return bool(self._menu.wheel(mx, my, delta, time.monotonic()))
            except Exception:
                return False
        return False

    def global_wheel(self, screen_x: int, screen_y: int, delta: int) -> bool:
        """Rueda capturada con hook: funciona aunque la isla no tenga foco."""
        island = self._island
        if island is None or not self._menu_open:
            return False
        x = int(screen_x) - int(island.x)
        y = int(screen_y) - int(island.y)
        if not (0 <= x < int(island.w) and 0 <= y < int(island.h)):
            return False
        return self.wheel(x / self.s, y / self.s, delta)

    def cursor_hand(self, x: float, y: float) -> bool:
        in_tab, (lx, ly), (mx, my), menu_visible, tab_w = self._pointer_geometry(x, y)
        if in_tab:
            return self._hit_button(lx, ly, tab_w) is not None
        if menu_visible:
            try:
                return self._menu.hit(mx, my, time.monotonic()) is not None
            except Exception:
                return False
        return False

    def _set_hover(self, action: str | None) -> None:
        if action == self._hover:
            return
        self._hover = action
        self._invalidate()

    def _activate(self, action: str) -> None:
        if action == "arrow":
            self.toggle_menu()
            return
        # La isla nombra el control ("mic"); la app escucha la intención
        # ("mute"). Sin este puente, pulsar el micrófono no hacía nada.
        intent = "mute" if action == "mic" else action
        try:
            self.on_action(intent)
        except Exception as exc:  # noqa: BLE001
            print(f"[ezagent-voice] Notch acción {intent}: {exc}", flush=True)

    # ------------------------------------------------------------- render
    def render_frame(self) -> np.ndarray:
        now = time.monotonic()
        dt = min(max(now - self._last_frame, 0.0), 0.05)
        self._last_frame = now

        self._ui.update(now)
        self._pose.update(now)
        self._arrow.update(now)
        self._tab_w.update(now)
        if self._menu is not None:
            try:
                self._menu.update(now)
            except Exception:
                pass
            try:
                self._dirty = bool(self._menu.settings.dirty())
            except Exception:
                pass
            if self._pending_save_result is not None:
                result, self._pending_save_result = self._pending_save_result, None
                try:
                    self._menu.on_applied(result)
                except Exception:
                    pass
            try:
                self._menu.set_input_level(
                    self._mic_level,
                    muted=self._ui.muted,
                    active=(
                        self._ui.in_call
                        and self._state == "listening"
                    ),
                )
            except Exception:
                pass

        target = min(1.0, max(0.0, (self._rms / RMS_FULL_SCALE) ** RMS_GAMMA))
        self._level.step(target, dt * 1000.0)

        # A-01/A-02
        if self._show_t0 is not None:
            elapsed = now - self._show_t0
            if elapsed >= T_OPEN / 1000.0:
                self._show_t0 = None
                self._pose.set_pose(self._ui.pose, now)
                self._last_pose_name = self._ui.pose
        if self._hide_t0 is not None:
            elapsed = now - self._hide_t0
            close_ms = T_CLOSE_EYES if self._hide_eyes else 0
            if elapsed >= (close_ms + T_CLOSE_RETRACT) / 1000.0:
                self._visible = False
                self._ui.set_visible(False)
                self._hide_t0 = None
                if self._island is not None:
                    self._island.hide()
                # No cambiamos el tamaño del DIB al final: la ventana se oculta.
                if self._is_side_attachment():
                    return np.zeros((self.H_SIDE, self.W_SIDE, 4), np.uint8)
                return np.zeros((self.H_ISLAND, self.W_COMPACT, 4), np.uint8)

        menu_h = self.menu_height(now)
        menu_visible = menu_h > 0.5
        if not menu_visible:
            self._menu_open = False
        scale = self.s
        side = self._is_side_attachment()
        if not self._dragging and not self._menu_open and self._island is not None:
            self._attachment = self._attachment_for_rect(
                self._island.x,
                self._island.y,
                self._island.w,
                self._island.h,
            )
            side = self._is_side_attachment()
        if side:
            window_w, height, panel_x, tab_x, tab_y, _progress = (
                self._side_window_geometry(menu_h)
            )
            tab_w = float(W_SIDE)
            tab_h = float(H_SIDE)
            surface = render_side_surface(
                window_w,
                height,
                self._theme,
                scale,
                attachment=self._attachment,
                panel_width=float(W),
                panel_height=menu_h,
            )
        else:
            height = H_ISLAND + menu_h
            window_w = W if menu_visible else W_COMPACT
            if menu_visible:
                tab_w = min(
                    float(W), max(float(W_COMPACT), float(self._tab_w.value))
                )
            else:
                tab_w = float(W_COMPACT)
            tab_h = float(H_ISLAND)
            surface = render_surface(
                window_w,
                height,
                self._theme,
                scale,
                tab_width=tab_w,
                attachment=self._attachment,
                menu_h=menu_h,
                menu_above=self._menu_above,
                body_width=tab_w,
            )
            tab_x = (window_w - tab_w) / 2.0
            tab_y = menu_h if (menu_visible and self._menu_above) else 0.0
            panel_x = 0.0
        frame_h = max(1, int(round(height * scale)))
        frame_w = max(1, int(round(window_w * scale)))
        buf = surface[..., [2, 1, 0, 3]].astype(np.float32) / 255.0

        # La isla se mantiene anclada en pantalla: el menú crece hacia el lado
        # con espacio y la ventana se desplaza en consecuencia.
        if side:
            mon, _work_bottom = self._monitor_bottom(
                self._anchor_x, self._anchor_y
            )
            if self._attachment == "left":
                window_x = float(mon[0])
            else:
                window_x = float(mon[2]) - window_w * scale
            window_y = self._anchor_y - (height * scale) / 2.0
        else:
            window_x = self._anchor_x - (window_w * scale) / 2.0
            window_y = self._anchor_y - (
                menu_h * scale if self._menu_above and menu_visible else 0.0
            )
        if self._island is not None:
            ix, iy = int(round(window_x)), int(round(window_y))
            if self._island.x != ix or self._island.y != iy:
                self._island.set_pos(ix, iy)

        # Ojos y controles (siempre sobre la isla); la isla no lleva texto.
        pose_name = "open" if self._show_t0 is not None else (
            "close" if self._hide_t0 is not None else self._ui.pose
        )
        if pose_name != self._last_pose_name:
            self._pose.set_pose(pose_name, now)
            self._last_pose_name = pose_name
            self._pose_t0 = now
        pose = self._pose.current()
        if not self._animations and pose.glow:
            # Movimiento reducido: sin glow, solo forma y color.
            pose = replace(pose, glow=0.0)
        capsule_alpha, arc_alpha = self._pose.shape_alphas(now)

        blink_allowed = (
            pose_name in ("idle", "listen")
            and not self._pose.animating()
            and self._show_t0 is None
            and self._hide_t0 is None
        )
        blink = self._blink.update(now, blink_allowed and self._animations)
        offset = (
            self._pose_offset(pose_name, now) if self._animations else (0.0, 0.0)
        )
        level = 1.0 if pose_name in ("listen", "speak") else 0.0
        wave_alpha = self._pose.wave_alpha.value
        if side:
            _mic, eyes_cx, eyes_cy, _handle = self._side_control_layout()
            eyes_x = tab_x + eyes_cx
            eyes_y = tab_y + eyes_cy
            gap_override = SIDE_EYES_GAP
            wave_below = True
        else:
            _mic, eyes_cx, _handle = self._control_layout(tab_w)
            eyes_x = tab_x + eyes_cx
            eyes_y = tab_y + EYES_CY
            gap_override = None
            wave_below = False
        draw_eyes(
            buf,
            eyes_x,
            eyes_y,
            pose,
            tuple(self._pose.color),
            scale,
            level=self._level.value if level else 0.0,
            t=now,
            theme=self._theme,
            blink=blink,
            offset=offset,
            capsule_alpha=capsule_alpha,
            arc_alpha=arc_alpha,
            wave_alpha=wave_alpha,
            wave_width=pose.wave_w,
            gap_override=gap_override,
            wave_below=wave_below,
        )

        self._draw_controls(buf, now, menu_h, tab_x, tab_y, tab_w)
        if menu_visible and self._menu is not None:
            try:
                if side:
                    vh = float(self._menu.viewport_height)
                    menu_buf = np.zeros(
                        (
                            max(1, int(round((H_ISLAND + vh) * scale))),
                            max(1, int(round(W * scale))),
                            4,
                        ),
                        np.float32,
                    )
                    self._menu.draw(menu_buf, now)
                    src0 = int(round(H_ISLAND * scale))
                    src_h = min(
                        int(round(menu_h * scale)),
                        max(0, menu_buf.shape[0] - src0),
                        buf.shape[0],
                    )
                    dst_x = int(round(panel_x * scale))
                    src_w = min(menu_buf.shape[1], max(0, buf.shape[1] - dst_x))
                    if src_h > 0 and src_w > 0:
                        src = menu_buf[src0:src0 + src_h, :src_w]
                        dst = buf[:src_h, dst_x:dst_x + src_w]
                        inv = 1.0 - src[..., 3:4]
                        dst[..., :3] = src[..., :3] + dst[..., :3] * inv
                        dst[..., 3] = (
                            src[..., 3] + dst[..., 3] * inv[..., 0]
                        )
                elif self._menu_above:
                    menu_buf = np.zeros_like(buf)
                    self._menu.draw(menu_buf, now)
                    src0 = int(round(H_ISLAND * scale))
                    src1 = min(menu_buf.shape[0], src0 + int(round(menu_h * scale)))
                    dst1 = max(0, min(buf.shape[0], src1 - src0))
                    if dst1 > 0:
                        # El menú se dibuja con su pestaña arriba; al abrirlo
                        # hacia arriba se recorta esa franja y se compone sobre
                        # la superficie opaca, no se sustituye (si no, los
                        # huecos del contenido perforan el cuerpo del menú).
                        src = menu_buf[src0:src0 + dst1]
                        dst = buf[:dst1]
                        inv = 1.0 - src[..., 3:4]
                        buf[:dst1, :, :3] = src[..., :3] + dst[..., :3] * inv
                        buf[:dst1, :, 3] = (
                            src[..., 3] + dst[..., 3] * inv[..., 0]
                        )
                else:
                    self._menu.draw(buf, now)
            except Exception as exc:  # noqa: BLE001
                print(f"[ezagent-voice] Menú render: {exc}", flush=True)

        if menu_visible:
            # Durante la expansión de ancho el contenido del menú se recorta a
            # la silueta; así no asoma por los lados mientras la pieza crece.
            mask = surface[..., 3:4].astype(np.float32) / 255.0
            np.minimum(buf[..., 3:4], mask, out=buf[..., 3:4])
            np.minimum(buf[..., :3], buf[..., 3:4], out=buf[..., :3])

        # A-01/A-02: transformar el conjunto (cuerpo + contenido).
        if side:
            if self._show_t0 is not None:
                k = min(1.0, max(0.0, (now - self._show_t0) / (T_OPEN / 1000.0)))
                ease = EASE_SPRING(k) if self._animations else EASE_OUT(k)
                buf = self._transform(buf, 1.0, 0.0, ease)
            elif self._hide_t0 is not None:
                elapsed = now - self._hide_t0
                close_ms = T_CLOSE_EYES if self._hide_eyes else 0
                if elapsed >= close_ms / 1000.0:
                    k = min(
                        1.0,
                        max(
                            0.0,
                            (elapsed - close_ms / 1000.0)
                            / (T_CLOSE_RETRACT / 1000.0),
                        ),
                    )
                    buf = self._transform(buf, 1.0, 0.0, 1.0 - k)
        elif self._show_t0 is not None:
            k = min(1.0, max(0.0, (now - self._show_t0) / (T_OPEN / 1000.0)))
            ease = EASE_SPRING(k) if self._animations else EASE_OUT(k)
            buf = self._transform(buf, 0.55 + 0.45 * ease, -H_ISLAND * (1.0 - ease), ease)
        elif self._hide_t0 is not None:
            elapsed = now - self._hide_t0
            close_ms = T_CLOSE_EYES if self._hide_eyes else 0
            if elapsed < close_ms / 1000.0:
                # Los ojos ya están en close; el cuerpo aún no se retira.
                pass
            else:
                k = min(
                    1.0,
                    max(
                        0.0,
                        (elapsed - close_ms / 1000.0)
                        / (T_CLOSE_RETRACT / 1000.0),
                    ),
                )
                ease = EASE_IN(k) if self._animations else k
                buf = self._transform(
                    buf, 1.0 - 0.45 * ease, -H_ISLAND * ease, 1.0 - ease
                )

        # FPS adaptativo: 60 con animación/voz/menú; 30 en reposo vivo.
        menu_animating = False
        if self._menu is not None:
            try:
                menu_animating = bool(self._menu.is_animating(now))
            except Exception:
                menu_animating = False
        if self._island is not None:
            pose_age = now - self._pose_t0
            moving_pose = (
                pose_name == "think"
                or (pose_name == "notice" and pose_age < 0.5)
                or (pose_name == "error" and pose_age < 0.4)
                or (
                    pose_name in ("listen", "speak")
                    and (
                        abs(self._level.value - target) > 0.002
                        or self._level.value > 0.01
                    )
                )
            )
            active = (
                self._pose.animating()
                or self._show_t0 is not None
                or self._hide_t0 is not None
                or menu_animating
                or moving_pose
            )
            if active:
                interval = 1.0 / 60.0
            elif pose_name in ("idle", "listen"):
                interval = 1.0 / 30.0
            else:
                # Estados estáticos (mute, error ya sacudido, aviso ya
                # saltado): no se repinta hasta el siguiente evento.
                interval = 1.0
            self._island._interval = interval

        if buf.shape[0] != frame_h or buf.shape[1] != frame_w:
            # La silueta y el buffer deben coincidir con el tamaño de ventana.
            buf = buf[:frame_h, :frame_w]
        np.minimum(buf[..., :3], buf[..., 3:4], out=buf[..., :3])
        rgba = np.empty((frame_h, frame_w, 4), np.uint8)
        rgba[..., :3] = np.clip(buf[..., :3] * 255.0 + 0.5, 0, 255)
        rgba[..., 3] = np.clip(buf[..., 3] * 255.0 + 0.5, 0, 255)
        return rgba[..., [2, 1, 0, 3]]

    def _pose_offset(self, pose_name: str, now: float) -> tuple[float, float]:
        if pose_name == "idle":
            t = now - self._pose_t0
            return (2.0 * math.sin(2.0 * math.pi * t / 7.0), 0.0)
        if pose_name == "think":
            t = now - self._pose_t0
            return (-5.0 * math.cos(2.0 * math.pi * t / 1.5), -2.0)
        if pose_name == "notice":
            t = now - self._pose_t0
            if t < 0.5:
                phase = (t % 0.25) / 0.25
                return (0.0, -3.0 * math.sin(math.pi * phase))
            return (0.0, 0.0)
        if pose_name == "error":
            t = now - self._pose_t0
            steps = ((0.0, 0.0), (0.2, -3.0), (0.4, 3.0), (0.6, -2.0), (0.8, 2.0), (1.0, 0.0))
            if t >= 0.4:
                return (0.0, 0.0)
            k = t / 0.4
            for (k0, x0), (k1, x1) in zip(steps, steps[1:]):
                if k <= k1:
                    f = (k - k0) / max(k1 - k0, 1e-6)
                    return (x0 + (x1 - x0) * f, 0.0)
        return (0.0, 0.0)

    def _draw_controls(
        self,
        buf: np.ndarray,
        now: float,
        menu_h: float,
        offset_x: float,
        offset_y: float,
        tab_w: float = W_COMPACT,
    ) -> None:
        theme = palette(self._theme)
        hover = self._hover
        pressed = self._pressed
        ox, oy = offset_x, offset_y
        if self._is_side_attachment():
            mic_rect, _eyes_cx, _eyes_cy, arrow_rect = self._side_control_layout()
        else:
            mic_rect, _eyes_cx, arrow_rect = self._control_layout(tab_w)
        mic_cx = (mic_rect.x0 + mic_rect.x1) / 2 + ox
        mic_cy = (mic_rect.y0 + mic_rect.y1) / 2 + oy
        arrow_cx = (arrow_rect.x0 + arrow_rect.x1) / 2 + ox
        arrow_cy = (arrow_rect.y0 + arrow_rect.y1) / 2 + oy

        # Micrófono: icono desnudo, sin disco ni fondo.
        mic_scale = BUTTON_PRESSED_SCALE if pressed == "mic" else 1.0
        if self._ui.muted:
            draw_icon(
                buf, "microphone-slash", mic_cx, mic_cy,
                ICON_MIC_SIZE * mic_scale, theme["ERR"][:3], self.s,
            )
        else:
            draw_icon(
                buf, "microphone", mic_cx, mic_cy,
                ICON_MIC_SIZE * mic_scale, theme["TEXT_2"][:3], self.s,
            )

        # Flecha desnuda a la derecha: gira con el menú, sin cápsula.
        if self._menu_open:
            arrow_color = theme["MINT"][:3]
        elif hover == "arrow" or pressed == "arrow":
            arrow_color = theme["TEXT"][:3]
        else:
            arrow_color = theme["TEXT_2"][:3]
        base_angle = 0.0
        if self._attachment == "left":
            base_angle = -90.0
        elif self._attachment == "right":
            base_angle = 90.0
        draw_chevron(
            buf, arrow_cx, arrow_cy,
            ICON_ARROW_SIZE * (
                BUTTON_PRESSED_SCALE if pressed == "arrow" else 1.0
            ),
            arrow_color, self.s, angle_deg=base_angle + self._arrow.value,
        )
        if self._dirty and menu_h <= 0.5:
            if self._is_side_attachment():
                dot_x = arrow_rect.x1 - 1.0 + ox
                dot_y = arrow_rect.y1 - 3.0 + oy
            else:
                dot_x = arrow_rect.x1 - 2.0 + ox
                dot_y = 8.0 + oy
            fill_circle(
                buf, dot_x, dot_y, 2.5, theme["WARM"][:3], self.s
            )

    def _transform(
        self, buf: np.ndarray, sx: float, dy: float, opacity: float
    ) -> np.ndarray:
        if abs(sx - 1.0) < 0.001 and abs(dy) < 0.01 and opacity >= 0.999:
            return buf
        scale = self.s
        h, w = buf.shape[:2]
        arr = np.clip(buf * 255.0 + 0.5, 0, 255).astype(np.uint8)
        img = Image.fromarray(arr, "RGBA")
        new_w = max(1, int(round(w * max(0.1, sx))))
        resized = img.resize((new_w, h), Image.BILINEAR)
        canvas = Image.new("RGBA", (w, h), (0, 0, 0, 0))
        canvas.paste(resized, ((w - new_w) // 2, int(round(dy * scale))))
        out = np.asarray(canvas, dtype=np.float32) / 255.0
        return out * max(0.0, min(1.0, opacity))
