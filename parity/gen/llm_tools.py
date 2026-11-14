"""Fixtures del bucle de herramientas del `Messenger` (F1-05).

Los turnos usan un cliente guionizado (como `tests/test_messenger.py`): cada
llamada a `chat.completions.create` consume el siguiente guion. El reloj
monótono está congelado para que `last_rounds` («0 ms») sea reproducible.

Formatos:

* ``turn``                entrada con ``scripts``/``runtime``/``history`` -> piezas, historial, peticiones
* ``ask``                 entrada con ``scripts`` -> respuesta final limpia
* ``tool_ack``            entrada ``{"name", "raw"}`` -> ``{"ack": str | None}``
* ``trim_history``        entrada ``{"messages", "keep"}`` -> ``{"messages"}``
* ``seal_tool_calls``     entrada ``{"ordered", "finish_reason"}`` -> ``{"ordered", "complete"}``
* ``close_interrupted``   entrada ``{"messages", "partial", "open_calls", "assistant_appended"}``
* ``note_done``           entrada ``{"messages", "args", "turn_active"}`` -> historial y pendientes
* ``max_tokens`` / ``request_timeout`` / ``turn_timeout`` / ``tool_round_limit``
* ``user_content`` / ``notice`` / ``with_volatile`` / ``tools_for`` / ``base_tools``
* ``wants_notify`` / ``wants_close`` / ``remaining_tools`` / ``prompt_parts``
* ``export_context`` / ``import_context`` / ``reset_history`` / ``reload_llm``
* ``set_confirm_actions`` / ``pending_notify_address`` / ``close``
"""

from __future__ import annotations

import copy
import json
import threading
import time
from types import SimpleNamespace
from typing import Any

from _common import FixtureFile, main_guard

from ezagent.voice.llm import (
    EMPTY_ANSWER,
    Messenger,
    TurnCancelled,
    tool_ack,
)

DOMAIN = "llm_tools"

SETTINGS: dict[str, Any] = {
    "llm_provider": "openai",
    "llm_model": "gpt-x",
    "llm_temperature": 0.3,
    "llm_max_tokens": 150,
    "llm_base_url": "https://api.openai.com/v1",
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
    "function": {"name": "get_datetime", "parameters": {"type": "object"}},
}
TOOL_SEARCH: dict[str, Any] = {
    "type": "function",
    "function": {"name": "web_search", "parameters": {"type": "object"}},
}
BASE_TOOLS: tuple[str, ...] = (
    "voice_control",
    "web_search",
    "fetch_page",
    "read_file",
    "delegate_to_agent",
)


class _Stream:
    def __init__(self, chunks: list[Any], boom: Exception | None = None) -> None:
        self.chunks = chunks
        self.boom = boom
        self.closed = False

    def __iter__(self):
        yield from self.chunks
        if self.boom is not None:
            raise self.boom

    def close(self) -> None:
        self.closed = True


class _Client:
    """Cada ``create`` consume el siguiente guion y anota la petición."""

    def __init__(self, *scripts: Any) -> None:
        self.scripts = list(scripts)
        self.requests: list[dict[str, Any]] = []
        self.streams: list[_Stream] = []
        self.chat = SimpleNamespace(completions=SimpleNamespace(create=self._create))

    def _create(self, **request: Any) -> _Stream:
        self.requests.append(copy.deepcopy(request))
        if not self.scripts:
            raise AssertionError("guion agotado")
        script = self.scripts.pop(0)
        if isinstance(script, Exception):
            raise script
        stream = script if isinstance(script, _Stream) else _Stream(script)
        self.streams.append(stream)
        return stream

    def close(self) -> None:
        self.closed = True


class _Registry:
    def __init__(self, fail: bool = False) -> None:
        self.fail = fail

    def schemas(
        self, names: list[str] | None = None, limit: int | None = None
    ) -> list[dict[str, Any]]:
        if self.fail:
            raise RuntimeError("registro roto")
        found = [
            {"type": "function", "function": {"name": name, "parameters": {"type": "object"}}}
            for name in names or ()
        ]
        return found[:limit] if limit is not None else found


class _Runtime:
    def __init__(self, spec: dict[str, Any] | None = None) -> None:
        spec = spec or {}
        self.spec = spec
        self.prompt = str(spec.get("prompt", "PROMPT-SISTEMA"))
        self.volatile = str(spec.get("volatile", ""))
        self.tools = copy.deepcopy(spec.get("tools", [TOOL_DATETIME]))
        self.results = dict(spec.get("results") or {})
        self.executed: list[tuple[str, str]] = []
        self.recorded: list[str] = []
        self.turns: list[str] = []
        self.refreshed = 0
        self.registry = _Registry(bool(spec.get("registry_fails")))
        if "remaining" in spec:
            values = spec["remaining"]
            if isinstance(values, list):
                queue = list(values)
                self.remaining_tool_calls = lambda: int(
                    queue.pop(0) if len(queue) > 1 else queue[0]
                )
            else:
                self.remaining_tool_calls = lambda: int(values)
        if spec.get("parts"):
            self.prompt_parts = lambda: (self.prompt, self.volatile)
        self.pending = spec.get("pending")

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
        return json.dumps(result, ensure_ascii=False)

    def record_assistant(self, text: str) -> None:
        self.recorded.append(text)

    def refresh(self) -> None:
        self.refreshed += 1

    def set_confirm_actions(self, enabled: bool) -> None:
        self.confirm = enabled

    def pending_notify_address(self) -> str | None:
        if self.pending == "raise":
            raise RuntimeError("sin dirección")
        return self.pending

    def execute(self, name: str, raw: str) -> Any:
        self.executed.append((name, raw))
        outcome = self.results.get(name, {"ok": True, "data": "resultado"})
        if isinstance(outcome, Exception):
            raise outcome
        return outcome


def _chunk(
    content: str | None = None,
    *,
    tool_calls: list[Any] | None = None,
    finish_reason: str | None = None,
) -> Any:
    delta = SimpleNamespace(content=content, tool_calls=tool_calls)
    return SimpleNamespace(choices=[SimpleNamespace(delta=delta, finish_reason=finish_reason)])


def _empty_chunk() -> Any:
    return SimpleNamespace(choices=[])


def _call(name: str | None, arguments: str = "", index: int = 0, call_id: str | None = None) -> Any:
    function = SimpleNamespace(name=name, arguments=arguments)
    return SimpleNamespace(index=index, id=call_id, function=function)


def _text_chunk(content: str, finish_reason: str | None = None) -> Any:
    return _chunk(content, finish_reason=finish_reason)


def _tool_round(
    name: str,
    arguments: str,
    call_id: str = "call_1",
    *,
    preface: str | None = None,
    index: int = 0,
    finish_reason: str = "tool_calls",
) -> list[Any]:
    chunks = [_chunk(preface)] if preface else []
    chunks.append(_chunk(tool_calls=[_call(name, "", index=index, call_id=call_id)]))
    chunks.append(
        _chunk(tool_calls=[_call(None, arguments, index=index)], finish_reason=finish_reason)
    )
    return chunks


def _history(*messages: dict[str, Any]) -> list[dict[str, Any]]:
    return [{"role": "system", "content": "PROMPT-SISTEMA"}, *messages]


def _settings(overrides: dict[str, Any] | None) -> SimpleNamespace:
    values = dict(SETTINGS)
    values.update(overrides or {})
    return SimpleNamespace(**values)


def _messenger(
    client: _Client,
    runtime: _Runtime | None,
    settings: dict[str, Any] | None = None,
) -> Messenger:
    messenger = Messenger(
        _settings(settings),
        SimpleNamespace(),
        last_summary=lambda: "",
        watch=lambda address: None,
        mute=lambda muted: None,
        request_close=lambda: None,
        mic_mute=lambda muted: None,
        runtime=runtime,
    )
    messenger.client = client
    return messenger


def _tool_names(request: dict[str, Any]) -> list[str]:
    names = []
    for tool in request.get("tools") or []:
        function = tool.get("function") if tool.get("type") == "function" else tool
        names.append(str((function or {}).get("name", "")))
    return names


def _request_summary(request: dict[str, Any]) -> dict[str, Any]:
    messages = request.get("messages") or []
    summary = {
        "max_tokens": request.get("max_tokens"),
        "tools": _tool_names(request),
        "tool_choice": request.get("tool_choice"),
        "roles": [message.get("role") for message in messages],
        "tool_call_ids": [
            call.get("id") for message in messages for call in (message.get("tool_calls") or [])
        ],
        "last_content": messages[-1].get("content") if messages else None,
        "system": messages[0].get("content") if messages else None,
    }
    for key in ("temperature", "extra_body", "max_completion_tokens"):
        if key in request:
            summary[key] = request[key]
    return summary


def _output(
    client: _Client,
    messenger: Messenger,
    runtime: _Runtime | None,
    pieces: list[str],
    events: list[str],
) -> dict[str, Any]:
    return {
        "pieces": pieces,
        "answer": "".join(pieces),
        "history": copy.deepcopy(messenger.messages),
        "requests": [_request_summary(request) for request in client.requests],
        "executed": copy.deepcopy(runtime.executed if runtime is not None else []),
        "recorded": copy.deepcopy(runtime.recorded if runtime is not None else []),
        "turns": copy.deepcopy(runtime.turns if runtime is not None else []),
        "last_rounds": list(messenger.last_rounds),
        "context_events": events,
        "summary": messenger.summary,
        "streams_closed": sum(1 for stream in client.streams if stream.closed),
        "streams": len(client.streams),
    }


def _clock_for(case: dict[str, Any]) -> Any:
    if case.get("clock") != "jump":
        return None
    ticks = [1000.0]

    def tick() -> float:
        ticks[0] += 2.0
        return ticks[0]

    return tick


def _run_turn(case: dict[str, Any]) -> dict[str, Any]:
    if "runtime" not in case:
        runtime: _Runtime | None = _Runtime()
    elif case["runtime"] is None:
        runtime = None
    else:
        runtime = _Runtime(case["runtime"])
    client = _Client(*copy.deepcopy(case.get("scripts") or []))
    messenger = _messenger(client, runtime, case.get("settings"))
    if case.get("history") is not None:
        messenger.messages = copy.deepcopy(case["history"])
    messenger.summary = str(case.get("summary", ""))
    events: list[str] = []
    messenger.on_context_event = events.append
    clock = _clock_for(case)
    original = time.monotonic
    if clock is not None:
        time.monotonic = clock
    try:
        pieces = list(
            messenger.ask_stream(str(case.get("user", "hola")), mode=str(case.get("mode", "voice")))
        )
    finally:
        time.monotonic = original
    return _output(client, messenger, runtime, pieces, events)


def _chunk_spec(chunk: Any) -> dict[str, Any]:
    record: dict[str, Any] = {"choices": []}
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
        record["choices"].append(
            {
                "content": delta.content,
                "finish_reason": choice.finish_reason,
                "tool_calls": calls,
            }
        )
    return record


def _script_spec(script: Any) -> dict[str, Any]:
    # DECISION: los guiones se serializan en la entrada para que el caso no
    # dependa de objetos Python y la paridad los pueda reconstruir.
    if isinstance(script, _Stream):
        return {
            "stream": [_chunk_spec(chunk) for chunk in script.chunks],
            "boom": None if script.boom is None else f"{type(script.boom).__name__}: {script.boom}",
        }
    if isinstance(script, Exception):
        return {"error": f"{type(script).__name__}: {script}"}
    return {"chunks": [_chunk_spec(chunk) for chunk in script]}


def _runtime_input(spec: dict[str, Any] | None) -> dict[str, Any] | None:
    if spec is None:
        return None
    out = {key: copy.deepcopy(value) for key, value in spec.items() if key != "results"}
    if "results" in spec:
        out["results"] = {
            name: (
                {"error": f"{type(value).__name__}: {value}"}
                if isinstance(value, Exception)
                else value
            )
            for name, value in spec["results"].items()
        }
    return out


TURN_CASES: list[dict[str, Any]] = [
    {
        "id": "respuesta_simple",
        "scripts": [[_text_chunk("Hola, "), _empty_chunk(), _text_chunk("¿qué tal?")]],
    },
    {
        "id": "herramienta_rapida",
        "runtime": {"results": {"get_datetime": {"ok": True, "data": {"hora": "14:05"}}}},
        "scripts": [_tool_round("get_datetime", '{"tz":"UTC"}'), [_text_chunk("Son las 14:05.")]],
    },
    {
        "id": "herramienta_lenta_web_search",
        "scripts": [_tool_round("web_search", "{}", "c1"), [_text_chunk("Hecho.")]],
    },
    {
        "id": "herramienta_lenta_delegate_run",
        "scripts": [
            _tool_round("delegate_to_agent", '{"action":"run","agent":"claude"}', "c1"),
            [_text_chunk("Hecho.")],
        ],
    },
    {
        "id": "delegate_send",
        "scripts": [
            _tool_round("delegate_to_agent", '{"action":"send","address":"t1"}', "c1"),
            [_text_chunk("Hecho.")],
        ],
    },
    {
        "id": "delegate_list_sin_aviso",
        "scripts": [
            _tool_round("delegate_to_agent", '{"action":"list"}', "c1"),
            [_text_chunk("Listo.")],
        ],
    },
    {
        "id": "modelo_ya_hablo",
        "scripts": [
            _tool_round("web_search", "{}", "c1", preface="Busco precios de GPU."),
            [_text_chunk("Encontré tres.")],
        ],
    },
    {
        "id": "varias_herramientas_orden",
        "scripts": [
            [
                _chunk(tool_calls=[_call("beta", "", index=1, call_id="b")]),
                _chunk(tool_calls=[_call(None, "{}", index=1)]),
                _chunk(tool_calls=[_call("alfa", "", index=0, call_id="a")]),
                _chunk(tool_calls=[_call(None, '{"x":1}', index=0)], finish_reason="tool_calls"),
            ],
            [_text_chunk("ok")],
        ],
    },
    {
        "id": "error_del_runtime",
        "runtime": {"results": {"read_file": RuntimeError("disco roto")}},
        "scripts": [
            _tool_round("read_file", "{}", "c1", preface="Lo leo."),
            [_text_chunk("No pude leerlo.")],
        ],
    },
    {
        "id": "sin_runtime_herramienta",
        "runtime": None,
        "scripts": [
            _tool_round("delete_path", '{"path": "C:/"}', "c1"),
            [_text_chunk("No puedo.")],
        ],
    },
    {
        "id": "web_repetida_cierra",
        "scripts": [
            _tool_round("web_search", "{}", "c1"),
            _tool_round("web_search", "{}", "c2"),
            [_text_chunk("Encontré resultados, pero faltan fuentes.")],
        ],
    },
    {
        "id": "tope_de_rondas_dos",
        "settings": {"llm_max_tool_rounds": 2},
        "scripts": [
            _tool_round("get_datetime", "{}", "c1"),
            _tool_round("get_datetime", "{}", "c2"),
            [_text_chunk("Fin.")],
        ],
    },
    {
        "id": "herramienta_en_ronda_final",
        "settings": {"llm_max_tool_rounds": 2},
        "scripts": [
            _tool_round("get_datetime", "{}", "c1"),
            _tool_round("get_datetime", "{}", "c2"),
            _tool_round("get_datetime", "{}", "c3"),
        ],
    },
    {
        "id": "tope_de_llamadas",
        "runtime": {"remaining": [2, 2, 1, 1, 0]},
        "scripts": [
            _tool_round("get_datetime", "{}", "c1"),
            _tool_round("get_datetime", "{}", "c2"),
            [_text_chunk("Hecho.")],
        ],
    },
    {
        "id": "presupuesto_cero",
        "runtime": {"remaining": 0},
        "scripts": [[_text_chunk("No hay llamadas.")]],
    },
    {
        "id": "argumentos_incompletos",
        "scripts": [
            _tool_round("write_file", '{"path": "a', "c1"),
            [_text_chunk("No se pudo.")],
        ],
    },
    {
        "id": "argumentos_irreparables",
        "scripts": [
            _tool_round("write_file", "{no json", "c1"),
            [_text_chunk("No se pudo.")],
        ],
    },
    {
        "id": "marcado_como_texto",
        "scripts": [
            [
                _text_chunk("Se lo paso. "),
                _text_chunk('<｜DSML｜ calls> <｜DSML｜ invoke name="delegate_to_agent">'),
                _text_chunk("basura"),
            ]
        ],
    },
    {
        "id": "marcado_al_inicio",
        "scripts": [[_text_chunk('<|python_tag|>{"x":1}')]],
    },
    {
        "id": "finish_length",
        "scripts": [[_text_chunk("Texto cortado", finish_reason="length")]],
    },
    {
        "id": "respuesta_vacia",
        "scripts": [[_empty_chunk()]],
    },
    {
        "id": "historial_reparado_al_abrir",
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
        "scripts": [
            RuntimeError("500: Failed to parse tool call arguments as JSON"),
            [_text_chunk("Listo.")],
        ],
    },
    {
        "id": "reintento_sin_herramientas",
        "history": _history(
            {"role": "user", "content": "busca"},
            {
                "role": "assistant",
                "content": "Busco.",
                "tool_calls": [
                    {
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "web_search", "arguments": "{}"},
                    }
                ],
            },
            {"role": "tool", "tool_call_id": "call_1", "content": "{}"},
            {"role": "assistant", "content": "Listo."},
        ),
        "scripts": [
            RuntimeError("500: Failed to parse tool call arguments as JSON"),
            RuntimeError("500: Failed to parse tool call arguments as JSON"),
            [_text_chunk("Sigo.")],
        ],
    },
    {
        "id": "tres_fallos_al_abrir",
        "expect_error": True,
        "scripts": [
            RuntimeError("500: tool call json"),
            RuntimeError("500: tool call json"),
            RuntimeError("500: tool call json"),
        ],
    },
    {
        "id": "error_no_de_herramientas",
        "expect_error": True,
        "scripts": [RuntimeError("server exploded")],
    },
    {
        "id": "corte_del_stream",
        "expect_error": True,
        "scripts": [_Stream([_text_chunk("Hola")], boom=ConnectionError("cortado"))],
    },
    {
        "id": "turno_agotado",
        "expect_error": True,
        "settings": {"llm_turn_timeout_secs": 1},
        "clock": "jump",
        "scripts": [[_text_chunk("nunca")]],
    },
    {
        "id": "begin_turn_falla",
        "runtime": {"begin_fails": True},
        "scripts": [[_text_chunk("Hola igual.")]],
    },
    {
        "id": "tools_for_falla_usa_base",
        "runtime": {"tools_for_fails": True},
        "scripts": [[_text_chunk("Hecho.")]],
    },
    {
        "id": "chat_sin_acuses",
        "scripts": [
            _tool_round("web_search", "{}", "c1"),
            [_text_chunk("Encontré tres resultados.")],
        ],
        "mode": "chat",
    },
    {
        "id": "contexto_recorta",
        "settings": {"llm_context_tokens": 4000},
        "history": _history(
            {"role": "user", "content": "lee"},
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "read_file", "arguments": '{"path": "a"}'},
                    }
                ],
            },
            {"role": "tool", "tool_call_id": "call_1", "content": "x" * 4000},
            {"role": "assistant", "content": "Leído."},
            {"role": "user", "content": "y ahora " + "y" * 2500},
            {"role": "assistant", "content": "ok"},
        ),
        "scripts": [[_text_chunk("Siguiente.")]],
    },
    {
        "id": "contexto_compacta",
        "settings": {"llm_context_tokens": 4000},
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
        "scripts": [[_text_chunk("resumen")], [_text_chunk("Listo.")]],
    },
]


def _add_turns(out: FixtureFile) -> None:
    for index, case in enumerate(TURN_CASES, start=1):
        payload = {
            "user": case.get("user", "hola"),
            "mode": case.get("mode", "voice"),
            "settings": {**SETTINGS, **(case.get("settings") or {})},
            "runtime": _runtime_input(case["runtime"]) if "runtime" in case else {},
            "history": copy.deepcopy(case.get("history")),
            "summary": case.get("summary", ""),
            "scripts": [_script_spec(script) for script in case.get("scripts") or []],
            "clock": case.get("clock"),
        }
        case_id = f"llm_tools/turn/{index:04d}"
        try:
            result = _run_turn(case)
        except Exception as error:  # noqa: BLE001 - el caso fija el texto exacto
            if not case.get("expect_error"):
                raise
            out.add_error(case_id, "turn", payload, f"{type(error).__name__}: {error}")
            continue
        out.add(case_id, "turn", payload, result)


ASK_CASES: list[dict[str, Any]] = [
    {"id": "limpia", "scripts": [[_text_chunk("  Hola  ")]]},
    {"id": "vacia", "scripts": [[]]},
    {"id": "solo_espacios", "scripts": [[_text_chunk("   ")]]},
    {"id": "mensaje_de_respaldo", "scripts": [[_text_chunk(EMPTY_ANSWER)]]},
    {"id": "sin_runtime", "runtime": None, "scripts": [[_text_chunk(" No puedo. ")]]},
]


def _add_ask_cases(out: FixtureFile) -> None:
    for index, case in enumerate(ASK_CASES, start=1):
        runtime = None if case.get("runtime", "default") is None else _Runtime()
        client = _Client(*copy.deepcopy(case["scripts"]))
        messenger = _messenger(client, runtime, case.get("settings"))
        answer = messenger.ask("x")
        out.add(
            f"llm_tools/ask/{index:04d}",
            "ask",
            {
                "id": case["id"],
                "runtime": case.get("runtime", "default"),
                "scripts": [_script_spec(script) for script in case["scripts"]],
            },
            {"answer": answer, "history": copy.deepcopy(messenger.messages)},
        )


TOOL_ACK_CASES: list[tuple[str, Any]] = [
    ("web_search", "{}"),
    ("web_search", ""),
    ("web_search", "{roto"),
    ("web_search", "null"),
    ("web_search", '{"query": "gpu"}'),
    ("fetch_page", "{}"),
    ("fetch_page", ""),
    ("fetch_page", '{"url": "x"}'),
    ("run_skill_script", "{}"),
    ("run_skill_script", '{"script": "x"}'),
    ("delegate_to_agent", '{"action":"send","address":"t1"}'),
    ("delegate_to_agent", '{"action":"run","agent":"claude"}'),
    ("delegate_to_agent", '{"action":"run","agent":"opencode"}'),
    ("delegate_to_agent", '{"action":"run","agent":"codex"}'),
    ("delegate_to_agent", '{"action":"run","agent":"CLAUDE"}'),
    ("delegate_to_agent", '{"action":"run","agent":" Claude "}'),
    ("delegate_to_agent", '{"action":"run","agent":"desconocido"}'),
    ("delegate_to_agent", '{"action":"run","agent":""}'),
    ("delegate_to_agent", '{"action":"run"}'),
    ("delegate_to_agent", '{"action":"run","agent":null}'),
    ("delegate_to_agent", '{"action":"run","agent":0}'),
    ("delegate_to_agent", '{"action":"run","agent":5}'),
    ("delegate_to_agent", '{"action":"list"}'),
    ("delegate_to_agent", '{"action":"RUN","agent":"claude"}'),
    ("delegate_to_agent", '{"action":""}'),
    ("delegate_to_agent", '{"action":0}'),
    ("delegate_to_agent", '{"action":true}'),
    ("delegate_to_agent", "{}"),
    ("delegate_to_agent", "[]"),
    ("delegate_to_agent", "null"),
    ("delegate_to_agent", '"texto"'),
    ("delegate_to_agent", "5"),
    ("delegate_to_agent", "no json"),
    ("voice_control", '{"action":"mute"}'),
    ("read_file", '{"path":"a.txt"}'),
    ("WEB_SEARCH", "{}"),
    ("", "{}"),
    ("web_search", None),
    ("web_search", 0),
    ("delegate_to_agent", '{"action":"run","agent":["claude"]}'),
]


def _add_tool_ack_cases(out: FixtureFile) -> None:
    for index, (name, raw) in enumerate(TOOL_ACK_CASES, start=1):
        out.add(
            f"llm_tools/tool_ack/{index:04d}",
            "tool_ack",
            {"name": name, "raw": raw},
            {"ack": tool_ack(name, raw)},
        )


TRIM_CASES: list[dict[str, Any]] = [
    {
        "id": "corto",
        "messages": _history(
            {"role": "user", "content": "u"}, {"role": "assistant", "content": "a"}
        ),
    },
    {
        "id": "sin_tool_huerfano",
        "keep": 12,
        "messages": _history(
            *[
                item
                for turn in range(10)
                for item in (
                    {"role": "user", "content": f"u{turn}"},
                    {"role": "assistant", "content": "", "tool_calls": [{"id": f"c{turn}"}]},
                    {"role": "tool", "tool_call_id": f"c{turn}", "content": "{}"},
                    {"role": "assistant", "content": f"a{turn}"},
                )
            ]
        ),
    },
    {
        "id": "empieza_con_tool",
        "keep": 6,
        "messages": _history(
            {"role": "assistant", "content": "", "tool_calls": [{"id": "c0"}]},
            {"role": "tool", "tool_call_id": "c0", "content": "{}"},
            *[
                item
                for turn in range(4)
                for item in (
                    {"role": "user", "content": f"u{turn}"},
                    {"role": "assistant", "content": f"a{turn}"},
                )
            ],
        ),
    },
    {
        "id": "keep_cero",
        "keep": 0,
        "messages": _history(
            {"role": "user", "content": "u"},
            {"role": "assistant", "content": "a"},
        ),
    },
    {
        "id": "frontera",
        "keep": 3,
        "messages": _history(
            {"role": "user", "content": "u0"},
            {"role": "assistant", "content": "a0"},
            {"role": "user", "content": "u1"},
            {"role": "assistant", "content": "a1"},
            {"role": "user", "content": "u2"},
            {"role": "assistant", "content": "a2"},
        ),
    },
    {
        "id": "vacío_sin_sistema",
        "keep": 4,
        "messages": [{"role": "user", "content": "u"}, {"role": "assistant", "content": "a"}],
    },
]


def _add_trim_cases(out: FixtureFile) -> None:
    for index, case in enumerate(TRIM_CASES, start=1):
        messenger = _messenger(_Client(), _Runtime())
        messenger.messages = copy.deepcopy(case["messages"])
        messenger._trim_history(int(case.get("keep", 12)))
        out.add(
            f"llm_tools/trim_history/{index:04d}",
            "trim_history",
            {"messages": case["messages"], "keep": int(case.get("keep", 12))},
            {"messages": copy.deepcopy(messenger.messages)},
        )


SEAL_CASES: list[dict[str, Any]] = [
    {
        "id": "completos",
        "ordered": [
            {"id": "a", "function": {"name": "a", "arguments": '{"x": 1}'}},
            {"id": "b", "function": {"name": "b", "arguments": "{}"}},
        ],
        "finish_reason": "tool_calls",
    },
    {
        "id": "cortado",
        "ordered": [
            {"id": "a", "function": {"name": "a", "arguments": '{"x": 1'}},
            {"id": "b", "function": {"name": "b", "arguments": ""}},
        ],
        "finish_reason": "length",
    },
    {
        "id": "irreparable",
        "ordered": [{"id": "a", "function": {"name": "a", "arguments": "{no json"}}],
        "finish_reason": None,
    },
    {
        "id": "vacio",
        "ordered": [],
        "finish_reason": "tool_calls",
    },
    {
        "id": "clave_sin_valor",
        "ordered": [{"id": "a", "function": {"name": "a", "arguments": '{"a": 1, "b"'}}],
        "finish_reason": "tool_calls",
    },
    {
        "id": "unicode",
        "ordered": [{"id": "ñ", "function": {"name": "f", "arguments": '{"texto": "ñandú ☕'}}],
        "finish_reason": "length",
    },
]


def _add_seal_cases(out: FixtureFile) -> None:
    for index, case in enumerate(SEAL_CASES, start=1):
        ordered = copy.deepcopy(case["ordered"])
        complete = Messenger._seal_tool_calls(ordered, case["finish_reason"])
        out.add(
            f"llm_tools/seal_tool_calls/{index:04d}",
            "seal_tool_calls",
            {"ordered": case["ordered"], "finish_reason": case["finish_reason"]},
            {"ordered": ordered, "complete": complete},
        )


CLOSE_CASES: list[dict[str, Any]] = [
    {
        "id": "texto_parcial",
        "partial": ["Hola", " mundo"],
        "open_calls": [],
        "assistant_appended": False,
    },
    {
        "id": "texto_vacio",
        "partial": ["   "],
        "open_calls": [],
        "assistant_appended": False,
    },
    {
        "id": "llamada_huerfana",
        "partial": [],
        "open_calls": [{"id": "c1", "function": {"name": "f", "arguments": "{}"}}],
        "assistant_appended": True,
    },
    {
        "id": "resultado_ya_presente",
        "partial": [],
        "messages": _history(
            {"role": "user", "content": "x"},
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [{"id": "c1"}, {"id": "c2"}],
            },
            {"role": "tool", "tool_call_id": "c1", "content": "{}"},
        ),
        "open_calls": [{"id": "c1"}, {"id": "c2"}],
        "assistant_appended": True,
    },
    {
        "id": "llamada_sin_id",
        "partial": [],
        "open_calls": [{"id": ""}, {}],
        "assistant_appended": True,
    },
    {
        "id": "huerfano_previo",
        "partial": ["x"],
        "messages": _history(
            {"role": "user", "content": "u"},
            {"role": "tool", "tool_call_id": "fantasma", "content": "{}"},
        ),
        "open_calls": [],
        "assistant_appended": False,
    },
]


def _add_close_cases(out: FixtureFile) -> None:
    for index, case in enumerate(CLOSE_CASES, start=1):
        messenger = _messenger(_Client(), _Runtime())
        if case.get("messages") is not None:
            messenger.messages = copy.deepcopy(case["messages"])
        partial = copy.deepcopy(case["partial"])
        open_calls = copy.deepcopy(case["open_calls"])
        messenger._close_interrupted(partial, open_calls, bool(case["assistant_appended"]))
        out.add(
            f"llm_tools/close_interrupted/{index:04d}",
            "close_interrupted",
            {
                "messages": case.get("messages"),
                "partial": partial,
                "open_calls": case["open_calls"],
                "assistant_appended": case["assistant_appended"],
            },
            {"messages": copy.deepcopy(messenger.messages)},
        )


NOTE_DONE_CASES: list[dict[str, Any]] = [
    {
        "id": "con_tarea",
        "args": {
            "address": "t1:2",
            "agent": "claude",
            "reason": "terminó",
            "output": "salida X",
            "task": "Haz el plan",
        },
    },
    {
        "id": "sin_tarea",
        "args": {"address": "t1:3", "agent": "codex", "reason": "ok", "output": "o"},
    },
    {
        "id": "salida_larga",
        "args": {"address": "t1:4", "agent": "opencode", "reason": "ok", "output": "z" * 12_500},
    },
    {
        "id": "salida_vacia",
        "args": {"address": "t1:5", "agent": "claude", "reason": "", "output": ""},
    },
    {
        "id": "tarea_espacios",
        "args": {
            "address": "t1:6",
            "agent": "claude",
            "reason": "ok",
            "output": "x",
            "task": "   ",
        },
    },
    {
        "id": "durante_turno",
        "args": {"address": "t1:7", "agent": "claude", "reason": "ok", "output": "x"},
        "turn_active": True,
    },
]


def _add_note_done_cases(out: FixtureFile) -> None:
    for index, case in enumerate(NOTE_DONE_CASES, start=1):
        messenger = _messenger(_Client(), _Runtime())
        if case.get("turn_active"):
            messenger._turn_active = True
        before = copy.deepcopy(messenger.messages)
        messenger.note_done(**case["args"])
        out.add(
            f"llm_tools/note_done/{index:04d}",
            "note_done",
            {
                "args": case["args"],
                "messages": before,
                "turn_active": bool(case.get("turn_active")),
            },
            {
                "messages": copy.deepcopy(messenger.messages),
                "pending": copy.deepcopy(messenger._pending_notes),
            },
        )


MAX_TOKENS_CASES: list[dict[str, Any]] = [
    {"llm_max_tokens": 150, "chat": False, "has_tools": False, "caps": None},
    {"llm_max_tokens": 150, "chat": True, "has_tools": False, "caps": None},
    {"llm_max_tokens": 150, "chat": False, "has_tools": True, "caps": None},
    {"llm_max_tokens": 3000, "chat": True, "has_tools": True, "caps": None},
    {
        "llm_max_tokens": 100,
        "chat": False,
        "has_tools": False,
        "caps": {"reasoning": True, "max_output": 32_000},
    },
    {
        "llm_max_tokens": 100,
        "chat": False,
        "has_tools": False,
        "caps": {"reasoning": False, "max_output": 100},
    },
    {
        "llm_max_tokens": 100_000,
        "chat": True,
        "has_tools": True,
        "caps": {"reasoning": True, "max_output": 8_000},
    },
    {
        "llm_max_tokens": 0,
        "chat": False,
        "has_tools": False,
        "caps": {"reasoning": False, "max_output": 0},
    },
]


def _add_max_tokens_cases(out: FixtureFile) -> None:
    for index, case in enumerate(MAX_TOKENS_CASES, start=1):
        messenger = _messenger(_Client(), _Runtime(), {"llm_max_tokens": case["llm_max_tokens"]})
        caps = SimpleNamespace(**case["caps"]) if case["caps"] is not None else None
        value = messenger._max_tokens(bool(case["chat"]), bool(case["has_tools"]), caps)
        out.add(
            f"llm_tools/max_tokens/{index:04d}",
            "max_tokens",
            case,
            {"max_tokens": value},
        )


TIMEOUT_CASES: list[Any] = [30.0, 1, 600, 0, 601, "5", "x", None, 0.5, 30.5]
ROUND_LIMIT_CASES: list[Any] = [8, 0, 1, 12, 13, 3.9, "4", "x", None, -1]


def _add_limit_cases(out: FixtureFile) -> None:
    for index, value in enumerate(TIMEOUT_CASES, start=1):
        messenger = _messenger(_Client(), _Runtime(), {"llm_timeout_secs": value})
        out.add(
            f"llm_tools/request_timeout/{index:04d}",
            "request_timeout",
            {"llm_timeout_secs": value},
            {"timeout": messenger._request_timeout()},
        )
    for index, value in enumerate(TIMEOUT_CASES, start=1):
        messenger = _messenger(_Client(), _Runtime(), {"llm_turn_timeout_secs": value})
        out.add(
            f"llm_tools/turn_timeout/{index:04d}",
            "turn_timeout",
            {"llm_turn_timeout_secs": value},
            {"timeout": messenger._turn_timeout()},
        )
    for index, value in enumerate(ROUND_LIMIT_CASES, start=1):
        messenger = _messenger(_Client(), _Runtime(), {"llm_max_tool_rounds": value})
        out.add(
            f"llm_tools/tool_round_limit/{index:04d}",
            "tool_round_limit",
            {"llm_max_tool_rounds": value},
            {"limit": messenger._tool_round_limit()},
        )


def _add_message_helpers(out: FixtureFile) -> None:
    volatile_cases = ["", "   ", "ahora", "  ahora  ", "línea\ncon salto"]
    for index, volatile in enumerate(volatile_cases, start=1):
        out.add(
            f"llm_tools/user_content/{index:04d}",
            "user_content",
            {"text": "hola", "volatile": volatile},
            {"content": Messenger._user_content("hola", volatile)},
        )
    for index, text in enumerate(["", "cierra", "Se lo paso.", "ñ ☕"], start=1):
        out.add(
            f"llm_tools/notice/{index:04d}",
            "notice",
            {"text": text},
            {"notice": Messenger._notice(text)},
        )
    messages = [
        {"role": "system", "content": "s"},
        {"role": "user", "content": "uno"},
        {"role": "assistant", "content": "dos"},
        {"role": "user", "content": "tres"},
    ]
    for index, volatile in enumerate(["", "  ", "ahora", "  ahora  "], start=1):
        out.add(
            f"llm_tools/with_volatile/{index:04d}",
            "with_volatile",
            {"messages": messages, "volatile": volatile},
            {"messages": Messenger._with_volatile(copy.deepcopy(messages), volatile)},
        )
    for name, text in (
        (
            "sin_usuario",
            [{"role": "system", "content": "s"}, {"role": "assistant", "content": "a"}],
        ),
        ("usuario_con_lista", [{"role": "user", "content": [{"type": "text", "text": "x"}]}]),
    ):
        out.add(
            f"llm_tools/with_volatile/{name}",
            "with_volatile",
            {"messages": text, "volatile": "dato"},
            {"messages": Messenger._with_volatile(copy.deepcopy(text), "dato")},
        )
    out.add(
        "llm_tools/with_volatile/no_muta",
        "with_volatile",
        {"messages": messages, "volatile": "dato"},
        {
            "messages": Messenger._with_volatile(messages, "dato"),
            "original": messages,
        },
    )


def _add_tools_cases(out: FixtureFile) -> None:
    runtime = _Runtime()
    caps = SimpleNamespace(tools=True, temperature=True, max_output=32_000, reasoning=False)
    cases = [
        ("runtime_none", None, caps),
        ("caps_sin_herramientas", runtime, SimpleNamespace(tools=False)),
        ("normal", runtime, caps),
        ("tools_for_falla", _Runtime({"tools_for_fails": True}), caps),
    ]
    for index, (name, rt, capability) in enumerate(cases, start=1):
        messenger = _messenger(_Client(), rt)
        out.add(
            f"llm_tools/tools_for/{index:04d}",
            "tools_for",
            {"variant": name},
            {"tools": messenger._tools_for(rt, "hola", capability)},
        )
    for index, fails in enumerate((False, True), start=1):
        rt = _Runtime({"registry_fails": fails})
        out.add(
            f"llm_tools/base_tools/{index:04d}",
            "base_tools",
            {"registry_fails": fails},
            {"tools": Messenger._base_tools(rt)},
        )


WANTS_NOTIFY = [
    "avísame",
    "avisame",
    "avísame cuando termine",
    "me avisas",
    "notifícame",
    "cuando termine",
    "cuando esté",
    "al terminar",
    "estate pendiente",
    "vigila",
    "pendiente de",
    "me dice",
    "dime",
    "resumen",
    "resúmeme",
    "AVÍSAME",
    "hola, ¿qué tal?",
    "",
]
WANTS_CLOSE = [
    "ciérrate",
    "cierrate",
    "cierra la escucha",
    "cierra el micro",
    "deja de escuchar",
    "sin escuchar",
    "CIÉRRATE",
    "hola",
    "",
]


def _add_wants_cases(out: FixtureFile) -> None:
    for index, text in enumerate(WANTS_NOTIFY, start=1):
        messenger = _messenger(_Client(), _Runtime())
        messenger._last_user_text = text
        out.add(
            f"llm_tools/wants_notify/{index:04d}",
            "wants_notify",
            {"text": text},
            {"result": messenger._wants_notify()},
        )
    for index, text in enumerate(WANTS_CLOSE, start=1):
        messenger = _messenger(_Client(), _Runtime())
        messenger._last_user_text = text
        out.add(
            f"llm_tools/wants_close/{index:04d}",
            "wants_close",
            {"text": text},
            {"result": messenger._wants_close()},
        )


def _add_remaining_cases(out: FixtureFile) -> None:
    cases: list[tuple[str, _Runtime | None]] = [
        ("ninguno", None),
        ("cero", _Runtime({"remaining": 0})),
        ("tres", _Runtime({"remaining": 3})),
        ("negativo", _Runtime({"remaining": -2})),
        ("sin_metodo", _Runtime()),
    ]
    for index, (name, runtime) in enumerate(cases, start=1):
        out.add(
            f"llm_tools/remaining_tools/{index:04d}",
            "remaining_tools",
            {"variant": name},
            {"remaining": Messenger._remaining_tools(runtime)},
        )
    for index, (name, spec) in enumerate(
        (
            ("completo", {"parts": True, "prompt": "P", "volatile": "V"}),
            ("vacio", {"parts": True, "prompt": "P", "volatile": ""}),
            ("sin_parts", {}),
            ("parts_falla", {"parts_fails": True}),
        ),
        start=1,
    ):
        runtime = _Runtime(spec)
        if spec.get("parts_fails"):

            def broken() -> Any:
                raise RuntimeError("sin partes")

            runtime.prompt_parts = broken
        messenger = _messenger(_Client(), runtime, {})
        out.add(
            f"llm_tools/prompt_parts/{index:04d}",
            "prompt_parts",
            {"variant": name},
            {"parts": list(messenger._prompt_parts(runtime))},
        )


def _add_state_cases(out: FixtureFile) -> None:
    messenger = _messenger(_Client(), _Runtime())
    messenger.messages = _history(
        {"role": "user", "content": "u"}, {"role": "assistant", "content": "a"}
    )
    messenger.summary = "resumen"
    messenger._last_user_text = "hola"
    exported = messenger.export_context()
    out.add(
        "llm_tools/export_context/0001",
        "export_context",
        {"messages": messenger.messages, "summary": "resumen", "last_user": "hola"},
        {"messages": exported[0], "summary": exported[1], "last_user": exported[2]},
    )
    imported = copy.deepcopy(messenger.messages) + [{"role": "system", "content": "otro"}]
    messenger.import_context(imported, "nuevo", "último")
    out.add(
        "llm_tools/import_context/0001",
        "import_context",
        {"messages": imported, "summary": "nuevo", "last_user": "último", "turn_active": False},
        {
            "messages": copy.deepcopy(messenger.messages),
            "summary": messenger.summary,
            "last_user": messenger._last_user_text,
        },
    )
    active = _messenger(_Client(), _Runtime())
    active._turn_active = True
    active.import_context([{"role": "user", "content": "x"}], "s", "u")
    out.add(
        "llm_tools/import_context/0002",
        "import_context",
        {
            "messages": [{"role": "user", "content": "x"}],
            "summary": "s",
            "last_user": "u",
            "turn_active": True,
        },
        {
            "messages": copy.deepcopy(active.messages),
            "deferred": copy.deepcopy(active._deferred_history),
        },
    )
    reset = _messenger(_Client(), _Runtime())
    reset.messages = _history({"role": "user", "content": "u"})
    reset.summary = "s"
    reset._last_user_text = "u"
    reset.reset_history()
    out.add(
        "llm_tools/reset_history/0001",
        "reset_history",
        {
            "messages": [
                {"role": "system", "content": "PROMPT-SISTEMA"},
                {"role": "user", "content": "u"},
            ]
        },
        {
            "messages": copy.deepcopy(reset.messages),
            "summary": reset.summary,
            "last_user": reset._last_user_text,
        },
    )
    reset_active = _messenger(_Client(), _Runtime())
    reset_active.messages = _history({"role": "user", "content": "u"})
    reset_active.summary = "s"
    reset_active._turn_active = True
    reset_active._pending_notes.append({"role": "user", "content": "nota"})
    reset_active.reset_history()
    out.add(
        "llm_tools/reset_history/0002",
        "reset_history",
        {
            "messages": [
                {"role": "system", "content": "PROMPT-SISTEMA"},
                {"role": "user", "content": "u"},
            ],
            "turn_active": True,
        },
        {
            "messages": copy.deepcopy(reset_active.messages),
            "deferred": copy.deepcopy(reset_active._deferred_history),
            "pending": copy.deepcopy(reset_active._pending_notes),
        },
    )


def _add_reload_cases(out: FixtureFile) -> None:
    runtime = _Runtime()
    client = _Client()
    messenger = _messenger(client, runtime)
    messenger.messages = _history({"role": "user", "content": "u"})
    runtime.prompt = "PROMPT-NUEVO"
    messenger.reload_llm()
    out.add(
        "llm_tools/reload_llm/0001",
        "reload_llm",
        {"prompt": "PROMPT-NUEVO", "runtime": True},
        {
            "messages": copy.deepcopy(messenger.messages),
            "refreshed": runtime.refreshed,
            "old_closed": getattr(client, "closed", False),
        },
    )
    confirm_runtime = _Runtime()
    confirm = _messenger(_Client(), confirm_runtime)
    confirm.set_confirm_actions(True)
    out.add(
        "llm_tools/set_confirm_actions/0001",
        "set_confirm_actions",
        {"enabled": True},
        {
            "cfg": confirm.cfg.confirm_actions,
            "runtime": getattr(confirm_runtime, "confirm", None),
            "system": confirm.messages[0],
        },
    )
    busy = _messenger(_Client(), _Runtime())
    busy._turn_active = True
    busy.set_confirm_actions(False)
    out.add(
        "llm_tools/set_confirm_actions/0002",
        "set_confirm_actions",
        {"enabled": False, "turn_active": True},
        {"cfg": busy.cfg.confirm_actions, "system": busy.messages[0]},
    )
    closing = _Client()
    close_messenger = _messenger(closing, _Runtime())
    close_messenger.close()
    out.add(
        "llm_tools/close/0001",
        "close",
        {"client": "guionizado"},
        {"client_closed": getattr(closing, "closed", False)},
    )


def _add_notify_cases(out: FixtureFile) -> None:
    cases: list[tuple[str, str | None, str, str | None]] = [
        ("runtime_direccion", "t1:9", "", None),
        ("runtime_none_sin_texto", None, "hola", "t1:1"),
        ("runtime_none_con_texto", None, "avísame cuando termine", "t1:1"),
        ("runtime_none_sin_direccion", None, "avísame", None),
        ("runtime_falla", "raise", "", None),
    ]
    for index, (name, pending, text, address) in enumerate(cases, start=1):
        runtime = None if name.startswith("runtime_none") else _Runtime({"pending": pending})
        messenger = _messenger(_Client(), runtime)
        messenger._last_user_text = text
        messenger._last_agent_address = address
        out.add(
            f"llm_tools/pending_notify_address/{index:04d}",
            "pending_notify_address",
            {"variant": name, "text": text, "address": address},
            {"address": messenger.pending_notify_address()},
        )


def _add_cancel_cases(out: FixtureFile) -> None:
    runtime = _Runtime()
    cancelled_calls = [0]

    def cancel_confirmation() -> None:
        cancelled_calls[0] += 1

    runtime.cancel_confirmation = cancel_confirmation
    messenger = _messenger(_Client(), runtime)
    token = SimpleNamespace(cancelled=threading.Event())
    stream = SimpleNamespace(closed=False)
    stream.close = lambda: setattr(stream, "closed", True)
    messenger._turn_token = token
    messenger._active_stream = stream
    before = messenger._turn_generation
    messenger.cancel_turn()
    out.add(
        "llm_tools/cancel_turn/0001",
        "cancel_turn",
        {"token": True, "active_stream": True, "runtime": True},
        {
            "cancelled": token.cancelled.is_set(),
            "generation_delta": messenger._turn_generation - before,
            "stream_closed": stream.closed,
            "runtime_cancelled": cancelled_calls[0],
        },
    )
    bare = _messenger(_Client(), _Runtime())
    bare.runtime.cancel_confirmation = lambda: (_ for _ in ()).throw(RuntimeError("x"))
    before = bare._turn_generation
    bare.cancel_turn()
    out.add(
        "llm_tools/cancel_turn/0002",
        "cancel_turn",
        {"token": False, "active_stream": False, "runtime": "falla"},
        {"generation_delta": bare._turn_generation - before},
    )


def _add_deferred_cases(out: FixtureFile) -> None:
    messenger = _messenger(_Client(), _Runtime())
    messenger.messages = _history({"role": "user", "content": "viejo"})
    messenger._pending_notes = [{"role": "user", "content": "nota"}]
    deferred = (
        [
            {"role": "system", "content": "PROMPT-SISTEMA"},
            {"role": "user", "content": "nuevo"},
        ],
        "resumen",
        "nuevo usuario",
    )
    messenger._deferred_history = copy.deepcopy(deferred)
    messenger._apply_deferred_history()
    out.add(
        "llm_tools/apply_deferred_history/0001",
        "apply_deferred_history",
        {"messages": deferred[0], "summary": "resumen", "last_user": "nuevo usuario"},
        {
            "messages": copy.deepcopy(messenger.messages),
            "summary": messenger.summary,
            "last_user": messenger._last_user_text,
            "pending": copy.deepcopy(messenger._pending_notes),
            "deferred": copy.deepcopy(messenger._deferred_history),
        },
    )
    idle = _messenger(_Client(), _Runtime())
    idle.messages = _history({"role": "user", "content": "u"})
    idle._apply_deferred_history()
    out.add(
        "llm_tools/apply_deferred_history/0002",
        "apply_deferred_history",
        {"messages": idle.messages, "summary": "", "last_user": ""},
        {
            "messages": copy.deepcopy(idle.messages),
            "deferred": copy.deepcopy(idle._deferred_history),
        },
    )
    pending = _messenger(_Client(), _Runtime())
    pending._pending_notes = [
        {"role": "user", "content": "uno"},
        {"role": "user", "content": "dos"},
    ]
    taken = pending._take_pending_notes()
    out.add(
        "llm_tools/take_pending_notes/0001",
        "take_pending_notes",
        {"pending": 2},
        {"taken": copy.deepcopy(taken), "remaining": copy.deepcopy(pending._pending_notes)},
    )
    token = SimpleNamespace(cancelled=threading.Event())
    token.cancelled.set()
    try:
        Messenger._check_cancel(token)
    except TurnCancelled as error:
        out.add_error(
            "llm_tools/check_cancel/0001",
            "check_cancel",
            {"cancelled": True},
            f"{type(error).__name__}: {error}",
        )
    else:
        raise AssertionError("check_cancel no levantó TurnCancelled")
    out.add(
        "llm_tools/check_cancel/0002",
        "check_cancel",
        {"cancelled": False},
        {"ok": Messenger._check_cancel(SimpleNamespace(cancelled=threading.Event())) is None},
    )
    out.add(
        "llm_tools/check_cancel/0003",
        "check_cancel",
        {"cancelled": None},
        {"ok": Messenger._check_cancel(None) is None},
    )


def generate(out: FixtureFile) -> None:
    # DECISION: reloj monótono congelado para que las notas de ronda («0 ms»)
    # sean idénticas en cada corrida; el valor real solo mide latencia.
    original = time.monotonic
    time.monotonic = lambda: 1000.0
    try:
        _add_turns(out)
        _add_ask_cases(out)
        _add_tool_ack_cases(out)
        _add_trim_cases(out)
        _add_seal_cases(out)
        _add_close_cases(out)
        _add_note_done_cases(out)
        _add_max_tokens_cases(out)
        _add_limit_cases(out)
        _add_message_helpers(out)
        _add_tools_cases(out)
        _add_wants_cases(out)
        _add_remaining_cases(out)
        _add_state_cases(out)
        _add_cancel_cases(out)
        _add_deferred_cases(out)
        _add_reload_cases(out)
        _add_notify_cases(out)
    finally:
        time.monotonic = original


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
