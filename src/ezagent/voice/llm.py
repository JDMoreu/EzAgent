"""Mensajero LLM: proveedor compatible con OpenAI + herramientas de Herdr."""

from __future__ import annotations

import json
import os
import time
import uuid
from collections.abc import Callable, Iterator
from typing import Any

from loguru import logger
from openai import OpenAI

from .anthropic_client import AnthropicChatClient
from .config import VoiceConfig
from .herdr import Herdr

# Acuse que dice el código si el modelo pide una herramienta sin haber dicho
# nada en el turno: así no queda un silencio mientras trabaja.
ACK_FILLERS: tuple[str, ...] = (
    "Un momento, lo reviso.",
    "Dale, ya lo veo.",
    "Va, dame un segundo.",
    "Listo, déjame ver.",
)
# Herramientas instantáneas que no merecen acuse (silenciar, cerrar…).
_NO_ACK_TOOLS = frozenset({"voice_control"})


def _status_of(exc: BaseException) -> int | None:
    status = getattr(exc, "status_code", None)
    if isinstance(status, int):
        return status
    text = str(exc)
    for code in (401, 403, 404, 413, 429):
        if f" {code}" in text or f"{code}:" in text:
            return code
    return None


def friendly_llm_error(exc: BaseException, label: str, model: str) -> str:
    """Frase corta para decir por voz cuando falla el proveedor del LLM."""
    status = _status_of(exc)
    if status == 429:
        retry = ""
        response = getattr(exc, "response", None)
        after = response.headers.get("retry-after") if response is not None else None
        if after and str(after).isdigit():
            retry = f" Espera unos {int(after)} segundos o"
        else:
            retry = " Espera un momento o"
        return (
            f"{label} me limitó por demasiadas peticiones o tokens por minuto."
            f"{retry} elige otro modelo o proveedor en ajustes."
        )
    if status == 413:
        return (
            f"La petición es más grande de lo que permite tu plan de {label} por "
            "minuto. Usa otro proveedor o un plan con más tokens."
        )
    if status == 404:
        return f"{label} no reconoce el modelo {model}. Revisa el nombre en ajustes."
    if status in (401, 403):
        return f"La clave de {label} no es válida o no tiene acceso. Revísala en ajustes."
    name = type(exc).__name__
    if "Connection" in name or "Timeout" in name or "ConnectError" in name:
        return f"No pude conectar con {label}. Revisa la conexión o la dirección de la API."
    return f"{label}: {exc}"


class Messenger:
    def __init__(
        self,
        cfg: VoiceConfig,
        herdr: Herdr,
        *,
        last_summary: Callable[[], str],
        watch: Callable[[str], None],
        mute: Callable[[int], None],
        request_close: Callable[[], None],
        mic_mute: Callable[[bool], None],
        runtime: Any | None = None,
    ):
        self.cfg = cfg
        self.herdr = herdr
        self.last_summary = last_summary
        self.watch = watch
        self.mute = mute
        self.request_close = request_close
        self.mic_mute = mic_mute
        self.runtime = runtime
        self._configure_client()
        self.messages: list[dict[str, Any]] = [{"role": "system", "content": self._system_prompt()}]
        self._last_user_text = ""
        self._last_agent_address: str | None = None
        self._filler_index = 0
        # Desglose del último turno: una entrada por llamada al LLM.
        self.last_rounds: list[str] = []

    def _next_filler(self) -> str:
        filler = ACK_FILLERS[self._filler_index % len(ACK_FILLERS)]
        self._filler_index += 1
        return filler

    def _configure_client(self) -> None:
        headers = {"User-Agent": "EzAgent-Voice/0.1"}
        if self.cfg.llm_provider == "opencode":
            headers["x-opencode-session"] = uuid.uuid4().hex
        if self.cfg.llm_provider == "anthropic":
            workspace = os.getenv("ANTHROPIC_WORKSPACE_ID")
            if workspace:
                headers["anthropic-workspace-id"] = workspace
        if self._anthropic_style():
            self.client = AnthropicChatClient(
                api_key=self.cfg.api_key or "sin-clave",
                base_url=self.cfg.llm_base_url,
                default_headers=headers,
            )
            return
        # DECISION: sin reintentos automáticos. Ante un 429 el SDK espera el
        # retry-after (hasta 60 s) en silencio; por voz es mejor avisar ya.
        self.client = OpenAI(
            api_key=self.cfg.api_key or "sin-clave",
            base_url=self.cfg.llm_base_url or None,
            default_headers=headers,
            max_retries=0,
        )

    def _anthropic_style(self) -> bool:
        return str(getattr(self.cfg, "llm_api_style", "") or "") == "anthropic"

    def reload_llm(self) -> None:
        """Recrea el cliente tras cambiar proveedor, modelo, URL o clave."""
        self._configure_client()
        if self.runtime is not None:
            try:
                self.runtime.refresh()
            except Exception:
                pass
        self.messages = [{"role": "system", "content": self._system_prompt()}]
        self._last_user_text = ""
        self._last_agent_address = None

    def _system_prompt(self) -> str:
        if self.runtime is None:
            # DECISION: sin núcleo agéntico se conserva una conversación
            # mínima en vez de permitir herramientas que no existen.
            return (
                "Eres el asistente de voz de EzAgent. El núcleo agéntico no "
                "está disponible, así que no puedes usar herramientas ni "
                "lanzar agentes. Solo puedes responder conversando, en "
                "español neutro, en una o dos frases cortas y sin markdown. "
                "Si te piden una acción, di con claridad que no puedes "
                "ejecutarla en este momento."
            )
        return self.runtime.system_prompt()

    def set_confirm_actions(self, enabled: bool) -> None:
        self.cfg.confirm_actions = bool(enabled)
        if self.runtime is not None:
            try:
                self.runtime.set_confirm_actions(bool(enabled))
            except Exception:
                pass
        if self.messages and self.messages[0].get("role") == "system":
            self.messages[0]["content"] = self._system_prompt()

    def _wants_notify(self) -> bool:
        text = self._last_user_text.lower()
        return any(
            word in text
            for word in (
                "avísame",
                "avisame",
                "avisa",
                "avísame cuando",
                "me avisas",
                "me avise",
                "me avises",
                "notifícame",
                "notificame",
                "notifica",
                "cuando termine",
                "cuando acabe",
                "cuando hayas terminado",
                "cuando esté",
                "cuando este",
                "al terminar",
                "al acabar",
                "estate atento",
                "estate pendiente",
                "esté atento",
                "este atento",
                "vigila",
                "vigílame",
                "vigilame",
                "pendiente de",
                "me dice",
                "me digas",
                "me dices",
                "dime",
                "me pasas",
                "me cuentas",
                "resumen",
                "resúmeme",
                "resumeme",
            )
        )

    def _wants_close(self) -> bool:
        text = self._last_user_text.lower()
        return any(
            word in text
            for word in (
                "ciérrate",
                "cierrate",
                "cierra la escucha",
                "cierra el micro",
                "deja de escuchar",
                "sin escuchar",
            )
        )

    def pending_notify_address(self) -> str | None:
        """Último agente lanzado, si el usuario pidió que se le avise."""
        if self.runtime is not None:
            try:
                return self.runtime.pending_notify_address()
            except Exception:
                return None
        if self._wants_notify() and self._last_agent_address:
            return self._last_agent_address
        return None

    def reset_history(self) -> None:
        """Olvida la conversación (conserva solo el mensaje de sistema)."""
        self.messages = self.messages[:1]
        self._last_user_text = ""

    # ------------------------------------------------------------- tool loop
    def _trim_history(self) -> None:
        """Recorta por grupos completos, nunca entre assistant y tool."""
        if len(self.messages) <= 14:
            return
        system = self.messages[0]
        history = self.messages[1:]
        while len(history) > 12:
            history.pop(0)
        # El primer mensaje del historial debe ser del usuario: así no queda
        # un assistant.tool_calls sin sus resultados tool ni un tool huérfano.
        while history and history[0].get("role") != "user":
            history.pop(0)
        self.messages = [system] + history

    def ask(self, text: str) -> str:
        answer = "".join(self.ask_stream(text)).strip()
        return answer or "No he podido completar la consulta."

    def ask_stream(self, text: str) -> Iterator[str]:
        """Streaming de la respuesta, incluidas las llamadas a herramientas."""
        self._last_user_text = text
        runtime = self.runtime
        if runtime is not None:
            try:
                runtime.begin_turn(text)
                self.messages[0] = {
                    "role": "system",
                    "content": runtime.system_prompt(),
                }
            except Exception as exc:  # noqa: BLE001
                logger.warning(
                    "Núcleo agéntico: {}; sigo sin herramientas en este turno.",
                    exc,
                )
                runtime = None
        self.messages.append({"role": "user", "content": text})
        self._trim_history()
        self.last_rounds = []
        spoke = False
        for _ in range(4):
            tools: list[dict[str, Any]] = []
            if runtime is not None:
                try:
                    tools = runtime.tools_for(text)
                except Exception as exc:  # noqa: BLE001
                    logger.warning("No pude cargar las herramientas: {}", exc)
            request: dict[str, Any] = {
                "model": self.cfg.llm_model,
                "messages": self.messages,
                "temperature": (
                    # El endpoint OpenAI de Anthropic exige 1.0; la API
                    # Messages nativa acepta la temperatura configurada.
                    1.0
                    if self.cfg.llm_provider == "anthropic" and not self._anthropic_style()
                    else self.cfg.llm_temperature
                ),
                "max_tokens": self.cfg.llm_max_tokens,
                "stream": True,
            }
            if tools:
                request["tools"] = tools
                request["tool_choice"] = "auto"
            effort = str(getattr(self.cfg, "llm_reasoning_effort", "none") or "none")
            if self.cfg.llm_provider in {"opencode", "openai"}:
                # Se conserva el valor explícito (incluido «none») que ya
                # usaba OpenCode/OpenAI; el usuario puede subirlo.
                request["extra_body"] = {"reasoning_effort": effort}
            elif effort != "none" and self.cfg.llm_provider not in {"anthropic", "gemini"}:
                # Groq, OpenRouter y endpoints compatibles suelen aceptar el
                # parámetro; Anthropic/Gemini se dejan sin él.
                request["extra_body"] = {"reasoning_effort": effort}
            began = time.monotonic()
            first_chunk: float | None = None
            stream = self.client.chat.completions.create(**request)
            tool_calls: dict[int, dict[str, Any]] = {}
            content_parts: list[str] = []
            try:
                for chunk in stream:
                    if not chunk.choices:
                        continue
                    delta = chunk.choices[0].delta
                    if first_chunk is None and (delta.content or delta.tool_calls):
                        first_chunk = time.monotonic() - began
                    if delta.content:
                        content_parts.append(delta.content)
                        spoke = spoke or bool(delta.content.strip())
                        yield delta.content
                    for call in delta.tool_calls or []:
                        name = call.function.name if call.function else ""
                        if not spoke and name and name not in _NO_ACK_TOOLS:
                            spoke = True
                            yield self._next_filler() + " "
                        index = int(call.index or 0)
                        item = tool_calls.setdefault(
                            index,
                            {
                                "id": "",
                                "type": "function",
                                "function": {"name": "", "arguments": ""},
                            },
                        )
                        if call.id:
                            item["id"] = call.id
                        if call.function:
                            if call.function.name:
                                item["function"]["name"] += call.function.name
                            if call.function.arguments:
                                item["function"]["arguments"] += call.function.arguments
            finally:
                try:
                    stream.close()
                except Exception:
                    pass

            round_note = f"llamada {len(self.last_rounds) + 1}: " + (
                f"{first_chunk * 1000:.0f} ms al 1.er fragmento"
                if first_chunk is not None
                else "sin fragmentos"
            )
            if tool_calls:
                if content_parts:
                    yield " "  # separa el acuse del modelo de lo que siga
                ordered = [tool_calls[i] for i in sorted(tool_calls)]
                self.messages.append(
                    {
                        "role": "assistant",
                        "content": "".join(content_parts),
                        "tool_calls": ordered,
                    }
                )
                for call in ordered:
                    name = call["function"]["name"]
                    tool_began = time.monotonic()
                    try:
                        if runtime is None:
                            raise RuntimeError("el núcleo agéntico no está disponible")
                        content = runtime.render_result(
                            runtime.execute(name, call["function"]["arguments"])
                        )
                    except Exception as exc:  # noqa: BLE001
                        logger.warning("Herramienta {}: {}", name, exc)
                        # El aviso sale por voz, no solo al log.
                        yield ("No pude ejecutar la herramienta: el núcleo agéntico falló.")
                        content = json.dumps(
                            {
                                "ok": False,
                                "error": (
                                    f"Error del núcleo agéntico: {type(exc).__name__}: {exc}"
                                ),
                            },
                            ensure_ascii=False,
                        )
                    self.messages.append(
                        {
                            "role": "tool",
                            "tool_call_id": call["id"],
                            "content": content,
                        }
                    )
                    round_note += f", {name} {(time.monotonic() - tool_began) * 1000:.0f} ms"
                self.last_rounds.append(round_note)
                continue
            self.last_rounds.append(round_note)

            content = "".join(content_parts).strip()
            self.messages.append({"role": "assistant", "content": content})
            if runtime is not None and content:
                try:
                    runtime.record_assistant(content)
                except Exception:
                    pass
            return
        yield "Me he quedado dando vueltas con las herramientas; prueba a preguntar de otra forma."

    # --------------------------------------------------------------- avisos
    def note_done(
        self,
        address: str,
        agent: str,
        reason: str,
        output: str,
        *,
        task: str = "",
    ) -> None:
        """Añade al contexto que un agente terminó, sin resumir todavía."""
        task_line = (
            f"La tarea que pidió el usuario era:\n{task}\n" if str(task or "").strip() else ""
        )
        self.messages.append(
            {
                "role": "system",
                "content": (
                    f"El agente {agent} ({address}) ha terminado ({reason}). "
                    "El usuario puede pedir la salida literal, un resumen, "
                    "verla en pantalla o una nueva instrucción. No des el "
                    "resumen hasta que lo pida.\n"
                    f"{task_line}"
                    f"Salida reciente:\n{output}"
                ),
            }
        )
