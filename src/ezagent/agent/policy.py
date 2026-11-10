"""Decisión de política: permitir, confirmar o denegar.

Copia exacta de la sección 9.2 del documento del núcleo. Es la única
puerta que decide si una herramienta puede ejecutarse.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class Decision:
    action: str  # 'allow' | 'confirm' | 'confirm_strong' | 'deny'
    level: int
    reason: str


def decide(
    tool_level: int,
    tainted: bool,
    bulk: int,
    always_confirm: bool,
    confirm_nondestructive: bool,
    bulk_threshold: int = 5,
) -> Decision:
    # always_confirm: borrado, sobrescritura, scripts y todo lo que salga
    # de write_roots.
    if tool_level >= 3:
        return Decision("deny", 3, "Nivel 3: prohibido")
    if always_confirm:
        strong = bulk > bulk_threshold
        return Decision("confirm_strong" if strong else "confirm", 2, "Operación destructiva")
    if bulk > bulk_threshold:
        return Decision("confirm_strong", 2, "Operación masiva")
    if tainted and tool_level >= 1:
        return Decision("confirm", tool_level, "Contenido no confiable en este turno")
    if tool_level == 2:
        return Decision("confirm" if confirm_nondestructive else "allow", 2, "Nivel 2")
    return Decision("allow", tool_level, "Permitido")
