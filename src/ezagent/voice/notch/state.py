"""Máquina de estados visual del notch.

La función ``display_state`` es pura y prueba la prioridad de la sección 9.2:
``off > error > speak > think > notice > mute > listen > idle``.

``UiState`` añade el comportamiento temporal que la especificación pide:
permanencia mínima de 120 ms, gracia de 250 ms en ``vad_end``, aviso de
1000 ms con vuelta a ``idle`` si no llega TTS en 1500 ms, y error transitorio
de 1500 ms. No conoce Tk, Win32 ni el pipeline: solo recibe eventos lógicos.
"""

from __future__ import annotations

import queue
import threading
import time

MIN_DWELL = 0.120
VAD_HANGOVER = 0.250
NOTICE_HOLD = 1.000
NOTICE_TTS_GRACE = 1.500
TRANSIENT_ERROR = 1.500

BASE_STATES = ("idle", "listen", "think", "speak", "notice")
DISPLAY_STATES = (
    "off",
    "error",
    "speak",
    "think",
    "notice",
    "mute",
    "listen",
    "idle",
)

STATE_WORD = {
    "idle": "ESPERA",
    "listen": "ESCUCHO",
    "think": "PIENSO",
    "speak": "HABLO",
    "notice": "AVISO",
    "mute": "PAUSA",
    "error": "ERROR",
    "open": "ABRE",
    "close": "CIERRA",
    "off": "",
}

POSE_FOR_STATE = {
    "idle": "idle",
    "listen": "listen",
    "think": "think",
    "speak": "speak",
    "notice": "notice",
    "mute": "mute",
    "error": "error",
    "open": "open",
    "close": "close",
    "off": "off",
}

# Eventos del pipeline real (app.py) -> estado base lógico. Los nombres de la
# izquierda son los del código actual, no los nombres idealizados de la spec.
APP_STATE_TO_BASE = {
    "idle": "idle",
    "listening": "listen",
    "thinking": "think",
    "transcribing": "think",
    "speaking": "speak",
    "notice": "notice",
    "error": None,
    "cancelled": None,
}


def display_state(
    visible: bool, error_active: bool, base: str, muted: bool
) -> str:
    """Función pura de la sección 9.2 / 12.5."""
    if not visible:
        return "off"
    if error_active:
        return "error"
    if base == "speak":
        return "speak"
    if base == "think":
        return "think"
    if base == "notice":
        return "notice"
    if muted:
        return "mute"
    if base == "listen":
        return "listen"
    return "idle"


class UiState:
    """Estado visual compartido entre el pipeline y el hilo de render."""

    def __init__(self, clock=time.monotonic):
        self._clock = clock
        self._lock = threading.RLock()
        self._events: queue.Queue[tuple[str, object, float]] = queue.Queue()
        self.visible = False
        self.in_call = False
        self.muted = False
        self.base = "idle"
        self.error_active = False
        self.menu_open = False

        self._displayed = "off"
        self._displayed_at = 0.0
        self._pending_base: str | None = None
        self._pending_at = 0.0
        self._vad_end_at: float | None = None
        self._notice_until: float | None = None
        self._notice_tts_deadline: float | None = None
        self._error_until: float | None = None

    # ------------------------------------------------------------ lectura
    @property
    def displayed(self) -> str:
        with self._lock:
            return self._displayed

    @property
    def word(self) -> str:
        return STATE_WORD.get(self.displayed, "")

    @property
    def pose(self) -> str:
        return POSE_FOR_STATE.get(self.displayed, "idle")

    def snapshot(self, now: float | None = None) -> tuple[str, str]:
        """Actualiza temporizadores y devuelve (estado mostrado, palabra)."""
        self.update(now)
        with self._lock:
            return self._displayed, STATE_WORD.get(self._displayed, "")

    # ------------------------------------------------------------ eventos
    def set_visible(self, visible: bool, now: float | None = None) -> None:
        now = self._now(now)
        self._push("visible", bool(visible), now)

    def set_in_call(self, in_call: bool, now: float | None = None) -> None:
        now = self._now(now)
        self._push("in_call", bool(in_call), now)

    def set_muted(self, muted: bool, now: float | None = None) -> None:
        now = self._now(now)
        self._push("muted", bool(muted), now)

    def set_menu_open(self, open_: bool) -> None:
        self._push("menu", bool(open_), self._now(None))

    def set_base(self, base: str, now: float | None = None) -> None:
        """Cambia el estado base. Respeta la permanencia mínima de 120 ms."""
        if base not in BASE_STATES:
            return
        now = self._now(now)
        self._push("base", base, now)

    def set_app_state(self, state: str, now: float | None = None) -> None:
        """Mapea el estado real de ``VoiceApp`` al modelo lógico."""
        if state in ("error", "cancelled"):
            self.raise_error("transient" if state == "cancelled" else "persistent")
            return
        # Cualquier estado normal implica que el error visual ya se resolvió.
        self.clear_error(now)
        base = APP_STATE_TO_BASE.get(state)
        if base is not None:
            self.set_base(base, now)

    def vad_end(self, now: float | None = None) -> None:
        """Cierra la escucha tras la gracia de 250 ms, salvo turn_end."""
        now = self._now(now)
        self._push("vad_end", now + VAD_HANGOVER, now)

    def turn_end(self, now: float | None = None) -> None:
        now = self._now(now)
        self._push("turn_end", None, now)

    def tts_start(self, now: float | None = None) -> None:
        now = self._now(now)
        self._push("tts_start", None, now)

    def notice(self, now: float | None = None) -> None:
        now = self._now(now)
        self._push("notice", now, now)

    def raise_error(self, kind: str = "transient", now: float | None = None) -> None:
        now = self._now(now)
        self._push("error", kind, now)

    def clear_error(self, now: float | None = None) -> None:
        self._push("error_clear", None, self._now(now))

    def hide(self, now: float | None = None) -> None:
        self.set_visible(False, now)

    # ------------------------------------------------------------ motor
    def _now(self, now: float | None) -> float:
        return self._clock() if now is None else float(now)

    def _push(self, kind: str, value: object, now: float) -> None:
        # La cola permite que los hilos del pipeline publiquen sin tocar el
        # estado; el hilo de render la vacía en update(). Si crece, no pasa
        # nada: se procesa en orden.
        self._events.put((kind, value, now))

    def _drain(self, now: float) -> None:
        while True:
            try:
                kind, value, at = self._events.get_nowait()
            except queue.Empty:
                return
            self._apply(kind, value, at, now)

    def _apply(self, kind: str, value: object, at: float, now: float) -> None:
        with self._lock:
            if kind == "visible":
                self.visible = bool(value)
            elif kind == "in_call":
                self.in_call = bool(value)
            elif kind == "muted":
                self.muted = bool(value)
            elif kind == "menu":
                self.menu_open = bool(value)
            elif kind == "base":
                self._queue_base(str(value), now)
            elif kind == "vad_end":
                self._vad_end_at = float(value)  # type: ignore[arg-type]
            elif kind == "turn_end":
                self._vad_end_at = None
                self._queue_base("think", now)
            elif kind == "tts_start":
                self._notice_until = None
                self._notice_tts_deadline = None
                self._queue_base("speak", now)
            elif kind == "notice":
                self._queue_base("notice", now)
                self._notice_until = float(at) + NOTICE_HOLD
                self._notice_tts_deadline = float(at) + NOTICE_TTS_GRACE
            elif kind == "error":
                self.error_active = True
                self._error_until = (
                    float(at) + TRANSIENT_ERROR
                    if str(value) == "transient"
                    else None
                )
            elif kind == "error_clear":
                self.error_active = False
                self._error_until = None

    def _queue_base(self, base: str, now: float) -> None:
        if base == self.base and self._pending_base is None:
            return
        # AVISO tiene prioridad sobre el nivel del micrófono y sobre el
        # regreso a idle mientras dura su ventana. Sin esto, cada bloque de
        # audio llamaba a set_base('listen') y borraba el aviso en un frame.
        if (
            self.base == "notice"
            and base in ("listen", "idle")
            and self._notice_until is not None
        ):
            return
        if self.error_active and base not in ("idle",):
            # El error manda; el base se aplica igualmente para cuando se
            # limpie, pero no reinicia el dwell del error.
            pass
        self.base = base
        self._pending_base = None
        self._pending_at = now
        # La permanencia mínima se aplica en update(); aquí solo fijamos el
        # cambio de base. El estado mostrado se recalcula en _recompute().

    def update(self, now: float | None = None) -> str:
        now = self._now(now)
        self._drain(now)
        with self._lock:
            if self._vad_end_at is not None and now >= self._vad_end_at:
                self._vad_end_at = None
                if self.base == "listen":
                    self.base = "idle"
            if self._notice_until is not None and now >= self._notice_until:
                # El AVISO no cae a idle hasta el deadline de 1500 ms sin
                # TTS (T-11); los 1000 ms solo protegen la permanencia.
                self._notice_until = None
            if (
                self._notice_tts_deadline is not None
                and now >= self._notice_tts_deadline
            ):
                self._notice_tts_deadline = None
                if self.base == "notice":
                    self.base = "idle"
            if self._error_until is not None and now >= self._error_until:
                self._error_until = None
                self.error_active = False
            self._recompute(now)
            return self._displayed

    def _recompute(self, now: float) -> None:
        target = display_state(
            self.visible, self.error_active, self.base, self.muted
        )
        if target == self._displayed:
            return
        # Excepción de la spec: error y off no esperan permanencia.
        if (
            target not in ("off", "error")
            and self._displayed not in ("off", "error")
            and now - self._displayed_at < MIN_DWELL
        ):
            # No aplicamos el cambio todavía. Se volverá a intentar en el
            # siguiente fotograma; el estado base ya está actualizado.
            return
        self._displayed = target
        self._displayed_at = now
