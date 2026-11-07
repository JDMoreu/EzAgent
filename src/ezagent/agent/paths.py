"""Validación de rutas antes de tocar el disco.

Implementación de referencia de la sección 9.1 del documento del núcleo.
Toda herramienta resuelve, valida y opera sobre la ruta devuelta por
``resolve_safe`` (nunca sobre la cadena original).
"""

from __future__ import annotations

import fnmatch
import os
from collections.abc import Sequence
from pathlib import Path

BS = chr(92)  # barra invertida, para evitar escapes en el código
RESERVED = {"CON", "PRN", "AUX", "NUL"} | {f"COM{i}" for i in range(1, 10)} | {
    f"LPT{i}" for i in range(1, 10)
}


class PathDenied(Exception):
    """La ruta no cumple la política de acceso."""


def _expand(value: str) -> Path:
    return Path(os.path.expandvars(os.path.expanduser(value)))


def resolve_safe(
    raw: str,
    base: Path,
    roots: Sequence[str],
    deny: Sequence[str],
    deny_globs: Sequence[str],
) -> Path:
    """Normaliza y valida ``raw``; devuelve la ruta resuelta o lanza PathDenied."""
    if not raw or "\x00" in raw:
        raise PathDenied("ruta vacía o inválida")
    s = raw.strip()
    if s.startswith(BS * 2) or s.startswith("//"):
        raise PathDenied("rutas UNC o de dispositivo no permitidas")
    if ":" in s[2:]:
        raise PathDenied("flujos de datos alternativos no permitidos")
    p = _expand(s)
    if not p.is_absolute():
        p = base / p  # DECISION: las rutas relativas se resuelven contra el workspace
    try:
        rp = p.resolve(strict=False)
    except (OSError, RuntimeError) as exc:
        raise PathDenied(f"no se pudo resolver la ruta: {exc}") from exc
    for part in rp.parts:
        if part.split(".")[0].upper() in RESERVED:
            raise PathDenied("nombre reservado de Windows")
    allowed = [_expand(root).resolve() for root in roots]
    if not any(rp == root or rp.is_relative_to(root) for root in allowed):
        raise PathDenied("fuera de las carpetas permitidas")
    for denied in deny:
        denied_path = _expand(denied).resolve()
        if rp == denied_path or rp.is_relative_to(denied_path):
            raise PathDenied("carpeta protegida")
    posix = rp.as_posix()
    for pattern in deny_globs:
        if fnmatch.fnmatch(posix, pattern) or fnmatch.fnmatch(
            rp.name, pattern.split("/")[-1]
        ):
            raise PathDenied("archivo protegido")  # DECISION: fnmatch simple
    if rp == Path(rp.anchor):
        raise PathDenied("raíz de unidad no permitida")
    return rp
