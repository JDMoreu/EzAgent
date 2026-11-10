"""AnthropicChatClient: traduce Chat Completions <-> Messages sin perder nada."""

from __future__ import annotations

import json

import httpx
import pytest

from ezagent.voice.anthropic_client import (
    AnthropicChatClient,
    convert_messages,
    convert_tools,
    iter_sse,
    messages_url,
    translate_events,
)


@pytest.mark.parametrize(
    ("base", "expected"),
    [
        ("https://api.anthropic.com/v1", "https://api.anthropic.com/v1/messages"),
        ("https://api.anthropic.com/v1/", "https://api.anthropic.com/v1/messages"),
        ("https://api.anthropic.com", "https://api.anthropic.com/v1/messages"),
        ("https://x.io/anthropic", "https://x.io/anthropic/v1/messages"),
        ("  https://x.io/v1  ", "https://x.io/v1/messages"),
    ],
)
def test_messages_url(base: str, expected: str) -> None:
    assert messages_url(base) == expected


# ------------------------------------------------------------- mensajes
def test_el_system_inicial_sale_aparte_y_el_resto_se_convierte() -> None:
    system, messages = convert_messages(
        [
            {"role": "system", "content": "Eres EzAgent."},
            {"role": "user", "content": "hola"},
            {"role": "assistant", "content": "¿en qué te ayudo?"},
        ]
    )
    assert system == "Eres EzAgent."
    assert messages == [
        {"role": "user", "content": [{"type": "text", "text": "hola"}]},
        {"role": "assistant", "content": [{"type": "text", "text": "¿en qué te ayudo?"}]},
    ]


def test_un_system_a_mitad_de_conversacion_se_vuelve_aviso_del_usuario() -> None:
    system, messages = convert_messages(
        [
            {"role": "system", "content": "base"},
            {"role": "assistant", "content": "ok"},
            {"role": "system", "content": "El agente terminó"},
        ]
    )
    assert system == "base"
    assert messages[-1]["role"] == "user"
    assert messages[-1]["content"][0]["text"] == "[Aviso del sistema]\nEl agente terminó"


def test_llamadas_a_herramientas_y_sus_resultados() -> None:
    _, messages = convert_messages(
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
        ]
    )
    assistant = messages[1]
    assert assistant["role"] == "assistant"
    assert assistant["content"][0] == {"type": "text", "text": "Un momento."}
    assert assistant["content"][1] == {
        "type": "tool_use",
        "id": "c1",
        "name": "get_datetime",
        "input": {"tz": "UTC"},
    }
    assert assistant["content"][2]["input"] == {}
    # Los dos resultados van juntos en un único turno de usuario.
    assert messages[2]["role"] == "user" and len(messages) == 3
    assert [b["tool_use_id"] for b in messages[2]["content"]] == ["c1", "c2"]


@pytest.mark.parametrize("arguments", ["{no json", "[1,2]", '"texto"', "null", None, 5])
def test_argumentos_invalidos_de_herramienta_se_convierten_en_objeto_vacio(arguments) -> None:
    _, messages = convert_messages(
        [
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [{"id": "c", "function": {"name": "f", "arguments": arguments}}],
            },
        ]
    )
    assert messages[0]["content"] == [{"type": "tool_use", "id": "c", "name": "f", "input": {}}]


def test_turnos_seguidos_del_mismo_rol_se_unen_y_el_texto_vacio_se_omite() -> None:
    _, messages = convert_messages(
        [
            {"role": "user", "content": "uno"},
            {"role": "user", "content": "   "},
            {"role": "user", "content": "dos"},
            {"role": "assistant", "content": ""},
        ]
    )
    assert messages == [
        {
            "role": "user",
            "content": [{"type": "text", "text": "uno"}, {"type": "text", "text": "dos"}],
        }
    ]


def test_roles_desconocidos_se_ignoran() -> None:
    assert convert_messages([{"role": "function", "content": "x"}, {"role": "developer"}]) == (
        "",
        [],
    )


def test_convert_tools() -> None:
    tools = [
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
    ]
    converted = convert_tools(tools)
    assert [t["name"] for t in converted] == ["a", "b", "c"]
    assert converted[0]["input_schema"]["properties"] == {"x": {"type": "string"}}
    assert converted[1] == {
        "name": "b",
        "description": "",
        "input_schema": {"type": "object", "properties": {}},
    }
    assert convert_tools(None) == [] and convert_tools([]) == []


# ----------------------------------------------------------------- SSE
def test_iter_sse_ignora_ruido() -> None:
    lines = [
        "event: message_start",
        'data: {"type": "a"}',
        "",
        ": ping",
        "data:",
        "data: [DONE]",
        "data: {roto",
        "data: [1]",
        'data:{"type":"b"}',
        '  data: {"type": "c"}  ',
    ]
    assert [e["type"] for e in iter_sse(iter(lines))] == ["a", "b", "c"]


def collect(events: list[dict]) -> tuple[str, list[dict]]:
    text, calls = [], {}
    for chunk in translate_events(iter(events)):
        delta = chunk.choices[0].delta
        if delta.content:
            text.append(delta.content)
        for call in delta.tool_calls or []:
            item = calls.setdefault(call.index, {"id": "", "name": "", "args": ""})
            item["id"] = call.id or item["id"]
            item["name"] += call.function.name or ""
            item["args"] += call.function.arguments or ""
    return "".join(text), [calls[i] for i in sorted(calls)]


def test_texto_en_streaming() -> None:
    text, calls = collect(
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
        ]
    )
    assert text == "Hola mundo" and calls == []  # nada después de message_stop


def test_herramienta_con_argumentos_en_trozos_y_texto_previo() -> None:
    text, calls = collect(
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
        ]
    )
    assert text == "Va."
    assert calls == [{"id": "tu1", "name": "write_file", "args": '{"path": "a.txt"}'}]
    assert json.loads(calls[0]["args"]) == {"path": "a.txt"}


def test_herramienta_sin_argumentos_recibe_un_objeto_json_vacio() -> None:
    _, calls = collect(
        [
            {
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "tool_use", "id": "t", "name": "get_datetime"},
            },
            {"type": "content_block_stop", "index": 0},
        ]
    )
    assert calls == [{"id": "t", "name": "get_datetime", "args": "{}"}]


def test_varias_herramientas_reciben_indices_consecutivos_aunque_el_bloque_sea_otro() -> None:
    _, calls = collect(
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
        ]
    )
    assert [(c["id"], c["name"]) for c in calls] == [("a", "f"), ("b", "g")]


def test_un_evento_de_error_levanta_con_el_mensaje() -> None:
    with pytest.raises(RuntimeError, match="overloaded_error: Sobrecargado"):
        collect(
            [{"type": "error", "error": {"type": "overloaded_error", "message": "Sobrecargado"}}]
        )


# ----------------------------------------------------------- cliente HTTP
def sse(*events: dict) -> bytes:
    return "".join(f"event: {e['type']}\ndata: {json.dumps(e)}\n\n" for e in events).encode()


def client_with(handler) -> AnthropicChatClient:
    return AnthropicChatClient(
        api_key="CLAVE-123",
        base_url="https://api.anthropic.com/v1",
        default_headers={"User-Agent": "EzAgent-Voice/0.1"},
        transport=httpx.MockTransport(handler),
    )


def test_cuerpo_de_la_peticion() -> None:
    client = client_with(lambda r: httpx.Response(200))
    body = client.build_body(
        {
            "model": "claude-sonnet-5-5",
            "max_tokens": 300,
            "temperature": 0.2,
            "messages": [{"role": "system", "content": "sys"}, {"role": "user", "content": "hola"}],
            "tools": [
                {"type": "function", "function": {"name": "f", "parameters": {"type": "object"}}}
            ],
            "stream": True,
            "extra_body": {"reasoning_effort": "none"},
        }
    )
    assert body["model"] == "claude-sonnet-5-5" and body["max_tokens"] == 300
    assert body["system"] == "sys" and body["temperature"] == 0.2 and body["stream"] is True
    assert body["tool_choice"] == {"type": "auto"} and body["tools"][0]["name"] == "f"
    assert "extra_body" not in body and "reasoning_effort" not in json.dumps(body)


def test_cuerpo_sin_herramientas_ni_temperatura_ni_system() -> None:
    body = client_with(lambda r: httpx.Response(200)).build_body(
        {"model": "m", "messages": [{"role": "user", "content": "x"}], "max_tokens": None}
    )
    assert "tools" not in body and "tool_choice" not in body
    assert "temperature" not in body and "system" not in body and body["max_tokens"] == 1024


def test_stream_completo_con_cabeceras_correctas() -> None:
    seen: dict = {}

    def handler(request: httpx.Request) -> httpx.Response:
        seen["url"] = str(request.url)
        seen["headers"] = dict(request.headers)
        seen["body"] = json.loads(request.content)
        return httpx.Response(
            200,
            content=sse(
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
                {"type": "message_stop"},
            ),
            headers={"content-type": "text/event-stream"},
        )

    stream = client_with(handler).chat.completions.create(
        model="m", messages=[{"role": "user", "content": "hi"}], stream=True, max_tokens=50
    )
    try:
        pieces = [c.choices[0].delta.content for c in stream]
    finally:
        stream.close()
    assert pieces == ["Hola"]
    assert seen["url"] == "https://api.anthropic.com/v1/messages"
    assert seen["headers"]["x-api-key"] == "CLAVE-123"
    assert seen["headers"]["anthropic-version"] == "2023-06-01"
    assert seen["headers"]["user-agent"] == "EzAgent-Voice/0.1"
    assert seen["body"]["messages"][0]["content"][0]["text"] == "hi"


@pytest.mark.parametrize("status", [401, 404, 429, 500])
def test_un_error_http_levanta_con_el_codigo_y_sin_la_clave(status: int) -> None:
    client = client_with(lambda r: httpx.Response(status, text='{"error": "malo"}'))
    with pytest.raises(RuntimeError, match=f"respondió {status}") as info:
        client.chat.completions.create(model="m", messages=[{"role": "user", "content": "x"}])
    assert "CLAVE-123" not in str(info.value)
