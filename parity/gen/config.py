"""Fixtures de configuración de voz (dominio `config`, F1-03).

Cubre `VoiceConfig` completo (valores de fábrica, carga con migraciones,
guardado atómico con merge de claves, `save_key`) y los ayudantes de nombres y
perfiles de agente. Los archivos se escriben en la carpeta temporal del
oráculo con nombres relativos: el texto guardado es el contrato, no la ruta.
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

from ezagent.voice.config import (
    DEFAULT_AGENTS,
    VoiceConfig,
    config_path,
    merge_agent_profiles,
    normalize_agent_name,
    resolve_agent_key,
)

DOMAIN = "config"

VOICE_FILE = "voice.json"
SECOND_FILE = "voice2.json"
# `apply_env` rellena `default_cwd` con el cwd real; el caso usa un valor fijo
# para que el texto guardado no dependa de la máquina.
WORKSPACE = "<workspace>"

# Variables que `apply_env` y `resolve_llm` leerían del entorno real.
_LLM_ENV = (
    "LLM_PROVIDER",
    "LLM_MODEL",
    "LLM_BASE_URL",
    "LLM_API_KEY",
    "LLM_API_STYLE",
    "OPENCODE_API_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GEMINI_API_KEY",
    "GROQ_API_KEY",
    "CEREBRAS_API_KEY",
    "DEEPSEEK_API_KEY",
    "OPENROUTER_API_KEY",
    "WHISPER_URL",
    "WHISPER_ENGINE",
    "EZ_OUTPUT_DEVICE_INDEX",
    "EZ_LLM_CONFIG",
)


@contextlib.contextmanager
def _env(values: dict[str, str | None]) -> Iterator[None]:
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


def _norm(value: str, home: Path) -> str:
    prefixes = sorted({str(home), str(home.resolve())}, key=len, reverse=True)
    text = value
    for prefix in prefixes:
        text = text.replace(prefix, "<tmp>")
    return text


def _mark(value: Any, home: Path) -> Any:
    if isinstance(value, str):
        return _norm(value, home)
    if isinstance(value, list):
        return [_mark(item, home) for item in value]
    if isinstance(value, dict):
        return {key: _mark(item, home) for key, item in value.items()}
    return value


def _snapshot(config: VoiceConfig, home: Path) -> dict[str, Any]:
    return _mark(asdict(config), home)


def _put(name: str, value: Any) -> Path:
    Path(name).unlink(missing_ok=True)
    if isinstance(value, tuple):
        Path(name).write_text(value[1], encoding="utf-8", newline="")
    elif value is not None:
        Path(name).write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding="utf-8")
    return Path(name)


def _file_snapshot(name: str) -> dict[str, Any]:
    raw = Path(name).read_bytes().decode("utf-8")
    return {"text": raw, "newline": "crlf" if "\r\n" in raw else "lf"}


def _config(**overrides: Any) -> VoiceConfig:
    return VoiceConfig(default_cwd=WORKSPACE, **overrides)


LOAD_CASES: list[tuple[Any, dict[str, str | None]]] = [
    (None, {}),
    ({}, {}),
    (
        {
            "voice": "ef_dora",
            "speed": 1.1,
            "hotkey": "alt+q",
            "version_futura": 3,
            "unknown_future_key": 1,
        },
        {},
    ),
    ({"notch_hidden": True}, {}),
    ({"hud_hidden": False, "notch_hidden": True}, {}),
    (("raw", "{ no es json"), {}),
    ([1, 2], {}),
    ({"vad_defaults_version": 1, "min_rms": 0.0015, "min_speech_secs": 0.3}, {}),
    ({"vad_defaults_version": 2, "min_rms": 0.0015}, {}),
    ({"voice": "em_alex", "min_rms": 0.0015}, {}),
    ({"defaults_version": 1, "stt_timeout_secs": 60.0}, {}),
    ({"defaults_version": 2, "stt_timeout_secs": 60.0}, {}),
    (
        {
            "agent_profiles": {
                "mcode": {"label": "Mi mcode", "default_model": "minimax/x"},
                "nuevo": {"label": "Nuevo"},
            }
        },
        {},
    ),
    (
        {
            "agent_profiles_version": 1,
            "agent_profiles": {
                "opencode": {
                    "label": "viejo",
                    "models": ["x"],
                    "default_model": "foo",
                    "command": ["opencode", "--beta"],
                }
            },
        },
        {},
    ),
    ({"agents": {"claude": ["claude", "--foo"]}}, {}),
    ({"speed": "rápido", "stt_beam_size": "cinco"}, {}),
    ({"api_key": "clave-de-prueba", "llm_api_style": "anthropic"}, {}),
    ({"llm_pinned": True, "llm_provider": "opencode", "llm_model": "glm-5.3"}, {}),
    (
        {"llm_pinned": True, "llm_provider": "desconocido", "llm_base_url": ""},
        {},
    ),
    (
        {
            "llm_pinned": True,
            "llm_provider": "openai",
            "llm_model": "gpt-x",
            "llm_base_url": "https://proxy.local/v1",
            "api_key": "clave-falsa",
            "llm_api_style": "openai",
        },
        {},
    ),
    (
        {
            "whisper_url": "http://127.0.0.1:9000/inference",
            "output_device_index": 3,
            "default_cwd": "C:/otro/sitio",
        },
        {
            "WHISPER_URL": "http://127.0.0.1:9999/inference",
            "WHISPER_ENGINE": "faster",
            "EZ_OUTPUT_DEVICE_INDEX": "7",
        },
    ),
    (
        {"output_device_index": 3},
        {"EZ_OUTPUT_DEVICE_INDEX": "no-es-numero"},
    ),
]

SAVE_CASES: list[tuple[Any, dict[str, Any]]] = [
    ({}, {"voice": "ef_dora", "speed": 1.05, "theme": "dark", "stt_beam_size": 1}),
    (
        {
            "futuro": 42,
            "notch_hidden": True,
            "api_key": "clave-vieja",
            "speed": 2.0,
        },
        {"speed": 0.9, "hud_hidden": True},
    ),
    (None, {"wake_phrase": "EzAgent, escúchame 🔊", "theme_base": "light"}),
]

SAVE_KEY_CASES: list[tuple[Any, str, Any]] = [
    (None, "hud_hidden", True),
    ({"voice": "ef_dora", "menú_abierto": True, "theme": "auto"}, "theme", "dark"),
    ({"voice": "ef_dora"}, "custom_key", 42),
]

NAME_CASES: tuple[str, ...] = (
    "OpenCode",
    "open code",
    "Claude Code",
    "códex",
    "  Qwen  ",
    "",
    "minimax-m3.1",
    "ÑANDÚ",
    "open_code!",
)

KEY_CASES: list[tuple[str, dict[str, list[str]] | None, dict[str, dict[str, object]] | None]] = [
    ("open code", None, None),
    ("Claude", None, None),
    ("minimax", None, None),
    ("m code", None, None),
    ("claude code", None, None),
    ("desconocido", None, None),
    ("", None, None),
    ("mi agente", {"my": ["my"]}, {"my": {"aliases": ["mi agente"]}}),
    ("otro", {"my": ["my"]}, {"otro": {"aliases": ["otro"]}}),
    ("my", {"my": ["my"]}, {}),
]

MERGE_CASES: list[tuple[dict[str, dict[str, object]] | None, dict[str, object] | None]] = [
    (None, None),
    (None, {"claude": {"label": "X"}, "nuevo": {"label": "N"}, "": {}, "malo": "x"}),
    ({"a": {"k": 1}, "b": {"k": 2}}, {"a": {"k": 2}, "c": {"k": 3}}),
    ({"a": {"k": 1}}, None),
]

PATH_CASES: list[dict[str, str | None]] = [
    {"APPDATA": "<tmp>", "XDG_CONFIG_HOME": None},
    {"APPDATA": None, "XDG_CONFIG_HOME": "<tmp>/xdg"},
    {"APPDATA": None, "XDG_CONFIG_HOME": None},
]


def _cases_load(out: FixtureFile, home: Path) -> None:
    for index, (file_value, environ) in enumerate(LOAD_CASES, start=1):
        case = f"config/VoiceConfig.load/{index:04d}"
        _put(VOICE_FILE, file_value)
        payload = {"path": VOICE_FILE, "file": file_value, "env": environ}
        try:
            with _env(environ):
                config = VoiceConfig.load(Path(VOICE_FILE))
        except Exception as error:  # el oráculo es el error real de Python
            out.add_error(case, "VoiceConfig.load", payload, _norm(str(error), home))
            continue
        out.add(case, "VoiceConfig.load", payload, _snapshot(config, home))


def _cases_save(out: FixtureFile, home: Path) -> None:
    for index, (file_value, overrides) in enumerate(SAVE_CASES, start=1):
        case = f"config/VoiceConfig.save/{index:04d}"
        _put(VOICE_FILE, file_value)
        config = _config(llm_api_style="openai", **overrides)
        config.save(Path(VOICE_FILE))
        snapshot = _file_snapshot(VOICE_FILE)
        out.add(
            case,
            "VoiceConfig.save",
            {"path": VOICE_FILE, "file": file_value, "config": asdict(config)},
            {
                "text": _norm(snapshot["text"], home),
                "newline": snapshot["newline"],
                "data": json.loads(snapshot["text"]),
                "tmp_left": Path(VOICE_FILE + ".tmp").exists(),
            },
        )


def _cases_save_round_trip(out: FixtureFile, home: Path) -> None:
    # Round-trip: guardar, cargar y volver a guardar produce el mismo texto.
    case = f"config/VoiceConfig.save/{len(SAVE_CASES) + 1:04d}"
    _put(VOICE_FILE, None)
    Path(SECOND_FILE).unlink(missing_ok=True)
    config = _config(
        llm_api_style="openai",
        voice="ef_dora",
        speed=1.05,
        wake_phrase="EzAgent, escúchame 🔊",
        show_advanced=True,
        chat_width=420.0,
        drop_dock="left",
    )
    config.save(Path(VOICE_FILE))
    first = Path(VOICE_FILE).read_bytes().decode("utf-8")
    reloaded = VoiceConfig.load(Path(VOICE_FILE))
    reloaded.save(Path(SECOND_FILE))
    second = Path(SECOND_FILE).read_bytes().decode("utf-8")
    out.add(
        case,
        "VoiceConfig.save",
        {"path": VOICE_FILE, "file": None, "config": _mark(asdict(config), home)},
        {
            "text": _norm(first, home),
            "newline": "crlf" if "\r\n" in first else "lf",
            "round_trip_text": first == second,
            "round_trip_values": asdict(config) == asdict(reloaded),
            "reloaded": _snapshot(reloaded, home),
        },
    )

    # Un destino nuevo en una carpeta que no existe se crea sin dejar el .tmp.
    case = f"config/VoiceConfig.save/{len(SAVE_CASES) + 2:04d}"
    nested = "sub/dir/voice.json"
    Path(nested).unlink(missing_ok=True)
    config = _config(llm_api_style="openai")
    config.save(Path(nested))
    snapshot = _file_snapshot(nested)
    out.add(
        case,
        "VoiceConfig.save",
        {"path": nested, "file": None, "config": asdict(config)},
        {
            "text": _norm(snapshot["text"], home),
            "newline": snapshot["newline"],
            "tmp_left": Path(nested + ".tmp").exists(),
            "exists": Path(nested).exists(),
        },
    )


def _cases_save_key(out: FixtureFile, home: Path) -> None:
    for index, (file_value, key, value) in enumerate(SAVE_KEY_CASES, start=1):
        _put(VOICE_FILE, file_value)
        config = _config()
        config.save_key(key, value, Path(VOICE_FILE))
        snapshot = _file_snapshot(VOICE_FILE)
        out.add(
            f"config/VoiceConfig.save_key/{index:04d}",
            "VoiceConfig.save_key",
            {"path": VOICE_FILE, "file": file_value, "key": key, "value": value},
            {
                "text": _norm(snapshot["text"], home),
                "newline": snapshot["newline"],
                "data": json.loads(snapshot["text"]),
            },
        )


def _cases_names(out: FixtureFile) -> None:
    for index, value in enumerate(NAME_CASES, start=1):
        out.add(
            f"config/normalize_agent_name/{index:04d}",
            "normalize_agent_name",
            {"value": value},
            {"name": normalize_agent_name(value)},
        )

    for index, (name, agents, profiles) in enumerate(KEY_CASES, start=1):
        payload: dict[str, Any] = {"name": name}
        if agents is not None:
            payload["agents"] = agents
        if profiles is not None:
            payload["profiles"] = profiles
        out.add(
            f"config/resolve_agent_key/{index:04d}",
            "resolve_agent_key",
            payload,
            {
                "key": resolve_agent_key(  # type: ignore[arg-type]
                    name,
                    dict(DEFAULT_AGENTS) if agents is None else agents,
                    profiles,
                )
            },
        )

    for index, (base, override) in enumerate(MERGE_CASES, start=1):
        out.add(
            f"config/merge_agent_profiles/{index:04d}",
            "merge_agent_profiles",
            {"base": base, "override": override},
            {"profiles": merge_agent_profiles(base, override)},  # type: ignore[arg-type]
        )

    for index, (value, default) in enumerate(
        ((8, 1), ("9", 1), (None, 1), ("x", 1), (True, 1), (9.7, 1)), start=1
    ):
        out.add(
            f"config/VoiceConfig._int/{index:04d}",
            "VoiceConfig._int",
            {"value": value, "default": default},
            {"int": VoiceConfig._int(value, default)},
        )


def _cases_path(out: FixtureFile, home: Path) -> None:
    for index, desired in enumerate(PATH_CASES, start=1):
        applied = {
            name: (value.replace("<tmp>", str(home)) if isinstance(value, str) else value)
            for name, value in desired.items()
        }
        with _env(applied):
            path = config_path()
        out.add(
            f"config/config_path/{index:04d}",
            "config_path",
            {"env": desired},
            {"path": _norm(str(path), home)},
        )


def generate(out: FixtureFile) -> None:
    home = Path(os.environ["HOME"])
    original = os.getcwd()
    os.chdir(home)
    cleared = {name: None for name in _LLM_ENV}
    try:
        with _env(cleared):
            out.add(
                "config/VoiceConfig/0001",
                "VoiceConfig",
                {},
                {"values": _snapshot(VoiceConfig(), home)},
            )
            _cases_load(out, home)
            _cases_save(out, home)
            _cases_save_round_trip(out, home)
            _cases_save_key(out, home)
            _cases_names(out)
            _cases_path(out, home)
    finally:
        os.chdir(original)
        for name in (VOICE_FILE, VOICE_FILE + ".tmp", SECOND_FILE, "sub/dir/voice.json"):
            Path(name).unlink(missing_ok=True)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
