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

from . import context as ctx_mgr
from .anthropic_client import AnthropicChatClient
from .config import VoiceConfig
from .herdr import Herdr
from .tool_json import is_tool_json_error, repair_arguments, sanitize_history

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

# Modo chat: el usuario escribe y lee en pantalla (a veces también escucha la respuesta).
# Sustituye la regla de «una o dos frases, sin markdown» del modo voz.
CHAT_MODE_PROMPT = (
    "# Modo chat\n"
    "El usuario te escribe y lee tu respuesta en una ventana de chat. Aquí puedes usar "
    "markdown (títulos, listas, **negrita**, tablas, bloques de código) y respuestas más "
    "largas cuando ayuden: resúmenes, comparativas, documentos, pasos. Empieza por lo "
    "importante y sé claro, sin relleno. Si la respuesta es larga, abre con una o dos "
    "frases que se entiendan solas.\n"
    "- Para enseñar un archivo o documento que creaste o encontraste, escribe en una "
    "línea sola `[Nombre](ruta completa)`; para una imagen local, `![descripción](ruta "
    "completa)`. Usa solo rutas reales que hayas visto en una herramienta; nunca las "
    "inventes.\n"
    "- No uses HTML. No pegues enlaces de imágenes de la web."
)
CHAT_MAX_TOKENS = 2400
# Con herramientas disponibles, la respuesta puede ser una llamada con argumentos largos (el
# prompt de un agente). Con el tope de voz (~320) se cortaba a mitad y quedaba JSON roto.
TOOL_MAX_TOKENS = 1500
# Tope duro de mensajes en el historial; el límite real lo pone la ventana de contexto.
MAX_HISTORY_MESSAGES = 60


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
    text = str(exc).lower()
    if status == 500 and "tool call" in text and ("parse" in text or "json" in text):
        return (
            f"{label} cortó una llamada a herramienta a la mitad y no pudo leerla. "
            "Ya limpié el historial; vuelve a intentarlo."
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
        # Resumen de lo que ya salió del historial por la ventana de contexto.
        self.summary = ""
        # Aviso de contexto («conversación compactada») para el chat; la app lo conecta.
        self.on_context_event: Callable[[str], None] | None = None
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
                timeout=self._request_timeout(),
            )
            return
        # DECISION: sin reintentos automáticos. Ante un 429 el SDK espera el
        # retry-after (hasta 60 s) en silencio; por voz es mejor avisar ya.
        self.client = OpenAI(
            api_key=self.cfg.api_key or "sin-clave",
            base_url=self.cfg.llm_base_url or None,
            default_headers=headers,
            max_retries=0,
            timeout=self._request_timeout(),
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
        self.summary = ""

    def export_context(self) -> tuple[list[dict[str, Any]], str, str]:
        """El contexto de la conversación actual: ``(mensajes, resumen, último texto del usuario)``.

        Lo usa el chat para guardar cada conversación por separado al cambiar de una a otra.
        """
        return self.messages, self.summary, self._last_user_text

    def import_context(
        self, messages: list[dict[str, Any]], summary: str = "", last_user: str = ""
    ) -> None:
        """Pone ese contexto como el de la conversación actual (con el prompt de sistema vigente)."""
        body = [m for m in messages if m.get("role") != "system"]
        self.messages = [{"role": "system", "content": self._system_prompt()}, *body]
        self.summary = summary
        self._last_user_text = last_user

    # ------------------------------------------------------------- tool loop
    def _trim_history(self, keep: int = 12) -> None:
        """Recorta por grupos completos, nunca entre assistant y tool."""
        if len(self.messages) <= keep + 2:
            return
        system = self.messages[0]
        history = self.messages[1:]
        while len(history) > keep:
            history.pop(0)
        # El primer mensaje del historial debe ser del usuario: así no queda
        # un assistant.tool_calls sin sus resultados tool ni un tool huérfano.
        while history and history[0].get("role") != "user":
            history.pop(0)
        self.messages = [system] + history

    def ask(self, text: str) -> str:
        answer = "".join(self.ask_stream(text)).strip()
        return answer or "No he podido completar la consulta."

    def ask_stream(self, text: str, mode: str = "voice") -> Iterator[str]:
        """Streaming de la respuesta, incluidas las llamadas a herramientas.

        ``mode="chat"`` es para el chat escrito: markdown permitido, respuestas largas y sin acuses
        («un momento, lo reviso»), que solo tienen sentido hablados.
        """
        chat = mode == "chat"
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
        if runtime is None:
            # Sin núcleo agéntico el prompt es fijo: se rehace para no arrastrar el del chat.
            self.messages[0] = {"role": "system", "content": self._system_prompt()}
        core = str(self.messages[0].get("content") or "")
        sanitize_history(self.messages)  # nunca se manda JSON roto de una llamada anterior
        self._manage_context()  # puede resumir lo viejo: el prompt se arma después, con el resumen
        base = ctx_mgr.with_summary(core, self.summary) if self.summary else core
        if chat:
            base = f"{base}\n\n{CHAT_MODE_PROMPT}"
        self.messages[0] = {"role": "system", "content": base}
        self.messages.append({"role": "user", "content": text})
        self._trim_history(MAX_HISTORY_MESSAGES)
        self.last_rounds = []
        spoke = chat  # el acuse de relleno es para la voz; en el chat no hace falta
        read_cache: dict[tuple[str, str], str] = {}
        finish_turn = False
        max_rounds = self._tool_round_limit()
        # DECISION: reserve one model response without tools to deliver the
        # results; the runtime's configured execution budget is never increased.
        for round_index in range(max_rounds + 1):
            remaining = self._remaining_tools(runtime)
            final = finish_turn or round_index == max_rounds or remaining == 0
            tools: list[dict[str, Any]] = []
            if runtime is not None and not final:
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
                "max_tokens": self._max_tokens(chat, bool(tools)),
                "stream": True,
            }
            if final:
                request["messages"] = self.messages + [
                    {
                        "role": "system",
                        "content": (
                            "Cierra ahora con los resultados ya obtenidos, sin herramientas. "
                            "No repitas consultas, no prometas seguir en otro turno ni pidas reformular. "
                            "Distingue lo verificado, lo pendiente y los archivos realmente creados. "
                            "Si delegaste, informa su dirección real y que la tarea sigue pendiente. "
                            "Por voz usa como máximo dos frases."
                        ),
                    }
                ]
            if tools:
                request["tools"] = tools
                request["tool_choice"] = "auto"
                if remaining is not None:
                    request["messages"] = self.messages + [
                        {
                            "role": "system",
                            "content": (
                                f"Quedan {remaining} llamadas a herramientas en este turno. "
                                "Planifica con ese presupuesto: si el trabajo requiere más pasos, "
                                "prioriza delegarlo antes de agotar las llamadas."
                            ),
                        }
                    ]
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
            stream = self._open_stream(request)
            finish_reason: str | None = None
            tool_calls: dict[int, dict[str, Any]] = {}
            content_parts: list[str] = []
            try:
                for chunk in stream:
                    if not chunk.choices:
                        continue
                    delta = chunk.choices[0].delta
                    finish_reason = (
                        getattr(chunk.choices[0], "finish_reason", None) or finish_reason
                    )
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
                complete = self._seal_tool_calls(ordered, finish_reason)
                self.messages.append(
                    {
                        "role": "assistant",
                        "content": "".join(content_parts),
                        "tool_calls": ordered,
                    }
                )
                for number, call in enumerate(ordered):
                    name = call["function"]["name"]
                    tool_began = time.monotonic()
                    try:
                        content, repeated = self._run_tool(
                            runtime,
                            call,
                            complete[number],
                            read_cache,
                            allow=not final,
                        )
                        finish_turn = finish_turn or repeated
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
                if final:
                    break
                continue
            self.last_rounds.append(round_note)

            content = "".join(content_parts).strip()
            if final and not content:
                break
            self._record_answer(content, runtime)
            return
        fallback = (
            "Conservé los resultados obtenidos, pero no pude completar la tarea. "
            "Todavía no hay una entrega final confirmada."
        )
        self._record_answer(fallback, runtime)
        yield fallback

    @staticmethod
    def _remaining_tools(runtime: Any) -> int | None:
        remaining = getattr(runtime, "remaining_tool_calls", None)
        return max(0, int(remaining())) if callable(remaining) else None

    def _request_timeout(self) -> float:
        try:
            timeout = float(getattr(self.cfg, "llm_timeout_secs", 30.0))
        except (TypeError, ValueError):
            return 30.0
        return timeout if 1 <= timeout <= 120 else 30.0

    def _tool_round_limit(self) -> int:
        try:
            return max(1, min(12, int(getattr(self.cfg, "llm_max_tool_rounds", 8))))
        except (TypeError, ValueError, OverflowError):
            return 8

    def _run_tool(
        self,
        runtime: Any,
        call: dict[str, Any],
        complete: bool,
        cache: dict[tuple[str, str], str],
        *,
        allow: bool,
    ) -> tuple[str, bool]:
        if not allow or self._remaining_tools(runtime) == 0:
            return json.dumps(
                {"ok": False, "error": "Finaliza con los resultados disponibles."}
            ), True
        if runtime is None:
            raise RuntimeError("el núcleo agéntico no está disponible")
        if not complete:
            return json.dumps(
                {
                    "ok": False,
                    "error": (
                        "Los argumentos de la herramienta llegaron incompletos "
                        "(la respuesta se cortó). Repite la llamada con menos texto."
                    ),
                }
            ), False
        name, raw = call["function"]["name"], call["function"]["arguments"]
        key = None
        if name in {"web_search", "fetch_page"}:
            key = (name, json.dumps(json.loads(raw), sort_keys=True, ensure_ascii=False))
            if key in cache:
                # DECISION: reuse only identical web reads in this turn; never
                # cache writes, confirmations or commands sent to an agent.
                return cache[key], True
        content = runtime.render_result(runtime.execute(name, raw))
        if key is not None:
            cache[key] = content
        return content, False

    def _record_answer(self, content: str, runtime: Any) -> None:
        self.messages.append({"role": "assistant", "content": content})
        if runtime is not None and content:
            try:
                runtime.record_assistant(content)
            except Exception:
                pass

    # ------------------------------------------------- JSON, contexto y recuperación
    def _max_tokens(self, chat: bool, has_tools: bool) -> int:
        """Tope de la respuesta: el configurado, más holgura si habrá un documento o una herramienta."""
        limit = int(self.cfg.llm_max_tokens)
        if chat:
            return max(limit, CHAT_MAX_TOKENS)
        return max(limit, TOOL_MAX_TOKENS) if has_tools else limit

    @staticmethod
    def _seal_tool_calls(ordered: list[dict[str, Any]], finish_reason: str | None) -> list[bool]:
        """Deja válidos los argumentos que se guardan y dice cuáles llegaron completos."""
        complete: list[bool] = []
        for item in ordered:
            fixed, ok = repair_arguments(item["function"]["arguments"])
            if not ok:
                logger.warning(
                    "Argumentos de {} incompletos (fin: {}); se reparan para el historial.",
                    item["function"]["name"],
                    finish_reason or "?",
                )
            item["function"]["arguments"] = fixed
            complete.append(ok)
        return complete

    def _open_stream(self, request: dict[str, Any]) -> Any:
        """Abre la petición; si el servidor rechaza JSON de herramientas del historial, lo arregla.

        Primero reintenta con los argumentos reparados y, si aun así falla, sin las llamadas a
        herramientas anteriores: la conversación nunca se queda atascada.
        """
        extra_messages = request["messages"][len(self.messages) :]
        try:
            return self.client.chat.completions.create(**request)
        except Exception as exc:  # noqa: BLE001
            if not is_tool_json_error(exc):
                raise
            logger.warning("El servidor rechazó argumentos de herramienta del historial: {}", exc)
        fixed = sanitize_history(self.messages)
        try:
            logger.info("Reparé {} llamada(s) del historial y reintento.", fixed)
            return self.client.chat.completions.create(**request)
        except Exception as exc:  # noqa: BLE001
            if not is_tool_json_error(exc):
                raise
        self._strip_tool_history()
        request["messages"] = [*self.messages, *extra_messages]
        logger.warning("Último recurso: reintento sin las llamadas a herramientas anteriores.")
        return self.client.chat.completions.create(**request)

    def _strip_tool_history(self) -> None:
        """Quita las llamadas a herramientas y sus resultados del historial (queda lo conversado)."""
        kept: list[dict[str, Any]] = [self.messages[0]]
        for message in self.messages[1:]:
            if message.get("role") == "tool":
                continue
            if message.get("tool_calls"):
                if str(message.get("content") or "").strip():
                    kept.append({"role": "assistant", "content": message["content"]})
                continue
            kept.append(message)
        self.messages[:] = kept

    def _notify_context(self, text: str) -> None:
        if self.on_context_event is not None:
            try:
                self.on_context_event(text)
            except Exception:  # noqa: BLE001
                pass

    def _manage_context(self) -> None:
        """Mantiene la conversación dentro de la ventana de contexto configurada.

        Primero recorta resultados de herramientas viejos (lo que más gasta) y, al acercarse al
        límite, resume lo antiguo en vez de olvidarlo.
        """
        budget = ctx_mgr.budget_tokens(getattr(self.cfg, "llm_context_tokens", 32_000))
        reserve = 3_000 + int(self.cfg.llm_max_tokens)  # esquemas de herramientas + respuesta
        if ctx_mgr.needs_clipping(self.messages, budget):
            ctx_mgr.clip_old_tool_results(self.messages)
        if ctx_mgr.needs_compaction(self.messages, budget, reserve):
            self._compact(budget)

    def _compact(self, budget: int) -> None:
        reserve = 3_000 + int(self.cfg.llm_max_tokens)
        old, recent = ctx_mgr.fit_recent(self.messages, budget, reserve)
        if not old:
            # Una sola vuelta (enorme): no hay nada que resumir; como último recurso se recortan
            # incluso los resultados de herramientas recientes.
            ctx_mgr.clip_old_tool_results(self.messages, protect_last_turns=0)
            return
        before = ctx_mgr.estimate_tokens(self.messages)
        self._notify_context("Compactando la conversación para no llenar el contexto…")
        try:
            summary = self._summarize(old)
        except Exception as exc:  # noqa: BLE001
            logger.warning("No pude resumir la conversación: {}", exc)
            summary = ""
        if summary:
            self.summary = summary
        self.messages = [self.messages[0]] + recent
        after = ctx_mgr.estimate_tokens(self.messages)
        logger.info(
            "Contexto compactado: {} -> {}",
            ctx_mgr.describe(before, budget),
            ctx_mgr.describe(after, budget),
        )
        self._notify_context(
            "Conversación compactada: lo anterior queda resumido."
            if summary
            else "No pude resumir lo anterior; lo más antiguo se olvidó para seguir."
        )

    def _summarize(self, old: list[dict[str, Any]]) -> str:
        """Pide al propio modelo un resumen de ``old`` (integrando el resumen previo)."""
        messages = ctx_mgr.summary_messages(old, self.summary)
        return self._complete_text(messages, ctx_mgr.SUMMARY_TOKENS).strip()[:6000]

    def _complete_text(self, messages: list[dict[str, Any]], max_tokens: int) -> str:
        """Una respuesta de texto sin herramientas (se lee el streaming entero)."""
        request: dict[str, Any] = {
            "model": self.cfg.llm_model,
            "messages": messages,
            "temperature": (
                1.0 if self.cfg.llm_provider == "anthropic" and not self._anthropic_style() else 0.2
            ),
            "max_tokens": max_tokens,
            "stream": True,
        }
        stream = self.client.chat.completions.create(**request)
        parts: list[str] = []
        try:
            for chunk in stream:
                if chunk.choices and chunk.choices[0].delta.content:
                    parts.append(chunk.choices[0].delta.content)
        finally:
            try:
                stream.close()
            except Exception:  # noqa: BLE001
                pass
        return "".join(parts)

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
