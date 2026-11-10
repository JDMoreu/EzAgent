"""`ezagent-voice`: mensajero de voz para Herdr en modo llamada.

- Ctrl+Alt+Space: iniciar / colgar la llamada.
- La barra queda visible durante toda la llamada.
- El micrófono sigue escuchando entre turnos; el VAD usa pre-roll y un
  silencio de cierre largo para no cortar al usuario.
- Si hablas mientras el asistente responde, se corta el TTS y se te escucha.
- whisper.cpp -> OpenCode DeepSeek 4.1 Flash + tools de Herdr -> Kokoro.
"""

from __future__ import annotations

import argparse
import math
import os
import queue
import re
import signal
import socket
import subprocess
import sys
import threading
import time
import tkinter as tk
import unicodedata
import uuid
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit

from dotenv import load_dotenv

from ezagent.agent.handoff import answer_or_tail, compose_handoff
from ezagent.agent.handoff_packet import Packet, build_packet, packet_text
from ezagent.agent.orchestration import chain_spec
from ezagent.llm_providers import (
    fetch_models,
    load_provider_catalog,
    match_model,
    needs_api_key,
    normalize_provider,
    provider_models,
    provider_spec,
    save_provider_profile,
)

from .audio import MicCapture, list_input_devices, pcm_is_weak, pcm_to_wav
from .chat.contexts import ChatContexts
from .chat.session import ChatModel
from .chat.speech import spoken_version
from .chat.store import ChatStore
from .condenser import Condenser
from .config import VoiceConfig, config_path
from .herdr import Herdr
from .hotkeys import GlobalHotkey, parse_hotkey
from .llm import Messenger, friendly_llm_error
from .metrics import LatencyStats, TurnTimings, format_stt
from .notch.drop import create_ui
from .speech_queue import SpeechQueue
from .stt import (
    STTError,
    available_whisper_models,
    is_hallucination,
    transcribe,
    whisper_model_path,
)
from .tasks import TaskRegistry
from .tts import Speaker
from .turn import SmartTurn
from .wake import DEFAULT_WAKE_PHRASE, is_wake_phrase, worth_transcribing
from .watcher import Watcher

IDLE = "idle"
LISTENING = "listening"
THINKING = "thinking"
SPEAKING = "speaking"

HOTKEY_KEYS = (
    "hotkey",
    "cancel_hotkey",
    "repeat_hotkey",
    "mute_hotkey",
    "menu_hotkey",
    "chat_hotkey",
)

# Apariencia y escucha: el menú los aplica y guarda al instante (sin «Guardar»).
# Debe coincidir con notch.settings.INSTANT_KEYS (lo comprueba una prueba).
INSTANT_SETTINGS = (
    "ui_layout",
    "theme",
    "theme_base",
    "theme_primary",
    "theme_secondary",
    "theme_accent",
    "theme_background",
    "ui_motion",
    "ui_scale",
    "show_advanced",
    "close_action",
    "voice_wake",
    "wake_phrase",
    "chat_voice",
)


def _hotkey_key(spec: str) -> str:
    """Normaliza un atajo para comparar duplicados."""
    return "+".join(part.strip().lower() for part in str(spec or "").split("+") if part.strip())


MENU_VOICES = (
    ("em_alex", "Álex · masculina"),
    ("ef_dora", "Dora · femenina"),
    ("em_santa", "Santa · masculina grave"),
)


class _Sentences:
    """Acumula tokens del LLM y los corta en frases para el TTS."""

    def __init__(self, max_chars: int = 160, first_chars: int = 40):
        self.buf = ""
        self.max_chars = max_chars
        self.first_chars = first_chars
        self._first = True

    def _emit_first_clause(self) -> str | None:
        """Primer trozo corto: una cláusula o, como máximo, ``first_chars``."""
        if not self._first or len(self.buf) < 12:
            return None
        clause = re.search(r"[,;:](?=\s)", self.buf)
        if clause is not None and clause.start() >= 8:
            end = clause.start() + 1
        elif len(self.buf) >= self.first_chars:
            end = self.buf.rfind(" ", 0, self.first_chars)
            if end <= 0:
                return None
        else:
            return None
        sentence = self.buf[:end].strip()
        self.buf = self.buf[end:].lstrip()
        self._first = False
        return sentence or None

    def feed(self, delta: str) -> list[str]:
        self.buf += delta
        out: list[str] = []
        first = self._emit_first_clause()
        if first:
            out.append(first)
        while True:
            match = re.search(r"[.!?](?=\s|$)", self.buf)
            if not match:
                break
            end = match.end()
            sentence = self.buf[:end].strip()
            self.buf = self.buf[end:].lstrip()
            if sentence:
                out.append(sentence)
                self._first = False
        if len(self.buf) > self.max_chars:
            cut = self.buf.rfind(" ", 0, self.max_chars)
            if cut > 0:
                out.append(self.buf[:cut].strip())
                self.buf = self.buf[cut:].lstrip()
                self._first = False
        return out

    def flush(self) -> list[str]:
        text = self.buf.strip()
        self.buf = ""
        return [text] if text else []


# Ventana tras cerrar un turno en la que, si el usuario vuelve a hablar
# mientras el modelo piensa, se une la frase nueva con la anterior.
RESUME_SECS = 3.0

# Cortesías que se quitan al principio antes de buscar un atajo local:
# «Ok, gracias. Mutéate» debe silenciar igual que «mutéate».
_COURTESY_WORDS = frozenset(
    {
        "ok",
        "okay",
        "oki",
        "bueno",
        "listo",
        "perfecto",
        "dale",
        "genial",
        "gracias",
        "muchas",
        "mil",
        "eh",
        "este",
        "pues",
        "va",
    }
)


def _strip_courtesy(key: str) -> str:
    """Quita cortesías iniciales; si no queda nada, devuelve ``key`` tal cual."""
    words = key.split()
    while words and words[0] in _COURTESY_WORDS:
        words.pop(0)
    return " ".join(words) or key


def log(message: str) -> None:
    print(f"[ezagent-voice] {message}", flush=True)


def _timed_stream(stream, timing: TurnTimings | None):
    """Entrega los trozos del LLM sumando solo el tiempo que se espera por ellos.

    Mientras el consumidor habla una frase el generador queda en pausa, así que
    ese tiempo de voz no cuenta como tiempo del LLM.
    """
    iterator = iter(stream)
    while True:
        started = time.monotonic()
        try:
            item = next(iterator)
        except StopIteration:
            if timing is not None:
                timing.add_span("llm", time.monotonic() - started)
            return
        if timing is not None:
            timing.add_span("llm", time.monotonic() - started)
        yield item


def _resource_root() -> Path:
    """Raíz de recursos; en PyInstaller apunta a ``sys._MEIPASS``."""
    bundle = getattr(sys, "_MEIPASS", None)
    if bundle:
        return Path(bundle)
    return Path(__file__).resolve().parents[3]


def _free_local_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def _whisper_ready(url: str, timeout: float = 1.0) -> bool:
    parts = urlsplit(url)
    host = parts.hostname
    port = parts.port or (443 if parts.scheme == "https" else 80)
    if not host:
        return False
    try:
        with socket.create_connection((host, port), timeout=timeout):
            return True
    except OSError:
        return False


def ensure_whisper(cfg: VoiceConfig, wait_secs: float = 25.0) -> subprocess.Popen | None:
    """Si whisper-server no responde, lo arranca y devuelve el proceso dueño."""
    if _whisper_ready(cfg.whisper_url):
        return None
    if not cfg.auto_start_whisper:
        return None
    parts = urlsplit(cfg.whisper_url)
    if parts.hostname not in {"127.0.0.1", "localhost", "::1"}:
        return None
    script = _resource_root() / "scripts" / "start-whisper.ps1"
    if not script.exists():
        log(f"No encuentro {script}; arranca whisper-server a mano.")
        return None
    try:
        from .stt import whisper_home

        home = whisper_home()
    except Exception:
        home = None
    model_path = whisper_model_path(cfg.stt_model)
    has_server = home is not None and any(
        (home / sub / "whisper-server.exe").is_file()
        for sub in ("", "whisper_vulkan", "vulkan", "cpu")
    )
    if not has_server and model_path is None:
        # DECISION: app.py usa log() propio (sin niveles); el aviso hablado lo
        # emite la capa que trata el estado "error" del notch.
        log("No encuentro whisper. Descárgalo con: uv run python -m ezagent.core.downloads --all")
        return None
    port = _free_local_port()
    cfg.whisper_url = urlunsplit(
        (parts.scheme or "http", f"127.0.0.1:{port}", parts.path, parts.query, "")
    )
    log(f"whisper-server: puerto local libre {port}.")
    log("whisper-server no responde; arrancándolo…")
    try:
        command = [
            "powershell",
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            str(script),
            "-Port",
            str(port),
            "-Engine",
            str(cfg.stt_engine or "auto"),
        ]
        if home is not None:
            command.extend(["-WhisperHome", str(home)])
        if model_path is not None:
            command.extend(["-Model", str(model_path)])
        proc = subprocess.Popen(
            command,
            cwd=str(script.parent.parent),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
        )
    except OSError as exc:
        log(f"No se pudo arrancar whisper-server: {exc}")
        return None
    deadline = time.monotonic() + wait_secs
    while time.monotonic() < deadline:
        if _whisper_ready(cfg.whisper_url):
            log("whisper-server listo.")
            return proc
        time.sleep(0.5)
    log("whisper-server no respondió; revisa scripts\\start-whisper.ps1.")
    return proc


class VoiceApp:
    def __init__(
        self,
        cfg: VoiceConfig,
        tasks_path: Path | None = None,
        config_file: Path | None = None,
    ):
        self.cfg = cfg
        self.config_file = Path(config_file or config_path())
        self.state = IDLE
        self.in_call = False
        self.muted = False
        self.tasks = TaskRegistry(tasks_path or config_path().with_name("tasks.json"))
        self._user_speaking = False
        self._ignore_utterance = False
        # Unión de frases (hilo de audio): si el turno se cerró en una pausa y
        # el usuario sigue hablando mientras el modelo piensa, la frase nueva
        # se transcribe junto con la anterior en vez de perder la primera.
        self._last_turn_audio: tuple[bytes, int, float] | None = None
        self._resume_pcm: tuple[bytes, int] | None = None
        self.last_summary = ""
        # El chat escrito comparte hilo con la voz: todo lo que se dice o escribe entra aquí.
        self.chat = ChatModel(ChatStore(), voice_replies=bool(getattr(cfg, "chat_voice", False)))
        # Cada conversación del chat conserva su propio contexto del modelo.
        self.contexts = ChatContexts(self.chat, lambda: getattr(self, "messenger", None))
        self.mute_until = 0.0
        self.watched: set[str] = set()
        self._deliver: set[str] = set()
        self._prelaunch: dict[str, dict] = {}
        self._prelaunch_lock = threading.Lock()
        # Avisos de agentes en orden: nunca se pisan entre sí.
        self._announce_queue: list[tuple[str, bool]] = []
        self._close_after_answer = False
        self._close_mode = ""  # "", "pause" o "hangup" ("" = ajuste)
        self._wake_lock = threading.Lock()
        self._reset_capture = threading.Event()
        self._post_tts_until = 0.0
        self._post_tts_ignore = False
        self._call_stop: threading.Event | None = None
        self._call_thread: threading.Thread | None = None
        self._tts_stop: threading.Event | None = None
        # Un único lock protege la reasignación del evento de parada.
        self._tts_lock = threading.Lock()
        self._speaking_kind: str | None = None
        self._turn_gen = 0
        self._turn_started = 0.0
        self._turn_timing: TurnTimings | None = None
        self._latency = LatencyStats(jsonl_path=config_path().parent / "logs" / "latency.jsonl")
        self._notch_hidden = bool(cfg.notch_hidden)
        self._restarting = False
        self._shutdown_done = False
        self.tray = None
        self.events: queue.Queue[tuple] = queue.Queue()
        self.hotkey_events: queue.Queue[str] = queue.Queue()
        # El menú del notch corre en el hilo de su ventana Win32: nunca toca
        # Tk, PyAudio ni disco directamente. Publica intenciones aquí y el
        # hilo principal las aplica en _tick.
        self.menu_events: queue.Queue[tuple] = queue.Queue()
        self._devices_cache: list[dict] = []
        self._menu_saved: dict | None = None
        self._saved_llm = (
            cfg.llm_provider,
            cfg.llm_model,
            cfg.llm_base_url,
            cfg.api_key,
            cfg.llm_api_style,
        )
        # Lista real de modelos por proveedor (GET /models), pedida en un hilo.
        self._llm_models_cache: dict[str, tuple[str, ...]] = {}
        self._llm_models_lock = threading.Lock()
        self._llm_models_tried: set[tuple[str, str, str]] = set()
        self._saved_stt_model = cfg.stt_model
        self._saved_stt_engine = cfg.stt_engine

        self.root = tk.Tk()
        self.root.withdraw()
        self.notch = create_ui(self.root, cfg)
        for sig in (signal.SIGINT, signal.SIGTERM):
            try:
                signal.signal(sig, lambda *_: self.root.after(0, self.root.quit))
            except Exception:
                pass

        self._whisper_proc = ensure_whisper(cfg)
        if not _whisper_ready(cfg.whisper_url):
            # DECISION: sin whisper la app arranca igual; el aviso hablado usa
            # el canal de errores que ya existe y el notch queda en error.
            self.notch.set_state("error")
            self.events.put(
                (
                    "error",
                    "No encuentro whisper. Revisa la carpeta de whisper en la configuración.",
                )
            )
        log("Cargando Kokoro…")
        self.speaker = Speaker(cfg)
        threading.Thread(target=self._warm_tts, daemon=True).start()
        threading.Thread(target=self._warm_stt, daemon=True).start()
        self.mic = MicCapture(
            device_index=cfg.input_device_index,
            min_rms=cfg.min_rms,
            silence_secs=cfg.silence_secs,
            pre_roll_secs=cfg.pre_roll_secs,
            min_speech_secs=cfg.min_speech_secs,
            max_utterance_secs=cfg.max_utterance_secs,
            noise_multiplier=cfg.noise_multiplier,
            barge_in_rms=cfg.barge_in_rms,
            barge_in_secs=cfg.barge_in_secs,
        )
        log(
            f"Micrófono: {self.mic.device_name} (índice {self.mic.device_index}, "
            f"{self.mic.rate} Hz)"
        )
        self.turn_detector = self._create_turn_detector(cfg)
        turn = (
            f"fin de turno a {cfg.turn_quick_secs}s si la frase está completa"
            if self.turn_detector is not None
            else "sin Smart Turn"
        )
        log(
            f"VAD: silencio={cfg.silence_secs}s, {turn}, min_rms={cfg.min_rms}, "
            f"pre_roll={cfg.pre_roll_secs}s, barge_in={cfg.barge_in}"
        )
        self.herdr = Herdr(cfg)
        # Si Herdr o `hc` faltan, se instalan solos en segundo plano; el
        # primer comando que los necesite espera a que termine el instalador.
        threading.Thread(
            target=self.herdr.ensure_installed,
            daemon=True,
            name="ez-herdr-install",
        ).start()
        self.agent = self._create_agent_runtime(cfg)
        self.messenger = Messenger(
            cfg,
            self.herdr,
            last_summary=lambda: self.last_summary,
            watch=self._watch,
            mute=self._mute,
            request_close=self._request_close,
            mic_mute=self._llm_mic_mute,
            runtime=self.agent,
        )
        # Cuando la conversación se compacta, el chat lo cuenta con una nota.
        self.messenger.on_context_event = lambda text: self.chat.add_note(text)
        if self.chat.snapshot()[1]:  # la conversación que se ve ya tenía mensajes
            self.contexts.rebuild()  # el modelo la recuerda aunque se reinició la app
        # Lista real de modelos del proveedor activo: llena el menú y corrige
        # un id guardado sin prefijo antes del primer turno.
        self._fetch_llm_models(
            self.cfg.llm_provider,
            self.cfg.llm_base_url,
            self.cfg.api_key,
            self.cfg.llm_api_style,
            force=False,
        )
        self.watcher = Watcher(
            cfg,
            on_event=self._on_watcher_event,
            should_announce=self._should_announce,
        )

        self.hotkeys = self._build_hotkeys()
        for hotkey in self.hotkeys:
            hotkey.start()
        self.notch.on_action = self._on_notch_action
        self.notch.on_menu_action = lambda action: self.events.put(("menu_action", action))
        self.notch.on_hotkey_capture = lambda active: self.events.put(
            ("hotkey_capture", bool(active))
        )
        self.notch.menu_settings_provider = self._notch_menu_settings
        self.notch.chat_model = self.chat
        self.notch.chat_hotkey = cfg.chat_hotkey
        if cfg.chat_width > 0 and cfg.chat_height > 0:
            self.notch.chat_size = (float(cfg.chat_width), float(cfg.chat_height))
        self.notch.chat_roots, self.notch.chat_base_dirs = self._chat_media_roots()
        self.notch.on_chat_new = lambda: self.events.put(("chat_new",))
        self.notch.on_chat_stop = lambda: self.events.put(("chat_stop",))
        self.notch.on_chat_switch = lambda session: self.events.put(("chat_switch", int(session)))
        self.notch.on_chat_delete = lambda session: self.events.put(("chat_delete", int(session)))
        self.notch.on_chat_confirm = lambda yes: self.events.put(("confirm_click", bool(yes)))
        self.notch.on_chat_send = lambda text: self.events.put(("chat_send", text))
        self.notch.on_chat_voice = self._set_chat_voice
        self.notch.on_chat_mic = lambda muted: self.events.put(("chat_mute", bool(muted)))
        self.notch.on_chat_menu = lambda: self.events.put(("notch_action", "panel"))
        self.notch.on_live_setting = self._notch_live_setting
        self.notch.on_save_settings = self._notch_save_settings
        self.notch.on_test_voice = self._notch_test_voice
        self.notch.on_hide = self._notch_hide
        try:
            self._push_custom_colors()
            self.notch.set_custom_base(self.cfg.theme_base)
            self.notch.set_theme(self.cfg.theme)
        except Exception:
            pass
        threading.Thread(target=self._refresh_devices, daemon=True).start()
        self.notch.set_in_call(False)
        if not self._notch_hidden:
            self.notch.show(IDLE)
        self.root.after(120, self._report_hotkeys)
        self.root.after(30, self._tick)
        self.tray = self._create_tray()
        self._update_tray_title()

    def _create_agent_runtime(self, cfg: VoiceConfig):
        """Crea el núcleo agéntico y lo deja caer con elegancia si falla.

        El pipeline de voz sigue funcionando con el mensajero antiguo si el
        paquete `ezagent.agent` no está disponible o permissions.toml es
        ilegible; el fallo queda en el log, no tumba la app.
        """
        try:
            from ezagent.agent.runtime import AgentRuntime, RuntimeCallbacks

            callbacks = RuntimeCallbacks(
                watch=self._watch,
                request_close=self._request_close,
                mic_mute=self._llm_mic_mute,
                last_summary=lambda: self.last_summary,
                mute=self._mute,
                get_last_user_text=lambda: (
                    getattr(self.messenger, "_last_user_text", "")
                    if hasattr(self, "messenger")
                    else ""
                ),
                remember_task=self._remember_task,
                enqueue_chain=self._enqueue_chain,
                deliver_answer=self._deliver_answer,
                prewarm_chain=self._start_prelaunch,
                async_launch=True,
                pending_tasks=lambda: self.tasks.prompt_block(),
                on_confirm_ask=lambda summary, strong: self.events.put(
                    ("confirm_ask", summary, strong)
                ),
                on_confirm_done=lambda: self.events.put(("confirm_done",)),
                on_warn=lambda message: self.events.put(("agent_warn", message)),
            )
            runtime = AgentRuntime(cfg, self.herdr, callbacks)
            log("Núcleo agéntico disponible.")
            return runtime
        except Exception as exc:  # noqa: BLE001
            log(f"Núcleo agéntico no disponible: {exc}")
            return None

    def _apply_turn_settings(self) -> None:
        """Enciende, apaga o ajusta Smart Turn sin reiniciar la llamada."""
        detector = getattr(self, "turn_detector", None)
        if not self.cfg.smart_turn:
            self.mic.turn_detector = None
            return
        if detector is None:
            self.turn_detector = self._create_turn_detector(self.cfg)
            return
        detector.threshold = float(self.cfg.turn_threshold)
        self.mic.turn_detector = detector

    def _create_turn_detector(self, cfg: VoiceConfig) -> SmartTurn | None:
        """Smart Turn en segundo plano; si falla, el VAD sigue solo con silencio."""
        if not getattr(cfg, "smart_turn", True):
            return None
        try:
            detector = SmartTurn(threshold=float(cfg.turn_threshold))
        except Exception as exc:  # noqa: BLE001
            log(f"Smart Turn no disponible: {exc}")
            return None
        self.mic.turn_detector = detector
        self.mic.turn_quick_secs = float(cfg.turn_quick_secs)
        return detector

    # ------------------------------------------------------------- bandeja
    def _create_tray(self):
        """Crea el icono de la bandeja; si falla, la app sigue sin él."""
        try:
            from .tray import TrayIcon

            tray = TrayIcon(
                lambda action: self.events.put(("tray_action", action)),
                image_path=_resource_root() / "assets" / "icon.png",
            )
            if tray.start():
                log("Bandeja del sistema activa.")
                return tray
            log("Bandeja del sistema no disponible.")
        except Exception as exc:  # noqa: BLE001
            log(f"No pude crear la bandeja del sistema: {exc}")
        return None

    def _update_tray_title(self) -> None:
        tray = getattr(self, "tray", None)
        if tray is None:
            return
        if not self.in_call:
            title = "EzAgent · en espera"
        elif self.muted:
            title = "EzAgent · micrófono silenciado"
        elif self.state == LISTENING:
            title = "EzAgent · escuchando"
        else:
            title = "EzAgent · en conversación"
        try:
            tray.set_title(title)
        except Exception:
            pass

    def _tray_action_ui(self, action: str) -> None:
        if action == "config":
            if self._notch_hidden:
                self._notch_hidden = False
                self.cfg.notch_hidden = False
                self.cfg.save_key("notch_hidden", False)
            current = self.state if self.in_call else IDLE
            try:
                self.notch.show(current)
                self.notch.open_menu()
            except Exception as exc:  # noqa: BLE001
                log(f"No pude abrir Ajustes desde la bandeja: {exc}")
        elif action == "shape":
            toggle = getattr(self.notch, "toggle_layout", None)
            if callable(toggle):
                try:
                    toggle()
                except Exception as exc:  # noqa: BLE001
                    log(f"No pude cambiar la forma: {exc}")
        elif action == "restart":
            self._restart_app()
        elif action == "quit":
            log("Cierre solicitado desde la bandeja.")
            self.root.quit()

    def _menu_action_ui(self, action: str) -> None:
        if action == "open_system_md":
            self._open_system_md()
        elif action == "open_voice_json":
            self._open_voice_json()
        elif action == "open_permissions":
            self._open_permissions()
        elif action == "open_config_dir":
            self._open_config_dir()
        elif action == "repeat_last":
            self._handle_hotkey("repeat")
        elif action == "new_conversation":
            self._new_conversation()
        else:
            log(f"Acción de menú desconocida: {action}")

    def _new_conversation(self) -> None:
        """Olvida la conversación (el menú ya pidió la confirmación)."""
        self._turn_gen += 1
        self._signal_tts_stop()
        self._speaking_kind = None
        self.contexts.new_chat()
        self.last_summary = ""
        log("Conversación borrada desde el menú.")
        self._speak("Listo, empezamos de cero.", "answer")

    def _open_system_md(self) -> None:
        """Abre system.md en el editor predeterminado; lo crea si falta."""
        try:
            runtime = getattr(self, "agent", None)
            if runtime is not None:
                path = Path(runtime.config.system_md_path)
            else:
                path = config_path().with_name("system.md")
            if not path.exists():
                path.parent.mkdir(parents=True, exist_ok=True)
                try:
                    from ezagent.agent.system_prompt import DEFAULT_SYSTEM_MD
                except Exception:
                    DEFAULT_SYSTEM_MD = (
                        "# Identidad\nEres EzAgent, un agente autónomo para Windows. "
                        "Responde corto y directo, en español neutro latinoamericano.\n"
                    )
                path.write_text(DEFAULT_SYSTEM_MD, encoding="utf-8")
            os.startfile(str(path))
            log(f"Abriendo {path}")
        except Exception as exc:  # noqa: BLE001
            log(f"No pude abrir system.md: {exc}")

    def _app_dir(self) -> Path:
        """Carpeta de datos de la app: la del runtime del agente o la de ``voice.json``."""
        runtime = getattr(self, "agent", None)
        if runtime is not None:
            return Path(runtime.config.app_dir)
        return Path(getattr(self, "config_file", None) or config_path()).parent

    def _open_permissions(self) -> None:
        """Abre permissions.toml (la política del agente) en el editor predeterminado."""
        try:
            runtime = getattr(self, "agent", None)
            if runtime is not None:
                path = Path(runtime.config.permissions_path)
            else:
                path = self._app_dir() / "permissions.toml"
            if not path.exists():
                log(f"permissions.toml todavía no existe en {path.parent}.")
                return
            os.startfile(str(path))
            log(f"Abriendo {path}")
        except Exception as exc:  # noqa: BLE001
            log(f"No pude abrir permissions.toml: {exc}")

    def _open_config_dir(self) -> None:
        """Abre la carpeta de datos (ajustes, permisos, memoria, skills y registros)."""
        try:
            folder = self._app_dir()
            folder.mkdir(parents=True, exist_ok=True)
            os.startfile(str(folder))
            log(f"Abriendo {folder}")
        except Exception as exc:  # noqa: BLE001
            log(f"No pude abrir la carpeta de datos: {exc}")

    def _open_voice_json(self) -> None:
        """Abre voice.json para editar perfiles de agente, rutas o atajos."""
        try:
            path = Path(getattr(self, "config_file", None) or config_path())
            if not path.exists():
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("{}\n", encoding="utf-8")
            os.startfile(str(path))
            log(f"Abriendo {path}")
        except Exception as exc:  # noqa: BLE001
            log(f"No pude abrir voice.json: {exc}")

    def _resume_listening_from_mic(self) -> None:
        """El micrófono tachado reactiva la escucha; si estaba muteado, lo quita."""
        if self.muted:
            self._set_mic_muted(False)
        if not self.in_call:
            self._start_call()
        else:
            self._set_state(LISTENING)

    def _restart_app(self) -> None:
        if getattr(self, "_restarting", False):
            return
        self._restarting = True
        if not self._spawn_restart():
            log("No pude reiniciar EzAgent; sigo en marcha.")
            self._restarting = False
            return
        log("Reiniciando EzAgent…")
        try:
            self.shutdown()
        except Exception as exc:  # noqa: BLE001
            log(f"Error al cerrar para reiniciar: {exc}")
        self.root.after(0, self.root.quit)

    def _spawn_restart(self) -> bool:
        """Lanza otra instancia; el proceso actual se cierra después."""
        try:
            if getattr(sys, "frozen", False):
                command = [sys.executable, *sys.argv[1:]]
                cwd = str(Path(sys.executable).resolve().parent)
            else:
                command = [
                    sys.executable,
                    "-m",
                    "ezagent.voice.app",
                    *sys.argv[1:],
                ]
                cwd = str(Path.cwd())
            flags = 0
            if os.name == "nt":
                flags = getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0) | getattr(
                    subprocess, "DETACHED_PROCESS", 0
                )
            subprocess.Popen(
                command,
                cwd=cwd,
                close_fds=True,
                creationflags=flags,
            )
            return True
        except Exception as exc:  # noqa: BLE001
            log(f"No pude lanzar el reinicio: {exc}")
            return False

    def _warm_tts(self) -> None:
        try:
            self.speaker.synthesize("Hola.")
            log("Kokoro precalentado.")
        except Exception as exc:  # noqa: BLE001
            log(f"No pude precalentar Kokoro: {exc}")

    def _warm_stt(self) -> None:
        """Carga el modelo de whisper antes del primer turno real."""
        if not _whisper_ready(self.cfg.whisper_url):
            # Ya se avisó al arrancar: no repetir el error en cada intento.
            log("whisper no disponible; salto el precalentamiento.")
            return
        try:
            transcribe(self.cfg, pcm_to_wav(b"\x00\x00" * 3200, 16000))
            log("whisper precalentado.")
        except Exception as exc:  # noqa: BLE001
            log(f"No pude precalentar whisper: {exc}")

    # --------------------------------------------------------------- arranque
    @staticmethod
    def _check_hotkey_duplicates(values: dict[str, str]) -> None:
        seen: dict[str, str] = {}
        for key in HOTKEY_KEYS:
            normalized = _hotkey_key(values.get(key, ""))
            if not normalized:
                continue
            if normalized in seen:
                raise ValueError(f"Los atajos {seen[normalized]} y {key} son el mismo.")
            seen[normalized] = key

    def _build_hotkeys(self) -> list[GlobalHotkey]:
        """Atajos activos según `voice.json`, con sus alias compatibles."""
        cancel_specs: list[str] = []
        cancel_used = {
            _hotkey_key(self.cfg.hotkey),
            _hotkey_key(self.cfg.repeat_hotkey),
            _hotkey_key(self.cfg.mute_hotkey),
            _hotkey_key(self.cfg.menu_hotkey),
        }
        for spec in (self.cfg.cancel_hotkey, "ctrl+alt+e", "ctrl+alt+c"):
            if (
                spec
                and spec not in cancel_specs
                and (spec == self.cfg.cancel_hotkey or _hotkey_key(spec) not in cancel_used)
            ):
                cancel_specs.append(spec)
        repeat_specs: list[str] = []
        repeat_used = {
            _hotkey_key(self.cfg.hotkey),
            _hotkey_key(self.cfg.cancel_hotkey),
            _hotkey_key(self.cfg.mute_hotkey),
            _hotkey_key(self.cfg.menu_hotkey),
        }
        for spec in (self.cfg.repeat_hotkey, "ctrl+alt+y"):
            if (
                spec
                and spec not in repeat_specs
                and (spec == self.cfg.repeat_hotkey or _hotkey_key(spec) not in repeat_used)
            ):
                repeat_specs.append(spec)
        hotkeys = [GlobalHotkey(self.cfg.hotkey, "toggle", self.hotkey_events, 47201)]
        for index, spec in enumerate(cancel_specs[:4]):
            hotkeys.append(GlobalHotkey(spec, "cancel", self.hotkey_events, 47202 + index))
        for index, spec in enumerate(repeat_specs[:4]):
            hotkeys.append(GlobalHotkey(spec, "repeat", self.hotkey_events, 47210 + index))
        hotkeys.append(GlobalHotkey(self.cfg.mute_hotkey, "mute", self.hotkey_events, 47220))
        hotkeys.append(GlobalHotkey(self.cfg.menu_hotkey, "menu", self.hotkey_events, 47221))
        hotkeys.append(GlobalHotkey(self.cfg.chat_hotkey, "chat", self.hotkey_events, 47222))
        return hotkeys

    def _suspend_hotkeys(self, active: bool) -> None:
        """Suelta los atajos globales mientras Ajustes graba uno; los vuelve a registrar."""
        if not active:
            self._reload_hotkeys()
            return
        for hotkey in list(getattr(self, "hotkeys", [])):
            try:
                hotkey.stop(wait=True)
            except Exception:
                pass
        self.hotkeys = []

    def _reload_hotkeys(self) -> None:
        """Re-registra los atajos tras cambiarlos en Ajustes."""
        old = list(getattr(self, "hotkeys", []))
        for hotkey in old:
            try:
                hotkey.stop(wait=True)
            except Exception:
                pass
        self.hotkeys = self._build_hotkeys()
        for hotkey in self.hotkeys:
            hotkey.start()
        self.root.after(150, self._report_hotkeys)

    def _report_hotkeys(self) -> None:
        for hotkey in self.hotkeys:
            if hotkey.error:
                log(f"Hotkey {hotkey.spec}: {hotkey.error}")
        log(
            f"Listo. {self.cfg.hotkey}=llamada, ctrl+alt+e/ctrl+alt+c=colgar, "
            f"{self.cfg.mute_hotkey}=mute, ctrl+alt+r/ctrl+alt+y=repetir, "
            f"{self.cfg.menu_hotkey}=menú. Ctrl+C para salir."
        )

    def run(self) -> None:
        self.watcher.start()
        # Las tareas/cadenas pendientes se reanudan en segundo plano: lanzar
        # una cadena puede tardar y no debe bloquear el mainloop.
        threading.Thread(
            target=self._resume_pending_tasks,
            name="ezagent-resume-tasks",
            daemon=True,
        ).start()
        # La app nace en modo escucha: no hace falta pulsar nada al abrirla.
        self.root.after(600, self._start_listening_on_launch)
        try:
            self.root.mainloop()
        finally:
            self.shutdown()

    def _start_listening_on_launch(self) -> None:
        if self.in_call:
            return
        if self._notch_hidden:
            # Si la barra estaba oculta de la sesión anterior, este arranque la
            # devuelve: el usuario pidió abrir la app escuchando.
            self._notch_hidden = False
            self.cfg.notch_hidden = False
            self.cfg.save_key("notch_hidden", False)
            self.notch.show(IDLE)
        self._start_call()
        log("Escuchando al iniciar.")

    def shutdown(self) -> None:
        if getattr(self, "_shutdown_done", False):
            return
        self._shutdown_done = True
        agent = getattr(self, "agent", None)
        if agent is not None:
            try:
                agent.cancel_confirmation()
            except Exception:
                pass
        self.in_call = False
        self._user_speaking = False
        self._turn_gen += 1
        if self._call_stop is not None:
            self._call_stop.set()
        self._signal_tts_stop()
        self.watcher.stop()
        for hotkey in self.hotkeys:
            hotkey.stop()
        if self._whisper_proc is not None and self._whisper_proc.poll() is None:
            try:
                subprocess.run(
                    [
                        "taskkill",
                        "/PID",
                        str(self._whisper_proc.pid),
                        "/T",
                        "/F",
                    ],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
                )
            except Exception:
                pass
        try:
            self.notch.close()
        except Exception:
            pass
        agent = getattr(self, "agent", None)
        if agent is not None:
            try:
                agent.close()
            except Exception:
                pass
        try:
            self.speaker.close()
        except Exception:
            pass
        try:
            self.mic.close()
        except Exception:
            pass
        detector = getattr(self, "turn_detector", None)
        if detector is not None:
            detector.close()
        tray = getattr(self, "tray", None)
        if tray is not None:
            try:
                tray.stop()
            except Exception:
                pass

    # ---------------------------------------------------------------- eventos
    def _tick(self) -> None:
        try:
            while True:
                try:
                    event = self.hotkey_events.get_nowait()
                except queue.Empty:
                    break
                try:
                    self._handle_hotkey(event)
                except Exception as exc:  # noqa: BLE001
                    log(f"Error en hotkey {event!r}: {exc}")
            while True:
                try:
                    event = self.events.get_nowait()
                except queue.Empty:
                    break
                try:
                    self._handle_event(event)
                except Exception as exc:  # noqa: BLE001
                    log(f"Error en evento {event[0]!r}: {exc}")
            while True:
                try:
                    event = self.menu_events.get_nowait()
                except queue.Empty:
                    break
                try:
                    self._handle_menu_event(event)
                except Exception as exc:  # noqa: BLE001
                    log(f"Error en menú {event[0]!r}: {exc}")
            # Red de seguridad: un aviso encolado nunca se queda atascado si
            # el turno que lo bloqueaba termina en un evento no previsto.
            self._pump_announcements()
        finally:
            self.root.after(30, self._tick)

    def _handle_hotkey(self, event: str) -> None:
        log(f"Hotkey: {event}")
        if event == "toggle":
            if self.in_call:
                self._hang_up()
            else:
                self._start_call()
        elif event == "cancel":
            self._hang_up()
        elif event == "repeat":
            if self.last_summary:
                self._speak(self.last_summary, "answer")
            elif self.state == IDLE:
                self._speak("No hay nada que repetir todavía.", "answer")
        elif event == "mute":
            self._set_mic_muted(not self.muted, source="atajo")
        elif event == "menu":
            if self._notch_hidden:
                self._notch_hidden = False
                self.cfg.notch_hidden = False
                self.cfg.save_key("notch_hidden", False)
                self.notch.show(IDLE)
            self._toggle_menu()
        elif event == "chat":
            self._show_bar_if_hidden()
            self._toggle_chat()

    def _show_bar_if_hidden(self) -> None:
        """Si la barra estaba oculta, vuelve a mostrarla (el chat sale de ella)."""
        if self._notch_hidden:
            self._notch_hidden = False
            self.cfg.notch_hidden = False
            self.cfg.save_key("notch_hidden", False)
            self.notch.show(IDLE)

    def _handle_event(self, event: tuple) -> None:
        kind = event[0]
        if kind == "notch_action":
            self._notch_action_ui(str(event[1]))
        elif kind == "tray_action":
            self._tray_action_ui(str(event[1]))
        elif kind == "menu_action":
            self._menu_action_ui(str(event[1]))
        elif kind == "hotkey_capture":
            self._suspend_hotkeys(bool(event[1]))
        elif kind == "chat_send":
            self._chat_send(str(event[1]))
        elif kind == "chat_new":
            self._chat_new()
        elif kind == "chat_switch":
            self._chat_switch(int(event[1]))
        elif kind == "chat_delete":
            self._chat_delete(int(event[1]))
        elif kind == "chat_stop":
            self._chat_stop()
        elif kind == "chat_mute":
            self._set_mic_muted(bool(event[1]), source="chat")
        elif kind == "transcribed":
            _, text, gen = event
            text = (text or "").strip()
            # Una confirmación pendiente se resuelve con la siguiente frase,
            # antes de cualquier otro atajo o turno del LLM (sección 5.5).
            agent = getattr(self, "agent", None)
            if agent is not None and agent.confirm_pending and text:
                agent.submit_confirmation(text)
                return
            if not self.in_call or gen != self._turn_gen:
                return
            if not text:
                self._set_state(LISTENING)
                return
            if self._turn_timing is not None:
                log(f"Tiempos: {format_stt(self._turn_timing.report())}")
            log(f"Tú: {text}")
            fast_t0 = time.monotonic()
            if self._fast_command(text):
                handled_ms = (time.monotonic() - fast_t0) * 1000
                self._latency.record_command("fast_command", handled_ms)
                log(f"Comando local: {handled_ms:.0f} ms")
                return
            self._set_state(THINKING)
            self.chat.add_user(text, "voice")
            threading.Thread(target=self._answer_worker, args=(text, gen), daemon=True).start()
        elif kind == "speaking":
            gen = event[1]
            if gen == self._turn_gen:
                self._set_state(SPEAKING)
        elif kind == "answer_done":
            gen = event[1]
            if gen != self._turn_gen:
                return
            timing = self._turn_timing
            if timing is not None:
                timing.mark("turn_end")
                self._latency.record(timing)
                self._turn_timing = None
            self._speaking_kind = None
            self._reset_capture.set()
            self._ignore_utterance = False
            self._post_tts_ignore = False
            self._post_tts_until = time.monotonic() + 0.25
            # Red de seguridad: si el modelo no llamó a la herramienta pero el
            # usuario pidió aviso, registra el último agente lanzado.
            notify_address = self.messenger.pending_notify_address()
            if notify_address and notify_address not in self.watched:
                self._watch(notify_address)
            if self._close_after_answer:
                self._close_after_answer = False
                if self._close_listening():
                    self._pump_announcements()
                return
            if self.in_call:
                self._set_state(LISTENING)
            else:
                self._set_idle()
            self._pump_announcements()
        elif kind == "done_answer":
            self._handle_done_answer(*event[1:])
        elif kind == "done_prompt":
            _, address, agent, reason, output, task = event
            self._handle_done_prompt(address, agent, reason, output, task)
        elif kind == "speak_done":
            gen = event[1]
            if gen != self._turn_gen:
                return
            self._speaking_kind = None
            self._reset_capture.set()
            self._ignore_utterance = False
            self._post_tts_ignore = False
            self._post_tts_until = time.monotonic() + 0.25
            if self._close_after_answer:
                self._close_after_answer = False
                if self.in_call:
                    self._close_listening()
                self._pump_announcements()
                return
            if self.in_call:
                self._set_state(LISTENING)
            else:
                self._set_idle()
            self._pump_announcements()
        elif kind == "llm_models_ready":
            self._fix_model_name(str(event[1]), tuple(event[2]))
        elif kind == "error":
            message = str(event[1])[:320]
            log(f"Error: {message}")
            if message.startswith("Kokoro:"):
                # No reintentar con el mismo TTS que acaba de fallar.
                self._set_state("error")
                return
            if message.lower().startswith(("no puedo usar el micrófono", "micrófono")):
                self._hang_up()
                self._speak(message, "answer")
            elif self.in_call:
                self._speak(message, "answer")
            else:
                self._set_idle()
                self._speak(message, "answer")
        elif kind == "test_done":
            _, old_muted, old_barge = event
            self.cfg.barge_in = bool(old_barge)
            if not old_muted and self.muted:
                self._set_mic_muted(False)
            try:
                self.notch.set_testing(False)
            except Exception:
                pass
            if self.in_call:
                self._set_state(LISTENING)
            else:
                self._set_idle()
        elif kind == "confirm_ask":
            self._show_confirmation(str(event[1]), bool(event[2]))
        elif kind == "confirm_done":
            self.chat.clear_confirm()
            try:
                self.notch.hide_confirmation()
            except Exception:
                pass
        elif kind == "confirm_click":
            self._signal_tts_stop()
            agent = getattr(self, "agent", None)
            if agent is not None:
                agent.submit_confirmation_click(bool(event[1]))
        elif kind == "agent_warn":
            message = str(event[1])
            log(f"Núcleo: {message}")
            # Durante una llamada no se interrumpe el turno para leer un aviso
            # de configuración; queda en el log. Fuera de llamada sí se dice.
            if not self.in_call:
                self._speak(message, "answer")
        elif kind == "notch_error":
            log(f"Notch: {event[1]}")
        elif kind == "close_request":
            self._close_after_answer = True
            self._close_mode = str(event[1])
        elif kind == "wake":
            self._on_wake()

    # ---------------------------------------------------------------- llamada
    def _start_call(self) -> None:
        if self.in_call:
            return
        self._signal_tts_stop()
        self._speaking_kind = None
        self.in_call = True
        self._user_speaking = False
        self._turn_gen += 1
        self._call_stop = threading.Event()
        self.notch.set_in_call(True)
        self._set_state(LISTENING)
        log("Llamada iniciada. Habla cuando quieras; Ctrl+Alt+E para colgar.")
        self._call_thread = threading.Thread(
            target=self._call_worker, args=(self._call_stop,), daemon=True
        )
        self._call_thread.start()

        # DECISION: una llamada nueva siempre empieza escuchando; la pausa
        # pertenece a la llamada anterior y no se arrastra.
        if self.muted:
            self._set_mic_muted(False, source="llamada nueva")

    def _hang_up(self) -> None:
        agent = getattr(self, "agent", None)
        if agent is not None:
            try:
                agent.cancel_confirmation()
            except Exception:
                pass
        try:
            self.notch.hide_confirmation()
        except Exception:
            pass
        if not self.in_call:
            return
        self.in_call = False
        try:
            self.notch.set_mic_level(0.0)
        except Exception:
            pass
        self._user_speaking = False
        if self._call_stop is not None:
            self._call_stop.set()
        if self._speaking_kind == "announce":
            # Un aviso de agente en curso se entrega aunque cuelgues.
            log("Llamada terminada; termino el aviso en curso.")
            return
        self._turn_gen += 1
        self._signal_tts_stop()
        self._speaking_kind = None
        self.notch.set_in_call(False)
        self._set_idle()
        log("Llamada terminada.")
        self.root.after(200, self._pump_announcements)

    def _call_worker(self, stop_event: threading.Event) -> None:
        try:
            self.mic.stream(
                stop_event,
                on_level=self._on_mic_level,
                on_speech_start=self._on_speech_start,
                on_utterance=self._on_utterance,
                reset_event=self._reset_capture,
                is_speaking=lambda: self.state == SPEAKING,
                on_barge_in=self._on_barge_in,
            )
        except Exception as exc:  # noqa: BLE001
            self.events.put(("error", f"No puedo usar el micrófono: {exc}"))

    def _on_mic_level(self, level: float) -> None:
        try:
            self.notch.set_mic_level(0.0 if self.muted else level)
        except Exception:
            pass
        if self.in_call and self.state == LISTENING:
            self.notch.draw(0.0 if self.muted else level, LISTENING)

    def _on_barge_in(self, level: float) -> None:
        if self.muted:
            return
        if not self.in_call or self.state != SPEAKING or not self.cfg.barge_in:
            return
        agent = getattr(self, "agent", None)
        if agent is not None and agent.confirm_pending:
            # La respuesta a la confirmación pertenece al turno bloqueado:
            # se corta el TTS pero no se invalida el turno.
            self._signal_tts_stop()
            self._user_speaking = True
            self._ignore_utterance = False
            self._set_state(LISTENING)
            return
        if self._close_after_answer:
            # El usuario interrumpe el cierre: no cerramos.
            self._close_after_answer = False
            self._close_mode = ""
        log("Barge-in: te escucho.")
        self._turn_gen += 1
        self._signal_tts_stop()
        self._user_speaking = True
        self._ignore_utterance = False
        self._set_state(LISTENING)

    def _on_speech_start(self, level: float) -> None:
        if not self.in_call:
            return
        if self.muted:
            self._ignore_utterance = True
            self._post_tts_ignore = False
            return
        agent = getattr(self, "agent", None)
        if agent is not None and agent.confirm_pending:
            if self.state == SPEAKING and self.cfg.barge_in and self.cfg.barge_in_rms > 0:
                # La pregunta aún suena: un arranque de voz puede ser eco o un
                # golpe. Corta BargeInDetector si es voz sostenida (_on_barge_in).
                self._ignore_utterance = True
                self._post_tts_ignore = False
                return
            # El usuario está respondiendo a la confirmación: se corta el TTS
            # de la pregunta y se captura la frase sin abrir otro turno.
            self._user_speaking = True
            self._ignore_utterance = False
            self._post_tts_ignore = False
            self._signal_tts_stop()
            self._set_state(LISTENING)
            return
        if time.monotonic() < self._post_tts_until:
            # Cola del TTS: se descarta el eco, salvo que la frase siga viva
            # cuando termine la ventana (se decide en _on_utterance).
            self._ignore_utterance = True
            self._post_tts_ignore = True
            return
        self._user_speaking = True
        # Un arranque anterior que no llegó a frase (ruido corto) no debe
        # pegar audio viejo a esta.
        self._resume_pcm = None
        if self.state == THINKING:
            # No hay eco: el usuario puede corregir o repetir mientras el
            # modelo piensa. La respuesta pendiente se descarta.
            last = getattr(self, "_last_turn_audio", None)
            if last is not None and time.monotonic() - last[2] <= RESUME_SECS:
                # Seguramente el turno se cerró en una pausa: se une la frase
                # anterior con la que viene para no perder la primera parte.
                self._resume_pcm = (last[0], last[1])
                log("Sigues hablando: uno esta frase con la anterior.")
            self._turn_gen += 1
            self._signal_tts_stop()
            self._set_state(LISTENING)
            return
        if self.state == SPEAKING:
            # DECISION: este arranque de voz ya no interrumpe por sí solo. Si el
            # asistente sigue hablando, es eco del TTS o un ruido corto: la
            # interrupción real la confirma BargeInDetector (audio.py), que exige
            # voz sostenida, y llega a _on_barge_in. Antes bastaba un pico de
            # 90 ms por encima de barge_in_rms para cortar al modelo.
            if self.cfg.barge_in and self.cfg.barge_in_rms <= 0:
                # Sin umbral configurado no hay detector: se interrumpe al hablar.
                self._on_barge_in(level)
                return
            self._ignore_utterance = True
            self._post_tts_ignore = False

    def _on_utterance(self, pcm: bytes, rate: int) -> None:
        self._user_speaking = False
        if not pcm:
            # Arranque de voz descartado por el VAD (clic o ruido corto): se
            # suelta el estado de «hablando» para que los avisos no se atasquen.
            self._ignore_utterance = False
            self._post_tts_ignore = False
            return
        if self.muted:
            self._ignore_utterance = False
            self._post_tts_ignore = False
            self._maybe_wake(pcm, rate)
            return
        if self._ignore_utterance:
            now = time.monotonic()
            if self._post_tts_ignore and now >= self._post_tts_until:
                # La frase empezó en la cola del TTS, pero siguió después:
                # es del usuario y no se tira.
                self._ignore_utterance = False
                self._post_tts_ignore = False
            else:
                self._ignore_utterance = False
                self._post_tts_ignore = False
                return
        if not self.in_call or not pcm:
            return
        resume = getattr(self, "_resume_pcm", None)
        self._resume_pcm = None
        if resume is not None and resume[1] == rate:
            pcm = resume[0] + pcm
        now = time.monotonic()
        self._last_turn_audio = (pcm, rate, now)
        self._turn_started = now
        speech_end = getattr(self.mic, "last_speech_end", None) or now
        timing = TurnTimings(speech_end=speech_end, vad_end=now)
        self._turn_timing = timing
        agent = getattr(self, "agent", None)
        if agent is not None and agent.confirm_pending:
            # La frase resuelve la confirmación; no invalida el turno que
            # está esperando en el hilo de herramientas.
            gen = self._turn_gen
        else:
            self._turn_gen += 1
            gen = self._turn_gen
        self._set_state(THINKING)
        threading.Thread(
            target=self._stt_worker, args=(pcm, rate, gen, timing), daemon=True
        ).start()

    # ---------------------------------------------------------------- estados
    def _set_state(self, state: str) -> None:
        self.state = state
        if state == LISTENING:
            self._ignore_utterance = False
            self._post_tts_ignore = False
        if state == IDLE:
            # La barra queda como control compacto: micrófono, ojos y ajustes.
            # El icono del micro refleja el estado real de la escucha.
            self.notch.set_in_call(self.in_call)
            try:
                self.notch.set_mic_level(0.0)
            except Exception:
                pass
            if not self._notch_hidden:
                self.notch.show(IDLE)
                self.notch.draw(0.0, IDLE)
        else:
            if self._notch_hidden:
                self._notch_hidden = False
                self.cfg.notch_hidden = False
                self.cfg.save_key("notch_hidden", False)
            self.notch.set_in_call(self.in_call)
            self.notch.show(state)
            self.notch.draw(0.0, state)
        self._update_tray_title()

    def _set_idle(self) -> None:
        self._set_state(IDLE)

    def _tts_guard(self) -> threading.Lock:
        """Lock de ``_tts_stop``; lo crea si la instancia no pasó por __init__."""
        # DECISION: las pruebas construyen VoiceApp con __new__; en la app real
        # el lock existe siempre desde __init__.
        lock = getattr(self, "_tts_lock", None)
        if lock is None:
            lock = threading.Lock()
            self._tts_lock = lock
        return lock

    def _new_tts_stop(self) -> threading.Event:
        with self._tts_guard():
            if self._tts_stop is not None:
                self._tts_stop.set()
            self._tts_stop = threading.Event()
            return self._tts_stop

    def _signal_tts_stop(self) -> None:
        with self._tts_guard():
            if self._tts_stop is not None:
                self._tts_stop.set()

    def _speak(self, text: str, kind: str = "announce") -> None:
        if not text:
            return
        self._turn_gen += 1
        generation = self._turn_gen
        stop_event = self._new_tts_stop()
        self._speaking_kind = kind
        self._set_state(SPEAKING)
        threading.Thread(
            target=self._tts_worker,
            args=(text, stop_event, generation),
            daemon=True,
        ).start()

    def _speak_same_turn(self, text: str, kind: str = "answer") -> None:
        """Habla sin incrementar ``_turn_gen``.

        La usa la pregunta de confirmación: el hilo de herramientas está
        esperando dentro del turno actual y no debe quedar invalidado por el
        TTS de la pregunta.
        """
        if not text:
            return
        generation = self._turn_gen
        stop_event = self._new_tts_stop()
        self._speaking_kind = kind
        self._set_state(SPEAKING)
        threading.Thread(
            target=self._tts_worker,
            args=(text, stop_event, generation),
            daemon=True,
        ).start()

    def _show_confirmation(self, summary: str, strong: bool) -> None:
        """Muestra la fila Sí/No en la barra y lee el resumen."""
        agent = getattr(self, "agent", None)
        # El evento puede llegar después de que el usuario ya haya respondido
        # por voz; en ese caso no se muestra ni se lee la pregunta.
        if agent is None or not agent.confirm_pending:
            return
        self.chat.ask_confirm(summary, strong)
        try:
            self.notch.show_confirmation(
                summary,
                strong,
                lambda: self.events.put(("confirm_click", True)),
                lambda: self.events.put(("confirm_click", False)),
            )
        except Exception as exc:  # noqa: BLE001
            log(f"No pude mostrar la confirmación: {exc}")
        hint = " Di «confirmo» y el número para aceptar." if strong else " ¿Confirmas?"
        self._speak_same_turn(summary + hint)

    def _announcements(self) -> list[tuple[str, bool]]:
        # Creación diferida: algunas pruebas construyen VoiceApp sin __init__.
        if not hasattr(self, "_announce_queue"):
            self._announce_queue = []
        return self._announce_queue

    def _queue_announcement(self, text: str, *, reopen_call: bool = True) -> None:
        """Encola un aviso; suena cuando el turno anterior termine."""
        if not text:
            return
        self._announcements().append((text, reopen_call))
        self._pump_announcements()

    def _pump_announcements(self) -> None:
        """Dice el siguiente aviso solo si no hay voz ni turno en curso."""
        queue_ = self._announcements()
        if not queue_:
            return
        agent = getattr(self, "agent", None)
        if (
            getattr(self, "_user_speaking", False)
            or getattr(self, "state", IDLE) in (THINKING, SPEAKING)
            or (agent is not None and getattr(agent, "confirm_pending", False))
        ):
            return
        text, reopen = queue_.pop(0)
        if reopen and not getattr(self, "in_call", False):
            self._start_call()
        self._speak(text, "announce")

    # --------------------------------------------------------------- workers
    def _stt_worker(self, pcm: bytes, rate: int, gen: int, timing: TurnTimings) -> None:
        try:
            text = transcribe(self.cfg, pcm_to_wav(pcm, rate))
            timing.mark("stt_ready")
            if is_hallucination(text, weak_audio=pcm_is_weak(pcm, rate)):
                log(f"STT: descarto posible alucinación de Whisper: {text!r}")
                text = ""
            self.events.put(("transcribed", text, gen))
        except STTError as exc:
            self.events.put(("error", str(exc)))
        except Exception as exc:  # noqa: BLE001
            self.events.put(("error", f"STT: {exc}"))

    def _answer_worker(self, text: str, gen: int) -> None:
        """Consume el LLM en streaming y habla cada frase en cuanto llega."""
        stop_event = self._new_tts_stop()
        self._speaking_kind = "answer"
        sentences = _Sentences()
        full: list[str] = []
        started = False
        timing = self._turn_timing if gen == self._turn_gen else None
        first_token = False
        first_audio = False
        speech = self._speech_queue(stop_event, timing)
        finished = False
        chat_msg = self.chat.begin_assistant("voice")
        chat_done = False

        def speak_sentence(sentence: str) -> None:
            nonlocal first_audio
            if timing is not None and not first_audio:
                timing.mark("first_sentence")
                first_audio = True
            # Sintetiza aquí y encola: suena en el hilo de la cola mientras
            # este hilo sigue leyendo el LLM y sintetizando la frase siguiente.
            speech.say(sentence)

        try:
            if timing is not None:
                timing.mark("llm_start")
            for delta in _timed_stream(self.messenger.ask_stream(text), timing):
                if gen != self._turn_gen or stop_event.is_set():
                    return
                if timing is not None and not first_token and delta.strip():
                    timing.mark("first_token")
                    first_token = True
                full.append(delta)
                self.chat.append(chat_msg.id, delta)
                for sentence in sentences.feed(delta):
                    if gen != self._turn_gen or stop_event.is_set():
                        return
                    if not started:
                        started = True
                        self.events.put(("speaking", gen))
                    try:
                        speak_sentence(sentence)
                    except Exception as exc:  # noqa: BLE001
                        self.events.put(("error", f"Kokoro: {exc}"))
                        return
            for sentence in sentences.flush():
                if gen != self._turn_gen or stop_event.is_set():
                    return
                if not started:
                    started = True
                    self.events.put(("speaking", gen))
                try:
                    speak_sentence(sentence)
                except Exception as exc:  # noqa: BLE001
                    self.events.put(("error", f"Kokoro: {exc}"))
                    return
            answer = "".join(full).strip()
            self.chat.finish(chat_msg.id)
            chat_done = True
            if answer:
                self.last_summary = answer
                log(f"Mensajero: {answer}")
            rounds = list(getattr(self.messenger, "last_rounds", []) or [])
            if len(rounds) > 1:
                # Con herramientas, «1.er token» suma varias llamadas al LLM.
                log("LLM por llamada: " + " | ".join(rounds))
            try:
                # answer_done sale cuando termina de sonar, como antes.
                speech.finish()
                finished = True
            except Exception as exc:  # noqa: BLE001
                self.events.put(("error", f"Kokoro: {exc}"))
        except Exception as exc:  # noqa: BLE001
            log(f"LLM ({self.cfg.llm_provider}): {type(exc).__name__}: {exc}")
            reason = friendly_llm_error(exc, self._llm_label(), self.cfg.llm_model)
            self.chat.fail(chat_msg.id, reason)
            chat_done = True
            self.events.put(("error", reason))
        finally:
            if not chat_done:  # turno interrumpido: se conserva lo que llegó
                self.chat.cancel(chat_msg.id)
            if not finished:
                speech.abort()
            self.events.put(("answer_done", gen))

    # ------------------------------------------------------------------ chat
    def _toggle_chat(self) -> None:
        """El chat escrito vive en la barra: se abre con el botón 💬 o con el atajo."""
        try:
            self.notch.toggle_chat()
        except Exception as exc:  # noqa: BLE001
            log(f"No pude abrir el chat: {exc}")

    def _set_chat_voice(self, enabled: bool) -> None:
        """Interruptor «responder por voz» del chat: se aplica ya y se guarda."""
        self.cfg.chat_voice = bool(enabled)
        self.chat.set_voice_replies(bool(enabled))
        try:
            self.cfg.save_key("chat_voice", bool(enabled))
        except Exception as exc:  # noqa: BLE001
            log(f"No pude guardar chat_voice: {exc}")

    def _chat_media_roots(self) -> tuple[tuple[Path, ...], tuple[Path, ...]]:
        """Carpetas desde las que el chat muestra imágenes y documentos (las de lectura del agente)."""
        agent = getattr(self, "agent", None)
        if agent is None:
            return ((Path.home(),), ())
        config = agent.config
        roots = [Path(str(r)).expanduser() for r in getattr(config.paths, "read_roots", [])]
        roots += [Path(config.workspace), Path(config.app_dir)]
        return (tuple(roots), (Path(config.workspace),))

    def _chat_stop(self) -> None:
        """El usuario pulsó «detener» en el chat: se corta la respuesta y la voz."""
        self._turn_gen += 1
        self._signal_tts_stop()
        self._speaking_kind = None
        if self.in_call:
            self._set_state(LISTENING)
        else:
            self._set_idle()

    def _chat_new(self) -> None:
        """Nueva conversación desde el chat: igual que la del menú, pero sin decirlo en voz alta."""
        self._turn_gen += 1
        self._signal_tts_stop()
        self._speaking_kind = None
        self.contexts.new_chat()
        self.last_summary = ""
        log("Conversación nueva desde el chat.")

    def _chat_switch(self, session: int) -> None:
        """Retoma otra conversación del historial con su propio contexto del modelo."""
        if self.chat.busy:  # no se cambia el contexto en mitad de una respuesta
            return
        self._turn_gen += 1
        self._signal_tts_stop()
        self._speaking_kind = None
        if self.contexts.switch(session):
            last = self.chat.last_assistant_text()
            self.last_summary = spoken_version(last) if last else ""
            log(f"Conversación {session} retomada desde el chat.")

    def _chat_delete(self, session: int) -> None:
        """Borra una conversación del historial (y su contexto) a petición del usuario."""
        if self.contexts.delete(session):
            if session == self.chat.current_session:
                self.last_summary = ""
            log(f"Conversación {session} borrada del historial.")

    def _chat_send(self, text: str) -> None:
        """Un mensaje escrito en el chat: es un turno más, igual que uno hablado."""
        text = (text or "").strip()
        if not text:
            return
        agent = getattr(self, "agent", None)
        if agent is not None and agent.confirm_pending:
            # Escribir «sí» o «confirmo 3» responde a la confirmación pendiente.
            self.chat.add_user(text, "chat")
            agent.submit_confirmation(text)
            return
        self._turn_gen += 1
        gen = self._turn_gen
        self._signal_tts_stop()
        self._speaking_kind = None
        self.chat.add_user(text, "chat")
        log(f"Chat: {text[:80]}")
        self._set_state(THINKING)
        threading.Thread(target=self._chat_worker, args=(text, gen), daemon=True).start()

    def _chat_worker(self, text: str, gen: int) -> None:
        """Turno del chat: el texto llega en streaming a la pantalla; solo habla si se quiere."""
        stop_event = self._new_tts_stop()
        message = self.chat.begin_assistant("chat")
        finished = False
        try:
            got_text = False
            for delta in self.messenger.ask_stream(text, mode="chat"):
                if gen != self._turn_gen or stop_event.is_set():
                    return
                got_text = got_text or bool(delta.strip())
                self.chat.append(message.id, delta)
            if not got_text:
                self.chat.fail(message.id, "El modelo no devolvió respuesta. Prueba de nuevo.")
                finished = True
                return
            self.chat.finish(message.id)
            finished = True
            answer = self.chat.last_assistant_text()
            spoken = spoken_version(answer) if answer else ""
            if answer:
                self.last_summary = spoken or answer
            if spoken and self.chat.voice_replies and gen == self._turn_gen:
                self._speaking_kind = "answer"
                self.events.put(("speaking", gen))
                self.speaker.speak(
                    spoken, stop_event, on_level=lambda level: self.notch.draw(level, SPEAKING)
                )
        except Exception as exc:  # noqa: BLE001
            log(f"Chat ({self.cfg.llm_provider}): {type(exc).__name__}: {exc}")
            reason = friendly_llm_error(exc, self._llm_label(), self.cfg.llm_model)
            self.chat.fail(message.id, reason)
            finished = True
        finally:
            if not finished:  # interrumpido: se conserva lo que llegó
                self.chat.cancel(message.id)
            self.events.put(("answer_done", gen))

    def _speech_queue(self, stop_event: threading.Event, timing: TurnTimings | None) -> SpeechQueue:
        """Cola de voz del turno: sintetiza en el hilo del LLM y suena aparte."""

        def first_sound() -> None:
            # Solo el primer sonido del turno cuenta como «primer audio».
            if timing is not None and "first_sound" not in timing.marks:
                timing.mark("first_sound")

        return SpeechQueue(
            self.speaker.synthesize,
            lambda samples, rate, on_start: self.speaker.play(
                samples,
                rate,
                stop_event,
                on_level=lambda level: self.notch.draw(level, SPEAKING),
                on_start=on_start,
            ),
            stop_event,
            on_sound=first_sound,
            on_synth=(
                (lambda secs: timing.add_span("tts_synth", secs)) if timing is not None else None
            ),
            on_play=(
                (lambda secs: timing.add_span("tts_play", secs)) if timing is not None else None
            ),
        )

    def _tts_worker(self, text: str, stop_event: threading.Event, generation: int) -> None:
        try:
            self.speaker.speak(
                text,
                stop_event,
                on_level=lambda level: self.notch.draw(level, SPEAKING),
            )
        except Exception as exc:  # noqa: BLE001
            self.events.put(("error", f"Kokoro: {exc}"))
            return
        self.events.put(("speak_done", generation))

    # --------------------------------------------------------------- watcher
    def _should_announce(self, address: str) -> bool:
        if time.time() < self.mute_until:
            return False
        return self.cfg.announce_all or address in self.watched

    def _on_watcher_event(self, address: str, agent: str, state: str, reason: str) -> None:
        if address not in self.watched:
            return
        has_chains = bool(self.tasks.chains_for(address))
        if has_chains and reason == "blocked":
            # La fuente necesita atención; no lanzamos la cadena todavía y
            # seguimos vigilando por si se desbloquea.
            info = self.tasks.get(address) or {}
            try:
                output = self.herdr.read_agent(address, 60)
            except Exception:
                output = ""
            task = info.get("task") or (
                "La cadena sigue pendiente: el agente de origen está bloqueado."
            )
            self.events.put(("done_prompt", address, agent, reason, output, task))
            return
        if has_chains and reason != "done":
            self.tasks.remove_chains_for(address)
        self.watched.discard(address)
        launched = self._launch_chains(address) if has_chains and reason == "done" else 0
        info = self.tasks.get(address) or {}
        if has_chains and reason != "done" and not info.get("task"):
            info["task"] = "La cadena no se lanzó porque el agente de origen falló."
        self.tasks.remove(address)
        if launched:
            # La cadena ya tiene su propio aviso al terminar el destino.
            return
        if reason == "done" and address in self._deliver_set():
            self._deliver_set().discard(address)
            answer = self._read_answer(address, info.get("task", ""))
            if answer:
                # Por voz y al contexto solo la conclusión, no el registro.
                packet = self._packet_for(answer)
                self.events.put(
                    (
                        "done_answer",
                        address,
                        agent,
                        packet.text or answer,
                        info.get("task", ""),
                    )
                )
                return
        try:
            output = self.herdr.read_agent(address, 60)
        except Exception:
            output = ""
        self.events.put(
            (
                "done_prompt",
                address,
                agent,
                reason,
                output,
                info.get("task", ""),
            )
        )

    def _read_full(self, address: str) -> str:
        """Lectura larga (sin el recorte de 6000 caracteres) de un agente."""
        reader = getattr(self.herdr, "read_full", None)
        if callable(reader):
            return str(reader(address))
        return str(self.herdr.read_agent(address, 200))

    def _read_answer(self, address: str, task: str = "") -> str:
        """Respuesta limpia del agente (sin cabecera ni pie de pantalla)."""
        try:
            screen = self._read_full(address)
        except Exception:  # noqa: BLE001
            return ""
        return answer_or_tail(screen, self._sent_map().get(address) or task)

    def _deliver_set(self) -> set[str]:
        # Creación diferida: algunas pruebas construyen VoiceApp sin __init__.
        if not hasattr(self, "_deliver"):
            self._deliver = set()
        return self._deliver

    def _packet_for(self, answer: str) -> Packet:
        """Qué parte de una salida pasa al siguiente agente (o se dice por voz)."""
        select = None
        client = getattr(getattr(self, "messenger", None), "client", None)
        if client is not None:
            if not hasattr(self, "_condenser"):
                self._condenser = Condenser(client, self.cfg)
            select = self._condenser.conclusion_start
        return build_packet(answer, select=select, save=getattr(self.herdr, "save_context", None))

    def _sent_map(self) -> dict[str, str]:
        """Texto exacto que se pegó a cada agente (ancla para recortar su respuesta)."""
        if not hasattr(self, "_sent"):
            self._sent = {}
        return self._sent

    def _prelaunch_state(self) -> tuple[dict, threading.Lock]:
        if not hasattr(self, "_prelaunch"):
            self._prelaunch = {}
            self._prelaunch_lock = threading.Lock()
        return self._prelaunch, self._prelaunch_lock

    def _deliver_answer(self, address: str) -> None:
        """Marca un agente cuya respuesta se dirá sola al terminar."""
        self._deliver_set().add(str(address))

    def _agent_label(self, agent: str) -> str:
        try:
            key = self.herdr.resolve_agent(agent)
            profile = (getattr(self.cfg, "agent_profiles", {}) or {}).get(key, {})
            return str(profile.get("label") or agent or "El agente")
        except Exception:  # noqa: BLE001
            return str(agent or "El agente")

    def _handle_done_answer(
        self,
        address: str,
        agent: str,
        answer: str,
        task: str = "",
    ) -> None:
        """Dice la respuesta del agente en cuanto termina, sin preguntar."""
        log(f"Respuesta de {agent} ({address}); la digo sin preguntar.")
        self.messenger.note_done(address, agent, "done", answer, task=task)
        spoken = " ".join(answer.split())
        more = len(spoken) > 360
        if more:
            spoken = spoken[:359].rstrip() + "…"
        text = f"{self._agent_label(agent)} respondió: {spoken}"
        if more:
            text += " El resto está en su terminal."
        try:
            self.notch.notice()
        except Exception:  # noqa: BLE001
            pass
        self._queue_announcement(text)

    def _handle_done_prompt(
        self,
        address: str,
        agent: str,
        reason: str,
        output: str,
        task: str = "",
    ) -> None:
        log(f"Aviso de {agent} ({reason}); reabro la escucha para preguntar.")
        self.messenger.note_done(address, agent, reason, output, task=task)
        spoken = " ".join(str(task or "").split())
        if len(spoken) > 140:
            spoken = spoken[:139].rstrip() + "…"
        label = f"«{spoken}»" if spoken else "la tarea"
        if reason == "done":
            question = (
                f"Oye, ya terminé {label}. ¿Quieres que te resuma qué hizo, "
                "que te lea la salida o que te la muestre?"
            )
        else:
            question = (
                f"El agente {agent} necesita tu atención con {label}. "
                "¿Quieres que te diga qué pasa o le das una instrucción?"
            )
        # El aviso visual (spec 9.3) y la pregunta esperan su turno en la cola:
        # así tres tareas que terminan a la vez no se cortan entre ellas.
        try:
            self.notch.notice()
        except Exception:
            pass
        self._queue_announcement(question)

    # ------------------------------------------------------- atajos y ajustes
    @staticmethod
    def _normalize(text: str) -> str:
        raw = unicodedata.normalize("NFKD", text.lower())
        raw = "".join(c for c in raw if not unicodedata.combining(c))
        return re.sub(r"[^a-z0-9 ]+", " ", raw).strip()

    def _fast_command(self, text: str) -> bool:
        """Órdenes de control que no necesitan pasar por el LLM."""
        raw_key = re.sub(r"\s+", " ", self._normalize(text))
        key = _strip_courtesy(raw_key)
        words = raw_key.split()
        if "gracias" in words and all(word in _COURTESY_WORDS for word in words):
            log("Atajo local: agradecimiento.")
            self._speak("De nada.", "answer")
            return True
        # DECISION: «silencio» / «silencia» / «mute» sueltos ya no silencian: Whisper
        # los alucina con ruido y se dicen al hablar con otras personas. Solo una
        # orden explícita que nombre el micrófono silencia por voz.
        mute_words = {
            "silencia el micro",
            "silencia el microfono",
            "silencio el micro",
            "silencio el microfono",
            "mutea el micro",
            "mutea el microfono",
            "silencia mi micro",
            "silencia mi microfono",
            "apaga el micro",
            "apaga el microfono",
            "mutea",
            "muteate",
        }
        unmute_words = {
            "desmute",
            "desmutea",
            "unmute",
            "activa el micro",
            "reactiva el micro",
            "activa el microfono",
        }
        # «Cierra la escucha» sigue el ajuste close_action (pausa o colgar);
        # despedirse o pedir colgar cuelga siempre.
        close_words = {
            "cierra",
            "cierrate",
            "cierra la escucha",
            "cierra el micro",
            "deja de escuchar",
            "callate",
            "pausa",
            "pausa la escucha",
            "pon pausa",
            "pausate",
        }
        hangup_words = {"adios", "cuelga", "termina la llamada", "colgar"}
        repeat_words = {"repite", "repitelo", "otra vez"}
        greet_words = {"hola", "buenas", "hola ezagent", "buenas ezagent"}
        agent_words = {
            "que agentes hay",
            "que agentes tengo",
            "lista los agentes",
            "agentes vivos",
            "cuantos agentes hay",
        }
        task_words = {
            "que tareas tienes",
            "que tareas pendientes",
            "que tareas pendientes tengo",
            "que tareas tengo pendientes",
            "que estas vigilando",
            "que tareas estas vigilando",
            "tareas pendientes",
            "que tareas sigues",
            "que tareas hay pendientes",
        }
        if key in greet_words:
            log("Atajo local: saludo.")
            self._speak("Hola, ¿qué necesitas?", "answer")
            return True
        if key in agent_words:
            log("Atajo local: listar agentes.")
            try:
                agents = self.herdr.list_agents()
            except Exception as exc:  # noqa: BLE001
                log(f"No pude listar agentes: {exc}")
                self._speak("No pude consultar Herdr.", "answer")
                return True
            if not agents:
                self._speak("No hay agentes vivos.", "answer")
                return True
            state_names = {
                "working": "trabajando",
                "done": "terminado",
                "blocked": "bloqueado",
                "idle": "inactivo",
            }
            parts = []
            for agent in agents[:4]:
                name = agent.get("agent") or "agente"
                address = agent.get("address") or "?"
                state = state_names.get(
                    str(agent.get("state") or ""), str(agent.get("state") or "—")
                )
                parts.append(f"{name} en {address}, {state}")
            extra = len(agents) - len(parts)
            suffix = f", y {extra} más" if extra > 0 else ""
            self._speak(
                f"Hay {len(agents)} agentes: " + "; ".join(parts) + suffix + ".",
                "answer",
            )
            return True
        if key in task_words:
            log("Atajo local: listar tareas pendientes.")
            registry = getattr(self, "tasks", None)
            tasks = registry.all() if registry is not None else []
            if not tasks:
                self._speak("No tengo tareas pendientes.", "answer")
                return True
            parts: list[str] = []
            for item in tasks[:4]:
                agent = item.get("agent") or "agente"
                address = item.get("address") or "?"
                task = " ".join(str(item.get("task") or "").split())
                if len(task) > 100:
                    task = task[:99].rstrip() + "…"
                parts.append(f"{agent} en {address}: {task or 'sin descripción'}")
            extra = len(tasks) - len(parts)
            suffix = f"; y {extra} más" if extra > 0 else ""
            self._speak(
                f"Tengo {len(tasks)} tareas pendientes: " + "; ".join(parts) + suffix + ".",
                "answer",
            )
            return True
        if key in mute_words:
            log("Atajo local: mute.")
            self._set_mic_muted(True, source="voz")
            self._speak(self._pause_message(), "answer")
            return True
        if key in unmute_words:
            log("Atajo local: unmute.")
            self._set_mic_muted(False)
            self._speak("Micrófono activo.", "answer")
            return True
        if key in close_words or key in hangup_words:
            hangup = key in hangup_words or self._close_action() != "pause"
            log(f"Atajo local: {'colgar' if hangup else 'pausar la escucha'}.")
            self._close_after_answer = True
            self._close_mode = "hangup" if hangup else "pause"
            self._speak("Listo, cierro." if hangup else self._pause_message(), "answer")
            return True
        if key in repeat_words:
            log("Atajo local: repetir.")
            self._speak(self.last_summary or "No hay nada que repetir todavía.", "answer")
            return True
        return False

    def _on_notch_action(self, action: str) -> None:
        # El WndProc del notch entrega el clic en un hilo nativo. No se toca
        # Tk desde ahí: la intención se publica y _tick la aplica.
        log(f"Notch: {action}")
        self.events.put(("notch_action", action))

    def _notch_action_ui(self, action: str) -> None:
        if action == "mute":
            self._set_mic_muted(not self.muted, source="boton")
        elif action == "listen":
            self._resume_listening_from_mic()
        elif action == "panel":
            self._toggle_menu()
        elif action == "chat":
            self._toggle_chat()
        elif action == "repeat":
            self._handle_hotkey("repeat")
        elif action == "hangup":
            self._handle_hotkey("cancel")
        elif action == "hide":
            self.hide_notch()

    def _toggle_menu(self) -> None:
        """El menú vive dentro de la barra; no hay panel Tk."""
        try:
            self.notch.toggle_menu()
        except Exception as exc:  # noqa: BLE001
            log(f"No pude abrir el menú: {exc}")

    def hide_notch(self, close_eyes: bool = True) -> None:
        self._notch_hidden = True
        self.cfg.notch_hidden = True
        self.cfg.save_key("notch_hidden", True)
        self.notch.hide(close_eyes=close_eyes)
        log("Isla oculta hasta la próxima llamada.")

    # ------------------------------------------------------- menú embebido
    def _refresh_devices(self) -> None:
        try:
            self._devices_cache = list_input_devices()
        except Exception:
            pass

    def _notch_menu_settings(self):
        """Instantánea para el menú; se ejecuta al abrirlo."""
        try:
            from .notch.settings import EDITABLE_KEYS, MenuSettings
        except Exception as exc:  # noqa: BLE001
            log(f"Menú no disponible: {exc}")
            return None
        devices = self._devices_cache
        if not devices:
            # La enumeración de PyAudio no corre en el hilo de render; se
            # refresca en segundo plano y el menú muestra «Sin micrófonos»
            # hasta que termine.
            threading.Thread(target=self._refresh_devices, daemon=True).start()

        values = {key: getattr(self.cfg, key, None) for key in (*EDITABLE_KEYS, *INSTANT_SETTINGS)}
        values["input_device_index"] = self.mic.device_index
        values["llm_profile"] = self.cfg.llm_provider

        providers: dict[str, dict[str, str]] = {}
        try:
            providers = load_provider_catalog()
        except Exception as exc:  # noqa: BLE001
            log(f"No pude leer llm.json: {exc}")
        current = normalize_provider(self.cfg.llm_provider)
        profile = providers.setdefault(current, {})
        profile.setdefault("label", current.title())
        profile["base_url"] = self.cfg.llm_base_url or profile.get("base_url", "")
        profile["model"] = self.cfg.llm_model or profile.get("model", "")
        if self.cfg.api_key:
            profile["api_key"] = self.cfg.api_key
        profile["api_style"] = self.cfg.llm_api_style or profile.get("api_style", "openai")

        try:
            stt_models = available_whisper_models(self.cfg.stt_model)
        except Exception:
            stt_models = ()
        with self._llm_models_lock:
            cached = dict(self._llm_models_cache)
        llm_models = {name: cached.get(name) or provider_models(name) for name in providers}
        self._fetch_llm_models(
            current,
            self.cfg.llm_base_url,
            self.cfg.api_key,
            self.cfg.llm_api_style,
            force=False,
        )
        settings = MenuSettings.from_app(
            values,
            devices,
            MENU_VOICES,
            providers,
            stt_models,
            llm_models,
        )
        if self._menu_saved is None:
            self._menu_saved = settings.saved_snapshot()
        else:
            try:
                settings.restore_saved(self._menu_saved)
            except Exception:
                pass
        return settings

    def _notch_live_setting(self, key: str, value) -> None:
        self.menu_events.put(("live", key, value))

    def _notch_save_settings(self, values: dict) -> None:
        # El guardado real se aplica en _tick (hilo Tk). El menú recibe el
        # resultado por notch.set_save_result(); None = pendiente.
        self.menu_events.put(("save", dict(values)))

    def _notch_test_voice(self, values: dict) -> None:
        self.menu_events.put(("test", dict(values)))

    def _push_custom_colors(self) -> None:
        """Pasa a la barra los cuatro colores del tema personalizado."""
        setter = getattr(self.notch, "set_custom_colors", None)
        if callable(setter):
            setter(
                self.cfg.theme_primary,
                self.cfg.theme_secondary,
                self.cfg.theme_accent,
                self.cfg.theme_background,
            )

    def _apply_custom_theme(self, key: str, value) -> None:
        """Un color o una paleta base: se guardan y el tema pasa a «Personalizado»."""
        from .notch.flow import PRESETS

        if key == "theme_preset":
            preset = PRESETS.get(str(value))
            if preset is None:
                return
            background, primary, secondary, accent = preset
            updates = {
                "theme_background": background,
                "theme_primary": primary,
                "theme_secondary": secondary,
                "theme_accent": accent,
            }
        else:
            updates = {key: value}
        for name, color in updates.items():
            self._apply_instant_setting(name, color, push=False, persist=False)
        self._push_custom_colors()
        if self.cfg.theme != "custom":
            self._apply_instant_setting("theme", "custom", persist=False)

    def _apply_instant_setting(
        self, key: str, value, push: bool = True, persist: bool = True
    ) -> None:
        """Apariencia y escucha: se aplican a la barra en vivo.

        Con ``persist`` también se escriben ya en ``voice.json``; los controles de
        Ajustes pasan ``persist=False`` y se guardan con «Guardar» como los demás.
        """
        try:
            from .notch.settings import normalize_setting

            value = normalize_setting(key, value)
        except Exception as exc:  # noqa: BLE001
            log(f"Ajuste {key} no válido: {exc}")
            return
        setattr(self.cfg, key, value)
        if persist:
            try:
                self.cfg.save_key(key, value)
            except Exception as exc:  # noqa: BLE001
                log(f"No pude guardar {key}: {exc}")
        setter = {
            "ui_layout": "set_layout",
            "theme": "set_theme",
            "theme_base": "set_custom_base",
            "ui_motion": "set_motion",
            "ui_scale": "set_bar_scale",
        }.get(key)
        if key == "chat_voice":  # el interruptor de Ajustes y el del chat son el mismo
            self.chat.set_voice_replies(bool(value))
        method = getattr(self.notch, setter, None) if setter else None
        if callable(method):
            try:
                method(value)
            except Exception as exc:  # noqa: BLE001
                log(f"No pude aplicar {key} a la barra: {exc}")
        if push and key.startswith("theme_") and key != "theme_base":
            self._push_custom_colors()
            if self.cfg.theme != "custom":
                self._apply_instant_setting("theme", "custom", persist=persist)
        log(f"Ajuste {key} = {'(frase)' if key == 'wake_phrase' else value}")

    def _notch_hide(self) -> None:
        self.menu_events.put(("hide",))

    def _handle_menu_event(self, event: tuple) -> None:
        kind = event[0]
        if kind == "live":
            self._apply_live_setting(str(event[1]), event[2])
        elif kind == "save":
            self._save_from_menu(dict(event[1]))
        elif kind == "test":
            self._test_voice_from_menu(dict(event[1]))
        elif kind == "hide":
            # El botón «–» oculta con retirada, sin cerrar los ojos (A-02).
            self.hide_notch(close_eyes=False)

    def _apply_live_setting(self, key: str, value) -> None:
        """Aplica un control del menú en vivo, sin persistir todavía."""
        if key == "theme_preset":
            self._apply_custom_theme(key, value)
            return
        if key in INSTANT_SETTINGS:
            self._apply_instant_setting(key, value, persist=False)
            return
        try:
            if key == "input_device_index":
                if value is None or int(value) < 0:
                    return
                index = int(value)
                if index != self.mic.device_index:
                    self._reload_mic(index)
                return
            if key == "llm_profile":
                try:
                    profile = load_provider_catalog().get(str(value))
                except Exception:
                    profile = None
                if profile:
                    self.cfg.llm_provider = normalize_provider(str(value))
                    self.cfg.llm_model = profile.get("model", "")
                    self.cfg.llm_base_url = profile.get("base_url", "")
                    self.cfg.api_key = profile.get("api_key", "")
                    self.cfg.llm_api_style = profile.get("api_style", "openai")
                    self.messenger.reload_llm()
                    self._fetch_llm_models(
                        self.cfg.llm_provider,
                        self.cfg.llm_base_url,
                        self.cfg.api_key,
                        self.cfg.llm_api_style,
                    )
                return
            if key == "min_rms":
                self.cfg.min_rms = float(value)
                self.mic.min_rms = float(value)
            elif key == "noise_multiplier":
                self.cfg.noise_multiplier = float(value)
                self.mic.noise_multiplier = float(value)
            elif key == "silence_secs":
                self.cfg.silence_secs = float(value)
                self.mic.silence_secs = float(value)
            elif key == "pre_roll_secs":
                self.cfg.pre_roll_secs = float(value)
                self.mic.pre_roll_secs = float(value)
            elif key == "min_speech_secs":
                self.cfg.min_speech_secs = float(value)
                self.mic.min_speech_secs = float(value)
            elif key == "max_utterance_secs":
                self.cfg.max_utterance_secs = float(value)
                self.mic.max_utterance_secs = float(value)
            elif key == "barge_in_rms":
                self.cfg.barge_in_rms = float(value)
                self.mic.barge_in_rms = float(value)  # en vivo, sin colgar
            elif key == "voice":
                self.cfg.voice = str(value)
                self.speaker.set_voice(self.cfg.voice)
            elif key == "speed":
                self.cfg.speed = float(value)
            elif key == "tts_sentence_pause":
                self.cfg.tts_sentence_pause = float(value)
            elif key == "tts_clause_pause":
                self.cfg.tts_clause_pause = float(value)
            elif key == "tts_model_variant":
                self.cfg.tts_model_variant = str(value)
            elif key == "kokoro_model_path":
                self.cfg.kokoro_model_path = str(value)
            elif key == "kokoro_voices_path":
                self.cfg.kokoro_voices_path = str(value)
            elif key == "whisper_url":
                self.cfg.whisper_url = str(value)
            elif key == "stt_model":
                self.cfg.stt_model = str(value)
            elif key == "stt_engine":
                self.cfg.stt_engine = str(value)
            elif key == "stt_prompt":
                self.cfg.stt_prompt = str(value)
            elif key == "stt_timeout_secs":
                self.cfg.stt_timeout_secs = float(value)
            elif key == "auto_start_whisper":
                self.cfg.auto_start_whisper = bool(value)
            elif key == "llm_provider":
                self.cfg.llm_provider = normalize_provider(str(value))
            elif key in ("llm_model", "llm_model_pick"):
                if str(value or "").strip():
                    self.cfg.llm_model = str(value).strip()
            elif key == "llm_api_style":
                self.cfg.llm_api_style = str(value)
            elif key == "llm_base_url":
                self.cfg.llm_base_url = str(value)
            elif key == "api_key":
                self.cfg.api_key = str(value)
            elif key == "llm_temperature":
                self.cfg.llm_temperature = float(value)
            elif key == "llm_max_tokens":
                self.cfg.llm_max_tokens = int(round(float(value)))
            elif key == "llm_context_tokens":
                self.cfg.llm_context_tokens = int(round(float(value)))
            elif key == "llm_reasoning_effort":
                self.cfg.llm_reasoning_effort = str(value)
            elif key in ("smart_turn", "turn_threshold"):
                if key == "smart_turn":
                    self.cfg.smart_turn = bool(value)
                else:
                    self.cfg.turn_threshold = float(value)
                self._apply_turn_settings()
            elif key == "barge_in":
                self.cfg.barge_in = bool(value)
            elif key == "confirm_actions":
                self.cfg.confirm_actions = bool(value)
                self.messenger.set_confirm_actions(self.cfg.confirm_actions)
            elif key in HOTKEY_KEYS:
                spec = str(value or "").strip()
                parse_hotkey(spec)
                candidate = {item: str(getattr(self.cfg, item, "") or "") for item in HOTKEY_KEYS}
                candidate[key] = spec
                self._check_hotkey_duplicates(candidate)
                setattr(self.cfg, key, spec)
                self._reload_hotkeys()
        except Exception as exc:  # noqa: BLE001
            log(f"Ajuste en vivo {key}: {exc}")

    def _save_from_menu(self, values: dict) -> None:
        ok = False
        try:
            self._apply_settings(values)
            self._menu_saved = dict(values)
            ok = True
        except Exception as exc:  # noqa: BLE001
            log(f"No pude guardar los ajustes: {exc}")
        if ok:
            try:
                self.notch.refresh_menu_settings()
            except Exception:
                pass
        try:
            self.notch.set_save_result(ok)
        except Exception:
            pass

    def _test_voice_from_menu(self, values: dict) -> None:
        phrase = "Hola, soy tu asistente de voz."
        voice = str(values.get("voice") or self.cfg.voice)
        speed = float(values.get("speed") or self.cfg.speed)
        old_muted = self.muted
        old_barge = self.cfg.barge_in
        self.cfg.barge_in = False
        if not old_muted:
            self._set_mic_muted(True)
        try:
            self.notch.set_testing(True)
        except Exception:
            pass
        self._set_state(SPEAKING)

        def run() -> None:
            old_voice = self.cfg.voice
            old_speed = self.cfg.speed
            try:
                self.cfg.voice = voice
                self.cfg.speed = speed
                self.speaker.set_voice(voice)
                self.speaker.speak(
                    phrase,
                    threading.Event(),
                    on_level=lambda level: self.notch.draw(level, SPEAKING),
                )
            except Exception as exc:  # noqa: BLE001
                log(f"Prueba de voz: {exc}")
            finally:
                self.cfg.voice = old_voice
                self.cfg.speed = old_speed
                try:
                    self.speaker.set_voice(old_voice)
                except Exception:
                    pass
                self.events.put(("test_done", old_muted, old_barge))

        threading.Thread(target=run, daemon=True).start()

    def _reload_mic(self, index: int | None, *, restart: bool = True) -> None:
        was_in_call = self.in_call
        if was_in_call:
            self._hang_up()
        old = self.mic
        try:
            new = MicCapture(
                device_index=index,
                min_rms=self.cfg.min_rms,
                silence_secs=self.cfg.silence_secs,
                pre_roll_secs=self.cfg.pre_roll_secs,
                min_speech_secs=self.cfg.min_speech_secs,
                max_utterance_secs=self.cfg.max_utterance_secs,
                noise_multiplier=self.cfg.noise_multiplier,
                barge_in_rms=self.cfg.barge_in_rms,
                barge_in_secs=self.cfg.barge_in_secs,
            )
        except Exception:
            raise
        try:
            old.close()
        except Exception:
            pass
        self.mic = new
        self.cfg.input_device_index = new.device_index
        log(f"Micrófono: {new.device_name} (índice {new.device_index})")
        if was_in_call and restart:
            self.root.after(250, self._start_call)

    def _fix_model_name(self, provider: str, models: tuple[str, ...]) -> None:
        """Completa el id del modelo activo con el de la lista real y lo guarda."""
        if provider != self.cfg.llm_provider:
            return
        fixed = match_model(self.cfg.llm_model, models)
        if fixed == self.cfg.llm_model:
            return
        log(f"Modelo «{self.cfg.llm_model}» corregido a «{fixed}» según {self._llm_label()}.")
        self.cfg.llm_model = fixed
        try:
            save_provider_profile(
                self.cfg.llm_provider,
                model=fixed,
                base_url=self.cfg.llm_base_url,
                api_key=self.cfg.api_key,
                api_style=self.cfg.llm_api_style,
            )
            self.cfg.save()
        except Exception as exc:  # noqa: BLE001
            log(f"No pude guardar el modelo corregido: {exc}")
        self._saved_llm = (
            self.cfg.llm_provider,
            self.cfg.llm_model,
            self.cfg.llm_base_url,
            self.cfg.api_key,
            self.cfg.llm_api_style,
        )
        self.messenger.reload_llm()

    def _llm_label(self) -> str:
        """Nombre del proveedor activo para los mensajes de error."""
        spec = provider_spec(self.cfg.llm_provider)
        return spec.label if spec is not None else str(self.cfg.llm_provider or "LLM")

    def _fetch_llm_models(
        self,
        provider: str,
        base_url: str,
        api_key: str,
        api_style: str,
        *,
        force: bool = True,
    ) -> None:
        """Pide en un hilo la lista real de modelos y la pasa al menú.

        Sin ``force`` (al abrir el menú) solo se pide una vez por endpoint.
        """
        if not str(base_url or "").strip():
            return
        attempt = (provider, str(base_url), str(api_style))
        with self._llm_models_lock:
            if not force and attempt in self._llm_models_tried:
                return
            self._llm_models_tried.add(attempt)

        def worker() -> None:
            try:
                models = tuple(fetch_models(base_url, api_key, api_style))
            except Exception as exc:  # noqa: BLE001 - sin lista, quedan sugerencias
                log(f"Lista de modelos de {provider}: {exc}")
                return
            if not models:
                return
            with self._llm_models_lock:
                self._llm_models_cache[provider] = models
            # Si el modelo guardado viene sin prefijo («qwen3-32b» en vez de
            # «qwen/qwen3-32b»), el hilo principal lo corrige y lo guarda.
            self.events.put(("llm_models_ready", provider, models))
            try:
                self.notch.update_llm_models(provider, models)
            except Exception:
                pass

        threading.Thread(target=worker, name="ezagent-models", daemon=True).start()

    def _apply_settings(self, values: dict) -> bool:
        """Aplica y guarda los valores del panel; True si reinició la llamada."""
        old_device = self.mic.device_index
        was_in_call = self.in_call
        device_changed = False
        restarted = False
        old_hotkeys = tuple(str(getattr(self.cfg, key, "") or "") for key in HOTKEY_KEYS)
        hotkey_values: dict[str, str] = {}
        # Valida los atajos antes de tocar la configuración: un spec inválido
        # no debe dejar el resto de ajustes a medio aplicar.
        for key in HOTKEY_KEYS:
            spec = str(values.get(key, getattr(self.cfg, key, "")) or "").strip()
            try:
                parse_hotkey(spec)
            except ValueError as exc:
                raise ValueError(f"Atajo {key}: {exc}") from exc
            hotkey_values[key] = spec
        self._check_hotkey_duplicates(hotkey_values)
        old_llm = self._saved_llm
        old_tts = (
            self.cfg.tts_model_variant,
            self.cfg.kokoro_model_path,
            self.cfg.kokoro_voices_path,
        )

        def number(key: str, default: float) -> float:
            return float(values.get(key, default))

        self.cfg.min_rms = number("min_rms", self.cfg.min_rms)
        self.cfg.silence_secs = number("silence_secs", self.cfg.silence_secs)
        self.cfg.noise_multiplier = number("noise_multiplier", self.cfg.noise_multiplier)
        self.cfg.pre_roll_secs = number("pre_roll_secs", self.cfg.pre_roll_secs)
        self.cfg.min_speech_secs = number("min_speech_secs", self.cfg.min_speech_secs)
        self.cfg.max_utterance_secs = number("max_utterance_secs", self.cfg.max_utterance_secs)
        self.cfg.barge_in_rms = number("barge_in_rms", self.cfg.barge_in_rms)
        self.cfg.voice = str(values.get("voice", self.cfg.voice))
        self.cfg.speed = number("speed", self.cfg.speed)
        self.cfg.tts_sentence_pause = number("tts_sentence_pause", self.cfg.tts_sentence_pause)
        self.cfg.tts_clause_pause = number("tts_clause_pause", self.cfg.tts_clause_pause)
        self.cfg.tts_model_variant = str(
            values.get("tts_model_variant", self.cfg.tts_model_variant)
        )
        self.cfg.kokoro_model_path = str(
            values.get("kokoro_model_path", self.cfg.kokoro_model_path)
        )
        self.cfg.kokoro_voices_path = str(
            values.get("kokoro_voices_path", self.cfg.kokoro_voices_path)
        )
        self.cfg.whisper_url = str(values.get("whisper_url", self.cfg.whisper_url))
        self.cfg.stt_model = str(values.get("stt_model", self.cfg.stt_model))
        self.cfg.stt_engine = str(values.get("stt_engine", self.cfg.stt_engine))
        self.cfg.stt_prompt = str(values.get("stt_prompt", self.cfg.stt_prompt))
        self.cfg.stt_timeout_secs = number("stt_timeout_secs", self.cfg.stt_timeout_secs)
        self.cfg.auto_start_whisper = bool(
            values.get("auto_start_whisper", self.cfg.auto_start_whisper)
        )
        self.cfg.llm_provider = normalize_provider(
            str(values.get("llm_provider", self.cfg.llm_provider))
        )
        self.cfg.llm_model = str(values.get("llm_model", self.cfg.llm_model)).strip()
        self.cfg.llm_base_url = str(values.get("llm_base_url", self.cfg.llm_base_url)).strip()
        self.cfg.api_key = str(values.get("api_key", self.cfg.api_key)).strip()
        self.cfg.llm_api_style = str(
            values.get("llm_api_style", self.cfg.llm_api_style) or "openai"
        )
        self.cfg.llm_temperature = number("llm_temperature", self.cfg.llm_temperature)
        self.cfg.llm_max_tokens = int(round(number("llm_max_tokens", self.cfg.llm_max_tokens)))
        self.cfg.llm_context_tokens = int(
            round(number("llm_context_tokens", self.cfg.llm_context_tokens))
        )
        self.cfg.llm_reasoning_effort = str(
            values.get("llm_reasoning_effort", self.cfg.llm_reasoning_effort)
        )
        self.cfg.barge_in = bool(values.get("barge_in", self.cfg.barge_in))
        self.cfg.smart_turn = bool(values.get("smart_turn", self.cfg.smart_turn))
        self.cfg.turn_threshold = number("turn_threshold", self.cfg.turn_threshold)
        self._apply_turn_settings()
        self.cfg.confirm_actions = bool(values.get("confirm_actions", self.cfg.confirm_actions))

        new_llm = (
            self.cfg.llm_provider,
            self.cfg.llm_model,
            self.cfg.llm_base_url,
            self.cfg.api_key,
            self.cfg.llm_api_style,
        )
        with self._llm_models_lock:
            known = self._llm_models_cache.get(self.cfg.llm_provider, ())
        fixed = match_model(self.cfg.llm_model, known)
        if fixed != self.cfg.llm_model:
            log(f"Modelo «{self.cfg.llm_model}» completado como «{fixed}».")
            self.cfg.llm_model = fixed
            new_llm = (
                self.cfg.llm_provider,
                self.cfg.llm_model,
                self.cfg.llm_base_url,
                self.cfg.api_key,
                self.cfg.llm_api_style,
            )
        if new_llm != old_llm:
            if not self.cfg.llm_base_url:
                raise ValueError("El proveedor LLM necesita un endpoint (URL).")
            if not self.cfg.llm_model:
                raise ValueError("El proveedor LLM necesita un modelo.")
            save_provider_profile(
                self.cfg.llm_provider,
                model=self.cfg.llm_model,
                base_url=self.cfg.llm_base_url,
                api_key=self.cfg.api_key,
                api_style=self.cfg.llm_api_style,
            )
            self.cfg.llm_pinned = True
            # Con URL o clave nuevas, la lista de modelos puede ser otra.
            self._fetch_llm_models(
                self.cfg.llm_provider,
                self.cfg.llm_base_url,
                self.cfg.api_key,
                self.cfg.llm_api_style,
            )

        new_device = values.get("input_device_index")
        if new_device is not None and int(new_device) != old_device:
            self._reload_mic(int(new_device), restart=False)
            device_changed = True
        else:
            self.mic.min_rms = self.cfg.min_rms
            self.mic.silence_secs = self.cfg.silence_secs
            self.mic.noise_multiplier = self.cfg.noise_multiplier
            self.mic.pre_roll_secs = self.cfg.pre_roll_secs
            self.mic.min_speech_secs = self.cfg.min_speech_secs
            self.mic.max_utterance_secs = self.cfg.max_utterance_secs
            self.mic.barge_in_rms = self.cfg.barge_in_rms
            self.mic.barge_in_secs = self.cfg.barge_in_secs
        new_tts = (
            self.cfg.tts_model_variant,
            self.cfg.kokoro_model_path,
            self.cfg.kokoro_voices_path,
        )
        if new_tts != old_tts:
            self.speaker.reload_model()
        self.speaker.set_voice(self.cfg.voice)
        self.messenger.set_confirm_actions(self.cfg.confirm_actions)
        agent = getattr(self, "agent", None)
        if agent is not None:
            try:
                agent.set_confirm_actions(self.cfg.confirm_actions)
            except Exception:
                pass
        if new_llm != old_llm:
            self.messenger.reload_llm()
        for key, spec in hotkey_values.items():
            setattr(self.cfg, key, spec)
        # Apariencia y escucha ya están aplicadas en vivo; aquí se confirman y se escriben.
        for key in INSTANT_SETTINGS:
            if key in values:
                setattr(self.cfg, key, values[key])
        self.cfg.save()
        if tuple(str(getattr(self.cfg, key, "") or "") for key in HOTKEY_KEYS) != old_hotkeys:
            self._reload_hotkeys()
        self._saved_llm = new_llm
        if (
            self.cfg.stt_model != self._saved_stt_model
            or self.cfg.stt_engine != self._saved_stt_engine
        ):
            self._saved_stt_model = self.cfg.stt_model
            self._saved_stt_engine = self.cfg.stt_engine
            self._restart_whisper_async()

        if was_in_call and device_changed:
            # Los umbrales del VAD se leen en cada bloque (audio.py), así que
            # no hace falta reiniciar la llamada. El dispositivo sí exige
            # reabrir el stream.
            if self.in_call:
                self._hang_up()
            self.root.after(250, self._start_call)
            restarted = True
        log("Ajustes guardados.")
        return restarted

    def _restart_whisper_async(self) -> None:
        """Aplica el modelo elegido si el whisper-server lo arrancó esta app."""
        proc = self._whisper_proc
        if proc is None:
            log("Modelo de Whisper guardado; se aplicará al reiniciar whisper-server.")
            return

        def run() -> None:
            try:
                if proc.poll() is None:
                    subprocess.run(
                        ["taskkill", "/PID", str(proc.pid), "/T", "/F"],
                        stdout=subprocess.DEVNULL,
                        stderr=subprocess.DEVNULL,
                        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
                    )
            except Exception:
                pass
            self._whisper_proc = None
            try:
                self._whisper_proc = ensure_whisper(self.cfg)
            except Exception as exc:  # noqa: BLE001
                log(f"No pude reiniciar whisper-server: {exc}")

        threading.Thread(target=run, daemon=True).start()

    def _request_close(self) -> None:
        self._close_after_answer = True

    # ------------------------------------------------- pausa y reactivación
    def _close_action(self) -> str:
        cfg = getattr(self, "cfg", None)
        value = str(getattr(cfg, "close_action", "pause") or "pause").lower()
        return value if value in ("pause", "hangup") else "pause"

    def _pause_message(self) -> str:
        """Texto de la pausa; lo genera el código, nunca el LLM."""
        cfg = getattr(self, "cfg", None)
        if not bool(getattr(cfg, "voice_wake", True)):
            return "En pausa. Toca el micrófono para volver."
        phrase = str(getattr(cfg, "wake_phrase", "") or DEFAULT_WAKE_PHRASE)
        return f"En pausa. Di «{phrase}» cuando me necesites."

    def _close_listening(self) -> bool:
        """Cierra la escucha tras una respuesta; True si la llamada sigue (pausa)."""
        mode = getattr(self, "_close_mode", "") or self._close_action()
        self._close_mode = ""
        if mode != "pause" or not self.in_call:
            self._hang_up()
            return False
        self._set_mic_muted(True, source="cierre")
        self._set_state(LISTENING)
        return True

    def _llm_mic_mute(self, muted: bool) -> None:
        """Herramienta mic_mute del LLM (hilo de herramientas).

        DECISION: el LLM ya no silencia de golpe. Pedir silencio deja la
        escucha en pausa al terminar la respuesta (reversible con el botón o la
        frase de reactivación); pedir quitarlo se ignora porque en pausa nada
        llega al LLM.
        """
        if not muted:
            log("El modelo pidió activar el micro; ya está activo.")
            return
        self.events.put(("close_request", "pause"))

    def _wake_guard(self) -> threading.Lock:
        lock = getattr(self, "_wake_lock", None)
        if lock is None:
            lock = threading.Lock()
            self._wake_lock = lock
        return lock

    def _maybe_wake(self, pcm: bytes, rate: int) -> None:
        """En pausa: solo una frase corta se transcribe, para buscar la reactivación."""
        cfg = getattr(self, "cfg", None)
        if not self.in_call or not bool(getattr(cfg, "voice_wake", False)):
            return
        if not worth_transcribing(pcm, rate):
            return
        lock = self._wake_guard()
        if not lock.acquire(blocking=False):
            return  # ya hay una comprobación en curso
        threading.Thread(target=self._wake_worker, args=(pcm, rate, lock), daemon=True).start()

    def _wake_worker(self, pcm: bytes, rate: int, lock: threading.Lock) -> None:
        try:
            text = transcribe(self.cfg, pcm_to_wav(pcm, rate))
        except Exception as exc:  # noqa: BLE001
            log(f"Pausa: no pude comprobar la frase ({type(exc).__name__}).")
            return
        finally:
            lock.release()
        # En pausa no se registra lo transcrito: solo si era la frase.
        if is_wake_phrase(text, str(getattr(self.cfg, "wake_phrase", "") or "")):
            self.events.put(("wake",))

    def _on_wake(self) -> None:
        if not self.in_call or not self.muted:
            return
        log("Frase de reactivación: vuelvo a escuchar.")
        self._set_mic_muted(False, source="frase")
        self._speak("Dime.", "answer")

    def _set_mic_muted(self, muted: bool, source: str = "") -> None:
        self.muted = bool(muted)
        self.notch.set_muted(self.muted)
        self._reset_capture.set()
        if self.muted:
            self._ignore_utterance = True
            log(f"Micrófono silenciado ({source or 'sin origen'}).")
        else:
            self._ignore_utterance = False
            log(f"Micrófono activo ({source or 'sin origen'}).")
        self._update_tray_title()

    def _remember_task(self, address: str, agent: str = "", task: str = "") -> None:
        try:
            self.tasks.add(address, agent, task)
        except Exception as exc:  # noqa: BLE001
            log(f"No pude guardar la tarea de {address}: {exc}")

    def _start_prelaunch(self, spec: dict) -> None:
        """Abre ya el agente destino de una cadena, mientras la fuente trabaja.

        Así, cuando la fuente termina solo falta pegar el texto: el arranque
        del programa (varios segundos) ocurre en paralelo con el trabajo.
        """
        chain_id = str(spec.get("chain_id") or "")
        agent = str(spec.get("agent") or "").strip()
        if not chain_id or not agent or spec.get("background") or spec.get("gui"):
            return
        entry: dict = {"event": threading.Event(), "address": ""}
        registry, lock = self._prelaunch_state()
        with lock:
            if chain_id in registry:
                return  # ya se está abriendo (calentado antes de lanzar el origen)
            registry[chain_id] = entry

        def work() -> None:
            try:
                kwargs: dict = {
                    "cwd": spec.get("cwd") or None,
                    "name": spec.get("name") or None,
                    "confirmed": True,
                    "space": spec.get("space") or None,
                }
                if spec.get("model"):
                    kwargs["model"] = str(spec.get("model"))
                if spec.get("effort"):
                    kwargs["effort"] = str(spec.get("effort"))
                result = self.herdr.run_agent(agent, "", **kwargs)
                if result.get("ok"):
                    entry["address"] = str(result.get("address") or result.get("terminal") or "")
                else:
                    log(f"No pude abrir {agent} por adelantado: {result.get('error')}")
            except Exception as exc:  # noqa: BLE001
                log(f"No pude abrir {agent} por adelantado: {exc}")
            finally:
                entry["event"].set()

        threading.Thread(target=work, daemon=True, name="ez-prelaunch").start()

    def _take_prelaunched(self, spec: dict) -> str:
        """Dirección del agente abierto por adelantado (o '' si no hay)."""
        registry, lock = self._prelaunch_state()
        with lock:
            entry = registry.pop(str(spec.get("chain_id") or ""), None)
        if not entry:
            return ""
        entry["event"].wait(timeout=30)
        return str(entry.get("address") or "")

    def _enqueue_chain(self, spec: dict) -> None:
        """Guarda una cadena y asegura que la fuente esté vigilada."""
        try:
            spec = dict(spec)
            spec.setdefault("chain_id", uuid.uuid4().hex)
            self.tasks.add_chain(spec)
            self._start_prelaunch(spec)
            source = str(spec.get("source") or "").strip()
            if source:
                self.watched.add(source)
                self._check_task_now(source)
            log(
                f"Cadena registrada: {source} → "
                f"{spec.get('agent')} (tarea: {str(spec.get('task'))[:80]})"
            )
        except Exception as exc:  # noqa: BLE001
            log(f"No pude guardar la cadena: {exc}")

    def _launch_chains(self, source: str) -> int:
        """Lanza los agentes encadenados cuando la fuente termina."""
        chains = self.tasks.remove_chains_for(source)
        if not chains:
            return 0
        try:
            output = self._read_full(source)
        except Exception:
            output = ""
        source_task = self._sent_map().get(source) or str(
            (self.tasks.get(source) or {}).get("task") or ""
        )
        packet = self._packet_for(answer_or_tail(output, source_task))
        launched = 0
        for spec in chains:
            agent = str(spec.get("agent") or "").strip()
            task = str(spec.get("task") or "").strip()
            if not agent:
                log(f"Cadena sin agente destino para {source}.")
                continue
            kwargs: dict = {
                "cwd": spec.get("cwd") or None,
                "name": spec.get("name") or None,
                "confirmed": True,
                "background": bool(spec.get("background")),
                "gui": bool(spec.get("gui")),
                "space": spec.get("space") or None,
            }
            if spec.get("model"):
                kwargs["model"] = str(spec.get("model"))
            if spec.get("effort"):
                kwargs["effort"] = str(spec.get("effort"))
            handoff = compose_handoff(task, packet_text(packet))
            try:
                ready = self._take_prelaunched(spec)
                if ready:
                    sender = getattr(self.herdr, "send_task", None)
                    sent = (
                        sender(ready, handoff)
                        if callable(sender)
                        else self.herdr.send_agent(ready, handoff, confirmed=True)
                    )
                    result = {
                        "ok": bool(sent.get("ok")),
                        "address": ready,
                        "error": sent.get("error"),
                    }
                else:
                    result = self.herdr.run_agent(agent, handoff, **kwargs)
            except Exception as exc:  # noqa: BLE001
                log(f"No pude lanzar la cadena {source} → {agent}: {exc}")
                continue
            if not result.get("ok"):
                log(f"La cadena {source} → {agent} falló: {result.get('error')}")
                continue
            address = str(result.get("address") or result.get("terminal") or "").strip()
            if address:
                self._sent_map()[address] = handoff
            launched += 1
            rest = spec.get("then")
            if address and isinstance(rest, list) and rest:
                # Siguiente paso del flujo: espera a que termine este.
                self._enqueue_chain(
                    chain_spec(
                        address,
                        rest,
                        cwd=spec.get("cwd") or None,
                        space=spec.get("space") or None,
                        notify=bool(spec.get("notify_final")),
                        deliver=bool(spec.get("deliver_final")),
                    )
                )
            if address and spec.get("deliver_answer"):
                self._deliver_answer(address)
            if address and (spec.get("notify_on_done") or spec.get("deliver_answer")):
                shown = task.replace("{output}", "").strip(" :")
                self._remember_task(address, agent, shown or task)
                self._watch(address)
                log(f"Cadena lanzada: {source} → {agent} ({address}); avisaré al terminar.")
            else:
                log(f"Cadena lanzada: {source} → {agent}.")
        return launched

    def _resume_pending_tasks(self) -> None:
        """Recupera tareas y cadenas pendientes tras un reinicio."""
        for item in self.tasks.all():
            address = str(item.get("address") or "").strip()
            if not address:
                continue
            self.watched.add(address)
            self._check_task_now(address)
        for chain in self.tasks.chains():
            source = str(chain.get("source") or "").strip()
            if not source:
                continue
            self.watched.add(source)
            self._check_task_now(source)

    def _check_task_now(self, address: str) -> None:
        """Si el agente ya terminó, emite el aviso o lanza su cadena."""
        try:
            agent = self.herdr.get_agent(address)
        except Exception:
            return
        state = agent.get("state")
        if state not in ("done", "blocked", "error"):
            return
        had_chains = bool(self.tasks.chains_for(address))
        if had_chains:
            if state == "blocked":
                # Se queda vigilado para lanzar la cadena cuando se desbloquee.
                info = self.tasks.get(address) or {}
                try:
                    output = self.herdr.read_agent(address, 60)
                except Exception:
                    output = ""
                self.events.put(
                    (
                        "done_prompt",
                        address,
                        agent.get("agent") or info.get("agent") or "agente",
                        "blocked",
                        output,
                        info.get("task")
                        or ("La cadena sigue pendiente: el agente de origen está bloqueado."),
                    )
                )
                return
            if state == "error":
                self.tasks.remove_chains_for(address)
            else:
                self.watched.discard(address)
                self._launch_chains(address)
                return
        info = self.tasks.get(address)
        if info is None:
            if state == "error" and had_chains:
                info = {"task": ("La cadena no se lanzó porque el agente de origen falló.")}
            else:
                self.watched.discard(address)
                return
        self.watched.discard(address)
        self.tasks.remove(address)
        try:
            output = self.herdr.read_agent(address, 60)
        except Exception:
            output = ""
        self.events.put(
            (
                "done_prompt",
                address,
                agent.get("agent") or info.get("agent") or "agente",
                state,
                output,
                info.get("task", ""),
            )
        )

    def _watch(self, address: str) -> None:
        self.watched.add(address)
        if self.tasks.get(address) is None:
            user_text = ""
            messenger = getattr(self, "messenger", None)
            if messenger is not None:
                user_text = str(getattr(messenger, "_last_user_text", "") or "")
            self._remember_task(address, "", user_text)
        self._check_task_now(address)

    def _mute(self, minutes: int) -> None:
        self.mute_until = time.time() + max(1, minutes) * 60
        log(f"Avisos silenciados {minutes} min.")


def _print_agents(cfg: VoiceConfig) -> None:
    herdr = Herdr(cfg)
    for agent in herdr.list_agents():
        print(
            f"{agent['address']:>4}  {agent['agent'] or '?':<14} "
            f"{agent['state'] or '?':<8} {agent['cwd'] or ''}  "
            f"{agent['where'] or ''}"
        )


def _print_mics() -> None:
    for device in list_input_devices():
        print(
            f"{device['index']:>3}  {device['name']:<45} "
            f"{device['rate']:>6} Hz  {device['channels']} canales"
        )


def run_demo() -> None:
    """Recorre los estados de la spec 13 sin pipeline de voz.

    Útil para revisar poses, voz simulada, aviso, pausa y error en la ventana
    real. No crea micrófono, whisper, Kokoro ni Herdr.
    """
    root = tk.Tk()
    root.withdraw()
    notch = create_ui(root, VoiceConfig())
    notch.set_theme(VoiceConfig().theme)
    try:
        from .notch.settings import MenuSettings

        settings = MenuSettings.from_app(
            {
                "input_device_index": None,
                "min_rms": 0.0006,
                "noise_multiplier": 1.3,
                "silence_secs": 1.3,
                "voice": "em_alex",
                "speed": 0.95,
                "barge_in": True,
                "confirm_actions": False,
            },
            [],
            MENU_VOICES,
        )
        notch.menu_settings_provider = lambda: settings
        notch.on_save_settings = lambda values: True
        notch.on_hide = lambda: notch.hide()
    except Exception:
        pass

    steps = [
        ("open", None),
        ("idle", None),
        ("listen", None),
        ("think", None),
        ("speak", None),
        ("notice", None),
        ("mute", None),
        ("idle", None),
        ("error", None),
        ("idle", None),
        ("menu", None),
        ("menu", None),
        ("idle", None),
        ("close", None),
    ]
    index = 0
    level_t0 = time.monotonic()

    def enter(state: str) -> None:
        if state == "open":
            notch.show(IDLE)
            return
        if state == "close":
            notch.hide()
            return
        if state == "menu":
            notch.toggle_menu()
            return
        if state == "notice":
            notch.set_muted(False)
            notch.notice()
            return
        if state == "mute":
            notch.set_muted(True)
            notch.set_state(IDLE)
            return
        if state == "error":
            notch.set_state("error")
            return
        notch.set_muted(False)
        notch.set_state(
            {"idle": IDLE, "listen": LISTENING, "think": THINKING, "speak": SPEAKING}.get(
                state, IDLE
            )
        )

    def step() -> None:
        nonlocal index
        if index >= len(steps):
            notch.close()
            root.destroy()
            return
        state, _ = steps[index]
        enter(state)
        index += 1
        root.after(2000, step)

    def tick() -> None:
        t = time.monotonic() - level_t0
        state = steps[min(index - 1, len(steps) - 1)][0] if index else "idle"
        if state in ("listen", "speak"):
            rms = 0.004 + 0.016 * (0.5 + 0.5 * math.sin(t * 5.1))
            notch.draw(rms, LISTENING if state == "listen" else SPEAKING)
        root.after(33, tick)

    step()
    tick()
    try:
        root.mainloop()
    finally:
        try:
            notch.close()
        except Exception:
            pass
        try:
            root.destroy()
        except Exception:
            pass


def main() -> None:
    # Antes de crear Tk o cualquier ventana Win32.
    try:
        from .notch.layered import enable_dpi_awareness

        enable_dpi_awareness()
    except Exception:
        pass
    env_candidates = [
        _resource_root() / ".env",
        Path.cwd() / ".env",
    ]
    if getattr(sys, "frozen", False):
        env_candidates.insert(0, Path(sys.executable).resolve().parent / ".env")
    env_path = next((path for path in env_candidates if path.exists()), None)
    load_dotenv(env_path)
    parser = argparse.ArgumentParser(description="EzAgent Voice — mensajero para Herdr")
    parser.add_argument("--config", type=Path, default=None)
    parser.add_argument("--ask", help="Pregunta única (sin micrófono) y sale")
    parser.add_argument("--say", help="Dice un texto por Kokoro y sale")
    parser.add_argument("--list-agents", action="store_true")
    parser.add_argument("--list-mics", action="store_true")
    parser.add_argument(
        "--demo-ui",
        action="store_true",
        help="Recorre los estados del notch sin micrófono ni servidores",
    )
    parser.add_argument("--mic", type=int, default=None, help="Índice de micrófono")
    parser.add_argument(
        "--write-config",
        action="store_true",
        help="Escribe voice.json con los valores actuales y sale",
    )
    args = parser.parse_args()

    if args.demo_ui:
        run_demo()
        return

    if args.list_mics:
        _print_mics()
        return

    cfg = VoiceConfig.load(args.config)
    if args.mic is not None:
        cfg.input_device_index = args.mic

    if args.write_config:
        path = args.config or config_path()
        cfg.save(path)
        print(path)
        return

    if args.list_agents:
        _print_agents(cfg)
        return

    if args.say:
        speaker = Speaker(cfg)
        try:
            speaker.speak(args.say, threading.Event())
        finally:
            speaker.close()
        return

    if not cfg.api_key and needs_api_key(cfg.llm_provider, cfg.llm_base_url):
        log(
            f"Falta la API key del proveedor {cfg.llm_provider}. Configura LLM_API_KEY o la variable del proveedor en .env o llm.json."
        )
        raise SystemExit(1)

    if args.ask:
        herdr = Herdr(cfg)
        last = {"text": ""}
        messenger = Messenger(
            cfg,
            herdr,
            last_summary=lambda: last["text"],
            watch=lambda address: None,
            mute=lambda minutes: None,
            request_close=lambda: None,
            mic_mute=lambda muted: None,
        )
        answer = messenger.ask(args.ask)
        print(answer)
        return

    log(f"Configuración: {args.config or config_path()}")
    tasks_path = args.config.with_name("tasks.json") if args.config else None
    app = VoiceApp(
        cfg,
        tasks_path=tasks_path,
        config_file=args.config or config_path(),
    )
    app.run()


if __name__ == "__main__":
    main()
