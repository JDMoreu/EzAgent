"""Carga y validación de la política de permisos (`permissions.toml`).

El archivo es la política que edita el usuario. Nunca lo escribe el agente.
Un archivo inválido no tumba el arranque: se avisa y se usan los valores por
defecto de la sección 6.4 del documento del núcleo.
"""

from __future__ import annotations

import logging
import os
import tomllib
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)

DEFAULT_PERMISSIONS_TOML = """\
[general]
max_tool_calls_per_turn = 4
confirm_timeout_s = 15
bulk_threshold = 5
max_file_mb = 10

[paths]
read_roots = ['~/Documents', '~/Desktop', '~/Downloads', '~/EzAgentWorkspace']
write_roots = ['~/EzAgentWorkspace', '~/Downloads']
deny = ['C:/Windows', 'C:/Program Files', 'C:/Program Files (x86)', '~/.ssh', '~/AppData', '%APPDATA%/EzAgent']
deny_globs = ['**/.env', '**/.env.*', '**/*.pem', '**/*.key', '**/id_rsa*', '**/*.kdbx']

[confirm]
accept = ['sí', 'si', 'confirmo', 'adelante', 'dale']
reject = ['no', 'cancela', 'para']

[web]
max_fetch_chars = 8000
block_private_ips = true

[skills]
allow_scripts = false
trusted = []

[tools_d]
allowed_executables = ['code']
"""

_DEFAULT_READ_ROOTS = ["~/Documents", "~/Desktop", "~/Downloads", "~/EzAgentWorkspace"]
_DEFAULT_WRITE_ROOTS = ["~/EzAgentWorkspace", "~/Downloads"]
_DEFAULT_DENY = [
    "C:/Windows",
    "C:/Program Files",
    "C:/Program Files (x86)",
    "~/.ssh",
    "~/AppData",
    "%APPDATA%/EzAgent",
]
_DEFAULT_DENY_GLOBS = ["**/.env", "**/.env.*", "**/*.pem", "**/*.key", "**/id_rsa*", "**/*.kdbx"]
_DEFAULT_ACCEPT = ["sí", "si", "confirmo", "adelante", "dale"]
_DEFAULT_REJECT = ["no", "cancela", "para"]

_SECTION_KEYS: dict[str, set[str]] = {
    "general": {"max_tool_calls_per_turn", "confirm_timeout_s", "bulk_threshold", "max_file_mb"},
    "paths": {"read_roots", "write_roots", "deny", "deny_globs"},
    "confirm": {"accept", "reject"},
    "web": {"max_fetch_chars", "block_private_ips"},
    "skills": {"allow_scripts", "trusted"},
    "tools_d": {"allowed_executables"},
}


@dataclass
class GeneralConfig:
    max_tool_calls_per_turn: int = 4
    confirm_timeout_s: float = 15.0
    bulk_threshold: int = 5
    max_file_mb: float = 10.0


@dataclass
class PathsConfig:
    read_roots: list[str]
    write_roots: list[str]
    deny: list[str]
    deny_globs: list[str]


@dataclass
class ConfirmConfig:
    accept: list[str]
    reject: list[str]


@dataclass
class WebConfig:
    max_fetch_chars: int = 8000
    block_private_ips: bool = True


@dataclass
class SkillsConfig:
    allow_scripts: bool = False
    # DECISION: default vacío para respetar el orden del contrato sin romper
    # la regla de dataclasses (un campo con default no puede ir antes de uno
    # obligatorio).
    trusted: list[str] = field(default_factory=list)


@dataclass
class ToolsDConfig:
    allowed_executables: list[str]


@dataclass
class AgentConfig:
    general: GeneralConfig
    paths: PathsConfig
    confirm: ConfirmConfig
    web: WebConfig
    skills: SkillsConfig
    tools_d: ToolsDConfig
    app_dir: Path
    workspace: Path
    permissions_path: Path
    system_md_path: Path
    memory_dir: Path
    history_path: Path
    audit_path: Path
    journal_dir: Path
    skills_dir: Path
    global_skills_dir: Path
    tools_d_dir: Path

    @classmethod
    def load(cls, path: Path | None = None) -> "AgentConfig":
        """Carga la política; crea la plantilla por defecto si no existe.

        Si se pasa ``path``, su carpeta se toma como ``app_dir`` para que las
        pruebas y los perfiles alternativos no dependan de %APPDATA%.
        """
        permissions_path = Path(path) if path is not None else _default_app_dir() / "permissions.toml"
        if not permissions_path.exists():
            _write_default_template(permissions_path)
        data = _read_toml(permissions_path)
        _warn_unknown_keys(data)

        general_data = _section(data, "general")
        paths_data = _section(data, "paths")
        confirm_data = _section(data, "confirm")
        web_data = _section(data, "web")
        skills_data = _section(data, "skills")
        tools_d_data = _section(data, "tools_d")

        general = GeneralConfig(
            max_tool_calls_per_turn=_get_int(
                general_data, "max_tool_calls_per_turn", 4, "general"
            ),
            confirm_timeout_s=_get_float(
                general_data, "confirm_timeout_s", 15.0, "general"
            ),
            bulk_threshold=_get_int(general_data, "bulk_threshold", 5, "general"),
            max_file_mb=_get_float(general_data, "max_file_mb", 10.0, "general"),
        )
        paths = PathsConfig(
            read_roots=_get_str_list(paths_data, "read_roots", _DEFAULT_READ_ROOTS, "paths"),
            write_roots=_get_str_list(
                paths_data, "write_roots", _DEFAULT_WRITE_ROOTS, "paths"
            ),
            deny=_get_str_list(paths_data, "deny", _DEFAULT_DENY, "paths"),
            deny_globs=_get_str_list(
                paths_data, "deny_globs", _DEFAULT_DENY_GLOBS, "paths"
            ),
        )
        confirm = ConfirmConfig(
            accept=_get_str_list(confirm_data, "accept", _DEFAULT_ACCEPT, "confirm"),
            reject=_get_str_list(confirm_data, "reject", _DEFAULT_REJECT, "confirm"),
        )
        web = WebConfig(
            max_fetch_chars=_get_int(web_data, "max_fetch_chars", 8000, "web"),
            block_private_ips=_get_bool(
                web_data, "block_private_ips", True, "web"
            ),
        )
        skills = SkillsConfig(
            allow_scripts=_get_bool(skills_data, "allow_scripts", False, "skills"),
            trusted=_get_str_list(skills_data, "trusted", [], "skills"),
        )
        tools_d = ToolsDConfig(
            allowed_executables=_get_str_list(
                tools_d_data, "allowed_executables", ["code"], "tools_d"
            )
        )

        app_dir = permissions_path.parent
        memory_dir = app_dir / "memory"
        return cls(
            general=general,
            paths=paths,
            confirm=confirm,
            web=web,
            skills=skills,
            tools_d=tools_d,
            app_dir=app_dir,
            workspace=Path.home() / "EzAgentWorkspace",
            permissions_path=permissions_path,
            system_md_path=app_dir / "system.md",
            memory_dir=memory_dir,
            history_path=memory_dir / "history.db",
            audit_path=app_dir / "logs" / "audit.jsonl",
            journal_dir=app_dir / "journal",
            skills_dir=app_dir / "skills",
            global_skills_dir=Path.home() / ".agents" / "skills",
            tools_d_dir=app_dir / "tools.d",
        )

    def ensure_dirs(self) -> None:
        """Crea las carpetas de trabajo de la app y el workspace."""
        # DECISION: ~/.agents/skills es compartida con otros clientes y puede
        # no existir; el descubrimiento de skills debe tolerarlo y no la crea.
        for directory in (
            self.app_dir,
            self.workspace,
            self.memory_dir,
            self.audit_path.parent,
            self.journal_dir,
            self.skills_dir,
            self.tools_d_dir,
        ):
            directory.mkdir(parents=True, exist_ok=True)

    def reload(self) -> "AgentConfig":
        return AgentConfig.load(self.permissions_path)


def _default_app_dir() -> Path:
    base = os.getenv("APPDATA") or os.getenv("XDG_CONFIG_HOME")
    if base:
        return Path(base) / "EzAgent"
    return Path.home() / ".config" / "EzAgent"


def _write_default_template(path: Path) -> None:
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(DEFAULT_PERMISSIONS_TOML, encoding="utf-8")
        logger.info("Creada la política por defecto en %s", path)
    except OSError as exc:
        logger.warning(
            "No se pudo crear %s; se usan los valores por defecto: %s", path, exc
        )


def _read_toml(path: Path) -> dict[str, Any]:
    try:
        raw = path.read_bytes()
        data = tomllib.loads(raw.decode("utf-8"))
    except FileNotFoundError:
        return {}
    except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError) as exc:
        logger.warning(
            "No se pudo leer %s; se usan los valores por defecto: %s", path, exc
        )
        return {}
    if not isinstance(data, dict):
        logger.warning("%s no contiene una tabla TOML; se usan los valores por defecto", path)
        return {}
    return data


def _warn_unknown_keys(data: dict[str, Any]) -> None:
    for section_name, section in data.items():
        if section_name not in _SECTION_KEYS:
            logger.warning(
                "Sección desconocida en permissions.toml: [%s] (se ignora)",
                section_name,
            )
            continue
        if not isinstance(section, dict):
            logger.warning(
                "La sección [%s] debe ser una tabla; se usan los valores por defecto",
                section_name,
            )
            continue
        for key in section:
            if key == "destructive_always_confirm":
                logger.warning(
                    "La opción destructive_always_confirm no existe; el borrado "
                    "siempre pide confirmación. Se ignora."
                )
            elif key not in _SECTION_KEYS[section_name]:
                logger.warning(
                    "Clave desconocida en [%s]: %s (se ignora)", section_name, key
                )


def _section(data: dict[str, Any], name: str) -> dict[str, Any]:
    value = data.get(name)
    return value if isinstance(value, dict) else {}


def _get_int(section: dict[str, Any], key: str, default: int, section_name: str) -> int:
    value = section.get(key, default)
    if isinstance(value, bool) or not isinstance(value, int):
        logger.warning(
            "Tipo inválido en %s.%s: se esperaba un entero y se usa %s",
            section_name,
            key,
            default,
        )
        return default
    return value


def _get_float(
    section: dict[str, Any], key: str, default: float, section_name: str
) -> float:
    value = section.get(key, default)
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        logger.warning(
            "Tipo inválido en %s.%s: se esperaba un número y se usa %s",
            section_name,
            key,
            default,
        )
        return default
    return float(value)


def _get_bool(
    section: dict[str, Any], key: str, default: bool, section_name: str
) -> bool:
    value = section.get(key, default)
    if not isinstance(value, bool):
        logger.warning(
            "Tipo inválido en %s.%s: se esperaba true/false y se usa %s",
            section_name,
            key,
            default,
        )
        return default
    return value


def _get_str_list(
    section: dict[str, Any], key: str, default: list[str], section_name: str
) -> list[str]:
    value = section.get(key, default)
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        logger.warning(
            "Tipo inválido en %s.%s: se esperaba una lista de textos y se usa %s",
            section_name,
            key,
            default,
        )
        return list(default)
    return list(value)
