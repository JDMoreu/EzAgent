"""Hotkeys globales de Windows (RegisterHotKey), sin dependencias externas.

Basado en el enfoque de Dicta (MIT): un hilo registra la tecla y bombea
WM_HOTKEY; el resto de la app consume eventos de una cola.
"""

from __future__ import annotations

import ctypes
import queue
import threading
import time
from ctypes import wintypes

_user32 = ctypes.WinDLL("user32", use_last_error=True)

_MOD_ALT = 0x0001
_MOD_CONTROL = 0x0002
_MOD_SHIFT = 0x0004
_MOD_WIN = 0x0008
_MOD_NOREPEAT = 0x4000
_WM_HOTKEY = 0x0312
_PM_REMOVE = 0x0001

_KEYS = {
    "space": 0x20,
    "enter": 0x0D,
    "esc": 0x1B,
    "escape": 0x1B,
    "tab": 0x09,
    "backspace": 0x08,
    "delete": 0x2E,
    "insert": 0x2D,
    "home": 0x24,
    "end": 0x23,
    "pageup": 0x21,
    "pagedown": 0x22,
    "up": 0x26,
    "down": 0x28,
    "left": 0x25,
    "right": 0x27,
}
for _i in range(1, 25):
    _KEYS[f"f{_i}"] = 0x6F + _i
for _c in "abcdefghijklmnopqrstuvwxyz":
    _KEYS[_c] = ord(_c.upper())
for _c in "0123456789":
    _KEYS[_c] = ord(_c)


def parse_hotkey(spec: str) -> tuple[int, int]:
    parts = [part.strip().lower() for part in (spec or "").split("+") if part.strip()]
    if not parts:
        raise ValueError("Hotkey vacío.")
    mods = 0
    key = None
    for part in parts:
        if part in {"alt", "option"}:
            mods |= _MOD_ALT
        elif part in {"ctrl", "control"}:
            mods |= _MOD_CONTROL
        elif part == "shift":
            mods |= _MOD_SHIFT
        elif part in {"win", "windows", "super"}:
            mods |= _MOD_WIN
        elif part in _KEYS:
            key = _KEYS[part]
        else:
            raise ValueError(f"Tecla no reconocida en hotkey: {part!r}")
    if key is None:
        raise ValueError(f"Falta la tecla final en hotkey: {spec!r}")
    return mods, key


class GlobalHotkey:
    def __init__(self, spec: str, event: str, events: queue.Queue[str], hk_id: int):
        self.spec = spec
        self.event = event
        self.events = events
        self.hk_id = hk_id
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None
        self.error: str | None = None

    def start(self) -> None:
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._thread.start()

    def stop(self, wait: bool = False) -> None:
        self._stop.set()
        if wait:
            thread = self._thread
            if thread is not None and thread.is_alive():
                thread.join(timeout=1.0)

    def _run(self) -> None:
        try:
            mods, key = parse_hotkey(self.spec)
        except ValueError as exc:
            self.error = str(exc)
            return
        # Crear la cola de mensajes del hilo ANTES de registrar.
        probe = wintypes.MSG()
        _user32.PeekMessageW(ctypes.byref(probe), None, 0, 0, 0)
        if not _user32.RegisterHotKey(None, self.hk_id, mods | _MOD_NOREPEAT, key):
            self.error = f"No se pudo registrar {self.spec} (¿lo usa otra app?)"
            return
        msg = wintypes.MSG()
        try:
            while not self._stop.is_set():
                if _user32.PeekMessageW(ctypes.byref(msg), None, 0, 0, _PM_REMOVE):
                    if msg.message == _WM_HOTKEY and msg.wParam == self.hk_id:
                        self.events.put(self.event)
                else:
                    time.sleep(0.02)
        finally:
            _user32.UnregisterHotKey(None, self.hk_id)
