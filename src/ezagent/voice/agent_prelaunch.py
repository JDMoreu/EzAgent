"""Preparativos previos al lanzamiento de agentes que guardan estado propio.

OpenCode recuerda en ``model.json`` la última variante (esfuerzo) elegida para
cada modelo y esa elección tiene prioridad sobre la configuración con la que se
lanza. Si alguna vez quedó guardado ``default``, el esfuerzo pedido por EzAgent
se ignoraba en silencio. Para que el lanzamiento sea determinista, EzAgent le
da a OpenCode un directorio de estado propio (``XDG_STATE_HOME``) con la
variante pedida ya escrita.
"""

from __future__ import annotations

import json
import os
import re
from pathlib import Path

import yaml
from loguru import logger


def opencode_state_root() -> Path:
    base = os.environ.get("LOCALAPPDATA") or str(Path.home() / ".local" / "state")
    return Path(base) / "EzAgent" / "opencode-state"


def prepare_opencode_state(model_ref: str, effort: str) -> dict[str, str]:
    """Escribe ``model.json`` y devuelve las variables de entorno a fijar.

    ``model_ref`` es ``proveedor/modelo``. Con ``effort`` vacío no se fija
    ninguna variante. Si no se puede escribir, devuelve ``{}`` (el lanzamiento
    sigue con el estado normal de OpenCode).
    """
    provider, _, model = model_ref.partition("/")
    if not provider or not model:
        return {}
    root = opencode_state_root()
    target = root / "opencode" / "model.json"
    data = {
        "recent": [{"providerID": provider, "modelID": model}],
        "favorite": [],
        "variant": {model_ref: effort} if effort else {},
    }
    try:
        target.parent.mkdir(parents=True, exist_ok=True)
        tmp = target.with_name(target.name + ".tmp")
        tmp.write_text(json.dumps(data, ensure_ascii=False), encoding="utf-8")
        os.replace(tmp, target)
    except OSError as exc:
        logger.warning("No pude preparar el estado de OpenCode: {}", exc)
        return {}
    return {"XDG_STATE_HOME": str(root)}


# --------------------------------------------------------------------- mcode
MCODE_EFFORTS = ("default", "low", "medium", "high", "xhigh", "max")
_MCODE_EFFORT_RE = re.compile(
    r"(?m)^(defaultModelThinking:[ \t]*\r?\n[ \t]+effort:[ \t]*)\S+"
)


def mcode_config_path() -> Path:
    base = os.environ.get("MINIMAX_DATA_DIR") or str(Path.home() / ".minimax")
    return Path(base) / "config.yaml"


def discover_mcode_models() -> list[str]:
    """Modelos ``proveedor/modelo`` declarados en el config.yaml de mcode."""
    try:
        data = yaml.safe_load(mcode_config_path().read_text(encoding="utf-8"))
    except (OSError, yaml.YAMLError):
        return []
    providers = data.get("provider") if isinstance(data, dict) else None
    refs: list[str] = []
    for provider, spec in (providers or {}).items():
        models = spec.get("models") if isinstance(spec, dict) else None
        refs += [f"{provider}/{name}" for name in (models or {})]
    return refs


def set_mcode_effort(effort: str) -> bool:
    """Fija ``defaultModelThinking.effort`` en el config.yaml de mcode.

    mcode no tiene un flag de esfuerzo para su TUI: lo lee de ese ajuste (el
    mismo que escribe su selector /model). Es una edición de una sola línea,
    sin reescribir el YAML (tiene anclas); antes se guarda una copia
    ``config.yaml.ezagent-bak`` la primera vez. Devuelve False si no pudo.
    """
    if effort not in MCODE_EFFORTS:
        return False
    path = mcode_config_path()
    try:
        with open(path, encoding="utf-8", newline="") as handle:
            text = handle.read()
        if _MCODE_EFFORT_RE.search(text):
            new = _MCODE_EFFORT_RE.sub(lambda m: m.group(1) + effort, text, 1)
        else:
            newline = "\r\n" if "\r\n" in text else "\n"
            sep = "" if text.endswith(("\n", "\r\n")) else newline
            new = f"{text}{sep}defaultModelThinking:{newline}  effort: {effort}{newline}"
        if new == text:
            return True
        backup = path.with_name(path.name + ".ezagent-bak")
        if not backup.exists():
            backup.write_text(text, encoding="utf-8", newline="")
        tmp = path.with_name(path.name + ".tmp")
        tmp.write_text(new, encoding="utf-8", newline="")
        os.replace(tmp, path)
    except OSError as exc:
        logger.warning("No pude fijar el esfuerzo de mcode: {}", exc)
        return False
    return True


# --------------------------------------------------------------------- codex
def codex_home() -> Path:
    return Path(os.environ.get("CODEX_HOME") or Path.home() / ".codex")


def discover_codex_models() -> dict[str, list[str]]:
    """Modelos visibles de Codex y sus niveles de esfuerzo.

    Codex descarga con la cuenta del usuario un catálogo (``models_cache.json``)
    donde cada modelo declara ``supported_reasoning_levels``. Es la fuente más
    fiable de qué niveles admite cada modelo.
    """
    try:
        data = json.loads(
            (codex_home() / "models_cache.json").read_text(encoding="utf-8")
        )
    except (OSError, ValueError):
        return {}
    table: dict[str, list[str]] = {}
    for model in (data.get("models") if isinstance(data, dict) else None) or []:
        if not isinstance(model, dict) or model.get("visibility") != "list":
            continue
        slug = str(model.get("slug") or "").strip()
        levels = [
            str(item.get("effort") if isinstance(item, dict) else item)
            for item in model.get("supported_reasoning_levels") or []
        ]
        if slug and levels:
            table[slug] = levels
    return table


# ---------------------------------------------------------- opencode default
def _strip_jsonc(text: str) -> str:
    """Quita comentarios // y /* */ y comas finales, respetando las cadenas."""
    out: list[str] = []
    i, n, in_string = 0, len(text), False
    while i < n:
        ch = text[i]
        if in_string:
            out.append(ch)
            if ch == "\\" and i + 1 < n:
                out.append(text[i + 1])
                i += 1
            elif ch == '"':
                in_string = False
        elif ch == '"':
            in_string = True
            out.append(ch)
        elif text.startswith("//", i):
            while i < n and text[i] != "\n":
                i += 1
            continue
        elif text.startswith("/*", i):
            end = text.find("*/", i + 2)
            i = n if end < 0 else end + 2
            continue
        else:
            out.append(ch)
        i += 1
    return re.sub(r",(\s*[}\]])", r"\1", "".join(out))


def opencode_default_model() -> str:
    """Modelo por defecto de OpenCode (``proveedor/modelo``) o '' si no se sabe."""
    candidates = [os.environ.get("OPENCODE_CONFIG") or ""]
    config_dir = Path(
        os.environ.get("OPENCODE_CONFIG_DIR")
        or Path.home() / ".config" / "opencode"
    )
    candidates += [str(config_dir / "opencode.jsonc"), str(config_dir / "opencode.json")]
    for candidate in candidates:
        if not candidate:
            continue
        try:
            data = json.loads(
                _strip_jsonc(Path(candidate).read_text(encoding="utf-8"))
            )
        except (OSError, ValueError):
            continue
        model = data.get("model") if isinstance(data, dict) else None
        if isinstance(model, str) and "/" in model:
            return model.split("#", 1)[0]
        if isinstance(model, dict) and model.get("providerID") and model.get("model"):
            return f"{model['providerID']}/{model['model']}"
    return ""
