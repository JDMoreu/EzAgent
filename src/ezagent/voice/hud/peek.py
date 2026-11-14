"""Vista rápida de la actividad: una tarjeta compacta que nace de la forma.

Rest the pointer on the shape for a moment, or let a new outcome arrive, and a
small card rises out of the bar sharing its material. Rows follow priority
(needs you → failed → working → waiting → stale → done); each one has a state
mark, the task, and «estado · agente · tiempo». There is no header, pin or close
button: the card leaves when the pointer does (after a short grace period so it
can be reached), and clicking a row brings that agent's terminal forward.

Threads: ``tick`` runs on the bar's paint thread, ``refresh``/``toast`` on the
Tk thread and ``render`` on the card's own paint thread. Shared state lives
behind ``_lock``; the static layer is cached and only marks move per frame.
"""

from __future__ import annotations

import threading
import time

import numpy as np

from ..activity import WORKING, ActivityItem, dismissible
from . import layered as _shell
from .peek_paint import (
    CLOSE_SECS,
    DISMISS_W,
    FS_META,
    FS_TASK,
    GAP,
    HOVER_DELAY,
    LEAVE_GRACE,
    MARGIN,
    MAX_ROWS,
    OPEN_SECS,
    PAD_X,
    PAD_Y,
    PEEK_W,
    REVEAL_SECS,
    RISE,
    ROW_H,
    TOAST_SECS,
    age,
    card_height,
    ease_out,
    fit,
    meta_line,
    paint_marks,
    paint_rows,
)
from .tokens import palette_version


# --------------------------------------------------------------- colocación
def place_card(
    hub: tuple[float, float, float, float],
    size: tuple[int, int],
    work: tuple[int, int, int, int],
    scale: float,
) -> tuple[int, int, bool]:
    """Esquina de la ventana de la tarjeta y si va encima del hub.

    Se centra en todo el hub (forma y botones). Si así se saliera de la pantalla,
    alinea su borde con el borde del hub de ese lado, no con la esquina de la
    pantalla: sigue «dentro» del hub. Va encima del hub en la mitad inferior de la
    pantalla y debajo en la superior, y cambia de lado si no cabe.
    """
    hx0, hy0, hx1, hy1 = hub
    w, h = size
    left, top, right, bottom = work
    m = round(MARGIN * scale)
    card_w, card_h = w - 2 * m, h - 2 * m
    gap = GAP * scale
    x = (hx0 + hx1) / 2 - card_w / 2  # borde izquierdo de la tarjeta (sin sombra)
    if x + card_w > right:
        x = min(hx1, right) - card_w
    if x < left:
        x = max(hx0, left)
    x = min(x, right - card_w)  # una tarjeta más ancha que el hub nunca sale por la derecha
    above = (hy0 + hy1) / 2 > (top + bottom) / 2
    if above and hy0 - gap - card_h < top:
        above = False
    elif not above and hy1 + gap + card_h > bottom:
        above = True
    y = hy0 - gap - card_h if above else hy1 + gap
    return (round(x) - m, round(y) - m, above)


# ----------------------------------------------------------------- ventana
class PeekWindow(_shell.LayeredWindow):
    lazy_drag = True

    def __init__(self, peek: Peek, x: int, y: int, w: int, h: int) -> None:
        super().__init__(int(w), int(h), int(x), int(y), click_through=False, title="peek")
        self.peek = peek
        self._interval = 1.0 / 60.0

    def _install_mouse_hook(self) -> None:
        return None

    def render(self):
        return self.peek.render()

    def on_pointer_down(self, x: int, y: int) -> bool:
        return True  # la tarjeta no se arrastra

    def on_pointer_up(self, x: int, y: int) -> bool:
        self.peek.click(x, y)
        return True

    def cursor_hand(self, x: int, y: int) -> bool:
        return self.peek.row_at(x, y) is not None

    def clamp_position(self, x: int, y: int) -> tuple[int, int]:
        return (self.x, self.y)


# -------------------------------------------------------------- controlador
class Peek:
    """Decide cuándo se ve la tarjeta y qué muestra; la ventana se crea al primer uso."""

    def __init__(self, ui) -> None:
        self.ui = ui
        self._lock = threading.Lock()
        self._items: tuple[ActivityItem, ...] = ()
        self._mode: str | None = None  # "hover" | "toast" | None
        self._toast: tuple[str, float] | None = None  # pedido desde el hilo de Tk
        self._toast_key = ""
        self._until = 0.0
        self._hover_since: float | None = None
        self._out_since: float | None = None
        self._shown_at = 0.0
        self._closing: tuple[float, float] | None = None  # (instante, progreso al cerrar)
        self._above = True
        self._hover_row: int | None = None
        self._rows: tuple[tuple[ActivityItem, ...], int] = ((), 0)
        self._static: tuple | None = None
        self._win: PeekWindow | None = None

    # ---- datos (hilo de Tk)
    def refresh(self, items: tuple[ActivityItem, ...]) -> None:
        with self._lock:
            self._items = tuple(items)

    def toast(self, item: ActivityItem, now: float) -> None:
        with self._lock:
            self._toast = (item.key, now + TOAST_SECS.get(item.state, 3.5))

    def reveal(self, now: float) -> None:
        """Muestra la lista completa un rato (pedida por voz), sin esperar al puntero."""
        with self._lock:
            self._toast = ("", now + REVEAL_SECS)

    @property
    def _scale(self) -> float:
        """Escala de la barra (pantalla × tamaño elegido): la tarjeta crece y encoge con ella."""
        return float(getattr(self.ui, "bs", None) or self.ui.s)

    @property
    def is_open(self) -> bool:
        return self._mode is not None

    def hide_now(self) -> None:
        """Oculta la tarjeta sin animación (la barra se oculta)."""
        with self._lock:
            self._mode = self._closing = self._toast = None
        if self._win is not None:
            self._win.hide()

    def stop(self) -> None:
        win, self._win = self._win, None
        if win is not None:
            win.stop()

    # ---- estado (hilo de la barra)
    def tick(self, now: float, over_form: bool, blocked: bool) -> None:
        with self._lock:
            if blocked:
                self._hover_since = self._toast = None
                self._close(now)
            else:
                self._advance(now, over_form)
            plan = self._plan(now, self._visible_rows())
        self._apply(plan)

    def _advance(self, now: float, over_form: bool) -> None:
        if self._toast is not None and self._mode != "hover":
            self._toast_key, self._until = self._toast
            self._open(now, "toast")
        self._toast = None
        if not over_form:
            self._hover_since = None
        elif self._hover_since is None:
            self._hover_since = now
        ready = self._hover_since is not None and now - self._hover_since >= HOVER_DELAY
        if self._mode is None and ready and self._items:
            self._open(now, "hover")
        inside = over_form or self._pointer_in_card()
        if self._mode == "toast":
            if inside:
                self._mode = "hover"
            elif now >= self._until:
                self._close(now)
        if self._mode == "hover":
            if inside:
                self._out_since = None
            elif self._out_since is None:
                self._out_since = now
            if not self._items or (
                self._out_since is not None and now - self._out_since > LEAVE_GRACE
            ):
                self._close(now)
        self._hover_row = self._pointer_row() if self._mode else None

    def _open(self, now: float, mode: str) -> None:
        current = self._progress(now)
        self._mode = mode
        self._out_since = None
        self._shown_at = now - OPEN_SECS * current  # retoma desde donde iba
        self._closing = None

    def _close(self, now: float) -> None:
        if self._mode is not None:
            self._closing = (now, self._progress(now))
        self._mode = None

    def _progress(self, now: float) -> float:
        if self._closing is not None:
            at, start = self._closing
            return start * (1.0 - ease_out((now - at) / CLOSE_SECS))
        if self._mode is None:
            return 0.0
        return ease_out((now - self._shown_at) / OPEN_SECS)

    def _visible_rows(self) -> tuple[tuple[ActivityItem, ...], int]:
        if self._mode == "toast" and self._toast_key:
            match = tuple(i for i in self._items if i.key == self._toast_key)
            return (match or self._items[:1], 0)
        if self._mode is None:
            return self._rows  # se mantienen mientras se desvanece
        return (self._items[:MAX_ROWS], max(0, len(self._items) - MAX_ROWS))

    # ---- ventana
    def _window_size(self, rows: int, more: int) -> tuple[int, int]:
        s = self._scale
        return (
            round((PEEK_W + 2 * MARGIN) * s),
            round((card_height(max(1, rows), bool(more)) + 2 * MARGIN) * s),
        )

    def _plan(self, now: float, rows: tuple[tuple[ActivityItem, ...], int]) -> tuple | None:
        """Qué hacer con la ventana (con ``_lock`` tomado); lo aplica ``_apply`` sin él."""
        win = self._win
        if self._mode is None and self._progress(now) <= 0.0:
            self._closing = None
            return ("hide",) if win is not None and win._visible else None
        if not rows[0]:
            return None
        size = self._window_size(len(rows[0]), rows[1])
        fresh = rows != self._rows or win is None or not win._visible
        self._rows = rows
        animating = self._closing is not None or now - self._shown_at < OPEN_SECS
        spin = any(i.state == WORKING for i in rows[0])
        interval = 1.0 / (60.0 if animating else (30.0 if spin else 4.0))
        if fresh or (win.w, win.h) != size:
            return ("place", size, interval)
        return ("frame", interval)

    def _apply(self, plan: tuple | None) -> None:
        # DECISION: never with ``_lock`` held. ShowWindow/SetWindowPos on a window
        # owned by another thread wait until that thread handles the message, and
        # that thread may be in ``render()`` waiting for ``_lock``: a deadlock that
        # froze the bar (and then Tk in ``refresh``) — «la app no responde».
        if plan is None:
            return
        if plan[0] == "hide":
            win = self._win
            if win is not None:
                win.hide()
            return
        if plan[0] == "place":
            self._place(plan[1])
        win = self._win
        if win is not None:
            win._interval = plan[-1]
            win._dirty = True

    def _hub(self) -> tuple[float, float, float, float] | None:
        """Todo el hub en pantalla; sin él (pruebas), un cuadrado alrededor de la forma."""
        extent = getattr(self.ui, "hub_extent", None)
        if callable(extent):
            return extent()
        form = self.ui.form_extent()
        if form is None:
            return None
        ax, ay, half = form
        return (ax - half, ay - half, ax + half, ay + half)

    def _place(self, size: tuple[int, int]) -> None:
        hub = self._hub()
        if hub is None:
            return
        w, h = size
        x, y, above = place_card(hub, size, _shell.work_area(), self._scale)
        with self._lock:
            self._above = above
        if self._win is None:
            self._win = PeekWindow(self, x, y, w, h)
            # start() espera a que Windows cree la ventana: fuera del hilo de la barra,
            # que si no se salta cuadros (medido: hasta 60 ms) justo al aparecer.
            threading.Thread(target=self._win.start, daemon=True, name="ezagent-peek-start").start()
            return
        self._win.resize(w, h)
        self._win.set_pos(x, y)
        if not self._win._visible:
            self._win.show()

    def _cursor_client(self) -> tuple[int, int] | None:
        win = self._win
        if win is None or not win._visible:
            return None
        try:
            sx, sy = win._screen_cursor()
        except Exception:
            return None
        return (sx - win.x, sy - win.y)

    def _pointer_in_card(self) -> bool:
        cur = self._cursor_client()
        if cur is None:
            return False
        m, win = MARGIN * self._scale, self._win
        return m <= cur[0] <= win.w - m and m <= cur[1] <= win.h - m

    def _pointer_row(self) -> int | None:
        cur = self._cursor_client()
        return None if cur is None else self.row_at(*cur)

    def row_at(self, x: int, y: int) -> int | None:
        """Fila bajo un punto de la ventana (px físicos), o None."""
        s = self._scale
        lx, ly = x / s - MARGIN, y / s - MARGIN - PAD_Y
        rows = self._rows[0]
        if not (0 <= lx <= PEEK_W) or ly < 0:
            return None
        index = int(ly // ROW_H)
        return index if index < len(rows) else None

    def dismiss_at(self, x: int, y: int) -> str | None:
        """Clave de la fila cuya × está bajo el punto, o None."""
        index = self.row_at(x, y)
        rows = self._rows[0]
        if index is None or index >= len(rows) or not dismissible(rows[index]):
            return None
        lx = x / self._scale - MARGIN
        return rows[index].key if lx >= PEEK_W - PAD_X - DISMISS_W + 8 else None

    def dismiss(self, key: str) -> None:
        """Quita la fila al instante; la app la olvida de verdad en su hilo."""
        with self._lock:
            self._items = tuple(i for i in self._items if i.key != key)
            rows, more = self._rows
            self._rows = (tuple(i for i in rows if i.key != key), more)
            if self._mode == "toast" and self._toast_key == key:
                self._close(time.monotonic())
        try:
            self.ui.on_activity_dismiss(key)
        except Exception:  # noqa: BLE001 - un clic nunca tumba la barra
            pass

    def click(self, x: int, y: int) -> None:
        gone = self.dismiss_at(x, y)
        if gone is not None:
            self.dismiss(gone)
            return
        index = self.row_at(x, y)
        rows = self._rows[0]
        if index is not None and index < len(rows):
            try:
                self.ui.on_activity_open(rows[index].key)
            except Exception:  # noqa: BLE001 - un clic nunca tumba la barra
                pass

    # ---- dibujo (hilo de la tarjeta)
    def render(self) -> np.ndarray:
        now = time.monotonic()
        with self._lock:
            (rows, more), hover = self._rows, self._hover_row
            p, above = self._progress(now), self._above
        win = self._win
        w, h = (win.w, win.h) if win is not None else (1, 1)
        if p <= 0.0 or not rows:
            return np.zeros((h, w, 4), np.uint8)
        theme, scale = self.ui._theme_name(), self._scale
        reduced = bool(getattr(self.ui, "_reduced", False))
        wall = time.time()
        key = (
            tuple((i.key, i.state, i.title, i.agent, i.unseen, meta_line(i, wall)) for i in rows),
            more,
            hover,
            theme,
            palette_version(),
            scale,
            w,
            h,
        )
        if self._static is None or self._static[0] != key:
            self._static = (key, paint_rows(rows, more, w, h, scale, theme, hover, wall))
        buf = np.array(self._static[1], copy=True)
        paint_marks(buf, rows, scale, theme, now, reduced)
        return compose(buf, p, above, scale, reduced)


def compose(buf: np.ndarray, p: float, above: bool, scale: float, reduced: bool) -> np.ndarray:
    """Fundido y leve desplazamiento hacia la forma; nunca se reescala el contenido."""
    shift = 0 if reduced else int(round(RISE * (1.0 - p) * scale))
    out = np.zeros_like(buf)
    if shift and above:
        out[shift:] = buf[:-shift]
    elif shift:
        out[:-shift] = buf[shift:]
    else:
        out[:] = buf
    out *= p
    frame = (np.clip(out, 0.0, 1.0) * 255.0 + 0.5).astype(np.uint8)
    return np.ascontiguousarray(frame[..., [2, 1, 0, 3]])


__all__ = [
    "FS_META",
    "FS_TASK",
    "PEEK_W",
    "ROW_H",
    "Peek",
    "PeekWindow",
    "age",
    "card_height",
    "compose",
    "fit",
    "meta_line",
    "paint_marks",
    "paint_rows",
]
