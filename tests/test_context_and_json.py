"""Memoria de contexto y JSON de herramientas: la conversación no se atasca ni se llena."""

from __future__ import annotations

import json
from types import SimpleNamespace

import pytest
from test_messenger import Client, Runtime, call, empty_chunk, make, say, text

from ezagent.voice import context as cm
from ezagent.voice.llm import TOOL_MAX_TOKENS, friendly_llm_error
from ezagent.voice.tool_json import (
    close_json,
    is_tool_json_error,
    repair_arguments,
    sanitize_history,
)

USER_ERROR = (
    "Error code: 500 - Failed to parse tool call arguments as JSON: "
    "[json.exception.parse_error.101] parse error at line 1, column 87: syntax error "
    "while parsing object - unexpected end of input; expected '}'"
)
SAMPLE = {
    "action": "run",
    "agent": "claude",
    "prompt": 'Revisa el repositorio, "cuidado" con \\rutas\\ y acentos: ñandú ☕. Luego resume.',
    "options": {"background": False, "tags": ["a", "b", {"x": 1}], "n": 3.5},
    "empty": "",
}


# ------------------------------------------------------------- JSON roto
def test_valid_and_empty_arguments_pass_through() -> None:
    raw = json.dumps(SAMPLE)
    assert repair_arguments(raw) == (raw, True)
    assert repair_arguments("") == ("{}", True)
    assert repair_arguments("   ") == ("{}", True)
    assert repair_arguments(None) == ("{}", True)  # herramientas sin argumentos


def test_the_exact_truncation_from_the_report_is_repaired_and_flagged() -> None:
    cut = '{"action":"run","agent":"claude","prompt":"Revisa el repositorio y resume el estado de'
    fixed, complete = repair_arguments(cut)
    assert complete is False
    parsed = json.loads(fixed)
    assert parsed["action"] == "run" and parsed["prompt"].startswith("Revisa el repositorio")


def test_every_possible_truncation_gives_valid_json() -> None:
    """Sea cual sea el punto donde se corte la respuesta, lo que se guarda es JSON válido."""
    raw = json.dumps(SAMPLE, ensure_ascii=False)
    for end in range(0, len(raw)):
        fixed, complete = repair_arguments(raw[:end])
        assert isinstance(json.loads(fixed), dict), (end, raw[:end])
        if end < len(raw) and raw[:end].strip():
            assert complete is False or json.loads(raw[:end]) is not None
    assert repair_arguments(raw)[1] is True


@pytest.mark.parametrize(
    "cut",
    [
        '{"a": 1, "b"',
        '{"a": 1,',
        '{"a":',
        '{"a": [1, 2',
        '{"a": {"b": "x',
        '{"a": "esc\\',
        '{"a": "q\\"',
    ],
)
def test_awkward_cut_points(cut: str) -> None:
    fixed, complete = repair_arguments(cut)
    assert isinstance(json.loads(fixed), dict) and complete is False


@pytest.mark.parametrize("garbage", ["no es json", "[1, 2", "null", "123", '"texto"', "}{"])
def test_hopeless_or_non_object_arguments_become_an_empty_object(garbage: str) -> None:
    fixed, complete = repair_arguments(garbage)
    assert json.loads(fixed) == {} and complete is False


def test_close_json_balances_strings_and_brackets() -> None:
    assert json.loads(close_json('{"a": ["x", {"b": "y')) == {"a": ["x", {"b": "y"}]}
    assert close_json('{"a": 1}') == '{"a": 1}'


def test_sanitize_history_fixes_only_what_is_broken() -> None:
    good = {"id": "1", "type": "function", "function": {"name": "a", "arguments": '{"x": 1}'}}
    bad = {"id": "2", "type": "function", "function": {"name": "b", "arguments": '{"x": "cort'}}
    empty = {"id": "3", "type": "function", "function": {"name": "c", "arguments": ""}}
    messages = [
        {"role": "system", "content": "s"},
        {"role": "user", "content": "u"},
        {"role": "assistant", "content": "", "tool_calls": [good, bad, empty]},
        {"role": "tool", "tool_call_id": "1", "content": "{}"},
    ]
    assert sanitize_history(messages) == 2
    assert good["function"]["arguments"] == '{"x": 1}'
    assert json.loads(bad["function"]["arguments"]) == {"x": "cort"}
    assert empty["function"]["arguments"] == "{}"
    assert sanitize_history(messages) == 0  # ya está sano: no toca nada más


def test_the_server_error_is_recognized() -> None:
    assert is_tool_json_error(Exception(USER_ERROR))
    assert not is_tool_json_error(Exception("Error code: 500 - Internal error"))
    assert not is_tool_json_error(Exception("Error code: 429 - rate limit"))


def test_the_friendly_message_explains_it_in_plain_language() -> None:
    exc = Exception(USER_ERROR)
    exc.status_code = 500  # type: ignore[attr-defined]
    message = friendly_llm_error(exc, "Personalizado", "modelo")
    assert "llamada a herramienta" in message and "vuelve a intentarlo" in message
    assert "json.exception" not in message


# --------------------------------------------- el mensajero: no se atasca
def truncated_call(arguments: str, name: str = "delegate_to_agent"):
    last = SimpleNamespace(
        choices=[
            SimpleNamespace(
                delta=SimpleNamespace(content=None, tool_calls=None), finish_reason="length"
            )
        ]
    )
    return [call(name, arguments, call_id="c1"), last]


def test_a_cut_tool_call_is_not_executed_and_the_history_stays_valid() -> None:
    cut = '{"action": "run", "prompt": "revisa el repo y'
    runtime = Runtime()
    client = Client(truncated_call(cut), [text("Lo intento de nuevo, más corto.")])
    messenger = make(client, runtime)
    spoken = say(messenger, "lanza un agente")
    assert runtime.executed == []  # no se ejecuta con medio argumento
    assert "más corto" in spoken
    assistant = next(m for m in messenger.messages if m.get("tool_calls"))
    assert json.loads(assistant["tool_calls"][0]["function"]["arguments"])["action"] == "run"
    tool = next(m for m in messenger.messages if m["role"] == "tool")
    assert "incompletos" in tool["content"] and '"ok": false' in tool["content"]
    # el modelo ve el error y la siguiente petición no lleva JSON roto
    second = client.requests[1]["messages"]
    for message in second:
        for tool_call in message.get("tool_calls") or []:
            json.loads(tool_call["function"]["arguments"])


def test_a_complete_tool_call_still_runs_normally() -> None:
    runtime = Runtime()
    client = Client([call("get_datetime", '{"tz": "UTC"}', call_id="c1")], [text("Listo.")])
    say(make(client, runtime), "hora")
    assert runtime.executed == [("get_datetime", '{"tz": "UTC"}')]


def test_a_poisoned_history_is_repaired_before_the_request() -> None:
    client = Client([text("Ya estoy bien.")])
    messenger = make(client)
    messenger.messages += [
        {"role": "user", "content": "viejo"},
        {
            "role": "assistant",
            "content": "",
            "tool_calls": [
                {
                    "id": "x",
                    "type": "function",
                    "function": {"name": "t", "arguments": '{"a": "cort'},
                }
            ],
        },
        {"role": "tool", "tool_call_id": "x", "content": "{}"},
        {"role": "assistant", "content": "hecho"},
    ]
    assert say(messenger, "sigamos") == "Ya estoy bien."
    for message in client.requests[0]["messages"]:
        for tool_call in message.get("tool_calls") or []:
            json.loads(tool_call["function"]["arguments"])


def test_if_the_server_still_rejects_it_the_request_is_retried_after_repairing() -> None:
    client = Client(Exception(USER_ERROR), [text("Ahora sí.")])
    messenger = make(client)
    assert say(messenger, "hola") == "Ahora sí."
    assert len(client.requests) == 2


def test_as_a_last_resort_the_old_tool_calls_are_dropped_from_the_history() -> None:
    client = Client(Exception(USER_ERROR), Exception(USER_ERROR), [text("Por fin.")])
    messenger = make(client)
    messenger.messages += [
        {"role": "user", "content": "antes"},
        {
            "role": "assistant",
            "content": "voy",
            "tool_calls": [
                {"id": "x", "type": "function", "function": {"name": "t", "arguments": "{}"}}
            ],
        },
        {"role": "tool", "tool_call_id": "x", "content": "{}"},
        {"role": "assistant", "content": "hecho"},
    ]
    assert say(messenger, "sigamos") == "Por fin."
    final = client.requests[2]["messages"]
    assert not any(m["role"] == "tool" or m.get("tool_calls") for m in final)
    assert any(m.get("content") == "antes" for m in final)  # lo conversado se conserva


def test_other_server_errors_are_not_swallowed() -> None:
    client = Client(Exception("Error code: 500 - algo distinto"))
    with pytest.raises(Exception, match="algo distinto"):
        say(make(client), "hola")
    assert len(client.requests) == 1


def test_with_tools_the_answer_has_room_and_without_them_it_keeps_the_voice_limit() -> None:
    with_tools, without = Client([text("a")]), Client([text("b")])
    say(make(with_tools), "x")
    say(make(without, Runtime(tools=[])), "x")
    assert with_tools.requests[0]["max_tokens"] == TOOL_MAX_TOKENS
    assert without.requests[0]["max_tokens"] == 150


# ------------------------------------------------------------ módulo context
def msg(role: str, content: str, **extra):
    return {"role": role, "content": content, **extra}


def convo(turns: int, size: int = 400):
    out = [msg("system", "sistema")]
    for i in range(turns):
        out += [
            msg("user", f"pregunta {i} " + "x" * size),
            msg("assistant", f"respuesta {i} " + "y" * size),
        ]
    return out


def test_token_estimate_grows_with_content_and_counts_tool_calls() -> None:
    small, big = cm.estimate_tokens(convo(2)), cm.estimate_tokens(convo(20))
    assert 0 < small < big and big > 8 * small
    plain = [msg("assistant", "")]
    with_call = [
        msg("assistant", "", tool_calls=[{"function": {"name": "x", "arguments": "a" * 3200}}])
    ]
    assert cm.estimate_tokens(with_call) > cm.estimate_tokens(plain) + 900


def test_budget_has_a_floor_and_tolerates_junk() -> None:
    assert cm.budget_tokens(200_000) == 200_000
    assert cm.budget_tokens(10) == cm.MIN_BUDGET
    assert cm.budget_tokens(None) == 32_000 and cm.budget_tokens("x") == 32_000


def test_compaction_triggers_at_seventy_percent_including_the_reserve() -> None:
    messages = convo(10)
    tokens = cm.estimate_tokens(messages)
    assert not cm.needs_compaction(messages, int(tokens / cm.COMPACT_AT) + 500)
    assert cm.needs_compaction(messages, int(tokens / cm.COMPACT_AT) - 500)
    budget = int(tokens / cm.COMPACT_AT) + 500
    assert cm.needs_compaction(messages, budget, reserve=2000)  # las herramientas también ocupan
    assert cm.needs_clipping(messages, int(tokens / cm.CLIP_AT) - 10)


def test_split_keeps_the_last_turns_starting_at_a_user_message() -> None:
    old, recent = cm.split_history(convo(8), keep_turns=3)
    assert [m["role"] for m in recent][0] == "user" and sum(
        m["role"] == "user" for m in recent
    ) == 3
    assert len(old) + len(recent) == 16
    assert old[0]["content"].startswith("pregunta 0")
    assert cm.split_history(convo(3), keep_turns=4) == ([], convo(3)[1:])


def test_split_never_separates_a_tool_call_from_its_result() -> None:
    history = [msg("system", "s")]
    for i in range(6):
        history += [
            msg("user", f"u{i}"),
            msg(
                "assistant",
                "",
                tool_calls=[{"id": f"c{i}", "function": {"name": "t", "arguments": "{}"}}],
            ),
            msg("tool", "r", tool_call_id=f"c{i}"),
            msg("assistant", f"a{i}"),
        ]
    old, recent = cm.split_history(history, keep_turns=2)
    assert recent[0]["role"] == "user" and old[-1]["role"] == "assistant"
    for part in (old, recent):
        for index, m in enumerate(part):
            if m["role"] == "tool":
                assert part[index - 1].get("tool_calls")


def test_the_transcript_for_the_summary_has_roles_tools_and_the_previous_summary() -> None:
    old = [
        msg("user", "lanza un agente"),
        msg(
            "assistant",
            "",
            tool_calls=[{"function": {"name": "delegate", "arguments": '{"a": 1}'}}],
        ),
        msg("tool", "x" * 900, tool_call_id="1"),
        msg("assistant", "Listo, está corriendo."),
    ]
    body = cm.render_transcript(old, "Antes hablamos de precios.")
    assert body.startswith("RESUMEN PREVIO:") and "Antes hablamos de precios." in body
    assert "Usuario: lanza un agente" in body and "[herramienta delegate" in body
    assert "Asistente: Listo, está corriendo." in body
    assert max(len(line) for line in body.splitlines()) < 400  # el resultado largo va recortado


def test_a_huge_transcript_keeps_the_end() -> None:
    old = [msg("user", f"mensaje {i} " + "z" * 200) for i in range(1000)]
    body = cm.render_transcript(old)
    assert (
        len(body) <= cm.TRANSCRIPT_CHARS + 10 and "mensaje 999" in body and "mensaje 0 " not in body
    )


def test_the_summary_request_and_the_prompt_with_summary() -> None:
    request = cm.summary_messages(convo(2)[1:], "previo")
    assert request[0]["role"] == "system" and "archivista" in request[0]["content"]
    assert request[1]["role"] == "user" and "previo" in request[1]["content"]
    assert cm.with_summary("PROMPT", "") == "PROMPT"
    combined = cm.with_summary("PROMPT", "- el usuario quiere X")
    assert combined.startswith("PROMPT") and "# Resumen de la conversación hasta ahora" in combined


def test_old_tool_results_are_clipped_but_the_current_turn_is_protected() -> None:
    long = "L" * 5000
    messages = [
        msg("system", "s"),
        msg("user", "u1"),
        msg("tool", long, tool_call_id="1"),
        msg("assistant", "a1"),
        msg("user", "u2"),
        msg("tool", long, tool_call_id="2"),
    ]
    assert cm.clip_old_tool_results(messages) == 1
    assert (
        len(messages[2]["content"]) < cm.TOOL_RESULT_KEEP + 60
        and "recortado" in messages[2]["content"]
    )
    assert messages[5]["content"] == long  # la vuelta actual no se toca
    assert cm.clip_old_tool_results(messages) == 0


# ------------------------------------------------- el compactador en acción
def big_history(messenger, turns: int = 10, size: int = 2500) -> None:
    for i in range(turns):
        messenger.messages += [
            msg("user", f"pregunta {i} " + "x" * size),
            msg("assistant", f"respuesta {i} " + "y" * size),
        ]


def test_a_full_conversation_is_summarised_instead_of_forgotten() -> None:
    client = Client([text("- Hablamos de precios y de un agente lanzado.")], [text("Seguimos.")])
    messenger = make(client, llm_context_tokens=8000)
    notes: list[str] = []
    messenger.on_context_event = notes.append
    big_history(messenger)
    assert say(messenger, "¿en qué íbamos?") == "Seguimos."
    summary_call, answer_call = client.requests
    assert "archivista" in summary_call["messages"][0]["content"]
    assert "pregunta 0" in summary_call["messages"][1]["content"]
    assert messenger.summary.startswith("- Hablamos de precios")
    system = answer_call["messages"][0]["content"]
    assert "Resumen de la conversación hasta ahora" in system and "Hablamos de precios" in system
    users = [m["content"] for m in answer_call["messages"] if m["role"] == "user"]
    assert any(u.startswith("pregunta 9") for u in users) and not any(
        u.startswith("pregunta 0") for u in users
    )
    assert users[-1] == "¿en qué íbamos?"
    assert len(notes) == 2 and "Compactando" in notes[0] and "compactada" in notes[1]


def test_the_summary_survives_the_next_turns_and_reset_clears_it() -> None:
    client = Client([text("RESUMEN-UNO")], [text("a")], [text("b")])
    messenger = make(client, llm_context_tokens=8000)
    big_history(messenger)
    say(messenger, "uno")
    say(messenger, "dos")
    third = client.requests[2]["messages"][0]["content"]
    assert "RESUMEN-UNO" in third  # la siguiente vuelta sigue sabiendo lo anterior
    messenger.reset_history()
    assert messenger.summary == "" and len(messenger.messages) == 1


def test_a_second_compaction_folds_the_previous_summary_in() -> None:
    client = Client([text("RESUMEN-1")], [text("a")], [text("RESUMEN-2")], [text("b")])
    messenger = make(client, llm_context_tokens=8000)
    big_history(messenger)
    say(messenger, "uno")
    big_history(messenger)
    say(messenger, "dos")
    second_request = client.requests[2]["messages"][1]["content"]
    assert second_request.startswith("RESUMEN PREVIO:") and "RESUMEN-1" in second_request
    assert messenger.summary == "RESUMEN-2"


def test_if_the_summary_fails_the_conversation_still_goes_on() -> None:
    client = Client(Exception("el modelo no responde"), [text("Aquí sigo.")])
    messenger = make(client, llm_context_tokens=8000)
    notes: list[str] = []
    messenger.on_context_event = notes.append
    big_history(messenger)
    assert say(messenger, "hola") == "Aquí sigo."
    assert messenger.summary == "" and "olvidó" in notes[-1]
    assert len(messenger.messages) < 14  # no queda nada que desborde la ventana


def test_a_short_conversation_is_never_compacted() -> None:
    client = Client([text("uno")], [text("dos")])
    messenger = make(client)  # sin llm_context_tokens: 32 000 por defecto
    notes: list[str] = []
    messenger.on_context_event = notes.append
    say(messenger, "a")
    say(messenger, "b")
    assert len(client.requests) == 2 and notes == [] and messenger.summary == ""


def test_a_bigger_window_postpones_compaction() -> None:
    small = Client([text("RES")], [text("ok")])
    large = Client([text("ok")])
    a, b = make(small, llm_context_tokens=8000), make(large, llm_context_tokens=200_000)
    big_history(a)
    big_history(b)
    say(a, "x")
    say(b, "x")
    assert len(small.requests) == 2 and len(large.requests) == 1
    assert len(b.messages) > 20  # con 200 k nada se pierde


def test_a_single_enormous_turn_is_clipped_not_summarised() -> None:
    client = Client([text("ok")])
    messenger = make(client, llm_context_tokens=8000)
    messenger.messages += [
        msg("user", "lee el archivo"),
        msg(
            "assistant",
            "",
            tool_calls=[
                {"id": "c", "type": "function", "function": {"name": "r", "arguments": "{}"}}
            ],
        ),
        msg("tool", "Q" * 60_000, tool_call_id="c"),
        msg("assistant", "es largo"),
    ]
    say(messenger, "¿y ahora?")
    assert len(client.requests) == 1  # no hay vueltas viejas que resumir
    tool = next(m for m in client.requests[0]["messages"] if m["role"] == "tool")
    assert len(tool["content"]) < 2000 and "recortado" in tool["content"]


def test_a_notification_callback_that_fails_does_not_break_the_turn() -> None:
    client = Client([text("RES")], [text("ok")])
    messenger = make(client, llm_context_tokens=8000)

    def boom(_text: str) -> None:
        raise RuntimeError("la ventana se cerró")

    messenger.on_context_event = boom
    big_history(messenger)
    assert say(messenger, "x") == "ok"


def test_the_request_marker_helpers_exist() -> None:
    assert empty_chunk().choices == []
