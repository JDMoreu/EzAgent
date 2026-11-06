"""`ezagent-voice`: mensajero de voz para Herdr en modo llamada.

- Ctrl+Alt+Space: iniciar / colgar la llamada.
- La isla queda visible durante toda la llamada.
- El micrófono sigue escuchando entre turnos; el VAD usa pre-roll y un
  silencio de cierre largo para no cortar al usuario.
- Si hablas mientras el asistente responde, se corta el TTS y se te escucha.
- whisper.cpp -> OpenCode DeepSeek 4.1 Flash + tools de Herdr -> Kokoro.
"""

from __future__ import annotations

import argparse
import math
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
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit

from dotenv import load_dotenv

from ezagent.llm_providers import (
    load_provider_catalog,
    normalize_provider,
    provider_models,
    save_provider_profile,
)

from .audio import MicCapture, list_input_devices, pcm_to_wav
from .config import VoiceConfig, config_path
from .herdr import Herdr
from .hotkeys import GlobalHotkey
from .llm import Messenger
from .notch.island import Notch
from .stt import (
    STTError,
    available_whisper_models,
    transcribe,
    whisper_model_path,
)
from .metrics import LatencyStats, TurnTimings
from .tts import Speaker
from .watcher import Watcher

IDLE = "idle"
LISTENING = "listening"
THINKING = "thinking"
SPEAKING = "speaking"

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


def log(message: str) -> None:
    print(f"[ezagent-voice] {message}", flush=True)


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


def ensure_whisper(
    cfg: VoiceConfig, wait_secs: float = 25.0
) -> subprocess.Popen | None:
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
    port = _free_local_port()
    cfg.whisper_url = urlunsplit(
        (parts.scheme or "http", f"127.0.0.1:{port}", parts.path, parts.query, "")
    )
    log(f"whisper-server: puerto local libre {port}.")
    log("whisper-server no responde; arrancándolo…")
    try:
        command = [
            "powershell", "-NoProfile", "-ExecutionPolicy", "Bypass",
            "-File", str(script),
            "-Port", str(port),
            "-Engine", str(cfg.stt_engine or "auto"),
        ]
        try:
            from .stt import dicta_home

            home = dicta_home()
        except Exception:
            home = None
        if home is not None:
            command.extend(["-DictaHome", str(home)])
        model_path = whisper_model_path(cfg.stt_model)
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
    def __init__(self, cfg: VoiceConfig):
        self.cfg = cfg
        self.state = IDLE
        self.in_call = False
        self.muted = False
        self._user_speaking = False
        self._ignore_utterance = False
        self.last_summary = ""
        self.mute_until = 0.0
        self.watched: set[str] = set()
        self._pending_prompts: list[tuple[str, str, str, str]] = []
        self._close_after_answer = False
        self._reset_capture = threading.Event()
        self._post_tts_until = 0.0
        self._post_tts_ignore = False
        self._call_stop: threading.Event | None = None
        self._call_thread: threading.Thread | None = None
        self._tts_stop: threading.Event | None = None
        self._speaking_kind: str | None = None
        self._turn_gen = 0
        self._turn_started = 0.0
        self._turn_timing: TurnTimings | None = None
        self._latency = LatencyStats()
        self._notch_hidden = bool(cfg.notch_hidden)
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
        )
        self._saved_stt_model = cfg.stt_model
        self._saved_stt_engine = cfg.stt_engine

        self.root = tk.Tk()
        self.root.withdraw()
        self.notch = Notch(self.root)
        for sig in (signal.SIGINT, signal.SIGTERM):
            try:
                signal.signal(sig, lambda *_: self.root.after(0, self.root.quit))
            except Exception:
                pass

        self._whisper_proc = ensure_whisper(cfg)
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
        )
        log(
            f"Micrófono: {self.mic.device_name} (índice {self.mic.device_index}, "
            f"{self.mic.rate} Hz)"
        )
        log(
            f"VAD: silencio={cfg.silence_secs}s, min_rms={cfg.min_rms}, "
            f"pre_roll={cfg.pre_roll_secs}s, barge_in={cfg.barge_in}"
        )
        self.herdr = Herdr(cfg)
        self.messenger = Messenger(
            cfg,
            self.herdr,
            last_summary=lambda: self.last_summary,
            watch=self._watch,
            mute=self._mute,
            request_close=self._request_close,
            mic_mute=self._set_mic_muted,
        )
        self.watcher = Watcher(
            cfg,
            on_event=self._on_watcher_event,
            should_announce=self._should_announce,
        )

        cancel_specs: list[str] = []
        for spec in (cfg.cancel_hotkey, "ctrl+alt+e", "ctrl+alt+c"):
            if spec and spec not in cancel_specs:
                cancel_specs.append(spec)
        self.hotkeys = [
            GlobalHotkey(cfg.hotkey, "toggle", self.hotkey_events, 47201)
        ]
        for index, spec in enumerate(cancel_specs):
            self.hotkeys.append(
                GlobalHotkey(spec, "cancel", self.hotkey_events, 47202 + index)
            )
        repeat_specs: list[str] = []
        for spec in (cfg.repeat_hotkey, "ctrl+alt+y"):
            if spec and spec not in repeat_specs:
                repeat_specs.append(spec)
        for index, spec in enumerate(repeat_specs):
            self.hotkeys.append(
                GlobalHotkey(spec, "repeat", self.hotkey_events, 47210 + index)
            )
        self.hotkeys.append(
            GlobalHotkey(cfg.mute_hotkey, "mute", self.hotkey_events, 47211)
        )
        self.hotkeys.append(
            GlobalHotkey(cfg.menu_hotkey, "menu", self.hotkey_events, 47212)
        )
        for hotkey in self.hotkeys:
            hotkey.start()
        self.notch.on_action = self._on_notch_action
        self.notch.menu_settings_provider = self._notch_menu_settings
        self.notch.on_live_setting = self._notch_live_setting
        self.notch.on_save_settings = self._notch_save_settings
        self.notch.on_test_voice = self._notch_test_voice
        self.notch.on_hide = self._notch_hide
        try:
            self.notch.set_theme(self.cfg.theme)
        except Exception:
            pass
        threading.Thread(target=self._refresh_devices, daemon=True).start()
        self.notch.set_in_call(False)
        if not self._notch_hidden:
            self.notch.show(IDLE)
        self.root.after(120, self._report_hotkeys)
        self.root.after(30, self._tick)

    def _warm_tts(self) -> None:
        try:
            self.speaker.synthesize("Hola.")
            log("Kokoro precalentado.")
        except Exception as exc:  # noqa: BLE001
            log(f"No pude precalentar Kokoro: {exc}")

    def _warm_stt(self) -> None:
        """Carga el modelo de whisper antes del primer turno real."""
        try:
            transcribe(self.cfg, pcm_to_wav(b"\x00\x00" * 3200, 16000))
            log("whisper precalentado.")
        except Exception as exc:  # noqa: BLE001
            log(f"No pude precalentar whisper: {exc}")

    # --------------------------------------------------------------- arranque
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
            # Si la isla estaba oculta de la sesión anterior, este arranque la
            # devuelve: el usuario pidió abrir la app escuchando.
            self._notch_hidden = False
            self.cfg.notch_hidden = False
            self.cfg.save_key("notch_hidden", False)
            self.notch.show(IDLE)
        self._start_call()
        log("Escuchando al iniciar.")

    def shutdown(self) -> None:
        self.in_call = False
        self._user_speaking = False
        self._turn_gen += 1
        if self._call_stop is not None:
            self._call_stop.set()
        if self._tts_stop is not None:
            self._tts_stop.set()
        self.watcher.stop()
        for hotkey in self.hotkeys:
            hotkey.stop()
        if self._whisper_proc is not None and self._whisper_proc.poll() is None:
            try:
                subprocess.run(
                    [
                        "taskkill", "/PID", str(self._whisper_proc.pid),
                        "/T", "/F",
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
        try:
            self.speaker.close()
        except Exception:
            pass
        try:
            self.mic.close()
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
            self._set_mic_muted(not self.muted)
        elif event == "menu":
            if self._notch_hidden:
                self._notch_hidden = False
                self.cfg.notch_hidden = False
                self.cfg.save_key("notch_hidden", False)
                self.notch.show(IDLE)
            self._toggle_menu()

    def _handle_event(self, event: tuple) -> None:
        kind = event[0]
        if kind == "notch_action":
            self._notch_action_ui(str(event[1]))
        elif kind == "transcribed":
            _, text, gen = event
            if not self.in_call or gen != self._turn_gen:
                return
            text = (text or "").strip()
            if not text:
                self._set_state(LISTENING)
                return
            if self._turn_started:
                log(f"STT: {(time.monotonic() - self._turn_started) * 1000:.0f} ms")
            log(f"Tú: {text}")
            if self._fast_command(text):
                return
            self._set_state(THINKING)
            threading.Thread(
                target=self._answer_worker, args=(text, gen), daemon=True
            ).start()
        elif kind == "speaking":
            gen = event[1]
            if gen == self._turn_gen:
                if self._turn_started:
                    log(
                        "Primer audio: "
                        f"{(time.monotonic() - self._turn_started) * 1000:.0f} ms"
                    )
                self._set_state(SPEAKING)
        elif kind == "answer_done":
            gen = event[1]
            if gen != self._turn_gen:
                return
            if self._turn_started:
                log(
                    "Turno completo: "
                    f"{(time.monotonic() - self._turn_started) * 1000:.0f} ms"
                )
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
                self._hang_up()
                return
            if self.in_call:
                self._set_state(LISTENING)
            else:
                self._set_idle()
            self._drain_pending()
        elif kind == "done_prompt":
            _, address, agent, reason, output = event
            if self._user_speaking or self.state in (THINKING, SPEAKING):
                self._pending_prompts.append((address, agent, reason, output))
                return
            self._handle_done_prompt(address, agent, reason, output)
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
                    self._hang_up()
                self._drain_pending()
                return
            if self.in_call:
                self._set_state(LISTENING)
            else:
                self._set_idle()
            self._drain_pending()
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
        elif kind == "notch_error":
            log(f"Notch: {event[1]}")

    # ---------------------------------------------------------------- llamada
    def _start_call(self) -> None:
        if self.in_call:
            return
        if self._tts_stop is not None:
            self._tts_stop.set()
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

        def muted_prompt() -> None:
            if self.in_call and self.muted:
                self._speak(
                    "El micrófono está silenciado. Actívalo para hablar.",
                    "answer",
                )

        if self.muted:
            self.root.after(150, muted_prompt)

    def _cut_off(self) -> None:
        """Botón stop del notch: corta también un aviso en curso."""
        if self._tts_stop is not None:
            self._tts_stop.set()
        self._speaking_kind = None
        self._hang_up()

    def _hang_up(self) -> None:
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
        if self._tts_stop is not None:
            self._tts_stop.set()
        self._speaking_kind = None
        self.notch.set_in_call(False)
        self._set_idle()
        log("Llamada terminada.")
        self.root.after(200, self._drain_pending)

    def _call_worker(self, stop_event: threading.Event) -> None:
        try:
            self.mic.stream(
                stop_event,
                on_level=self._on_mic_level,
                on_speech_start=self._on_speech_start,
                on_utterance=self._on_utterance,
                reset_event=self._reset_capture,
                is_speaking=lambda: self.state == SPEAKING,
                barge_in_rms=self.cfg.barge_in_rms,
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
        log("Barge-in: te escucho.")
        self._turn_gen += 1
        if self._tts_stop is not None:
            self._tts_stop.set()
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
        if time.monotonic() < self._post_tts_until:
            # Cola del TTS: se descarta el eco, salvo que la frase siga viva
            # cuando termine la ventana (se decide en _on_utterance).
            self._ignore_utterance = True
            self._post_tts_ignore = True
            return
        self._user_speaking = True
        if self.state == THINKING:
            # No hay eco: el usuario puede corregir o repetir mientras el
            # modelo piensa. La respuesta pendiente se descarta.
            self._turn_gen += 1
            if self._tts_stop is not None:
                self._tts_stop.set()
            self._set_state(LISTENING)
            return
        if self.state == SPEAKING:
            if not self.cfg.barge_in or level < self.cfg.barge_in_rms:
                # Probable eco del TTS: no lo tratamos como turno.
                self._ignore_utterance = True
                self._post_tts_ignore = False
                return
            if self._close_after_answer:
                # El usuario interrumpe el cierre: no cerramos.
                self._close_after_answer = False
            log("Barge-in: te escucho.")
            self._turn_gen += 1
            if self._tts_stop is not None:
                self._tts_stop.set()
            self._set_state(LISTENING)

    def _on_utterance(self, pcm: bytes, rate: int) -> None:
        self._user_speaking = False
        if self.muted:
            self._ignore_utterance = False
            self._post_tts_ignore = False
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
        now = time.monotonic()
        self._turn_started = now
        speech_end = getattr(self.mic, "last_speech_end", None) or now
        timing = TurnTimings(speech_end=speech_end, vad_end=now)
        self._turn_timing = timing
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
            # La isla queda como control compacto: play, mute y ajustes.
            self.notch.set_in_call(False)
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

    def _set_idle(self) -> None:
        self._set_state(IDLE)

    def _speak(self, text: str, kind: str = "announce") -> None:
        if not text:
            return
        self._turn_gen += 1
        generation = self._turn_gen
        if self._tts_stop is not None:
            self._tts_stop.set()
        self._tts_stop = threading.Event()
        self._speaking_kind = kind
        self._set_state(SPEAKING)
        threading.Thread(
            target=self._tts_worker,
            args=(text, self._tts_stop, generation),
            daemon=True,
        ).start()

    def _drain_pending(self) -> None:
        if self._user_speaking or self.state not in (IDLE, LISTENING):
            return
        if self._pending_prompts:
            address, agent, reason, output = self._pending_prompts.pop(0)
            self._handle_done_prompt(address, agent, reason, output)

    # --------------------------------------------------------------- workers
    def _stt_worker(
        self, pcm: bytes, rate: int, gen: int, timing: TurnTimings
    ) -> None:
        try:
            text = transcribe(self.cfg, pcm_to_wav(pcm, rate))
            timing.mark("stt_ready")
            self.events.put(("transcribed", text, gen))
        except STTError as exc:
            self.events.put(("error", str(exc)))
        except Exception as exc:  # noqa: BLE001
            self.events.put(("error", f"STT: {exc}"))

    def _answer_worker(self, text: str, gen: int) -> None:
        """Consume el LLM en streaming y habla cada frase en cuanto llega."""
        stop_event = threading.Event()
        self._tts_stop = stop_event
        self._speaking_kind = "answer"
        sentences = _Sentences()
        full: list[str] = []
        started = False
        timing = self._turn_timing if gen == self._turn_gen else None
        first_token = False
        first_audio = False

        def speak_sentence(sentence: str) -> None:
            nonlocal first_audio
            if timing is not None and not first_audio:
                timing.mark("first_sentence")
                first_audio = True
            self.speaker.speak(
                sentence,
                stop_event,
                on_level=lambda level: self.notch.draw(level, SPEAKING),
                on_start=(
                    (lambda: timing.mark("first_sound"))
                    if timing is not None else None
                ),
            )

        try:
            for delta in self.messenger.ask_stream(text):
                if gen != self._turn_gen or stop_event.is_set():
                    return
                if timing is not None and not first_token and delta.strip():
                    timing.mark("first_token")
                    first_token = True
                full.append(delta)
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
            if answer:
                self.last_summary = answer
                log(f"Mensajero: {answer}")
        except Exception as exc:  # noqa: BLE001
            self.events.put(("error", f"OpenCode: {exc}"))
        finally:
            self.events.put(("answer_done", gen))

    def _tts_worker(
        self, text: str, stop_event: threading.Event, generation: int
    ) -> None:
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

    def _on_watcher_event(
        self, address: str, agent: str, state: str, reason: str
    ) -> None:
        if address not in self.watched:
            return
        self.watched.discard(address)
        try:
            output = self.herdr.read_agent(address, 60)
        except Exception:
            output = ""
        self.events.put(("done_prompt", address, agent, reason, output))

    def _handle_done_prompt(
        self, address: str, agent: str, reason: str, output: str
    ) -> None:
        if not self.in_call:
            # Reabre la llamada sola: notch de escucha + STT.
            self._start_call()
        log(f"Aviso de {agent} ({reason}); reabro la escucha para preguntar.")
        self.messenger.note_done(address, agent, reason, output)
        if reason == "done":
            question = (
                "He terminado la tarea. ¿Quieres que te diga la salida, "
                "te resuma lo que hizo, o quieres darme otra instrucción?"
            )
        else:
            question = (
                f"El agente {agent} necesita tu atención. ¿Quieres que te diga "
                "qué pasa o le das una instrucción?"
            )
        # AVISO dura 1 s (spec 9.3). Si el usuario habla antes, se cancela la
        # pregunta hablada y su turno manda.
        try:
            self.notch.notice()
        except Exception:
            pass
        gen = self._turn_gen

        def speak_after_notice() -> None:
            if gen != self._turn_gen:
                return
            self._speak(question, "announce")

        self.root.after(1000, speak_after_notice)

    # ------------------------------------------------------- atajos y ajustes
    @staticmethod
    def _normalize(text: str) -> str:
        raw = unicodedata.normalize("NFKD", text.lower())
        raw = "".join(c for c in raw if not unicodedata.combining(c))
        return re.sub(r"[^a-z0-9 ]+", " ", raw).strip()

    def _fast_command(self, text: str) -> bool:
        """Órdenes de control que no necesitan pasar por el LLM."""
        key = re.sub(r"\s+", " ", self._normalize(text))
        mute_words = {
            "mute", "mutea", "muteate", "silencia", "silencio",
            "silencia el micro", "silencio el micro", "silencio el microfono",
        }
        unmute_words = {
            "desmute", "desmutea", "unmute", "activa el micro",
            "reactiva el micro", "activa el microfono",
        }
        close_words = {
            "cierra", "cierrate", "cierra la escucha", "cierra el micro",
            "deja de escuchar", "adios", "callate",
        }
        repeat_words = {"repite", "repitelo", "otra vez"}
        greet_words = {"hola", "buenas", "hola ezagent", "buenas ezagent"}
        agent_words = {
            "que agentes hay", "que agentes tengo", "lista los agentes",
            "agentes vivos", "cuantos agentes hay",
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
                "working": "trabajando", "done": "terminado",
                "blocked": "bloqueado", "idle": "inactivo",
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
        if key in mute_words:
            log("Atajo local: mute.")
            self._set_mic_muted(True)
            self._speak("Micrófono silenciado.", "answer")
            return True
        if key in unmute_words:
            log("Atajo local: unmute.")
            self._set_mic_muted(False)
            self._speak("Micrófono activo.", "answer")
            return True
        if key in close_words:
            log("Atajo local: cerrar escucha.")
            self._close_after_answer = True
            self._speak("Vale, cierro.", "answer")
            return True
        if key in repeat_words:
            log("Atajo local: repetir.")
            self._speak(
                self.last_summary or "No hay nada que repetir todavía.", "answer"
            )
            return True
        return False

    def _on_notch_action(self, action: str) -> None:
        # El WndProc del notch entrega el clic en un hilo nativo. No se toca
        # Tk desde ahí: la intención se publica y _tick la aplica.
        log(f"Notch: {action}")
        self.events.put(("notch_action", action))

    def _notch_action_ui(self, action: str) -> None:
        if action == "mute":
            self._set_mic_muted(not self.muted)
        elif action == "panel":
            self._toggle_menu()
        elif action == "hide":
            self.hide_notch()

    def _toggle_menu(self) -> None:
        """El menú vive dentro de la isla; no hay panel Tk."""
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

        values = {
            key: getattr(self.cfg, key, None) for key in EDITABLE_KEYS
        }
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

        try:
            stt_models = available_whisper_models(self.cfg.stt_model)
        except Exception:
            stt_models = ()
        llm_models = {
            name: provider_models(name) for name in providers
        }
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
                    self.messenger.reload_llm()
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
            elif key == "llm_model":
                self.cfg.llm_model = str(value)
            elif key == "llm_base_url":
                self.cfg.llm_base_url = str(value)
            elif key == "api_key":
                self.cfg.api_key = str(value)
            elif key == "llm_temperature":
                self.cfg.llm_temperature = float(value)
            elif key == "llm_max_tokens":
                self.cfg.llm_max_tokens = int(round(float(value)))
            elif key == "llm_reasoning_effort":
                self.cfg.llm_reasoning_effort = str(value)
            elif key == "barge_in":
                self.cfg.barge_in = bool(value)
            elif key == "confirm_actions":
                self.cfg.confirm_actions = bool(value)
                self.messenger.set_confirm_actions(self.cfg.confirm_actions)
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

    def _apply_settings(self, values: dict) -> bool:
        """Aplica y guarda los valores del panel; True si reinició la llamada."""
        old_device = self.mic.device_index
        was_in_call = self.in_call
        device_changed = False
        restarted = False
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
        self.cfg.noise_multiplier = number(
            "noise_multiplier", self.cfg.noise_multiplier
        )
        self.cfg.pre_roll_secs = number(
            "pre_roll_secs", self.cfg.pre_roll_secs
        )
        self.cfg.min_speech_secs = number(
            "min_speech_secs", self.cfg.min_speech_secs
        )
        self.cfg.max_utterance_secs = number(
            "max_utterance_secs", self.cfg.max_utterance_secs
        )
        self.cfg.barge_in_rms = number(
            "barge_in_rms", self.cfg.barge_in_rms
        )
        self.cfg.voice = str(values.get("voice", self.cfg.voice))
        self.cfg.speed = number("speed", self.cfg.speed)
        self.cfg.tts_sentence_pause = number(
            "tts_sentence_pause", self.cfg.tts_sentence_pause
        )
        self.cfg.tts_clause_pause = number(
            "tts_clause_pause", self.cfg.tts_clause_pause
        )
        self.cfg.tts_model_variant = str(
            values.get("tts_model_variant", self.cfg.tts_model_variant)
        )
        self.cfg.kokoro_model_path = str(
            values.get("kokoro_model_path", self.cfg.kokoro_model_path)
        )
        self.cfg.kokoro_voices_path = str(
            values.get("kokoro_voices_path", self.cfg.kokoro_voices_path)
        )
        self.cfg.whisper_url = str(
            values.get("whisper_url", self.cfg.whisper_url)
        )
        self.cfg.stt_model = str(values.get("stt_model", self.cfg.stt_model))
        self.cfg.stt_engine = str(
            values.get("stt_engine", self.cfg.stt_engine)
        )
        self.cfg.stt_prompt = str(
            values.get("stt_prompt", self.cfg.stt_prompt)
        )
        self.cfg.stt_timeout_secs = number(
            "stt_timeout_secs", self.cfg.stt_timeout_secs
        )
        self.cfg.auto_start_whisper = bool(
            values.get("auto_start_whisper", self.cfg.auto_start_whisper)
        )
        self.cfg.llm_provider = normalize_provider(
            str(values.get("llm_provider", self.cfg.llm_provider))
        )
        self.cfg.llm_model = str(
            values.get("llm_model", self.cfg.llm_model)
        ).strip()
        self.cfg.llm_base_url = str(
            values.get("llm_base_url", self.cfg.llm_base_url)
        ).strip()
        self.cfg.api_key = str(values.get("api_key", self.cfg.api_key)).strip()
        self.cfg.llm_temperature = number(
            "llm_temperature", self.cfg.llm_temperature
        )
        self.cfg.llm_max_tokens = int(
            round(number("llm_max_tokens", self.cfg.llm_max_tokens))
        )
        self.cfg.llm_reasoning_effort = str(
            values.get(
                "llm_reasoning_effort", self.cfg.llm_reasoning_effort
            )
        )
        self.cfg.barge_in = bool(values.get("barge_in", self.cfg.barge_in))
        self.cfg.confirm_actions = bool(
            values.get("confirm_actions", self.cfg.confirm_actions)
        )

        new_llm = (
            self.cfg.llm_provider,
            self.cfg.llm_model,
            self.cfg.llm_base_url,
            self.cfg.api_key,
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
            )
            self.cfg.llm_pinned = True

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
        new_tts = (
            self.cfg.tts_model_variant,
            self.cfg.kokoro_model_path,
            self.cfg.kokoro_voices_path,
        )
        if new_tts != old_tts:
            self.speaker.reload_model()
        self.speaker.set_voice(self.cfg.voice)
        self.messenger.set_confirm_actions(self.cfg.confirm_actions)
        if new_llm != old_llm:
            self.messenger.reload_llm()
        self.cfg.save()
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
            log(
                "Modelo de Whisper guardado; se aplicará al reiniciar "
                "whisper-server."
            )
            return

        def run() -> None:
            try:
                if proc.poll() is None:
                    subprocess.run(
                        ["taskkill", "/PID", str(proc.pid), "/T", "/F"],
                        stdout=subprocess.DEVNULL,
                        stderr=subprocess.DEVNULL,
                        creationflags=getattr(
                            subprocess, "CREATE_NO_WINDOW", 0
                        ),
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

    def _set_mic_muted(self, muted: bool) -> None:
        self.muted = bool(muted)
        self.notch.set_muted(self.muted)
        self._reset_capture.set()
        if self.muted:
            self._ignore_utterance = True
            log("Micrófono silenciado.")
        else:
            self._ignore_utterance = False
            log("Micrófono activo.")

    def _watch(self, address: str) -> None:
        self.watched.add(address)
        try:
            agent = self.herdr.get_agent(address)
        except Exception:
            return
        state = agent.get("state")
        if state not in ("done", "blocked", "error"):
            return
        self.watched.discard(address)
        try:
            output = self.herdr.read_agent(address, 60)
        except Exception:
            output = ""
        self.events.put(
            (
                "done_prompt",
                address,
                agent.get("agent") or "agente",
                state,
                output,
            )
        )

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
    notch = Notch(root)
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

    if not cfg.api_key:
        log(f"Falta la API key del proveedor {cfg.llm_provider}. Configura LLM_API_KEY o la variable del proveedor en .env o llm.json.")
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
    app = VoiceApp(cfg)
    app.run()


if __name__ == "__main__":
    main()
