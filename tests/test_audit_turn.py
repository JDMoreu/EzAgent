"""Regresiones de integridad del turno: cancelación, historial y eventos atrasados.

Cubren F01/F38 (cancelar no deja el historial inválido ni sigue pidiendo al
modelo), F02 (una transcripción vieja no resuelve la confirmación vigente) y
F29 (un error viejo no interrumpe el turno actual).
"""

from __future__ import annotations

import json
import threading
from types import SimpleNamespace

from ezagent.agent.registry import ToolResult
from ezagent.voice.app import VoiceApp
from ezagent.voice.llm import Messenger, TurnCancelled


class Stream:
    def __init__(self, chunks) -> None:
        self.chunks = chunks

    def __iter__(self):
        yield from self.chunks

    def close(self) -> None:
        pass


class Client:
    def __init__(self, *scripts) -> None:
        self.scripts = list(scripts)
        self.requests: list[dict] = []
        self.chat = SimpleNamespace(completions=SimpleNamespace(create=self._create))

    def _create(self, **request):
        self.requests.append({**request, "messages": json.loads(json.dumps(request["messages"]))})
        script = self.scripts.pop(0)
        return script if isinstance(script, Stream) else Stream(script)


def text(content: str):
    return SimpleNamespace(
        choices=[SimpleNamespace(delta=SimpleNamespace(content=content, tool_calls=None))]
    )


def call(name: str, arguments: str = "{}", *, call_id: str):
    function = SimpleNamespace(name=name, arguments=arguments)
    tool = SimpleNamespace(index=0, id=call_id, function=function)
    return SimpleNamespace(
        choices=[SimpleNamespace(delta=SimpleNamespace(content=None, tool_calls=[tool]))]
    )


class Runtime:
    def __init__(self) -> None:
        self.entered = threading.Event()
        self.release = threading.Event()
        self.cancelled = 0
        self.result = ToolResult(ok=True, data="hecho")

    def begin_turn(self, text: str) -> None:
        pass

    def system_prompt(self) -> str:
        return "SISTEMA"

    def prompt_parts(self) -> tuple[str, str]:
        return "SISTEMA", ""

    def tools_for(self, text: str):
        return [{"type": "function", "function": {"name": "slow"}}]

    def remaining_tool_calls(self) -> int:
        return 4

    def render_result(self, result) -> str:
        return json.dumps(result.to_dict(), ensure_ascii=False)

    def record_assistant(self, text: str) -> None:
        pass

    def cancel_confirmation(self) -> None:
        self.cancelled += 1

    def execute(self, name: str, raw: str):
        self.entered.set()
        self.release.wait(2)
        return self.result


def make(client: Client, runtime=None) -> Messenger:
    cfg = SimpleNamespace(
        llm_provider="openai",
        llm_model="gpt-x",
        llm_base_url="http://localhost/v1",
        api_key="k",
        llm_api_style="openai",
        llm_temperature=0.3,
        llm_max_tokens=320,
        llm_context_tokens=32_000,
        llm_reasoning_effort="none",
        llm_timeout_secs=30,
        llm_turn_timeout_secs=120,
        llm_max_tool_rounds=8,
        confirm_actions=False,
    )
    messenger = Messenger(
        cfg,
        object(),
        last_summary=lambda: "",
        watch=lambda address: None,
        mute=lambda minutes: None,
        request_close=lambda: None,
        mic_mute=lambda muted: None,
        runtime=runtime,
    )
    messenger.client = client
    return messenger


def test_cancelar_durante_una_herramienta_no_deja_llamadas_huerfanas() -> None:
    runtime = Runtime()
    client = Client([call("slow", call_id="c1")], [text("Después.")])
    messenger = make(client, runtime)
    seen: list[str] = []

    def consume() -> None:
        try:
            for delta in messenger.ask_stream("hazlo"):
                seen.append(delta)
        except TurnCancelled:
            pass

    worker = threading.Thread(target=consume, daemon=True)
    worker.start()
    assert runtime.entered.wait(2)
    messenger.cancel_turn()
    runtime.release.set()
    worker.join(3)
    assert not worker.is_alive()
    assert len(client.requests) == 1  # el turno cancelado no abrió otra petición
    assert "c1" in {m.get("tool_call_id") for m in messenger.messages if m.get("role") == "tool"}
    assert messenger.messages[-1]["role"] == "tool"

    # El siguiente turno manda un historial estructuralmente válido.
    answer = messenger.ask("sigue")
    assert answer == "Después."
    sent = client.requests[-1]["messages"]
    open_ids = {
        call["id"]
        for message in sent
        if message.get("role") == "assistant"
        for call in message.get("tool_calls") or []
    }
    closed_ids = {m.get("tool_call_id") for m in sent if m.get("role") == "tool"}
    assert open_ids <= closed_ids


def test_una_llamada_huerfana_se_repara_antes_de_enviar() -> None:
    client = Client(Stream([text("ok")]))
    messenger = make(client)
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
                }
            ],
        },
    ]
    assert messenger.ask("hola") == "ok"
    sent = client.requests[0]["messages"]
    assert any(m.get("role") == "tool" and m.get("tool_call_id") == "old" for m in sent), (
        "el historial enviado debe cerrar la llamada abierta"
    )


def test_note_done_durante_un_turno_se_aplica_al_siguiente() -> None:
    runtime = Runtime()
    client = Client([call("slow", call_id="c1")], [text("ok")], [text("fin")])
    messenger = make(client, runtime)
    seen: list[str] = []

    def consume() -> None:
        for delta in messenger.ask_stream("hazlo"):
            seen.append(delta)

    worker = threading.Thread(target=consume, daemon=True)
    worker.start()
    assert runtime.entered.wait(2)
    # Llega desde el hilo de Tk mientras la herramienta está bloqueada.
    messenger.note_done("t1:2", "claude", "terminó", "salida X", task="Haz el plan")
    assert "Datos de un agente delegado" not in {
        str(message.get("content") or "") for message in messenger.messages
    }
    runtime.release.set()
    worker.join(3)
    assert not worker.is_alive()

    for request in client.requests:
        messages = request["messages"]
        for index, message in enumerate(messages):
            if message.get("role") != "assistant" or not message.get("tool_calls"):
                continue
            call_id = message["tool_calls"][0]["id"]
            assert messages[index + 1].get("role") == "tool"
            assert messages[index + 1].get("tool_call_id") == call_id
    assert all("Datos de un agente delegado" not in str(r) for r in client.requests[0]["messages"])

    assert messenger.ask("siguiente") == "fin"
    second = client.requests[-1]["messages"]
    last_user = next(m for m in reversed(second) if m.get("role") == "user")
    assert "Datos de un agente delegado" in last_user["content"]
    assert "siguiente" in last_user["content"]
    positions = [i for i, m in enumerate(messenger.messages) if "Datos de un agente" in str(m)]
    user_positions = [
        i for i, m in enumerate(messenger.messages) if m.get("content") == "siguiente"
    ]
    assert positions and user_positions and positions[-1] < user_positions[-1]


def test_los_datos_volatiles_solo_viajan_en_la_peticion() -> None:
    runtime = Runtime()
    client = Client([text("a")], [text("b")], [text("c")])
    messenger = make(client, runtime)
    volatile = ("MEM-A", "MEM-B", "MEM-C")
    for index, data in enumerate(volatile):
        runtime.prompt_parts = lambda data=data: ("SISTEMA", data)  # type: ignore[method-assign]
        assert messenger.ask(f"turno {index}") == "abc"[index]

    # El historial persistido guarda solo lo que dijo el usuario.
    assert all("Datos del entorno" not in str(message) for message in messenger.messages)

    for index, data in enumerate(volatile):
        messages = client.requests[index]["messages"]
        containing = [m for m in messages if "Datos del entorno" in str(m.get("content"))]
        assert len(containing) == 1, f"turno {index}: el bloque debe aparecer una sola vez"
        assert data in containing[0]["content"]
        users = [m for m in messages if m.get("role") == "user"]
        assert containing[0] == users[-1], "el bloque va en el último mensaje del usuario"


def test_una_transcripcion_atrasada_no_resuelve_la_confirmacion() -> None:
    answered: list[tuple[str, str | None]] = []
    stub = SimpleNamespace(
        in_call=True,
        _turn_gen=9,
        _confirm_token="tok-vigente",
        agent=SimpleNamespace(
            confirm_pending=True,
            submit_confirmation=lambda text, token=None: answered.append((text, token)),
        ),
        _speak_same_turn=lambda *args, **kwargs: None,
    )
    VoiceApp._handle_event(stub, ("transcribed", "sí", 1))
    assert answered == []

    VoiceApp._handle_event(stub, ("transcribed", "sí", 9))
    assert answered == [("sí", "tok-vigente")]


def test_un_error_atrasado_no_interrumpe_el_turno_actual() -> None:
    spoken: list[tuple] = []
    stub = SimpleNamespace(
        _turn_gen=9,
        in_call=False,
        _speak=lambda *args, **kwargs: spoken.append(args),
        _set_state=lambda state: None,
        _set_idle=lambda: None,
        _hang_up=lambda: None,
    )
    VoiceApp._handle_event(stub, ("error", "fallo viejo", 1))
    assert spoken == []

    VoiceApp._handle_event(stub, ("error", "fallo actual", 9))
    assert spoken and "fallo actual" in spoken[0][0]
