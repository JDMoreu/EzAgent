"""Fixtures del parseo SSE y de los errores del proveedor LLM (F1-05).

Cubre los dos formatos que usa el `Messenger`:

* OpenAI Chat Completions (cliente real del SDK contra un `http.server` local),
* Anthropic Messages (`iter_sse` y `translate_events` directos, y el cliente
  real contra el mismo servidor).

También fija `friendly_llm_error` con las excepciones y cabeceras que el SDK
produce de verdad (429 con `retry-after`, 401/403/404/413/500, red y tiempo).

Formato de cada caso:

* ``iter_sse``         entrada ``{"lines": [...]}`` -> ``{"events": [...]}``
* ``translate_events`` entrada ``{"events": [...]}`` -> ``{"chunks": [...], "text": str}``
* ``openai_stream``    entrada ``{"raw": str, "delivery": str, ...}`` -> trozos y texto
* ``anthropic_stream`` igual, con la API Messages
* ``friendly_llm_error`` entrada ``{"class", "message", "label", "model", ...}`` -> ``{"text"}``
"""

from __future__ import annotations

import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from types import SimpleNamespace
from typing import Any

from _common import FixtureFile, main_guard

from ezagent.voice.anthropic_client import (
    AnthropicChatClient,
    iter_sse,
    translate_events,
)
from ezagent.voice.llm import LLMTurnTimeout, friendly_llm_error

DOMAIN = "llm_sse"

_MISSING = object()


class _State:
    """Respuesta cruda que sirve el servidor local en el caso actual."""

    # DECISION: `delivery` (entera, por líneas o byte a byte) y `cut_at`
    # modelan los cortes en mitad de línea y de evento sin tocar el Python.
    body = b""
    status = 200
    headers: dict[str, str] = {}
    delivery = "whole"
    cut_at: int | None = None


class _Handler(BaseHTTPRequestHandler):
    def do_POST(self) -> None:
        length = int(self.headers.get("content-length") or 0)
        self.rfile.read(length)
        body = _State.body if _State.cut_at is None else _State.body[: _State.cut_at]
        self.send_response(_State.status)
        for name, value in _State.headers.items():
            self.send_header(name, value)
        if _State.status >= 400:
            self.send_header("content-length", str(len(body)))
        else:
            self.send_header("content-type", "text/event-stream")
        self.end_headers()
        try:
            if _State.status >= 400 or _State.delivery == "whole":
                self.wfile.write(body)
                self.wfile.flush()
            elif _State.delivery == "byte":
                for byte in body:
                    self.wfile.write(bytes((byte,)))
                    self.wfile.flush()
            else:
                for line in body.splitlines(keepends=True):
                    self.wfile.write(line)
                    self.wfile.flush()
        except OSError:
            pass

    def log_message(self, *args: Any) -> None:
        pass


def _event(event: dict[str, Any]) -> str:
    return f"event: {event.get('type')}\ndata: {json.dumps(event)}\n\n"


ANTHROPIC_TEXT: list[dict[str, Any]] = [
    {"type": "message_start", "message": {"id": "msg_1"}},
    {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}},
    {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Hola"}},
    {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": " mundo"}},
    {"type": "content_block_stop", "index": 0},
    {"type": "message_delta", "delta": {"stop_reason": "end_turn"}},
    {"type": "message_stop"},
]

ANTHROPIC_TOOL: list[dict[str, Any]] = [
    {"type": "message_start", "message": {"id": "msg_2"}},
    {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": "Va."}},
    {
        "type": "content_block_start",
        "index": 1,
        "content_block": {"type": "tool_use", "id": "tu1", "name": "write_file"},
    },
    {
        "type": "content_block_delta",
        "index": 1,
        "delta": {"type": "input_json_delta", "partial_json": '{"path": "a'},
    },
    {
        "type": "content_block_delta",
        "index": 1,
        "delta": {"type": "input_json_delta", "partial_json": '.txt"}'},
    },
    {"type": "content_block_stop", "index": 1},
    {"type": "message_delta", "delta": {"stop_reason": "tool_use"}},
    {"type": "message_stop"},
]


def _openai_line(
    content: str | None = None,
    *,
    tool_calls: list[dict[str, Any]] | None = None,
    finish_reason: str | None = None,
    choices: bool = True,
    usage: dict[str, Any] | None = None,
    **extra: Any,
) -> str:
    delta: dict[str, Any] = {}
    if content is not None:
        delta["content"] = content
    if tool_calls is not None:
        delta["tool_calls"] = tool_calls
    payload: dict[str, Any] = {
        "id": "chatcmpl-local",
        "object": "chat.completion.chunk",
        "created": 0,
        "model": "m",
        **extra,
    }
    if choices:
        payload["choices"] = [{"index": 0, "delta": delta, "finish_reason": finish_reason}]
    else:
        payload["choices"] = []
    if usage is not None:
        payload["usage"] = usage
    return f"data: {json.dumps(payload)}\n\n"


OPENAI_STREAM_CASES: list[dict[str, Any]] = [
    {
        "id": "texto_simple",
        "raw": _openai_line("Hola, ") + _openai_line("¿qué tal?") + "data: [DONE]\n\n",
    },
    {
        "id": "texto_por_bytes",
        "raw": _openai_line("Hola, ") + _openai_line("¿qué tal?") + "data: [DONE]\n\n",
        "delivery": "byte",
    },
    {
        "id": "texto_por_lineas",
        "raw": _openai_line("Hola, ") + _openai_line("¿qué tal?") + "data: [DONE]\n\n",
        "delivery": "line",
    },
    {
        "id": "finish_length",
        "raw": _openai_line("cortado", finish_reason="length") + "data: [DONE]\n\n",
    },
    {
        "id": "herramienta_en_trozos",
        "raw": _openai_line(
            tool_calls=[
                {
                    "index": 0,
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": "write_", "arguments": ""},
                }
            ]
        )
        + _openai_line(
            tool_calls=[{"index": 0, "function": {"name": "file", "arguments": '{"path": "a'}}]
        )
        + _openai_line(
            tool_calls=[{"index": 0, "function": {"arguments": '.txt"}'}}],
            finish_reason="tool_calls",
        )
        + "data: [DONE]\n\n",
    },
    {
        "id": "varias_herramientas_desordenadas",
        "raw": _openai_line(
            tool_calls=[
                {"index": 1, "id": "b", "function": {"name": "beta", "arguments": "{}"}},
                {"index": 0, "id": "a", "function": {"name": "alfa", "arguments": "{}"}},
            ]
        )
        + "data: [DONE]\n\n",
    },
    {
        "id": "chunk_sin_choices",
        "raw": _openai_line(choices=False) + _openai_line("después") + "data: [DONE]\n\n",
    },
    {
        "id": "delta_vacio",
        "raw": _openai_line() + _openai_line("texto") + "data: [DONE]\n\n",
    },
    {
        "id": "usage_al_final",
        "raw": _openai_line("uno")
        + _openai_line(
            choices=False, usage={"prompt_tokens": 3, "completion_tokens": 1, "total_tokens": 4}
        )
        + "data: [DONE]\n\n",
    },
    {
        "id": "sin_done",
        "raw": _openai_line("sin marcador"),
    },
    {
        "id": "corte_a_mitad_de_evento",
        "raw": _openai_line("uno") + _openai_line("dos"),
        "cut_at": len(_openai_line("uno")) + 20,
    },
    {
        "id": "corte_en_el_primer_json",
        "raw": _openai_line("uno"),
        "cut_at": 20,
    },
    {
        "id": "comentarios_y_eventos",
        "raw": (
            ": ping\n"
            "event: message\n"
            + _openai_line("a")
            + _openai_line("b").replace("data: ", "data:", 1)
            + "data: [DONE]\n\n"
        ),
    },
    {
        "id": "data_multilinea",
        "raw": (
            'data: {"id": "x", "object": "chat.completion.chunk", "created": 0,\n'
            'data: "model": "m", "choices": [{"index": 0, "delta": {"content": "junto"}, "finish_reason": null}]}\n\n'
            "data: [DONE]\n\n"
        ),
    },
    {
        "id": "solo_done",
        "raw": "data: [DONE]\n\n",
    },
    {
        "id": "campos_extra",
        "raw": _openai_line("extra", system_fingerprint="fp_1", service_tier="default")
        + "data: [DONE]\n\n",
    },
    {
        "id": "unicode",
        "raw": _openai_line("ñandú ☕ 🙂") + _openai_line("áéíóú") + "data: [DONE]\n\n",
    },
    {
        "id": "crlf",
        "raw": (_openai_line("crlf") + "data: [DONE]\n\n").replace("\n", "\r\n"),
    },
    {
        "id": "lineas_en_blanco",
        "raw": "\n\n" + _openai_line("blanco") + "\n\n\n" + "data: [DONE]\n\n",
    },
    {
        "id": "error_en_mitad",
        "raw": _openai_line("antes")
        + "data: "
        + json.dumps({"error": {"message": "boom", "type": "server_error"}})
        + "\n\n",
    },
    {
        "id": "error_con_codigo",
        "raw": "data: " + json.dumps({"error": {"message": "bad", "code": 429}}) + "\n\n",
    },
    {
        "id": "json_roto",
        "raw": "data: {oops\n\n",
    },
    {
        "id": "http_401",
        "raw": '{"error": "malo"}',
        "status": 401,
        "headers": {"content-type": "application/json"},
    },
    {
        "id": "http_429_retry_after",
        "raw": '{"error": "lento"}',
        "status": 429,
        "headers": {"content-type": "application/json", "retry-after": "42"},
    },
]


ANTHROPIC_STREAM_CASES: list[dict[str, Any]] = [
    {"id": "texto_simple", "raw": "".join(_event(event) for event in ANTHROPIC_TEXT)},
    {
        "id": "texto_por_bytes",
        "raw": "".join(_event(event) for event in ANTHROPIC_TEXT),
        "delivery": "byte",
    },
    {
        "id": "texto_por_lineas",
        "raw": "".join(_event(event) for event in ANTHROPIC_TEXT),
        "delivery": "line",
    },
    {"id": "herramienta_en_trozos", "raw": "".join(_event(event) for event in ANTHROPIC_TOOL)},
    {
        "id": "herramienta_sin_argumentos",
        "raw": _event(
            {
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "tool_use", "id": "t", "name": "get_datetime"},
            }
        )
        + _event({"type": "content_block_stop", "index": 0})
        + _event({"type": "message_stop"}),
    },
    {
        "id": "varias_herramientas",
        "raw": "".join(
            _event(event)
            for event in (
                {
                    "type": "content_block_start",
                    "index": 2,
                    "content_block": {"type": "tool_use", "id": "a", "name": "f"},
                },
                {
                    "type": "content_block_delta",
                    "index": 2,
                    "delta": {"type": "input_json_delta", "partial_json": "{}"},
                },
                {
                    "type": "content_block_start",
                    "index": 5,
                    "content_block": {"type": "tool_use", "id": "b", "name": "g"},
                },
                {
                    "type": "content_block_delta",
                    "index": 5,
                    "delta": {"type": "input_json_delta", "partial_json": "{}"},
                },
                {"type": "message_delta", "delta": {"stop_reason": "tool_use"}},
                {"type": "message_stop"},
            )
        ),
    },
    {
        "id": "max_tokens",
        "raw": _event(
            {
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "x"},
            }
        )
        + _event({"type": "message_delta", "delta": {"stop_reason": "max_tokens"}}),
    },
    {
        "id": "stop_desconocido",
        "raw": _event({"type": "message_delta", "delta": {"stop_reason": "weird"}}),
    },
    {
        "id": "solo_usage",
        "raw": _event({"type": "message_delta", "usage": {"input_tokens": 1, "output_tokens": 2}}),
    },
    {
        "id": "message_stop_corta",
        "raw": _event({"type": "message_stop"})
        + _event(
            {
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "NO"},
            }
        ),
    },
    {
        "id": "error_de_anthropic",
        "raw": _event(
            {"type": "error", "error": {"type": "overloaded_error", "message": "Sobrecargado"}}
        ),
    },
    {"id": "error_sin_tipo", "raw": _event({"type": "error", "error": {"message": "sin tipo"}})},
    {"id": "error_sin_cuerpo", "raw": _event({"type": "error"})},
    {
        "id": "texto_al_abrir_bloque",
        "raw": _event(
            {
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "text", "text": "inicio"},
            }
        ),
    },
    {
        "id": "json_delta_de_bloque_desconocido",
        "raw": _event(
            {
                "type": "content_block_delta",
                "index": 9,
                "delta": {"type": "input_json_delta", "partial_json": "{}"},
            }
        ),
    },
    {
        "id": "stop_de_bloque_desconocido",
        "raw": _event({"type": "content_block_stop", "index": 9}),
    },
    {
        "id": "eventos_desconocidos",
        "raw": _event({"type": "ping"})
        + _event(
            {"type": "content_block_delta", "index": 0, "delta": {"type": "otra", "text": "no"}}
        )
        + _event({"type": 5}),
    },
    {
        "id": "data_multilinea",
        "raw": 'event: content_block_delta\ndata: {"type": "content_block_delta", "index": 0,\n'
        'data: "delta": {"type": "text_delta", "text": "junto"}}\n\n',
    },
    {
        "id": "varios_eventos_sin_linea_en_blanco",
        "raw": 'data: {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "a"}}\n'
        'data: {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "b"}}\n',
    },
    {
        "id": "corte_a_mitad_de_evento",
        "raw": "".join(_event(event) for event in ANTHROPIC_TEXT),
        "cut_at": 60,
    },
    {
        "id": "inyeccion_unicode",
        "raw": _event(
            {
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "ñ ☕"},
            }
        ),
    },
    {
        "id": "http_500",
        "raw": '{"error": "malo"}',
        "status": 500,
        "headers": {"content-type": "application/json"},
    },
    {
        "id": "http_401_cuerpo_largo",
        "raw": "x" * 400,
        "status": 401,
        "headers": {"content-type": "text/plain"},
    },
]


def _chunk_record(chunk: Any) -> dict[str, Any]:
    choices = []
    for choice in chunk.choices:
        delta = choice.delta
        calls = []
        for call in delta.tool_calls or []:
            calls.append(
                {
                    "index": int(call.index or 0),
                    "id": call.id,
                    "name": call.function.name,
                    "arguments": call.function.arguments,
                }
            )
        choices.append(
            {
                "content": delta.content,
                "finish_reason": choice.finish_reason,
                "tool_calls": calls,
            }
        )
    record: dict[str, Any] = {"choices": choices}
    usage = getattr(chunk, "usage", None)
    if usage is not None:
        record["usage"] = (
            usage.model_dump(exclude_none=True) if hasattr(usage, "model_dump") else usage
        )
    return record


def _accumulate(records: list[dict[str, Any]]) -> dict[str, Any]:
    text: list[str] = []
    finish: list[str] = []
    calls: dict[int, dict[str, Any]] = {}
    for record in records:
        for choice in record["choices"]:
            if choice["content"]:
                text.append(choice["content"])
            if choice["finish_reason"]:
                finish.append(choice["finish_reason"])
            for call in choice["tool_calls"]:
                item = calls.setdefault(
                    call["index"], {"index": call["index"], "id": "", "name": "", "arguments": ""}
                )
                if call["id"]:
                    item["id"] = call["id"]
                item["name"] += call["name"] or ""
                item["arguments"] += call["arguments"] or ""
    return {
        "text": "".join(text),
        "finish_reasons": finish,
        "final_finish_reason": finish[-1] if finish else None,
        "tool_calls": [calls[index] for index in sorted(calls)],
    }


def _collect(stream: Any) -> dict[str, Any]:
    records = [None if chunk is None else _chunk_record(chunk) for chunk in stream]
    present = [record for record in records if record is not None]
    return {"chunks": records, **_accumulate(present)}


def _input_of(case: dict[str, Any]) -> dict[str, Any]:
    payload = {key: case[key] for key in ("raw", "cut_at", "status", "headers") if key in case}
    payload["delivery"] = case.get("delivery", "whole")
    return payload


def _run_openai(case: dict[str, Any], port: int) -> dict[str, Any]:
    from openai import OpenAI

    _State.body = str(case["raw"]).encode()
    _State.status = int(case.get("status", 200))
    _State.headers = dict(case.get("headers") or {})
    _State.delivery = str(case.get("delivery", "whole"))
    _State.cut_at = case.get("cut_at")
    client = OpenAI(
        api_key="clave-falsa",
        base_url=f"http://127.0.0.1:{port}/v1",
        max_retries=0,
    )
    stream = client.chat.completions.create(
        model="m",
        messages=[{"role": "user", "content": "x"}],
        stream=True,
        max_tokens=10,
    )
    try:
        return _collect(stream)
    finally:
        stream.close()


def _run_anthropic(case: dict[str, Any], port: int) -> dict[str, Any]:
    _State.body = str(case["raw"]).encode()
    _State.status = int(case.get("status", 200))
    _State.headers = dict(case.get("headers") or {})
    _State.delivery = str(case.get("delivery", "whole"))
    _State.cut_at = case.get("cut_at")
    client = AnthropicChatClient(
        api_key="clave-falsa",
        base_url=f"http://127.0.0.1:{port}",
        default_headers={"User-Agent": "EzAgent-Voice/0.1"},
        timeout=5.0,
    )
    try:
        stream = client.chat.completions.create(
            model="m",
            messages=[{"role": "user", "content": "x"}],
            stream=True,
            max_tokens=10,
        )
        try:
            return _collect(stream)
        finally:
            stream.close()
    finally:
        client.close()


OPENAI_ERROR_CASES = {
    "error_en_mitad",
    "error_con_codigo",
    "json_roto",
    "http_401",
    "http_429_retry_after",
}
ANTHROPIC_ERROR_CASES = {
    "error_de_anthropic",
    "error_sin_tipo",
    "error_sin_cuerpo",
    "http_500",
    "http_401_cuerpo_largo",
}


def _add_streams(out: FixtureFile, port: int) -> None:
    for index, case in enumerate(OPENAI_STREAM_CASES, start=1):
        payload = _input_of(case)
        try:
            result = _run_openai(case, port)
        except Exception as error:  # noqa: BLE001 - el caso fija el texto exacto
            if case["id"] not in OPENAI_ERROR_CASES:
                raise
            out.add_error(
                f"llm_sse/openai_stream/{index:04d}",
                "openai_stream",
                payload,
                f"{type(error).__name__}: {error}",
            )
            continue
        out.add(f"llm_sse/openai_stream/{index:04d}", "openai_stream", payload, result)
    for index, case in enumerate(ANTHROPIC_STREAM_CASES, start=1):
        payload = _input_of(case)
        try:
            result = _run_anthropic(case, port)
        except Exception as error:  # noqa: BLE001
            if case["id"] not in ANTHROPIC_ERROR_CASES:
                raise
            out.add_error(
                f"llm_sse/anthropic_stream/{index:04d}",
                "anthropic_stream",
                payload,
                f"{type(error).__name__}: {error}",
            )
            continue
        out.add(f"llm_sse/anthropic_stream/{index:04d}", "anthropic_stream", payload, result)


ITER_SSE_CASES: list[list[str]] = [
    ["event: message_start", 'data: {"type": "a"}', ""],
    [
        ": ping",
        "data:",
        "data: [DONE]",
        "data: {roto",
        "data: [1]",
        'data:{"type":"b"}',
        '  data: {"type": "c"}  ',
    ],
    ['data: {"type": "uno"}', "", 'data: {"type": "dos"}', ""],
    ['data: {"type": "uno"}', 'data: {"type": "dos"}'],
    ['data: {"type": "uno"}', "data: no json", 'data: {"type": "dos"}'],
    ["data:", "", "data:   ", ""],
    ["data: [DONE]", ""],
    ["", "", 'data: {"type": "tras_blancos"}', ""],
    ["data:null", ""],
    ['data: "texto"', ""],
    ["data: 123", ""],
    ['data: {"type": "ñ"}', ""],
    ["event: x", "id: 1", 'data: {"a": 1,', 'data: "b": 2}', ""],
    ['data:{"type":"pegado"}', ""],
    ['DATA: {"type": "mayusculas"}', ""],
    ['datax: {"type": "otro"}', ""],
    ['  data:   {"type": "espacios"}   ', ""],
    ['data: {"type": "uno"}\r\n', "\r\n"],
    ['data: {"type": "sin_salto"}'],
    ['data: {"type": "uno"}', "", 'data: {"type": "dos"}', "", ""],
    ['data: {"type": "a"}', 'data: {"type": "b"}', 'data: {"type": "c"}', ""],
]


def _add_sse_cases(out: FixtureFile) -> None:
    for index, lines in enumerate(ITER_SSE_CASES, start=1):
        events = list(iter_sse(iter(lines)))
        out.add(
            f"llm_sse/iter_sse/{index:04d}",
            "iter_sse",
            {"lines": lines},
            {"events": events},
        )


TRANSLATE_CASES: list[tuple[str, list[dict[str, Any]]]] = [
    (
        "texto",
        [
            {"type": "message_start"},
            {
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "text", "text": ""},
            },
            {
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "Hola"},
            },
            {
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": " mundo"},
            },
            {"type": "content_block_stop", "index": 0},
            {"type": "message_stop"},
            {
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "NO"},
            },
        ],
    ),
    (
        "texto_al_inicio",
        [
            {
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "text", "text": "Va."},
            },
            {
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": " más"},
            },
        ],
    ),
    (
        "texto_vacio",
        [{"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": ""}}],
    ),
    (
        "delta_sin_texto",
        [{"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta"}}],
    ),
    (
        "delta_de_otro_tipo",
        [{"type": "content_block_delta", "index": 0, "delta": {"type": "otra", "text": "x"}}],
    ),
    (
        "herramienta_con_argumentos",
        [
            {
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "text", "text": "Va."},
            },
            {
                "type": "content_block_start",
                "index": 1,
                "content_block": {"type": "tool_use", "id": "tu1", "name": "write_file"},
            },
            {
                "type": "content_block_delta",
                "index": 1,
                "delta": {"type": "input_json_delta", "partial_json": '{"path": "a'},
            },
            {
                "type": "content_block_delta",
                "index": 1,
                "delta": {"type": "input_json_delta", "partial_json": '.txt"}'},
            },
            {"type": "content_block_stop", "index": 1},
        ],
    ),
    (
        "herramienta_sin_argumentos",
        [
            {
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "tool_use", "id": "t", "name": "get_datetime"},
            },
            {"type": "content_block_stop", "index": 0},
        ],
    ),
    (
        "varias_herramientas",
        [
            {
                "type": "content_block_start",
                "index": 2,
                "content_block": {"type": "tool_use", "id": "a", "name": "f"},
            },
            {
                "type": "content_block_delta",
                "index": 2,
                "delta": {"type": "input_json_delta", "partial_json": "{}"},
            },
            {
                "type": "content_block_start",
                "index": 5,
                "content_block": {"type": "tool_use", "id": "b", "name": "g"},
            },
            {
                "type": "content_block_delta",
                "index": 5,
                "delta": {"type": "input_json_delta", "partial_json": "{}"},
            },
        ],
    ),
    (
        "herramienta_sin_id_ni_nombre",
        [
            {"type": "content_block_start", "index": 0, "content_block": {"type": "tool_use"}},
            {"type": "content_block_stop", "index": 0},
        ],
    ),
    (
        "delta_de_bloque_desconocido",
        [
            {
                "type": "content_block_delta",
                "index": 7,
                "delta": {"type": "input_json_delta", "partial_json": "{}"},
            },
            {"type": "content_block_stop", "index": 7},
        ],
    ),
    (
        "stop_end_turn",
        [{"type": "message_delta", "delta": {"stop_reason": "end_turn"}}],
    ),
    (
        "stop_max_tokens",
        [
            {
                "type": "message_delta",
                "delta": {"stop_reason": "max_tokens"},
                "usage": {"output_tokens": 9},
            }
        ],
    ),
    ("stop_tool_use", [{"type": "message_delta", "delta": {"stop_reason": "tool_use"}}]),
    ("stop_desconocido", [{"type": "message_delta", "delta": {"stop_reason": "weird"}}]),
    ("stop_vacio", [{"type": "message_delta", "delta": {"stop_reason": ""}}]),
    ("solo_usage", [{"type": "message_delta", "usage": {"input_tokens": 1, "output_tokens": 2}}]),
    ("message_delta_vacio", [{"type": "message_delta"}]),
    (
        "message_stop_corta",
        [
            {"type": "message_stop"},
            {
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "NO"},
            },
        ],
    ),
    (
        "error_completo",
        [{"type": "error", "error": {"type": "overloaded_error", "message": "Sobrecargado"}}],
    ),
    ("error_sin_tipo", [{"type": "error", "error": {"message": "sin tipo"}}]),
    ("error_sin_cuerpo", [{"type": "error"}]),
    ("error_cuerpo_no_dict", [{"type": "error", "error": "x"}]),
    ("tipo_desconocido", [{"type": "ping"}, {"type": 5}, {}]),
    (
        "indice_flotante",
        [
            {
                "type": "content_block_start",
                "index": 2.9,
                "content_block": {"type": "tool_use", "id": "a", "name": "f"},
            }
        ],
    ),
    (
        "indice_no_numerico",
        [{"type": "content_block_start", "index": "x", "content_block": {"type": "text"}}],
    ),
    ("content_block_no_dict", [{"type": "content_block_start", "index": 0, "content_block": "x"}]),
    (
        "usage_nulo",
        [{"type": "message_delta", "delta": {"stop_reason": None}, "usage": None}],
    ),
    (
        "texto_y_error",
        [
            {
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "antes"},
            },
            {"type": "error", "error": {"type": "api_error", "message": "después"}},
        ],
    ),
]


TRANSLATE_ERRORS = {
    "error_completo",
    "error_sin_tipo",
    "error_sin_cuerpo",
    "error_cuerpo_no_dict",
    "indice_no_numerico",
    "content_block_no_dict",
    "texto_y_error",
}


def _add_translate_cases(out: FixtureFile) -> None:
    for index, (name, events) in enumerate(TRANSLATE_CASES, start=1):
        case = f"llm_sse/translate_events/{index:04d}"
        try:
            result = _collect(translate_events(iter(events)))
        except Exception as error:  # noqa: BLE001 - el caso fija el texto exacto
            if name not in TRANSLATE_ERRORS:
                raise
            out.add_error(
                case,
                "translate_events",
                {"events": events, "variant": name},
                f"{type(error).__name__}: {error}",
            )
            continue
        out.add(case, "translate_events", {"events": events, "variant": name}, result)


def _exception(
    name: str,
    message: str,
    status: Any = _MISSING,
    headers: Any = _MISSING,
) -> BaseException:
    if name == "LLMTurnTimeout":
        return LLMTurnTimeout(message)
    error = type(name, (Exception,), {})(message)
    if status is not _MISSING:
        error.status_code = status  # type: ignore[attr-defined]
    if headers is not _MISSING:
        error.response = SimpleNamespace(headers=headers)  # type: ignore[attr-defined]
    return error


FRIENDLY_CASES: list[dict[str, Any]] = [
    {
        "class": "LLMTurnTimeout",
        "message": "El turno superó el tiempo máximo configurado sin terminar.",
        "label": "OpenAI",
        "model": "gpt-x",
    },
    {
        "class": "StatusError",
        "message": "Error code: 429",
        "label": "Groq",
        "model": "m",
        "status": 429,
        "headers": {},
    },
    {
        "class": "StatusError",
        "message": "Error code: 429",
        "label": "Groq",
        "model": "m",
        "status": 429,
        "headers": {"retry-after": "42"},
    },
    {
        "class": "StatusError",
        "message": "Error code: 429",
        "label": "Groq",
        "model": "m",
        "status": 429,
        "headers": {"retry-after": "0"},
    },
    {
        "class": "StatusError",
        "message": "Error code: 429",
        "label": "Groq",
        "model": "m",
        "status": 429,
        "headers": {"retry-after": ""},
    },
    {
        "class": "StatusError",
        "message": "Error code: 429",
        "label": "Groq",
        "model": "m",
        "status": 429,
        "headers": {"retry-after": "mañana"},
    },
    {
        "class": "StatusError",
        "message": "Error code: 429",
        "label": "Groq",
        "model": "m",
        "status": 429,
        "headers": {"retry-after": " 42"},
    },
    {
        "class": "StatusError",
        "message": "Error code: 429",
        "label": "Groq",
        "model": "m",
        "status": 429,
        "headers": {"retry-after": 42},
    },
    {
        "class": "StatusError",
        "message": "Error code: 429",
        "label": "Groq",
        "model": "m",
        "status": 429,
        "headers": {"retry-after": "0042"},
    },
    {
        "class": "StatusError",
        "message": "Error code: 429",
        "label": "Groq",
        "model": "m",
        "status": 429,
        "headers": {"retry-after": True},
    },
    {
        "class": "StatusError",
        "message": "Error code: 429",
        "label": "Groq",
        "model": "m",
        "status": 429,
    },
    {
        "class": "StatusError",
        "message": "Error code: 413",
        "label": "Cerebras",
        "model": "gpt",
        "status": 413,
    },
    {
        "class": "StatusError",
        "message": "Error code: 500 - Tool call arguments failed to parse as JSON",
        "label": "OpenAI",
        "model": "gpt",
        "status": 500,
    },
    {
        "class": "StatusError",
        "message": "Error code: 500 - tool call json",
        "label": "OpenAI",
        "model": "gpt",
        "status": 500,
    },
    {
        "class": "StatusError",
        "message": "Error code: 500 - Tool call parse",
        "label": "OpenAI",
        "model": "gpt",
        "status": 500,
    },
    {
        "class": "StatusError",
        "message": "Error code: 500 - tool call failed",
        "label": "OpenAI",
        "model": "gpt",
        "status": 500,
    },
    {
        "class": "StatusError",
        "message": "Error code: 502 - bad gateway",
        "label": "OpenAI",
        "model": "gpt",
        "status": 502,
    },
    {
        "class": "StatusError",
        "message": "Error code: 404",
        "label": "Anthropic",
        "model": "claude-x",
        "status": 404,
    },
    {
        "class": "StatusError",
        "message": "Error code: 401",
        "label": "OpenAI",
        "model": "gpt",
        "status": 401,
    },
    {
        "class": "StatusError",
        "message": "Error code: 403",
        "label": "OpenAI",
        "model": "gpt",
        "status": 403,
    },
    {
        "class": "StatusError",
        "message": "Error code: 400",
        "label": "OpenAI",
        "model": "gpt",
        "status": 400,
    },
    {
        "class": "StatusError",
        "message": "Incorrect API key provided: sk-SECRETO",
        "label": "OpenAI",
        "model": "gpt",
        "status": 401,
    },
    {"class": "RuntimeError", "message": "Error code: 429 - slow", "label": "Groq", "model": "m"},
    {"class": "RuntimeError", "message": "HTTP 404: no model", "label": "Y", "model": "m"},
    {"class": "RuntimeError", "message": "La API respondió 401: nope", "label": "Y", "model": "m"},
    {"class": "RuntimeError", "message": "Error 4290: raro", "label": "Y", "model": "m"},
    {"class": "ValueError", "message": "algo raro", "label": "Y", "model": "m"},
    {"class": "APIConnectionError", "message": "sin red", "label": "X", "model": "m"},
    {"class": "ConnectTimeout", "message": "sin red", "label": "X", "model": "m"},
    {"class": "ReadTimeout", "message": "sin red", "label": "X", "model": "m"},
    {"class": "ConnectError", "message": "sin red", "label": "X", "model": "m"},
    {"class": "APITimeoutError", "message": "sin red", "label": "X", "model": "m"},
    {"class": "MyConnectionBoom", "message": "sin red", "label": "X", "model": "m"},
    {"class": "SocketConnectionRefused", "message": "sin red", "label": "X", "model": "m"},
    {"class": "StatusError", "message": "x" * 400, "label": "Y", "model": "m", "status": 400},
    {
        "class": "StatusError",
        "message": "ñ" * 299 + "🙂" + "z",
        "label": "Y",
        "model": "m",
        "status": 400,
    },
    {"class": "StatusError", "message": "con\nsalto", "label": "Y", "model": "m", "status": 400},
    {"class": "StatusError", "message": "", "label": "Y", "model": "m", "status": 400},
    {
        "class": "StatusError",
        "message": "tool call parse",
        "label": "Y",
        "model": "m",
        "status": 401,
    },
    {
        "class": "StatusError",
        "message": "tool call json",
        "label": "Y",
        "model": "m",
        "status": 500,
    },
    {
        "class": "StatusError",
        "message": "json pero no herramienta",
        "label": "Y",
        "model": "m",
        "status": 500,
    },
    {
        "class": "StatusError",
        "message": "Error code: 429",
        "label": "",
        "model": "",
        "status": 429,
        "headers": {},
    },
]


def _add_friendly_cases(out: FixtureFile) -> None:
    for index, case in enumerate(FRIENDLY_CASES, start=1):
        status = case.get("status", _MISSING)
        headers = case.get("headers", _MISSING)
        error = _exception(str(case["class"]), str(case["message"]), status, headers)
        payload = {
            "class": case["class"],
            "message": case["message"],
            "label": case["label"],
            "model": case["model"],
        }
        if status is not _MISSING:
            payload["status"] = status
        if headers is not _MISSING:
            payload["headers"] = headers
        out.add(
            f"llm_sse/friendly_llm_error/{index:04d}",
            "friendly_llm_error",
            payload,
            {"text": friendly_llm_error(error, str(case["label"]), str(case["model"]))},
        )


def generate(out: FixtureFile) -> None:
    server = ThreadingHTTPServer(("127.0.0.1", 0), _Handler)
    server.daemon_threads = True
    thread = threading.Thread(target=server.serve_forever, name="ezparity-llm-sse")
    thread.start()
    try:
        _add_streams(out, int(server.server_port))
        _add_sse_cases(out)
        _add_translate_cases(out)
        _add_friendly_cases(out)
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
