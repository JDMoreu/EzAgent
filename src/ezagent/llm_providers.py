"""Proveedores LLM: presets y perfiles compatibles con OpenAI o con Anthropic.

La app necesita un ``base_url``, una clave, un modelo y el tipo de API
(``api_style``): ``openai`` (Chat Completions) o ``anthropic`` (Messages). Este
módulo resuelve esos valores desde variables de entorno y, opcionalmente,
desde un ``llm.json`` con perfiles por proveedor. «Personalizado» sirve para
cualquier otro servicio compatible con uno de los dos formatos.
"""

from __future__ import annotations

import json
import os
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Final
from urllib.parse import urlsplit

API_STYLES: Final[tuple[str, ...]] = ("openai", "anthropic")
CUSTOM_PROVIDER: Final[str] = "custom"
LOCAL_HOSTS: Final[frozenset[str]] = frozenset({"127.0.0.1", "localhost", "::1", "0.0.0.0"})


@dataclass(frozen=True, slots=True)
class ProviderSpec:
    name: str
    label: str
    base_url: str
    api_key_env: str
    default_model: str
    api_style: str = "openai"


PROVIDERS: Final[dict[str, ProviderSpec]] = {
    "opencode": ProviderSpec(
        "opencode",
        "OpenCode Go",
        "https://opencode.ai/inference/go/openai/v1",
        "OPENCODE_API_KEY",
        "deepseek-v4.1-flash",
    ),
    "openai": ProviderSpec(
        "openai",
        "OpenAI (GPT, Codex)",
        "https://api.openai.com/v1",
        "OPENAI_API_KEY",
        "gpt-4.1-mini",
    ),
    "anthropic": ProviderSpec(
        "anthropic",
        "Claude (Anthropic)",
        "https://api.anthropic.com/v1",
        "ANTHROPIC_API_KEY",
        "claude-sonnet-5-5",
        api_style="anthropic",
    ),
    "gemini": ProviderSpec(
        "gemini",
        "Gemini",
        "https://generativelanguage.googleapis.com/v1beta/openai/",
        "GEMINI_API_KEY",
        "gemini-2.5-flash",
    ),
    "groq": ProviderSpec(
        "groq",
        "Groq",
        "https://api.groq.com/openai/v1",
        "GROQ_API_KEY",
        "llama-3.3-70b-versatile",
    ),
    "cerebras": ProviderSpec(
        "cerebras",
        "Cerebras",
        "https://api.cerebras.ai/v1",
        "CEREBRAS_API_KEY",
        "gpt-oss-120b",
    ),
    "deepseek": ProviderSpec(
        "deepseek",
        "DeepSeek",
        "https://api.deepseek.com/v1",
        "DEEPSEEK_API_KEY",
        "deepseek-chat",
    ),
    "openrouter": ProviderSpec(
        "openrouter",
        "OpenRouter",
        "https://openrouter.ai/api/v1",
        "OPENROUTER_API_KEY",
        "openai/gpt-4.1-mini",
    ),
    # Cualquier otro servicio: el usuario pone URL, clave, modelo y formato.
    CUSTOM_PROVIDER: ProviderSpec(
        CUSTOM_PROVIDER,
        "Personalizado",
        "",
        "LLM_API_KEY",
        "",
    ),
}


def normalize_api_style(value: str | None) -> str:
    text = str(value or "").strip().lower()
    return text if text in API_STYLES else "openai"


@dataclass(frozen=True, slots=True)
class ModelCapabilities:
    """Lo que un modelo/endpoint acepta; conservador si el nombre no lo aclara."""

    tools: bool = True
    temperature: bool = True
    max_output: int = 32_000
    reasoning: bool = False


# ``r1``, ``reasoner`` y las familias o1/o3/o4 razonan antes de responder y no
# aceptan los mismos parámetros que un modelo de chat corriente.
_REASONING_NAME = re.compile(r"(?:^|[/_.:-])(?:r1|reasoner|o1|o3|o4|thinking)(?:$|[/_.:-])")


def capabilities_for(
    provider: str | None, model: str | None, api_style: str = ""
) -> ModelCapabilities:
    """Capacidades declarativas de un modelo, sin consultar al servidor."""
    # DECISION: el nombre del modelo decide el razonamiento; provider/api_style
    # quedan como contexto para perfiles más finos cuando haya datos medidos.
    name = str(model or "").strip().lower()
    reasoning = bool(_REASONING_NAME.search(name))
    return ModelCapabilities(reasoning=reasoning)


OPENCODE_BASE_URL: Final[str] = "https://opencode.ai/inference/go/openai/v1"
DEFAULT_LLM_MODEL: Final[str] = "deepseek-v4.1-flash"
# Sugerencias de reserva para cuando no se pudo pedir la lista real a la API
# (``fetch_models``). El modelo siempre se puede escribir a mano en el menú.
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
    "cerebras": (
        ("gpt-oss-120b", "GPT-OSS 120B"),
        ("llama-3.3-70b", "Llama 3.3 70B"),
    ),
    "deepseek": (
        ("deepseek-chat", "DeepSeek Chat"),
        ("deepseek-reasoner", "DeepSeek Reasoner · razonamiento"),
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
    base = os.getenv("APPDATA") or os.getenv("XDG_CONFIG_HOME") or str(Path.home() / ".config")
    return Path(base) / "EzAgent" / "llm.json"


def normalize_provider(name: str | None) -> str:
    value = (name or "").strip().lower()
    value = ALIASES.get(value, value)
    return value or "opencode"


def provider_spec(name: str | None) -> ProviderSpec | None:
    return PROVIDERS.get(normalize_provider(name))


def is_local_endpoint(base_url: str) -> bool:
    """True si la URL apunta a esta máquina (Ollama, LM Studio, vLLM…)."""
    try:
        host = urlsplit(str(base_url or "")).hostname
    except ValueError:
        return False
    return (host or "").lower() in LOCAL_HOSTS


def needs_api_key(provider: str | None, base_url: str) -> bool:
    """Solo los proveedores de nube exigen clave; un endpoint local no."""
    spec = provider_spec(provider)
    if spec is None or spec.name == CUSTOM_PROVIDER:
        return False
    return not is_local_endpoint(base_url)


@dataclass(frozen=True, slots=True)
class LLMConfig:
    provider: str
    model: str
    base_url: str
    api_key: str = field(repr=False)
    source: str = "defaults"
    api_style: str = "openai"

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
        if isinstance(raw, dict) and normalize_provider(str(raw_name or "")) == selected:
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

    Cada entrada contiene ``label``, ``base_url``, ``model``, ``api_key`` y
    ``api_style``. Las claves se leen del entorno, pero el menú solo las muestra
    enmascaradas.
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
            "api_style": spec.api_style,
        }
    for raw_name, raw in profiles.items():
        raw_key = str(raw_name or "").strip()
        if not raw_key or not isinstance(raw, dict):
            continue
        name = normalize_provider(raw_key)
        base = catalog.get(name, {})
        spec = provider_spec(name)
        builtin = spec is not None and name != CUSTOM_PROVIDER
        catalog[name] = {
            # Los presets conservan su nombre de fábrica; los perfiles
            # personalizados, el que eligió el usuario.
            "label": str((spec.label if builtin else raw.get("label")) or name.title()),
            "api_style": normalize_api_style(raw.get("api_style") or base.get("api_style")),
            "base_url": str(raw.get("base_url") or base.get("base_url") or ""),
            "model": str(
                raw.get("model")
                or (spec.default_model if spec is not None else "")
                or base.get("model")
                or ""
            ),
            "api_key": str(raw.get("api_key") or base.get("api_key") or ""),
        }
        if raw.get("saved"):
            catalog[name]["provider"] = str(raw.get("provider") or CUSTOM_PROVIDER)
            catalog[name]["saved"] = "true"
    return catalog


def save_provider_profile(
    provider: str,
    *,
    model: str,
    base_url: str,
    api_key: str,
    api_style: str = "openai",
    profile_name: str = "",
    config_file: Path | None = None,
) -> str:
    """Guarda un perfil en ``llm.json`` y lo deja como proveedor activo."""
    name = normalize_provider(provider)
    if not name:
        raise ValueError("El perfil necesita un nombre de proveedor.")
    profile_name = str(profile_name or "").strip()
    if len(profile_name) > 80:
        raise ValueError("El nombre de la configuración admite hasta 80 caracteres.")
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
        "api_style": normalize_api_style(api_style),
    }
    if profile_name:
        # DECISION: named snapshots coexist with provider defaults; saving
        # another model for the same provider must not overwrite older presets.
        providers["saved:" + profile_name.casefold()] = {
            **providers[name],
            "label": profile_name,
            "provider": name,
            "saved": True,
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
    api_style: str | None = None,
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
        spec = ProviderSpec(selected, selected.title(), "", "LLM_API_KEY", "")

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
        base_url or env_base or profile.get("base_url") or data.get("base_url") or spec.base_url
    )
    resolved_model = (
        model or env_model or profile.get("model") or data.get("model") or spec.default_model
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
        raise ValueError(f"El proveedor {selected!r} necesita LLM_BASE_URL o base_url en llm.json.")
    source = (
        "env"
        if any(
            env.get(name) for name in ("LLM_PROVIDER", "LLM_MODEL", "LLM_BASE_URL", "LLM_API_KEY")
        )
        else ("file" if data else "defaults")
    )
    return LLMConfig(
        provider=selected,
        model=str(resolved_model).strip(),
        base_url=str(resolved_base).strip(),
        api_key=str(resolved_key).strip(),
        source=source,
        api_style=normalize_api_style(
            api_style or env.get("LLM_API_STYLE") or profile.get("api_style") or spec.api_style
        ),
    )


# Modelos que no sirven para conversar (audio, imágenes, embeddings…).
_NON_CHAT_HINTS: Final[tuple[str, ...]] = (
    "embed",
    "whisper",
    "tts",
    "dall-e",
    "moderation",
    "transcribe",
    "image",
    "audio",
    "realtime",
    "guard",
    "rerank",
    "search",
    "sora",
    "orpheus",
)


def match_model(model: str, available: list[str] | tuple[str, ...]) -> str:
    """Completa un id sin prefijo con el de la lista real, si es inequívoco.

    Groq y OpenRouter usan ids como ``qwen/qwen3-32b``; escribir solo
    ``qwen3-32b`` da 404. Si hay una única coincidencia, se usa esa.
    """
    wanted = str(model or "").strip()
    if not wanted or not available or wanted in available:
        return wanted
    lowered = wanted.lower()
    exact = [item for item in available if item.lower() == lowered]
    if len(exact) == 1:
        return exact[0]
    suffix = [item for item in available if item.lower().endswith("/" + lowered)]
    return suffix[0] if len(suffix) == 1 else wanted


def models_url(base_url: str, api_style: str) -> str:
    """URL de la lista de modelos (``GET /models``) para un endpoint."""
    base = str(base_url or "").strip().rstrip("/")
    if normalize_api_style(api_style) == "anthropic" and not base.endswith("/v1"):
        return base + "/v1/models"
    return base + "/models"


def fetch_models(
    base_url: str,
    api_key: str,
    api_style: str = "openai",
    *,
    timeout: float = 8.0,
    limit: int = 80,
) -> list[str]:
    """Pide a la API la lista real de modelos. Llamada de red: usar en un hilo.

    Devuelve ids ordenados, sin modelos de audio/imagen/embeddings. Lanza
    ``ValueError`` con un mensaje sin la clave si la petición falla.
    """
    import httpx

    style = normalize_api_style(api_style)
    headers = {"User-Agent": "EzAgent-Voice/0.1"}
    if style == "anthropic":
        headers["x-api-key"] = api_key
        headers["anthropic-version"] = "2023-06-01"
    elif api_key:
        headers["Authorization"] = f"Bearer {api_key}"
    url = models_url(base_url, style)
    params = {"limit": "1000"} if style == "anthropic" else None
    try:
        response = httpx.get(url, headers=headers, params=params, timeout=timeout)
    except httpx.HTTPError as exc:
        raise ValueError(f"No pude pedir los modelos: {type(exc).__name__}") from exc
    if response.status_code >= 400:
        raise ValueError(f"La lista de modelos respondió {response.status_code}.")
    try:
        payload = response.json()
    except ValueError as exc:
        raise ValueError("La lista de modelos no es JSON.") from exc
    items = payload.get("data") if isinstance(payload, dict) else payload
    ids: set[str] = set()
    for item in items or []:
        ident = item.get("id") if isinstance(item, dict) else item
        text = str(ident or "").strip()
        # Gemini (formato OpenAI) devuelve «models/gemini-…».
        text = text.removeprefix("models/")
        if text and not any(hint in text.lower() for hint in _NON_CHAT_HINTS):
            ids.add(text)
    return sorted(ids)[:limit]


__all__ = [
    "ALIASES",
    "API_STYLES",
    "CUSTOM_PROVIDER",
    "LLMConfig",
    "ModelCapabilities",
    "DEFAULT_LLM_MODEL",
    "OPENCODE_BASE_URL",
    "PROVIDERS",
    "capabilities_for",
    "ProviderSpec",
    "config_path",
    "fetch_models",
    "is_local_endpoint",
    "load_provider_catalog",
    "match_model",
    "models_url",
    "needs_api_key",
    "normalize_api_style",
    "MODEL_CATALOG",
    "normalize_provider",
    "provider_models",
    "provider_spec",
    "resolve_llm",
    "save_provider_profile",
]
