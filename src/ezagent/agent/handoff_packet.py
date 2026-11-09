"""Qué se pasa al siguiente agente: respuesta, conclusión o índice + archivo.

Pasar siempre la salida completa de un agente satura el contexto del siguiente:
un agente que editó código deja un registro largo (comandos, diffs, vistas de
archivos) y **cierra con una conclusión** («creé X en la ruta Y, sirve para Z»).
Eso último es lo que hace falta. Reglas, sin reescribir nunca el texto:

- **short**: una respuesta corta sin trabajo de herramientas se pasa tal cual.
- **report**: hubo trabajo (``Ran``, ``Wrote``, ``$ comando``...). Se pasa solo
  la conclusión; el registro completo se guarda en un ``.md`` y se deja su ruta
  por si el siguiente agente lo necesita.
- **document**: texto largo sin herramientas (un plan). Se pasa un índice breve
  y la ruta del texto completo.

La conclusión se localiza con reglas por agente y, si se le da un ``select``,
con un modelo pequeño que **solo devuelve un número de línea** (nunca texto):
lo peor que puede hacer un texto malicioso es elegir otra línea de inicio.
"""

from __future__ import annotations

import re
from collections.abc import Callable
from dataclasses import dataclass

SHORT_CHARS = 2500
DOCUMENT_FILE_CHARS = 6000
MIN_LOG_CHARS = 400
MAX_FALLBACK_CHARS = 1500
_SPOKEN_CHARS = 360
_OUTLINE_LINES = 24

# Marcas de trabajo de herramientas, verificadas con capturas reales.
_STRONG = re.compile(
    r"""^(?:
        Ran\s\d+\s                                            # Claude: «Ran 1 shell command»
      | [•×●]\s(?:Ran|Read|Wrote|Write|Edit|Edited|Added|Updated|Deleted
                 |Explored|Searched|Listed|Thought|Create|Created)\b  # Codex / mcode
      | \#\s(?:Wrote|Read|Edit|Created|Deleted|Patched)\b      # OpenCode
      | [←→↳]\s                                                # OpenCode
      | \+\s(?:Show\sdetails|Thought)\b
      | Explored:
    )""",
    re.VERBOSE,
)
# Marcas débiles: solo cuentan para ubicar el final del registro si ya hay una
# fuerte (en un plan pueden aparecer «$ uv sync» o listas numeradas).
_WEAK = re.compile(r"^(?:\$\s\S|\d{1,4}\s{1,2}\S|[⎿└]\s)")
_BULLET = re.compile(r"^[●•⏺]\s+")
_HEADING = re.compile(
    r"^(?:#{1,4}\s+\S.{0,80}|\d{1,2}[.)]\s+\S.{0,80}|[A-ZÁÉÍÓÚÑ][^.!?:]{3,80}:?)$"
)


@dataclass
class Packet:
    kind: str  # short | report | document
    text: str  # lo que se inserta en la tarea del siguiente agente
    spoken: str  # lo que se diría por voz
    full: str  # texto completo original
    saved_path: str = ""


def has_work(text: str) -> bool:
    return any(_STRONG.match(line.strip()) for line in text.splitlines())


def _after_last_marker(lines: list[str]) -> int:
    """Índice de la primera línea posterior al último registro de herramientas."""
    last = -1
    for index, raw in enumerate(lines):
        text = raw.strip()
        if _STRONG.match(text) or (last >= 0 and _WEAK.match(text)):
            last = index
    return last + 1


def outline(text: str, limit: int = _OUTLINE_LINES) -> str:
    """Índice breve: primer párrafo + títulos reconocibles (sin inventar nada)."""
    lines = [ln.strip() for ln in text.splitlines()]
    first = next((ln for ln in lines if ln), "")
    headings = [
        ln
        for ln in lines
        if ln and len(ln) <= 90 and not ln.endswith(".") and _HEADING.match(ln)
    ]
    seen: set[str] = set()
    unique = [h for h in headings if not (h in seen or seen.add(h))]
    parts = [first[:300]] if first else []
    if unique:
        parts.append("Contenido:\n" + "\n".join(f"- {h}" for h in unique[:limit]))
    return "\n".join(parts)


def _first_chars(text: str, limit: int = _SPOKEN_CHARS) -> str:
    flat = " ".join(text.split())
    return flat if len(flat) <= limit else flat[: limit - 1].rstrip() + "…"


def build_packet(
    answer: str,
    *,
    select: Callable[[list[str]], int | None] | None = None,
    save: Callable[[str], str | None] | None = None,
) -> Packet:
    """Decide qué parte de la salida de un agente pasa al siguiente."""
    answer = str(answer or "").strip()
    if not answer:
        return Packet("short", "", "", "")
    lines = answer.splitlines()

    if not has_work(answer):
        if len(answer) <= SHORT_CHARS:
            return Packet("short", answer, _first_chars(answer), answer)
        # Texto largo sin herramientas (un plan): hasta ~6000 caracteres se
        # pasa entero; por encima, índice breve + archivo con el texto completo.
        path = save(answer) if save and len(answer) > DOCUMENT_FILE_CHARS else None
        if path:
            digest = outline(answer)
            return Packet("document", digest, _first_chars(digest), answer, path)
        return Packet("document", answer, _first_chars(answer), answer)

    start = None
    if select is not None and len(lines) > 12:
        try:
            start = select(lines)
        except Exception:  # noqa: BLE001 - el modelo es opcional
            start = None
    by_model = isinstance(start, int) and 0 < start <= len(lines)
    if not by_model:
        start = _after_last_marker(lines) + 1
    conclusion = "\n".join(lines[start - 1 :]).strip()
    if not conclusion:
        conclusion = "\n".join(lines[-12:]).strip()
    if not by_model and len(conclusion) > MAX_FALLBACK_CHARS:
        # Tras el último comando puede haber mucha salida pegada a la
        # conclusión: sin el modelo se queda solo el final, por líneas enteras.
        tail = conclusion[-MAX_FALLBACK_CHARS:]
        conclusion = tail[tail.find("\n") + 1 :] if "\n" in tail else tail
    conclusion = _BULLET.sub("", conclusion, count=1)
    path = ""
    if save and len(answer) - len(conclusion) > MIN_LOG_CHARS:
        path = save(answer) or ""
    return Packet("report", conclusion, _first_chars(conclusion), answer, path)


def packet_text(packet: Packet) -> str:
    """Texto que se inserta en la tarea, con la ruta del archivo si la hay."""
    if not packet.saved_path:
        return packet.text
    if packet.kind == "document":
        note = f"[Texto completo, léelo antes de empezar: {packet.saved_path}]"
    else:
        note = (
            "[Registro completo del trabajo, solo si lo necesitas: "
            f"{packet.saved_path}]"
        )
    return f"{packet.text}\n\n{note}"
