"""Proveedores LLM compatibles con la API de chat de OpenAI.

OpenAI, Groq, Gemini y Claude exponen (o aceptan) el formato OpenAI; OpenCode
también. La app solo necesita un ``base_url``, una clave y un modelo. Este
módulo resuelve esos tres valores desde variables de entorno y, opcionalmente,
desde un ``llm.json`` con perfiles por proveedor.
"""

from __future__ import annotations

import json
import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Final


@dataclass(frozen=True, slots=True)
class ProviderSpec:
    name: str
    label: str
    base_url: str
    api_key_env: str
    default_model: str


PROVIDERS: Final[dict[str, ProviderSpec]] = {
    "opencode": ProviderSpec(
        "opencode", "OpenCode",
        "https://opencode.ai/inference/go/openai/v1",
        "OPENCODE_API_KEY", "deepseek-v4.1-flash",
    ),
    "openai": ProviderSpec(
        "openai", "OpenAI",
        "https://api.openai.com/v1",
        "OPENAI_API_KEY", "gpt-4.1-mini",
    ),
    "groq": ProviderSpec(
        "groq", "Groq",
        "https://api.groq.com/openai/v1",
        "GROQ_API_KEY", "llama-3.3-70b-versatile",
    ),
    "anthropic": ProviderSpec(
        "anthropic", "Claude",
        "https://api.anthropic.com/v1/",
        "ANTHROPIC_API_KEY", "claude-sonnet-5-5",
    ),
    "gemini": ProviderSpec(
        "gemini", "Gemini",
        "https://generativelanguage.googleapis.com/v1beta/openai/",
        "GEMINI_API_KEY", "gemini-2.5-flash",
    ),
    "openrouter": ProviderSpec(
        "openrouter", "OpenRouter",
        "https://openrouter.ai/api/v1",
        "OPENROUTER_API_KEY", "openai/gpt-4.1-mini",
    ),
}

OPENCODE_BASE_URL: Final[str] = "https://opencode.ai/inference/go/openai/v1"
DEFAULT_LLM_MODEL: Final[str] = "deepseek-v4.1-flash"
# Catálogo breve para las filas de selección del notch. No pretende ser
# exhaustivo: el modelo actual y los perfiles guardados siempre se añaden
# delante/después para no limitar una configuración existente.
MODEL_CATALOG: Final[dict[str, tuple[tuple[str, str], ...]]] = {
    "opencode": (
        ("deepseek-v4.1-flash", "DeepSeek V4.1 Flash · rápido"),
        ("deepseek-v4.1", "DeepSeek V4.1"),
    ),
    "openai": (
        ("gpt-4.1-mini", "GPT-4.1 mini"),
        ("gpt-4.1", "GPT-4.1"),
        ("gpt-4o-mini", "GPT-4o mini"),
        ("gpt-4o", "GPT-4o"),
        ("o4-mini", "o4-mini · razonamiento"),
    ),
    "groq": (
        ("llama-3.3-70b-versatile", "Llama 3.3 70B · versátil"),
        ("llama-3.1-8b-instant", "Llama 3.1 8B · instantáneo"),
        ("deepseek-r1-distill-llama-70b", "DeepSeek R1 Distill 70B"),
    ),
    "anthropic": (
        ("claude-sonnet-5-5", "Claude Sonnet 5.5"),
        ("claude-opus-5-5", "Claude Opus 5.5"),
        ("claude-haiku-5-5", "Claude Haiku 5.5 · rápido"),
    ),
    "gemini": (
        ("gemini-2.5-flash", "Gemini 2.5 Flash"),
        ("gemini-2.5-pro", "Gemini 2.5 Pro"),
        ("gemini-2.0-flash", "Gemini 2.0 Flash"),
    ),
    "openrouter": (
        ("openai/gpt-4.1-mini", "GPT-4.1 mini · OpenRouter"),
        ("anthropic/claude-sonnet-5-5", "Claude Sonnet 5.5 · OpenRouter"),
        ("google/gemini-2.5-flash", "Gemini 2.5 Flash · OpenRouter"),
        ("meta-llama/llama-3.3-70b-instruct", "Llama 3.3 70B · OpenRouter"),
    ),
}


def provider_models(provider: str | None) -> tuple[tuple[str, str], ...]:
    """Modelos sugeridos para un proveedor (vacío para uno personalizado)."""
    return MODEL_CATALOG.get(normalize_provider(provider), ())



ALIASES: Final[dict[str, str]] = {
    "claude": "anthropic",
    "google": "gemini",
    "open-ai": "openai",
    "open_ai": "openai",
}


def config_path() -> Path:
    """Ruta del archivo opcional de proveedores.

    ``EZ_LLM_CONFIG`` manda; si no, se usa la carpeta de configuración de la
    app (``%APPDATA%/EzAgent/llm.json`` en Windows). No se versiona.
    """
    explicit = os.getenv("EZ_LLM_CONFIG")
    if explicit:
        return Path(explicit).expanduser()
    base = os.getenv("APPDATA") or os.getenv("XDG_CONFIG_HOME") or str(
        Path.home() / ".config"
    )
    return Path(base) / "EzAgent" / "llm.json"


def normalize_provider(name: str | None) -> str:
    value = (name or "").strip().lower()
    value = ALIASES.get(value, value)
    return value or "opencode"


def provider_spec(name: str | None) -> ProviderSpec | None:
    return PROVIDERS.get(normalize_provider(name))


@dataclass(frozen=True, slots=True)
class LLMConfig:
    provider: str
    model: str
    base_url: str
    api_key: str = field(repr=False)
    source: str = "defaults"

    @property
    def label(self) -> str:
        spec = provider_spec(self.provider)
        return spec.label if spec else self.provider


def _profile_for(profiles: dict, selected: str) -> dict:
    """Busca un perfil por id canónico, tolerando alias como ``claude``."""
    direct = profiles.get(selected)
    if isinstance(direct, dict):
        return direct
    for raw_name, raw in profiles.items():
        if (
            isinstance(raw, dict)
            and normalize_provider(str(raw_name or "")) == selected
        ):
            return raw
    return {}


def _load_file(path: Path) -> dict:
    if not path.exists():
        if os.getenv("EZ_LLM_CONFIG"):
            raise ValueError(f"No existe EZ_LLM_CONFIG: {path}")
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except Exception as exc:  # noqa: BLE001
        raise ValueError(f"llm.json inválido en {path}: {exc}") from exc
    return data if isinstance(data, dict) else {}


def load_provider_catalog(
    *,
    environ: dict[str, str] | None = None,
    config_file: Path | None = None,
) -> dict[str, dict[str, str]]:
    """Catálogo para el menú: integrados + perfiles guardados.

    Cada entrada contiene ``label``, ``base_url``, ``model`` y ``api_key``.
    Las claves se leen del entorno, pero el menú solo las muestra enmascaradas.
    """
    env = os.environ if environ is None else environ
    data = _load_file(config_file or config_path())
    profiles = data.get("providers") if isinstance(data.get("providers"), dict) else {}
    catalog: dict[str, dict[str, str]] = {}
    for name, spec in PROVIDERS.items():
        catalog[name] = {
            "label": spec.label,
            "base_url": spec.base_url,
            "model": spec.default_model,
            "api_key": env.get(spec.api_key_env) or "",
        }
    for raw_name, raw in profiles.items():
        raw_key = str(raw_name or "").strip()
        if not raw_key or not isinstance(raw, dict):
            continue
        name = normalize_provider(raw_key)
        base = catalog.get(name, {})
        spec = provider_spec(name)
        catalog[name] = {
            "label": str(
                raw.get("label")
                or (spec.label if spec is not None else name.title())
                or name
            ),
            "base_url": str(raw.get("base_url") or base.get("base_url") or ""),
            "model": str(
                raw.get("model")
                or (spec.default_model if spec is not None else "")
                or base.get("model")
                or ""
            ),
            "api_key": str(raw.get("api_key") or base.get("api_key") or ""),
        }
    return catalog


def save_provider_profile(
    provider: str,
    *,
    model: str,
    base_url: str,
    api_key: str,
    config_file: Path | None = None,
) -> str:
    """Guarda un perfil en ``llm.json`` y lo deja como proveedor activo."""
    name = normalize_provider(provider)
    if not name:
        raise ValueError("El perfil necesita un nombre de proveedor.")
    path = config_file or config_path()
    data: dict = {}
    if path.exists():
        try:
            loaded = json.loads(path.read_text(encoding="utf-8"))
            if isinstance(loaded, dict):
                data = loaded
        except Exception as exc:  # noqa: BLE001
            raise ValueError(f"llm.json inválido en {path}: {exc}") from exc
    providers = data.get("providers")
    if not isinstance(providers, dict):
        providers = {}
    previous = providers.get(name)
    label = (
        str(previous.get("label"))
        if isinstance(previous, dict) and previous.get("label")
        else (provider_spec(name).label if provider_spec(name) else name.title())
    )
    providers[name] = {
        "label": label,
        "base_url": str(base_url or "").strip(),
        "model": str(model or "").strip(),
        "api_key": str(api_key or "").strip(),
    }
    data["providers"] = providers
    data["provider"] = name
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + ".tmp")
    try:
        tmp.write_text(
            json.dumps(data, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        os.replace(tmp, path)
    finally:
        try:
            if tmp.exists():
                tmp.unlink()
        except Exception:
            pass
    return name


def resolve_llm(
    *,
    provider: str | None = None,
    model: str | None = None,
    base_url: str | None = None,
    api_key: str | None = None,
    environ: dict[str, str] | None = None,
    config_file: Path | None = None,
) -> LLMConfig:
    """Resuelve proveedor, modelo, endpoint y clave.

    Prioridad: argumentos explícitos > variables de entorno > perfil del
    archivo > valores del proveedor integrado. ``api_key`` nunca se registra.
    """
    env = os.environ if environ is None else environ
    file_path = config_file or config_path()
    data = _load_file(file_path)
    profiles = data.get("providers") if isinstance(data.get("providers"), dict) else {}

    selected = normalize_provider(
        provider or env.get("LLM_PROVIDER") or data.get("provider") or "opencode"
    )
    spec = provider_spec(selected)
    profile = _profile_for(profiles, selected)
    # Un proveedor desconocido se trata como endpoint compatible personalizado.
    if spec is None:
        spec = ProviderSpec(
            selected, selected.title(), "", "LLM_API_KEY", ""
        )

    env_base = env.get("LLM_BASE_URL")
    env_model = env.get("LLM_MODEL")
    if selected != "opencode":
        # Al cambiar de proveedor, los valores de fábrica de OpenCode que
        # quedaron en .env no deben apuntar al endpoint equivocado.
        if env_base and env_base.rstrip("/") == "https://opencode.ai/inference/go/openai/v1":
            env_base = None
        if env_model == "deepseek-v4.1-flash":
            env_model = None
    resolved_base = (
        base_url
        or env_base
        or profile.get("base_url")
        or data.get("base_url")
        or spec.base_url
    )
    resolved_model = (
        model
        or env_model
        or profile.get("model")
        or data.get("model")
        or spec.default_model
    )
    resolved_key = (
        api_key
        or env.get("LLM_API_KEY")
        or profile.get("api_key")
        or data.get("api_key")
        or env.get(spec.api_key_env)
        or ""
    )
    if not str(resolved_base).strip():
        raise ValueError(
            f"El proveedor {selected!r} necesita LLM_BASE_URL o base_url en llm.json."
        )
    source = (
        "env" if any(
            env.get(name)
            for name in ("LLM_PROVIDER", "LLM_MODEL", "LLM_BASE_URL", "LLM_API_KEY")
        ) else ("file" if data else "defaults")
    )
    return LLMConfig(
        provider=selected,
        model=str(resolved_model).strip(),
        base_url=str(resolved_base).strip(),
        api_key=str(resolved_key).strip(),
        source=source,
    )


__all__ = [
    "ALIASES",
    "LLMConfig",
    "DEFAULT_LLM_MODEL",
    "OPENCODE_BASE_URL",
    "PROVIDERS",
    "ProviderSpec",
    "config_path",
    "load_provider_catalog",
    "MODEL_CATALOG",
    "normalize_provider",
    "provider_models",
    "provider_spec",
    "resolve_llm",
    "save_provider_profile",
]
