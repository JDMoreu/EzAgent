"""Fixtures de `ezagent.voice.condenser` (F1-02).

El condensador solo pide un número al modelo: la línea donde empieza la
conclusión. El cliente LLM se simula (nada de red) y se registra tal cual la
petición que recibe, porque ahí está la lógica del oráculo: offset de 160
líneas, recorte a 220 caracteres y numeración.

* ``conclusion_start`` entrada ``{"lines": [str, ...], "reply": str,
  "mode": str, "provider": str}`` -> ``{"start": int | None, "requests": [...],
  "closed": bool | null}``

``mode`` reproduce el comportamiento del cliente:
``stream`` (fragmentos de 2 caracteres), ``nostream`` (rechaza ``stream=True``),
``ignore_stream`` (devuelve la respuesta completa), ``raise`` (falla al pedir),
``broken`` (corta a mitad del stream) y ``no_choices``.
"""

from __future__ import annotations

from types import SimpleNamespace
from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "condenser"


class _Stream:
    def __init__(self, pieces: list[str], broken: bool = False) -> None:
        self.pieces = pieces
        self.broken = broken
        self.closed = False

    def __iter__(self) -> Any:
        for index, piece in enumerate(self.pieces):
            if self.broken and index == 1:
                raise RuntimeError("stream cortado")
            yield SimpleNamespace(choices=[SimpleNamespace(delta=SimpleNamespace(content=piece))])
        if self.broken:
            raise RuntimeError("stream cortado")

    def close(self) -> None:
        self.closed = True


class _Completions:
    def __init__(self, reply: str, mode: str, chunk_size: int = 2) -> None:
        self.reply = reply
        self.mode = mode
        self.chunk_size = chunk_size
        self.requests: list[dict[str, Any]] = []
        self.stream: _Stream | None = None

    def create(self, **request: Any) -> Any:
        self.requests.append(dict(request))
        if self.mode == "raise":
            raise RuntimeError("el modelo no responde")
        if self.mode == "nostream":
            if "stream" in request:
                raise TypeError("create() got an unexpected keyword argument 'stream'")
            return SimpleNamespace(
                choices=[SimpleNamespace(message=SimpleNamespace(content=self.reply))]
            )
        if self.mode == "ignore_stream":
            return SimpleNamespace(
                choices=[SimpleNamespace(message=SimpleNamespace(content=self.reply))]
            )
        pieces = [
            self.reply[index : index + self.chunk_size]
            for index in range(0, len(self.reply), self.chunk_size)
        ]
        self.stream = _Stream([] if self.mode == "no_choices" else pieces)
        return self.stream


def _cases() -> list[dict[str, Any]]:
    lines19 = [f"línea {number}" for number in range(1, 20)]
    lines200 = [f"línea {number}" for number in range(1, 201)]
    return [
        {"lines": lines19, "reply": "12"},
        {"lines": [f"línea {number}" for number in range(1, 10)], "reply": "3"},
        {"lines": lines19, "reply": "1"},
        {"lines": lines19, "reply": "19"},
        {"lines": lines19, "reply": "0"},
        {"lines": lines19, "reply": "20"},
        {"lines": lines19, "reply": "-3"},
        {"lines": lines19, "reply": "línea 5 de 19"},
        {"lines": lines19, "reply": "  7  "},
        {"lines": lines19, "reply": "12\n"},
        {"lines": lines19, "reply": "00012"},
        {"lines": lines19, "reply": "1e3"},
        {"lines": lines19, "reply": "٣"},
        {"lines": lines19, "reply": ""},
        {"lines": lines19, "reply": "no hay conclusión"},
        {"lines": lines19, "reply": "**5**"},
        {"lines": [], "reply": "0"},
        {"lines": [], "reply": "1"},
        {"lines": ["única"], "reply": "1"},
        {"lines": [""], "reply": "1"},
        {"lines": ["", ""], "reply": "2"},
        {"lines": [f"línea {n}" for n in range(1, 161)], "reply": "1"},
        {"lines": [f"línea {n}" for n in range(1, 162)], "reply": "1"},
        {"lines": [f"línea {n}" for n in range(1, 162)], "reply": "160"},
        {"lines": lines200, "reply": "160"},
        {"lines": lines200, "reply": "161"},
        {"lines": lines200, "reply": "1"},
        {"lines": ["x" * 500, "corta"], "reply": "2"},
        {"lines": ["ñandú ☕ " + "á" * 300, "中文 conclusión"], "reply": "2"},
        {"lines": ["\tcon tabulador", "  con espacios  "], "reply": "1"},
        {"lines": lines19, "reply": "12", "provider": "openai"},
        {"lines": lines19, "reply": "12", "provider": "opencode"},
        {"lines": lines19, "reply": "12", "provider": "anthropic"},
        {"lines": lines19, "reply": "12", "provider": ""},
        {"lines": lines19, "reply": "3", "mode": "nostream"},
        {"lines": lines19, "reply": "4", "mode": "ignore_stream"},
        {"lines": lines19, "reply": "5", "mode": "raise"},
        {"lines": lines19, "reply": "123", "mode": "broken"},
        {"lines": lines19, "reply": "6", "mode": "no_choices"},
        {"lines": lines19, "reply": "8", "chunk_size": 1},
    ]


def _run(case: dict[str, Any]) -> dict[str, Any]:
    from ezagent.voice.condenser import Condenser

    completions = _Completions(
        case.get("reply", ""), case.get("mode", "stream"), case.get("chunk_size", 2)
    )
    client = SimpleNamespace(chat=SimpleNamespace(completions=completions))
    cfg = SimpleNamespace(
        llm_model=case.get("model", "modelo-baratito"),
        llm_provider=case.get("provider", "openai"),
    )
    start = Condenser(client, cfg).conclusion_start(case["lines"])
    stream = completions.stream
    return {
        "start": start,
        "requests": completions.requests,
        "closed": None if stream is None else stream.closed,
    }


def generate(out: FixtureFile) -> None:
    for index, case in enumerate(_cases(), start=1):
        payload = {
            "lines": case["lines"],
            "reply": case.get("reply", ""),
            "mode": case.get("mode", "stream"),
            "provider": case.get("provider", "openai"),
        }
        out.add(f"condenser/conclusion_start/{index:03d}", "conclusion_start", payload, _run(case))

    try:
        _run({"lines": [None], "reply": "1"})
    except TypeError as error:
        out.add_error(
            "condenser/conclusion_start/error01",
            "conclusion_start",
            {"lines": [None], "reply": "1", "mode": "stream", "provider": "openai"},
            str(error),
        )


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
