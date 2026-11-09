"""Traspaso de resultados entre agentes: solo la respuesta, sin la pantalla.

Cuando un agente termina y su resultado alimenta a otro, lo que hay que pasar
es lo que habría copiado una persona: la respuesta. No la cabecera del
programa, ni el eco de la pregunta, ni la barra de estado. Las pantallas de
OpenCode, Claude Code, Codex y mcode comparten un patrón (verificado con
capturas reales en ``tests/fixtures/screens``): el mensaje enviado aparece
repetido entre ``<hc from="..">`` y ``</hc>``, la respuesta empieza justo
después y termina en una línea de pie (``Worked for``, ``Completed in``,
``Build · modelo``, ``✻ ... for 3s``).

La salida de un agente es un dato no confiable (AGENTS.md §5): aquí solo se
recorta y se entrega; nunca se interpreta.
"""

from __future__ import annotations

import re

MAX_ANSWER_CHARS = 60_000
_TAIL_LINES = 30

_ECHO_END = "</hc>"
_BULLET_START = re.compile(r"^[●•⏺]\s+")
_BULLET_LINE = re.compile(r"^\s*[●•⏺]\s+")
_FOOTER = re.compile(
    r"""^(?:
        [╹╰╯╭╮┌┐└┘├┤─━│┃▀▄█]{2,}              # marcos y bordes de la interfaz
      | (?:└\s)?(?:Worked\ for|Completed\ in)\b   # Codex / mcode
      | [✻✽✶✢✳]\s.*\bfor\s\d                  # Claude: «✻ Sautéed for 3s»
      | [›❯]\s?(?:Ask\b|$)                    # caja de entrada vacía
      | ⏵ | Message\s· | Tip: | \[hc\] | \?\sfor\sshortcuts | esc\sto\s
      | Suggested\snext\smessage | .*\s↗$      # sugerencias y chips de la interfaz
      | (?:Build|Plan|Default|Ask)\s·\s        # OpenCode: «Build · modelo · 2.4s»
      | .*\s·\s\d+(?:\.\d+)?\s?(?:s|ms|m)\b    # «... · 2.4s ...»
      | GPT-[\w.]+\s(?:low|medium|high|xhigh|max|ultra|default)\b
    )""",
    re.VERBOSE,
)
_THINKING = re.compile(r"^(?:└\s*)?(?:[•●+▸▾]\s*)?(?:Thought|Thinking)\b")


def _lines(screen: str) -> list[str]:
    return [line.rstrip() for line in str(screen or "").splitlines()]


def _tail_line(lines: list[str], tail: str) -> int | None:
    """Línea donde termina ``tail``, aunque la pantalla lo parta en varias."""
    joined = ""
    ends: list[int] = []
    squeezed = [" ".join(line.split()) for line in lines]
    for text in squeezed:
        if text:
            joined = f"{joined} {text}" if joined else text
        ends.append(len(joined))
    position = joined.rfind(tail)
    if position < 0:
        return None
    end = position + len(tail)
    for index, line_end in enumerate(ends):
        if line_end >= end and squeezed[index]:
            return index
    return None


def _anchor(lines: list[str], sent_task: str) -> int | None:
    """Índice de la línea donde termina el eco del mensaje enviado."""
    # 1) Si el programa marca el origen (<hc from=..>), el cierre es exacto.
    for index in range(len(lines) - 1, -1, -1):
        if _ECHO_END in lines[index]:
            return index
    # 2) El texto que EzAgent pegó: su final, aunque se haya partido en líneas.
    tail = " ".join(str(sent_task or "").split())[-60:]
    if len(tail) >= 8:
        found = _tail_line(lines, tail)
        if found is not None:
            return found
    # Sin eco reconocible: la última viñeta de respuesta marca el inicio.
    for index in range(len(lines) - 1, -1, -1):
        if _BULLET_LINE.match(lines[index]):
            return index - 1
    return None


def extract_answer(screen: str, sent_task: str = "") -> str:
    """Devuelve la última respuesta del agente, o '' si no la reconoce."""
    lines = _lines(screen)
    start = _anchor(lines, sent_task)
    if start is None:
        return ""
    collected: list[str] = []
    for raw in lines[start + 1 :]:
        text = raw.strip()
        if not text:
            if collected and collected[-1]:
                collected.append("")
            continue
        if _THINKING.match(text):
            continue
        if _FOOTER.match(text):
            break
        collected.append(text)
    while collected and not collected[-1]:
        collected.pop()
    if collected:
        collected[0] = _BULLET_START.sub("", collected[0])
    answer = "\n".join(collected).strip()
    return answer[:MAX_ANSWER_CHARS]


def clean_tail(screen: str) -> str:
    """Respaldo: las últimas líneas útiles de la pantalla, sin marcos ni pie."""
    kept = [
        line.strip()
        for line in _lines(screen)
        if line.strip()
        and not _FOOTER.match(line.strip())
        and not _THINKING.match(line.strip())
    ]
    return "\n".join(kept[-_TAIL_LINES:])[-MAX_ANSWER_CHARS:]


def answer_or_tail(screen: str, sent_task: str = "") -> str:
    """La respuesta limpia si se reconoce; si no, la cola de la pantalla."""
    return extract_answer(screen, sent_task) or clean_tail(screen)


def compose_handoff(task: str, answer: str) -> str:
    """Instrucción + resultado previo, como si se copiara y pegara.

    Con ``{output}`` en la tarea, la respuesta va en ese lugar; sin él, se
    añade debajo entre comillas triples para delimitarla como dato.
    """
    task = str(task or "").strip()
    answer = str(answer or "").strip()
    if not answer:
        return task
    if "{output}" in task:
        return task.replace("{output}", answer)
    return f'{task}\n\n"""\n{answer}\n"""' if task else answer
