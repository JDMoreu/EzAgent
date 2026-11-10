"""Sesión de un agente ya abierto: esperar, fijar modelo/esfuerzo y entregar la tarea.

Los agentes (OpenCode, Claude Code, Codex, mcode, pi...) se manejan como lo
haría una persona: se abre una terminal, se escribe el comando, se espera a que
el programa arranque, se cambian modelo y esfuerzo con sus comandos internos
(``/models``, ``/effort``...) y por último se pega la tarea.

Todo lo que se escribe sale de los perfiles de ``voice.json`` o del código;
el LLM solo aporta valores (modelo, esfuerzo, tarea) que se validan antes.
"""

from __future__ import annotations

import re
import time
from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Any

_PLAIN_ARG = re.compile(r"^[A-Za-z0-9_./:=@%+,\\-]+$")
_PS_PROMPT = re.compile(r"^PS [^>]*>")
# Diálogos que piden una decisión del usuario: EzAgent NO pulsa Enter sobre
# ellos (en «No, exit» o «Yes, I trust» cambiaría algo sin que lo decida).
_BLOCKING_DIALOG = re.compile(
    r"(?i)(do you trust|trust this (folder|directory|project)|yes, i trust"
    r"|quick safety check)"
)
_PROMPT_GLYPH = re.compile(r"^[›❯>]\s?")
_SHELLS = {"powershell", "pwsh", "cmd", "bash", "sh", "zsh", "nu", "fish"}
_KEY = re.compile(
    r"^(?:(?:ctrl|alt|shift)\+)*"
    r"(?:[a-z0-9]|enter|esc|tab|up|down|left|right|home|end|pageup|pagedown"
    r"|backspace|space|f(?:[1-9]|1[0-2]))$"
)
MAX_KEYS = 12

DEFAULT_SHELL = "powershell.exe"
READY_TIMEOUT_S = 30.0
POLL_S = 0.25
STEP_PAUSE_S = 1.0
CHECK_RETRY_S = 0.7
SETTLE_S = 0.5
SHELL_PROMPT_TIMEOUT_S = 4.0
CHECK_TRIES = 4
SCREEN_LINES = 40
CONFIRM_WINDOW_S = 4.0
CONFIRM_POLL_S = 0.5


@dataclass
class AgentLaunch:
    """Cómo abrir un agente y qué falta por fijar dentro de su sesión."""

    argv: list[str] | None = None
    warnings: list[str] = field(default_factory=list)
    # Valores que el CLI no acepta como flag y se fijan con comandos de sesión.
    pending: dict[str, str] = field(default_factory=dict)
    # Variables de entorno que se fijan en la shell antes de lanzar el comando.
    env: dict[str, str] = field(default_factory=dict)
    error: str = ""


def _ps_quote(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def command_line(argv: list[str], env: dict[str, str] | None = None) -> str:
    """Línea que se escribe en PowerShell; cita lo que no sea un argumento simple.

    ``env`` se antepone como ``$env:NOMBRE='valor'; `` (solo vive en esa shell).
    """
    prefix = "".join(f"$env:{name}={_ps_quote(value)}; " for name, value in (env or {}).items())
    parts = [p if _PLAIN_ARG.match(p) else _ps_quote(p) for p in argv]
    return prefix + " ".join(parts)


def valid_keys(keys: Any) -> list[str] | None:
    """Teclas nombradas (enter, esc, ctrl+t...) o None si alguna no es válida."""
    if not isinstance(keys, list) or not 0 < len(keys) <= MAX_KEYS:
        return None
    cleaned = [str(k).strip().lower() for k in keys]
    return cleaned if all(_KEY.match(k) for k in cleaned) else None


def fill(text: str, values: dict[str, str], escape: bool = False) -> str:
    """Sustituye {model}, {model_search} y {effort}; ``escape`` para regex."""
    model = values.get("model", "")
    search = re.sub(r"[-_]+", " ", model.split("/")[-1])
    quote = re.escape if escape else (lambda x: x)
    return (
        str(text)
        .replace("{model_search}", quote(search))
        .replace("{model}", quote(model))
        .replace("{effort}", quote(values.get("effort", "")))
    )


def _is_shell(name: str) -> bool:
    plain = re.sub(r"\.(exe|cmd|bat)$", "", str(name or "").strip().lower())
    return not plain or plain in _SHELLS


class AgentSession:
    """Mixin: requiere ``cfg``, ``get_agent``, ``send_agent`` y ``keys_agent``."""

    cfg: Any
    get_agent: Callable[[str], dict[str, Any]]
    send_agent: Callable[..., dict[str, Any]]
    keys_agent: Callable[..., dict[str, Any]]
    read_agent: Callable[..., str]
    _sleep: Callable[[float], None] = staticmethod(time.sleep)

    def _session_steps(self, key: str, kind: str) -> list[dict[str, Any]]:
        """Pasos de sesión de un perfil, normalizados a diccionarios.

        Un paso es un texto (se escribe y Enter) o un dict con ``send``,
        ``keys`` o ``cycle`` y, opcionalmente, ``expect``/``fail`` (regex que se
        buscan en la pantalla), ``message``, ``optional`` y ``soft``.
        """
        commands = self._profile(key).get("session_commands")
        raw = commands.get(kind) if isinstance(commands, dict) else None
        items = raw if isinstance(raw, list) else ([raw] if raw else [])
        steps: list[dict[str, Any]] = []
        for item in items:
            if isinstance(item, str) and item.strip():
                steps.append({"send": item})
            elif isinstance(item, dict) and any(item.get(k) for k in ("send", "keys", "cycle")):
                steps.append(item)
        return steps

    def wait_shell_prompt(self, address: str) -> bool:
        """Espera (sin pausa fija) a que la shell recién abierta dibuje su prompt."""
        deadline = time.monotonic() + SHELL_PROMPT_TIMEOUT_S
        while time.monotonic() < deadline:
            lines = [ln.strip() for ln in self._screen(address).splitlines() if ln.strip()]
            if lines and _PS_PROMPT.match(lines[-1]):
                return True
            self._sleep(0.15)
        return False

    def _screen_shows_program(self, address: str) -> bool:
        lines = [ln.strip() for ln in self._screen(address).splitlines() if ln.strip()]
        return len(lines) >= 3 and not _PS_PROMPT.match(lines[-1])

    def wait_ready(self, address: str, timeout: float = READY_TIMEOUT_S) -> bool:
        """Espera a que en la terminal corra un agente (no la shell) y repose.

        El plazo es monotónico e incluye lo que tarden `get_agent` y la lectura
        de pantalla, no solo las pausas.
        """
        deadline = time.monotonic() + timeout
        while time.monotonic() <= deadline:
            try:
                info = self.get_agent(address)
            except Exception:  # noqa: BLE001 - la terminal puede no estar lista
                info = {}
            name = str(info.get("agent") or info.get("app") or "")
            state = str(info.get("state") or "").strip().lower()
            if not _is_shell(name) and state != "working":
                self._sleep(SETTLE_S)  # deja que el TUI termine de dibujarse
                return True
            if _is_shell(name) and self._screen_shows_program(address):
                # Herdr no reconoce todos los agentes (pi se ve como
                # powershell.exe): si la pantalla ya no acaba en el prompt de
                # la shell, el programa arrancó.
                self._sleep(SETTLE_S)
                return True
            self._sleep(POLL_S)
        return False

    def _try_screen(self, address: str) -> str | None:
        """Pantalla visible, o None si la lectura falló.

        None (fallo de lectura) es distinto de "" (pantalla vacía): quien
        confirma una entrega no puede tratar un fallo como «no hay nada
        pendiente».
        """
        try:
            text = self.read_agent(address, SCREEN_LINES)
        except Exception:  # noqa: BLE001 - leer es best-effort
            return None
        return "\n".join(str(text).splitlines()[-SCREEN_LINES:])

    def _screen(self, address: str) -> str:
        return self._try_screen(address) or ""

    def _check_screen(self, address: str, step: dict[str, Any], values: dict[str, str]) -> str:
        """Comprueba ``expect``/``fail`` en pantalla; devuelve el fallo o ''."""
        expect, fail = step.get("expect"), step.get("fail")
        if not expect and not fail:
            return ""
        message = fill(
            str(step.get("message") or "la pantalla no mostró lo esperado"),
            values,
        )
        for _ in range(CHECK_TRIES if expect else 1):
            self._sleep(CHECK_RETRY_S)
            screen = self._screen(address)
            if fail and re.search(fill(str(fail), values, True), screen, re.I):
                return message
            if not expect or re.search(fill(str(expect), values, True), screen, re.I):
                return ""
        return message

    def _cycle(self, address: str, step: dict[str, Any], values: dict[str, str]) -> tuple[str, str]:
        """Pulsa una tecla hasta que la pantalla muestre ``until``."""
        spec = step["cycle"] if isinstance(step.get("cycle"), dict) else {}
        keys = valid_keys(spec.get("keys"))
        until = fill(str(spec.get("until") or ""), values, True)
        message = fill(str(step.get("message") or "no encontré el valor"), values)
        if not keys or not until:
            return "", "paso de ciclo mal definido"
        try:
            tries = max(1, min(int(spec.get("max", 8)), 20))
        except (TypeError, ValueError):
            tries = 8
        for _ in range(tries):
            if not self.keys_agent(address, keys).get("ok"):
                return "", message
            self._sleep(CHECK_RETRY_S)
            if re.search(until, self._screen(address), re.I):
                return "+".join(keys), ""
        return "", message

    def _run_step(
        self, address: str, step: dict[str, Any], values: dict[str, str]
    ) -> tuple[str, str]:
        """Ejecuta un paso; devuelve (lo aplicado, motivo del fallo o '')."""
        if step.get("cycle"):
            return self._cycle(address, step, values)
        if step.get("keys"):
            keys = valid_keys(step["keys"])
            if keys is None:
                return "", "teclas no válidas en el perfil"
            label = "+".join(keys)
            sent = self.keys_agent(address, keys)
        else:
            label = fill(str(step["send"]), values)
            sent = self.send_agent(address, label, confirmed=True)
        if not sent.get("ok"):
            return "", f"no pude escribir «{label}»"
        if step.get("expect") or step.get("fail"):
            # Hay una condición observable: `_check_screen` espera y reintenta.
            return label, self._check_screen(address, step, values)
        self._sleep(STEP_PAUSE_S)  # respaldo: nada que observar en pantalla
        return label, ""

    def _apply_in_session(
        self, address: str, key: str, values: dict[str, str]
    ) -> tuple[list[str], list[str], str]:
        """Fija modelo/esfuerzo dentro de la sesión verificando la pantalla.

        Devuelve (aplicado, avisos, fallo grave). Un paso ``soft`` solo avisa;
        uno ``optional`` avisa y salta el resto de ese ajuste; cualquier otro
        fallo detiene todo (y se pulsa Esc para no dejar un selector abierto).
        """
        applied: list[str] = []
        warnings: list[str] = []
        for kind, label in (("model", "modelo"), ("effort", "esfuerzo")):
            if not values.get(kind):
                continue
            steps = self._session_steps(key, kind)
            if not steps:
                warnings.append(
                    f"{key} no permite cambiar el {label} dentro de la sesión; "
                    "hay que lanzarlo de nuevo con run."
                )
                continue
            for step in steps:
                done, reason = self._run_step(address, step, values)
                if done:
                    applied.append(done)
                if not reason:
                    continue
                if step.get("soft"):
                    warnings.append(f"No pude confirmar el {label}: {reason}.")
                    continue
                self.keys_agent(address, ["esc"])
                if step.get("optional"):
                    warnings.append(f"No se fijó el {label}: {reason}.")
                    break
                return applied, warnings, f"{label}: {reason}"
        return applied, warnings, ""

    def blocking_dialog(self, address: str) -> bool:
        """True si la pantalla pide una confirmación (p. ej. confiar en la carpeta)."""
        return bool(_BLOCKING_DIALOG.search(self._screen(address)))

    @staticmethod
    def _unsent_in_input(screen: str, task: str) -> bool:
        """¿Quedó la tarea escrita en la caja de entrada sin enviarse?"""
        lines = [ln.strip() for ln in screen.splitlines() if ln.strip()]
        if any("[Pasted" in ln for ln in lines[-8:]):
            return True  # OpenCode muestra el pegado como «[Pasted ~N lines]»
        tail = " ".join(str(task or "").split())[-30:]
        if len(tail) < 8:
            return False
        for index in range(len(lines) - 1, -1, -1):
            if _PROMPT_GLYPH.match(lines[index]):
                region = " ".join(" ".join(lines[index:]).split())
                return tail in region
        return False

    def confirm_submitted(self, address: str, task: str) -> bool:
        """Comprueba que el agente tomó la tarea; si quedó sin enviar, Enter una vez.

        Con la máquina cargada el Enter del pegado puede perderse y la tarea se
        queda escrita en la caja. Se mira el estado y la caja de entrada. Si la
        pantalla no se pudo leer no hay evidencia: nunca se confirma.
        """
        deadline = time.monotonic() + CONFIRM_WINDOW_S
        presses = 0
        while True:
            try:
                state = str(self.get_agent(address).get("state") or "").lower()
            except Exception:  # noqa: BLE001
                state = ""
            if state == "working":
                return True
            screen = self._try_screen(address)
            if screen is None or not screen.strip():
                # DECISION: sin pantalla legible no hay evidencia de entrega;
                # pulsar Enter a ciegas podría enviar otra cosa. Nunca True.
                return False
            if not self._unsent_in_input(screen, task):
                return True
            if time.monotonic() < deadline:
                self._sleep(CONFIRM_POLL_S)
                continue
            if presses:
                return False
            presses += 1
            self.keys_agent(address, ["enter"])
            deadline = time.monotonic() + CONFIRM_WINDOW_S

    def send_task(self, address: str, task: str) -> dict[str, Any]:
        """Pega la tarea y confirma que el agente la recibió."""
        sent = self.send_agent(address, task, confirmed=True)
        if sent.get("ok") and not self.confirm_submitted(address, task):
            sent = {
                **sent,
                "warning": "No pude confirmar que el agente recibió la tarea.",
            }
        return sent

    def finish_launch(
        self, address: str, key: str, launch: AgentLaunch, task: str
    ) -> dict[str, Any]:
        """Espera al agente, fija modelo/esfuerzo pendientes y pega la tarea."""
        if not self.wait_ready(address):
            return {
                "ok": False,
                "error": (
                    f"Abrí la terminal {address}, pero no vi que {key} "
                    "arrancara; no le envié nada. Revísala con action=read."
                ),
            }
        if self.blocking_dialog(address):
            return {
                "ok": False,
                "error": (
                    f"{key} está pidiendo confirmar algo en la terminal "
                    f"{address} (por ejemplo, confiar en la carpeta). No pulso "
                    "Enter por ti: confírmalo tú y vuelve a pedirme la tarea."
                ),
            }
        applied, warnings, fatal = self._apply_in_session(address, key, launch.pending)
        if fatal:
            return {
                "ok": False,
                "applied": applied,
                "error": (
                    f"No pude fijar el {fatal}. Dejé {key} abierto en "
                    f"{address} sin enviar la tarea; revísalo con action=read."
                ),
            }
        task_sent = False
        if task:
            sent = self.send_task(address, task)
            task_sent = bool(sent.get("ok"))
            if not task_sent:
                warnings.append(f"No pude enviar la tarea: {sent.get('error')}")
            elif sent.get("warning"):
                warnings.append(str(sent["warning"]))
        return {
            "ok": True,
            "applied": applied,
            "warnings": launch.warnings + warnings,
            "task_sent": task_sent,
        }

    def configure_agent(
        self,
        address: str,
        model: str | None = None,
        effort: str | None = None,
    ) -> dict[str, Any]:
        """Cambia modelo/esfuerzo de un agente ya abierto con sus comandos."""
        values = {"model": (model or "").strip(), "effort": (effort or "").strip()}
        if not any(values.values()):
            return {"ok": False, "error": "Indica el modelo o el esfuerzo."}
        info = self.get_agent(address)
        raw = str(info.get("agent") or info.get("app") or "")
        key = self.resolve_agent(raw)
        if key is None:
            return {
                "ok": False,
                "error": (
                    f"No sé qué agente es {address} ({raw or 'sin nombre'}); "
                    "no puedo cambiarle el modelo."
                ),
            }
        for kind, label in (("model", "modelo"), ("effort", "esfuerzo")):
            if values[kind]:
                problem = self._check_value(key, label, values[kind])
                if problem:
                    return {"ok": False, "error": problem}
        applied, warnings, fatal = self._apply_in_session(address, key, values)
        if fatal:
            return {"ok": False, "error": f"No pude fijar el {fatal}."}
        if not applied:
            return {"ok": False, "error": " ".join(warnings) or "Nada que aplicar."}
        return {
            "ok": True,
            "address": address,
            "agent": key,
            "applied": applied,
            "warnings": warnings,
        }
