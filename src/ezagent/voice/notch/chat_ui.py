"""La ventana del chat: la forma de la barra (esfera o píldora) se transforma en el panel.

``ChatPanel`` lleva el ciclo de vida (abrir, animar, cerrar), el redimensionado y el reparto del
ratón y del teclado; ``ChatWindow`` es la ventana *layered* que lo pinta. El contenido (cabecera,
mensajes, entrada) lo dibuja ``ChatView``; aquí solo se compone con la transformación:

* al abrir, un rectángulo redondeado pasa del tamaño de la forma al del panel (con un pequeño
  rebote al asentarse), con el fluido Marea de relleno que se funde al fondo; el contenido aparece
  de arriba abajo mientras sube unos píxeles;
* al cerrar, el recorrido es el inverso y más corto, y el contenido se va antes que la forma;
* el orbe de la cabecera y la línea bajo ella son el mismo fluido de la barra y siguen su estado
  (escuchando, pensando, hablando): es la retroalimentación de qué está haciendo EzAgent.

La geometría del panel es *local a la ventana*: siempre está a ``MARGIN`` del borde, así que
mover o redimensionar la ventana nunca lo saca del lienzo. Con movimiento reducido no hay
transformación: solo un fundido.
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
from .chat_geometry import (
    FADE_SECS,
    GRIP,
    MARGIN,
    MIN_H,
    MIN_W,
    MORPH_SECS,
    PANEL_H,
    PANEL_RADIUS,
    PANEL_W,
    SCREEN_PAD,
    Edges,
    Rect,
    ease,
    edge_cursor,
    edges_at,
    lerp,
    lerp_rect,
    panel_limits,
    panel_rect,
    panel_size,
    resize_rect,
    rounded_coverage,
    smoothstep,
)
from .chat_paint import PaintMixin, row_reveal, shape_progress
from .chat_view import Action, ChatView

CLOSE_SECS: Final[float] = 0.30  # cerrar es más rápido que abrir
STATUS_TEXT: Final[dict[str, str]] = {
    "listen": "Escuchando…",
    "think": "Pensando…",
    "speak": "Hablando…",
    "ask": "Esperando tu respuesta",
    "error": "Algo salió mal",
}
ACTIVE_POSES: Final[frozenset[str]] = frozenset({"listen", "think", "speak", "ask"})


@dataclass(slots=True)
class _Anim:
    state: str = "closed"  # closed | opening | open | closing
    t0: float = 0.0
    p: float = 0.0  # 0 = forma, 1 = panel


@dataclass(slots=True)
class _Resize:
    edges: Edges
    cursor: tuple[int, int]  # posición del puntero al empezar (pantalla)
    rect: Rect  # rectángulo físico del panel al empezar (pantalla)


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
        return self.panel.pointer_move(x, y)

    def on_pointer_up(self, x: int, y: int) -> bool:
        return self.panel.pointer_up()

    def cursor_hand(self, x: int, y: int) -> bool:
        return self.panel.hover(x, y)

    def cursor_ident(self, x: int, y: int) -> int:
        return self.panel.cursor_ident(x, y)

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
class ChatPanel(PaintMixin):
    """Abre, anima y cierra el chat; reparte las entradas y ejecuta las acciones de la vista."""

    def __init__(self, ui) -> None:
        self.ui = ui
        self.anim = _Anim()
        self.view: ChatView | None = None
        self.window: ChatWindow | None = None
        self._form: Rect = (0.0, 0.0, 0.0, 0.0)  # la forma de la barra (pantalla, px físicos)
        self._cache: tuple[tuple, np.ndarray] | None = None
        self._base: tuple[tuple, np.ndarray] | None = None  # cuadro estable sin la parte viva
        self._finished = False
        self._resize: _Resize | None = None
        self._act = 0.0  # 0 = en reposo, 1 = trabajando: suaviza el orbe y la línea
        self._last = 0.0

    # ------------------------------------------------------------ geometría
    @property
    def _margin(self) -> int:
        return round(MARGIN * self.ui.s)

    @property
    def _panel(self) -> Rect:
        """Rectángulo físico del panel *en pantalla*: sigue a la ventana donde esté."""
        w = self.window
        if w is None:
            return (0.0, 0.0, 0.0, 0.0)
        m = self._margin
        return (float(w.x + m), float(w.y + m), float(w.x + w.w - m), float(w.y + w.h - m))

    def _panel_local(self) -> Rect:
        """El panel dentro del lienzo de la ventana: siempre a ``MARGIN`` del borde."""
        w, m = self.window, self._margin
        return (float(m), float(m), float(w.w - m), float(w.h - m)) if w else (0.0,) * 4

    def _form_local(self) -> Rect | None:
        """La forma de la barra en el lienzo, o ``None`` si queda fuera (el panel se movió lejos)."""
        w = self.window
        if w is None:
            return None
        x0, y0, x1, y1 = self._form
        form = (x0 - w.x, y0 - w.y, x1 - w.x, y1 - w.y)
        cx, cy = (form[0] + form[2]) / 2, (form[1] + form[3]) / 2
        return form if (0 <= cx <= w.w and 0 <= cy <= w.h) else None

    def _logical_size(self) -> tuple[float, float]:
        r = self._panel
        return ((r[2] - r[0]) / self.ui.s, (r[3] - r[1]) / self.ui.s)

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
                "opening",
                time.monotonic() - self._elapsed_for(self.anim.p, MORPH_SECS),
                self.anim.p,
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
        saved = getattr(self.ui, "chat_size", None)
        panel = panel_rect(self._form, work, s, saved)
        self.view = ChatView(
            model,
            roots=getattr(self.ui, "chat_roots", None),
            base_dirs=getattr(self.ui, "chat_base_dirs", ()),
            hotkey_label=_hotkey_label(getattr(self.ui, "chat_hotkey", "")),
        )
        self._configure_view((panel[2] - panel[0]) / s, (panel[3] - panel[1]) / s)
        m = round(MARGIN * s)
        self.window = ChatWindow(
            self,
            round(panel[0]) - m,
            round(panel[1]) - m,
            round(panel[2] - panel[0]) + 2 * m,
            round(panel[3] - panel[1]) + 2 * m,
        )
        self.anim = _Anim("opening", time.monotonic(), 0.0)
        self._finished = False
        self._cache = self._base = self._resize = None
        self._act, self._last = 0.0, time.monotonic()
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
        self._end_resize()
        anchor = self.ui._form_anchor() if hasattr(self.ui, "_form_anchor") else None
        if anchor is not None:  # la barra pudo moverse mientras tanto
            fw, fh = self.ui.form_size()
            bs = self.ui.bs
            self._form = (
                anchor[0] - fw * bs / 2,
                anchor[1] - fh * bs / 2,
                anchor[0] + fw * bs / 2,
                anchor[1] + fh * bs / 2,
            )
        now = time.monotonic()
        elapsed = self._elapsed_for(1.0 - self.anim.p, CLOSE_SECS)
        self.anim = _Anim("closing", now - elapsed, self.anim.p)
        self.ui.set_form_hidden(False)  # la forma vuelve debajo mientras el panel se encoge
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
        self._cache = self._base = self._resize = None
        if window is not None:
            window.stop()
        self.ui.set_chat_open(False)
        self.ui.set_form_hidden(False)

    def _duration(self, closing: bool = False) -> float:
        if self.ui._reduced:
            return FADE_SECS
        return CLOSE_SECS if closing else MORPH_SECS

    def _elapsed_for(self, p: float, duration: float) -> float:
        """Tiempo que corresponde a un progreso eased ``p`` (para reanudar sin saltos)."""
        duration = FADE_SECS if self.ui._reduced else duration
        lo, hi = 0.0, 1.0
        for _ in range(24):
            mid = (lo + hi) / 2
            lo, hi = (mid, hi) if ease(mid) < p else (lo, mid)
        return lo * duration

    def _step(self, now: float) -> float:
        a = self.anim
        if a.state not in ("opening", "closing"):
            return 1.0 if a.state == "open" else 0.0
        raw = (now - a.t0) / self._duration(a.state == "closing")
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

    # --------------------------------------------------------------- estado vivo
    def _pose(self) -> str:
        pose = str(self.ui.chat_pose()) if hasattr(self.ui, "chat_pose") else "idle"
        view = self.view
        if view is not None and (view.model.busy or view.model.confirm is not None):
            if pose in ("idle", "sleep", "mute"):
                pose = "ask" if view.model.confirm is not None else "think"
        return pose

    def _update_live(self, now: float) -> None:
        """Pone el rótulo de estado y suaviza cuánto «trabaja» el orbe."""
        view = self.view
        if view is None:
            return
        pose = self._pose()
        view.status = STATUS_TEXT.get(pose, "")
        dt = max(0.0, min(0.1, now - self._last))
        self._last = now
        target = 1.0 if pose in ACTIVE_POSES else 0.0
        self._act += (target - self._act) * min(1.0, dt * 7.0)
        if abs(target - self._act) < 0.004:
            self._act = target

    # ------------------------------------------------------------------ dibujo
    def render(self) -> np.ndarray:
        window, view = self.window, self.view
        if window is None or view is None:
            return np.zeros((1, 1, 4), np.uint8)
        now = time.monotonic()
        p = self._step(now)
        self._update_live(now)
        if self.anim.state in ("opening", "open"):
            self.ui.set_form_hidden(True)  # recién con el primer cuadro pintado: sin parpadeo
        moving = self.anim.state in ("opening", "closing") or view.animating(now)
        window._interval = 1.0 / (60.0 if moving else (40.0 if self._act > 0.02 else 20.0))
        if self.anim.state == "open":
            return self._stable(window, view, now)
        return self._compose(window, view, p, now, live=True)

    # ------------------------------------------------------------------ entradas
    def _local(self, x: int, y: int) -> tuple[float, float] | None:
        """Punto de la ventana → punto lógico del panel (``None`` si no está quieto y abierto)."""
        if self.anim.state != "open" or self.window is None:
            return None
        s, m = self.ui.s, self._margin
        return ((x - m) / s, (y - m) / s)

    def _edges(self, x: int, y: int) -> Edges | None:
        local = self._local(x, y)
        if local is None:
            return None
        pw, ph = self._logical_size()
        return edges_at(local[0], local[1], pw, ph, GRIP)

    def pointer_down(self, x: int, y: int) -> bool:
        local = self._local(x, y)
        if local is None or self.view is None:
            return True  # durante la animación no se arrastra ni se pulsa
        edges = self._edges(x, y)
        if edges is not None and self.window is not None:
            self._begin_resize(edges)
            return True
        if self.view._at(*local) is None:
            return False  # zona sin controles: arrastra la ventana
        self._perform(self.view.pointer_down(*local))
        return True

    def pointer_move(self, x: int, y: int) -> bool:
        """Movimiento con el botón pulsado dentro de la ventana (redimensionar)."""
        if self._resize is not None:
            self._drag_resize()
            return True
        return self.hover(x, y)

    def pointer_up(self) -> bool:
        self._end_resize()
        return True

    def hover(self, x: int, y: int) -> bool:
        local = self._local(x, y)
        if local is None or self.view is None:
            return False
        if self._edges(x, y) is not None:
            self.view.pointer_move(-1.0, -1.0)  # sobre el borde no hay nada «caliente»
            return False
        self.view.pointer_move(*local)
        return self.view.cursor_is_hand(*local)

    def cursor_ident(self, x: int, y: int) -> int:
        edges = self._resize.edges if self._resize is not None else self._edges(x, y)
        return edge_cursor(edges) if edges is not None else 0

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
        """Posición permitida de la ventana: el panel entero en pantalla (la sombra puede salirse)."""
        wa = _shell.work_area()
        w = self.window.w if self.window else 0
        h = self.window.h if self.window else 0
        m = self._margin
        return (
            max(wa[0] - m, min(x, wa[2] - w + m)),
            max(wa[1] - m, min(y, wa[3] - h + m)),
        )

    # ------------------------------------------------------------- redimensionar
    def _begin_resize(self, edges: Edges) -> None:
        window = self.window
        if window is None:
            return
        try:
            cursor = window._screen_cursor()
        except Exception:
            return
        self._resize = _Resize(edges, cursor, self._panel)

    def _drag_resize(self) -> None:
        window, view, rz = self.window, self.view, self._resize
        if window is None or view is None or rz is None:
            return
        try:
            cx, cy = window._screen_cursor()
        except Exception:
            return
        s, m = self.ui.s, self._margin
        rect = resize_rect(
            rz.edges, rz.rect, cx - rz.cursor[0], cy - rz.cursor[1], _shell.work_area(), s
        )
        pw, ph = round(rect[2] - rect[0]), round(rect[3] - rect[1])
        window.x, window.y = round(rect[0]) - m, round(rect[1]) - m
        window.resize(pw + 2 * m, ph + 2 * m)
        self._configure_view(pw / s, ph / s)
        self._cache = self._base = None

    def _end_resize(self) -> None:
        if self._resize is None:
            return
        self._resize = None
        remember = getattr(self.ui, "remember_chat_size", None)
        if remember is not None and self.window is not None:
            pw, ph = self._logical_size()
            remember(pw, ph)

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
            if self.view is not None:
                self.view.flash_copied(text)  # «COPIAR» → «COPIADO» un momento


def _hotkey_label(spec: str) -> str:
    parts = hk.parts_for(spec) if spec else []
    return "+".join(parts) if parts else "Ctrl+Alt+T"


def fluid_array(rgb: np.ndarray | None, w: int, h: int) -> np.ndarray | None:
    """El fluido de la forma (RGB uint8 de rejilla gruesa) estirado a ``w``×``h`` en float 0..1."""
    if rgb is None:
        return None
    img = Image.fromarray(rgb, "RGB").resize((max(2, w), max(2, h)), Image.BICUBIC)
    return np.asarray(img, np.float32) / 255.0


__all__ = [
    "FADE_SECS",
    "MARGIN",
    "MIN_H",
    "MIN_W",
    "MORPH_SECS",
    "PANEL_H",
    "PANEL_RADIUS",
    "PANEL_W",
    "SCREEN_PAD",
    "ChatPanel",
    "ChatWindow",
    "ease",
    "lerp",
    "lerp_rect",
    "panel_limits",
    "panel_rect",
    "panel_size",
    "rounded_coverage",
    "row_reveal",
    "shape_progress",
    "smoothstep",
]
