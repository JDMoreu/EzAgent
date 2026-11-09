"""Modelo de vista de los ajustes del menú del notch.

Este módulo no conoce Tk, Win32, audio ni el sistema de archivos: solo guarda
los valores editables, su copia persistida y las opciones que el menú necesita
para pintarse. La escritura atómica de ``voice.json`` pertenece a la capa de
aplicación; aquí únicamente se prepara el diccionario de ``snapshot()``.

Claves editables: audio/VAD, voz/TTS, Whisper/STT, LLM/API y conversación.
``llm_profile`` (proveedor) y ``llm_model_pick`` (lista de modelos de la API)
son selectores de conveniencia que no se persisten: rellenan los campos reales.
``test_voice`` es una acción del menú, no una clave.

Los flotantes se redondean siempre al paso del slider y se normalizan para
evitar ruido binario (``0.30000000000000004``). ``dirty()`` compara la copia
guardada con los valores actuales después de ese redondeo.
"""

from __future__ import annotations

import threading
from dataclasses import dataclass
from typing import Any, Final, Iterable, Mapping, Sequence

# --------------------------------------------------------------------------
# Vocabulario de ajustes
# --------------------------------------------------------------------------

EDITABLE_KEYS: Final[tuple[str, ...]] = (
    # Audio y VAD.
    "input_device_index",
    "min_rms",
    "noise_multiplier",
    "silence_secs",
    "smart_turn",
    "turn_threshold",
    "pre_roll_secs",
    "min_speech_secs",
    "max_utterance_secs",
    "barge_in_rms",
    # Voz y TTS.
    "voice",
    "speed",
    "tts_sentence_pause",
    "tts_clause_pause",
    "tts_model_variant",
    "kokoro_model_path",
    "kokoro_voices_path",
    # Whisper / STT.
    "whisper_url",
    "stt_model",
    "stt_engine",
    "stt_prompt",
    "stt_timeout_secs",
    "auto_start_whisper",
    # LLM y API.
    "llm_provider",
    "llm_api_style",
    "llm_model",
    "llm_base_url",
    "api_key",
    "llm_temperature",
    "llm_max_tokens",
    "llm_reasoning_effort",
    # Conversación.
    "barge_in",
    "confirm_actions",
    # Atajos de teclado.
    "hotkey",
    "cancel_hotkey",
    "repeat_hotkey",
    "mute_hotkey",
    "menu_hotkey",
)

# ``llm_profile`` es un selector de conveniencia: elige un perfil guardado y
# copia proveedor, modelo, endpoint y clave en los campos reales. No se
# persiste como ajuste propio.
# ``llm_model_pick`` copia en ``llm_model`` un modelo de la lista de la API.
UI_KEYS: Final[tuple[str, ...]] = ("llm_profile", "llm_model_pick")
# Apariencia y escucha: se aplican y se guardan al instante (como el tema), sin
# pasar por «Guardar» ni contar como cambios pendientes.
INSTANT_KEYS: Final[tuple[str, ...]] = (
    "ui_layout",
    "theme",
    "ui_motion",
    "close_action",
    "voice_wake",
    "wake_phrase",
)
ALL_KEYS: Final[tuple[str, ...]] = EDITABLE_KEYS + UI_KEYS + INSTANT_KEYS

SLIDER_KEYS: Final[tuple[str, ...]] = (
    "min_rms",
    "noise_multiplier",
    "silence_secs",
    "turn_threshold",
    "pre_roll_secs",
    "min_speech_secs",
    "max_utterance_secs",
    "barge_in_rms",
    "speed",
    "tts_sentence_pause",
    "tts_clause_pause",
    "stt_timeout_secs",
    "llm_temperature",
    "llm_max_tokens",
)
SELECT_KEYS: Final[tuple[str, ...]] = (
    "input_device_index",
    "voice",
    "tts_model_variant",
    "stt_model",
    "stt_engine",
    "llm_profile",
    "llm_api_style",
    "llm_model_pick",
    "llm_reasoning_effort",
    "ui_layout",
    "theme",
    "ui_motion",
    "close_action",
)
TOGGLE_KEYS: Final[tuple[str, ...]] = (
    "smart_turn",
    "barge_in",
    "confirm_actions",
    "auto_start_whisper",
    "voice_wake",
)
TEXT_KEYS: Final[tuple[str, ...]] = (
    "llm_provider",
    "llm_model",
    "llm_base_url",
    "api_key",
    "whisper_url",
    "stt_prompt",
    "kokoro_model_path",
    "kokoro_voices_path",
    "hotkey",
    "cancel_hotkey",
    "repeat_hotkey",
    "mute_hotkey",
    "menu_hotkey",
    "wake_phrase",
)
SECRET_KEYS: Final[tuple[str, ...]] = ("api_key",)

LABELS: Final[dict[str, str]] = {
    "ui_layout": "Forma",
    "theme": "Tema",
    "ui_motion": "Movimiento",
    "close_action": "Al decir «cierra»",
    "voice_wake": "Reactivar por voz",
    "wake_phrase": "Frase de reactivación",
    "repeat_last": "Repetir lo último",
    "new_conversation": "Nueva conversación",
    "input_device_index": "Micrófono",
    "min_rms": "Umbral de voz",
    "noise_multiplier": "Umbral de ruido",
    "silence_secs": "Silencio máximo",
    "smart_turn": "Cierre inteligente",
    "turn_threshold": "Certeza para cerrar",
    "pre_roll_secs": "Antelación de escucha",
    "min_speech_secs": "Duración mínima",
    "max_utterance_secs": "Frase máxima",
    "barge_in_rms": "Umbral de interrupción",
    "voice": "Voz",
    "speed": "Velocidad",
    "tts_sentence_pause": "Pausa de frase",
    "tts_clause_pause": "Pausa de cláusula",
    "tts_model_variant": "Variante de Kokoro",
    "kokoro_model_path": "Ruta del modelo Kokoro",
    "kokoro_voices_path": "Ruta de voces Kokoro",
    "whisper_url": "Dirección de Whisper",
    "stt_model": "Modelo de Whisper",
    "stt_engine": "Motor de Whisper",
    "input_meter": "Entrada y umbral",
    "stt_prompt": "Indicación para Whisper",
    "stt_timeout_secs": "Tiempo máximo",
    "auto_start_whisper": "Arrancar Whisper",
    "llm_profile": "Proveedor",
    "llm_provider": "Nombre del perfil",
    "llm_api_style": "Compatibilidad",
    "llm_model": "Modelo",
    "llm_model_pick": "Modelos de la API",
    "llm_base_url": "Dirección de la API",
    "api_key": "Clave API",
    "llm_temperature": "Temperatura",
    "llm_max_tokens": "Máximo de tokens",
    "llm_reasoning_effort": "Razonamiento",
    "barge_in": "Interrupción activa",
    "confirm_actions": "Confirmar acciones",
    "test_voice": "Prueba de voz",
    "hotkey": "Atajo de llamada",
    "cancel_hotkey": "Atajo de colgar",
    "repeat_hotkey": "Atajo de repetir",
    "mute_hotkey": "Atajo de silencio",
    "menu_hotkey": "Atajo de ajustes",
    "open_system_md": "Abrir system.md",
    "open_voice_json": "Abrir voice.json",
}

# Etiquetas de la lista real de voces del motor (los ids son los de Kokoro).
VOICE_LABELS: Final[dict[str, str]] = {
    "em_alex": "Álex · masculina",
    "ef_dora": "Dora · femenina",
    "em_santa": "Santa · masculina grave",
}
VOICE_IDS: Final[tuple[str, ...]] = tuple(VOICE_LABELS)

DEFAULT_VALUES: Final[dict[str, Any]] = {
    "input_device_index": None,
    "min_rms": 0.0006,
    "noise_multiplier": 1.3,
    "silence_secs": 1.3,
    "smart_turn": True,
    "turn_threshold": 0.5,
    "pre_roll_secs": 0.6,
    "min_speech_secs": 0.2,
    "max_utterance_secs": 30.0,
    "barge_in_rms": 0.008,
    "voice": "em_alex",
    "speed": 0.95,
    "tts_sentence_pause": 0.30,
    "tts_clause_pause": 0.12,
    "tts_model_variant": "int8",
    "kokoro_model_path": "",
    "kokoro_voices_path": "",
    "whisper_url": "http://127.0.0.1:8088/inference",
    "stt_model": "ggml-large-v3-turbo-q5_0",
    "stt_prompt": "",
    "stt_timeout_secs": 60.0,
    "auto_start_whisper": True,
    "llm_profile": "opencode",
    "llm_provider": "opencode",
    "llm_api_style": "openai",
    "llm_model": "deepseek-v4.1-flash",
    "llm_model_pick": "",
    "llm_base_url": "https://opencode.ai/inference/go/openai/v1",
    "api_key": "",
    "llm_temperature": 0.3,
    "llm_max_tokens": 320,
    "llm_reasoning_effort": "none",
    "stt_engine": "auto",
    "barge_in": True,
    "confirm_actions": False,
    "hotkey": "ctrl+alt+space",
    "cancel_hotkey": "ctrl+alt+e",
    "repeat_hotkey": "ctrl+alt+r",
    "mute_hotkey": "ctrl+alt+m",
    "menu_hotkey": "ctrl+alt+o",
    "ui_layout": "pill",
    "theme": "auto",
    "ui_motion": "system",
    "close_action": "pause",
    "voice_wake": True,
    "wake_phrase": "EzAgent, escúchame",
}


@dataclass(frozen=True, slots=True)
class SliderSpec:
    """Rango cerrado, paso y formato de un slider."""

    minimum: float
    maximum: float
    step: float
    decimals: int
    suffix: str = ""


SLIDERS: Final[dict[str, SliderSpec]] = {
    "min_rms": SliderSpec(0.0003, 0.008, 0.0001, 4),
    "noise_multiplier": SliderSpec(1.0, 2.5, 0.1, 1, "×"),
    "silence_secs": SliderSpec(0.8, 3.5, 0.1, 1, " s"),
    "turn_threshold": SliderSpec(0.3, 0.95, 0.05, 2),
    "pre_roll_secs": SliderSpec(0.2, 1.5, 0.1, 1, " s"),
    "min_speech_secs": SliderSpec(0.1, 1.0, 0.05, 2, " s"),
    "max_utterance_secs": SliderSpec(5.0, 120.0, 5.0, 0, " s"),
    "barge_in_rms": SliderSpec(0.001, 0.05, 0.001, 3),
    "speed": SliderSpec(0.75, 1.25, 0.05, 2, "×"),
    "tts_sentence_pause": SliderSpec(0.0, 1.0, 0.05, 2, " s"),
    "tts_clause_pause": SliderSpec(0.0, 0.6, 0.02, 2, " s"),
    "stt_timeout_secs": SliderSpec(5.0, 180.0, 5.0, 0, " s"),
    "llm_temperature": SliderSpec(0.0, 1.5, 0.05, 2),
    "llm_max_tokens": SliderSpec(80.0, 1200.0, 20.0, 0),
}


@dataclass(frozen=True, slots=True)
class SelectOption:
    """Una opción de un selector en línea."""

    value: Any
    label: str


TTS_VARIANTS: Final[tuple[SelectOption, ...]] = (
    SelectOption("int8", "int8 · rápida"),
    SelectOption("fp32", "fp32 · precisión"),
)
STT_ENGINE_OPTIONS: Final[tuple[SelectOption, ...]] = (
    SelectOption("auto", "Automático"),
    SelectOption("vulkan", "GPU · Vulkan"),
    SelectOption("cpu", "CPU"),
)
LLM_REASONING_OPTIONS: Final[tuple[SelectOption, ...]] = (
    SelectOption("none", "Sin razonamiento"),
    SelectOption("low", "Bajo"),
    SelectOption("medium", "Medio"),
    SelectOption("high", "Alto"),
)
# Selectores de apariencia y escucha (valores fijos, en orden de muestra).
FIXED_OPTIONS: Final[dict[str, tuple[SelectOption, ...]]] = {
    "ui_layout": (SelectOption("pill", "Píldora"), SelectOption("circle", "Círculo")),
    "theme": (
        SelectOption("auto", "Automático"),
        SelectOption("light", "Claro"),
        SelectOption("dark", "Oscuro"),
    ),
    "ui_motion": (
        SelectOption("system", "Según el sistema"),
        SelectOption("full", "Completo"),
        SelectOption("reduced", "Reducido"),
    ),
    "close_action": (
        SelectOption("pause", "Pausa"),
        SelectOption("hangup", "Cuelga"),
    ),
    # Formato de la API del proveedor (para «Personalizado» lo elige el usuario).
    "llm_api_style": (
        SelectOption("openai", "OpenAI"),
        SelectOption("anthropic", "Anthropic"),
    ),
}


_MISSING: Final[object] = object()


# --------------------------------------------------------------------------
# Utilidades internas
# --------------------------------------------------------------------------


def _read(values: Any, key: str, default: Any) -> Any:
    """Lee una clave de un dict o de un objeto de configuración."""
    if isinstance(values, Mapping):
        return values.get(key, default)
    return getattr(values, key, default)


def _as_bool(value: Any) -> bool:
    if isinstance(value, str):
        return value.strip().lower() not in ("", "0", "false", "no", "off")
    return bool(value)


def _decimal(value: float, decimals: int) -> str:
    """Número con coma decimal, como el resto de la UI en español."""
    return f"{float(value):.{decimals}f}".replace(".", ",")


def _quantize(value: Any, spec: SliderSpec) -> float:
    """Redondea al paso (anclado al mínimo) y limpia el ruido binario."""
    try:
        number = float(value)
    except (TypeError, ValueError):
        number = spec.minimum
    if number != number:  # NaN defensivo
        number = spec.minimum
    number = max(spec.minimum, min(spec.maximum, number))
    steps = round((number - spec.minimum) / spec.step)
    quantized = spec.minimum + steps * spec.step
    # ``decimals + 2`` conserva el paso y elimina colas como 0.30000000000004.
    quantized = round(quantized, spec.decimals + 2)
    return float(max(spec.minimum, min(spec.maximum, quantized)))


def _normalize_devices(devices: Iterable[Any] | None) -> tuple[dict[str, Any], ...]:
    """Normaliza la lista real ``[{index,name,rate,channels}]`` del motor."""
    out: list[dict[str, Any]] = []
    seen: set[int] = set()
    for raw in devices or ():
        if isinstance(raw, Mapping):
            index = raw.get("index")
            name = raw.get("name", "")
            rate = raw.get("rate", 0)
            channels = raw.get("channels", 0)
        else:
            index = getattr(raw, "index", None)
            name = getattr(raw, "name", "")
            rate = getattr(raw, "rate", 0)
            channels = getattr(raw, "channels", 0)
        try:
            index_i = int(index)
        except (TypeError, ValueError):
            continue
        if index_i in seen:
            continue
        seen.add(index_i)
        try:
            rate_i = int(rate or 0)
        except (TypeError, ValueError):
            rate_i = 0
        try:
            channels_i = int(channels or 0)
        except (TypeError, ValueError):
            channels_i = 0
        out.append(
            {
                "index": index_i,
                "name": str(name or ""),
                "rate": rate_i,
                "channels": channels_i,
            }
        )
    return tuple(out)


def _normalize_voices(
    voices: Iterable[Any] | Mapping[str, str] | None,
    current: str,
) -> tuple[SelectOption, ...]:
    """Acepta ids, pares ``(id, etiqueta)``, dicts o un mapping id -> etiqueta."""
    pairs: list[tuple[str, str]] = []
    if isinstance(voices, Mapping):
        pairs = [(str(key), str(label)) for key, label in voices.items()]
    elif voices:
        for item in voices:
            if isinstance(item, str):
                pairs.append((item, VOICE_LABELS.get(item, item)))
            elif isinstance(item, Mapping):
                ident = item.get("id") or item.get("voice")
                if ident:
                    pairs.append(
                        (str(ident), str(item.get("label", VOICE_LABELS.get(str(ident), ident))))
                    )
            elif isinstance(item, Sequence) and len(item) >= 2:
                pairs.append((str(item[0]), str(item[1])))
    if not pairs:
        pairs = [(ident, label) for ident, label in VOICE_LABELS.items()]
    seen: set[str] = set()
    out: list[SelectOption] = []
    for ident, label in pairs:
        if not ident or ident in seen:
            continue
        seen.add(ident)
        out.append(SelectOption(ident, label or VOICE_LABELS.get(ident, ident)))
    if current and current not in seen:
        # Una voz nueva del motor no debe romper el selector.
        out.append(SelectOption(current, VOICE_LABELS.get(current, current)))
    return tuple(out)


def _normalize_names(
    options: Mapping[str, str] | Iterable[Any] | None,
    current: str = "",
) -> tuple[SelectOption, ...]:
    """Opciones de modelo: mapping, pares ``(id, etiqueta)`` o ``SelectOption``."""
    pairs: list[tuple[str, str]] = []
    if isinstance(options, Mapping):
        pairs = [(str(key), str(label)) for key, label in options.items()]
    elif options:
        for item in options:
            if isinstance(item, SelectOption):
                pairs.append((str(item.value), str(item.label)))
            elif isinstance(item, str):
                pairs.append((item, item))
            elif isinstance(item, Sequence) and len(item) >= 2:
                pairs.append((str(item[0]), str(item[1])))
            elif isinstance(item, Mapping):
                ident = item.get("value") or item.get("id") or item.get("model")
                if ident:
                    pairs.append(
                        (str(ident), str(item.get("label", ident)))
                    )
    seen: set[str] = set()
    out: list[SelectOption] = []
    for ident, label in pairs:
        if not ident or ident in seen:
            continue
        seen.add(ident)
        out.append(SelectOption(ident, label or ident))
    if current and current not in seen:
        out.append(SelectOption(current, current))
    return tuple(out)


def _normalize_providers(
    providers: Mapping[str, Any] | Iterable[Any] | None,
) -> dict[str, dict[str, str]]:
    """Normaliza perfiles ``nombre -> {label, base_url, model, api_key, api_style}``."""
    out: dict[str, dict[str, str]] = {}
    if not providers:
        return out
    items: list[tuple[Any, Any]] = []
    if isinstance(providers, Mapping):
        items = list(providers.items())
    else:
        for raw in providers:
            if isinstance(raw, Mapping):
                name = raw.get("name") or raw.get("provider")
                if name:
                    items.append((name, raw))
            elif isinstance(raw, Sequence) and len(raw) >= 2:
                items.append((raw[0], raw[1]))
    for raw_name, raw in items:
        name = str(raw_name or "").strip()
        if not name:
            continue
        if isinstance(raw, Mapping):
            label = str(raw.get("label") or raw.get("name") or name)
            base_url = str(raw.get("base_url") or "")
            model = str(raw.get("model") or raw.get("default_model") or "")
            api_key = str(raw.get("api_key") or "")
            provider = str(raw.get("provider") or name)
            api_style = str(raw.get("api_style") or "openai")
        else:
            label = str(getattr(raw, "label", name))
            base_url = str(getattr(raw, "base_url", ""))
            model = str(
                getattr(raw, "default_model", "") or getattr(raw, "model", "")
            )
            api_key = str(getattr(raw, "api_key", ""))
            provider = str(getattr(raw, "provider", name) or name)
            api_style = str(getattr(raw, "api_style", "openai") or "openai")
        out[name] = {
            "label": label or name,
            "base_url": base_url,
            "model": model,
            "api_key": api_key,
            "provider": provider or name,
            "api_style": api_style if api_style in ("openai", "anthropic") else "openai",
        }
    return out


def mask_secret(value: Any) -> str:
    """Devuelve una clave enmascarada; nunca pinta el valor completo."""
    text = str(value or "")
    if not text:
        return "Sin clave"
    tail = text[-4:] if len(text) >= 8 else ""
    return "Configurada" + (f" · ····{tail}" if tail else "")


# --------------------------------------------------------------------------
# Modelo de vista
# --------------------------------------------------------------------------


class MenuSettings:
    """Valores actuales, copia guardada y opciones de los selectores.

    Construcción típica en la capa de aplicación::

        settings = MenuSettings.from_app(cfg_dict, devices, MENU_VOICES)

    ``from_app`` no escribe nada. ``mark_saved()`` solo actualiza la copia
    interna; el guardado real lo hace el callback ``on_save`` del menú.
    """

    def __init__(
        self,
        values: Mapping[str, Any],
        devices: Iterable[Any] | None,
        voices: Iterable[Any] | Mapping[str, str] | None,
        providers: Mapping[str, Any] | Iterable[Any] | None = None,
        stt_models: Mapping[str, str] | Iterable[Any] | None = None,
        llm_models: Mapping[str, Any] | None = None,
    ) -> None:
        current_voice = str(_read(values, "voice", DEFAULT_VALUES["voice"]) or "")
        self._devices = _normalize_devices(devices)
        self._voices = _normalize_voices(voices, current_voice)
        self._providers = _normalize_providers(providers)
        self._stt_models = _normalize_names(
            stt_models, str(_read(values, "stt_model", "") or "")
        )
        self._llm_models: dict[str, tuple[SelectOption, ...]] = {}
        # La lista real de modelos llega desde un hilo de red (set_llm_models).
        self._models_lock = threading.Lock()
        if isinstance(llm_models, Mapping):
            for provider, options in llm_models.items():
                self._llm_models[str(provider)] = _normalize_names(options)
        self._values: dict[str, Any] = {}
        for key in ALL_KEYS:
            default = DEFAULT_VALUES[key]
            self._values[key] = self._normalize_value(
                key, _read(values, key, default)
            )
        # El selector de perfiles arranca en el proveedor activo.
        current_provider = str(self._values.get("llm_provider") or "")
        if current_provider:
            self._values["llm_profile"] = current_provider
        self._saved: dict[str, Any] = {
            key: self._values[key] for key in EDITABLE_KEYS
        }

    # ---- construcción -----------------------------------------------------
    @classmethod
    def from_app(
        cls,
        values: Any,
        devices: Iterable[Any] | None = None,
        voices: Iterable[Any] | Mapping[str, str] | None = None,
        providers: Mapping[str, Any] | Iterable[Any] | None = None,
        stt_models: Mapping[str, str] | Iterable[Any] | None = None,
        llm_models: Mapping[str, Any] | None = None,
    ) -> "MenuSettings":
        """Crea el modelo desde la configuración real y las listas del motor.

        ``values`` puede ser un dict o cualquier objeto con atributos. Las
        claves desconocidas se ignoran. La copia guardada arranca igual a los
        valores recibidos.
        """
        if isinstance(values, Mapping):
            data = dict(values)
        else:
            data = {
                key: getattr(values, key, DEFAULT_VALUES[key])
                for key in ALL_KEYS
            }
        return cls(data, devices, voices, providers, stt_models, llm_models)

    # ---- normalización ----------------------------------------------------
    def _voice_ids(self) -> set[str]:
        return {str(option.value) for option in self._voices}

    def _normalize_value(self, key: str, value: Any) -> Any:
        if key in SLIDERS:
            return _quantize(value, SLIDERS[key])
        if key in TOGGLE_KEYS:
            return _as_bool(value)
        if key == "input_device_index":
            if value is None or value == "":
                return None
            try:
                index = int(value)
            except (TypeError, ValueError):
                return None
            return None if index < 0 else index
        if key == "voice":
            text = str(value or DEFAULT_VALUES["voice"])
            if text not in self._voice_ids():
                self._voices = self._voices + (
                    SelectOption(text, VOICE_LABELS.get(text, text)),
                )
            return text
        if key == "tts_model_variant":
            text = str(value or DEFAULT_VALUES["tts_model_variant"])
            valid = {option.value for option in TTS_VARIANTS}
            return (
                text
                if text in valid
                else str(DEFAULT_VALUES["tts_model_variant"])
            )
        if key == "stt_engine":
            text = str(value or "auto")
            valid = {option.value for option in STT_ENGINE_OPTIONS}
            return text if text in valid else "auto"
        if key == "llm_reasoning_effort":
            text = str(value or "none")
            valid = {option.value for option in LLM_REASONING_OPTIONS}
            return text if text in valid else "none"
        if key in FIXED_OPTIONS:
            text = str(value or DEFAULT_VALUES[key]).lower()
            valid = {option.value for option in FIXED_OPTIONS[key]}
            return text if text in valid else str(DEFAULT_VALUES[key])
        if key == "wake_phrase":
            return str(value or "").strip() or str(DEFAULT_VALUES[key])
        if key in ("llm_profile", "llm_model_pick"):
            return str(value or "").strip()
        if key in ("stt_model", "llm_model"):
            return str(value if value is not None else "")
        if key in TEXT_KEYS:
            return str(value if value is not None else "")
        raise KeyError(f"ajuste desconocido: {key!r}")

    # ---- lectura ----------------------------------------------------------
    @property
    def devices(self) -> tuple[dict[str, Any], ...]:
        """Copia superficial de la lista normalizada de micrófonos."""
        return tuple(dict(device) for device in self._devices)

    @property
    def voices(self) -> tuple[SelectOption, ...]:
        return self._voices

    @property
    def providers(self) -> dict[str, dict[str, str]]:
        """Perfiles disponibles, ya normalizados (copia defensiva)."""
        return {name: dict(values) for name, values in self._providers.items()}


    def value(self, key: str) -> Any:
        self._require(key)
        return self._values[key]

    def get(self, key: str, default: Any = None) -> Any:
        return self._values.get(key, default)

    # ---- edición ----------------------------------------------------------
    def _require(self, key: str) -> None:
        if key not in ALL_KEYS:
            raise KeyError(f"ajuste desconocido: {key!r}")

    def set(self, key: str, value: Any) -> Any:
        """Cambia un valor, lo valida y lo normaliza. Devuelve el nuevo valor."""
        self._require(key)
        new = self._normalize_value(key, value)
        self._values[key] = new
        if key == "llm_profile":
            self._apply_profile(new)
        elif key == "llm_model_pick" and new:
            self._values["llm_model"] = new
        return new

    def set_llm_models(self, provider: str, models: Iterable[Any]) -> None:
        """Sustituye la lista de modelos de un proveedor (hilo de red)."""
        options = _normalize_names(models)
        with self._models_lock:
            updated = dict(self._llm_models)
            updated[str(provider)] = options
            self._llm_models = updated

    def _model_options(self) -> tuple[SelectOption, ...]:
        provider = str(self._values.get("llm_provider") or "")
        with self._models_lock:
            options = self._llm_models.get(provider, ())
        current = str(self._values.get("llm_model") or "")
        if current and all(option.value != current for option in options):
            options = (SelectOption(current, current),) + options
        return options

    def _apply_profile(self, name: str) -> None:
        """Copia un perfil guardado en los campos editables de LLM."""
        profile = self._providers.get(str(name))
        if profile is None:
            return
        provider = profile.get("provider") or str(name)
        values = {
            "llm_provider": provider,
            "llm_api_style": profile.get("api_style", "openai"),
            "llm_model": profile.get("model", ""),
            "llm_base_url": profile.get("base_url", ""),
            "api_key": profile.get("api_key", ""),
        }
        for key, value in values.items():
            self._values[key] = self._normalize_value(key, value)




    # ---- guardado ---------------------------------------------------------
    def dirty(self) -> bool:
        """True si algún valor editable difiere de la copia guardada.

        Los sliders se comparan después de redondear al paso, de modo que un
        temblor de coma flotante no marca cambios falsos.
        """
        for key in EDITABLE_KEYS:
            current = self._values[key]
            saved = self._saved[key]
            if key in SLIDERS:
                if abs(
                    _quantize(current, SLIDERS[key])
                    - _quantize(saved, SLIDERS[key])
                ) > 1e-9:
                    return True
            elif current != saved:
                return True
        return False

    def mark_saved(self, values: Mapping[str, Any] | None = None) -> None:
        """Marca como guardados los valores actuales (o una instantánea dada)."""
        if values is None:
            self._saved = dict(self._values)
            return
        saved: dict[str, Any] = {}
        for key in EDITABLE_KEYS:
            raw = values.get(key, self._saved[key])
            saved[key] = self._normalize_for_saved(key, raw)
        self._saved = saved

    def restore_saved(self, values: Mapping[str, Any]) -> None:
        """Restaura la copia guardada sin tocar los valores actuales.

        Lo usa el menú cuando un guardado asíncrono falla después de haber
        mostrado la confirmación optimista.
        """
        self.mark_saved(values)

    def _normalize_for_saved(self, key: str, value: Any) -> Any:
        if key in SLIDERS:
            return _quantize(value, SLIDERS[key])
        if key in TOGGLE_KEYS:
            return _as_bool(value)
        if key == "input_device_index":
            if value is None or value == "":
                return None
            try:
                index = int(value)
            except (TypeError, ValueError):
                return None
            return None if index < 0 else index
        if key == "voice":
            return str(value or DEFAULT_VALUES["voice"])
        if key == "tts_model_variant":
            text = str(value or DEFAULT_VALUES["tts_model_variant"])
            valid = {option.value for option in TTS_VARIANTS}
            return (
                text
                if text in valid
                else str(DEFAULT_VALUES["tts_model_variant"])
            )
        if key == "stt_engine":
            text = str(value or "auto")
            valid = {option.value for option in STT_ENGINE_OPTIONS}
            return text if text in valid else "auto"
        if key == "llm_reasoning_effort":
            text = str(value or "none")
            valid = {option.value for option in LLM_REASONING_OPTIONS}
            return text if text in valid else "none"
        if key in ("stt_model", "llm_model"):
            return str(value if value is not None else "")
        if key in TEXT_KEYS:
            return str(value if value is not None else "")
        return value

    def saved_snapshot(self) -> dict[str, Any]:
        """Copia de la instantánea persistida (para depuración y rollback)."""
        return dict(self._saved)

    def snapshot(self) -> dict[str, Any]:
        """Diccionario de valores actuales listo para ``on_save``/``on_test``."""
        return {key: self._values[key] for key in EDITABLE_KEYS}

    # ---- presentación -----------------------------------------------------

    def format(self, key: str, value: Any = _MISSING) -> str:
        """Texto de un valor con el formato cerrado de la especificación."""
        self._require(key)
        if value is _MISSING:
            value = self._values[key]
        if key in SLIDERS:
            spec = SLIDERS[key]
            return _decimal(_quantize(value, spec), spec.decimals) + spec.suffix
        if key == "input_device_index":
            return self._device_label(value)
        if key == "voice":
            return self._voice_label(value)
        if key == "tts_model_variant":
            for option in TTS_VARIANTS:
                if option.value == value:
                    return option.label
            return str(value)
        if key == "stt_engine":
            for option in STT_ENGINE_OPTIONS:
                if option.value == value:
                    return option.label
            return str(value)
        if key == "llm_reasoning_effort":
            for option in LLM_REASONING_OPTIONS:
                if option.value == value:
                    return option.label
            return str(value)
        if key in FIXED_OPTIONS:
            for option in FIXED_OPTIONS[key]:
                if option.value == value:
                    return option.label
            return str(value)
        if key == "stt_model":
            for option in self.select_options("stt_model"):
                if option.value == value:
                    return option.label
            return str(value or "Sin modelo")
        if key == "llm_model":
            return str(value or "Sin modelo")
        if key == "llm_model_pick":
            count = len(self._model_options())
            return f"{count} disponibles" if count else "Sin lista"
        if key == "llm_profile":
            profile = self._providers.get(str(value))
            return (
                str(profile.get("label"))
                if profile
                else str(value or "Sin perfiles")
            )
        if key == "llm_provider":
            profile = self._providers.get(str(value))
            return (
                str(profile.get("label"))
                if profile
                else str(value or "Sin proveedor")
            )
        if key in SECRET_KEYS:
            return mask_secret(value)
        if key in TOGGLE_KEYS:
            return "Sí" if _as_bool(value) else "No"
        if key in TEXT_KEYS:
            return str(value) if str(value or "") else "Sin definir"
        return str(value)

    def value_label(self, key: str) -> str:
        """Etiqueta del valor actual (lo que pinta la fila)."""
        return self.format(key)

    def _device_label(self, value: Any) -> str:
        if not self._devices:
            return "Sin micrófonos"
        if value is None:
            return "Predeterminado del sistema"
        try:
            index = int(value)
        except (TypeError, ValueError):
            return "Predeterminado del sistema"
        for device in self._devices:
            if int(device["index"]) == index:
                return f"{index}: {device['name']}"
        return f"{index}: no disponible"

    def _voice_label(self, value: Any) -> str:
        ident = str(value)
        for option in self._voices:
            if str(option.value) == ident:
                return option.label
        return VOICE_LABELS.get(ident, ident)

    # ---- selectores -------------------------------------------------------
    def select_options(self, key: str) -> tuple[SelectOption, ...]:
        """Opciones del selector, en el orden en que se muestran."""
        if key == "input_device_index":
            return tuple(
                SelectOption(int(device["index"]), f"{device['index']}: {device['name']}")
                for device in self._devices
            )
        if key == "voice":
            return self._voices
        if key == "tts_model_variant":
            return TTS_VARIANTS
        if key == "stt_model":
            return self._stt_models
        if key == "stt_engine":
            return STT_ENGINE_OPTIONS
        if key == "llm_reasoning_effort":
            return LLM_REASONING_OPTIONS
        if key in FIXED_OPTIONS:
            return FIXED_OPTIONS[key]
        if key == "llm_model_pick":
            return self._model_options()
        if key == "llm_profile":
            return tuple(
                SelectOption(name, values.get("label", name))
                for name, values in self._providers.items()
            )
        raise KeyError(f"{key!r} no es un selector")

    def selector_enabled(self, key: str) -> bool:
        """False solo para micrófono sin dispositivos (fila deshabilitada)."""
        if key == "input_device_index":
            return bool(self._devices)
        if key == "voice":
            return bool(self._voices)
        if key == "tts_model_variant":
            return True
        if key == "stt_model":
            return bool(self._stt_models)
        if key == "stt_engine":
            return True
        if key == "llm_reasoning_effort" or key in FIXED_OPTIONS:
            return True
        if key == "llm_model_pick":
            return bool(self._model_options())
        if key == "llm_profile":
            return bool(self._providers)
        return False



def normalize_setting(key: str, value: Any) -> Any:
    """Valor válido para ``key`` (cae al predeterminado si no lo es)."""
    return MenuSettings({}, None, None)._normalize_value(key, value)


__all__ = [
    "ALL_KEYS",
    "DEFAULT_VALUES",
    "EDITABLE_KEYS",
    "FIXED_OPTIONS",
    "INSTANT_KEYS",
    "LABELS",
    "MenuSettings",
    "SECRET_KEYS",
    "SELECT_KEYS",
    "SLIDERS",
    "SLIDER_KEYS",
    "STT_ENGINE_OPTIONS",
    "LLM_REASONING_OPTIONS",
    "SelectOption",
    "SliderSpec",
    "TEXT_KEYS",
    "TOGGLE_KEYS",
    "TTS_VARIANTS",
    "UI_KEYS",
    "VOICE_IDS",
    "VOICE_LABELS",
    "mask_secret",
    "normalize_setting",
]
