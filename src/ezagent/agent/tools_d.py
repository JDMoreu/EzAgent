"""Herramientas declarativas de ``tools.d/*.toml``.

Permiten añadir herramientas sin escribir Python.  Las reglas de seguridad de
la sección 6.3 del documento son obligatorias:

1. ``command`` es una lista de argumentos, nunca una cadena.
2. Los parámetros se sustituyen después de validar, cada uno como un elemento
   independiente de la lista.
3. El ejecutable debe estar en ``[tools_d] allowed_executables``.
4. Nivel mínimo 1; si falta ``level`` se asume 2.
5. Validadores: ``path_in_read_roots``, ``path_in_write_roots``, ``integer``,
   ``url_http`` y ``enum:a,b,c``.
6. Un archivo inválido se ignora y se registra, sin tumbar el arranque.
"""

from __future__ import annotations

import logging
import os
import re
import subprocess
import tomllib
from collections.abc import Sequence
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit

from ezagent.agent.config import AgentConfig
from ezagent.agent.paths import PathDenied, resolve_safe
from ezagent.agent.registry import ToolInfo, ToolResult, ToolSpec

logger = logging.getLogger(__name__)

DEFAULT_LEVEL = 2
DEFAULT_TIMEOUT_S = 10.0
MAX_OUTPUT_CHARS = 8000

_PARAM_RE = re.compile(r"\{([A-Za-z_][A-Za-z0-9_]*)\}")
_PARAM_NAME_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
_TOOL_NAME_RE = re.compile(r"^[a-z][a-z0-9_]{0,63}$")
_ALLOWED_TYPES = {"string", "integer", "number", "boolean"}
_ALLOWED_EXECUTABLES_ENV = ("PATH", "SYSTEMROOT", "TEMP")
_INVALID = object()


def clean_environment() -> dict[str, str]:
    """Entorno mínimo para subprocess: solo PATH, SYSTEMROOT y TEMP.

    Nunca se heredan claves de API, tokens ni variables del proyecto.
    """

    env: dict[str, str] = {}
    for name in _ALLOWED_EXECUTABLES_ENV:
        value = os.environ.get(name)
        if value is not None:
            env[name] = value
    return env


def load_tools_d(config: AgentConfig) -> list[ToolSpec]:
    """Carga todas las herramientas válidas de ``config.tools_d_dir``."""
    directory = Path(config.tools_d_dir)
    try:
        if not directory.is_dir():
            return []
        files = sorted(
            (
                path
                for path in directory.iterdir()
                if path.is_file() and path.suffix.lower() == ".toml"
            ),
            key=lambda path: path.name.lower(),
        )
    except OSError as exc:
        logger.warning("tools.d: no se pudo leer %s: %s", directory, exc)
        return []

    allowed = {
        entry.strip().casefold()
        for entry in getattr(config.tools_d, "allowed_executables", [])
        if isinstance(entry, str) and entry.strip()
    }

    tools: list[ToolSpec] = []
    seen: set[str] = set()
    for path in files:
        try:
            with path.open("rb") as handle:
                data = tomllib.load(handle)
        except (OSError, tomllib.TOMLDecodeError) as exc:
            logger.warning(
                "tools.d: se ignora %s: TOML inválido o ilegible (%s)",
                path.name,
                exc,
            )
            continue

        tool = _build_tool(data, config, allowed, path.name)
        if tool is None:
            continue
        if tool.name in seen:
            logger.warning(
                "tools.d: el nombre «%s» está duplicado; se ignora %s",
                tool.name,
                path.name,
            )
            continue
        seen.add(tool.name)
        tools.append(tool)
    return tools


def _build_tool(
    data: dict[str, Any],
    config: AgentConfig,
    allowed: set[str],
    filename: str,
) -> ToolSpec | None:
    name = data.get("name")
    if not isinstance(name, str) or not _TOOL_NAME_RE.match(name.strip()):
        _reject(filename, "falta un «name» válido (minúsculas, números y guion bajo)")
        return None
    name = name.strip()

    description = data.get("description")
    if not isinstance(description, str) or not description.strip():
        _reject(filename, "falta «description»")
        return None

    command = data.get("command")
    if not isinstance(command, list) or not command:
        _reject(
            filename,
            "«command» debe ser una lista de argumentos, nunca una cadena",
        )
        return None
    if not all(isinstance(item, str) and item.strip() for item in command):
        _reject(filename, "«command» solo admite argumentos de texto no vacíos")
        return None
    if any("\x00" in item for item in command):
        _reject(filename, "«command» contiene bytes nulos")
        return None

    executable = command[0].strip()
    # DECISION: se compara el nombre declarado tal cual (sin resolverlo por
    # PATH); así una ruta completa no puede suplantar a un ejecutable listado.
    if executable.casefold() not in allowed:
        _reject(
            filename,
            f"el ejecutable «{executable}» no está en "
            "[tools_d] allowed_executables",
        )
        return None

    level_raw = data.get("level", DEFAULT_LEVEL)
    if isinstance(level_raw, bool) or not isinstance(level_raw, int):
        _reject(filename, "«level» debe ser un entero")
        return None
    level = level_raw
    if level < 1:
        # DECISION: el nivel mínimo es 1; se corrige hacia arriba en vez de
        # rechazar toda la herramienta.
        logger.warning(
            "tools.d: %s declara level=%s; se usa el nivel mínimo 1",
            filename,
            level,
        )
        level = 1

    timeout_raw = data.get("timeout_s", DEFAULT_TIMEOUT_S)
    if (
        isinstance(timeout_raw, bool)
        or not isinstance(timeout_raw, (int, float))
        or float(timeout_raw) <= 0
    ):
        _reject(filename, "«timeout_s» debe ser un número mayor que cero")
        return None
    timeout_s = float(timeout_raw)

    params_data = data.get("params", {})
    if not isinstance(params_data, dict):
        _reject(filename, "«params» debe ser una tabla")
        return None
    parsed = _parse_params(params_data, filename)
    if parsed is None:
        return None
    params, properties, required = parsed

    for element in command:
        for match in _PARAM_RE.finditer(element):
            if match.group(1) not in params:
                _reject(
                    filename,
                    f"«command» usa un parámetro no declarado: "
                    f"{{{match.group(1)}}}",
                )
                return None

    # DECISION: las herramientas declarativas no marcan taint por defecto; el
    # usuario puede activarlo con untrusted_output = true si su salida es
    # contenido externo que el LLM no debe obedecer.
    untrusted = data.get("untrusted_output", False)
    if not isinstance(untrusted, bool):
        _reject(filename, "«untrusted_output» debe ser true o false")
        return None

    command_tuple = tuple(command)
    params_copy = dict(params)
    roots_read = tuple(getattr(config.paths, "read_roots", ()))
    roots_write = tuple(getattr(config.paths, "write_roots", ()))
    deny = tuple(getattr(config.paths, "deny", ()))
    deny_globs = tuple(getattr(config.paths, "deny_globs", ()))
    workspace = Path(config.workspace)

    def handler(args: dict[str, Any]) -> ToolResult:
        return _run_declarative_tool(
            tool_name=name,
            command=command_tuple,
            params=params_copy,
            args=args,
            workspace=workspace,
            roots_read=roots_read,
            roots_write=roots_write,
            deny=deny,
            deny_globs=deny_globs,
            timeout_s=timeout_s,
        )

    def describe(_args: dict[str, Any]) -> ToolInfo:
        return ToolInfo(
            level=level,
            summary=f"Ejecutar «{executable}» con una herramienta de tools.d",
        )

    schema = {
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": False,
    }
    return ToolSpec(
        name=name,
        description=description.strip(),
        parameters=schema,
        level=level,
        handler=handler,
        untrusted_output=untrusted,
        # DECISION: el registro ejecuta el manejador en un hilo con este
        # timeout; se deja un segundo de margen para que el subprocess pueda
        # agotar su propio timeout y devolver un error claro.
        timeout_s=timeout_s + 1.0,
        describe_fn=describe,
    )


def _parse_params(
    params_data: dict[str, Any], filename: str
) -> tuple[
    dict[str, dict[str, Any]], dict[str, dict[str, Any]], list[str]
] | None:
    params: dict[str, dict[str, Any]] = {}
    properties: dict[str, dict[str, Any]] = {}
    required: list[str] = []
    for raw_name, raw_spec in params_data.items():
        if not isinstance(raw_name, str) or not _PARAM_NAME_RE.match(raw_name):
            _reject(filename, f"nombre de parámetro inválido: {raw_name!r}")
            return None
        if not isinstance(raw_spec, dict):
            _reject(filename, f"el parámetro «{raw_name}» debe ser una tabla")
            return None

        param_type = raw_spec.get("type", "string")
        if not isinstance(param_type, str) or param_type not in _ALLOWED_TYPES:
            _reject(
                filename,
                f"tipo inválido en el parámetro «{raw_name}»: {param_type!r}",
            )
            return None

        description = raw_spec.get("description", "")
        if not isinstance(description, str):
            _reject(
                filename,
                f"«description» del parámetro «{raw_name}» debe ser texto",
            )
            return None

        required_raw = raw_spec.get("required", True)
        if not isinstance(required_raw, bool):
            _reject(
                filename,
                f"«required» del parámetro «{raw_name}» debe ser true o false",
            )
            return None

        validator, choices = _parse_validator(raw_spec, raw_name, filename)
        if validator is _INVALID:
            return None

        minimum = raw_spec.get("minimum")
        maximum = raw_spec.get("maximum")
        for bound_name, bound in (("minimum", minimum), ("maximum", maximum)):
            if bound is not None and (
                isinstance(bound, bool) or not isinstance(bound, (int, float))
            ):
                _reject(
                    filename,
                    f"«{bound_name}» del parámetro «{raw_name}» debe ser numérico",
                )
                return None

        prop: dict[str, Any] = {"type": param_type}
        if description.strip():
            prop["description"] = description.strip()
        if choices is not None:
            prop["enum"] = list(choices)
        if minimum is not None:
            prop["minimum"] = minimum
        if maximum is not None:
            prop["maximum"] = maximum
        properties[raw_name] = prop
        params[raw_name] = {
            "type": param_type,
            "required": required_raw,
            "validate": validator,
            "choices": choices,
            "minimum": minimum,
            "maximum": maximum,
        }
        if required_raw:
            required.append(raw_name)
    return params, properties, required


def _parse_validator(
    raw_spec: dict[str, Any], param_name: str, filename: str
) -> tuple[Any, tuple[str, ...] | None]:
    raw = raw_spec.get("validate")
    if raw is None:
        return None, None
    if not isinstance(raw, str):
        _reject(filename, f"«validate» del parámetro «{param_name}» debe ser texto")
        return _INVALID, None
    value = raw.strip()
    if value in ("path_in_read_roots", "path_in_write_roots", "integer", "url_http"):
        return value, None
    if value.startswith("enum:"):
        choices = tuple(part.strip() for part in value[5:].split(",") if part.strip())
        if not choices:
            _reject(
                filename,
                f"el validador enum del parámetro «{param_name}» no tiene valores",
            )
            return _INVALID, None
        return "enum", choices
    _reject(
        filename,
        f"validador desconocido en el parámetro «{param_name}»: «{value}»",
    )
    return _INVALID, None


def _run_declarative_tool(
    *,
    tool_name: str,
    command: Sequence[str],
    params: dict[str, dict[str, Any]],
    args: dict[str, Any],
    workspace: Path,
    roots_read: Sequence[str],
    roots_write: Sequence[str],
    deny: Sequence[str],
    deny_globs: Sequence[str],
    timeout_s: float,
) -> ToolResult:
    if not isinstance(args, dict):
        return ToolResult(ok=False, error="Los argumentos deben ser un objeto.")

    extra = sorted(set(args) - set(params))
    if extra:
        return ToolResult(
            ok=False,
            error="Parámetros no admitidos: " + ", ".join(extra),
        )

    values: dict[str, str] = {}
    for param_name, spec in params.items():
        if param_name not in args or args[param_name] is None:
            if spec["required"]:
                return ToolResult(
                    ok=False,
                    error=f"Falta el parámetro obligatorio «{param_name}».",
                )
            continue
        try:
            values[param_name] = _validate_value(
                param_name,
                args[param_name],
                spec,
                workspace=workspace,
                roots_read=roots_read,
                roots_write=roots_write,
                deny=deny,
                deny_globs=deny_globs,
            )
        except ValueError as exc:
            return ToolResult(ok=False, error=f"{tool_name}: {exc}")

    argv: list[str] = []
    for element in command:
        replaced = _PARAM_RE.sub(
            lambda match: values.get(match.group(1), match.group(0)), element
        )
        unresolved = _PARAM_RE.search(replaced)
        if unresolved is not None:
            return ToolResult(
                ok=False,
                error=(
                    "Falta el valor del parámetro "
                    f"«{unresolved.group(1)}» para ejecutar la herramienta."
                ),
            )
        argv.append(replaced)

    env = clean_environment()
    try:
        completed = subprocess.run(
            argv,
            shell=False,
            cwd=str(workspace),
            env=env,
            timeout=timeout_s,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            check=False,
        )
    except subprocess.TimeoutExpired:
        return ToolResult(
            ok=False,
            error=f"La herramienta superó el tiempo máximo ({timeout_s:g} s).",
        )
    except OSError as exc:
        return ToolResult(
            ok=False,
            error=f"No se pudo ejecutar «{command[0]}»: {exc}",
        )

    stdout = _clip(completed.stdout)
    stderr = _clip(completed.stderr)
    returncode = int(completed.returncode)
    ok = returncode == 0
    error: str | None = None
    if not ok:
        error = stderr.strip() or f"La herramienta terminó con código {returncode}."
    return ToolResult(
        ok=ok,
        data={"returncode": returncode, "stdout": stdout, "stderr": stderr},
        error=error,
    )


def _validate_value(
    name: str,
    value: Any,
    spec: dict[str, Any],
    *,
    workspace: Path,
    roots_read: Sequence[str],
    roots_write: Sequence[str],
    deny: Sequence[str],
    deny_globs: Sequence[str],
) -> str:
    expected = spec["type"]
    if expected == "integer":
        if isinstance(value, bool):
            raise ValueError(f"«{name}» debe ser un entero.")
        if isinstance(value, int):
            number = value
        elif isinstance(value, str) and re.fullmatch(r"[+-]?\d+", value.strip()):
            number = int(value.strip())
        else:
            raise ValueError(f"«{name}» debe ser un entero.")
        minimum = spec.get("minimum")
        maximum = spec.get("maximum")
        if minimum is not None and number < minimum:
            raise ValueError(f"«{name}» debe ser mayor o igual que {minimum}.")
        if maximum is not None and number > maximum:
            raise ValueError(f"«{name}» debe ser menor o igual que {maximum}.")
        value = number
    elif expected == "number":
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            raise ValueError(f"«{name}» debe ser un número.")
        minimum = spec.get("minimum")
        maximum = spec.get("maximum")
        if minimum is not None and value < minimum:
            raise ValueError(f"«{name}» debe ser mayor o igual que {minimum}.")
        if maximum is not None and value > maximum:
            raise ValueError(f"«{name}» debe ser menor o igual que {maximum}.")
    elif expected == "boolean":
        if not isinstance(value, bool):
            raise ValueError(f"«{name}» debe ser true o false.")
    elif not isinstance(value, str):
        raise ValueError(f"«{name}» debe ser texto.")

    validator = spec.get("validate")
    if validator == "integer":
        if isinstance(value, bool):
            raise ValueError(f"«{name}» debe ser un entero.")
        if isinstance(value, int):
            return str(value)
        if isinstance(value, str) and re.fullmatch(r"[+-]?\d+", value.strip()):
            return str(int(value.strip()))
        raise ValueError(f"«{name}» debe ser un entero.")

    text = _stringify(value)

    if validator in ("path_in_read_roots", "path_in_write_roots"):
        roots = roots_read if validator == "path_in_read_roots" else roots_write
        try:
            resolved = resolve_safe(
                text,
                workspace,
                roots,
                deny,
                deny_globs,
            )
        except PathDenied as exc:
            raise ValueError(f"ruta no permitida: {exc}") from exc
        return str(resolved)

    if validator == "url_http":
        parts = urlsplit(text)
        if parts.scheme.lower() not in ("http", "https") or not parts.hostname:
            raise ValueError(f"«{name}» debe ser una URL http o https válida.")
        if parts.username is not None or parts.password is not None:
            raise ValueError(f"«{name}» no puede incluir credenciales en la URL.")
        return text

    if validator == "enum":
        choices = spec.get("choices") or ()
        if text not in choices:
            raise ValueError(
                f"«{name}» debe ser uno de: " + ", ".join(str(c) for c in choices)
            )
        return text

    return text


def _stringify(value: Any) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    return str(value)


def _clip(value: str | None) -> str:
    return (value or "")[:MAX_OUTPUT_CHARS]


def _reject(filename: str, reason: str) -> None:
    logger.warning("tools.d: se ignora %s: %s", filename, reason)


__all__ = ["clean_environment", "load_tools_d"]
