"""La ventana del chat: la forma de la barra (esfera o píldora) se transforma en el panel.

``ChatPanel`` lleva el ciclo de vida (abrir, animar, cerrar), la geometría y el reparto del ratón
y del teclado; ``ChatWindow`` es la ventana *layered* que lo pinta. El contenido (cabecera,
mensajes, entrada) lo dibuja ``ChatView``; aquí solo se compone con la transformación:

* al abrir, un rectángulo redondeado pasa del tamaño de la forma al del panel, con el fluido
  Marea como relleno que se funde al fondo del panel, y el contenido aparece al final;
* al cerrar, el recorrido es el inverso y la barra recupera su forma.

Con movimiento reducido no hay transformación: solo un fundido.
"""

from __future__ import annotations

import time
from dataclasses import dataclass
from typing import Final

import numpy as np
from PIL import Image

from ..chat import media
from ..chat.session import ChatModel
from . import hotkey_field as hk
from . import layered as _shell
from .bar_theme import card_palette
from .chat_view import Action, ChatView

PANEL_W: Final[float] = 440.0
PANEL_H: Final[float] = 640.0
MARGIN: Final[float] = 16.0  # aire alrededor del panel para la sombra (px lógicos)
SCREEN_PAD: Final[float] = 12.0  # separación mínima al borde de la pantalla
PANEL_RADIUS: Final[float] = 22.0
MORPH_SECS: Final[float] = 0.36
FADE_SECS: Final[float] = 0.14  # con movimiento reducido

Rect = tuple[float, float, float, float]


# ---------------------------------------------------------------- geometría
def smoothstep(x: float) -> float:
    x = max(0.0, min(1.0, x))
    return x * x * (3.0 - 2.0 * x)


def ease(x: float) -> float:
    """Aceleración y frenado suaves (cúbica): la forma arranca con calma y se asienta sin golpe."""
    x = max(0.0, min(1.0, x))
    return 4 * x**3 if x < 0.5 else 1.0 - (-2 * x + 2) ** 3 / 2


def lerp(a: float, b: float, k: float) -> float:
    return a + (b - a) * k


def lerp_rect(a: Rect, b: Rect, k: float) -> Rect:
    return (lerp(a[0], b[0], k), lerp(a[1], b[1], k), lerp(a[2], b[2], k), lerp(a[3], b[3], k))


def panel_size(work: tuple[int, int, int, int], scale: float) -> tuple[float, float]:
    """Tamaño lógico del panel: el de fábrica, recortado si la pantalla es pequeña."""
    avail_w = (work[2] - work[0]) / scale - 2 * SCREEN_PAD
    avail_h = (work[3] - work[1]) / scale - 2 * SCREEN_PAD
    return (max(320.0, min(PANEL_W, avail_w)), max(360.0, min(PANEL_H, avail_h)))


def panel_rect(form: Rect, work: tuple[int, int, int, int], scale: float) -> Rect:
    """Rectángulo físico del panel: crece desde la forma hacia donde haya más espacio.

    Centrado en la forma y pegado a ella por el lado que mira al centro de la pantalla (una
    barra abajo despliega hacia arriba, una arriba hacia abajo); siempre dentro de la pantalla.
    """
    pw, ph = (v * scale for v in panel_size(work, scale))
    pad = SCREEN_PAD * scale
    cx = (form[0] + form[2]) / 2
    x0 = max(work[0] + pad, min(cx - pw / 2, work[2] - pad - pw))
    mid = (work[1] + work[3]) / 2
    if (form[1] + form[3]) / 2 > mid:  # barra en la mitad de abajo: crece hacia arriba
        y1 = min(form[3], work[3] - pad)
        y0 = max(work[1] + pad, y1 - ph)
        y1 = y0 + ph
    else:
        y0 = max(work[1] + pad, form[1])
        y1 = min(work[3] - pad, y0 + ph)
        y0 = y1 - ph
    return (x0, y0, x0 + pw, y0 + ph)


def rounded_coverage(w: int, h: int, radius: float, inset: float = 0.0) -> np.ndarray:
    """Cobertura (0..1) suave de un rectángulo redondeado ``w``×``h`` (distancia con signo)."""
    w, h = max(2, w), max(2, h)
    r = max(0.0, min(radius, min(w, h) / 2.0)) - inset
    xs = np.abs(np.arange(w, dtype=np.float32) + 0.5 - w / 2)[None, :]
    ys = np.abs(np.arange(h, dtype=np.float32) + 0.5 - h / 2)[:, None]
    dx = np.maximum(xs - (w / 2 - inset - r), 0.0)
    dy = np.maximum(ys - (h / 2 - inset - r), 0.0)
    dist = np.sqrt(dx * dx + dy * dy) - r
    return np.clip(0.5 - dist, 0.0, 1.0)


@dataclass(slots=True)
class _Anim:
    state: str = "closed"  # closed | opening | open | closing
    t0: float = 0.0
    p: float = 0.0  # 0 = forma, 1 = panel


# ------------------------------------------------------------------ ventana
class ChatWindow(_shell.LayeredWindow):
    """Ventana *layered* del chat; delega todo en ``ChatPanel``."""

    lazy_drag = True  # moverla no repinta (el contenido no depende de dónde esté)

    def __init__(self, panel: ChatPanel, x: int, y: int, w: int, h: int) -> None:
        super().__init__(int(w), int(h), int(x), int(y), click_through=False, title="chat")
        self.panel = panel
        self._interval = 1.0 / 60.0

    def _install_mouse_hook(self) -> None:
        if _shell.wheel_follows_pointer():
            return None
        super()._install_mouse_hook()

    def render(self):
        return self.panel.render()

    def clamp_position(self, x: int, y: int) -> tuple[int, int]:
        return self.panel.clamp(x, y)

    def on_pointer_down(self, x: int, y: int) -> bool:
        return self.panel.pointer_down(x, y)

    def on_pointer_move(self, x: int, y: int) -> bool:
        return self.panel.hover(x, y)

    def on_pointer_up(self, x: int, y: int) -> bool:
        return True

    def cursor_hand(self, x: int, y: int) -> bool:
        return self.panel.hover(x, y)

    def on_wheel(self, x: int, y: int, delta: int) -> bool:
        return self.panel.wheel(delta)

    def on_global_wheel(self, x: int, y: int, delta: int) -> bool:
        inside = 0 <= x - self.x < self.w and 0 <= y - self.y < self.h
        return self.panel.wheel(delta) if inside else False

    def on_key(self, vk: int, ctrl: bool) -> bool:
        return self.panel.key(vk, ctrl)

    def on_char(self, char: str) -> bool:
        return self.panel.char(char)

    def on_paste(self, text: str) -> bool:
        return self.panel.paste(text)

    def on_files(self, paths) -> None:
        self.panel.files(list(paths))

    def on_dpi_changed(self, dpi: int) -> None:
        self.panel.ui.set_scale(max(1.0, dpi / 96.0))


# -------------------------------------------------------------------- panel
class ChatPanel:
    """Abre, anima y cierra el chat; reparte las entradas y ejecuta las acciones de la vista."""

    def __init__(self, ui) -> None:
        self.ui = ui
        self.anim = _Anim()
        self.view: ChatView | None = None
        self.window: ChatWindow | None = None
        self._form: Rect = (0.0, 0.0, 0.0, 0.0)
        self._panel: Rect = (0.0, 0.0, 0.0, 0.0)
        self._cache: tuple[tuple, np.ndarray] | None = None
        self._shadow: tuple[tuple, np.ndarray] | None = None
        self._finished = False

    # ------------------------------------------------------------ ciclo de vida
    @property
    def is_open(self) -> bool:
        return self.anim.state in ("opening", "open")

    def toggle(self) -> bool:
        """Abre el chat si está cerrado y lo cierra si no. ``False`` si no se pudo abrir."""
        if self.is_open:
            self.close()
            return True
        return self.open()

    def open(self) -> bool:
        model: ChatModel | None = getattr(self.ui, "chat_model", None)
        anchor = self.ui._form_anchor() if hasattr(self.ui, "_form_anchor") else None
        if model is None or anchor is None:
            return False
        if self.anim.state == "closing":  # reabrir a mitad de cierre: vuelve desde donde iba
            self.anim = _Anim(
                "opening", time.monotonic() - self._elapsed_for(self.anim.p), self.anim.p
            )
            return True
        s, bs = self.ui.s, self.ui.bs
        fw, fh = self.ui.form_size()
        self._form = (
            anchor[0] - fw * bs / 2,
            anchor[1] - fh * bs / 2,
            anchor[0] + fw * bs / 2,
            anchor[1] + fh * bs / 2,
        )
        work = _shell.work_area()
        self._panel = panel_rect(self._form, work, s)
        pw, ph = panel_size(work, s)
        self.view = ChatView(
            model,
            roots=getattr(self.ui, "chat_roots", None),
            base_dirs=getattr(self.ui, "chat_base_dirs", ()),
            hotkey_label=_hotkey_label(getattr(self.ui, "chat_hotkey", "")),
        )
        self._configure_view(pw, ph)
        m = round(MARGIN * s)
        self.window = ChatWindow(
            self,
            round(self._panel[0]) - m,
            round(self._panel[1]) - m,
            round(self._panel[2] - self._panel[0]) + 2 * m,
            round(self._panel[3] - self._panel[1]) + 2 * m,
        )
        self.anim = _Anim("opening", time.monotonic(), 0.0)
        self._finished = False
        self._cache = None
        self.ui.set_chat_open(True)
        if not self.window.start():
            self._teardown()
            return False
        self.window.show()
        self.window.set_keyboard_enabled(True)
        return True

    def close(self) -> None:
        if self.anim.state in ("closed", "closing"):
            return
        now = time.monotonic()
        self.anim = _Anim("closing", now - self._elapsed_for(1.0 - self.anim.p), self.anim.p)
        self.ui.set_form_hidden(
            False
        )  # la forma vuelve debajo mientras el panel se encoge hacia ella
        if self.window is not None:
            self.window.set_keyboard_enabled(False)

    def close_now(self) -> None:
        """Cierra sin animar (al ocultar o cerrar la barra)."""
        self._teardown()

    def poll(self, now: float) -> None:
        """Lo llama la barra cada fotograma: retira la ventana cuando termina de cerrarse."""
        if self._finished:
            self._teardown()

    def _teardown(self) -> None:
        window, self.window = self.window, None
        self.view = None
        self.anim = _Anim()
        self._finished = False
        self._cache = None
        if window is not None:
            window.stop()
        self.ui.set_chat_open(False)
        self.ui.set_form_hidden(False)

    def _duration(self) -> float:
        return FADE_SECS if self.ui._reduced else MORPH_SECS

    def _elapsed_for(self, p: float) -> float:
        """Tiempo que corresponde a un progreso eased ``p`` (para reanudar sin saltos)."""
        lo, hi = 0.0, 1.0
        for _ in range(24):
            mid = (lo + hi) / 2
            lo, hi = (mid, hi) if ease(mid) < p else (lo, mid)
        return lo * self._duration()

    def _step(self, now: float) -> float:
        a = self.anim
        if a.state not in ("opening", "closing"):
            return 1.0 if a.state == "open" else 0.0
        raw = (now - a.t0) / self._duration()
        if a.state == "opening":
            a.p = ease(raw)
            if raw >= 1.0:
                a.state, a.p = "open", 1.0
        else:
            a.p = 1.0 - ease(raw)
            if raw >= 1.0:
                a.p = 0.0
                self._finished = True
        return a.p

    def _configure_view(self, pw: float, ph: float) -> None:
        if self.view is None:
            return
        theme = self.ui._theme_name()
        self.view.configure(pw, ph, self.ui.s, theme, card_palette(theme))

    # ------------------------------------------------------------------ dibujo
    def render(self) -> np.ndarray:
        window, view = self.window, self.view
        if window is None or view is None:
            return np.zeros((1, 1, 4), np.uint8)
        now = time.monotonic()
        p = self._step(now)
        if self.anim.state in ("opening", "open"):
            self.ui.set_form_hidden(True)  # recién con el primer cuadro pintado: sin parpadeo
        window._interval = 1.0 / (
            60.0 if (self.anim.state in ("opening", "closing") or view.animating(now)) else 30.0
        )
        out = self._compose(window, view, p, now)
        return out

    def _sig(self, view: ChatView, now: float) -> tuple:
        m = view.model
        return (
            m.version,
            round(view.scroll, 1),
            view.hover,
            view.input.text,
            view.input.cursor,
            view.input.anchor,
            view.mode,
            int(now * 2),
            int(now * 30) if view.animating(now) else 0,
            view.g.theme,
            round(view.g.width),
        )

    def _content(self, view: ChatView, now: float) -> np.ndarray:
        sig = self._sig(view, now)
        if self._cache is None or self._cache[0] != sig:
            self._cache = (sig, view.render(now))
        return self._cache[1]

    def _compose(self, window: ChatWindow, view: ChatView, p: float, now: float) -> np.ndarray:
        s = self.ui.s
        ww, wh = window.w, window.h
        canvas = np.zeros((wh, ww, 4), np.float32)
        ox, oy = window.x, window.y
        panel = (self._panel[0] - ox, self._panel[1] - oy, self._panel[2] - ox, self._panel[3] - oy)
        form = (self._form[0] - ox, self._form[1] - oy, self._form[2] - ox, self._form[3] - oy)
        reduced = self.ui._reduced
        rect = panel if reduced else lerp_rect(form, panel, p)
        form_r = (
            min(form[2] - form[0], form[3] - form[1]) / 2
            if self.ui.layout == "circle"
            else (form[3] - form[1]) / 2
        )
        radius = PANEL_RADIUS * s if reduced else lerp(form_r, PANEL_RADIUS * s, p)
        self._paint_shape(canvas, rect, radius, p, now, reduced)
        reveal = p if reduced else smoothstep((p - 0.68) / 0.32)
        if reveal > 0.01:
            self._paint_content(canvas, view, rect, radius, reveal, now)
        return (np.clip(canvas, 0.0, 1.0) * 255.0 + 0.5).astype(np.uint8)[..., [2, 1, 0, 3]]

    def _paint_shape(
        self, canvas, rect: Rect, radius: float, p: float, now: float, reduced: bool
    ) -> None:
        x0, y0, x1, y1 = (round(v) for v in rect)
        w, h = x1 - x0, y1 - y0
        if w < 2 or h < 2:
            return
        cov = rounded_coverage(w, h, radius)
        solid = (
            p if reduced else smoothstep((p - 0.4) / 0.55)
        )  # cuánto del relleno es ya el fondo del panel
        body = np.asarray(self.ui.chat_body_rgb(), np.float32) / 255.0
        fluid = self.ui.chat_fluid(w, h, now)  # RGB float 0..1 de tamaño (h, w) o None
        rgb = body[None, None, :] * solid + (
            fluid if fluid is not None else body[None, None, :]
        ) * (1.0 - solid)
        alpha = cov * (p if reduced else 1.0)
        self._shadow_under(canvas, rect, radius, p)
        self._composite(
            canvas, np.concatenate([rgb * alpha[..., None], alpha[..., None]], axis=2), x0, y0
        )
        ring = np.clip(cov - rounded_coverage(w, h, radius, inset=max(1.0, self.ui.s)), 0.0, 1.0)
        line = np.asarray(card_palette(self.ui._theme_name())["LINE"], np.float32) / 255.0
        la = ring * line[3] * 2.2 * solid
        self._composite(
            canvas,
            np.concatenate([line[None, None, :3] * la[..., None], la[..., None]], axis=2),
            x0,
            y0,
        )

    def _shadow_under(self, canvas, rect: Rect, radius: float, p: float) -> None:
        k = smoothstep((p - 0.68) / 0.32) * 0.55
        if k <= 0.01:
            return
        x0, y0, x1, y1 = (round(v) for v in rect)
        w, h = x1 - x0, y1 - y0
        grow = round(MARGIN * self.ui.s * 0.8)
        for step in range(3):
            g = grow * (step + 1) // 3
            cov = rounded_coverage(w + 2 * g, h + 2 * g, radius + g)
            a = cov * (k * 0.10)
            layer = np.zeros(cov.shape + (4,), np.float32)
            layer[..., 3] = a
            self._composite(canvas, layer, x0 - g, y0 - g + round(self.ui.s * 5))

    def _paint_content(
        self, canvas, view: ChatView, rect: Rect, radius: float, reveal: float, now: float
    ) -> None:
        content = self._content(view, now)
        x0, y0, x1, y1 = (round(v) for v in rect)
        w, h = x1 - x0, y1 - y0
        cov = rounded_coverage(w, h, radius)
        part = content[:h, :w]
        mask = cov[: part.shape[0], : part.shape[1], None] * reveal
        self._composite(canvas, part * mask, x0, y0)

    @staticmethod
    def _composite(canvas: np.ndarray, src: np.ndarray, x0: int, y0: int) -> None:
        """``src`` (premultiplicado) sobre ``canvas``, recortado a sus límites."""
        sx0, sy0 = max(0, -x0), max(0, -y0)
        dx0, dy0 = max(0, x0), max(0, y0)
        w = min(src.shape[1] - sx0, canvas.shape[1] - dx0)
        h = min(src.shape[0] - sy0, canvas.shape[0] - dy0)
        if w <= 0 or h <= 0:
            return
        top = src[sy0 : sy0 + h, sx0 : sx0 + w]
        region = canvas[dy0 : dy0 + h, dx0 : dx0 + w]
        region[...] = top + region * (1.0 - top[..., 3:4])

    # ------------------------------------------------------------------ entradas
    def _local(self, x: int, y: int) -> tuple[float, float] | None:
        """Punto de la ventana → punto lógico del panel (``None`` fuera del panel o animando)."""
        if self.anim.state != "open" or self.window is None:
            return None
        s = self.ui.s
        px, py = x - (self._panel[0] - self.window.x), y - (self._panel[1] - self.window.y)
        return (px / s, py / s)

    def pointer_down(self, x: int, y: int) -> bool:
        local = self._local(x, y)
        if local is None or self.view is None:
            return True  # durante la animación no se arrastra ni se pulsa
        if self.view._at(*local) is None:
            return False  # zona sin controles: arrastra la ventana
        self._perform(self.view.pointer_down(*local))
        return True

    def hover(self, x: int, y: int) -> bool:
        local = self._local(x, y)
        if local is None or self.view is None:
            return False
        self.view.pointer_move(*local)
        return self.view.cursor_is_hand(*local)

    def wheel(self, delta: int) -> bool:
        if self.view is None or self.anim.state != "open":
            return False
        self.view.wheel(delta)
        return True

    def key(self, vk: int, ctrl: bool) -> bool:
        if self.view is None or self.anim.state != "open":
            return True  # tragarse las teclas mientras se anima
        shift = "shift" in hk.os_modifiers()
        self._perform(self.view.key(vk, ctrl, shift))
        return True

    def char(self, ch: str) -> bool:
        if self.view is not None and self.anim.state == "open":
            self.view.char(ch)
        return True

    def paste(self, text: str) -> bool:
        if self.view is not None and self.anim.state == "open":
            self.view.paste(text)
        return True

    def files(self, paths: list[str]) -> None:
        if self.view is not None:
            self.view.add_files(paths)

    def clamp(self, x: int, y: int) -> tuple[int, int]:
        wa = _shell.work_area()
        w = self.window.w if self.window else 0
        h = self.window.h if self.window else 0
        return (max(wa[0], min(x, wa[2] - w)), max(wa[1], min(y, wa[3] - h)))

    # ------------------------------------------------------------------ acciones
    def _perform(self, action: Action | None) -> None:
        if action is None:
            return
        ui, kind, data = self.ui, action.kind, action.data
        if kind == "send":
            ui.on_chat_send(str(data))
        elif kind == "close":
            self.close()
        elif kind == "voice":
            ui.on_chat_voice(bool(data))
        elif kind == "new":
            ui.on_chat_new()
        elif kind == "stop":
            ui.on_chat_stop()
        elif kind == "confirm":
            ui.on_chat_confirm(bool(data))
        elif kind == "copy":
            self._copy(str(data))
        elif kind == "open":
            media.open_file(data)  # type: ignore[arg-type]
        elif kind == "reveal":
            media.reveal_file(data)  # type: ignore[arg-type]

    def _copy(self, text: str) -> None:
        if self.window is not None and text:
            self.window.copy_text(text)


def _hotkey_label(spec: str) -> str:
    parts = hk.parts_for(spec) if spec else []
    return "+".join(parts) if parts else "Ctrl+Alt+T"


def fluid_array(rgb: np.ndarray | None, w: int, h: int) -> np.ndarray | None:
    """El fluido de la forma (RGB uint8 de rejilla gruesa) estirado a ``w``×``h`` en float 0..1."""
    if rgb is None:
        return None
    img = Image.fromarray(rgb, "RGB").resize((max(2, w), max(2, h)), Image.BICUBIC)
    return np.asarray(img, np.float32) / 255.0


__all__ = ["ChatPanel", "ChatWindow", "ease", "panel_rect", "panel_size", "rounded_coverage"]
