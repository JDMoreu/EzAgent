"""Pruebas de la instantánea de estado del notch (U4 de E0).

No abren ventanas ni tocan audio: construyen ``Notch(None)`` y llaman a
``render_frame()`` en el propio proceso de pytest, así que corren en CI
headless.
"""

from __future__ import annotations

import threading

import pytest

from ezagent.voice.notch.island import Notch
from ezagent.voice.notch.tokens import H_ISLAND, W_FLOAT


def _visible_notch() -> Notch:
    """Isla visible y en escucha, sin ventana real."""
    notch = Notch(None)
    notch.set_theme("dark")
    notch._visible = True
    notch._ui.set_visible(True)
    notch._ui.set_base("idle")
    notch.set_in_call(True)
    return notch


def test_render_uses_snapshot(monkeypatch):
    """El dibujo usa la instantánea y no el estado vivo posterior (U4)."""
    notch = _visible_notch()
    drawn: dict[str, object] = {}

    real_compose = Notch._compose

    def compose(self, snap, now):
        # Un setter de otro hilo cambia el estado justo DESPUÉS de copiarlo:
        # si el fotograma leyera el estado vivo, se vería esta versión.
        with self._state_lock:
            self._state = "error"
        self._muted = True
        self._theme = "light"
        self._level.value = 1.0
        self._arrow.value = 180.0
        self._hover = "mic"
        drawn["snap"] = snap
        return real_compose(self, snap, now)

    monkeypatch.setattr(Notch, "_compose", compose)
    frame = notch.render_frame()
    snap = drawn["snap"]

    # La instantánea es la del estado anterior al cambio.
    assert snap.state == "idle"
    assert snap.muted is False
    assert snap.theme == "dark"
    assert snap.mic_icon == "microphone"
    assert snap.level == pytest.approx(0.0)

    # Y el fotograma también: tamaño de la píldora, cuerpo oscuro y el
    # micrófono de escucha (no el tachado en rojo del mute posterior).
    assert frame.shape[:2] == (H_ISLAND, W_FLOAT)
    assert frame[10, W_FLOAT // 2, 3] == 255
    assert max(int(c) for c in frame[10, W_FLOAT // 2, :3]) < 60
    _blue, green, red, alpha = frame[18, 26]
    assert alpha == 255
    assert not (red > 180 and green < 100)


class _CountingLock:
    """Lock falso que cuenta adquisiciones; el contrato solo usa ``with``."""

    def __init__(self) -> None:
        self.count = 0

    def __enter__(self) -> "_CountingLock":
        self.count += 1
        return self

    def __exit__(self, *exc_info) -> bool:
        return False


def test_state_lock_exists():
    """``_state_lock`` es un Lock real y setters y render lo adquieren."""
    notch = _visible_notch()
    assert isinstance(notch._state_lock, type(threading.Lock()))

    counter = _CountingLock()
    notch._state_lock = counter

    notch.set_state("listen")  # con isla visible ya abierta: sin ventana
    notch.set_mic_level(0.5)
    notch.draw(0.01, wave_data=(0.1, 0.2, 0.3))
    assert counter.count >= 3
    assert notch._state == "listen"
    assert notch._mic_level == pytest.approx(0.5)
    assert notch._rms == pytest.approx(0.01)

    before = counter.count
    notch.render_frame()
    assert counter.count > before
