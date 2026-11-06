"""Mensajero LLM: proveedor compatible con OpenAI + herramientas de Herdr."""

from __future__ import annotations

import json
import os
import uuid
from collections.abc import Callable, Iterator
from typing import Any

from openai import OpenAI

from .config import VoiceConfig
from .herdr import Herdr, HerdrError

SYSTEM_PROMPT = """Eres el mensajero de voz de Herdr. No programas ni haces tareas:
escuchas al usuario, consultas Herdr y delegas en los agentes.

Responde siempre en español de España, en una frase corta y natural para voz cuando ejecutes una acción; en consultas, 1-3 frases.
No uses markdown, listas, emojis ni URLs.

Herramientas:
- herdr_list_agents: agentes vivos (estado, app, carpeta, pestaña).
- herdr_get_agent: metadatos de un terminal.
- herdr_read_agent: últimas líneas del buffer de un agente.
- herdr_send_agent: enviar una tarea o mensaje a un agente.
- herdr_run_agent: abrir una pestaña visible en Herdr y lanzar el agente.
- herdr_open_terminal: abrir una terminal visible en una pestaña nueva.
- herdr_focus_agent: enfocar un agente para verlo.
- herdr_kill_agent: terminar un agente.
- herdr_watch_agent: avisar por voz cuando ese agente termine o se bloquee,
  sin cerrar la escucha.
- voice_close_and_watch: cerrar la escucha y avisar cuando el agente termine;
  al terminar, el sistema reabre la escucha y te pregunta qué quieres.
- voice_close: cerrar la escucha cuando el usuario diga "deja de escuchar",
  "cierra" o "adiós".
- voice_mic_mute: silenciar o reactivar el micrófono (no los avisos).
- voice_repeat_last: recuperar el último resumen hablado.
- voice_mute: silenciar los avisos durante N minutos.

VISIBILIDAD (importante):
- Por defecto TODO debe verse en Herdr. Usa herdr_run_agent con
  background=false (abre una pestaña visible) o herdr_open_terminal.
- Solo usa background=true si el usuario dice explícitamente "en segundo
  plano", "sin mostrarlo" o "no quiero verlo".
- Para abrir una terminal usa herdr_open_terminal. Para lanzar un agente usa
  herdr_run_agent. No uses `hc run` a mano.
- Si la tarea es de interfaz gráfica (abrir Brave, YouTube, navegar, hacer
  clic, escribir en apps), pon gui=true; el agente usará la skill cua-driver
  y dejará las ventanas a la vista.
- Cuando lances algo, di en qué pestaña/terminal está y qué está haciendo.
  No digas "en segundo plano" si es visible. Puedes leer el terminal con
  herdr_read_agent para resumir el progreso.
- Si la tarea tiene varios pasos (terminal + agente + CUA), ábrelos en
  pestañas visibles y delega; no lo hagas todo tú.

Reglas:
1. Para "¿qué está haciendo X?", usa herdr_list_agents, localiza el agente por
   app/nombre/carpeta, lee su buffer con herdr_read_agent y resume. Si hay varios
   candidatos, pregunta cuál.
2. Para "avísame cuando termine", usa herdr_watch_agent y confirma.
3. Para "ciérrate y avísame cuando termine", usa voice_close_and_watch:
   cierra la escucha al terminar de hablar y avisa cuando el agente acabe.
   Al acabar, el sistema reabre la escucha y te pregunta qué quiere el usuario.
4. Si lanzas un agente y el usuario pide que le avises al terminar, pon
   notify_on_done=true en herdr_run_agent. Si además pide cerrar la escucha,
   pon close_listening=true. Es más fiable que una segunda llamada.
5. Si el agente ya está lanzado, usa voice_close_and_watch con su dirección.
6. Si el usuario dice "deja de escuchar", "cierra" o "adiós", usa voice_close.
7. Si el usuario dice "mute", "silencia el micro" o "no me escuches", usa
   voice_mic_mute(true). Para reactivarlo, voice_mic_mute(false).
8. Si el usuario da una orden explícita, ejecútala directamente, sin pedir
   confirmación ni repetir la orden. Responde en una sola frase corta diciendo
   qué se abre y dónde. Ejemplo: «abre una terminal» → llama a
   herdr_open_terminal y responde «La abro en Herdr». No digas «te confirmo»,
   «¿me confirmas?» ni «¿quieres que lo haga?».
9. No leas código ni logs largos en voz alta; resume en una o dos frases.
10. Si no hay agentes o la herramienta falla, dilo con claridad y sin inventar.
11. No hagas tú la tarea: delega en el agente adecuado.
12. Cuando el sistema te avise de que una tarea terminó, recibirás la salida
    reciente. No des el resumen hasta que el usuario lo pida. Si pide "solo la
    salida", léela; si pide "resume", resume; si pide otra cosa, actúa.
13. No cierres la escucha por tu cuenta salvo que el usuario lo pida."""

TOOLS: list[dict[str, Any]] = [
    {
        "type": "function",
        "function": {
            "name": "herdr_list_agents",
            "description": "Lista los agentes vivos de Herdr con su estado y carpeta.",
            "parameters": {"type": "object", "properties": {}},
        },
    },
    {
        "type": "function",
        "function": {
            "name": "herdr_get_agent",
            "description": "Metadatos de un terminal/agente por dirección (t1, t3...).",
            "parameters": {
                "type": "object",
                "properties": {"address": {"type": "string"}},
                "required": ["address"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "herdr_read_agent",
            "description": "Lee las últimas líneas del buffer de un agente.",
            "parameters": {
                "type": "object",
                "properties": {
                    "address": {"type": "string"},
                    "lines": {"type": "integer", "minimum": 10, "maximum": 120},
                },
                "required": ["address"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "herdr_send_agent",
            "description": "Envía texto a un agente. La orden explícita se ejecuta directamente, sin pedir confirmación.",
            "parameters": {
                "type": "object",
                "properties": {
                    "address": {"type": "string"},
                    "text": {"type": "string"},
                },
                "required": ["address", "text"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "herdr_run_agent",
            "description": "Abre una pestaña VISIBLE en Herdr y lanza un agente (claude, opencode, mcode, codex) con la tarea como primer mensaje. background=true solo si el usuario lo pide explícitamente; gui=true para tareas de interfaz gráfica (cua-driver); notify_on_done=true para avisar al terminar; close_listening=true para cerrar la escucha tras hablar. La orden explícita se ejecuta directamente.",
            "parameters": {
                "type": "object",
                "properties": {
                    "agent": {"type": "string"},
                    "task": {"type": "string"},
                    "cwd": {"type": "string"},
                    "name": {"type": "string"},
                    "background": {"type": "boolean"},
                    "gui": {"type": "boolean"},
                    "notify_on_done": {"type": "boolean"},
                    "close_listening": {"type": "boolean"},
                    "space": {"type": "string"},
                },
                "required": ["agent", "task"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "herdr_open_terminal",
            "description": "Abre una terminal visible en una pestaña nueva de Herdr (PowerShell por defecto).",
            "parameters": {
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "cwd": {"type": "string"},
                    "shell": {"type": "string"},
                    "space": {"type": "string"},
                },
                "required": ["name"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "herdr_focus_agent",
            "description": "Enfoca en Herdr el terminal/agente indicado para que el usuario lo vea.",
            "parameters": {
                "type": "object",
                "properties": {"address": {"type": "string"}},
                "required": ["address"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "herdr_kill_agent",
            "description": "Termina un agente. La orden explícita se ejecuta directamente, sin pedir confirmación.",
            "parameters": {
                "type": "object",
                "properties": {
                    "address": {"type": "string"},
                },
                "required": ["address"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "herdr_watch_agent",
            "description": "Registra un agente para avisar por voz cuando termine o se bloquee, sin cerrar la escucha.",
            "parameters": {
                "type": "object",
                "properties": {"address": {"type": "string"}},
                "required": ["address"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "voice_close_and_watch",
            "description": "Cierra la escucha al terminar de hablar y avisa cuando el agente termine. Al acabar, el sistema reabre la escucha y pregunta qué quiere el usuario.",
            "parameters": {
                "type": "object",
                "properties": {"address": {"type": "string"}},
                "required": ["address"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "voice_close",
            "description": "Cierra la escucha al terminar de hablar. Úsalo cuando el usuario diga 'deja de escuchar', 'cierra' o 'adiós'.",
            "parameters": {"type": "object", "properties": {}},
        },
    },
    {
        "type": "function",
        "function": {
            "name": "voice_mic_mute",
            "description": "Silencia o reactiva el micrófono. No afecta a los avisos proactivos.",
            "parameters": {
                "type": "object",
                "properties": {"muted": {"type": "boolean"}},
                "required": ["muted"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "voice_repeat_last",
            "description": "Devuelve el último resumen que se dijo por voz.",
            "parameters": {"type": "object", "properties": {}},
        },
    },
    {
        "type": "function",
        "function": {
            "name": "voice_mute",
            "description": "Silencia los avisos proactivos durante N minutos.",
            "parameters": {
                "type": "object",
                "properties": {"minutes": {"type": "integer", "minimum": 1, "maximum": 480}},
                "required": ["minutes"],
            },
        },
    },
]


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
    ):
        self.cfg = cfg
        self.herdr = herdr
        self.last_summary = last_summary
        self.watch = watch
        self.mute = mute
        self.request_close = request_close
        self.mic_mute = mic_mute
        self._configure_client()
        self.messages: list[dict[str, Any]] = [
            {"role": "system", "content": self._system_prompt()}
        ]
        self._last_user_text = ""
        self._last_agent_address: str | None = None

    def _configure_client(self) -> None:
        headers = {"User-Agent": "EzAgent-Voice/0.1"}
        if self.cfg.llm_provider == "opencode":
            headers["x-opencode-session"] = uuid.uuid4().hex
        if self.cfg.llm_provider == "anthropic":
            workspace = os.getenv("ANTHROPIC_WORKSPACE_ID")
            if workspace:
                headers["anthropic-workspace-id"] = workspace
        self.client = OpenAI(
            api_key=self.cfg.api_key or "sin-clave",
            base_url=self.cfg.llm_base_url or None,
            default_headers=headers,
        )

    def reload_llm(self) -> None:
        """Recrea el cliente tras cambiar proveedor, modelo, URL o clave."""
        self._configure_client()
        self.messages = [
            {"role": "system", "content": self._system_prompt()}
        ]
        self._last_user_text = ""
        self._last_agent_address = None

    def _system_prompt(self) -> str:
        if self.cfg.confirm_actions:
            rule = (
                "Confirmación de acciones: SÍ. Antes de lanzar, enviar o matar, "
                "pide confirmación explícita y espera un sí."
            )
        else:
            rule = (
                "Confirmación de acciones: NO. Si el usuario da una orden "
                "explícita, ejecútala directamente sin preguntar de nuevo. "
                "Responde en una sola frase corta con qué abres y dónde."
            )
        return (
            SYSTEM_PROMPT
            + "\n\n"
            + rule
            + "\nEsta regla tiene prioridad sobre la regla 8."
        )

    def set_confirm_actions(self, enabled: bool) -> None:
        self.cfg.confirm_actions = bool(enabled)
        if self.messages and self.messages[0].get("role") == "system":
            self.messages[0]["content"] = self._system_prompt()

    def _wants_notify(self) -> bool:
        text = self._last_user_text.lower()
        return any(
            word in text
            for word in (
                "avísame", "avisame", "avisa", "notifica",
                "cuando termine", "cuando acabe",
            )
        )

    def _wants_close(self) -> bool:
        text = self._last_user_text.lower()
        return any(
            word in text
            for word in (
                "ciérrate", "cierrate", "cierra la escucha", "cierra el micro",
                "deja de escuchar", "sin escuchar",
            )
        )

    def pending_notify_address(self) -> str | None:
        """Último agente lanzado, si el usuario pidió que se le avise."""
        if self._wants_notify() and self._last_agent_address:
            return self._last_agent_address
        return None

    # ------------------------------------------------------------- tool loop
    def _trim_history(self) -> None:
        if len(self.messages) <= 14:
            return
        history = self.messages[1:]
        while history and history[0].get("role") == "tool":
            history.pop(0)
        self.messages = [self.messages[0]] + history[-12:]

    def ask(self, text: str) -> str:
        answer = "".join(self.ask_stream(text)).strip()
        return answer or "No he podido completar la consulta."

    def ask_stream(self, text: str) -> Iterator[str]:
        """Streaming de la respuesta, incluidas las llamadas a herramientas."""
        self._last_user_text = text
        self.messages.append({"role": "user", "content": text})
        self._trim_history()
        for _ in range(6):
            request = {
                "model": self.cfg.llm_model,
                "messages": self.messages,
                "temperature": (
                    1.0 if self.cfg.llm_provider == "anthropic"
                    else self.cfg.llm_temperature
                ),
                "max_tokens": self.cfg.llm_max_tokens,
                "tools": TOOLS,
                "tool_choice": "auto",
                "stream": True,
            }
            effort = str(
                getattr(self.cfg, "llm_reasoning_effort", "none") or "none"
            )
            if self.cfg.llm_provider in {"opencode", "openai"}:
                # Se conserva el valor explícito (incluido «none») que ya
                # usaba OpenCode/OpenAI; el usuario puede subirlo.
                request["extra_body"] = {"reasoning_effort": effort}
            elif (
                effort != "none"
                and self.cfg.llm_provider
                not in {"anthropic", "gemini"}
            ):
                # Groq, OpenRouter y endpoints compatibles suelen aceptar el
                # parámetro; Anthropic/Gemini se dejan sin él.
                request["extra_body"] = {"reasoning_effort": effort}
            stream = self.client.chat.completions.create(**request)
            tool_calls: dict[int, dict[str, Any]] = {}
            content_parts: list[str] = []
            try:
                for chunk in stream:
                    if not chunk.choices:
                        continue
                    delta = chunk.choices[0].delta
                    if delta.content:
                        content_parts.append(delta.content)
                        yield delta.content
                    for call in delta.tool_calls or []:
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

            if tool_calls:
                ordered = [tool_calls[i] for i in sorted(tool_calls)]
                self.messages.append(
                    {
                        "role": "assistant",
                        "content": "".join(content_parts),
                        "tool_calls": ordered,
                    }
                )
                for call in ordered:
                    result = self._execute(
                        call["function"]["name"],
                        call["function"]["arguments"],
                    )
                    self.messages.append(
                        {
                            "role": "tool",
                            "tool_call_id": call["id"],
                            "content": json.dumps(result, ensure_ascii=False),
                        }
                    )
                continue

            content = "".join(content_parts).strip()
            self.messages.append({"role": "assistant", "content": content})
            return
        yield "Me he quedado dando vueltas con Herdr; prueba a preguntar de otra forma."

    def _execute(self, name: str, arguments: str) -> Any:
        print(f"[ezagent-voice] Tool {name}: {arguments[:300]}", flush=True)
        try:
            args = json.loads(arguments or "{}")
        except json.JSONDecodeError:
            return {"ok": False, "error": "Argumentos JSON inválidos."}
        try:
            if name == "herdr_list_agents":
                return {"ok": True, "agents": self.herdr.list_agents()}
            if name == "herdr_get_agent":
                return {"ok": True, "agent": self.herdr.get_agent(args["address"])}
            if name == "herdr_read_agent":
                return {
                    "ok": True,
                    "address": args["address"],
                    "text": self.herdr.read_agent(
                        args["address"], args.get("lines")
                    ),
                }
            if name == "herdr_send_agent":
                return self.herdr.send_agent(
                    args["address"], args["text"], bool(args.get("confirmed"))
                )
            if name == "herdr_run_agent":
                result = self.herdr.run_agent(
                    args["agent"],
                    args["task"],
                    args.get("cwd"),
                    args.get("name"),
                    bool(args.get("confirmed")),
                    background=bool(args.get("background")),
                    gui=bool(args.get("gui")),
                    space=args.get("space"),
                )
                if result.get("ok"):
                    self._last_agent_address = result.get("address")
                    notify = bool(args.get("notify_on_done")) or self._wants_notify()
                    close = bool(args.get("close_listening")) or self._wants_close()
                    if notify:
                        self.watch(result["address"])
                    if close:
                        self.request_close()
                return result
            if name == "herdr_open_terminal":
                return self.herdr.open_terminal(
                    args["name"],
                    args.get("cwd"),
                    args.get("shell", "powershell.exe"),
                    args.get("space"),
                )
            if name == "herdr_focus_agent":
                return self.herdr.focus_agent(args["address"])
            if name == "herdr_kill_agent":
                return self.herdr.kill_agent(
                    args["address"], bool(args.get("confirmed"))
                )
            if name == "herdr_watch_agent":
                self._last_agent_address = args["address"]
                self.watch(args["address"])
                return {"ok": True, "message": "Avisaré cuando termine o se bloquee."}
            if name == "voice_close_and_watch":
                self._last_agent_address = args["address"]
                self.watch(args["address"])
                self.request_close()
                return {
                    "ok": True,
                    "message": "Cerraré la escucha y te avisaré cuando termine.",
                }
            if name == "voice_close":
                if self._wants_notify() and self._last_agent_address:
                    self.watch(self._last_agent_address)
                self.request_close()
                return {"ok": True, "message": "Cerraré la escucha al terminar de hablar."}
            if name == "voice_mic_mute":
                muted = bool(args.get("muted"))
                self.mic_mute(muted)
                return {
                    "ok": True,
                    "muted": muted,
                    "message": "Micrófono silenciado." if muted else "Micrófono activo.",
                }
            if name == "voice_repeat_last":
                return {"ok": True, "text": self.last_summary()}
            if name == "voice_mute":
                minutes = int(args.get("minutes", 10))
                self.mute(minutes)
                return {"ok": True, "message": f"Silenciado {minutes} minutos."}
        except HerdrError as exc:
            return {"ok": False, "error": str(exc)}
        except Exception as exc:  # noqa: BLE001
            return {"ok": False, "error": f"{type(exc).__name__}: {exc}"}
        return {"ok": False, "error": f"Herramienta desconocida: {name}"}

    # --------------------------------------------------------------- avisos
    def note_done(
        self, address: str, agent: str, reason: str, output: str
    ) -> None:
        """Añade al contexto que un agente terminó, sin resumir todavía."""
        self.messages.append(
            {
                "role": "system",
                "content": (
                    f"El agente {agent} ({address}) ha terminado ({reason}). "
                    "El usuario puede pedir la salida literal, un resumen o una "
                    "nueva instrucción. No des el resumen hasta que lo pida.\n"
                    f"Salida reciente:\n{output}"
                ),
            }
        )

