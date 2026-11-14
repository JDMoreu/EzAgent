"""Golden cases for the HUD settings model (domain `hud_settings`, F1-10b).

Every case comes from the real Python model in ``ezagent.voice.hud.settings``:
values, normalisation, validation, formatted texts, selectors and the saved
copy (``dirty``/``mark_saved``/``restore_saved``). The settings window and the
hotkey field are painted into ``parity/golden/hud/settings/*.png`` by the
``hud_menu`` generator; this domain only covers the model.
"""

from __future__ import annotations

from typing import Any

from _common import FixtureFile, main_guard

from ezagent.voice.hud import hotkey_field as hk
from ezagent.voice.hud import settings_window as sw
from ezagent.voice.hud.settings import (
    ALL_KEYS,
    COLOR_KEYS,
    DEFAULT_VALUES,
    EDITABLE_KEYS,
    FIXED_OPTIONS,
    INSTANT_KEYS,
    LABELS,
    LLM_REASONING_OPTIONS,
    METER_DB_CEIL,
    METER_DB_FLOOR,
    SECRET_KEYS,
    SELECT_KEYS,
    SLIDER_KEYS,
    SLIDERS,
    STT_ENGINE_OPTIONS,
    TEXT_KEYS,
    TOGGLE_KEYS,
    TRACKED_KEYS,
    TTS_VARIANTS,
    UI_KEYS,
    VOICE_IDS,
    VOICE_LABELS,
    MenuSettings,
    SelectOption,
    db_to_rms,
    mask_secret,
    normalize_setting,
)

DOMAIN = "hud_settings"

DEVICES = [
    {"index": 0, "name": "Micrófono (Realtek® Audio)", "rate": 48000, "channels": 1},
    {"index": 1, "name": "Auriculares USB", "rate": 44100, "channels": 2},
]
PROVIDERS = {
    "opencode": {
        "label": "OpenCode",
        "base_url": "https://opencode.ai/inference/go/openai/v1",
        "model": "deepseek-v4.1-flash",
        "api_style": "openai",
    },
    "local": {
        "label": "Ollama local",
        "base_url": "http://127.0.0.1:11434/v1",
        "model": "llama3.2",
        "api_style": "openai",
    },
    "guardado": {
        "label": "Mi perfil",
        "base_url": "https://api.example.test/v1",
        "model": "gpt-4.1-mini",
        "api_key": "sk-or-v1-1234567890abcdef",
        "saved": "1",
        "api_style": "openai",
    },
}
STT_MODELS = {
    "ggml-large-v3-turbo-q8_0": "large-v3 turbo",
    "ggml-base": "base",
    "ggml-small": "small",
}
LLM_MODELS = {
    "opencode": [
        ("deepseek-v4.1-flash", "DeepSeek V4.1 Flash"),
        ("gpt-4.1-mini", "GPT-4.1 mini"),
        ("claude-sonnet-5-5", "Claude Sonnet 5.5"),
        ("llama-3.3-70b-versatile", "Llama 3.3 70B"),
        ("gpt-oss-120b", "GPT OSS 120B"),
        ("qwen3-coder-480b", "Qwen3 Coder 480B"),
        ("minimax-m2", "MiniMax M2"),
        ("deepseek-r1", "DeepSeek R1"),
    ]
}
WORLD: dict[str, Any] = {
    "devices": DEVICES,
    "voices": None,
    "providers": PROVIDERS,
    "stt_models": STT_MODELS,
    "llm_models": LLM_MODELS,
}
# DECISION: el mundo (micrófonos, voces, perfiles y modelos) viaja en cada caso
# para que el .jsonl sea autocontenido; el duplicado con hud_menu.py es
# deliberado (el brief no permite un módulo común nuevo).


class Namespace:
    """Un objeto con atributos, para la rama ``getattr`` de ``from_app``/``_read``."""

    def __init__(self, data: dict[str, Any]) -> None:
        self.__dict__.update(data)


def world_of(data: Any) -> dict[str, Any]:
    """Mundo del caso: el de por defecto con las claves que traiga ``data``."""
    merged = dict(WORLD)
    if isinstance(data, dict):
        merged.update(data)
    return merged


def ready(value: Any) -> Any:
    """Valor serializable con el vocabulario de los fixtures."""
    if isinstance(value, SelectOption):
        return {"value": value.value, "label": value.label}
    if isinstance(value, dict):
        return {str(key): ready(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [ready(item) for item in value]
    return value


def build(values: dict[str, Any] | None = None, world: Any = None) -> MenuSettings:
    data = world_of(world)
    return MenuSettings(
        values or {},
        data["devices"],
        data["voices"],
        data["providers"],
        data["stt_models"],
        data["llm_models"],
    )


def settings_input(
    values: dict[str, Any] | None = None,
    world: Any = None,
    **extra: Any,
) -> dict[str, Any]:
    data: dict[str, Any] = {"values": values or {}, "world": world_of(world)}
    data.update(extra)
    return data


def _add(out: FixtureFile, case: str, fn: str, data: Any, value: Any) -> None:
    out.add(f"hud_settings/{case}", fn, data, {"value": ready(value)})


def _raw(out: FixtureFile, case: str, fn: str, data: Any, output: dict[str, Any]) -> None:
    out.add(f"hud_settings/{case}", fn, data, {key: ready(value) for key, value in output.items()})


def _add_text(out: FixtureFile, case: str, data: Any, text: str) -> None:
    out.add(f"hud_settings/{case}", "MenuSettings.format", data, {"text": text})


# ------------------------------------------------------------------ defaults
def defaults(out: FixtureFile) -> None:
    settings = build()
    for index, key in enumerate(ALL_KEYS):
        _raw(out, f"defaults/{index:03d}-{key}", "MenuSettings.format", {"key": key},
             {"value": settings.get(key), "text": settings.format(key)})
    out.add("hud_settings/defaults/keys", "MenuSettings.snapshot", {},
            {"tracked": list(TRACKED_KEYS), "editable": list(EDITABLE_KEYS),
             "instant": list(INSTANT_KEYS), "ui": list(UI_KEYS), "all": list(ALL_KEYS)})
    out.add("hud_settings/tables", "settings", {},
            {"slider_keys": list(SLIDER_KEYS), "select_keys": list(SELECT_KEYS),
             "toggle_keys": list(TOGGLE_KEYS), "text_keys": list(TEXT_KEYS),
             "color_keys": list(COLOR_KEYS), "secret_keys": list(SECRET_KEYS),
             "sliders": {key: {"minimum": spec.minimum, "maximum": spec.maximum,
                               "step": spec.step, "decimals": spec.decimals,
                               "suffix": spec.suffix, "log": spec.log}
                         for key, spec in SLIDERS.items()},
             "defaults": dict(DEFAULT_VALUES),
             "voice_ids": list(VOICE_IDS), "voice_labels": dict(VOICE_LABELS)})
    out.add("hud_settings/defaults/snapshot", "MenuSettings.snapshot", {},
            {"value": ready(settings.snapshot())})
    out.add("hud_settings/defaults/saved", "MenuSettings.saved_snapshot", {},
            {"value": ready(settings.saved_snapshot())})
    out.add("hud_settings/defaults/dirty", "MenuSettings.dirty", {}, {"value": False})
    out.add("hud_settings/defaults/labels", "settings.LABELS", {},
            {"value": dict(LABELS)})


# ---------------------------------------------------------- normalizar valores
def _slider_values(key: str) -> list[tuple[str, Any]]:
    spec = SLIDERS[key]
    middle = (spec.minimum + spec.maximum) / 2.0
    return [
        ("min", spec.minimum),
        ("max", spec.maximum),
        ("mid", middle),
        ("below", spec.minimum - 10.0 * spec.step),
        ("above", spec.maximum + 10.0 * spec.step),
        ("none", None),
        ("empty", ""),
        ("text", "nada"),
        ("comma", "1,5"),
        ("numeric", str(middle)),
        ("bool", True),
        ("nan", "nan"),
        ("inf", "inf"),
        ("neg", -1),
    ]


def _toggle_values() -> list[tuple[str, Any]]:
    return [
        ("none", None), ("empty", ""), ("zero", "0"), ("false", "false"),
        ("no", "no"), ("off", "off"), ("true", " true "), ("yes", "sí"),
        ("one", "1"), ("int0", 0), ("int1", 1), ("bool", False),
        ("list", [1]), ("float", 2.5),
    ]


def _text_values() -> list[tuple[str, Any]]:
    return [
        ("none", None), ("empty", ""), ("spaces", "   "),
        ("padded", "  hola  "), ("unicode", "áéíóú ✨ 中文"),
        ("newline", "uno\ndos"), ("int", 5), ("bool", True),
    ]


def _enum_values(options: tuple[SelectOption, ...]) -> list[tuple[str, Any]]:
    values: list[tuple[str, Any]] = [("none", None), ("empty", ""), ("invalid", "otro")]
    values += [(f"ok-{option.value}", option.value) for option in options]
    values += [(f"upper-{option.value}", str(option.value).upper()) for option in options]
    return values


def _normalize_table(key: str) -> list[tuple[str, Any]]:
    if key in SLIDERS:
        return _slider_values(key)
    if key in TOGGLE_KEYS:
        return _toggle_values()
    if key == "input_device_index":
        return [("none", None), ("empty", ""), ("zero", 0), ("three", 3), ("neg", -1),
                ("text", "x"), ("numeric", "2"), ("float", 2.0), ("bool", True)]
    if key == "voice":
        return [("none", None), ("empty", ""), ("known", "em_dora"), ("unknown", "nueva"),
                ("padded", "  em_alex  "), ("unicode", "voz-ñ")]
    if key == "tts_model_variant":
        return _enum_values(TTS_VARIANTS)
    if key == "stt_engine":
        return _enum_values(STT_ENGINE_OPTIONS)
    if key == "llm_reasoning_effort":
        return _enum_values(LLM_REASONING_OPTIONS)
    if key in FIXED_OPTIONS:
        return _enum_values(FIXED_OPTIONS[key])
    if key == "wake_phrase":
        return [("none", None), ("empty", ""), ("spaces", "   "),
                ("padded", "  EzAgent, escúchame  "), ("emoji", "Ey ✨")]
    if key == "llm_profile_name":
        return [("none", None), ("empty", ""), ("padded", "  Mi perfil  "),
                ("exactly80", "a" * 80), ("over81", "b" * 81)]
    if key in ("llm_profile", "llm_saved_profile", "llm_model_pick", "theme_preset"):
        return [("none", None), ("empty", ""), ("padded", "  valor  "), ("value", "x")]
    if key in COLOR_KEYS:
        return [("valid", "#1FAE9F"), ("lower", "1fae9f"), ("short", "abc"),
                ("short-hash", "#ABC"), ("bad", "#GGGGGG"), ("empty", ""),
                ("spaces", "   "), ("long", "#1234567"), ("none", None)]
    if key in ("stt_model", "llm_model"):
        return [("none", None), ("empty", ""), ("value", "modelo-x"), ("int", 5)]
    if key == "stt_dictionary":
        return [("empty", ""), ("arrow", "open code → OpenCode"),
                ("ascii", "open code -> OpenCode; cloud => Claude"),
                ("lines", "uno→1\ndos→2"), ("trim", "  origen   largo  →  destino  "),
                ("malformed", "sin flecha"), ("duplicate", "a→b; a→c"),
                ("too-many", "; ".join(f"a{i}→b{i}" for i in range(101))),
                ("too-long", "x→y; " + "a" * 12000)]
    if key in TEXT_KEYS:
        return _text_values()
    return _text_values()


def normalize(out: FixtureFile) -> None:
    for key in ALL_KEYS:
        for tag, value in _normalize_table(key):
            case = f"normalize/{key}/{tag}"
            try:
                result = normalize_setting(key, value)
            except Exception as error:  # texto exacto de CPython
                out.add_error(f"hud_settings/{case}", "normalize_setting",
                              {"key": key, "value": ready(value)}, str(error))
                continue
            _add(out, case, "normalize_setting", {"key": key, "value": ready(value)}, result)
    out.add_error("hud_settings/normalize/unknown-key", "normalize_setting",
                  {"key": "no_existe", "value": 1}, "'no_existe'")


# --------------------------------------------------------------------- set()
def _set_cases() -> list[tuple[str, dict[str, Any], str, Any, list[str]]]:
    return [
        ("speed", {}, "speed", 1.13, ["speed"]),
        ("speed-low", {}, "speed", 0.1, ["speed"]),
        ("speed-int", {}, "speed", 1, ["speed"]),
        ("speed-none", {}, "speed", None, ["speed"]),
        ("smart-turn", {}, "smart_turn", "0", ["smart_turn"]),
        ("device", {}, "input_device_index", "1", ["input_device_index"]),
        ("device-neg", {}, "input_device_index", -3, ["input_device_index"]),
        ("device-empty", {}, "input_device_index", "", ["input_device_index"]),
        ("voice-known", {}, "voice", "em_santa", ["voice"]),
        ("voice-new", {}, "voice", "voz-nueva", ["voice"]),
        ("dictionary", {}, "stt_dictionary", "open code → OpenCode", ["stt_dictionary"]),
        ("dictionary-normal", {}, "stt_dictionary", "a => b; c→d", ["stt_dictionary"]),
        ("color", {}, "theme_primary", "#112233", ["theme_primary", "theme"]),
        ("color-bad", {"theme": "light"}, "theme_primary", "#ZZZZZZ", ["theme_primary", "theme"]),
        ("color-clean", {}, "theme_accent", "  abc  ", ["theme_accent", "theme"]),
        ("preset", {}, "theme_preset", "lavanda",
         ["theme_preset", "theme", "theme_primary", "theme_secondary", "theme_accent",
          "theme_background"]),
        ("preset-unknown", {}, "theme_preset", "nada", ["theme_preset", "theme"]),
        ("profile-name", {}, "llm_profile_name", "  Mi perfil  ",
         ["llm_profile_name", "llm_saved_profile"]),
        ("profile-name-empty", {"llm_profile_name": "Algo"}, "llm_profile_name", "",
         ["llm_profile_name", "llm_saved_profile"]),
        ("profile-open", {}, "llm_profile", "local",
         ["llm_profile", "llm_provider", "llm_model", "llm_base_url", "llm_api_style",
          "api_key", "llm_profile_name", "llm_saved_profile"]),
        ("profile-saved", {}, "llm_saved_profile", "guardado",
         ["llm_saved_profile", "llm_profile", "llm_provider", "llm_model", "llm_base_url",
          "llm_api_style", "api_key", "llm_profile_name"]),
        ("profile-missing", {}, "llm_saved_profile", "no-existe",
         ["llm_saved_profile", "llm_provider", "llm_model"]),
        ("model-pick", {}, "llm_model_pick", "gpt-4.1-mini", ["llm_model_pick", "llm_model"]),
        ("model-pick-empty", {}, "llm_model_pick", "", ["llm_model_pick", "llm_model"]),
        ("theme-upper", {}, "theme", "DARK", ["theme"]),
        ("theme-invalid", {}, "theme", "fucsia", ["theme"]),
        ("wake", {}, "wake_phrase", "  Hola  ", ["wake_phrase"]),
        ("tts", {}, "tts_model_variant", "fp32", ["tts_model_variant"]),
        ("stt-engine", {}, "stt_engine", "cpu", ["stt_engine"]),
        ("reasoning", {}, "llm_reasoning_effort", "high", ["llm_reasoning_effort"]),
    ]


def _set_errors() -> list[tuple[str, dict[str, Any], str, Any, str]]:
    return [
        ("profile-name-81", {}, "llm_profile_name", "b" * 81,
         "El nombre de la configuración admite hasta 80 caracteres."),
        ("unknown", {}, "no_existe", 1, "'no_existe'"),
        ("dictionary-bad", {}, "stt_dictionary", "sin flecha",
         "Usa origen → corrección; separa las reglas con punto y coma."),
        ("dictionary-dupe", {}, "stt_dictionary", "a→b; a→c",
         "Cada expresión de origen debe aparecer una sola vez."),
        ("dictionary-long", {}, "stt_dictionary", "x→y; " + "a" * 12000,
         "El diccionario admite hasta 12000 caracteres."),
        ("dictionary-many", {}, "stt_dictionary", "; ".join(f"a{i}→b{i}" for i in range(101)),
         "El diccionario admite hasta 100 correcciones."),
    ]


def setters(out: FixtureFile) -> None:
    for tag, values, key, value, watch in _set_cases():
        settings = build(values)
        before = settings.dirty()
        try:
            result = settings.set(key, value)
        except Exception as error:
            out.add_error(f"hud_settings/set/{tag}", "MenuSettings.set",
                          settings_input(values, key=key, value=ready(value)), str(error))
            continue
        _raw(out, f"set/{tag}", "MenuSettings.set",
             settings_input(values, key=key, value=ready(value), watch=watch),
             {"value": result, "dirty": settings.dirty(), "was_dirty": before,
              "watch": {name: settings.get(name) for name in watch},
              "format": settings.format(key)})
    for tag, values, key, value, message in _set_errors():
        try:
            build(values).set(key, value)
        except Exception as error:
            out.add_error(f"hud_settings/set/error-{tag}", "MenuSettings.set",
                          settings_input(values, key=key, value=ready(value)), str(error))
            assert str(error), message
        else:
            raise AssertionError(f"set/{tag} debía fallar")


# ------------------------------------------------------------------ format()
def _format_cases() -> list[tuple[str, dict[str, Any], str, Any, Any]]:
    cases: list[tuple[str, dict[str, Any], str, Any, Any]] = []
    for index, key in enumerate(ALL_KEYS):
        cases.append((f"default/{index:03d}-{key}", {}, key, "__missing__", None))
    for key in tuple(SLIDERS):
        spec = SLIDERS[key]
        for tag, value in (("min", spec.minimum), ("max", spec.maximum),
                           ("mid", (spec.minimum + spec.maximum) / 2.0),
                           ("over", spec.maximum + spec.step)):
            cases.append((f"slider/{key}/{tag}", {}, key, value, None))
    cases += [
        ("device/no-devices", {}, "input_device_index", None, {"devices": []}),
        ("device/default", {}, "input_device_index", None, None),
        ("device/zero", {}, "input_device_index", 0, None),
        ("device/one", {}, "input_device_index", 1, None),
        ("device/unknown", {}, "input_device_index", 8, None),
        ("device/text", {}, "input_device_index", "1", None),
        ("device/bad", {}, "input_device_index", "x", None),
        ("voice/known", {}, "voice", "em_dora", None),
        ("voice/new", {}, "voice", "voz-x", None),
        ("voice/none", {}, "voice", None, None),
        ("secret/empty", {}, "api_key", "", None),
        ("secret/short", {}, "api_key", "corta", None),
        ("secret/eight", {}, "api_key", "12345678", None),
        ("secret/long", {}, "api_key", "sk-or-v1-1234567890abcdef", None),
        ("secret/unicode", {}, "api_key", "clave-ñ-12345678", None),
        ("stt/known", {}, "stt_model", "ggml-base", None),
        ("stt/unknown", {}, "stt_model", "otro", None),
        ("stt/none", {}, "stt_model", None, None),
        ("llm/none", {}, "llm_model", None, None),
        ("llm/value", {}, "llm_model", "modelo-x", None),
        ("pick/list", {}, "llm_model_pick", "", None),
        ("pick/empty", {}, "llm_model_pick", "", {"llm_models": {}}),
        ("pick/one", {}, "llm_model_pick", "", {"llm_models": {"opencode": [("a", "A")]}}),
        ("preset/known", {}, "theme_preset", "bosque", None),
        ("preset/unknown", {}, "theme_preset", "nada", None),
        ("preset/empty", {}, "theme_preset", "", None),
        ("provider/known", {}, "llm_provider", "opencode", None),
        ("provider/unknown", {}, "llm_provider", "otro", None),
        ("provider/empty", {}, "llm_provider", "", None),
        ("profile/known", {}, "llm_profile", "local", None),
        ("profile/empty", {}, "llm_profile", "", None),
        ("saved/known", {}, "llm_saved_profile", "guardado", None),
        ("saved/none", {}, "llm_saved_profile", "", None),
        ("saved/named", {"llm_profile_name": "Nuevo"}, "llm_saved_profile", "", None),
        ("toggle/true", {}, "smart_turn", "sí", None),
        ("toggle/false", {}, "barge_in", "0", None),
        ("text/empty", {}, "llm_model", "", None),
        ("text/defined", {}, "llm_model", "m", None),
        ("text/padded", {}, "llm_provider", "  x  ", None),
        ("dict/empty", {}, "stt_dictionary", "", None),
        ("dict/value", {}, "stt_dictionary", "a → b", None),
        ("color/valid", {}, "theme_primary", "#123456", None),
        ("meter/min-rms", {}, "min_rms", 0.0001, None),
    ]
    return cases


def formats(out: FixtureFile) -> None:
    for tag, values, key, value, world in _format_cases():
        settings = build(values, world)
        data = settings_input(values, world, key=key)
        if value == "__missing__":
            text = settings.format(key)
        else:
            data["value"] = ready(value)
            text = settings.format(key, value)
        _add_text(out, f"format/{tag}", data, text)
    try:
        build().format("no_existe")
    except Exception as error:
        out.add_error("hud_settings/format/unknown-key", "MenuSettings.format",
                      {"key": "no_existe"}, str(error))


# --------------------------------------------------- guardado y copia dirty
def saved(out: FixtureFile) -> None:
    settings = build()
    out.add("hud_settings/saved/clean", "MenuSettings.dirty", {}, {"value": settings.dirty()})
    settings.set("speed", 1.1)
    out.add("hud_settings/saved/changed", "MenuSettings.dirty", {}, {"value": settings.dirty()})
    settings.set("speed", 0.95)
    out.add("hud_settings/saved/back", "MenuSettings.dirty", {}, {"value": settings.dirty()})
    settings.set("speed", 0.9504)
    out.add("hud_settings/saved/epsilon", "MenuSettings.dirty", {},
            {"value": settings.dirty()})
    settings.mark_saved()
    out.add("hud_settings/saved/marked", "MenuSettings.dirty", {}, {"value": settings.dirty()})
    settings.set("smart_turn", False)
    settings.mark_saved({"smart_turn": "no", "speed": "1.05", "input_device_index": "1"})
    out.add("hud_settings/saved/mark-normalizes", "MenuSettings.mark_saved",
            {"values": {"smart_turn": "no", "speed": "1.05", "input_device_index": "1"}},
            {"dirty": settings.dirty(), "saved": ready(settings.saved_snapshot()),
             "current": ready(settings.snapshot())})
    settings.restore_saved({"speed": 0.95})
    out.add("hud_settings/saved/restore", "MenuSettings.restore_saved",
            {"values": {"speed": 0.95}},
            {"dirty": settings.dirty(), "current": ready(settings.snapshot()),
             "saved": ready(settings.saved_snapshot())})
    settings.set("input_device_index", None)
    out.add("hud_settings/saved/device-none", "MenuSettings.dirty", {},
            {"value": settings.dirty()})


# ------------------------------------------------- dispositivos, voces, nombres
def catalogs(out: FixtureFile) -> None:
    device_variants: list[tuple[str, Any]] = [
        ("none", None), ("empty", []), ("dupes", [
            {"index": 0, "name": "Uno"}, {"index": 0, "name": "Otro"},
            {"index": "2", "name": "Dos", "rate": "48000", "channels": "2"},
        ]), ("invalid", [
            {"index": "x", "name": "Malo"}, {"name": "Sin índice"},
            {"index": -1, "name": "Negativo", "rate": None, "channels": None},
        ]), ("objects", "__objects__"),
    ]
    for tag, devices in device_variants:
        if devices == "__objects__":
            devices = [Namespace({"index": 3, "name": "Objeto"})]
            world_input: Any = {"__objects__": [{"index": 3, "name": "Objeto"}]}
        else:
            world_input = devices
        settings = build(world={"devices": devices})
        out.add(f"hud_settings/devices/{tag}", "MenuSettings.devices",
                {"world": {"devices": world_input}},
                {"value": ready(settings.devices),
                 "options": ready(settings.select_options("input_device_index")),
                 "enabled": settings.selector_enabled("input_device_index"),
                 "text": settings.format("input_device_index"),
                 "index-text": settings.format("input_device_index", 0)})
    voice_variants: list[tuple[str, Any]] = [
        ("none", None), ("list", ["em_alex", "ef_dora"]),
        ("mapping", {"em_alex": "Álex", "otra": "Otra"}),
        ("dicts", [{"id": "em_alex", "label": "Álex"}, {"voice": "ef_dora"},
                   {"id": "", "label": "Vacía"}, ("pareja", "Pareja")]),
        ("dupes", ["em_alex", "em_alex", "", "ef_dora"]),
    ]
    for tag, voices in voice_variants:
        settings = build(world={"voices": voices})
        out.add(f"hud_settings/voices/{tag}", "MenuSettings.voices",
                {"world": {"voices": voices}},
                {"value": ready(settings.voices),
                 "options": ready(settings.select_options("voice")),
                 "enabled": settings.selector_enabled("voice"),
                 "label": settings.format("voice"),
                 "new-label": settings.format("voice", "voz-nueva")})
    settings = build({"voice": "voz-nueva"})
    out.add("hud_settings/voices/current-new", "MenuSettings.voices", {"values": {"voice": "voz-nueva"}},
            {"value": ready(settings.voices), "options": ready(settings.select_options("voice"))})
    provider_variants: list[tuple[str, Any]] = [
        ("none", None), ("empty", {}), ("mapping", {"uno": {"label": "Uno"}}),
        ("list", [{"name": "dos", "label": "Dos", "api_style": "anthropic"},
                  {"provider": "tres"}, ("cuatro", {"label": "Cuatro"})]),
        ("saved", {"guardado": {"label": "Guardado", "saved": True,
                               "api_style": "raro", "model": "", "base_url": ""}}),
    ]
    for tag, providers in provider_variants:
        settings = build(world={"providers": providers})
        out.add(f"hud_settings/providers/{tag}", "MenuSettings.providers",
                {"world": {"providers": providers}},
                {"value": ready(settings.providers),
                 "profiles": ready(settings.select_options("llm_profile")),
                 "saved": ready(settings.select_options("llm_saved_profile")),
                 "profile-enabled": settings.selector_enabled("llm_profile"),
                 "saved-enabled": settings.selector_enabled("llm_saved_profile")})
    name_variants: list[tuple[str, Any]] = [
        ("none", None), ("mapping", {"a": "A", "b": "B"}),
        ("list", ["uno", SelectOption("dos", "Dos"), {"value": "tres", "label": "Tres"},
                  {"id": "cuatro"}, {"model": "cinco"}, ("seis", "Seis")]),
        ("dupes", ["a", "a", ""]),
    ]
    for tag, models in name_variants:
        settings = build(world={"stt_models": models})
        out.add(f"hud_settings/stt-models/{tag}", "MenuSettings.select_options",
                {"key": "stt_model", "world": {"stt_models": ready(models)}},
                {"value": ready(settings.select_options("stt_model")),
                 "enabled": settings.selector_enabled("stt_model"),
                 "label": settings.format("stt_model", "dos")})
    settings = build({"stt_model": "modelo-actual"}, world={"stt_models": {"a": "A"}})
    out.add("hud_settings/stt-models/current", "MenuSettings.select_options",
            {"key": "stt_model", "values": {"stt_model": "modelo-actual"}},
            {"value": ready(settings.select_options("stt_model"))})


# --------------------------------------------------------------- selectores
def selectors(out: FixtureFile) -> None:
    settings = build()
    for key in SELECT_KEYS:
        out.add(f"hud_settings/select/{key}", "MenuSettings.select_options", {"key": key},
                {"value": ready(settings.select_options(key)),
                 "enabled": settings.selector_enabled(key),
                 "text": settings.format(key)})
    empty = build(world={"devices": [], "voices": [], "providers": {},
                         "stt_models": {}, "llm_models": {}})
    for key in SELECT_KEYS:
        out.add(f"hud_settings/select-empty/{key}", "MenuSettings.select_options", {"key": key},
                {"value": ready(empty.select_options(key)),
                 "enabled": empty.selector_enabled(key),
                 "text": empty.format(key)})
    for key in SELECT_KEYS:
        try:
            settings.select_options("no_existe")
        except Exception as error:
            out.add_error("hud_settings/select/error", "MenuSettings.select_options",
                          {"key": "no_existe"}, str(error))
            break
    settings = build()
    settings.set_llm_models("opencode", [("nuevo", "Nuevo"), ("otro", "Otro")])
    out.add("hud_settings/select/models", "MenuSettings.set_llm_models",
            {"provider": "opencode", "models": [["nuevo", "Nuevo"], ["otro", "Otro"]]},
            {"value": ready(settings.select_options("llm_model_pick")),
             "text": settings.format("llm_model_pick"),
             "current": settings.get("llm_model")})
    settings.set_llm_models("local", ["modelo-local"])
    out.add("hud_settings/select/models-other", "MenuSettings.set_llm_models",
            {"provider": "local", "models": ["modelo-local"]},
            {"value": ready(settings.select_options("llm_model_pick")),
             "all": ready(settings.select_options("llm_model_pick"))})


# ------------------------------------------------------------------- casos sueltos
def window_cases(out: FixtureFile) -> None:
    # DECISION: `settings_window` y `hotkey_field` son lógica pura de los
    # módulos del brief; sus casos viven en `hud_settings` porque no son menú.
    """`settings_window`: geometría pura de la ventana y la barra lateral."""
    for index, (w, h) in enumerate(((760.0, 620.0), (560.0, 400.0), (1000.0, 800.0),
                                    (0.0, 0.0), (-10.0, 50.0))):
        out.add(f"hud_settings/window/content/{index:02d}", "settings_window.content_size",
                {"win_w": w, "win_h": h},
                {"value": list(sw.content_size(w, h))})
        out.add(f"hud_settings/window/size/{index:02d}", "settings_window.window_size",
                {"viewport_h": h}, {"value": list(sw.window_size(h))})
    work = (0, 0, 1920, 1080)
    for index, (w, h, area, scale) in enumerate(((760.0, 620.0, work, 1.0),
                                                (3000.0, 2000.0, work, 1.0),
                                                (300.0, 200.0, work, 1.0),
                                                (760.0, 620.0, (0, 0, 1280, 720), 1.25),
                                                (560.0, 400.0, (0, 0, 500, 350), 1.0))):
        out.add(f"hud_settings/window/clamp-size/{index:02d}", "settings_window.clamp_size",
                {"w": w, "h": h, "work": list(area), "scale": scale},
                {"value": list(sw.clamp_size(w, h, area, scale))})
    for index, (area, w, h) in enumerate(((work, 760, 620), (work, 2000, 1500),
                                          ((100, 50, 2000, 1200), 760, 620),
                                          ((0, 0, 300, 200), 760, 620))):
        out.add(f"hud_settings/window/centered/{index:02d}", "settings_window.centered",
                {"work": list(area), "w": w, "h": h},
                {"value": list(sw.centered(area, w, h))})
    for index, (x, y, w, h, scale) in enumerate(((960, 540, 760, 620, 1.0),
                                                 (-500, -400, 760, 620, 1.0),
                                                 (2500, 1500, 760, 620, 1.0),
                                                 (960, 540, 760, 620, 1.5),
                                                 (0, 0, 760, 620, 1.0))):
        out.add(f"hud_settings/window/clamp-pos/{index:02d}", "settings_window.clamp_position",
                {"x": x, "y": y, "w": w, "h": h, "work": list(work), "scale": scale},
                {"value": list(sw.clamp_position(x, y, w, h, work, scale))})
    points = [(0.0, 0.0), (7.0, 7.0), (20.0, 20.0), (380.0, 310.0), (753.0, 613.0),
              (760.0, 620.0), (-2.0, 300.0), (300.0, -2.0), (300.0, 700.0),
              (9.0, 300.0), (750.0, 300.0), (300.0, 3.0), (300.0, 616.0),
              (185.0, 64.0), (100.0, 300.0)]
    for index, (x, y) in enumerate(points):
        edges = sw.resize_edges(x, y, 760.0, 620.0)
        out.add(f"hud_settings/window/edges/{index:02d}", "settings_window.resize_edges",
                {"x": x, "y": y, "w": 760.0, "h": 620.0},
                {"value": None if edges is None else list(edges),
                 "cursor": sw.cursor_for(edges)})
    for index in range(16):
        edges = tuple(bool(index & (1 << bit)) for bit in range(4))
        out.add(f"hud_settings/window/cursor/{index:02d}", "settings_window.cursor_for",
                {"edges": list(edges)}, {"value": sw.cursor_for(edges)})
    out.add("hud_settings/window/cursor/none", "settings_window.cursor_for",
            {"edges": None}, {"value": sw.cursor_for(None)})
    start = (100, 100, 760, 620)
    limits = (560, 400, 1200, 900)
    for index, (edges, delta) in enumerate((((True, False, True, True), (50, 60)),
                                            ((True, False, False, False), (50, 60)),
                                            ((False, True, False, False), (50, 60)),
                                            ((False, False, True, False), (-50, -60)),
                                            ((False, False, False, True), (5000, 5000)),
                                            ((False, True, True, False), (-2000, 10)))):
        out.add(f"hud_settings/window/resized/{index:02d}", "settings_window.resized_rect",
                {"edges": list(edges), "start": list(start), "delta": list(delta),
                 "limits": list(limits)},
                {"value": list(sw.resized_rect(edges, start, delta, limits))})
    for index, y in enumerate((-1.0, 0.0, 26.0, 52.0, 53.0)):
        out.add(f"hud_settings/window/header/{index:02d}", "settings_window.in_header",
                {"x": 300.0, "y": y}, {"value": sw.in_header(300.0, y)})
    for index in range(6):
        out.add(f"hud_settings/window/item/{index:02d}", "settings_window.item_rect",
                {"index": index}, {"value": list(sw.item_rect(index))})
    for index, (x, y) in enumerate(((14.0, 64.0), (100.0, 84.0), (100.0, 100.0),
                                    (100.0, 108.0), (13.0, 84.0), (300.0, 84.0),
                                    (100.0, 300.0), (100.0, 20.0))):
        out.add(f"hud_settings/window/item-at/{index:02d}", "settings_window.item_at",
                {"x": x, "y": y}, {"value": sw.item_at(x, y)})
    out.add("hud_settings/window/sections", "settings_window.sections", {},
            {"value": [list(pair) for pair in sw.sections()]})
    for section in (*sw.SIDEBAR_ORDER, "no-existe"):
        out.add(f"hud_settings/window/groups/{section}", "settings_window.section_groups",
                {"name": section},
                {"value": [[name, [list(row) for row in rows]]
                           for name, rows in sw.section_groups(section)]})
        out.add(f"hud_settings/window/title/{section}", "settings_window.section_title",
                {"name": section}, {"value": sw.section_title(section)})
    for index, amount in enumerate((-1.0, 0.0, 0.25, 0.5, 0.75, 1.0, 1.5)):
        out.add(f"hud_settings/window/fade/{index:02d}", "settings_window.fade_factor",
                {"amount": amount}, {"value": sw.fade_factor(amount)})
    for index, (center, size) in enumerate(((26.0, 11.0), (31.0, 16.0), (0.0, 0.0),
                                            (100.0, 14.0))):
        out.add(f"hud_settings/window/cap/{index:02d}", "settings_window.cap_top",
                {"center_y": center, "size": size}, {"value": sw.cap_top(center, size)})
    out.add("hud_settings/window/constants", "settings_window", {},
            {"value": {"sidebar": sw.SIDEBAR_W, "default": [sw.DEFAULT_W, sw.DEFAULT_H],
                       "min": [sw.MIN_W, sw.MIN_H], "extra_h": sw.WINDOW_EXTRA_H,
                       "body_inset": sw.BODY_INSET, "edge": sw.EDGE, "corner": sw.CORNER,
                       "header_h": sw.HEADER_H, "keep_visible": sw.KEEP_VISIBLE,
                       "items_top": sw.ITEMS_TOP, "item_h": sw.ITEM_H,
                       "item_gap": sw.ITEM_GAP, "item_x": [sw.ITEM_X0, sw.ITEM_X1],
                       "item_radius": sw.ITEM_RADIUS, "icon": sw.ICON_SIZE,
                       "mark_d": sw.MARK_D, "mark_cx": sw.MARK_CX,
                       "mark_from_bottom": sw.MARK_FROM_BOTTOM, "foot_h": sw.FOOT_H,
                       "label_cy": sw.LABEL_CY, "cap_ratio": sw.CAP_RATIO,
                       "icons": dict(sw.SECTION_ICONS)}})


def hotkey_cases(out: FixtureFile) -> None:
    """`hotkey_field`: VK, combinaciones, rótulos, captura y duplicados."""
    vks = (0, 8, 9, 13, 16, 17, 18, 27, 32, 33, 34, 35, 36, 37, 38, 39, 40, 45, 46,
           48, 57, 65, 90, 91, 92, 112, 116, 123, 135, 136, 160, 165, -1, 255)
    for vk in vks:
        out.add(f"hud_settings/hotkey/vk/{vk + 2:03d}", "hotkey_field.vk_to_key",
                {"vk": vk}, {"key": hk.vk_to_key(vk), "modifier": hk.is_modifier_vk(vk)})
    specs = ("", " ", "ctrl", "ctrl+", "+", "ctrl+alt+space", "CTRL + ALT + Space",
             "control+shift+a", "windows+e", "super+esc", "option+f5", "ctrl+alt+e+",
             "ctrl+a+b", "shift", "shift+a", "shift+f5", "f5", "F5", "escape",
             "ctrl+alt+ñ", "+a")
    for index, spec in enumerate(specs):
        parsed = hk.canonical(spec)
        out.add(f"hud_settings/hotkey/canonical/{index:02d}", "hotkey_field.canonical",
                {"spec": spec},
                {"value": None if parsed is None else
                 {"mods": sorted(parsed[0]), "key": parsed[1]},
                 "parts": hk.parts_for(spec)})
    for index, key in enumerate(("space", "enter", "esc", "tab", "backspace", "delete",
                                 "insert", "home", "end", "pageup", "pagedown", "up",
                                 "down", "left", "right", "a", "f5", "ñ", "")):
        out.add(f"hud_settings/hotkey/label/{index:02d}", "hotkey_field.key_label",
                {"key": key}, {"value": hk.key_label(key)})
    for index in range(16):
        mods = [name for bit, name in enumerate(hk.MODIFIERS) if index & (1 << bit)]
        out.add(f"hud_settings/hotkey/format/{index:02d}", "hotkey_field.format_spec",
                {"mods": mods, "key": "space" if index % 2 else "f5"},
                {"value": hk.format_spec(set(mods), "space" if index % 2 else "f5"),
                 "labels": hk.mod_labels(set(mods))})
    presses = (((27, ()), (27, ("ctrl",)), (16, ()), (91, ()), (65, ()), (65, ("ctrl",)),
                (65, ("alt",)), (65, ("win",)), (65, ("shift",)), (65, ("ctrl", "shift")),
                (116, ()), (116, ("shift",)), (48, ()), (48, ("ctrl",)), (32, ("ctrl", "alt")),
                (255, ("ctrl",)), (13, ()), (46, ("alt", "shift"))))
    for index, (vk, mods) in enumerate(presses):
        event = hk.press(vk, frozenset(mods))
        out.add(f"hud_settings/hotkey/press/{index:02d}", "hotkey_field.press",
                {"vk": vk, "mods": list(mods)},
                {"kind": event.kind, "spec": event.spec, "reason": event.reason})
    current = {"hotkey": "ctrl+alt+space", "cancel_hotkey": "ctrl+alt+e",
               "mute_hotkey": "shift+f5", "menu_hotkey": "malo"}
    conflicts = (("ctrl+alt+space", "mute_hotkey"), ("control + alt + E", "hotkey"),
                 ("shift+f5", "hotkey"), ("ctrl+alt+space", "hotkey"),
                 ("ctrl+shift+x", "hotkey"), ("malo", "hotkey"), ("", "hotkey"))
    for index, (spec, skip) in enumerate(conflicts):
        out.add(f"hud_settings/hotkey/conflict/{index:02d}", "hotkey_field.conflict",
                {"spec": spec, "current": current, "skip": skip, "labels": LABELS},
                {"value": hk.conflict(spec, current, skip, LABELS)})
    out.add("hud_settings/hotkey/tables", "hotkey_field", {},
            {"value": {"keys": list(hk.HOTKEY_KEYS), "modifiers": list(hk.MODIFIERS),
                       "labels": dict(hk.MOD_LABELS), "escape": hk.VK_ESCAPE}})


def scalars(out: FixtureFile) -> None:
    settings = build()
    for index, key in enumerate(("speed", "theme", "api_key", "input_device_index")):
        out.add(f"hud_settings/value/{index:02d}", "MenuSettings.value", {"key": key},
                {"value": ready(settings.value(key)),
                 "label": settings.value_label(key),
                 "get": ready(settings.get(key))})
    for index, key in enumerate(("no_existe", "speed ", "SPEED")):
        try:
            settings.value(key)
        except Exception as error:
            out.add_error(f"hud_settings/value/error-{index}", "MenuSettings.value",
                          {"key": key}, str(error))
    out.add("hud_settings/value/default", "MenuSettings.get", {"key": "no_existe"},
            {"value": ready(settings.get("no_existe")), "fallback": ready(settings.get("no_existe", 7))})
    for index, value in enumerate((None, "", "abc", "12345678", "123456789", "ñ-12345678")):
        out.add(f"hud_settings/mask/{index:02d}", "mask_secret", {"value": value},
                {"value": mask_secret(value)})
    for index, db in enumerate((METER_DB_FLOOR, -60.0, -40.0, -20.0, METER_DB_CEIL)):
        out.add(f"hud_settings/db/{index:02d}", "db_to_rms", {"db": db},
                {"value": db_to_rms(db)})
    settings = build()
    out.add("hud_settings/colors/custom", "MenuSettings.custom_colors", {},
            {"value": ready(settings.custom_colors())})
    settings.set("theme_accent", "#010203")
    out.add("hud_settings/colors/set", "MenuSettings.custom_colors",
            {"values": {"theme_accent": "#010203"}},
            {"value": ready(settings.custom_colors()), "theme": settings.get("theme")})
    values = {"speed": "1.1", "smart_turn": "no", "api_key": None, "theme_primary": "abc"}
    object_settings = MenuSettings(Namespace(values), DEVICES, None, PROVIDERS, STT_MODELS,
                                   LLM_MODELS)
    out.add("hud_settings/app/object", "MenuSettings.from_app",
            {"object": values, "world": WORLD},
            {"value": ready(object_settings.snapshot()),
             "dirty": object_settings.dirty()})
    dict_settings = MenuSettings.from_app(values, DEVICES, None, PROVIDERS, STT_MODELS, LLM_MODELS)
    out.add("hud_settings/app/dict", "MenuSettings.from_app", {"values": values},
            {"value": ready(dict_settings.snapshot()),
             "dirty": dict_settings.dirty()})


def generate(out: FixtureFile) -> None:
    defaults(out)
    normalize(out)
    setters(out)
    formats(out)
    saved(out)
    catalogs(out)
    selectors(out)
    window_cases(out)
    hotkey_cases(out)
    scalars(out)
    if len(out.cases) < 1000:  # self-check: no perder la cobertura por campo
        raise AssertionError(f"hud_settings: solo {len(out.cases)} casos")


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
