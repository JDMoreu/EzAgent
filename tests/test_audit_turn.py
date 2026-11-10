"""Regresiones de integridad del turno: cancelación, historial y eventos atrasados.

Cubren F01/F38 (cancelar no deja el historial inválido ni sigue pidiendo al
modelo), F02 (una transcripción vieja no resuelve la confirmación vigente) y
F29 (un error viejo no interrumpe el turno actual).
"""

from __future__ import annotations

import json
import queue
import threading
import time
from types import SimpleNamespace

from ezagent.agent.registry import ToolResult
from ezagent.voice.app import LISTENING, THINKING, VoiceApp
from ezagent.voice.config import VoiceConfig
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


def _consume(messenger: Messenger, text: str, results: dict[str, str], key: str) -> None:
    try:
        results[key] = "".join(messenger.ask_stream(text))
    except TurnCancelled:
        results[key] = ""


def test_el_turno_que_espera_avisa_una_sola_vez() -> None:
    runtime = Runtime()
    client = Client([call("slow", call_id="c1")], [text("segundo")])
    messenger = make(client, runtime)
    busy: list[tuple[float, str]] = []
    messenger.on_busy = lambda mode: busy.append((time.monotonic(), mode))
    results: dict[str, str] = {}

    first = threading.Thread(target=_consume, args=(messenger, "uno", results, "uno"), daemon=True)
    first.start()
    assert runtime.entered.wait(2)

    started = time.monotonic()
    second = threading.Thread(target=_consume, args=(messenger, "dos", results, "dos"), daemon=True)
    second.start()
    deadline = time.monotonic() + 3
    while not busy and time.monotonic() < deadline:
        time.sleep(0.02)
    assert len(busy) == 1, "on_busy debe llamarse una sola vez"
    assert busy[0][1] == "voice"
    assert busy[0][0] - started >= 1.4, "no debe avisar antes del timeout de 1,5 s"

    runtime.release.set()
    first.join(3)
    second.join(3)
    assert not first.is_alive() and not second.is_alive()
    assert results["dos"] == "segundo"


def test_no_avisa_si_el_turno_anterior_suelta_antes() -> None:
    runtime = Runtime()
    client = Client([call("slow", call_id="c1")], [text("segundo")])
    messenger = make(client, runtime)
    busy: list[tuple[float, str]] = []
    messenger.on_busy = lambda mode: busy.append((time.monotonic(), mode))
    results: dict[str, str] = {}

    first = threading.Thread(target=_consume, args=(messenger, "uno", results, "uno"), daemon=True)
    first.start()
    assert runtime.entered.wait(2)
    second = threading.Thread(target=_consume, args=(messenger, "dos", results, "dos"), daemon=True)
    second.start()
    runtime.release.set()
    first.join(3)
    second.join(3)
    assert busy == []
    assert results["dos"] == "segundo"


def test_cancelar_la_espera_no_dice_nada() -> None:
    runtime = Runtime()
    client = Client([call("slow", call_id="c1")], [text("segundo")])
    messenger = make(client, runtime)
    busy: list[tuple[float, str]] = []
    messenger.on_busy = lambda mode: busy.append((time.monotonic(), mode))
    results: dict[str, str] = {}

    first = threading.Thread(target=_consume, args=(messenger, "uno", results, "uno"), daemon=True)
    first.start()
    assert runtime.entered.wait(2)
    second = threading.Thread(target=_consume, args=(messenger, "dos", results, "dos"), daemon=True)
    second.start()
    time.sleep(0.3)
    messenger.cancel_turn()
    runtime.release.set()
    first.join(3)
    second.join(3)
    assert busy == []
    assert results["dos"] == ""


def test_el_aviso_de_espera_en_voz_se_muestra_y_se_dice() -> None:
    notes: list[str] = []
    spoken: list[str] = []
    stub = SimpleNamespace(
        _turn_gen=5,
        chat=SimpleNamespace(add_note=lambda text: notes.append(text), voice_replies=False),
        _speak_busy=lambda text, generation: spoken.append(text),
    )
    VoiceApp._handle_event(stub, ("busy_notice", 5, "voice"))
    assert notes == ["Termino lo anterior y sigo."]
    assert spoken == ["Termino lo anterior y sigo."]

    # Un aviso de un turno ya cancelado no se dice ni se muestra.
    VoiceApp._handle_event(stub, ("busy_notice", 4, "voice"))
    assert len(notes) == 1 and len(spoken) == 1


def test_el_aviso_de_espera_en_chat_sin_voz_solo_deja_la_nota() -> None:
    notes: list[str] = []
    spoken: list[str] = []
    stub = SimpleNamespace(
        _turn_gen=5,
        chat=SimpleNamespace(add_note=lambda text: notes.append(text), voice_replies=False),
        _speak_busy=lambda text, generation: spoken.append(text),
    )
    VoiceApp._handle_event(stub, ("busy_notice", 5, "chat"))
    assert notes == ["Termino lo anterior y sigo."]
    assert spoken == []


def test_el_aviso_de_espera_en_chat_con_voz_se_dice() -> None:
    notes: list[str] = []
    spoken: list[str] = []
    stub = SimpleNamespace(
        _turn_gen=5,
        chat=SimpleNamespace(add_note=lambda text: notes.append(text), voice_replies=True),
        _speak_busy=lambda text, generation: spoken.append(text),
    )
    VoiceApp._handle_event(stub, ("busy_notice", 5, "chat"))
    assert notes == ["Termino lo anterior y sigo."]
    assert spoken == ["Termino lo anterior y sigo."]


def test_el_aviso_de_espera_cuenta_como_tts_para_el_vad() -> None:
    stub = SimpleNamespace(state=THINKING, _busy_speaking=threading.Event())
    assert VoiceApp._is_speaking(stub) is False
    stub._busy_speaking.set()
    assert VoiceApp._is_speaking(stub) is True


def test_un_arranque_de_voz_durante_el_aviso_se_ignora() -> None:
    cancels: list[int] = []
    stub = SimpleNamespace(
        in_call=True,
        muted=False,
        state=THINKING,
        _busy_speaking=threading.Event(),
        _ignore_utterance=False,
        _post_tts_ignore=False,
        _turn_gen=5,
        messenger=SimpleNamespace(cancel_turn=lambda: cancels.append(1)),
    )
    stub._busy_speaking.set()
    VoiceApp._on_speech_start(stub, 0.1)
    assert stub._turn_gen == 5 and cancels == []
    assert stub._ignore_utterance is True


def test_un_barge_in_durante_el_aviso_solo_corta_la_frase() -> None:
    stopped: list[int] = []
    states: list[str] = []
    cancels: list[int] = []
    stub = SimpleNamespace(
        muted=False,
        in_call=True,
        state=THINKING,
        cfg=SimpleNamespace(barge_in=True),
        _busy_speaking=threading.Event(),
        _close_after_answer=False,
        _user_speaking=False,
        _ignore_utterance=False,
        _post_tts_ignore=False,
        _turn_gen=5,
        _signal_tts_stop=lambda: stopped.append(1),
        _set_state=lambda state: states.append(state),
        messenger=SimpleNamespace(cancel_turn=lambda: cancels.append(1)),
    )
    stub._busy_speaking.set()
    VoiceApp._on_barge_in(stub, 0.1)
    assert stub._turn_gen == 5 and cancels == []
    assert stopped == [1] and not stub._busy_speaking.is_set()
    assert states == [LISTENING]


def test_al_terminar_el_aviso_aplica_la_ventana_post_tts() -> None:
    stub = SimpleNamespace(
        _turn_gen=5,
        _busy_speaking=threading.Event(),
        _ignore_utterance=True,
        _post_tts_ignore=True,
        _post_tts_until=0.0,
        state=THINKING,
        _reset_capture=threading.Event(),
    )
    stub._busy_speaking.set()
    before = time.monotonic()
    VoiceApp._handle_event(stub, ("busy_done", 5))
    assert not stub._busy_speaking.is_set()
    assert stub._post_tts_until >= before + 0.2
    assert stub._reset_capture.is_set()


def test_speak_busy_marca_el_aviso_y_emite_busy_done() -> None:
    events: queue.Queue = queue.Queue()
    spoken: list[str] = []
    stub = SimpleNamespace(
        _busy_speaking=threading.Event(),
        _tts_guard=lambda: threading.Lock(),
        _tts_stop=threading.Event(),
        speaker=SimpleNamespace(speak=lambda text, stop: spoken.append(text)),
        events=events,
        _start_worker=lambda target, args=(), name="": target(*args),
    )
    VoiceApp._speak_busy(stub, "frase", 5)
    assert stub._busy_speaking.is_set()
    assert spoken == ["frase"]
    assert events.get_nowait() == ("busy_done", 5)


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


def test_migracion_del_timeout_stt(tmp_path) -> None:
    path = tmp_path / "voice.json"
    path.write_text(json.dumps({"stt_timeout_secs": 60.0}), encoding="utf-8")
    assert VoiceConfig.load(path).stt_timeout_secs == 10.0

    path.write_text(json.dumps({"stt_timeout_secs": 25.0}), encoding="utf-8")
    assert VoiceConfig.load(path).stt_timeout_secs == 25.0

    # Un 60 explícito con la versión nueva se respeta: no se re-migra.
    path.write_text(
        json.dumps({"stt_timeout_secs": 60.0, "defaults_version": 2}),
        encoding="utf-8",
    )
    assert VoiceConfig.load(path).stt_timeout_secs == 60.0
