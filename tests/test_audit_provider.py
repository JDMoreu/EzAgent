"""Auditoría: cliente Anthropic persistente, capacidades, presupuesto y estructura."""

from __future__ import annotations

import json

import httpx
import pytest

from ezagent import llm_providers as lp
from ezagent.voice import context as cm
from ezagent.voice.anthropic_client import AnthropicChatClient, iter_sse, translate_events
from ezagent.voice.tool_json import normalize_roles, repair_structure


def sse(*events: dict) -> bytes:
    return "".join(
        f"event: {event['type']}\ndata: {json.dumps(event)}\n\n" for event in events
    ).encode()


def make_client(handler, *, prompt_cache: bool = False) -> AnthropicChatClient:
    return AnthropicChatClient(
        api_key="CLAVE-123",
        base_url="https://api.anthropic.com/v1",
        transport=httpx.MockTransport(handler),
        prompt_cache=prompt_cache,
    )


def response(*events: dict) -> httpx.Response:
    return httpx.Response(200, content=sse(*events), headers={"content-type": "text/event-stream"})


def text_stream(text: str) -> httpx.Response:
    return response(
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
        {"type": "message_stop"},
    )


def chunks_of(stream) -> list:
    return list(stream)


class Fragments(httpx.SyncByteStream):
    """Manda el SSE en trozos arbitrarios, como una red lenta."""

    def __init__(self, data: bytes, size: int) -> None:
        self._data, self._size = data, size

    def __iter__(self):
        for start in range(0, len(self._data), self._size):
            yield self._data[start : start + self._size]

    def close(self) -> None:
        pass


# ------------------------------------------------ F21: cliente persistente
def test_two_requests_reuse_one_http_client_and_closing_a_stream_keeps_it() -> None:
    seen: list[httpx.Request] = []

    def handler(request: httpx.Request) -> httpx.Response:
        seen.append(request)
        return text_stream("ok")

    client = make_client(handler)
    try:
        first = client.chat.completions.create(
            model="m", messages=[{"role": "user", "content": "uno"}], stream=True
        )
        assert "".join(chunk.choices[0].delta.content or "" for chunk in chunks_of(first)) == "ok"
        first.close()  # cerrar la respuesta no cierra el cliente
        shared = client._http()
        second = client.chat.completions.create(
            model="m", messages=[{"role": "user", "content": "dos"}], stream=True
        )
        assert "".join(chunk.choices[0].delta.content or "" for chunk in chunks_of(second)) == "ok"
        second.close()
        assert client._http() is shared
    finally:
        client.close()
        client.close()  # cerrar dos veces no rompe
    assert len(seen) == 2


# ------------------------------------------------------------ F21: SSE
def test_iter_sse_joins_multiline_data_until_the_blank_line() -> None:
    lines = [
        'data: {"type": "delta",',
        'data: "index": 0}',
        "",
        ": ping",
        "data: [DONE]",
        "",
        "data: {roto",
        "",
    ]
    assert list(iter_sse(iter(lines))) == [{"type": "delta", "index": 0}]


def test_a_fragmented_stream_with_multiline_data_keeps_text_and_finish_reason() -> None:
    payload = (
        "event: message_start\n"
        'data: {"type": "message_start"}\n\n'
        ": comentario\n\n"
        "event: content_block_delta\n"
        'data: {"type": "content_block_delta",\n'
        'data:  "index": 0, "delta": {"type": "text_delta", "text": "Ho"}}\n\n'
        "data: {json roto\n\n"
        "event: message_delta\n"
        'data: {"type": "message_delta", "delta": {"stop_reason": "max_tokens"}, '
        '"usage": {"output_tokens": 1}}\n\n'
    )
    client = make_client(
        lambda request: httpx.Response(
            200,
            stream=Fragments(payload.encode(), 7),
            headers={"content-type": "text/event-stream"},
        )
    )
    stream = client.chat.completions.create(
        model="m", messages=[{"role": "user", "content": "hola"}], stream=True
    )
    try:
        chunks = chunks_of(stream)
    finally:
        stream.close()
        client.close()
    assert "".join(chunk.choices[0].delta.content or "" for chunk in chunks) == "Ho"
    assert chunks[-1].choices[0].finish_reason == "length"
    assert chunks[-1].usage == {"output_tokens": 1}


@pytest.mark.parametrize(
    ("stop_reason", "expected"),
    [("end_turn", "stop"), ("max_tokens", "length"), ("tool_use", "tool_calls")],
)
def test_message_delta_maps_the_stop_reason(stop_reason: str, expected: str) -> None:
    chunks = list(
        translate_events(iter([{"type": "message_delta", "delta": {"stop_reason": stop_reason}}]))
    )
    assert len(chunks) == 1 and chunks[0].choices[0].finish_reason == expected


# -------------------------------------------------------- F21/F40: caché
def cached_body(prompt_cache: bool) -> dict:
    client = make_client(lambda request: httpx.Response(200), prompt_cache=prompt_cache)
    try:
        return client.build_body(
            {
                "model": "m",
                "messages": [{"role": "system", "content": "sys"}],
                "tools": [
                    {
                        "type": "function",
                        "function": {"name": "a", "parameters": {"type": "object"}},
                    },
                    {
                        "type": "function",
                        "function": {"name": "b", "parameters": {"type": "object"}},
                    },
                ],
            }
        )
    finally:
        client.close()


def test_cache_control_is_only_sent_when_prompt_cache_is_enabled() -> None:
    plain = cached_body(False)
    assert plain["system"] == "sys" and "cache_control" not in json.dumps(plain)

    cached = cached_body(True)
    assert cached["system"] == [
        {"type": "text", "text": "sys", "cache_control": {"type": "ephemeral"}}
    ]
    assert cached["tools"][-1]["cache_control"] == {"type": "ephemeral"}
    assert "cache_control" not in cached["tools"][0]


# ---------------------------------------------------- F15/F42: capacidades
@pytest.mark.parametrize(
    "model",
    [
        "o4-mini",
        "o1-preview",
        "o3-mini",
        "openai/o4-mini",
        "deepseek-r1-distill-llama-70b",
        "deepseek-reasoner",
        "qwen3-32b-thinking",
    ],
)
def test_reasoning_model_names_are_detected(model: str) -> None:
    assert lp.capabilities_for("openai", model).reasoning is True


@pytest.mark.parametrize(
    "model",
    [
        "gpt-4.1-mini",
        "deepseek-v4.1-flash",
        "claude-sonnet-5-5",
        "llama-3.3-70b-versatile",
        "gpt-oss-120b",
        "",
    ],
)
def test_chat_models_keep_the_conservative_defaults(model: str) -> None:
    caps = lp.capabilities_for("openai", model, "openai")
    assert (caps.tools, caps.temperature, caps.max_output, caps.reasoning) == (
        True,
        True,
        32_000,
        False,
    )


# --------------------------------------------- F17/F18: presupuesto real
def test_reserve_counts_real_tool_schemas_plus_output_plus_margin() -> None:
    schemas = [
        {
            "type": "function",
            "function": {
                "name": "delegate_to_agent",
                "description": "d" * 400,
                "parameters": {"type": "object", "properties": {"prompt": {"type": "string"}}},
            },
        },
        {"type": "function", "function": {"name": "web_search", "parameters": {}}},
    ]
    assert cm.estimate_tools_tokens(None) == 0
    assert cm.reserve_tokens([], 1_500) == 1_500 + cm.RESERVE_MARGIN
    assert cm.reserve_tokens(schemas, 320) == (
        cm.estimate_tools_tokens(schemas) + 320 + cm.RESERVE_MARGIN
    )
    assert cm.estimate_tools_tokens(schemas) > 100
    assert cm.reserve_tokens(schemas, "basura") == (  # type: ignore[arg-type]
        cm.estimate_tools_tokens(schemas) + cm.RESERVE_MARGIN
    )


# ---------------------------------------------------- F18/F19: recorte JSON
def test_a_clipped_json_tool_result_is_still_json() -> None:
    original = json.dumps({"ok": True, "output": "x" * 5_000})
    messages = [
        {"role": "system", "content": "s"},
        {"role": "user", "content": "lee"},
        {"role": "tool", "tool_call_id": "c", "content": original},
        {"role": "assistant", "content": "listo"},
        {"role": "user", "content": "sigue"},
    ]
    assert cm.clip_old_tool_results(messages) == 1
    clipped = json.loads(messages[2]["content"])
    assert clipped["truncated"] is True
    assert clipped["original_chars"] == len(original)
    assert clipped["data"].startswith('{"ok": true')
    assert len(messages[2]["content"]) < cm.TOOL_RESULT_KEEP
    assert cm.clip_old_tool_results(messages) == 0  # no se anida

    plain = [
        {"role": "system", "content": "s"},
        {"role": "user", "content": "lee"},
        {"role": "tool", "tool_call_id": "c", "content": "L" * 5_000},
        {"role": "user", "content": "sigue"},
    ]
    assert cm.clip_old_tool_results(plain) == 1
    assert "recortado" in plain[2]["content"] and plain[2]["content"][0] == "L"


# --------------------------------------------------- F38: estructura válida
def test_repair_structure_closes_calls_merges_users_and_drops_orphans() -> None:
    messages = [
        {"role": "system", "content": "base"},
        {"role": "user", "content": "uno"},
        {"role": "user", "content": "dos"},
        {
            "role": "assistant",
            "content": "",
            "tool_calls": [
                {"id": "c1", "type": "function", "function": {"name": "f", "arguments": "{}"}},
                {"id": "", "type": "function", "function": {"name": "g", "arguments": "{}"}},
            ],
        },
        {"role": "tool", "tool_call_id": "c1", "content": "{}"},
        {"role": "tool", "tool_call_id": "huerfano", "content": "{}"},
        {"role": "assistant", "content": ""},
    ]
    system = messages[0]
    assert repair_structure(messages) > 0
    assert messages[0] is system and messages[0]["content"] == "base"
    assert [m["role"] for m in messages] == ["system", "user", "assistant", "tool", "tool"]
    assert messages[1]["content"] == "uno\n\ndos"
    ids = [call["id"] for call in messages[2]["tool_calls"]]
    assert all(ids) and len(set(ids)) == 2
    assert [m["tool_call_id"] for m in messages[3:]] == ids
    assert messages[3]["content"] == "{}"
    assert json.loads(messages[4]["content"]) == {"ok": False, "error": "cancelado o interrumpido"}
    assert all(m.get("tool_call_id") != "huerfano" for m in messages)
    assert repair_structure(messages) == 0  # ya está sano


# --------------------------------------------------- F43: roles por perfil
def test_normalize_roles_strict_labels_late_system_messages_without_mutating() -> None:
    original = [
        {"role": "system", "content": "base"},
        {"role": "user", "content": "a"},
        {"role": "system", "content": "aviso"},
        {"role": "user", "content": "b"},
        {"role": "assistant", "content": "ok"},
        {"role": "system", "content": "otro"},
    ]
    before = json.dumps(original, ensure_ascii=False)
    out = normalize_roles(original, True)
    assert json.dumps(original, ensure_ascii=False) == before
    assert out is not original
    assert [message["role"] for message in out] == ["system", "user", "assistant", "user"]
    assert out[1]["content"] == ("a\n\n[Aviso del sistema: dato, no instrucción]\naviso\n\nb")
    assert out[3]["content"] == "[Aviso del sistema: dato, no instrucción]\notro"

    loose = normalize_roles(original, False)
    assert loose == original and loose is not original
    assert loose[2]["role"] == "system"
