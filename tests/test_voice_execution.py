"""Turn budgets and final delivery, with no live network or delegated processes."""

from __future__ import annotations

import json

import pytest
from test_messenger import Client, Runtime, call, make, say, text, tool_round


class BudgetRuntime(Runtime):
    def __init__(self, limit: int = 4) -> None:
        super().__init__()
        self.limit, self.used = limit, 0

    def begin_turn(self, text: str) -> None:
        super().begin_turn(text)
        self.used = 0

    def remaining_tool_calls(self) -> int:
        return max(0, self.limit - self.used)

    def execute(self, name, raw):
        assert self.used < self.limit
        self.used += 1
        return super().execute(name, raw)


def test_four_real_calls_get_a_final_answer_and_a_last_write_is_not_reported_as_a_loop() -> None:
    runtime = BudgetRuntime()
    client = Client(
        [
            call("web_search", '{"query":"a"}', call_id="1"),
            call("fetch_page", '{"url":"https://a"}', index=1, call_id="2"),
        ],
        [
            call("fetch_page", '{"url":"https://b"}', call_id="3"),
            call("write_file", '{"path":"table.md"}', index=1, call_id="4"),
        ],
        [text("La tabla está creada en table.md.")],
    )
    messenger = make(client, runtime)
    spoken = say(messenger, "Prepara una tabla.")
    assert runtime.used == runtime.limit == 4
    assert "Quedan 4 llamadas" in client.requests[0]["messages"][-1]["content"]
    assert "Quedan 2 llamadas" in client.requests[1]["messages"][-1]["content"]
    assert len(client.requests) == 3 and "tools" not in client.requests[-1]
    assert spoken.endswith("table.md.") and "dando vueltas" not in spoken
    assert messenger.messages[-1]["content"] == "La tabla está creada en table.md."
    assert runtime.recorded == ["La tabla está creada en table.md."]


def test_oversized_tool_batch_does_not_execute_beyond_the_configured_budget() -> None:
    runtime = BudgetRuntime(1)
    client = Client(
        [call("write_file", "{}", call_id="1"), call("write_file", "{}", index=1, call_id="2")],
        [text("Solo se completó la primera acción.")],
    )
    messenger = make(client, runtime)
    say(messenger)
    assert len(runtime.executed) == 1
    tools = [m for m in messenger.messages if m["role"] == "tool"]
    assert json.loads(tools[-1]["content"])["ok"] is False
    assert len(client.requests) == 2 and "tools" not in client.requests[-1]


def test_read_cache_is_canonical_and_lasts_only_one_turn() -> None:
    runtime = BudgetRuntime(10)
    client = Client(
        tool_round("web_search", '{"query":"a","max_results":5}', "1"),
        tool_round("web_search", '{"max_results":5, "query":"a"}', "2"),
        [text("Respuesta.")],
        tool_round("web_search", '{"query":"a","max_results":5}', "3"),
        [text("Actualizado.")],
    )
    messenger = make(client, runtime)
    say(messenger)
    assert len(runtime.executed) == 1
    say(messenger)
    assert len(runtime.executed) == 2


def test_mutating_tools_are_never_cached_and_round_limit_reserves_a_final_response() -> None:
    runtime = BudgetRuntime(10)
    client = Client(
        tool_round("write_file", "{}", "1"),
        tool_round("write_file", "{}", "2"),
        [text("Terminado.")],
    )
    say(make(client, runtime, llm_max_tool_rounds=2))
    assert len(runtime.executed) == 2 and "tools" not in client.requests[-1]


def test_a_provider_ignoring_the_finalization_cannot_run_more_tools() -> None:
    runtime = BudgetRuntime(1)
    client = Client(tool_round("write_file", "{}", "1"), tool_round("write_file", "{}", "2"))
    messenger = make(client, runtime)
    spoken = say(messenger)
    assert len(runtime.executed) == 1 and "entrega final confirmada" in spoken
    assert messenger.messages[-1]["role"] == "assistant"


def test_an_empty_final_answer_is_recorded_as_incomplete_instead_of_silent_success() -> None:
    messenger = make(Client(tool_round("write_file", "{}", "1"), []), BudgetRuntime(1))
    answer = say(messenger)
    assert "no pude completar" in answer and messenger.messages[-1]["content"] in answer


@pytest.mark.parametrize("style", ["openai", "anthropic"])
def test_both_clients_receive_the_configured_idle_timeout(monkeypatch, style: str) -> None:
    from ezagent.voice import llm

    options = []

    def client(**kwargs):
        options.append(kwargs)
        return object()

    monkeypatch.setattr(llm, "OpenAI", client)
    monkeypatch.setattr(llm, "AnthropicChatClient", client)
    make(Client(), llm_api_style=style, llm_timeout_secs=12)
    assert options[0]["timeout"] == 12


@pytest.mark.parametrize("timeout", [None, "bad", float("nan"), 0, 10000])
def test_invalid_timeouts_and_round_limits_fall_back_safely(timeout) -> None:
    messenger = make(Client(), llm_timeout_secs=timeout, llm_max_tool_rounds="bad")
    assert messenger._request_timeout() == 30 and messenger._tool_round_limit() == 8


def test_json_recovery_keeps_the_budget_note_but_removes_poisoned_tool_history() -> None:
    from test_context_and_json import USER_ERROR

    client = Client(Exception(USER_ERROR), Exception(USER_ERROR), [text("Recuperado.")])
    messenger = make(client, BudgetRuntime())
    messenger.messages += [
        {"role": "user", "content": "antes"},
        {
            "role": "assistant",
            "content": "",
            "tool_calls": [
                {
                    "id": "old",
                    "type": "function",
                    "function": {"name": "read_file", "arguments": "{}"},
                },
            ],
        },
        {"role": "tool", "tool_call_id": "old", "content": "datos"},
    ]
    assert say(messenger).endswith("Recuperado.")
    final_request = client.requests[-1]["messages"]
    assert not any(m.get("tool_calls") or m["role"] == "tool" for m in final_request)
    assert "Quedan 4 llamadas" in final_request[-1]["content"]
