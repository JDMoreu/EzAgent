"""Atajos de teclado como campo de captura: pulsas la combinación y se ve en vivo.

Lógica pura (sin dibujo ni ventanas) para que se pueda probar: traducir teclas
de Windows (VK) a los nombres que entiende ``hotkeys.parse_hotkey``, dar el texto
que se muestra en cada tecla dibujada, decidir cuándo una combinación está
completa y detectar duplicados con los otros atajos.
"""

from __future__ import annotations

import os
from dataclasses import dataclass
from typing import Final, Literal

# Filas de ajustes que son atajos (claves de ``voice.json``).
HOTKEY_KEYS: Final[tuple[str, ...]] = (
    "hotkey",
    "cancel_hotkey",
    "repeat_hotkey",
    "mute_hotkey",
    "menu_hotkey",
)
MODIFIERS: Final[tuple[str, ...]] = ("ctrl", "alt", "shift", "win")
MOD_LABELS: Final[dict[str, str]] = {
    "ctrl": "Ctrl",
    "alt": "Alt",
    "shift": "Mayús",
    "win": "Win",
}
_ALIASES: Final[dict[str, str]] = {
    "control": "ctrl",
    "option": "alt",
    "windows": "win",
    "super": "win",
    "escape": "esc",
}
_KEY_LABELS: Final[dict[str, str]] = {
    "space": "Espacio",
    "enter": "Intro",
    "esc": "Esc",
    "tab": "Tab",
    "backspace": "Retroceso",
    "delete": "Supr",
    "insert": "Ins",
    "home": "Inicio",
    "end": "Fin",
    "pageup": "Re Pág",
    "pagedown": "Av Pág",
    "up": "Arriba",
    "down": "Abajo",
    "left": "Izquierda",
    "right": "Derecha",
}
_VK_NAMES: Final[dict[int, str]] = {
    32: "space",
    13: "enter",
    9: "tab",
    8: "backspace",
    46: "delete",
    45: "insert",
    36: "home",
    35: "end",
    33: "pageup",
    34: "pagedown",
    38: "up",
    40: "down",
    37: "left",
    39: "right",
}
# Shift, Ctrl, Alt (Menu), Win izquierda y derecha, y sus variantes por lado.
_MODIFIER_VKS: Final[frozenset[int]] = frozenset({16, 17, 18, 91, 92, 160, 161, 162, 163, 164, 165})
VK_ESCAPE: Final[int] = 27


def vk_to_key(vk: int) -> str | None:
    """Nombre de la tecla final para un código VK, o ``None`` si no vale."""
    if 65 <= vk <= 90:
        return chr(vk).lower()
    if 48 <= vk <= 57:
        return chr(vk)
    if 112 <= vk <= 135:
        return f"f{vk - 111}"
    return _VK_NAMES.get(vk)


def is_modifier_vk(vk: int) -> bool:
    return vk in _MODIFIER_VKS


def canonical(spec: str) -> tuple[frozenset[str], str] | None:
    """``(modificadores, tecla)`` de un atajo como ``ctrl+alt+space``; ``None`` si no vale."""
    mods: set[str] = set()
    key = ""
    for raw in str(spec or "").split("+"):
        part = _ALIASES.get(raw.strip().lower(), raw.strip().lower())
        if not part:
            continue
        if part in MODIFIERS:
            mods.add(part)
        elif key:
            return None
        else:
            key = part
    return (frozenset(mods), key) if key else None


def format_spec(mods: frozenset[str] | set[str], key: str) -> str:
    """Texto de ``voice.json``: modificadores en orden fijo y la tecla, en minúsculas."""
    return "+".join([m for m in MODIFIERS if m in mods] + [key])


def key_label(key: str) -> str:
    """Rótulo de una tecla final: ``Espacio``, ``F5``, ``R``…"""
    return _KEY_LABELS.get(key, key.upper())


def parts_for(spec: str) -> list[str]:
    """Rótulos de las teclas de un atajo, en el orden en que se dibujan."""
    parsed = canonical(spec)
    if parsed is None:
        return [str(spec)] if spec else []
    mods, key = parsed
    return [MOD_LABELS[m] for m in MODIFIERS if m in mods] + [key_label(key)]


def mod_labels(mods: frozenset[str] | set[str]) -> list[str]:
    return [MOD_LABELS[m] for m in MODIFIERS if m in mods]


def os_modifiers() -> frozenset[str]:
    """Modificadores pulsados ahora mismo (Windows); vacío en otros sistemas."""
    if os.name != "nt":
        return frozenset()
    try:
        import ctypes

        get = ctypes.windll.user32.GetAsyncKeyState
        held = {
            "ctrl": get(0x11),
            "alt": get(0x12),
            "shift": get(0x10),
            "win": get(0x5B) | get(0x5C),
        }
        return frozenset(name for name, state in held.items() if state & 0x8000)
    except Exception:
        return frozenset()


@dataclass(frozen=True, slots=True)
class CaptureEvent:
    """Resultado de pulsar una tecla durante la captura."""

    kind: Literal["pending", "done", "cancel", "reject"]
    spec: str = ""
    reason: str = ""


def press(vk: int, mods: frozenset[str]) -> CaptureEvent:
    """Decide qué hace una tecla mientras se captura un atajo.

    Esc sin modificadores cancela; los modificadores solos esperan a la tecla
    final; una tecla final cierra la combinación si es válida (con Ctrl, Alt o
    Win, salvo las teclas F, que valen solas).
    """
    if vk == VK_ESCAPE and not mods:
        return CaptureEvent("cancel")
    if is_modifier_vk(vk):
        return CaptureEvent("pending")
    key = vk_to_key(vk)
    if key is None:
        return CaptureEvent("reject", reason="Esa tecla no se puede usar")
    is_function = key.startswith("f") and key[1:].isdigit()
    if not is_function and not (mods - {"shift"}):
        return CaptureEvent("reject", reason="Combínala con Ctrl, Alt o Win")
    return CaptureEvent("done", spec=format_spec(mods, key))


def conflict(spec: str, current: dict[str, str], skip: str, labels: dict[str, str]) -> str | None:
    """Etiqueta del otro atajo que ya usa ``spec`` (o ``None`` si está libre)."""
    wanted = canonical(spec)
    if wanted is None:
        return None
    for key, other in current.items():
        if key != skip and canonical(other) == wanted:
            return labels.get(key, key)
    return None
