"""Fixtures de proveedores LLM (dominio `llm_providers`, F1-03).

Cubre presets, alias, resolución de proveedor/modelo/clave (`resolve_llm`),
el catálogo del menú, el guardado de perfiles y `match_model` sobre todos los
modelos de `MODEL_CATALOG`. Nada de red: `fetch_models` queda fuera del
oráculo porque su contrato es HTTP (se prueba con dobles en Rust).

Los archivos se escriben con rutas relativas dentro de la carpeta temporal del
oráculo, así los mensajes de error no dependen de la máquina.
"""

from __future__ import annotations

import contextlib
import json
import os
from collections.abc import Iterator
from dataclasses import asdict
from pathlib import Path
from typing import Any

from _common import FixtureFile, main_guard

from ezagent.llm_providers import (
    MODEL_CATALOG,
    PROVIDERS,
    capabilities_for,
    config_path,
    is_local_endpoint,
    load_provider_catalog,
    match_model,
    models_url,
    needs_api_key,
    normalize_api_style,
    normalize_provider,
    provider_models,
    provider_spec,
    resolve_llm,
    save_provider_profile,
)

DOMAIN = "llm_providers"

CONFIG_FILE = "llm.json"


def _mark(value: Any, home: Path) -> Any:
    """Sustituye las rutas de la máquina por marcadores estables."""
    if isinstance(value, str):
        text = value
        for prefix in sorted({str(home), str(home.resolve())}, key=len, reverse=True):
            text = text.replace(prefix, "<tmp>")
        return text
    if isinstance(value, list):
        return [_mark(item, home) for item in value]
    if isinstance(value, dict):
        return {key: _mark(item, home) for key, item in value.items()}
    return value


@contextlib.contextmanager
def _env(values: dict[str, str | None]) -> Iterator[None]:
    """Aplica variables de entorno del caso; `None` las borra."""
    saved = {name: os.environ.get(name) for name in values}
    try:
        for name, value in values.items():
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value
        yield
    finally:
        for name, value in saved.items():
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value


def _put_file(file_value: Any) -> None:
    path = Path(CONFIG_FILE)
    path.unlink(missing_ok=True)
    if isinstance(file_value, tuple):
        path.write_text(file_value[1], encoding="utf-8", newline="")
    elif file_value is not None:
        text = json.dumps(file_value, ensure_ascii=False, indent=2) + "\n"
        path.write_text(text, encoding="utf-8")


def _snapshot(config: Any, home: Path) -> dict[str, Any]:
    return _mark({**asdict(config), "label": config.label}, home)


def _cases_basic(out: FixtureFile) -> None:
    for index, value in enumerate(
        (None, "", "  ", "OpenAI", " ANTHROPIC ", "gemini", "openai\n", 123), start=1
    ):
        out.add(
            f"llm_providers/normalize_api_style/{index:04d}",
            "normalize_api_style",
            {"value": value},
            {"style": normalize_api_style(value)},  # type: ignore[arg-type]
        )

    for index, value in enumerate(
        (
            None,
            "",
            "  ",
            "opencode",
            "OpenAI",
            "open-ai",
            "open_ai",
            "google",
            "claude",
            "CLAUDE",
            " mistral ",
            "custom",
        ),
        start=1,
    ):
        out.add(
            f"llm_providers/normalize_provider/{index:04d}",
            "normalize_provider",
            {"name": value},
            {"provider": normalize_provider(value)},
        )


def _cases_presets(out: FixtureFile) -> None:
    for index, name in enumerate((*PROVIDERS, "mistral", "claude", "google", "", None), start=1):
        spec = provider_spec(name)  # type: ignore[arg-type]
        out.add(
            f"llm_providers/provider_spec/{index:04d}",
            "provider_spec",
            {"name": name},
            {"spec": asdict(spec) if spec is not None else None},
        )

    for index, name in enumerate((*PROVIDERS, "unknown", None), start=1):
        models = provider_models(name)
        out.add(
            f"llm_providers/provider_models/{index:04d}",
            "provider_models",
            {"provider": name},
            {"models": [{"id": model, "label": label} for model, label in models]},
        )


def _cases_endpoints(out: FixtureFile) -> None:
    for index, url in enumerate(LOCAL_URLS, start=1):
        out.add(
            f"llm_providers/is_local_endpoint/{index:04d}",
            "is_local_endpoint",
            {"base_url": url},
            {"local": is_local_endpoint(url)},
        )

    for index, (provider, base_url) in enumerate(KEY_CASES, start=1):
        out.add(
            f"llm_providers/needs_api_key/{index:04d}",
            "needs_api_key",
            {"provider": provider, "base_url": base_url},
            {"needed": needs_api_key(provider, base_url)},
        )


# Listas compactas separadas por `|`: hay valores con espacios y uno vacío.
LOCAL_URLS = (
    "http://127.0.0.1:8088/inference|http://localhost:11434/v1|http://LOCALHOST:11434/v1"
    "|http://[::1]:8080/v1|http://0.0.0.0:1234/v1|http://192.168.1.5:8000/v1"
    "|https://api.openai.com/v1||no es una url|http://[oops|file:///tmp/x"
).split("|")

KEY_CASES: tuple[tuple[str | None, str], ...] = (
    ("opencode", "https://opencode.ai/inference/go/openai/v1"),
    ("openai", "http://127.0.0.1:8080/v1"),
    ("custom", "https://mi.servidor/v1"),
    ("mistral", "https://api.mistral.ai/v1"),
    (None, "https://api.openai.com/v1"),
    ("groq", "http://localhost:9000"),
)

CAPABILITY_MODELS = (
    "deepseek-chat|deepseek-reasoner|deepseek-r1-distill-llama-70b|deepseek_r1"
    "|o1|o1-mini|o1preview|o3|o4-mini|gpt-4o1|my-r1-model|r1x|thinking-model"
    "|claude-sonnet-5-5|"
).split("|")


def _cases_capabilities(out: FixtureFile) -> None:
    models: tuple[Any, ...] = (*CAPABILITY_MODELS, None)
    for index, model in enumerate(models, start=1):
        caps = capabilities_for("openai", model, "openai")
        out.add(
            f"llm_providers/capabilities_for/{index:04d}",
            "capabilities_for",
            {"provider": "openai", "model": model, "api_style": "openai"},
            {"capabilities": asdict(caps)},
        )


def _match_cases() -> list[tuple[str, list[str]]]:
    """Un caso por modelo de cada preset, más los bordes de resolución."""
    cases: list[tuple[str, list[str]]] = []
    for entries in MODEL_CATALOG.values():
        available = [model for model, _label in entries]
        for model, _label in entries:
            short = model.split("/")[-1]
            cases.append((model, available))
            cases.append((short, available))
            cases.append((short.upper(), available))
        cases.append(("", available))
        cases.append((available[0], []))
        cases.append((available[0], [available[0]]))
    cases.extend(
        [
            ("qwen3-32b", ["qwen/qwen3-32b"]),
            ("QWEN3-32B", ["qwen/qwen3-32b"]),
            ("qwen3-32b", ["a/qwen3-32b", "b/qwen3-32b"]),
            ("x", ["x", "X"]),
            ("qwen3", ["qwen/qwen3-32b"]),
            ("  modelo  ", ["modelo"]),
            ("", ["a"]),
            ("a/b/c", ["z/a/b/c"]),
            ("b/c", ["a/b/c"]),
            (None, ["a"]),  # type: ignore[list-item]
        ]
    )
    return cases


def _cases_matching(out: FixtureFile) -> None:
    for index, (model, available) in enumerate(_match_cases(), start=1):
        out.add(
            f"llm_providers/match_model/{index:04d}",
            "match_model",
            {"model": model, "available": available},
            {"model": match_model(model, available)},
        )

    url_cases: tuple[tuple[str, str], ...] = (
        ("https://api.openai.com/v1", "openai"),
        ("https://api.openai.com/v1/", "openai"),
        ("https://api.anthropic.com/v1", "anthropic"),
        ("https://api.anthropic.com", "anthropic"),
        ("https://api.anthropic.com/", "anthropic"),
        ("https://api.anthropic.com/v1/", "anthropic"),
        ("", "openai"),
        ("http://127.0.0.1:8080/v1", "ANTHROPIC"),
        ("http://127.0.0.1:8080/v1", "otro"),
    )
    for index, (base_url, style) in enumerate(url_cases, start=1):
        out.add(
            f"llm_providers/models_url/{index:04d}",
            "models_url",
            {"base_url": base_url, "api_style": style},
            {"url": models_url(base_url, style)},
        )


RESOLVE_CASES: list[tuple[dict[str, str], Any, dict[str, str]]] = [
    ({}, None, {}),
    ({"LLM_PROVIDER": "openai", "OPENAI_API_KEY": "clave-falsa"}, None, {}),
    (
        {
            "LLM_PROVIDER": "groq",
            "LLM_MODEL": "mi-modelo",
            "LLM_BASE_URL": "https://proxy.local/v1",
            "LLM_API_KEY": "clave-falsa",
            "LLM_API_STYLE": "anthropic",
        },
        None,
        {},
    ),
    (
        {
            "LLM_BASE_URL": "https://opencode.ai/inference/go/openai/v1",
            "LLM_MODEL": "deepseek-v4.1-flash",
        },
        None,
        {"provider": "groq"},
    ),
    (
        {
            "LLM_BASE_URL": "https://opencode.ai/inference/go/openai/v1",
            "LLM_MODEL": "deepseek-v4.1-flash",
        },
        None,
        {"provider": "opencode"},
    ),
    (
        {},
        {
            "provider": "anthropic",
            "providers": {
                "claude": {
                    "base_url": "https://proxy.local/v1",
                    "model": "claude-x",
                    "api_key": "clave-falsa",
                }
            },
        },
        {},
    ),
    (
        {},
        {
            "provider": "groq",
            "model": "m1",
            "base_url": "https://g.local/v1",
            "api_key": "clave-falsa",
        },
        {},
    ),
    (
        {
            "LLM_PROVIDER": "openai",
            "LLM_MODEL": "env-model",
            "LLM_BASE_URL": "https://env.local/v1",
            "LLM_API_KEY": "clave-env",
        },
        {"provider": "groq", "providers": {"groq": {"model": "file-model"}}},
        {
            "provider": "anthropic",
            "model": "arg-model",
            "base_url": "https://arg.local/v1",
            "api_key": "clave-arg",
            "api_style": "anthropic",
        },
    ),
    ({}, None, {"provider": "mistral"}),
    ({}, None, {"provider": "mistral", "base_url": "https://api.mistral.ai/v1"}),
    (
        {},
        None,
        {
            "provider": "custom",
            "base_url": "http://127.0.0.1:1234/v1",
            "model": "local",
        },
    ),
    ({}, None, {"provider": "opencode", "api_style": "anthropic"}),
    ({}, None, {"provider": "opencode", "api_style": "raro"}),
    ({"LLM_API_STYLE": "anthropic"}, None, {"provider": "opencode"}),
    ({}, ("raw", "{ no json"), {}),
    ({}, [1, 2], {}),
    (
        {},
        {
            "providers": {
                "custom": {
                    "label": "Mío",
                    "base_url": "https://x.local/v1",
                    "model": "mx",
                    "api_style": "anthropic",
                }
            }
        },
        {"provider": "custom"},
    ),
    ({}, {"provider": "OPENCODE"}, {}),
    ({"LLM_PROVIDER": "  "}, None, {}),
]

CATALOG_FILE: dict[str, Any] = {
    "provider": "openai",
    "providers": {
        "openai": {
            "base_url": "https://proxy.local/v1",
            "model": "gpt-x",
            "api_style": "anthropic",
        },
        "custom": {
            "label": "Mi servidor",
            "base_url": "http://127.0.0.1:1234/v1",
            "model": "local",
            "api_style": "raro",
        },
        "saved:trabajo": {
            "label": "Trabajo",
            "provider": "custom",
            "saved": True,
            "base_url": "https://t.local/v1",
            "model": "t1",
            "api_key": "clave-falsa",
        },
        "": {"label": "vacío"},
        "roto": "no soy dict",
    },
}

CATALOG_CASES: list[tuple[dict[str, str], Any]] = [
    ({}, None),
    ({"OPENCODE_API_KEY": "clave-falsa", "OPENAI_API_KEY": "otra-falsa"}, CATALOG_FILE),
    ({}, {"provider": "groq"}),
]

SAVE_CASES: list[tuple[Any, dict[str, Any]]] = [
    (
        None,
        {
            "provider": "custom",
            "model": "local",
            "base_url": "http://127.0.0.1:1234/v1",
            "api_key": "clave-falsa",
        },
    ),
    (
        None,
        {
            "provider": "openai",
            "model": "gpt-4.1-mini",
            "base_url": "https://api.openai.com/v1",
            "api_key": "clave-falsa",
            "api_style": "openai",
        },
    ),
    (
        None,
        {
            "provider": "Custom",
            "model": "m",
            "base_url": "https://x.local/v1",
            "api_key": "",
            "api_style": "anthropic",
            "profile_name": "Trabajo",
        },
    ),
    (
        {"providers": {"custom": {"label": "Mi endpoint"}}},
        {
            "provider": "custom",
            "model": "otro",
            "base_url": "https://otro.local/v1",
            "api_key": "",
        },
    ),
    (
        {
            "provider": "groq",
            "providers": {
                "groq": {
                    "label": "Groq",
                    "model": "old",
                    "base_url": "https://g.local/v1",
                    "api_key": "",
                }
            },
        },
        {
            "provider": "opencode",
            "model": "nuevo",
            "base_url": "https://o.local/v1",
            "api_key": "",
            "profile_name": "Casa",
        },
    ),
    (
        None,
        {
            "provider": "openai",
            "model": "m",
            "base_url": "https://x.local/v1",
            "api_key": "",
            "profile_name": "x" * 81,
        },
    ),
    (
        ("raw", "{ no json"),
        {
            "provider": "openai",
            "model": "m",
            "base_url": "https://x.local/v1",
            "api_key": "",
        },
    ),
]

CONFIG_PATH_CASES: list[dict[str, str | None]] = [
    {"EZ_LLM_CONFIG": None, "APPDATA": "<tmp>", "XDG_CONFIG_HOME": None},
    {"EZ_LLM_CONFIG": "~/llm-custom.json", "APPDATA": "<tmp>", "XDG_CONFIG_HOME": None},
    {"EZ_LLM_CONFIG": "cfg/llm.json", "APPDATA": "<tmp>", "XDG_CONFIG_HOME": None},
    {"EZ_LLM_CONFIG": None, "APPDATA": None, "XDG_CONFIG_HOME": "<tmp>/xdg"},
    {"EZ_LLM_CONFIG": None, "APPDATA": None, "XDG_CONFIG_HOME": None},
]


def _cases_resolution(out: FixtureFile, home: Path) -> None:
    for index, (environ, file_value, kwargs) in enumerate(RESOLVE_CASES, start=1):
        case = f"llm_providers/resolve_llm/{index:04d}"
        _put_file(file_value)
        payload = {
            "kwargs": {"config_file": CONFIG_FILE, **kwargs},
            "environ": environ,
            "file": None if file_value is None else file_value,
        }
        try:
            config = resolve_llm(
                environ=dict(environ),
                config_file=Path(CONFIG_FILE),
                **kwargs,  # type: ignore[arg-type]
            )
        except ValueError as error:
            out.add_error(case, "resolve_llm", payload, _mark(str(error), home))
            continue
        out.add(case, "resolve_llm", payload, _snapshot(config, home))

    # Con EZ_LLM_CONFIG apuntando a un archivo inexistente, es un error.
    env = {"EZ_LLM_CONFIG": str(home / "nope" / "llm.json")}
    try:
        with _env(env):
            resolve_llm(environ={})
    except ValueError as error:
        out.add_error(
            f"llm_providers/resolve_llm/{len(RESOLVE_CASES) + 1:04d}",
            "resolve_llm",
            {"kwargs": {}, "environ": {}, "file": None, "env_file": "<tmp>/nope/llm.json"},
            _mark(str(error), home),
        )


def _cases_catalog(out: FixtureFile, home: Path) -> None:
    for index, (environ, file_value) in enumerate(CATALOG_CASES, start=1):
        _put_file(file_value)
        payload = {"environ": environ, "config_file": CONFIG_FILE, "file": file_value}
        catalog = load_provider_catalog(environ=dict(environ), config_file=Path(CONFIG_FILE))
        out.add(
            f"llm_providers/load_provider_catalog/{index:04d}",
            "load_provider_catalog",
            payload,
            {"catalog": _mark(catalog, home)},
        )


def _cases_save(out: FixtureFile, home: Path) -> None:
    for index, (file_value, kwargs) in enumerate(SAVE_CASES, start=1):
        case = f"llm_providers/save_provider_profile/{index:04d}"
        _put_file(file_value)
        payload = {"kwargs": {**kwargs, "config_file": CONFIG_FILE}, "file": file_value}
        try:
            name = save_provider_profile(config_file=Path(CONFIG_FILE), **kwargs)
        except ValueError as error:
            out.add_error(case, "save_provider_profile", payload, _mark(str(error), home))
            continue
        text = Path(CONFIG_FILE).read_bytes().decode("utf-8")
        out.add(
            case,
            "save_provider_profile",
            payload,
            {
                "name": name,
                "text": _mark(text, home),
                "newline": "crlf" if chr(13) in text else "lf",
                "data": json.loads(text),
            },
        )


def _cases_config_path(out: FixtureFile, home: Path) -> None:
    for index, desired in enumerate(CONFIG_PATH_CASES, start=1):
        applied = {
            name: (value.replace("<tmp>", str(home)) if isinstance(value, str) else value)
            for name, value in desired.items()
        }
        with _env(applied):
            path = config_path()
        out.add(
            f"llm_providers/config_path/{index:04d}",
            "config_path",
            {"env": desired},
            {"path": _mark(str(path), home)},
        )


def generate(out: FixtureFile) -> None:
    home = Path(os.environ["HOME"])
    original = os.getcwd()
    os.chdir(home)
    try:
        with _env({"EZ_LLM_CONFIG": None}):
            _cases_basic(out)
            _cases_presets(out)
            _cases_endpoints(out)
            _cases_capabilities(out)
            _cases_matching(out)
            _cases_resolution(out, home)
            _cases_catalog(out, home)
            _cases_save(out, home)
            _cases_config_path(out, home)
    finally:
        os.chdir(original)
        Path(CONFIG_FILE).unlink(missing_ok=True)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
