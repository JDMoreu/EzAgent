"""Detecta el marcado interno de «llamar a una herramienta» que algunos modelos escriben como texto.

Si no reciben el esquema de herramientas (o lo ignoran), modelos como DeepSeek escupen su
formato nativo (``<｜DSML｜ invoke name="…">``) dentro de la respuesta. Eso no es una llamada
real y nunca debe llegar a la voz ni al chat: se corta en cuanto aparece, aunque llegue partido
en varios fragmentos del streaming.
"""

from __future__ import annotations

from typing import Final

# En minúsculas: la comparación ignora mayúsculas.
_MARKERS: Final[tuple[str, ...]] = (
    "<｜dsml｜",
    "<|dsml|",
    "<dsml",
    "｜dsml｜",
    "<｜tool▁call",
    "<｜tool_call",
    "<|tool_call",
    "<tool_call>",
    "<tool_calls>",
    "<function_call",
    "<function_calls>",
    "[tool_calls]",
    "<|python_tag|>",
)
_LONGEST: Final[int] = max(len(marker) for marker in _MARKERS)

# Lo dice el código, no el modelo: el modelo no llegó a ejecutar nada.
TOOL_MARKUP_NOTICE: Final[str] = (
    "No pude ejecutar esa acción porque la herramienta no respondió. "
    "No se hizo nada; inténtalo de nuevo."
)


class ToolMarkupGuard:
    """Filtro por ronda: deja pasar el texto normal y descarta desde el primer marcador."""

    def __init__(self) -> None:
        self._held = ""
        self.tripped = False

    def feed(self, text: str) -> str:
        """Texto seguro para mostrar/hablar; guarda una cola que podría ser inicio de marcador."""
        if self.tripped or not text:
            return ""
        buffer = self._held + text
        lowered = buffer.lower()
        cut = min((i for m in _MARKERS if (i := lowered.find(m)) >= 0), default=-1)
        if cut >= 0:
            self.tripped = True
            self._held = ""
            return buffer[:cut]
        keep = 0
        for size in range(min(len(buffer), _LONGEST - 1), 0, -1):
            tail = lowered[-size:]
            if any(marker.startswith(tail) for marker in _MARKERS):
                keep = size
                break
        self._held = buffer[len(buffer) - keep :] if keep else ""
        return buffer[: len(buffer) - keep]

    def flush(self) -> str:
        """Al terminar el stream: lo retenido no era un marcador, así que se libera."""
        held, self._held = ("" if self.tripped else self._held), ""
        return held
