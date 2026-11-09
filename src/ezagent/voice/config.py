"""Configuración de `ezagent-voice` (mensajero de voz para Herdr)."""

from __future__ import annotations

import copy
import json
import os
import re
import unicodedata
from dataclasses import asdict, dataclass, field
from pathlib import Path

from ezagent.llm_providers import OPENCODE_BASE_URL, resolve_llm

DEFAULT_AGENTS: dict[str, list[str]] = {
    "claude": ["claude"],
    "opencode": ["opencode"],
    "mcode": ["mcode"],
    "minimax": ["mcode"],
    "codex": ["codex"],
    "pi": ["pi"],
    "qwen": ["qwen"],
}

# Versión de los perfiles de fábrica. Al subirla, ``load`` refresca en
# voice.json los campos de conocimiento del CLI (flags, esfuerzos, alias,
# comandos de sesión) sin tocar lo que el usuario eligió (command, modelos).
AGENT_PROFILES_VERSION = 8
_PROFILE_KNOWLEDGE_KEYS = (
    "label",
    "aliases",
    "models",
    "efforts",
    "model_efforts",
    "prelaunch",
    "discover",
    "model_command",  # obsoleto: se elimina al migrar
    "model_args",
    "effort_args",
    "model_prefix",
    "env",
    "env_args",
    "task_args",  # obsoleto
    "session_commands",
    "notes",
)

# Perfiles de lanzamiento por agente. El usuario puede editarlos en
# voice.json. Un agente se abre como lo haría una persona: terminal, comando,
# esperar a que arranque, modelo/esfuerzo y, al final, la tarea. Campos:
# - command: argv que se escribe en la terminal.
# - model_args / effort_args: flags con {model}/{effort} que se añaden al
#   comando. Lista vacía = el CLI no los acepta como flag.
# - model_prefix: se antepone al modelo si no trae «proveedor/».
# - "{variant}" dentro de model_args pega el esfuerzo al modelo (modelo#nivel).
# - env / env_args: variables de entorno (se fijan en la shell antes del
#   comando) y flags extra, para CLIs cuyo TUI no acepta --model. Admiten
#   {model_ref} (modelo con prefijo) y {variant} (#esfuerzo o vacío).
# - session_commands: lo que se escribe DENTRO del agente ya abierto, línea a
#   línea (Enter tras cada una), para fijar modelo/esfuerzo cuando no hay flag
#   o para cambiarlos en caliente. Admite {model}, {model_search} (el modelo
#   con espacios, para buscadores) y {effort}. Cada paso es un texto o un dict:
#   {"send": texto} | {"keys": [teclas]} | {"cycle": {"keys", "until", "max"}}
#   con "expect"/"fail" (regex que se buscan en la pantalla tras el paso),
#   "message", "optional" (si falla se avisa y se sigue) y "soft" (solo avisa).
# - aliases: cómo suena el nombre al dictarlo («open code», «cloud code»).
# - notes: contexto del CLI que ve el LLM (nunca se ejecuta).
DEFAULT_AGENT_PROFILES: dict[str, dict[str, object]] = {
    "claude": {
        "label": "Claude Code",
        "aliases": ["claude code", "cloud code", "clod code", "clode"],
        "models": ["fable", "opus", "sonnet", "haiku"],
        "efforts": ["low", "medium", "high", "xhigh", "max"],
        "default_model": "",
        "default_effort": "",
        "model_args": ["--model", "{model}"],
        "effort_args": ["--effort", "{effort}"],
        "session_commands": {
            "model": [{"send": "/model {model}", "soft": True}],
            "effort": [{"send": "/effort {effort}", "soft": True}],
        },
        "notes": (
            "Modelos por alias (fable, opus, sonnet, haiku). Admite plan "
            "mode y subagentes propios; bueno para planificar y auditar."
        ),
    },
    "opencode": {
        "label": "OpenCode",
        "aliases": ["open code", "open-code", "opencoder", "opencod"],
        "models": [
            "deepseek-v4.1-flash",
            "deepseek-v4-pro",
            "glm-5.3",
            "kimi-k3",
            "minimax-m3.1-flash",
            "claude-haiku-5-5",
        ],
        "efforts": [],
        # Variantes reales de cada modelo (GET /api/model de OpenCode). Una
        # variante que el modelo no tiene OpenCode la descarta en silencio.
        "model_efforts": {
            "deepseek-v4.1-flash": ["low", "high", "max"],
            "deepseek-v4-pro": ["high", "max"],
            "glm-5.3": ["low", "high", "max"],
            "kimi-k3": ["max"],
            "minimax-m3.1-flash": ["low", "medium", "high", "xhigh", "max"],
            "claude-haiku-5-5": ["none", "low", "medium", "high", "xhigh", "max"],
        },
        # OpenCode recuerda la última variante por modelo y esa gana a la
        # configuración: se le da un estado propio con la variante ya escrita.
        "prelaunch": "opencode_state",
        "default_model": "",
        "default_effort": "",
        # El TUI «opencode» rechaza --model y --effort: se fijan dentro, con
        # /models (selector con buscador) y /effort (alias de /variants).
        "model_args": [],
        "effort_args": [],
        "model_prefix": "opencode-go/",
        # Lanzamiento directo: OPENCODE_CONFIG_CONTENT con «proveedor/modelo#nivel»
        # (verificado con «opencode debug config --standalone»); el servidor
        # privado --standalone es el que lee esa variable. Los comandos de
        # sesión de abajo solo se usan para cambiar algo con el agente abierto.
        "env": {"OPENCODE_CONFIG_CONTENT": '{"model":"{model_ref}{variant}"}'},
        "env_args": ["--standalone"],
        "session_commands": {
            "model": [
                {
                    "send": "/models",
                    "expect": "Select model",
                    "message": "no se abrió el selector de modelos",
                },
                {
                    "send": "{model_search}",
                    "expect": "Switched model to",
                    "message": "OpenCode no encontró el modelo {model}",
                },
            ],
            "effort": [
                {
                    "send": "/effort",
                    "expect": "Select variant",
                    "fail": "No variants available|does not support any variants",
                    "message": "el modelo actual de OpenCode no tiene niveles de esfuerzo",
                    "optional": True,
                },
                {"keys": ["esc"]},
                {
                    "cycle": {
                        "keys": ["ctrl+t"],
                        "until": "Switched variant to {effort}",
                        "max": 8,
                    },
                    "message": "OpenCode no tiene el nivel {effort} para este modelo",
                    "optional": True,
                },
            ],
        },
        "notes": (
            "Modelo y esfuerzo se fijan al lanzar (proveedor/modelo#nivel, "
            "p. ej. opencode-go/deepseek-v4.1-flash#high); el nivel debe "
            "existir en ese modelo. action=guide trae el manual."
        ),
    },
    "mcode": {
        "label": "MiniMax Code",
        "aliases": ["m code", "minimax code", "mini max code", "emcode"],
        "models": [],
        "efforts": ["default", "low", "medium", "high", "xhigh", "max"],
        "default_model": "",
        "default_effort": "",
        "model_args": ["--model", "{model}"],
        "effort_args": [],
        # El TUI de mcode no tiene flag de esfuerzo: lo lee de su config.yaml
        # (defaultModelThinking.effort), que se fija antes de abrirlo. Los
        # modelos se leen de ese mismo archivo (distingue mayúsculas).
        "prelaunch": "mcode_effort",
        "discover": "mcode",
        "session_commands": {},
        "notes": (
            "Modelo como proveedor/modelo (se resuelve contra su config); el "
            "esfuerzo se escribe en su config.yaml antes de abrir y queda como "
            "su valor por defecto. action=guide trae el manual."
        ),
    },
    "minimax": {
        "label": "MiniMax (mcode)",
        "aliases": ["mini max"],
        "models": [],
        "efforts": ["default", "low", "medium", "high", "xhigh", "max"],
        "default_model": "",
        "default_effort": "",
        "model_args": ["--model", "{model}"],
        "effort_args": [],
        "prelaunch": "mcode_effort",
        "discover": "mcode",
        "session_commands": {},
        "notes": "Alias de mcode.",
    },
    "codex": {
        "label": "Codex",
        "aliases": ["codex cli", "kodex", "codecs", "códex"],
        "models": [],
        "efforts": ["low", "medium", "high", "xhigh", "max", "ultra"],
        "default_model": "",
        "default_effort": "",
        "model_args": ["--model", "{model}"],
        "effort_args": ["-c", "model_reasoning_effort={effort}"],
        # Modelos y niveles por modelo salen del catálogo que Codex descarga
        # con la cuenta (~/.codex/models_cache.json).
        "discover": "codex",
        "session_commands": {},
        "notes": (
            "Modelo y esfuerzo se fijan al lanzar; dentro de la sesión /model "
            "abre un selector que no se puede dictar."
        ),
    },
    "qwen": {
        "label": "Qwen Code",
        "aliases": ["qwen code", "quen code", "qwen coder", "cuen"],
        "models": ["qwen3-coder-plus"],
        "efforts": [],
        "default_model": "",
        "default_effort": "",
        "model_args": ["--model", "{model}"],
        "effort_args": [],
        "session_commands": {},
        "notes": (
            "El modelo se fija al lanzar con --model; no tiene flag de "
            "esfuerzo. action=guide trae el manual."
        ),
    },
    "pi": {
        "label": "Pi",
        "aliases": ["pi agent", "pi coding agent", "pie"],
        "models": [],
        "efforts": ["off", "minimal", "low", "medium", "high", "xhigh", "max"],
        "default_model": "",
        "default_effort": "",
        "model_args": ["--model", "{model}"],
        "effort_args": ["--thinking", "{effort}"],
        "session_commands": {},
        "notes": (
            "Modelo como proveedor/id (admite sufijo :nivel de razonamiento). "
            "El esfuerzo es el nivel de thinking."
        ),
    },
}


def normalize_agent_name(value: str) -> str:
    """Forma comparable de un nombre dictado: sin acentos ni separadores."""
    decomposed = unicodedata.normalize("NFKD", str(value or ""))
    plain = "".join(c for c in decomposed if not unicodedata.combining(c))
    return re.sub(r"[^a-z0-9]", "", plain.lower())


def resolve_agent_key(
    name: str,
    agents: dict[str, list[str]],
    profiles: dict[str, dict[str, object]] | None = None,
) -> str | None:
    """Devuelve la clave canónica de un agente a partir de cómo se dictó."""
    wanted = normalize_agent_name(name)
    if not wanted:
        return None
    for key in agents:
        if normalize_agent_name(key) == wanted:
            return key
    for key, profile in (profiles or {}).items():
        if key not in agents or not isinstance(profile, dict):
            continue
        aliases = profile.get("aliases")
        if not isinstance(aliases, list):
            continue
        if wanted in {normalize_agent_name(str(a)) for a in aliases}:
            return key
    return None


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
    # Atajos globales (formato alt+q, ctrl+alt+space...).
    # Alt+Q/E/R quedan libres para otras apps.
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
    # Carpeta con whisper-server.exe y models/ (vacío = autodetección).
    whisper_home: str = ""
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
    # Segundos de voz por encima de barge_in_rms para interrumpir al asistente: un
    # clic o un golpe de teclado no llegan; una frase sí.
    barge_in_secs: float = 0.25
    # Fin de turno con Smart Turn: tras turn_quick_secs de silencio, si el
    # modelo ve la frase completa, el turno se cierra ya; si no, se espera
    # silence_secs como siempre. No toca la interrupción (barge-in).
    smart_turn: bool = True
    turn_quick_secs: float = 0.35
    turn_threshold: float = 0.5
    # Versión de los valores por defecto del VAD. La 2 baja el suelo de
    # arranque para voces flojas y acepta frases cortas.
    vad_defaults_version: int = 2

    # LLM.
    llm_provider: str = "opencode"
    llm_model: str = "deepseek-v4.1-flash"
    llm_base_url: str = OPENCODE_BASE_URL
    api_key: str = ""
    # Formato de la API: «openai» (Chat Completions) o «anthropic» (Messages).
    # Vacío = el del perfil guardado o el del preset.
    llm_api_style: str = ""
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
    agent_profiles_version: int = AGENT_PROFILES_VERSION
    announce_all: bool = False
    max_read_lines: int = 40
    notch_hidden: bool = False

    # La barra guarda su propia posición física al terminar un arrastre.
    drop_x: int | None = None
    drop_y: int | None = None
    drop_layout: str = ""      # disposición con la que se guardó drop_x/drop_y
    # Forma de la barra: «pill» (cápsula) o «circle» (círculo sobre los botones).
    # Los colores siguen al tema (claro u oscuro) y los botones solo aparecen al
    # acercar el puntero; «ui_palette» y «ui_buttons» antiguos se ignoran al cargar.
    ui_layout: str = "pill"
    # Movimiento de la barra: «system» (sigue a Windows), «full» o «reduced».
    ui_motion: str = "system"

    # Escucha: qué hace «cierra la escucha» («pause» deja el micro en pausa y
    # solo atiende a la frase de reactivación; «hangup» cuelga la llamada).
    close_action: str = "pause"
    # En pausa, la frase de reactivación vuelve a abrir la escucha. Apagado,
    # la pausa es un silencio total y solo el botón la quita.
    voice_wake: bool = True
    wake_phrase: str = "EzAgent, escúchame"

    # Tema del notch: auto (sigue a Windows), dark, light o custom. Se guarda al
    # instante y no cuenta para «cambios sin guardar».
    theme: str = "auto"
    # Tema personalizado: los cuatro colores del gradiente (Marea por defecto)
    # y si la interfaz (ventana, botones) es clara u oscura.
    theme_base: str = "dark"
    theme_primary: str = "#1FAE9F"
    theme_secondary: str = "#8FE3C0"
    theme_accent: str = "#FF8A65"
    theme_background: str = "#0E6F7C"

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
        # DECISION: voice.json guarda `agents` entero; sin esto un agente
        # añadido en una versión nueva (pi, qwen) nunca aparecería.
        for name, command in DEFAULT_AGENTS.items():
            if isinstance(cfg.agents, dict):
                cfg.agents.setdefault(name, list(command))
        if data and cls._int(data.get("agent_profiles_version"), 1) < AGENT_PROFILES_VERSION:
            cfg._refresh_profile_knowledge()
        cfg.agent_profiles_version = AGENT_PROFILES_VERSION
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
                    api_style=self.llm_api_style or None,
                )
            except ValueError:
                resolved = None
            if resolved is not None:
                self.llm_provider = resolved.provider
                self.llm_model = resolved.model
                self.llm_base_url = resolved.base_url
                self.api_key = resolved.api_key
                self.llm_api_style = resolved.api_style
        else:
            resolved = resolve_llm()
            # .env y llm.json mandan sobre los valores antiguos de voice.json.
            if resolved.source in ("env", "file"):
                self.llm_provider = resolved.provider
                self.llm_model = resolved.model
                self.llm_base_url = resolved.base_url
                self.api_key = resolved.api_key
                self.llm_api_style = resolved.api_style
            elif not self.api_key:
                self.api_key = resolved.api_key
            if not self.llm_api_style:
                self.llm_api_style = resolved.api_style
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
    def _int(value, default: int) -> int:
        try:
            return int(value)
        except (TypeError, ValueError):
            return default

    def _refresh_profile_knowledge(self) -> None:
        """Pone al día los campos de conocimiento de los perfiles guardados.

        voice.json se guarda entero, así que un perfil viejo (por ejemplo, el de
        OpenCode con --effort, que ese CLI rechaza) taparía los valores nuevos.
        Solo se pisan los campos de ``_PROFILE_KNOWLEDGE_KEYS``.
        """
        for name, default in DEFAULT_AGENT_PROFILES.items():
            profile = self.agent_profiles.setdefault(name, {})
            for key in _PROFILE_KNOWLEDGE_KEYS:
                if key in default:
                    profile[key] = copy.deepcopy(default[key])
                else:
                    profile.pop(key, None)


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

        No vuelca el resto del dataclass: así ocultar la barra no guarda por
        accidente cambios del menú que aún no se han confirmado (spec 10.1).
        """
        path = path or config_path()
        data = self._read_json(path)
        data[key] = value
        self._write_json(path, data)
