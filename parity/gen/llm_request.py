"""Fixtures del cuerpo exacto de las peticiones del `Messenger` (F1-05).

Cada caso `turn` ejecuta un turno real de `Messenger` contra un `http.server`
local que captura el cuerpo JSON de cada petición (OpenAI Chat Completions o
Anthropic Messages). Nada de red externa ni de claves reales.

Además se ejercitan las conversiones de `anthropic_client` sin red:

* ``messages_url``       entrada ``{"base_url": str}`` -> ``{"url": str}``
* ``convert_messages``   entrada ``{"messages": [...]}`` -> ``{"system": str, "messages": [...]}``
* ``convert_tools``      entrada ``{"tools": list | None}`` -> ``{"tools": [...]}``
* ``build_body``         entrada ``{"request": {...}, "prompt_cache": bool}`` -> ``{"body": {...}}``

En los turnos, ``input.settings.llm_base_url`` vale ``"<local>"``: el puerto del
servidor de prueba no forma parte del oráculo.
"""

from __future__ import annotations

import copy
import json
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from types import SimpleNamespace
from typing import Any

from _common import FixtureFile, main_guard

from ezagent.voice.anthropic_client import (
    AnthropicChatClient,
    convert_messages,
    convert_tools,
    messages_url,
)
from ezagent.voice.llm import Messenger

DOMAIN = "llm_request"

BASE_SETTINGS: dict[str, Any] = {
    "llm_provider": "openai",
    "llm_model": "gpt-x",
    "llm_temperature": 0.3,
    "llm_max_tokens": 150,
    "llm_base_url": "http://127.0.0.1:1/v1",
    "api_key": "clave-falsa",
    "llm_api_style": "openai",
    "llm_reasoning_effort": "none",
    "llm_context_tokens": 32_000,
    "llm_timeout_secs": 30.0,
    "llm_turn_timeout_secs": 120.0,
    "llm_max_tool_rounds": 8,
    "confirm_actions": False,
}

TOOL_DATETIME: dict[str, Any] = {
    "type": "function",
    "function": {
        "name": "get_datetime",
        "description": "Hora actual",
        "parameters": {"type": "object", "properties": {}},
    },
}

TOOL_SEARCH: dict[str, Any] = {
    "type": "function",
    "function": {
        "name": "web_search",
        "description": "Busca en la web",
        "parameters": {
            "type": "object",
            "properties": {"query": {"type": "string"}},
            "required": ["query"],
        },
    },
}

BASE_TOOLS: tuple[str, ...] = (
    "voice_control",
    "web_search",
    "fetch_page",
    "read_file",
    "delegate_to_agent",
)


class _Recorder:
    """Peticiones capturadas y texto que devuelve el servidor."""

    # DECISION: solo se guardan `path` y el cuerpo JSON; el puerto efímero y
    # las cabeceras (incluida la sesión aleatoria de opencode) no son oráculo.
    requests: list[dict[str, Any]] = []
    reply = "ok"


def _openai_sse(text: str) -> bytes:
    payload = {
        "id": "chatcmpl-local",
        "object": "chat.completion.chunk",
        "created": 0,
        "model": "local",
        "choices": [{"index": 0, "delta": {"content": text}, "finish_reason": "stop"}],
    }
    return f"data: {json.dumps(payload)}\n\ndata: [DONE]\n\n".encode()


def _anthropic_sse(text: str) -> bytes:
    events = [
        {"type": "message_start", "message": {"id": "msg_local", "type": "message"}},
        {
            "type": "content_block_start",
            "index": 0,
            "content_block": {"type": "text", "text": ""},
        },
        {
            "type": "content_block_delta",
            "index": 0,
            "delta": {"type": "text_delta", "text": text},
        },
        {"type": "content_block_stop", "index": 0},
        {
            "type": "message_delta",
            "delta": {"stop_reason": "end_turn"},
            "usage": {"output_tokens": 1},
        },
        {"type": "message_stop"},
    ]
    return "".join(
        f"event: {event['type']}\ndata: {json.dumps(event)}\n\n" for event in events
    ).encode()


class _Handler(BaseHTTPRequestHandler):
    def do_POST(self) -> None:
        length = int(self.headers.get("content-length") or 0)
        raw = self.rfile.read(length)
        try:
            body = json.loads(raw)
        except ValueError:
            body = None
        _Recorder.requests.append({"path": self.path, "body": body})
        data = (
            _anthropic_sse(_Recorder.reply)
            if self.path.endswith("/messages")
            else _openai_sse(_Recorder.reply)
        )
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.send_header("cache-control", "no-cache")
        self.end_headers()
        try:
            self.wfile.write(data)
            self.wfile.flush()
        except OSError:
            pass

    def log_message(self, *args: Any) -> None:
        pass


class _Registry:
    """``runtime.registry`` mínimo para el camino de herramientas base."""

    def schemas(
        self, names: list[str] | None = None, limit: int | None = None
    ) -> list[dict[str, Any]]:
        found = [
            {"type": "function", "function": {"name": name, "parameters": {"type": "object"}}}
            for name in names or ()
        ]
        return found[:limit] if limit is not None else found


class _Runtime:
    """Runtime simulado del núcleo agéntico."""

    def __init__(self, spec: dict[str, Any]) -> None:
        self.spec = spec
        self.prompt = str(spec.get("prompt", "PROMPT-SISTEMA"))
        self.volatile = str(spec.get("volatile", ""))
        self.tools = copy.deepcopy(spec.get("tools", [TOOL_DATETIME]))
        self.executed: list[tuple[str, str]] = []
        self.recorded: list[str] = []
        self.turns: list[str] = []
        self.registry = _Registry()
        remaining = spec.get("remaining")
        if remaining is not None:
            self.remaining_tool_calls = lambda: int(remaining)
        if spec.get("prompt_parts"):
            self.prompt_parts = lambda: (self.prompt, self.volatile)

    def begin_turn(self, text: str) -> None:
        if self.spec.get("begin_fails"):
            raise RuntimeError("memoria corrupta")
        self.turns.append(text)

    def system_prompt(self) -> str:
        return self.prompt

    def tools_for(self, text: str) -> list[dict[str, Any]]:
        if self.spec.get("tools_for_fails"):
            raise TypeError("expected string or bytes-like object, got 'NoneType'")
        return copy.deepcopy(self.tools)

    def render_result(self, result: Any) -> str:
        return json.dumps(result.to_dict(), ensure_ascii=False)

    def record_assistant(self, text: str) -> None:
        self.recorded.append(text)

    def execute(self, name: str, raw: str) -> Any:
        self.executed.append((name, raw))
        return SimpleNamespace(to_dict=lambda: {"ok": True, "data": "resultado"})

    def pending_notify_address(self) -> None:
        return None


def _history(*messages: dict[str, Any]) -> list[dict[str, Any]]:
    return [{"role": "system", "content": "PROMPT-SISTEMA"}, *messages]


TURN_CASES: list[dict[str, Any]] = [
    {
        "id": "openai_sin_herramientas",
        "runtime": {"tools": []},
        "user": "hola",
    },
    {
        "id": "openai_sin_runtime",
        "runtime": None,
        "user": "borra todo",
    },
    {
        "id": "openai_con_herramientas_sin_presupuesto",
        "runtime": {"tools": [TOOL_DATETIME, TOOL_SEARCH]},
        "user": "qué hora es",
    },
    {
        "id": "openai_con_herramientas_presupuesto_3",
        "runtime": {"tools": [TOOL_DATETIME], "remaining": 3},
        "user": "qué hora es",
    },
    {
        "id": "openai_presupuesto_cero_cierra",
        "runtime": {"tools": [TOOL_DATETIME], "remaining": 0},
        "user": "qué hora es",
    },
    {
        "id": "openai_chat_con_herramientas",
        "runtime": {"tools": [TOOL_DATETIME]},
        "user": "resume el repositorio",
        "mode": "chat",
    },
    {
        "id": "openai_chat_sin_herramientas",
        "runtime": {"tools": []},
        "user": "resume",
        "mode": "chat",
    },
    {
        "id": "openai_razonador_o4",
        "settings": {"llm_provider": "openai", "llm_model": "o4-mini", "llm_max_tokens": 100},
        "runtime": {"tools": []},
        "user": "piensa",
    },
    {
        "id": "openai_razonador_o1_esfuerzo_alto",
        "settings": {
            "llm_provider": "openai",
            "llm_model": "o1",
            "llm_reasoning_effort": "high",
            "llm_max_tokens": 320,
        },
        "runtime": {"tools": [TOOL_DATETIME]},
        "user": "piensa",
    },
    {
        "id": "openai_tope_de_salida_del_modelo",
        "settings": {"llm_max_tokens": 100_000},
        "runtime": {"tools": []},
        "user": "escribe mucho",
    },
    {
        "id": "openai_tope_minimo_uno",
        "settings": {"llm_max_tokens": 1},
        "runtime": {"tools": []},
        "user": "x",
    },
    {
        "id": "openai_datos_volatiles",
        "runtime": {
            "tools": [TOOL_DATETIME],
            "prompt_parts": True,
            "volatile": "  hora local 12:00  ",
        },
        "user": "qué hora es",
    },
    {
        "id": "openai_datos_volatiles_vacios",
        "runtime": {"tools": [], "prompt_parts": True, "volatile": "   "},
        "user": "hola",
    },
    {
        "id": "openai_begin_turn_falla",
        "runtime": {"tools": [TOOL_DATETIME], "begin_fails": True},
        "user": "hola",
    },
    {
        "id": "openai_tools_for_falla_usa_base",
        "runtime": {"tools": [TOOL_DATETIME], "tools_for_fails": True},
        "user": "hola",
    },
    {
        "id": "openai_historial_system_intermedio",
        "runtime": {"tools": [TOOL_DATETIME]},
        "history": _history(
            {"role": "user", "content": "uno"},
            {"role": "assistant", "content": "ok"},
            {"role": "system", "content": "El agente terminó"},
            {"role": "user", "content": "dos"},
        ),
        "user": "tres",
    },
    {
        "id": "openai_historial_usuarios_seguidos",
        "runtime": {"tools": []},
        "history": _history(
            {"role": "user", "content": "uno"},
            {"role": "user", "content": "dos"},
            {"role": "assistant", "content": "ok"},
            {"role": "user", "content": "tres"},
        ),
        "user": "cuatro",
    },
    {
        "id": "openai_historial_json_roto",
        "runtime": {"tools": [TOOL_SEARCH]},
        "history": _history(
            {"role": "user", "content": "busca"},
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "web_search", "arguments": '{"query": "gpu'},
                    }
                ],
            },
            {"role": "tool", "tool_call_id": "call_1", "content": "{}"},
        ),
        "user": "sigue",
    },
    {
        "id": "openai_historial_tool_huerfano",
        "runtime": {"tools": []},
        "history": _history(
            {"role": "user", "content": "uno"},
            {"role": "tool", "tool_call_id": "fantasma", "content": "{}"},
            {"role": "assistant", "content": "ok"},
        ),
        "user": "dos",
    },
    {
        "id": "openai_historial_assistant_vacio",
        "runtime": {"tools": []},
        "history": _history(
            {"role": "user", "content": "uno"},
            {"role": "assistant", "content": "   "},
            {"role": "assistant", "content": "ok"},
        ),
        "user": "dos",
    },
    {
        "id": "openai_historial_llamada_y_resultado",
        "runtime": {"tools": [TOOL_SEARCH]},
        "history": _history(
            {"role": "user", "content": "busca"},
            {
                "role": "assistant",
                "content": "Busco.",
                "tool_calls": [
                    {
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "web_search", "arguments": '{"query": "gpu"}'},
                    }
                ],
            },
            {"role": "tool", "tool_call_id": "call_1", "content": '{"ok": true}'},
            {"role": "assistant", "content": "Listo."},
        ),
        "user": "gracias",
    },
    {
        "id": "openai_resumen_previo",
        "runtime": {"tools": []},
        "history": _history(
            {"role": "user", "content": "uno"},
            {"role": "assistant", "content": "ok"},
        ),
        "summary": "  Lo hablado antes: rutas y pendientes.  ",
        "user": "sigue",
    },
    {
        "id": "openai_contexto_recorta_resultado",
        "settings": {"llm_context_tokens": 4000},
        "runtime": {"tools": [TOOL_DATETIME]},
        "history": _history(
            {"role": "user", "content": "lee el archivo"},
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "read_file", "arguments": '{"path": "a.txt"}'},
                    }
                ],
            },
            {"role": "tool", "tool_call_id": "call_1", "content": "x" * 4000},
            {"role": "assistant", "content": "Leído."},
            {"role": "user", "content": "y ahora " + "y" * 2500},
            {"role": "assistant", "content": "ok"},
        ),
        "user": "siguiente",
    },
    {
        "id": "openai_contexto_recorta_json",
        "settings": {"llm_context_tokens": 4000},
        "runtime": {"tools": [TOOL_DATETIME]},
        "history": _history(
            {"role": "user", "content": "lee el archivo"},
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "read_file", "arguments": '{"path": "a.txt"}'},
                    }
                ],
            },
            {
                "role": "tool",
                "tool_call_id": "call_1",
                "content": json.dumps({"ok": True, "data": "z" * 4000}),
            },
            {"role": "assistant", "content": "Leído."},
            {"role": "user", "content": "y ahora " + "y" * 2500},
            {"role": "assistant", "content": "ok"},
        ),
        "user": "siguiente",
    },
    {
        "id": "openai_contexto_compacta",
        "settings": {"llm_context_tokens": 4000},
        "runtime": {"tools": [TOOL_DATETIME]},
        "history": _history(
            *[
                item
                for turn in range(6)
                for item in (
                    {"role": "user", "content": f"pregunta {turn} " + "texto " * 80},
                    {"role": "assistant", "content": f"respuesta {turn} " + "dato " * 80},
                )
            ]
        ),
        "user": "resume lo anterior",
        "emit": "resumen breve",
    },
    {
        "id": "anthropic_nativo_con_herramientas",
        "settings": {
            "llm_provider": "anthropic",
            "llm_api_style": "anthropic",
            "llm_model": "claude-x",
        },
        "runtime": {"tools": [TOOL_DATETIME]},
        "user": "qué hora es",
    },
    {
        "id": "anthropic_nativo_presupuesto",
        "settings": {
            "llm_provider": "anthropic",
            "llm_api_style": "anthropic",
            "llm_model": "claude-x",
        },
        "runtime": {"tools": [TOOL_DATETIME], "remaining": 2},
        "user": "qué hora es",
    },
    {
        "id": "anthropic_nativo_chat",
        "settings": {
            "llm_provider": "anthropic",
            "llm_api_style": "anthropic",
            "llm_model": "claude-x",
        },
        "runtime": {"tools": []},
        "user": "resume",
        "mode": "chat",
    },
    {
        "id": "anthropic_endpoint_openai_temperatura_uno",
        "settings": {
            "llm_provider": "anthropic",
            "llm_api_style": "openai",
            "llm_model": "claude-x",
        },
        "runtime": {"tools": []},
        "user": "hola",
    },
    {
        "id": "proveedor_opencode_esfuerzo_none",
        "settings": {"llm_provider": "opencode", "llm_model": "deepseek-v4.1-flash"},
        "runtime": {"tools": []},
        "user": "hola",
    },
    {
        "id": "proveedor_groq_esfuerzo_low",
        "settings": {
            "llm_provider": "groq",
            "llm_model": "llama-3.3-70b-versatile",
            "llm_reasoning_effort": "low",
        },
        "runtime": {"tools": []},
        "user": "hola",
    },
    {
        "id": "proveedor_gemini_sin_esfuerzo",
        "settings": {
            "llm_provider": "gemini",
            "llm_model": "gemini-2.5-flash",
            "llm_reasoning_effort": "high",
        },
        "runtime": {"tools": []},
        "user": "hola",
    },
    {
        "id": "proveedor_anthropic_esfuerzo_ignorado",
        "settings": {
            "llm_provider": "anthropic",
            "llm_api_style": "openai",
            "llm_model": "claude-x",
            "llm_reasoning_effort": "high",
        },
        "runtime": {"tools": []},
        "user": "hola",
    },
]


def _settings_of(case: dict[str, Any]) -> dict[str, Any]:
    settings = dict(BASE_SETTINGS)
    settings.update(case.get("settings") or {})
    return settings


def _runtime_of(spec: dict[str, Any] | None) -> _Runtime | None:
    return None if spec is None else _Runtime(spec)


def _messenger(settings: dict[str, Any], runtime: _Runtime | None) -> Messenger:
    return Messenger(
        SimpleNamespace(**settings),
        SimpleNamespace(),
        last_summary=lambda: "",
        watch=lambda address: None,
        mute=lambda muted: None,
        request_close=lambda: None,
        mic_mute=lambda muted: None,
        runtime=runtime,
    )


def _server_url(port: int, settings: dict[str, Any]) -> str:
    if settings["llm_provider"] == "anthropic" and settings["llm_api_style"] == "anthropic":
        return f"http://127.0.0.1:{port}"
    return f"http://127.0.0.1:{port}/v1"


def _input_of(case: dict[str, Any]) -> dict[str, Any]:
    settings = _settings_of(case)
    settings["llm_base_url"] = "<local>"
    return {
        "settings": settings,
        "runtime": copy.deepcopy(case.get("runtime")),
        "history": copy.deepcopy(case.get("history")),
        "summary": case.get("summary", ""),
        "user": case.get("user", "hola"),
        "mode": case.get("mode", "voice"),
        "emit": case.get("emit", "ok"),
    }


def _run_turn(case: dict[str, Any], port: int) -> dict[str, Any]:
    settings = _settings_of(case)
    settings["llm_base_url"] = _server_url(port, settings)
    runtime = _runtime_of(case.get("runtime"))
    messenger = _messenger(settings, runtime)
    history = case.get("history")
    if history is not None:
        messenger.messages = copy.deepcopy(history)
    messenger.summary = str(case.get("summary", ""))
    _Recorder.reply = str(case.get("emit", "ok"))
    _Recorder.requests.clear()
    pieces = list(
        messenger.ask_stream(str(case.get("user", "hola")), mode=str(case.get("mode", "voice")))
    )
    return {
        "requests": copy.deepcopy(_Recorder.requests),
        "pieces": pieces,
        "answer": "".join(pieces),
        "history": copy.deepcopy(messenger.messages),
        "summary": messenger.summary,
        "last_rounds": list(messenger.last_rounds),
    }


def _add_turns(out: FixtureFile, port: int) -> None:
    for index, case in enumerate(TURN_CASES, start=1):
        out.add(
            f"llm_request/turn/{index:04d}",
            "turn",
            _input_of(case),
            _run_turn(case, port),
        )


MESSAGES_URL_CASES: list[Any] = [
    "https://api.anthropic.com/v1",
    "https://api.anthropic.com/v1/",
    "https://api.anthropic.com",
    "https://x.io/anthropic",
    "  https://x.io/v1  ",
    "https://x.io/v1/messages",
    "http://127.0.0.1:8080/v1",
    "http://127.0.0.1:8080",
    "http://127.0.0.1:8080/",
    "",
    "   ",
    None,
    "/v1",
    "v1",
    "https://x.io/V1",
]

CONVERT_MESSAGES_CASES: list[list[dict[str, Any]]] = [
    [],
    [{"role": "system", "content": "Eres EzAgent."}],
    [
        {"role": "system", "content": "Eres EzAgent."},
        {"role": "user", "content": "hola"},
        {"role": "assistant", "content": "¿en qué te ayudo?"},
    ],
    [
        {"role": "system", "content": "base"},
        {"role": "assistant", "content": "ok"},
        {"role": "system", "content": "El agente terminó"},
    ],
    [
        {"role": "system", "content": "base"},
        {"role": "system", "content": "segundo"},
        {"role": "user", "content": "x"},
    ],
    [
        {"role": "user", "content": "hora"},
        {
            "role": "assistant",
            "content": "Un momento.",
            "tool_calls": [
                {
                    "id": "c1",
                    "type": "function",
                    "function": {"name": "get_datetime", "arguments": '{"tz": "UTC"}'},
                },
                {"id": "c2", "type": "function", "function": {"name": "otra", "arguments": ""}},
            ],
        },
        {"role": "tool", "tool_call_id": "c1", "content": '{"ok": true}'},
        {"role": "tool", "tool_call_id": "c2", "content": "{}"},
    ],
    [
        {"role": "user", "content": "uno"},
        {"role": "user", "content": "   "},
        {"role": "user", "content": "dos"},
        {"role": "assistant", "content": ""},
    ],
    [{"role": "function", "content": "x"}, {"role": "developer"}, {"content": "sin rol"}],
    [
        {"role": "assistant", "content": 5},
        {"role": "user", "content": None},
        {"role": "system", "content": None},
    ],
    [
        {"role": "assistant", "content": "solo", "tool_calls": []},
        {"role": "assistant", "content": "texto", "tool_calls": None},
        {"role": "assistant", "content": "", "tool_calls": [{"id": "x"}]},
    ],
    [
        {"role": "user", "content": "ñandú ☕"},
        {"role": "assistant", "content": "🙂"},
        {"role": "tool", "tool_call_id": "ñ", "content": "☕"},
    ],
]


TOOL_ARGUMENT_CASES: list[Any] = [
    "{no json",
    "[1,2]",
    '"texto"',
    "null",
    None,
    5,
    '{"a": 1}',
    "{}",
    "",
    '{"ñ": "☕"}',
]


def _convert_messages_cases(out: FixtureFile) -> None:
    for index, messages in enumerate(CONVERT_MESSAGES_CASES, start=1):
        system, converted = convert_messages(copy.deepcopy(messages))
        out.add(
            f"llm_request/convert_messages/{index:04d}",
            "convert_messages",
            {"messages": messages},
            {"system": system, "messages": converted},
        )
    for index, arguments in enumerate(TOOL_ARGUMENT_CASES, start=1):
        messages = [
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [{"id": "c", "function": {"name": "f", "arguments": arguments}}],
            }
        ]
        system, converted = convert_messages(copy.deepcopy(messages))
        out.add(
            f"llm_request/convert_messages_args/{index:04d}",
            "convert_messages",
            {"messages": messages},
            {"system": system, "messages": converted},
        )


CONVERT_TOOLS_CASES: list[Any] = [
    None,
    [],
    [
        {
            "type": "function",
            "function": {
                "name": "a",
                "description": "da",
                "parameters": {"type": "object", "properties": {"x": {"type": "string"}}},
            },
        },
        {"type": "function", "function": {"name": "b"}},
        {"name": "c", "description": "plana", "parameters": {"type": "object", "properties": {}}},
        {"type": "function", "function": {"description": "sin nombre"}},
        {"type": "function", "function": "no dict"},
    ],
    [
        {"name": "d", "parameters": None},
        {"name": "e", "description": ""},
        {"description": "sin nombre", "parameters": {"type": "object"}},
        {"name": "", "parameters": {"type": "object"}},
    ],
    [
        {
            "type": "function",
            "function": {
                "name": "ñ",
                "description": "☕",
                "parameters": {"type": "object", "properties": {"ñ": {"type": "string"}}},
            },
        },
    ],
]


BUILD_BODY_CASES: list[dict[str, Any]] = [
    {
        "prompt_cache": False,
        "request": {
            "model": "claude-sonnet-5-5",
            "max_tokens": 300,
            "temperature": 0.2,
            "messages": [
                {"role": "system", "content": "sys"},
                {"role": "user", "content": "hola"},
            ],
            "tools": [
                {"type": "function", "function": {"name": "f", "parameters": {"type": "object"}}}
            ],
            "stream": True,
            "extra_body": {"reasoning_effort": "none"},
        },
    },
    {
        "prompt_cache": True,
        "request": {
            "model": "claude-sonnet-5-5",
            "max_tokens": 300,
            "temperature": 0.2,
            "messages": [
                {"role": "system", "content": "sys"},
                {"role": "user", "content": "hola"},
            ],
            "tools": [
                {"type": "function", "function": {"name": "f", "parameters": {"type": "object"}}}
            ],
        },
    },
    {
        "prompt_cache": False,
        "request": {
            "model": "m",
            "messages": [{"role": "user", "content": "x"}],
            "max_tokens": None,
        },
    },
    {
        "prompt_cache": False,
        "request": {
            "model": "m",
            "messages": [{"role": "user", "content": "x"}],
            "max_tokens": 0,
            "temperature": None,
        },
    },
    {
        "prompt_cache": False,
        "request": {
            "model": "m",
            "messages": [{"role": "user", "content": "x"}],
            "max_tokens": "12",
            "temperature": 0,
        },
    },
    {
        "prompt_cache": False,
        "request": {
            "model": "m",
            "messages": [{"role": "user", "content": "x"}],
            "max_tokens": 12.9,
            "temperature": "0.5",
        },
    },
    {
        "prompt_cache": True,
        "request": {
            "model": "m",
            "messages": [
                {"role": "system", "content": "uno"},
                {"role": "system", "content": "dos"},
                {"role": "user", "content": "x"},
            ],
            "tools": [{"name": "plana"}],
        },
    },
    {
        "prompt_cache": False,
        "request": {
            "model": "m",
            "messages": [{"role": "system", "content": "   "}, {"role": "user", "content": "x"}],
            "tools": [],
        },
    },
]


def _client(prompt_cache: bool) -> AnthropicChatClient:
    return AnthropicChatClient(
        api_key="clave-falsa",
        base_url="https://api.anthropic.com/v1",
        prompt_cache=prompt_cache,
    )


def _add_conversions(out: FixtureFile) -> None:
    for index, base in enumerate(MESSAGES_URL_CASES, start=1):
        out.add(
            f"llm_request/messages_url/{index:04d}",
            "messages_url",
            {"base_url": base},
            {"url": messages_url(base)},
        )
    for index, tools in enumerate(CONVERT_TOOLS_CASES, start=1):
        out.add(
            f"llm_request/convert_tools/{index:04d}",
            "convert_tools",
            {"tools": tools},
            {"tools": convert_tools(copy.deepcopy(tools))},
        )
    for index, case in enumerate(BUILD_BODY_CASES, start=1):
        body = _client(bool(case["prompt_cache"])).build_body(copy.deepcopy(case["request"]))
        out.add(
            f"llm_request/build_body/{index:04d}",
            "build_body",
            case,
            {"body": body},
        )


def generate(out: FixtureFile) -> None:
    server = ThreadingHTTPServer(("127.0.0.1", 0), _Handler)
    server.daemon_threads = True
    thread = threading.Thread(target=server.serve_forever, name="ezparity-llm-request")
    thread.start()
    # DECISION: el reloj monótono se congela para que `last_rounds` («0 ms al
    # 1.er fragmento») sea reproducible; en el Python real solo mide latencia.
    original_monotonic = time.monotonic
    time.monotonic = lambda: 1000.0
    try:
        _add_turns(out, int(server.server_port))
        _add_conversions(out)
        _convert_messages_cases(out)
    finally:
        time.monotonic = original_monotonic
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
