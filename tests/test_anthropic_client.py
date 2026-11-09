"""Cliente compatible con Anthropic: conversión, streaming y bucle del Messenger."""

from __future__ import annotations

import json
from types import SimpleNamespace

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
from ezagent.voice.config import VoiceConfig
from ezagent.voice.llm import Messenger


def _sse(*events: dict) -> bytes:
    lines = []
    for event in events:
        lines.append(f"event: {event['type']}")
        lines.append("data: " + json.dumps(event))
        lines.append("")
    return ("\n".join(lines) + "\n").encode("utf-8")


TEXT_REPLY = _sse(
    {"type": "message_start", "message": {"id": "m1"}},
    {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}},
    {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Son "}},
    {"type": "ping"},
    {
        "type": "content_block_delta",
        "index": 0,
        "delta": {"type": "text_delta", "text": "las diez."},
    },
    {"type": "content_block_stop", "index": 0},
    {"type": "message_delta", "delta": {"stop_reason": "end_turn"}},
    {"type": "message_stop"},
)

TOOL_REPLY = _sse(
    {"type": "message_start", "message": {"id": "m0"}},
    {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}},
    {
        "type": "content_block_delta",
        "index": 0,
        "delta": {"type": "text_delta", "text": "Va, lo reviso."},
    },
    {"type": "content_block_stop", "index": 0},
    {
        "type": "content_block_start",
        "index": 1,
        "content_block": {"type": "tool_use", "id": "toolu_1", "name": "get_datetime", "input": {}},
    },
    {
        "type": "content_block_delta",
        "index": 1,
        "delta": {"type": "input_json_delta", "partial_json": '{"tz": '},
    },
    {
        "type": "content_block_delta",
        "index": 1,
        "delta": {"type": "input_json_delta", "partial_json": '"local"}'},
    },
    {"type": "content_block_stop", "index": 1},
    {"type": "message_delta", "delta": {"stop_reason": "tool_use"}},
    {"type": "message_stop"},
)


def test_url_de_messages_con_y_sin_version() -> None:
    assert messages_url("https://api.anthropic.com/v1") == "https://api.anthropic.com/v1/messages"
    assert messages_url("https://api.anthropic.com/v1/") == "https://api.anthropic.com/v1/messages"
    assert messages_url("https://api.minimax.io/anthropic") == (
        "https://api.minimax.io/anthropic/v1/messages"
    )


def test_conversion_de_mensajes_y_herramientas() -> None:
    system, messages = convert_messages(
        [
            {"role": "system", "content": "Eres EzAgent."},
            {"role": "user", "content": "¿Hora?"},
            {
                "role": "assistant",
                "content": "Va.",
                "tool_calls": [
                    {
                        "id": "c1",
                        "type": "function",
                        "function": {"name": "get_datetime", "arguments": '{"tz": "local"}'},
                    },
                    {"id": "c2", "type": "function", "function": {"name": "x", "arguments": ""}},
                ],
            },
            {"role": "tool", "tool_call_id": "c1", "content": "10:00"},
            {"role": "tool", "tool_call_id": "c2", "content": "ok"},
            {"role": "system", "content": "El agente terminó."},
            {"role": "assistant", "content": ""},
        ]
    )
    assert system == "Eres EzAgent."
    assert [m["role"] for m in messages] == ["user", "assistant", "user"]
    tool_use = messages[1]["content"][1]
    assert tool_use == {
        "type": "tool_use",
        "id": "c1",
        "name": "get_datetime",
        "input": {"tz": "local"},
    }
    assert messages[1]["content"][2]["input"] == {}
    # Los dos resultados y el aviso del sistema quedan en un solo turno.
    results = messages[2]["content"]
    assert [b["type"] for b in results] == ["tool_result", "tool_result", "text"]
    assert "El agente terminó." in results[2]["text"]

    tools = convert_tools(
        [
            {
                "type": "function",
                "function": {"name": "t", "description": "d", "parameters": {"type": "object"}},
            }
        ]
    )
    assert tools == [{"name": "t", "description": "d", "input_schema": {"type": "object"}}]


def test_traduccion_del_streaming() -> None:
    lines = TOOL_REPLY.decode("utf-8").splitlines()
    chunks = list(translate_events(iter_sse(iter(lines))))
    text = "".join(c.choices[0].delta.content or "" for c in chunks)
    assert text == "Va, lo reviso."
    calls = [call for c in chunks for call in (c.choices[0].delta.tool_calls or [])]
    assert calls[0].index == 0 and calls[0].id == "toolu_1"
    assert calls[0].function.name == "get_datetime"
    assert "".join(call.function.arguments for call in calls) == '{"tz": "local"}'


def test_herramienta_sin_argumentos_entrega_json_vacio() -> None:
    events = [
        {
            "type": "content_block_start",
            "index": 0,
            "content_block": {"type": "tool_use", "id": "t", "name": "n"},
        },
        {"type": "content_block_stop", "index": 0},
    ]
    calls = [c.choices[0].delta.tool_calls[0] for c in translate_events(iter(events))]
    assert "".join(call.function.arguments for call in calls) == "{}"


def test_error_en_el_streaming_se_propaga() -> None:
    events = [{"type": "error", "error": {"type": "overloaded_error", "message": "Overloaded"}}]
    with pytest.raises(RuntimeError, match="overloaded_error"):
        list(translate_events(iter(events)))


def _client(responses: list[bytes], seen: list[dict], status: int = 200) -> AnthropicChatClient:
    def handler(request: httpx.Request) -> httpx.Response:
        seen.append(
            {
                "headers": dict(request.headers),
                "body": json.loads(request.content),
                "url": str(request.url),
            }
        )
        return httpx.Response(status, content=responses.pop(0))

    return AnthropicChatClient(
        api_key="sk-test",
        base_url="https://api.example.com/anthropic",
        transport=httpx.MockTransport(handler),
    )


def test_respuesta_http_con_error_no_filtra_la_clave() -> None:
    seen: list[dict] = []
    client = _client([b'{"error": "bad"}'], seen, status=401)
    with pytest.raises(RuntimeError, match="401") as info:
        client.chat.completions.create(model="m", messages=[{"role": "user", "content": "hola"}])
    assert "sk-test" not in str(info.value)
    assert seen[0]["headers"]["x-api-key"] == "sk-test"
    assert seen[0]["url"] == "https://api.example.com/anthropic/v1/messages"


def test_messenger_completo_con_api_anthropic() -> None:
    class Runtime:
        def begin_turn(self, text): ...
        def system_prompt(self):
            return "sistema"

        def tools_for(self, text):
            return [
                {
                    "type": "function",
                    "function": {"name": "get_datetime", "parameters": {"type": "object"}},
                }
            ]

        def execute(self, name, arguments):
            assert json.loads(arguments) == {"tz": "local"}
            return "10:00"

        def render_result(self, result):
            return str(result)

        def record_assistant(self, content): ...

    cfg = VoiceConfig(llm_provider="custom", llm_model="mi-modelo", llm_api_style="anthropic")
    messenger = Messenger(
        cfg,
        SimpleNamespace(list_agents=lambda: []),
        last_summary=lambda: "",
        watch=lambda address: None,
        mute=lambda minutes: None,
        request_close=lambda: None,
        mic_mute=lambda muted: None,
        runtime=Runtime(),
    )
    seen: list[dict] = []
    messenger.client = _client([TOOL_REPLY, TEXT_REPLY], seen)
    answer = "".join(messenger.ask_stream("¿qué hora es?"))
    assert answer == "Va, lo reviso. Son las diez."
    first, second = seen[0]["body"], seen[1]["body"]
    assert first["system"] == "sistema"
    assert first["model"] == "mi-modelo"
    assert first["tools"][0]["name"] == "get_datetime"
    assert second["messages"][1]["content"][1]["type"] == "tool_use"
    assert second["messages"][2]["content"][0] == {
        "type": "tool_result",
        "tool_use_id": "toolu_1",
        "content": "10:00",
    }
