"""Adaptador entre `ezagent-voice` y Herdr (`hc` + `herdr-app`)."""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import threading
import time
import uuid
from collections.abc import Callable
from pathlib import Path
from typing import Any

from loguru import logger

from .agent_driver import (
    DEFAULT_SHELL,
    AgentDriver,
    command_line,
    valid_keys,
)
from .config import VoiceConfig

_ANSI = re.compile(r"\x1b\[[0-9;?]*[ -/]*[@-~]")
_OSC = re.compile(r"\x1b\].*?(?:\x07|\x1b\\)", re.DOTALL)

GUI_INSTRUCTION = (
    "Trabaja de forma visible en el escritorio. Usa la skill cua-driver para "
    "controlar la interfaz gráfica: abre las aplicaciones, navega, haz clic y "
    "escribe como lo haría una persona. Deja las ventanas a la vista para que "
    "el usuario vea todo el proceso. "
)


# `hc send` recibe el texto como argumento y Windows limita la línea de
# comandos a ~32 000 caracteres (un plan detallado mide más). Por encima de
# este umbral el mensaje se entrega como archivo (`hc put`).
LONG_TEXT_CHARS = 12_000


class HerdrError(RuntimeError):
    pass


# Carpeta de instalación por defecto y script oficial de cada pieza. Solo se
# ejecuta cuando el binario no aparece; si ya está, no se toca.
_HERDR_PARTS: tuple[tuple[str, str, str], ...] = (
    ("hc_path", "herdr-core", "hc.exe"),
    ("herdr_app_path", "herdr-app", "herdr-app.exe"),
)
_HERDR_INSTALLERS = {
    "herdr-core": "irm https://releases.herdr.dev/core/install.ps1 | iex",
    "herdr-app": "irm https://releases.herdr.dev/app/install.ps1 | iex",
}


def _programs_dir() -> Path:
    base = os.environ.get("LOCALAPPDATA") or str(Path.home() / "AppData" / "Local")
    return Path(base) / "Programs"


def strip_ansi(text: str) -> str:
    text = _OSC.sub("", text)
    text = _ANSI.sub("", text)
    text = text.replace("\r\n", "\n").replace("\r", "\n")
    # La UI de los agentes dibuja cajas y líneas; no aportan nada al resumen.
    text = re.sub(r"[─━│┃┌┐└┘├┤┬┴┼╭╮╰╯╔╗╚╝║═]+", " ", text)
    text = re.sub(r"[ \t]+", " ", text)
    lines = [line.strip() for line in text.splitlines()]
    text = "\n".join(line for line in lines if line)
    return text.strip()


class Herdr(AgentDriver):
    def __init__(self, cfg: VoiceConfig):
        self.cfg = cfg
        self._install_lock = threading.Lock()
        self._resolve_paths()

    # ------------------------------------------------------------- instalación
    def _resolve_paths(self) -> None:
        """Usa el binario del PATH o el que deja el instalador de Herdr."""
        for attr, folder, exe in _HERDR_PARTS:
            current = str(getattr(self.cfg, attr, "") or "")
            found = shutil.which(current)
            if found:
                setattr(self.cfg, attr, found)
                continue
            if current and Path(current).is_absolute():
                continue  # ruta elegida por el usuario: no se sustituye
            candidate = _programs_dir() / folder / exe
            if candidate.is_file():
                setattr(self.cfg, attr, str(candidate))

    def ensure_installed(self) -> bool:
        """Instala Herdr y su motor (hc) si faltan. True si ambos están listos.

        Usa el instalador oficial y fijo de herdr.dev, sin texto del usuario
        ni del LLM. Si el binario ya existe no descarga nada.
        """
        with self._install_lock:
            self._resolve_paths()
            for attr, folder, _exe in _HERDR_PARTS:
                if shutil.which(str(getattr(self.cfg, attr, ""))):
                    continue
                logger.info("Herdr: falta {}; instalándolo…", folder)
                if not self._run_installer(_HERDR_INSTALLERS[folder]):
                    logger.warning("Herdr: no pude instalar {}.", folder)
                self._resolve_paths()
            return all(
                shutil.which(str(getattr(self.cfg, attr, "")))
                for attr, _folder, _exe in _HERDR_PARTS
            )

    @staticmethod
    def _run_installer(script: str) -> bool:
        # DECISION: `iex` sobre un script de URL fija es el instalador oficial
        # de Herdr; PowerShell recibe la orden como argumento (sin shell=True)
        # y nunca incluye texto del usuario ni del modelo.
        command = [
            "powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass",
            "-Command", f"$ProgressPreference = 'SilentlyContinue'; {script}",
        ]
        try:
            result = subprocess.run(
                command,
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=900,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            logger.warning("Herdr: instalador: {}", exc)
            return False
        if result.returncode != 0:
            logger.warning(
                "Herdr: el instalador devolvió {}: {}",
                result.returncode,
                (result.stderr or "")[-400:],
            )
            return False
        return True

    # ------------------------------------------------------------------ base
    def _exec(
        self,
        command: list[str],
        timeout: float,
        env: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess:
        try:
            return subprocess.run(
                command,
                env=env,
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=timeout,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
        except subprocess.TimeoutExpired as exc:
            raise HerdrError(f"`{command[0]}` tardó demasiado.") from exc

    def _spawn(
        self,
        command: list[str],
        timeout: float,
        env: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess:
        try:
            return self._exec(command, timeout, env)
        except FileNotFoundError as exc:
            replacement = self._replacement_for(command[0])
            if replacement:
                try:
                    return self._exec([replacement, *command[1:]], timeout, env)
                except FileNotFoundError:
                    pass
            raise HerdrError(
                f"No encuentro `{command[0]}`. Revisa voice.json."
            ) from exc

    def _replacement_for(self, executable: str) -> str | None:
        name = Path(str(executable)).name.lower()
        if name in ("hc", "hc.exe"):
            attr = "hc_path"
        elif name in ("herdr-app", "herdr-app.exe"):
            attr = "herdr_app_path"
        else:
            return None
        self.ensure_installed()
        resolved = shutil.which(str(getattr(self.cfg, attr, "") or ""))
        return resolved or None

    def _run_command(
        self,
        command: list[str],
        timeout: float,
        env: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess:
        return self._spawn(command, timeout, env)

    @staticmethod
    def _anonymous_env() -> dict[str, str]:
        """Entorno sin la identidad de la terminal que ejecuta EzAgent."""
        return {
            key: value
            for key, value in os.environ.items()
            if key.upper() != "HC_TERMINAL_ID"
        }

    def _run(self, args: list[str], timeout: float = 20.0) -> subprocess.CompletedProcess:
        return self._run_command([self.cfg.hc_path, *args], timeout)


    def _json_command(self, command: list[str], timeout: float) -> Any:
        result = self._run_command(command, timeout)
        output = (result.stdout or "").strip()
        if not output:
            raise HerdrError(
                (result.stderr or f"{command[0]} no devolvió nada").strip()
            )
        try:
            data = json.loads(output)
        except json.JSONDecodeError as exc:
            raise HerdrError(
                f"{command[0]} devolvió algo que no es JSON: {output[:300]}"
            ) from exc
        if isinstance(data, dict) and data.get("error"):
            error = data["error"]
            message = error.get("message") if isinstance(error, dict) else str(error)
            raise HerdrError(str(message))
        return data

    def _json(self, args: list[str], timeout: float = 20.0) -> Any:
        return self._json_command([self.cfg.hc_path, *args], timeout)

    def _json_app(self, args: list[str], timeout: float = 20.0) -> Any:
        return self._json_command([self.cfg.herdr_app_path, *args], timeout)

    # --------------------------------------------------------------- lectura
    def list_agents(self) -> list[dict[str, Any]]:
        data = self._json(["ls", "--json"])
        agents: list[dict[str, Any]] = []
        for terminal in data.get("terminals", []):
            status = terminal.get("status")
            if isinstance(status, dict):
                state = status.get("state")
                app = status.get("app") or terminal.get("agent")
            elif isinstance(status, str):
                state = status
                app = terminal.get("agent")
            else:
                continue
            agents.append(
                {
                    "address": terminal.get("address"),
                    "agent": app,
                    "app": app,
                    "state": state,
                    "cwd": terminal.get("cwd"),
                    "where": terminal.get("where"),
                    "name": terminal.get("name"),
                    "turn": terminal.get("turn"),
                }
            )
        return agents

    def get_agent(self, address: str) -> dict[str, Any]:
        data = self._json(["get", address])
        status = data.get("status")
        if isinstance(status, dict):
            state = status.get("state")
            app = status.get("app") or data.get("agent")
        elif isinstance(status, str):
            state = status
            app = data.get("agent")
        else:
            state = None
            app = data.get("agent")
        return {
            "address": data.get("address"),
            "agent": app,
            "app": app,
            "state": state,
            "title": data.get("title"),
            "cwd": data.get("cwd"),
            "where": (data.get("labels") or {}).get("where"),
            "turn": data.get("turn"),
            "labels": data.get("labels") or {},
        }

    def read_agent(
        self, address: str, lines: int | None = None, limit: int = 6000
    ) -> str:
        lines = lines or self.cfg.max_read_lines
        result = self._run(["read", address, "--lines", str(lines)], timeout=20)
        text = strip_ansi(result.stdout or "")
        if not text:
            text = strip_ansi(result.stderr or "")
        return text[-limit:]

    def read_full(self, address: str, lines: int = 1500) -> str:
        """Pantalla + historial largo, para recortar la respuesta de un agente.

        ``read_agent`` corta a 6000 caracteres (suficiente para resumir); un
        plan detallado que pasa de un agente a otro necesita entero.
        """
        return self.read_agent(address, lines, limit=120_000)

    # --------------------------------------------------------------- acciones
    def send_agent(
        self, address: str, text: str, confirmed: bool = False
    ) -> dict[str, Any]:
        if len(text) > LONG_TEXT_CHARS:
            staged = self._stage_long_text(address, text)
            if staged is None:
                return {
                    "ok": False,
                    "error": "El mensaje es demasiado largo y no pude guardarlo "
                    "en un archivo para entregarlo.",
                }
            text = staged
        # DECISION: `hc send` marca el mensaje con <hc from="tN"> cuando se
        # invoca desde una terminal de Herdr (lo sabe por HC_TERMINAL_ID). Al
        # agente le llegaría ese texto literal; sin la variable llega limpio.
        result = self._run_command(
            [self.cfg.hc_path, "send", address, text, "--json"],
            timeout=40,
            env=self._anonymous_env(),
        )
        output = (result.stdout or "").strip()
        if result.returncode != 0:
            return {"ok": False, "error": (result.stderr or output)[:800]}
        return {"ok": True, "address": address, "message": "Mensaje enviado."}

    @staticmethod
    def _handoff_dir() -> Path:
        base = os.environ.get("LOCALAPPDATA") or str(Path.home())
        return Path(base) / "EzAgent" / "handoff"

    def save_context(self, text: str) -> str | None:
        """Guarda un contexto largo como .md y devuelve su ruta (o None).

        Es la carpeta propia de EzAgent; los archivos de más de 7 días se
        retiran de ella al guardar uno nuevo.
        """
        folder = self._handoff_dir()
        path = folder / f"contexto-{uuid.uuid4().hex[:8]}.md"
        try:
            folder.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
        except OSError:
            return None
        cutoff = time.time() - 7 * 86400
        for old in folder.glob("*.md"):
            try:
                if (
                    old.name.startswith(("contexto-", "mensaje-"))
                    and old.stat().st_mtime < cutoff
                ):
                    old.unlink()
            except OSError:
                continue
        return str(path)

    def _stage_long_text(self, address: str, text: str) -> str | None:
        """Guarda un mensaje largo, lo envía con `hc put` y devuelve la frase
        corta que se pega al agente (con la ruta donde quedó el archivo)."""
        folder = self._handoff_dir()
        path = folder / f"mensaje-{uuid.uuid4().hex[:8]}.md"
        try:
            folder.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
            result = self._run_command(
                [self.cfg.hc_path, "put", address, str(path), "--json"],
                timeout=40,
                env=self._anonymous_env(),
            )
        except (OSError, HerdrError):
            return None
        if result.returncode != 0:
            return None
        landed = str(path)
        try:
            landed = str(json.loads(result.stdout or "{}").get("path") or path)
        except ValueError:
            pass
        return (
            "El mensaje completo es demasiado largo para pegarlo aquí. "
            f"Léelo entero en este archivo y haz lo que indica: {landed}"
        )

    def keys_agent(self, address: str, keys: list[str]) -> dict[str, Any]:
        """Pulsa teclas nombradas (enter, esc, up, ctrl+t...) en un terminal."""
        cleaned = valid_keys(keys)
        if cleaned is None:
            return {"ok": False, "error": "Teclas no válidas."}
        result = self._run(["keys", address, *cleaned], timeout=15)
        if result.returncode != 0:
            message = (result.stderr or result.stdout or "").strip()
            return {"ok": False, "error": message[:300]}
        return {"ok": True, "address": address, "keys": cleaned}

    def open_terminal(
        self,
        name: str,
        cwd: str | None = None,
        shell: str = "powershell.exe",
        space: str | None = None,
    ) -> dict[str, Any]:
        """Abre una terminal visible en una pestaña nueva de Herdr."""
        folder = cwd or self.cfg.default_cwd
        args = [
            "do", "tab.new",
            "--name", name,
            "--folder", folder,
            "--argv", json.dumps([shell]),
        ]
        if space:
            args += ["--space", space]
        data = self._json_app(args, timeout=30)
        terminal = data.get("terminal") or data.get("address")
        if not terminal:
            return {"ok": False, "error": f"tab.new no devolvió terminal: {data}"}
        return {
            "ok": True,
            "terminal": terminal,
            "address": terminal,
            "tab": data.get("tab"),
            "space": data.get("space"),
            "visible": True,
        }

    def run_agent(
        self,
        agent: str,
        task: str = "",
        cwd: str | None = None,
        name: str | None = None,
        confirmed: bool = False,
        background: bool = False,
        gui: bool = False,
        space: str | None = None,
        model: str | None = None,
        effort: str | None = None,
        on_ready: Callable[[dict[str, Any]], None] | None = None,
    ) -> dict[str, Any]:
        """Abre un agente como lo haría una persona y le pega la tarea.

        Visible (por defecto): pestaña nueva con una shell donde se escribe el
        comando del agente. Cuando el programa ha arrancado se fijan modelo y
        esfuerzo con sus comandos internos y, al final, se pega la tarea.
        """
        launch = self.agent_launch(agent, model, effort)
        key = self.resolve_agent(agent)
        if launch.argv is None or key is None:
            return {
                "ok": False,
                "error": launch.error or f"Agente desconocido: {agent!r}.",
                "available": sorted(self.cfg.agents),
            }
        argv = launch.argv
        folder = cwd or self.cfg.default_cwd
        label = name or f"{key} · voz"
        task = (task or "").strip()
        # DECISION: sin tarea se abre el agente listo y a la espera.
        task_text = ((GUI_INSTRUCTION if gui else "") + task) if task else ""

        if background:
            run_argv = (
                ["powershell.exe", "-NoProfile", "-Command", command_line(argv, launch.env)]
                if launch.env
                else argv
            )
            data = self._json(
                [
                    "run", "--json", "--bg", "--no-parent",
                    "--cwd", folder,
                    "--name", label,
                    "--label", f"agent={key}",
                    "--", *run_argv,
                ],
                timeout=40,
            )
            address = data.get("address")
            if not address:
                return {"ok": False, "error": f"hc run no devolvió dirección: {data}"}
            where: dict[str, Any] = {"visible": False, "background": True}
        else:
            args = [
                "do", "tab.new",
                "--name", label,
                "--folder", folder,
                "--argv", json.dumps([DEFAULT_SHELL]),
            ]
            if space:
                args += ["--space", space]
            data = self._json_app(args, timeout=45)
            address = data.get("terminal") or data.get("address")
            if not address:
                return {"ok": False, "error": f"tab.new no devolvió terminal: {data}"}
            self.wait_shell_prompt(address)  # sin pausa fija
            typed = self.send_agent(
                address, command_line(argv, launch.env), confirmed=True
            )
            if not typed.get("ok"):
                return {
                    "ok": False,
                    "error": f"No pude escribir el comando en {address}: {typed.get('error')}",
                }
            where = {
                "visible": True,
                "background": False,
                "tab": data.get("tab"),
                "space": data.get("space"),
            }

        base = {
            "address": address,
            "terminal": address,
            "agent": key,
            "model": model or "",
            "effort": effort or "",
            "cwd": folder,
            "command": argv,
            **where,
        }

        def finish() -> dict[str, Any]:
            outcome = self.finish_launch(address, key, launch, task_text)
            if not outcome.pop("ok"):
                return {**outcome, "ok": False, "address": address, "agent": key}
            return {"ok": True, **base, **outcome}

        if on_ready is None:
            return finish()

        # Asíncrono: la terminal ya está abierta y con el comando escrito, así
        # que se responde al instante y el arranque del programa (hasta ~14 s
        # en Codex/OpenCode), modelo, esfuerzo y tarea se resuelven aparte.
        def work() -> None:
            try:
                result = finish()
            except Exception as exc:  # noqa: BLE001 - se informa, no se pierde
                result = {
                    "ok": False,
                    "address": address,
                    "agent": key,
                    "error": f"{type(exc).__name__}: {exc}",
                }
            on_ready(result)

        threading.Thread(target=work, daemon=True, name="ez-launch").start()
        return {
            "ok": True,
            "launching": True,
            **base,
            "task_sent": False,
            "warnings": launch.warnings,
        }

    def focus_agent(self, address: str) -> dict[str, Any]:
        self._json_app(
            ["do", "agent.focus", "--terminal", address], timeout=15
        )
        return {"ok": True, "address": address, "focused": True}

    def kill_agent(self, address: str, confirmed: bool = False) -> dict[str, Any]:
        self._json(["kill", address], timeout=20)
        return {"ok": True, "address": address, "message": "Terminal terminado."}
