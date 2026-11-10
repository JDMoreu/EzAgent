"""Catálogo y lanzamiento de los agentes delegados.

Los agentes (OpenCode, Claude Code, Codex, mcode, pi...) se abren como lo haría
una persona: una terminal, el comando con sus flags (o variables de entorno),
esperar a que el programa arranque y, al final, la tarea. Aquí viven los
perfiles, la validación de modelo/esfuerzo y el catálogo; la sesión del agente
ya abierto (esperas, comandos internos, entrega de la tarea) está en
``agent_session``.

Todo lo que se escribe sale de los perfiles de ``voice.json`` o del código; el
LLM solo aporta valores (modelo, esfuerzo, tarea) que se validan antes.
"""

from __future__ import annotations

import re
import shutil
from typing import Any

from .agent_prelaunch import (
    discover_codex_models,
    discover_mcode_models,
    opencode_default_model,
    prepare_opencode_state,
    set_mcode_effort,
)
from .agent_session import (
    DEFAULT_SHELL,
    AgentLaunch,
    AgentSession,
    command_line,
    valid_keys,
)
from .config import resolve_agent_key

__all__ = [
    "DEFAULT_SHELL",
    "AgentDriver",
    "AgentLaunch",
    "command_line",
    "valid_keys",
]

# Modelo o esfuerzo que acaba dentro de un comando: sin espacios, comillas ni
# saltos de línea, y sin empezar por «-».
_SAFE_VALUE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._:/#@+-]{0,80}$")


class AgentDriver(AgentSession):
    """Mixin de ``Herdr``: requiere ``cfg``, ``get_agent`` y ``send_agent``."""

    # ------------------------------------------------------------ perfiles
    def resolve_agent(self, name: str) -> str | None:
        """Clave canónica de un agente a partir de cómo se dictó."""
        return resolve_agent_key(name, self.cfg.agents, self._profiles())

    def _profiles(self) -> dict[str, Any]:
        profiles = getattr(self.cfg, "agent_profiles", {}) or {}
        return profiles if isinstance(profiles, dict) else {}

    def _profile(self, key: str) -> dict[str, Any]:
        profile = self._profiles().get(key)
        return profile if isinstance(profile, dict) else {}

    @staticmethod
    def _text_list(value: Any) -> list[str]:
        if isinstance(value, str):
            return [value] if value.strip() else []
        if not isinstance(value, list):
            return []
        return [str(part) for part in value]

    def _check_value(self, key: str, kind: str, value: str) -> str:
        """Valida modelo/esfuerzo; devuelve el error o una cadena vacía."""
        if not _SAFE_VALUE.match(value):
            return f"El {kind} {value!r} no es válido."
        if kind == "esfuerzo":
            allowed = self._text_list(self._profile(key).get("efforts"))
            if allowed and value.lower() not in {a.lower() for a in allowed}:
                return (
                    f"El esfuerzo {value!r} no existe en {key}. "
                    f"Valores válidos: {', '.join(allowed)}."
                )
        return ""

    # ----------------------------------------------------------- lanzamiento
    def agent_launch(
        self,
        agent: str,
        model: str | None = None,
        effort: str | None = None,
    ) -> AgentLaunch:
        """Comando del agente y lo que se fijará dentro de la sesión.

        Si el CLI tiene flag para el modelo o el esfuerzo, va en el comando; si
        no, pero tiene comando de sesión, queda en ``pending``; si tampoco, se
        anota un aviso (el lanzamiento no falla).
        """
        key = self.resolve_agent(agent)
        if key is None:
            return AgentLaunch(
                error=(
                    f"Agente desconocido: {agent!r}. "
                    f"Disponibles: {', '.join(sorted(self.cfg.agents))}."
                )
            )
        profile = self._profile(key)
        if (model or effort) and not profile:
            return AgentLaunch(
                error=(
                    f"El agente {key!r} no tiene perfil de modelo/esfuerzo. "
                    "Configura agent_profiles en voice.json."
                )
            )
        wanted = {
            "model": str(model or profile.get("default_model") or "").strip(),
            "effort": str(effort or profile.get("default_effort") or "").strip(),
        }
        if (
            profile.get("prelaunch") == "opencode_state"
            and wanted["effort"]
            and not wanted["model"]
        ):
            # Un esfuerzo sin modelo no se puede fijar en el arranque: se usa
            # el modelo por defecto de OpenCode (evita ciclar variantes dentro).
            wanted["model"] = opencode_default_model()
        for kind, label in (("model", "modelo"), ("effort", "esfuerzo")):
            if wanted[kind]:
                problem = self._check_value(key, label, wanted[kind])
                if problem:
                    return AgentLaunch(error=problem)

        if profile.get("discover") == "mcode" and wanted["model"]:
            # mcode distingue mayúsculas (minimax/MiniMax-M3.1-Flash): se
            # resuelve contra los modelos de su config.yaml.
            known_models = discover_mcode_models()
            if known_models:
                low = wanted["model"].lower()
                match = next(
                    (
                        ref
                        for ref in known_models
                        if low in (ref.lower(), ref.lower().split("/", 1)[-1])
                    ),
                    None,
                )
                if match is None:
                    return AgentLaunch(
                        error=(
                            f"mcode no tiene el modelo {wanted['model']!r}. "
                            f"Modelos: {', '.join(known_models)}."
                        )
                    )
                wanted["model"] = match

        if profile.get("discover") == "codex":
            problem = self._resolve_codex(wanted)
            if problem:
                return AgentLaunch(error=problem)

        model_efforts = profile.get("model_efforts")
        if wanted["model"] and wanted["effort"] and isinstance(model_efforts, dict):
            name = wanted["model"].split("/")[-1]
            known = self._text_list(model_efforts.get(name))
            if known and wanted["effort"].lower() not in {k.lower() for k in known}:
                return AgentLaunch(
                    error=(
                        f"El modelo {name} no tiene el esfuerzo "
                        f"{wanted['effort']!r}. Valores válidos: "
                        f"{', '.join(known)}."
                    )
                )

        argv = self._text_list(profile.get("command")) or list(self.cfg.agents[key])
        flag_args = {
            "model": self._text_list(profile.get("model_args", ["--model", "{model}"])),
            "effort": self._text_list(profile.get("effort_args", ["--effort", "{effort}"])),
        }
        launch = AgentLaunch(argv=list(argv))
        model_value, effort_value = wanted["model"], wanted["effort"]
        prefix = str(profile.get("model_prefix") or "")
        model_ref = model_value if "/" in model_value or not model_value else prefix + model_value
        consumed: set[str] = set()
        env_tpl = profile.get("env")
        if model_value and isinstance(env_tpl, dict) and env_tpl:
            # Modelo y esfuerzo por variable de entorno (OpenCode lee
            # «proveedor/modelo#variante» de OPENCODE_CONFIG_CONTENT).
            variant = f"#{effort_value}" if effort_value else ""
            for name, tpl in env_tpl.items():
                launch.env[str(name)] = (
                    str(tpl).replace("{model_ref}", model_ref).replace("{variant}", variant)
                )
            launch.argv += self._text_list(profile.get("env_args"))
            if profile.get("prelaunch") == "opencode_state":
                launch.env.update(prepare_opencode_state(model_ref, effort_value))
            consumed = {"model", "effort"} if effort_value else {"model"}
        elif model_value and flag_args["model"]:
            # «{variant}» deja pegar el esfuerzo al modelo (mcode: modelo#var).
            has_variant = any("{variant}" in p for p in flag_args["model"])
            variant = f"#{effort_value}" if effort_value and has_variant else ""
            launch.argv += [
                p.replace("{model}", model_ref).replace("{variant}", variant)
                for p in flag_args["model"]
            ]
            consumed = {"model", "effort"} if has_variant else {"model"}
        if profile.get("prelaunch") == "mcode_effort" and effort_value:
            # mcode lee el esfuerzo de su config.yaml, no de un flag del TUI.
            if set_mcode_effort(effort_value):
                consumed.add("effort")
        for kind, label in (("model", "modelo"), ("effort", "esfuerzo")):
            value = wanted[kind]
            if not value or kind in consumed:
                continue
            if kind == "effort" and flag_args["effort"]:
                launch.argv += [p.replace("{effort}", value) for p in flag_args["effort"]]
            elif self._session_steps(key, kind):
                launch.pending[kind] = value
            else:
                launch.warnings.append(
                    f"{key} no permite fijar el {label} desde aquí; se abrió "
                    f"con su {label} por defecto."
                )
        return launch

    @staticmethod
    def _resolve_codex(wanted: dict[str, str]) -> str:
        """Valida modelo/esfuerzo contra el catálogo de Codex; '' si está bien."""
        table = discover_codex_models()
        if not table:
            return ""
        model, effort = wanted["model"], wanted["effort"]
        if model:
            slug = next((s for s in table if s.lower() == model.lower()), None)
            if slug is None:
                return f"Codex no tiene el modelo {model!r}. Modelos: {', '.join(table)}."
            wanted["model"] = slug
            levels = table[slug]
        else:
            levels = list(dict.fromkeys(x for v in table.values() for x in v))
        if effort and effort.lower() not in {x.lower() for x in levels}:
            return (
                f"El esfuerzo {effort!r} no existe en Codex"
                f"{' para ' + wanted['model'] if model else ''}. "
                f"Valores válidos: {', '.join(levels)}."
            )
        return ""

    def agent_argv(
        self,
        agent: str,
        model: str | None = None,
        effort: str | None = None,
    ) -> list[str] | None:
        """Atajo de ``agent_launch`` que devuelve solo el argv (o None)."""
        return self.agent_launch(agent, model, effort).argv

    # -------------------------------------------------------------- sesión

    # ------------------------------------------------------------ catálogo
    def agents_catalog(self) -> list[dict[str, Any]]:
        """Qué agentes hay y qué se puede decidir de cada uno."""
        catalog: list[dict[str, Any]] = []
        for key in sorted(self.cfg.agents):
            profile = self._profile(key)
            command = self._text_list(profile.get("command")) or list(self.cfg.agents[key])
            flags = {
                kind: bool(self._text_list(profile.get(f"{kind}_args")))
                for kind in ("model", "effort")
            }
            if isinstance(profile.get("env"), dict) and profile.get("env"):
                flags = {"model": True, "effort": True}
            elif any("{variant}" in p for p in self._text_list(profile.get("model_args"))):
                flags["effort"] = True
            catalog.append(
                {
                    "agent": key,
                    "label": str(profile.get("label") or key),
                    "aliases": self._text_list(profile.get("aliases")),
                    "models": (
                        discover_mcode_models()
                        if profile.get("discover") == "mcode"
                        else list(discover_codex_models())
                        if profile.get("discover") == "codex"
                        else []
                    )
                    or self._text_list(profile.get("models")),
                    "efforts": self._text_list(profile.get("efforts")),
                    "settable": [
                        kind
                        for kind in ("model", "effort")
                        if flags[kind] or self._session_steps(key, kind)
                    ],
                    "notes": str(profile.get("notes") or ""),
                    "installed": bool(command and shutil.which(command[0])),
                }
            )
        return catalog

    def catalog_text(self) -> str:
        """Bloque del system prompt con los agentes y sus posibilidades."""
        names = {"model": "modelo", "effort": "esfuerzo"}
        lines = ["# Agentes disponibles (datos del catálogo)"]
        for item in self.agents_catalog():
            parts = [f"- {item['agent']} ({item['label']})"]
            if item["aliases"]:
                parts.append("suena también «" + "», «".join(item["aliases"]) + "»")
            if item["models"]:
                parts.append("modelos: " + ", ".join(item["models"]))
            if item["efforts"]:
                parts.append("esfuerzo: " + ", ".join(item["efforts"]))
            settable = [names[k] for k in item["settable"]]
            parts.append("se puede fijar: " + (", ".join(settable) or "nada"))
            if not item["installed"]:
                parts.append("no se encuentra en el PATH")
            if item["notes"]:
                parts.append(item["notes"])
            lines.append("; ".join(parts))
        return "\n".join(lines)
