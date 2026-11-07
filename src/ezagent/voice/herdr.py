"""Adaptador entre `ezagent-voice` y Herdr (`hc` + `herdr-app`)."""

from __future__ import annotations

import json
import re
import subprocess
from typing import Any

from .config import VoiceConfig

_ANSI = re.compile(r"\x1b\[[0-9;?]*[ -/]*[@-~]")
_OSC = re.compile(r"\x1b\].*?(?:\x07|\x1b\\)", re.DOTALL)

GUI_INSTRUCTION = (
    "Trabaja de forma visible en el escritorio. Usa la skill cua-driver para "
    "controlar la interfaz gráfica: abre las aplicaciones, navega, haz clic y "
    "escribe como lo haría una persona. Deja las ventanas a la vista para que "
    "el usuario vea todo el proceso. "
)


class HerdrError(RuntimeError):
    pass


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


class Herdr:
    def __init__(self, cfg: VoiceConfig):
        self.cfg = cfg

    # ------------------------------------------------------------------ base
    def _run_command(
        self, command: list[str], timeout: float
    ) -> subprocess.CompletedProcess:
        try:
            return subprocess.run(
                command,
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=timeout,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
        except FileNotFoundError as exc:
            raise HerdrError(
                f"No encuentro `{command[0]}`. Revisa voice.json."
            ) from exc
        except subprocess.TimeoutExpired as exc:
            raise HerdrError(f"`{command[0]}` tardó demasiado.") from exc

    def _run(self, args: list[str], timeout: float = 20.0) -> subprocess.CompletedProcess:
        return self._run_command([self.cfg.hc_path, *args], timeout)

    def _run_app(
        self, args: list[str], timeout: float = 20.0
    ) -> subprocess.CompletedProcess:
        return self._run_command([self.cfg.herdr_app_path, *args], timeout)

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

    def read_agent(self, address: str, lines: int | None = None) -> str:
        lines = lines or self.cfg.max_read_lines
        result = self._run(["read", address, "--lines", str(lines)], timeout=20)
        text = strip_ansi(result.stdout or "")
        if not text:
            text = strip_ansi(result.stderr or "")
        return text[-6000:]

    # --------------------------------------------------------------- acciones
    def send_agent(
        self, address: str, text: str, confirmed: bool = False
    ) -> dict[str, Any]:
        result = self._run(["send", address, text, "--json"], timeout=40)
        output = (result.stdout or "").strip()
        if result.returncode != 0:
            return {"ok": False, "error": (result.stderr or output)[:800]}
        return {"ok": True, "address": address, "message": "Mensaje enviado."}

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

    def agent_argv(
        self,
        agent: str,
        model: str | None = None,
        effort: str | None = None,
    ) -> list[str] | None:
        """Comando del agente con modelo/esfuerzo según su perfil."""
        key = agent.strip().lower()
        base = self.cfg.agents.get(key)
        if not base:
            return None
        profiles = getattr(self.cfg, "agent_profiles", {}) or {}
        profile = profiles.get(key)
        if (model or effort) and not isinstance(profile, dict):
            return None
        profile = profile if isinstance(profile, dict) else {}
        command = profile.get("command") or base
        if not isinstance(command, list):
            return None
        argv = [str(part) for part in command]
        selected_model = str(
            model or profile.get("default_model") or ""
        ).strip()
        selected_effort = str(
            effort or profile.get("default_effort") or ""
        ).strip()
        if selected_model:
            args = profile.get("model_args", ["--model", "{model}"])
            if not isinstance(args, list) or not args:
                return None
            argv += [
                str(part).replace("{model}", selected_model) for part in args
            ]
        if selected_effort:
            args = profile.get("effort_args", ["--effort", "{effort}"])
            if not isinstance(args, list) or not args:
                return None
            argv += [
                str(part).replace("{effort}", selected_effort) for part in args
            ]
        return argv

    def run_agent(
        self,
        agent: str,
        task: str,
        cwd: str | None = None,
        name: str | None = None,
        confirmed: bool = False,
        background: bool = False,
        gui: bool = False,
        space: str | None = None,
        model: str | None = None,
        effort: str | None = None,
    ) -> dict[str, Any]:
        key = agent.strip().lower()
        if key not in self.cfg.agents:
            return {
                "ok": False,
                "error": f"Agente desconocido: {agent!r}.",
                "available": sorted(self.cfg.agents),
            }
        argv = self.agent_argv(key, model, effort)
        if not argv:
            return {
                "ok": False,
                "error": (
                    f"El agente {key!r} no tiene perfil de modelo/esfuerzo. "
                    "Configura agent_profiles en voice.json."
                ),
            }
        folder = cwd or self.cfg.default_cwd
        label = name or f"{key} · voz"
        task_text = (GUI_INSTRUCTION if gui else "") + task

        if background:
            data = self._json(
                [
                    "run", "--json", "--bg", "--no-parent",
                    "--cwd", folder,
                    "--name", label,
                    "--label", f"agent={key}",
                    "--", *argv,
                ],
                timeout=40,
            )
            address = data.get("address")
            if not address:
                return {"ok": False, "error": f"hc run no devolvió dirección: {data}"}
            sent = self.send_agent(address, task_text, confirmed=True)
            return {
                "ok": True,
                "address": address,
                "terminal": address,
                "agent": key,
                "model": model or "",
                "effort": effort or "",
                "cwd": folder,
                "visible": False,
                "background": True,
                "task_sent": bool(sent.get("ok")),
                "send": sent,
            }

        # Por defecto: pestaña visible en Herdr, con la tarea como primer mensaje.
        args = [
            "do", "tab.new",
            "--name", label,
            "--folder", folder,
            "--argv", json.dumps(argv),
            "--task", task_text,
        ]
        if space:
            args += ["--space", space]
        data = self._json_app(args, timeout=45)
        terminal = data.get("terminal") or data.get("address")
        if not terminal:
            return {"ok": False, "error": f"tab.new no devolvió terminal: {data}"}
        return {
            "ok": True,
            "address": terminal,
            "terminal": terminal,
            "tab": data.get("tab"),
            "space": data.get("space"),
            "agent": key,
            "model": model or "",
            "effort": effort or "",
            "cwd": folder,
            "visible": True,
            "background": False,
            "task_sent": True,
        }

    def focus_agent(self, address: str) -> dict[str, Any]:
        self._json_app(
            ["do", "agent.focus", "--terminal", address], timeout=15
        )
        return {"ok": True, "address": address, "focused": True}

    def kill_agent(self, address: str, confirmed: bool = False) -> dict[str, Any]:
        self._json(["kill", address], timeout=20)
        return {"ok": True, "address": address, "message": "Terminal terminado."}
