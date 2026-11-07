"""Configuración de `ezagent-voice` (mensajero de voz para Herdr)."""

from __future__ import annotations

import copy
import json
import math
import os
from dataclasses import asdict, dataclass, field
from pathlib import Path

from ezagent.llm_providers import OPENCODE_BASE_URL, resolve_llm

DEFAULT_AGENTS: dict[str, list[str]] = {
    "claude": ["claude"],
    "opencode": ["opencode"],
    "mcode": ["mcode"],
    "minimax": ["mcode"],
    "codex": ["codex"],
}

# Perfiles de lanzamiento por agente. El usuario puede editarlos en
# voice.json: command, model_args, effort_args, modelos y esfuerzos.
# Los argumentos con {model}/{effort} se sustituyen solo si se piden; así
# lanzar sin modelo/esfuerzo conserva el comando original.
DEFAULT_AGENT_PROFILES: dict[str, dict[str, object]] = {
    "claude": {
        "models": ["sonnet", "opus", "haiku"],
        "efforts": ["low", "medium", "high"],
        "default_model": "",
        "default_effort": "",
        "model_args": ["--model", "{model}"],
        "effort_args": ["--effort", "{effort}"],
    },
    "opencode": {
        "models": ["deepseek-v4.1-flash", "claude-sonnet-5-5"],
        "efforts": ["low", "medium", "high"],
        "default_model": "",
        "default_effort": "",
        "model_args": ["--model", "{model}"],
        "effort_args": ["--effort", "{effort}"],
    },
    "mcode": {
        "models": [],
        "efforts": ["low", "medium", "high"],
        "default_model": "",
        "default_effort": "",
        "model_args": ["--model", "{model}"],
        "effort_args": ["--effort", "{effort}"],
    },
    "minimax": {
        "models": [],
        "efforts": ["low", "medium", "high"],
        "default_model": "",
        "default_effort": "",
        "model_args": ["--model", "{model}"],
        "effort_args": ["--effort", "{effort}"],
    },
    "codex": {
        "models": [],
        "efforts": ["low", "medium", "high"],
        "default_model": "",
        "default_effort": "",
        "model_args": ["--model", "{model}"],
        "effort_args": ["-c", "model_reasoning_effort={effort}"],
    },
}


def merge_agent_profiles(
    base: dict[str, dict[str, object]] | None,
    override: dict[str, object] | None,
) -> dict[str, dict[str, object]]:
    """Mezcla perfiles por agente conservando los valores por defecto."""
    result = copy.deepcopy(base or DEFAULT_AGENT_PROFILES)
    if not isinstance(override, dict):
        return result
    for raw_name, raw_profile in override.items():
        name = str(raw_name or "").strip()
        if not name or not isinstance(raw_profile, dict):
            continue
        profile = dict(result.get(name, {}))
        profile.update(raw_profile)
        result[name] = profile
    return result


def config_path() -> Path:
    base = os.getenv("APPDATA") or os.getenv("XDG_CONFIG_HOME") or str(
        Path.home() / ".config"
    )
    return Path(base) / "EzAgent" / "voice.json"


@dataclass
class VoiceConfig:
    # Atajos globales (formato de Dicta: alt+q, ctrl+alt+space...).
    # Se evitan Alt+Q/E/R de Dicta para que las dos apps convivan.
    hotkey: str = "ctrl+alt+space"
    cancel_hotkey: str = "ctrl+alt+e"
    repeat_hotkey: str = "ctrl+alt+r"
    mute_hotkey: str = "ctrl+alt+m"
    menu_hotkey: str = "ctrl+alt+o"

    # STT / TTS.
    whisper_url: str = "http://127.0.0.1:8088/inference"
    auto_start_whisper: bool = True
    stt_model: str = "ggml-large-v3-turbo-q5_0"
    stt_engine: str = "auto"
    stt_prompt: str = ""
    stt_timeout_secs: float = 60.0
    voice: str = "em_alex"
    speed: float = 0.95
    tts_model_variant: str = "int8"
    kokoro_model_path: str = ""
    kokoro_voices_path: str = ""
    tts_sentence_pause: float = 0.30
    tts_clause_pause: float = 0.12
    input_device_index: int | None = None
    output_device_index: int | None = None
    silence_secs: float = 1.3
    min_rms: float = 0.0006
    pre_roll_secs: float = 0.6
    min_speech_secs: float = 0.2
    max_utterance_secs: float = 30.0
    noise_multiplier: float = 1.3
    barge_in: bool = True
    barge_in_rms: float = 0.008
    # Versión de los valores por defecto del VAD. La 2 baja el suelo de
    # arranque para voces flojas y acepta frases cortas.
    vad_defaults_version: int = 2

    # LLM.
    llm_provider: str = "opencode"
    llm_model: str = "deepseek-v4.1-flash"
    llm_base_url: str = OPENCODE_BASE_URL
    api_key: str = ""
    llm_temperature: float = 0.3
    llm_max_tokens: int = 320
    llm_reasoning_effort: str = "none"
    # True cuando el usuario elige proveedor/modelo desde el menú del notch.
    # A partir de ahí voice.json manda sobre LLM_PROVIDER del .env, para que
    # el cambio sobreviva al reinicio.
    llm_pinned: bool = False
    confirm_actions: bool = False

    # Herdr.
    hc_path: str = "hc"
    herdr_app_path: str = "herdr-app"
    default_cwd: str = ""
    agents: dict[str, list[str]] = field(default_factory=lambda: dict(DEFAULT_AGENTS))
    agent_profiles: dict[str, dict[str, object]] = field(
        default_factory=lambda: copy.deepcopy(DEFAULT_AGENT_PROFILES)
    )
    announce_all: bool = False
    max_read_lines: int = 40
    notch_hidden: bool = False
    # Última posición del notch en píxeles físicos de pantalla. Solo se
    # persiste al terminar un arrastre; si falta una coordenada, ambas son
    # None y la isla usa su posición por defecto.
    notch_x: int | None = None
    notch_y: int | None = None

    # Tema del notch: auto (sigue a Windows), dark o light. Se guarda al
    # instante y no cuenta para «cambios sin guardar».
    theme: str = "auto"

    @classmethod
    def load(cls, path: Path | None = None) -> "VoiceConfig":
        path = path or config_path()
        data: dict = {}
        if path.exists():
            try:
                data = json.loads(path.read_text(encoding="utf-8"))
            except Exception:
                data = {}
        cfg = cls(**{k: v for k, v in data.items() if k in cls.__dataclass_fields__})
        # Los perfiles de agente se mezclan con los valores de fábrica para
        # que añadir un agente no borre los modelos/esfuerzos de los demás.
        cfg.agent_profiles = merge_agent_profiles(
            DEFAULT_AGENT_PROFILES,
            cfg.agent_profiles if isinstance(cfg.agent_profiles, dict) else None,
        )
        # La posición del notch se guarda como dos enteros; un valor suelto,
        # no numérico o no finito descarta la pareja entera.
        notch_x = cls._pixel(data.get("notch_x"))
        notch_y = cls._pixel(data.get("notch_y"))
        if notch_x is None or notch_y is None:
            cfg.notch_x = None
            cfg.notch_y = None
        else:
            cfg.notch_x = notch_x
            cfg.notch_y = notch_y
        try:
            vad_version = int(data.get("vad_defaults_version", 1))
        except (TypeError, ValueError):
            vad_version = 1
        if data and vad_version < 2:
            # Migración de los valores que venían de fábrica: el 0,0015 era
            # demasiado sordo para micrófonos de ganancia baja y 0,3 s
            # descartaba órdenes cortas.
            if cfg.min_rms == 0.0015:
                cfg.min_rms = 0.0006
            if cfg.min_speech_secs == 0.3:
                cfg.min_speech_secs = 0.2
            cfg.vad_defaults_version = 2
        cfg.apply_env()
        return cfg

    def apply_env(self) -> None:
        if self.llm_pinned:
            # Una elección explícita del menú no debe ser pisada por el .env
            # en el siguiente arranque. La clave se recupera del perfil.
            try:
                resolved = resolve_llm(
                    provider=self.llm_provider,
                    model=self.llm_model,
                    base_url=self.llm_base_url,
                    api_key=self.api_key or None,
                )
            except ValueError:
                resolved = None
            if resolved is not None:
                self.llm_provider = resolved.provider
                self.llm_model = resolved.model
                self.llm_base_url = resolved.base_url
                self.api_key = resolved.api_key
        else:
            resolved = resolve_llm()
            # .env y llm.json mandan sobre los valores antiguos de voice.json.
            if resolved.source in ("env", "file"):
                self.llm_provider = resolved.provider
                self.llm_model = resolved.model
                self.llm_base_url = resolved.base_url
                self.api_key = resolved.api_key
            elif not self.api_key:
                self.api_key = resolved.api_key
        self.whisper_url = os.getenv("WHISPER_URL") or self.whisper_url
        self.stt_engine = os.getenv("WHISPER_ENGINE") or self.stt_engine
        if self.output_device_index is None:
            raw = os.getenv("EZ_OUTPUT_DEVICE_INDEX")
            if raw:
                try:
                    self.output_device_index = int(raw)
                except ValueError:
                    pass
        if not self.default_cwd:
            self.default_cwd = os.getcwd()

    @staticmethod
    def _pixel(value) -> int | None:
        """Normaliza una coordenada guardada; None si no es un número válido."""
        if value is None or isinstance(value, bool):
            return None
        try:
            number = float(value)
        except (TypeError, ValueError):
            return None
        if not math.isfinite(number):
            return None
        return int(number)

    @staticmethod
    def _read_json(path: Path) -> dict:
        if path.exists():
            try:
                loaded = json.loads(path.read_text(encoding="utf-8"))
                if isinstance(loaded, dict):
                    return loaded
            except Exception:
                pass
        return {}

    @staticmethod
    def _write_json(path: Path, data: dict) -> None:
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

    def save(self, path: Path | None = None) -> None:
        """Escritura atómica y con merge de claves desconocidas.

        Se conserva cualquier clave que este proceso no conozca (por ejemplo,
        una versión futura o un ajuste manual) y solo se actualizan los campos
        del dataclass. El archivo temporal vive en la misma carpeta y se
        reemplaza con ``os.replace`` para no dejar un JSON a medias.
        """
        path = path or config_path()
        data = self._read_json(path)
        data.update(asdict(self))
        # La clave vive en .env; no se duplica en voice.json.
        data["api_key"] = ""
        self._write_json(path, data)

    def save_key(self, key: str, value, path: Path | None = None) -> None:
        """Persiste una sola clave instantánea (notch_hidden o theme).

        No vuelca el resto del dataclass: así ocultar la isla no guarda por
        accidente cambios del menú que aún no se han confirmado (spec 10.1).
        """
        path = path or config_path()
        data = self._read_json(path)
        data[key] = value
        self._write_json(path, data)

    def save_position(self, x: int, y: int, path: Path | None = None) -> None:
        """Persiste solo la posición del notch, sin volcar el dataclass.

        Se conserva el resto de claves del archivo (incluidas las que esta
        versión no conoce) y los ajustes en vivo que aún no se han confirmado.
        """
        path = path or config_path()
        data = self._read_json(path)
        data["notch_x"] = int(x)
        data["notch_y"] = int(y)
        self._write_json(path, data)
        self.notch_x = int(x)
        self.notch_y = int(y)
