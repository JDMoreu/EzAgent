"""Messenger: bucle LLM + herramientas, con un proveedor y un runtime simulados."""

from __future__ import annotations

import json
from types import SimpleNamespace

import pytest

from ezagent.agent.registry import ToolResult
from ezagent.voice.llm import (
    ACK_FILLERS,
    MAX_HISTORY_MESSAGES,
    TOOL_MAX_TOKENS,
    Messenger,
    friendly_llm_error,
)


# ------------------------------------------------------------- dobles
class Stream:
    def __init__(self, chunks, boom: Exception | None = None) -> None:
        self.chunks, self.boom, self.closed = chunks, boom, False

    def __iter__(self):
        yield from self.chunks
        if self.boom:
            raise self.boom

    def close(self) -> None:
        self.closed = True


class Client:
    """Cada llamada a ``create`` consume el siguiente guion."""

    def __init__(self, *scripts) -> None:
        self.scripts = list(scripts)
        self.requests: list[dict] = []
        self.streams: list[Stream] = []
        self.chat = SimpleNamespace(completions=SimpleNamespace(create=self._create))

    def _create(self, **request):
        self.requests.append({**request, "messages": json.loads(json.dumps(request["messages"]))})
        script = self.scripts.pop(0)
        if isinstance(script, Exception):
            raise script
        stream = script if isinstance(script, Stream) else Stream(script)
        self.streams.append(stream)
        return stream


def text(content: str):
    return SimpleNamespace(
        choices=[SimpleNamespace(delta=SimpleNamespace(content=content, tool_calls=None))]
    )


def call(name: str | None, arguments: str = "", *, index: int = 0, call_id: str | None = None):
    function = SimpleNamespace(name=name, arguments=arguments)
    tool = SimpleNamespace(index=index, id=call_id, function=function)
    return SimpleNamespace(
        choices=[SimpleNamespace(delta=SimpleNamespace(content=None, tool_calls=[tool]))]
    )


def empty_chunk():
    return SimpleNamespace(choices=[])


class Runtime:
    def __init__(self, results=None, tools=None) -> None:
        self.executed: list[tuple[str, str]] = []
        self.recorded: list[str] = []
        self.turns: list[str] = []
        self.results = results or {}
        self.tools = (
            tools
            if tools is not None
            else [{"type": "function", "function": {"name": "get_datetime"}}]
        )
        self.prompt = "PROMPT-SISTEMA"
        self.refreshed = 0
        self.notify: str | None = None

    def begin_turn(self, text):
        self.turns.append(text)

    def system_prompt(self):
        return self.prompt

    def tools_for(self, text):
        return list(self.tools)

    def render_result(self, result):
        return json.dumps(result.to_dict(), ensure_ascii=False)

    def record_assistant(self, text):
        self.recorded.append(text)

    def refresh(self):
        self.refreshed += 1

    def set_confirm_actions(self, enabled):
        self.confirm = enabled

    def pending_notify_address(self):
        return self.notify

    def execute(self, name, raw):
        self.executed.append((name, raw))
        outcome = self.results.get(name, ToolResult(ok=True, data="resultado"))
        if isinstance(outcome, Exception):
            raise outcome
        return outcome


def make(client: Client, runtime=Runtime, **cfg) -> Messenger:
    values = dict(
        llm_provider="openai",
        llm_model="gpt-x",
        llm_temperature=0.3,
        llm_max_tokens=150,
        llm_base_url="https://api.openai.com/v1",
        api_key="k",
        llm_api_style="openai",
        llm_reasoning_effort="none",
        confirm_actions=False,
    )
    values.update(cfg)
    rt = runtime() if callable(runtime) else runtime
    messenger = Messenger(
        SimpleNamespace(**values),
        SimpleNamespace(),
        last_summary=lambda: "",
        watch=lambda a: None,
        mute=lambda m: None,
        request_close=lambda: None,
        mic_mute=lambda m: None,
        runtime=rt,
    )
    messenger.client = client
    return messenger


def say(messenger: Messenger, user: str = "hola") -> str:
    return "".join(messenger.ask_stream(user))


# ------------------------------------------------------------- conversación
def test_respuesta_simple_en_streaming_y_se_guarda_en_el_historial() -> None:
    client = Client([text("Hola, "), empty_chunk(), text("¿qué tal?")])
    messenger = make(client)
    pieces = list(messenger.ask_stream("hola"))
    assert pieces == ["Hola, ", "¿qué tal?"]
    assert messenger.messages[-1] == {"role": "assistant", "content": "Hola, ¿qué tal?"}
    assert messenger.messages[-2] == {"role": "user", "content": "hola"}
    assert messenger.messages[0] == {"role": "system", "content": "PROMPT-SISTEMA"}
    assert messenger.cfg.llm_model == "gpt-x"
    assert client.streams[0].closed  # el stream siempre se cierra
    assert len(messenger.last_rounds) == 1 and "llamada 1" in messenger.last_rounds[0]


def test_el_runtime_ve_el_turno_y_la_respuesta_final() -> None:
    runtime = Runtime()
    messenger = make(Client([text("Listo.")]), runtime)
    say(messenger, "abre algo")
    assert runtime.turns == ["abre algo"] and runtime.recorded == ["Listo."]


def test_ask_devuelve_texto_limpio_o_un_mensaje_de_respaldo() -> None:
    assert make(Client([text("  Hola  ")])).ask("x") == "Hola"
    assert make(Client([])).ask("x") == "No he podido completar la consulta."


def test_la_peticion_lleva_modelo_temperatura_y_herramientas() -> None:
    client = Client([text("ok")])
    make(client).ask("hola")
    (request,) = client.requests
    # Con herramientas hay holgura: una llamada con argumentos largos no puede cortarse a medias.
    assert request["model"] == "gpt-x" and request["stream"] is True
    assert request["max_tokens"] == TOOL_MAX_TOKENS
    assert request["temperature"] == 0.3
    assert (
        request["tools"][0]["function"]["name"] == "get_datetime"
        and request["tool_choice"] == "auto"
    )
    assert request["messages"][0]["content"] == "PROMPT-SISTEMA"


def test_sin_herramientas_no_se_envia_tool_choice() -> None:
    client = Client([text("ok")])
    make(client, Runtime(tools=[])).ask("hola")
    assert "tools" not in client.requests[0] and "tool_choice" not in client.requests[0]


@pytest.mark.parametrize(
    ("provider", "effort", "expected"),
    [
        ("opencode", "none", {"reasoning_effort": "none"}),  # se conserva el valor explícito
        ("openai", "high", {"reasoning_effort": "high"}),
        ("groq", "none", None),
        ("groq", "low", {"reasoning_effort": "low"}),
        ("openrouter", "medium", {"reasoning_effort": "medium"}),
        ("anthropic", "high", None),
        ("gemini", "high", None),
    ],
)
def test_el_esfuerzo_de_razonamiento_solo_va_a_quien_lo_acepta(provider, effort, expected) -> None:
    client = Client([text("ok")])
    make(client, llm_provider=provider, llm_reasoning_effort=effort).ask("hola")
    assert client.requests[0].get("extra_body") == expected


def test_anthropic_con_endpoint_openai_exige_temperatura_uno() -> None:
    client = Client([text("ok")])
    make(client, llm_provider="anthropic", llm_api_style="openai", llm_temperature=0.2).ask("x")
    assert client.requests[0]["temperature"] == 1.0
    native = Client([text("ok")])
    make(native, llm_provider="anthropic", llm_api_style="anthropic", llm_temperature=0.2).ask("x")
    assert native.requests[0]["temperature"] == 0.2


# ------------------------------------------------------------- herramientas
def tool_round(name: str, arguments: str, call_id: str = "call_1", *, preface: str | None = None):
    chunks = [text(preface)] if preface else []
    return chunks + [call(name, "", call_id=call_id), call(None, arguments)]


def test_ciclo_completo_de_herramienta() -> None:
    runtime = Runtime(results={"get_datetime": ToolResult(ok=True, data={"hora": "14:05"})})
    client = Client(tool_round("get_datetime", '{"tz":"UTC"}'), [text("Son las 14:05.")])
    messenger = make(client, runtime)
    spoken = say(messenger, "qué hora es")
    assert runtime.executed == [("get_datetime", '{"tz":"UTC"}')]
    assert spoken.endswith("Son las 14:05.") and spoken.split()[0:2] == ACK_FILLERS[0].split()[0:2]
    assistant, tool = messenger.messages[-3], messenger.messages[-2]
    assert assistant["tool_calls"][0] == {
        "id": "call_1",
        "type": "function",
        "function": {"name": "get_datetime", "arguments": '{"tz":"UTC"}'},
    }
    assert tool["role"] == "tool" and tool["tool_call_id"] == "call_1"
    assert json.loads(tool["content"])["data"] == {"hora": "14:05"}
    # La segunda llamada al modelo ya incluye el resultado de la herramienta.
    roles = [m["role"] for m in client.requests[1]["messages"]]
    assert roles[-2:] == ["assistant", "tool"]
    assert len(messenger.last_rounds) == 2 and "get_datetime" in messenger.last_rounds[0]


def test_sin_texto_previo_el_codigo_dice_un_acuse_y_rota_entre_ellos() -> None:
    messenger = make(Client())
    seen = []
    for i in range(len(ACK_FILLERS) + 1):
        messenger.client = Client(tool_round("web_search", "{}", f"c{i}"), [text("Hecho.")])
        seen.append(say(messenger, "busca").split(" Hecho.")[0].strip())
    assert seen[: len(ACK_FILLERS)] == list(ACK_FILLERS)
    assert seen[len(ACK_FILLERS)] == ACK_FILLERS[0]  # y vuelve a empezar


def test_si_el_modelo_ya_hablo_no_se_agrega_un_acuse() -> None:
    client = Client(
        tool_round("web_search", "{}", preface="Un momento, lo busco."), [text("Encontré tres.")]
    )
    spoken = say(make(client), "busca")
    assert not any(filler in spoken for filler in ACK_FILLERS)
    assert spoken == "Un momento, lo busco. Encontré tres."  # separado por un espacio


def test_voice_control_no_lleva_acuse() -> None:
    client = Client(tool_round("voice_control", '{"action":"mute"}'), [text("Silenciado.")])
    spoken = say(make(client), "silénciate")
    assert not any(filler in spoken for filler in ACK_FILLERS) and spoken == "Silenciado."


def test_varias_herramientas_en_una_ronda_se_ejecutan_en_orden_de_indice() -> None:
    runtime = Runtime()
    round_one = [
        call("beta", "", index=1, call_id="b"),
        call(None, "{}", index=1),
        call("alfa", "", index=0, call_id="a"),
        call(None, '{"x":1}', index=0),
    ]
    messenger = make(Client(round_one, [text("ok")]), runtime)
    say(messenger, "haz dos cosas")
    assert [n for n, _ in runtime.executed] == ["alfa", "beta"]
    tool_ids = [m["tool_call_id"] for m in messenger.messages if m["role"] == "tool"]
    assert tool_ids == ["a", "b"]


def test_un_fallo_del_runtime_se_dice_por_voz_y_el_modelo_recibe_el_error() -> None:
    runtime = Runtime(results={"read_file": RuntimeError("disco roto")})
    client = Client(tool_round("read_file", "{}", preface="Lo leo."), [text("No pude leerlo.")])
    messenger = make(client, runtime)
    spoken = say(messenger, "lee")
    assert "No pude ejecutar la herramienta" in spoken and spoken.endswith("No pude leerlo.")
    tool_message = next(m for m in messenger.messages if m["role"] == "tool")
    payload = json.loads(tool_message["content"])
    assert (
        payload["ok"] is False
        and "RuntimeError" in payload["error"]
        and "disco roto" in payload["error"]
    )


def test_repeated_web_queries_finish_with_results_instead_of_asking_to_rephrase() -> None:
    runtime = Runtime()
    client = Client(
        tool_round("web_search", "{}", "c1"),
        tool_round("web_search", "{}", "c2"),
        [text("Encontré resultados, pero aún faltan fuentes por verificar.")],
    )
    messenger = make(client, runtime)
    spoken = say(messenger, "busca sin parar")
    assert len(client.requests) == 3 and len(runtime.executed) == 1
    assert "tools" not in client.requests[-1]
    assert spoken.endswith("faltan fuentes por verificar.")
    assert messenger.messages[-1]["role"] == "assistant"


def test_el_error_del_proveedor_se_propaga_para_que_la_app_lo_diga() -> None:
    boom = RuntimeError("429 too many requests")
    messenger = make(Client(boom))
    with pytest.raises(RuntimeError, match="429"):
        say(messenger)


def test_un_corte_a_mitad_del_stream_cierra_el_flujo() -> None:
    client = Client(Stream([text("Hola")], boom=ConnectionError("cortado")))
    with pytest.raises(ConnectionError):
        say(make(client))
    assert client.streams[0].closed


def test_sin_runtime_no_hay_herramientas_y_lo_dice_en_el_prompt() -> None:
    client = Client([text("No puedo hacer eso ahora.")])
    messenger = make(client, runtime=None)
    assert "núcleo agéntico no está disponible" in messenger.messages[0]["content"]
    say(messenger, "borra todo")
    assert "tools" not in client.requests[0]


def test_sin_runtime_una_herramienta_pedida_por_el_modelo_no_se_ejecuta() -> None:
    client = Client(tool_round("delete_path", '{"path": "C:/"}'), [text("No puedo.")])
    spoken = say(make(client, runtime=None), "borra")
    assert "No pude ejecutar la herramienta" in spoken


def test_un_runtime_que_falla_al_empezar_el_turno_degrada_a_conversacion() -> None:
    class Broken(Runtime):
        def begin_turn(self, text):
            raise RuntimeError("memoria corrupta")

    client = Client([text("Hola igual.")])
    assert make(client, Broken()).ask("hola") == "Hola igual."
    assert "tools" not in client.requests[0]


# ----------------------------------------------------------------- historial
def test_el_historial_tiene_un_tope_duro_y_no_deja_tool_huerfanos() -> None:
    turns = MAX_HISTORY_MESSAGES  # el doble de mensajes que el tope
    messenger = make(Client(*[[text(f"r{i}")] for i in range(turns)]))
    for i in range(turns):
        say(messenger, f"pregunta {i}")
    assert len(messenger.messages) <= MAX_HISTORY_MESSAGES + 2
    assert messenger.messages[0]["role"] == "system" and messenger.messages[1]["role"] == "user"
    assert (
        messenger.messages[-1]["content"] == f"r{turns - 1}"
        and messenger.messages[-2]["content"] == f"pregunta {turns - 1}"
    )


def test_una_conversacion_normal_ya_no_olvida_las_primeras_vueltas() -> None:
    messenger = make(Client(*[[text(f"r{i}")] for i in range(12)]))
    for i in range(12):
        say(messenger, f"pregunta {i}")
    texts = [m["content"] for m in messenger.messages[1:]]
    assert "pregunta 0" in texts and "r0" in texts  # antes se perdían con solo 6 vueltas
    assert len(messenger.messages) == 1 + 24


def test_trim_nunca_empieza_con_un_resultado_de_herramienta() -> None:
    messenger = make(Client())
    messenger.messages = [{"role": "system", "content": "s"}]
    for i in range(10):
        messenger.messages += [
            {"role": "user", "content": f"u{i}"},
            {"role": "assistant", "content": "", "tool_calls": [{"id": f"c{i}"}]},
            {"role": "tool", "tool_call_id": f"c{i}", "content": "{}"},
            {"role": "assistant", "content": f"a{i}"},
        ]
    messenger._trim_history()
    assert messenger.messages[0]["role"] == "system" and messenger.messages[1]["role"] == "user"
    # Cada tool conserva a su assistant con tool_calls inmediatamente antes.
    for index, message in enumerate(messenger.messages):
        if message["role"] == "tool":
            assert messenger.messages[index - 1]["role"] == "assistant"
            assert messenger.messages[index - 1]["tool_calls"][0]["id"] == message["tool_call_id"]


def test_un_historial_corto_no_se_toca() -> None:
    messenger = make(Client())
    before = list(messenger.messages) + [{"role": "user", "content": "x"}] * 5
    messenger.messages = list(before)
    messenger._trim_history()
    assert messenger.messages == before


def test_reset_history_conserva_solo_el_sistema() -> None:
    messenger = make(Client([text("a")]))
    say(messenger)
    messenger.reset_history()
    assert [m["role"] for m in messenger.messages] == ["system"]


def test_reload_llm_regenera_el_prompt_y_refresca_el_runtime() -> None:
    runtime = Runtime()
    messenger = make(Client([text("a")]), runtime)
    say(messenger)
    runtime.prompt = "PROMPT-NUEVO"
    messenger.reload_llm()
    assert messenger.messages == [{"role": "system", "content": "PROMPT-NUEVO"}]
    assert runtime.refreshed == 1


def test_set_confirm_actions_actualiza_config_y_runtime() -> None:
    runtime = Runtime()
    messenger = make(Client(), runtime)
    messenger.set_confirm_actions(True)
    assert messenger.cfg.confirm_actions is True and runtime.confirm is True


def test_el_cliente_correcto_segun_el_estilo_de_api() -> None:
    from ezagent.voice.anthropic_client import AnthropicChatClient

    def build(style, provider="openai"):
        return Messenger(
            SimpleNamespace(
                llm_provider=provider,
                llm_api_style=style,
                api_key="",
                llm_base_url="https://x/v1",
                llm_model="m",
                llm_temperature=0,
                llm_max_tokens=10,
                confirm_actions=False,
            ),
            SimpleNamespace(),
            last_summary=lambda: "",
            watch=lambda a: None,
            mute=lambda m: None,
            request_close=lambda: None,
            mic_mute=lambda m: None,
        )

    assert isinstance(build("anthropic", "anthropic").client, AnthropicChatClient)
    assert not isinstance(build("openai").client, AnthropicChatClient)
    assert build("openai").client.max_retries == 0  # un 429 se avisa de inmediato


# --------------------------------------------------------------- avisos
def test_note_done_agrega_contexto_sin_pedir_resumen() -> None:
    messenger = make(Client())
    messenger.note_done("t1:2", "claude", "terminó", "salida X", task="Haz el plan")
    note = messenger.messages[-1]
    assert note["role"] == "system"
    assert "claude (t1:2) ha terminado (terminó)" in note["content"]
    assert "No des el resumen hasta que lo pida" in note["content"]
    assert "Haz el plan" in note["content"] and "Salida reciente:\nsalida X" in note["content"]
    messenger.note_done("t1:3", "codex", "ok", "o")
    assert "La tarea que pidió" not in messenger.messages[-1]["content"]


def test_pending_notify_address_delega_en_el_runtime() -> None:
    runtime = Runtime()
    runtime.notify = "t1:9"
    assert make(Client(), runtime).pending_notify_address() == "t1:9"
    runtime.pending_notify_address = lambda: (_ for _ in ()).throw(RuntimeError("x"))
    assert make(Client(), runtime).pending_notify_address() is None


# --------------------------------------------------------- errores amigables
class StatusError(Exception):
    def __init__(self, status: int, headers: dict | None = None) -> None:
        super().__init__(f"Error code: {status}")
        self.status_code = status
        self.response = SimpleNamespace(headers=headers or {})


def test_429_menciona_el_limite_y_el_retry_after() -> None:
    plain = friendly_llm_error(StatusError(429), "Groq", "m")
    assert "Groq" in plain and "demasiadas peticiones" in plain and "Espera un momento" in plain
    timed = friendly_llm_error(StatusError(429, {"retry-after": "42"}), "Groq", "m")
    assert "42 segundos" in timed
    junk = friendly_llm_error(StatusError(429, {"retry-after": "mañana"}), "Groq", "m")
    assert "Espera un momento" in junk


@pytest.mark.parametrize(
    ("status", "fragment"),
    [(413, "más grande"), (404, "no reconoce el modelo gpt-x"), (401, "clave"), (403, "clave")],
)
def test_errores_http_conocidos(status: int, fragment: str) -> None:
    message = friendly_llm_error(StatusError(status), "OpenAI", "gpt-x")
    assert fragment in message and "OpenAI" in message


@pytest.mark.parametrize(
    "name", ["APIConnectionError", "ConnectTimeout", "ReadTimeout", "ConnectError"]
)
def test_errores_de_red(name: str) -> None:
    error = type(name, (Exception,), {})("sin red")
    assert "No pude conectar con X" in friendly_llm_error(error, "X", "m")


def test_el_estado_se_deduce_del_texto_si_no_hay_atributo() -> None:
    assert "clave" in friendly_llm_error(RuntimeError("La API respondió 401: nope"), "Y", "m")
    assert "no reconoce" in friendly_llm_error(RuntimeError("HTTP 404: no model"), "Y", "m")


def test_error_desconocido_conserva_el_mensaje_original() -> None:
    assert friendly_llm_error(ValueError("algo raro"), "Y", "m") == "Y: algo raro"


def test_el_mensaje_amigable_no_filtra_la_clave() -> None:
    error = StatusError(401)
    error.args = ("Incorrect API key provided: sk-SECRETO",)
    assert "sk-SECRETO" not in friendly_llm_error(error, "OpenAI", "m")


# -------------------------------------------------------------------- modo chat
def test_el_modo_chat_permite_markdown_y_respuestas_largas() -> None:
    from ezagent.voice.llm import CHAT_MAX_TOKENS, CHAT_MODE_PROMPT

    client = Client([text("## Resumen\n\n- uno")])
    messenger = make(client)
    assert "".join(messenger.ask_stream("resume", mode="chat")) == "## Resumen\n\n- uno"
    request = client.requests[0]
    assert request["max_tokens"] == CHAT_MAX_TOKENS  # 150 en voz; en el chat cabe un documento
    system = request["messages"][0]["content"]
    assert system.startswith("PROMPT-SISTEMA") and CHAT_MODE_PROMPT in system
    assert "markdown" in system and "[Nombre](ruta completa)" in system


def test_el_modo_voz_no_cambia_ni_el_prompt_ni_los_tokens() -> None:
    from ezagent.voice.llm import CHAT_MODE_PROMPT

    client = Client([text("Hola.")], [text("Hola otra vez.")])
    say(make(client, Runtime(tools=[])), "hola")  # sin herramientas: el tope configurado
    say(make(client), "hola")  # con herramientas: holgura para llamadas largas
    sin, con = client.requests
    assert sin["max_tokens"] == 150 and con["max_tokens"] == TOOL_MAX_TOKENS
    for request in (sin, con):
        assert request["messages"][0]["content"] == "PROMPT-SISTEMA"
        assert CHAT_MODE_PROMPT not in request["messages"][0]["content"]


def test_el_bloque_del_chat_no_se_arrastra_al_turno_de_voz_siguiente() -> None:
    from ezagent.voice.llm import CHAT_MODE_PROMPT

    client = Client([text("Uno.")], [text("Dos.")])
    messenger = make(client)
    "".join(messenger.ask_stream("a", mode="chat"))
    messenger.client = Client([text("Dos.")])
    say(messenger, "b")
    assert CHAT_MODE_PROMPT not in messenger.client.requests[0]["messages"][0]["content"]
    assert messenger.client.requests[0]["max_tokens"] == TOOL_MAX_TOKENS


def test_el_chat_no_dice_acuses_de_relleno() -> None:
    client = Client(tool_round("web_search", "{}", "c1"), [text("Encontré tres resultados.")])
    out = "".join(make(client).ask_stream("busca", mode="chat"))
    assert not any(filler in out for filler in ACK_FILLERS)
    assert out == "Encontré tres resultados."


def test_en_el_chat_el_historial_es_el_mismo_que_en_voz() -> None:
    messenger = make(Client([text("Hola.")]))
    "".join(messenger.ask_stream("hola", mode="chat"))
    assert messenger.messages[-2:] == [
        {"role": "user", "content": "hola"},
        {"role": "assistant", "content": "Hola."},
    ]


def test_sin_runtime_el_chat_tampoco_deja_su_prompt_pegado() -> None:
    from ezagent.voice.llm import CHAT_MODE_PROMPT

    client = Client([text("Hola.")], [text("Adiós.")])
    messenger = make(client, runtime=None)
    "".join(messenger.ask_stream("hola", mode="chat"))
    assert CHAT_MODE_PROMPT in client.requests[0]["messages"][0]["content"]
    say(messenger, "adiós")
    assert CHAT_MODE_PROMPT not in client.requests[1]["messages"][0]["content"]
    assert "núcleo agéntico no está disponible" in client.requests[1]["messages"][0]["content"]
