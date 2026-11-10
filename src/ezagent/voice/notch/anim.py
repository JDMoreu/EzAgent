"""Motor de animación del notch.

La matemática de las curvas, ``Tween``, ``LevelFollower`` y ``lerp_rgb`` está
copiada de la especificación (12.1, 12.2 y 12.7) sin cambiar su comportamiento.
Los puntos de control viven en :mod:`tokens` para que no haya valores visuales
duplicados; las funciones producen exactamente las mismas curvas CSS.

Todas las duraciones se expresan en milisegundos y los relojes en segundos.
"""

from __future__ import annotations

import ctypes
import math
import os
from typing import Callable

from .tokens import (
    BEZIER_IN,
    BEZIER_INOUT,
    BEZIER_OUT,
    BEZIER_SPRING,
    VOICE_ATTACK,
    VOICE_RELEASE,
)

Easing = Callable[[float], float]


def cubic_bezier(x1: float, y1: float, x2: float, y2: float) -> Easing:
    """Devuelve ease(x) -> y, igual que CSS cubic-bezier(x1, y1, x2, y2)."""
    cx = 3.0 * x1
    bx = 3.0 * (x2 - x1) - cx
    ax = 1.0 - cx - bx
    cy = 3.0 * y1
    by = 3.0 * (y2 - y1) - cy
    ay = 1.0 - cy - by

    def sample_x(t: float) -> float:
        return ((ax * t + bx) * t + cx) * t

    def sample_y(t: float) -> float:
        return ((ay * t + by) * t + cy) * t

    def ease(x: float) -> float:
        if x <= 0.0:
            return 0.0
        if x >= 1.0:
            return 1.0
        lo, hi = 0.0, 1.0
        for _ in range(22):
            t = (lo + hi) / 2.0
            if sample_x(t) < x:
                lo = t
            else:
                hi = t
        return sample_y((lo + hi) / 2.0)

    return ease


EASE_OUT: Easing = cubic_bezier(*BEZIER_OUT)
EASE_IN: Easing = cubic_bezier(*BEZIER_IN)
EASE_INOUT: Easing = cubic_bezier(*BEZIER_INOUT)
EASE_SPRING: Easing = cubic_bezier(*BEZIER_SPRING)


class Tween:
    """Valor animado que siempre parte del valor actual.

    Reglas:
    - ``to()`` solo se llama cuando el objetivo cambia, no en cada fotograma.
    - Se puede invertir a mitad de una animación sin saltos.
    """

    def __init__(self, value: float = 0.0) -> None:
        self.value = value
        self._v0 = value
        self._v1 = value
        self._t0 = 0.0
        self._dur = 0.0
        self._ease: Easing | None = None

    @property
    def active(self) -> bool:
        return self._ease is not None

    def to(
        self,
        target: float,
        now_s: float,
        dur_ms: float,
        ease: Easing,
    ) -> None:
        self._v0 = self.value
        self._v1 = target
        self._t0 = now_s
        self._dur = dur_ms / 1000.0
        self._ease = ease
        if self._dur <= 0.0:
            self.value = target
            self._ease = None

    def update(self, now_s: float) -> float:
        if self._ease is None:
            return self.value
        k = (now_s - self._t0) / self._dur
        if k >= 1.0:
            self.value = self._v1
            self._ease = None
        else:
            self.value = self._v0 + (self._v1 - self._v0) * self._ease(max(k, 0.0))
        return self.value


class LevelFollower:
    """Nivel de voz suavizado: sube rápido (ataque), baja suave (caída)."""

    def __init__(
        self,
        attack_ms: float = VOICE_ATTACK,
        release_ms: float = VOICE_RELEASE,
    ) -> None:
        self.attack = attack_ms
        self.release = release_ms
        self.value = 0.0

    def step(self, target: float, dt_ms: float) -> float:
        tau = self.attack if target > self.value else self.release
        self.value += (target - self.value) * (1.0 - math.exp(-dt_ms / tau))
        return self.value


def lerp_rgb(
    a: tuple[int, int, int],
    b: tuple[int, int, int],
    k: float,
) -> tuple[int, int, int]:
    """Interpola dos colores RGB y redondea cada canal."""
    return (
        round(a[0] + (b[0] - a[0]) * k),
        round(a[1] + (b[1] - a[1]) * k),
        round(a[2] + (b[2] - a[2]) * k),
    )


def animations_enabled() -> bool:
    """Movimiento del sistema permitido (SPI_GETCLIENTAREAANIMATION).

    Si la consulta falla o no estamos en Windows se asume ``True``. Todos los
    prototipos de Win32 declaran ``argtypes`` y ``restype`` para no truncar
    punteros en x64.
    """
    if os.name != "nt":
        return True
    try:
        from ctypes import wintypes

        user32 = ctypes.WinDLL("user32", use_last_error=True)
        user32.SystemParametersInfoW.argtypes = [
            wintypes.UINT,
            wintypes.UINT,
            ctypes.c_void_p,
            wintypes.UINT,
        ]
        user32.SystemParametersInfoW.restype = wintypes.BOOL
        flag = wintypes.BOOL(True)
        ok = user32.SystemParametersInfoW(0x1042, 0, ctypes.byref(flag), 0)
        return bool(flag.value) if ok else True
    except Exception:
        return True


__all__ = [
    "EASE_IN",
    "EASE_INOUT",
    "EASE_OUT",
    "EASE_SPRING",
    "Easing",
    "LevelFollower",
    "Tween",
    "animations_enabled",
    "cubic_bezier",
    "lerp_rgb",
]
