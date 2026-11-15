"""Arnés de reproducción de `ezagent-voice` (F1-12a).

Arranca el `VoiceApp` real de Python con las costuras de servicio cambiadas
desde fuera (sin tocar `src/`):

- micrófono → WAV de `parity/replay/audio/` inyectado a ritmo de 30 ms por
  bloque, con el `Segmenter` y el `BargeInDetector` reales;
- altavoz → registrador de frases (duración simulada y determinista);
- LLM, whisper y `hc` → falsos de `rust/crates/ez-fakes`;
- HUD y Tk → registradores sin ventana.

Produce `trace.jsonl` normalizado: eventos por hilo, ordenados dentro de cada
hilo (el orden causal de un hilo sí es determinista; el entrehilo no).

Uso:
    python parity/replay/run_py.py --all
    python parity/replay/run_py.py --scenario 02 --trace-out C:/tmp/02.jsonl
    python parity/replay/run_py.py --verify
"""

from __future__ import annotations

import argparse
import json
import os
import queue
import re
import socket
import subprocess
import sys
import tempfile
import threading
import time
import wave
from pathlib import Path
from typing import Any, Callable

import numpy as np
import yaml

ROOT = Path(__file__).resolve().parents[2]
REPLAY = ROOT / "parity" / "replay"
AUDIO = REPLAY / "audio"
SCENARIOS = REPLAY / "scenarios"
GOLDEN = REPLAY / "golden"
FAKES = ROOT / "rust" / "target" / "debug"
GEN = ROOT / "parity" / "gen"

sys.path.insert(0, str(GEN))
sys.path.insert(0, str(ROOT / "src"))

from _common import frozen_environment  # noqa: E402

CHUNK_BYTES = 480 * 2
CHUNK_SECS = 0.03
SAMPLE_RATE = 16000
THREAD_ORDER = (
    "main",
    "ezagent-call",
    "ezagent-stt",
    "ezagent-answer",
    "ezagent-speech-synth",
    "ezagent-speech",
    "ezagent-tts",
    "ezagent-busy",
    "ezagent-wake",
    "services",
    "audit",
)


# --------------------------------------------------------------------- traza
class Trace:
    """Eventos por hilo con secuencia propia: el archivo queda determinista."""

    def __init__(self, home: Path) -> None:
        self.home = str(home)
        self.lock = threading.Lock()
        self.events: list[dict[str, Any]] = []
        self.chrono: list[dict[str, Any]] = []
        self._keys: dict[int, str] = {}
        self._seq: dict[str, int] = {}
        self._ordinals: dict[str, int] = {}
        self._local = threading.local()

    def label(self, name: str) -> None:
        self._local.label = name

    def _key(self) -> str:
        ident = threading.get_ident()
        if ident in self._keys:
            return self._keys[ident]
        name = getattr(self._local, "label", None) or threading.current_thread().name
        if name == "MainThread" or re.fullmatch(r"(Thread-\d+|Dummy-\d+)", name):
            name = "main" if name == "MainThread" else "ezagent-call"
        ordinal = self._ordinals.get(name, 0)
        self._ordinals[name] = ordinal + 1
        key = f"{name}#{ordinal}"
        self._keys[ident] = key
        return key

    def _stable(self, value: Any) -> Any:
        if isinstance(value, str):
            return value.replace(self.home, "<tmp>").replace(self.home.replace("\\", "/"), "<tmp>")
        if isinstance(value, dict):
            return {str(k): self._stable(v) for k, v in value.items()}
        if isinstance(value, (list, tuple)):
            return [self._stable(item) for item in value]
        if isinstance(value, Path):
            return self._stable(str(value))
        return value

    def record(self, event: str, **data: Any) -> None:
        payload = self._stable(data)
        with self.lock:
            key = self._key()
            seq = self._seq.get(key, 0) + 1
            self._seq[key] = seq
            item = {"thread": key, "seq": seq, "event": event, **payload}
            self.events.append(item)
            if os.getenv("EZ_REPLAY_DUMP"):
                item = {**item, "at": round(time.monotonic(), 3)}
            self.chrono.append(item)

    def with_event(self, event: str) -> list[dict[str, Any]]:
        return [item for item in self.chrono if item["event"] == event]

    @staticmethod
    def _rank(item: dict[str, Any]) -> tuple[int, int, int]:
        name, ordinal = item["thread"].split("#")
        rank = THREAD_ORDER.index(name) if name in THREAD_ORDER else len(THREAD_ORDER)
        return (rank, int(ordinal), int(item["seq"]))

    def lines(self) -> list[str]:
        with self.lock:
            ordered = sorted(self.events, key=self._rank)
        return [json.dumps(item, ensure_ascii=False) for item in ordered]


# ------------------------------------------------------------------ dobles UI
class RecordingHud:
    """HUD sin ventana: anota lo que la app le pide y acepta cualquier método.

    Solo se anotan los cambios: `_set_state` repite el mismo estado a propósito
    (el micrófono lo adelanta y la transcripción lo confirma) y el hilo de UI
    vuelve a fijar `in_call` en cada transición.
    """

    def __init__(self, trace: Trace) -> None:
        self.trace = trace
        self._state: str | None = None
        self._in_call: bool | None = None
        self._muted: bool | None = None
        self._confirm = False

    def show(self, state: str) -> None:
        if state != self._state:
            self._state = str(state)
            self.trace.record("hud.show", state=self._state)

    def hide(self, **_: Any) -> None:
        self.trace.record("hud.hide")

    def set_in_call(self, value: bool) -> None:
        if bool(value) != self._in_call:
            self._in_call = bool(value)
            self.trace.record("hud.in_call", value=self._in_call)

    def set_muted(self, value: bool) -> None:
        if bool(value) != self._muted:
            self._muted = bool(value)
            self.trace.record("hud.muted", value=self._muted)

    def notice(self) -> None:
        self.trace.record("hud.notice")

    def show_confirmation(self, summary: str, strong: bool, *_: Any) -> None:
        self._confirm = True
        self.trace.record("hud.confirm", summary=str(summary), strong=bool(strong))

    def hide_confirmation(self) -> None:
        if self._confirm:
            self._confirm = False
            self.trace.record("hud.confirm_done")

    def set_testing(self, value: bool) -> None:
        self.trace.record("hud.testing", value=bool(value))

    def close(self) -> None:
        self.trace.record("hud.close")

    def draw(self, *_: Any) -> None:
        return None

    def set_mic_level(self, *_: Any) -> None:
        return None

    def __getattr__(self, name: str) -> Any:
        if name.startswith("_"):
            raise AttributeError(name)
        return lambda *args, **kwargs: None


class FakeRoot:
    """`tk.Tk` de mentira: bombea `after` y ejecuta el guion del escenario."""

    def __init__(self, timeout: float) -> None:
        self.driver: Driver | None = None
        self.timeout = timeout
        self.final: dict[str, Any] = {}
        self._after: list[tuple[float, Callable[[], None]]] = []
        self._quit = False

    def withdraw(self) -> None:
        return None

    def after(self, ms: int, callback: Callable[[], None] | None = None) -> str:
        if callback is not None:
            self._after.append((time.monotonic() + max(0, ms) / 1000.0, callback))
        return str(len(self._after))

    def quit(self) -> None:
        self._quit = True

    def destroy(self) -> None:
        self._quit = True

    def _pump(self) -> None:
        now = time.monotonic()
        due = [item for item in self._after if item[0] <= now]
        if not due:
            return
        self._after = [item for item in self._after if item[0] > now]
        for _, callback in due:
            callback()

    def mainloop(self) -> None:
        if self.driver is None:
            raise RuntimeError("el guion no está conectado al bucle")
        deadline = time.monotonic() + self.timeout
        while not self.driver.done and not self._quit:
            self._pump()
            self.driver.tick()
            if time.monotonic() > deadline:
                raise TimeoutError(f"escenario sin terminar en {self.timeout:.0f} s")
            time.sleep(0.005)
        self.final = self.driver.snapshot()


class ReplaySpeaker:
    """Altavoz de mentira: anota la frase y simula su duración en tiempo real."""

    def __init__(self, cfg: Any, trace: Trace, min_secs: float) -> None:
        self.cfg = cfg
        self.trace = trace
        self.min_secs = min_secs
        self.kokoro = None
        self._lock = threading.Lock()

    def synthesize(self, text: str) -> tuple[str, int]:
        return (str(text), SAMPLE_RATE)

    def set_voice(self, _voice: str) -> None:
        return None

    def reload_model(self) -> None:
        return None

    def close(self) -> None:
        return None

    def play(
        self,
        samples: Any,
        _rate: int,
        stop_event: threading.Event,
        on_level: Callable[[float], None] | None = None,
        on_start: Callable[[], None] | None = None,
    ) -> bool:
        text = str(samples)
        seconds = max(self.min_secs, 0.045 * len(text))
        with self._lock:
            if stop_event.is_set():
                return False
            self.trace.record("speech.say", text=text)
            if on_start is not None:
                on_start()
            end = time.monotonic() + seconds
            while time.monotonic() < end:
                if stop_event.is_set():
                    self.trace.record("speech.cut", text=text)
                    return False
                if on_level is not None:
                    on_level(0.01)
                time.sleep(0.02)
            return True

    def speak(
        self,
        text: str,
        stop_event: threading.Event,
        on_level: Callable[[float], None] | None = None,
        on_start: Callable[[], None] | None = None,
    ) -> bool:
        samples, rate = self.synthesize(text)
        return self.play(samples, rate, stop_event, on_level, on_start)


# ------------------------------------------------------------------ micrófono
def read_wav(path: Path) -> np.ndarray:
    with wave.open(str(path), "rb") as handle:
        if (handle.getnchannels(), handle.getsampwidth(), handle.getframerate()) != (
            1,
            2,
            SAMPLE_RATE,
        ):
            raise ValueError(f"{path.name}: se esperaba PCM16 mono {SAMPLE_RATE} Hz")
        return np.frombuffer(handle.readframes(handle.getnframes()), dtype=np.int16).copy()


class ReplayMic:
    """Micrófono real sustituido por una cola de segmentos WAV o silencio."""

    def __init__(
        self,
        *,
        trace: Trace,
        device_index: int | None = None,
        min_rms: float = 0.0006,
        silence_secs: float = 1.3,
        pre_roll_secs: float = 0.6,
        min_speech_secs: float = 0.2,
        max_utterance_secs: float = 30.0,
        noise_multiplier: float = 1.3,
        barge_in_rms: float = 0.008,
        barge_in_secs: float = 0.25,
    ) -> None:
        self.trace = trace
        self.device_index = device_index if device_index is not None else 0
        self.device_name = "replay"
        self.rate = SAMPLE_RATE
        self.turn_detector = None
        self.turn_quick_secs = 0.35
        self.last_speech_end: float | None = None
        self.min_rms = min_rms
        self.silence_secs = silence_secs
        self.pre_roll_secs = pre_roll_secs
        self.min_speech_secs = min_speech_secs
        self.max_utterance_secs = max_utterance_secs
        self.noise_multiplier = noise_multiplier
        self.barge_in_rms = barge_in_rms
        self.barge_in_secs = barge_in_secs
        self._segments: queue.Queue[np.ndarray] = queue.Queue()
        self._buffer = np.zeros(0, dtype=np.int16)
        self._speech_active = False
        self._barge_latched = False

    def close(self) -> None:
        return None

    def push_audio(self, name: str, gain: float = 1.0) -> None:
        samples = read_wav(AUDIO / name)
        if gain != 1.0:
            samples = np.clip(samples.astype(np.float64) * gain, -32768, 32767).astype(np.int16)
        self._segments.put(samples)

    def push_silence(self, seconds: float) -> None:
        self._segments.put(np.zeros(int(seconds * SAMPLE_RATE), dtype=np.int16))

    def push_noise(self, seconds: float) -> None:
        noise = read_wav(AUDIO / "ruido.wav")
        count = int(seconds * SAMPLE_RATE)
        if len(noise) == 0:
            self.push_silence(seconds)
            return
        reps = int(np.ceil(count / len(noise)))
        self._segments.put(np.tile(noise, reps)[:count])

    def idle(self) -> bool:
        return len(self._buffer) == 0 and self._segments.empty() and not self._speech_active

    def diagnostic(self) -> str:
        return (
            f"buffer={len(self._buffer)} queued={self._segments.qsize()} "
            f"speech={self._speech_active}"
        )

    def _next_chunk(self) -> bytes:
        time.sleep(CHUNK_SECS)
        if len(self._buffer) == 0:
            try:
                self._buffer = self._segments.get_nowait()
            except queue.Empty:
                return b"\x00" * CHUNK_BYTES
        chunk = self._buffer[:480]
        self._buffer = self._buffer[480:]
        if len(chunk) < 480:
            chunk = np.concatenate([chunk, np.zeros(480 - len(chunk), dtype=np.int16)])
        return chunk.astype(np.int16).tobytes()

    def stream(
        self,
        stop_event: threading.Event,
        on_level: Callable[[float], None] | None = None,
        on_speech_start: Callable[[float], None] | None = None,
        on_utterance: Callable[[bytes, int], None] | None = None,
        reset_event: threading.Event | None = None,
        is_speaking: Callable[[], bool] | None = None,
        barge_in_rms: float = 0.0,
        on_barge_in: Callable[[float], None] | None = None,
    ) -> None:
        from ezagent.voice.audio import BargeInDetector, Segmenter, pcm_rms

        self.trace.label("ezagent-call")
        segmenter = Segmenter(
            min_rms=self.min_rms,
            silence_secs=self.silence_secs,
            pre_roll_secs=self.pre_roll_secs,
            min_speech_secs=self.min_speech_secs,
            max_utterance_secs=self.max_utterance_secs,
            noise_multiplier=self.noise_multiplier,
            end_probe=None,
        )
        barge = BargeInDetector()
        while not stop_event.is_set():
            chunk = self._next_chunk()
            level = pcm_rms(chunk)
            segmenter.min_rms = self.min_rms
            segmenter.silence_secs = self.silence_secs
            segmenter.noise_multiplier = self.noise_multiplier
            segmenter.min_speech_secs = self.min_speech_secs
            segmenter.max_utterance_secs = self.max_utterance_secs
            segmenter.quick_silence_secs = self.turn_quick_secs
            segmenter.set_pre_roll(self.pre_roll_secs)
            if reset_event is not None and reset_event.is_set():
                segmenter.reset()
                reset_event.clear()
            barge.threshold = self.barge_in_rms or barge_in_rms
            barge.secs = self.barge_in_secs
            chunks = [(chunk, level)]
            speaking = is_speaking is not None and is_speaking()
            if speaking and barge.threshold > 0 and not self._barge_latched:
                run = barge.feed(chunk, level)
                if run is not None:
                    self.trace.record("vad.barge_in")
                    self._barge_latched = True
                    if on_barge_in is not None:
                        on_barge_in(level)
                    segmenter.reset()
                    chunks = run
            else:
                barge.reset()
                if not speaking:
                    self._barge_latched = False
            if on_level is not None:
                on_level(level)
            for piece, piece_level in chunks:
                pcm, started = segmenter.feed(piece, piece_level)
                self._emit(segmenter, pcm, started, on_speech_start, on_utterance)

    def _emit(
        self,
        segmenter: Any,
        pcm: bytes | None,
        started: bool,
        on_speech_start: Callable[[float], None] | None,
        on_utterance: Callable[[bytes, int], None] | None,
    ) -> None:
        if started:
            self._speech_active = True
            self.trace.record("vad.speech_start")
            if on_speech_start is not None:
                on_speech_start(0.0)
        if pcm == b"":
            self._speech_active = False
            self.trace.record("vad.discarded")
            if on_utterance is not None:
                on_utterance(b"", self.rate)
        elif pcm:
            self._speech_active = False
            self.last_speech_end = segmenter.last_speech_end
            self.trace.record(
                "vad.utterance",
                reason=segmenter.last_close_reason,
            )
            if on_utterance is not None:
                on_utterance(pcm, self.rate)


# -------------------------------------------------------------------- guion
class Driver:
    """Ejecuta los pasos del YAML; cada paso puede esperar una condición."""

    def __init__(
        self,
        app: Any,
        scenario: dict[str, Any],
        mic: ReplayMic,
        trace: Trace,
        requests_path: Path,
    ) -> None:
        self.app = app
        self.mic = mic
        self.trace = trace
        self.requests_path = requests_path
        self.steps = list(scenario.get("steps") or [])
        self.pending: Callable[[], bool] | None = None
        self.done = False
        self._gen = self._run()

    def _run(self) -> Any:
        for index, step in enumerate(self.steps, start=1):
            self.trace.record("driver.step", index=index, step=_describe(step))
            yield self._apply(step)

    def _apply(self, step: dict[str, Any]) -> Callable[[], bool]:
        if step.get("audio"):
            self.mic.push_audio(str(step["audio"]), float(step.get("gain", 1.0)))
            return _always
        if "silence" in step:
            self.mic.push_silence(float(step["silence"]))
            return _always
        if "noise" in step:
            self.mic.push_noise(float(step["noise"]))
            return _always
        if step.get("hotkey"):
            self.app.hotkey_events.put(str(step["hotkey"]))
            return _always
        if step.get("wait_state"):
            wanted = str(step["wait_state"])
            return lambda: self.app.state == wanted
        if "wait_requests" in step:
            wanted = int(step["wait_requests"])
            return lambda: self._request_count() >= wanted
        if "wait_quiet" in step:
            return self._quiet(float(step["wait_quiet"]))
        if "sleep" in step:
            deadline = time.monotonic() + float(step["sleep"])
            return lambda: time.monotonic() >= deadline
        raise ValueError(f"paso desconocido: {step!r}")

    def _request_count(self) -> int:
        if not self.requests_path.exists():
            return 0
        return len(self.requests_path.read_text(encoding="utf-8").splitlines())

    def _quiet(self, timeout: float) -> Callable[[], bool]:
        deadline = time.monotonic() + timeout
        since: list[float | None] = [None]

        def check() -> bool:
            now = time.monotonic()
            if now > deadline:
                recent = [item["event"] for item in self.trace.chrono[-8:]]
                raise TimeoutError(
                    f"wait_quiet agotó su plazo (state={self.app.state!r}, "
                    f"busy={self.app.chat.busy}, mic={self.mic.diagnostic()}, "
                    f"recientes={recent})"
                )
            busy = any(
                thread.name.startswith(
                    ("ezagent-answer", "ezagent-stt", "ezagent-tts", "ezagent-speech", "ezagent-wake")
                )
                and thread.is_alive()
                for thread in threading.enumerate()
            )
            good = self.app.state == "listening" and not self.app.chat.busy and self.mic.idle() and not busy
            if not good:
                since[0] = None
                return False
            if since[0] is None:
                since[0] = now
            return now - since[0] >= 0.25

        return check

    def tick(self) -> None:
        while self.pending is None:
            try:
                self.pending = next(self._gen)
            except StopIteration:
                self.done = True
                return
        if self.pending():
            self.pending = None

    def snapshot(self) -> dict[str, Any]:
        messages = [
            {"role": message.role, "text": message.text}
            for message in self.app.chat.snapshot()[1]
        ]
        return {
            "state": self.app.state,
            "in_call": bool(self.app.in_call),
            "muted": bool(self.app.muted),
            "messages": messages,
            "speech": [item["text"] for item in self.trace.with_event("speech.say")],
            "transcripts": [item["text"] for item in self.trace.with_event("stt.transcript")],
        }


def _always() -> bool:
    return True


def _describe(step: dict[str, Any]) -> str:
    if step.get("audio"):
        return f"audio:{step['audio']}"
    for key, value in step.items():
        return f"{key}:{value}"
    return "?"


# ------------------------------------------------------------ servicios falsos
def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def wait_port(port: int, timeout: float = 15.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                return
        except OSError:
            time.sleep(0.05)
    raise TimeoutError(f"el servicio falso no abrió el puerto {port}")


class Services:
    def __init__(self) -> None:
        self.procs: list[subprocess.Popen[bytes]] = []

    def start(self, name: str, scenario: dict[str, Any], home: Path) -> int:
        exe = FAKES / f"{name}.exe"
        if not exe.is_file():
            raise FileNotFoundError(
                f"falta {exe}; compílalo con: cargo build -p ez-fakes --bins"
            )
        scenario_path = home / f"{name}.json"
        scenario_path.write_text(json.dumps(scenario, ensure_ascii=False), encoding="utf-8")
        port = free_port()
        command = [str(exe), "--scenario", str(scenario_path), "--port", str(port)]
        if name == "fake-llm":
            command += ["--record", str(home / "requests.jsonl")]
        self.procs.append(
            subprocess.Popen(
                command,
                cwd=str(FAKES),
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                env=dict(os.environ),
            )
        )
        wait_port(port)
        return port

    def stop(self) -> None:
        for proc in self.procs:
            proc.terminate()
        for proc in self.procs:
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()


# ------------------------------------------------------------------ parcheo
class NullHotkey:
    def __init__(self, spec: str, action: str, *_: Any) -> None:
        self.spec = spec
        self.action = action
        self.error = None

    def start(self) -> None:
        return None

    def stop(self, wait: bool = False) -> None:
        return None


def _wrap(target: Any, name: str, trace: Trace, event: str, fields: Callable[..., dict]) -> None:
    original = getattr(target, name)

    def wrapper(*args: Any, **kwargs: Any) -> Any:
        result = original(*args, **kwargs)
        trace.record(event, **fields(result, *args, **kwargs))
        return result

    setattr(target, name, wrapper)


def instrument_chat(chat: Any, trace: Trace) -> None:
    def user(result: Any, _text: str, source: str = "chat") -> dict:
        return {"text": result.text, "source": source}

    def assistant(_result: Any, message_id: int) -> dict:
        message = chat._find(message_id)
        return {"text": message.text if message else ""}

    def partial(_result: Any, message_id: int) -> dict:
        message = chat._find(message_id)
        return {"text": message.text if message else ""}

    _wrap(chat, "add_user", trace, "chat.user", user)
    _wrap(chat, "finish", trace, "chat.assistant", assistant)
    _wrap(chat, "cancel", trace, "chat.cancel", partial)
    _wrap(chat, "fail", trace, "chat.fail", lambda _r, _i, text: {"text": text})
    _wrap(chat, "add_note", trace, "chat.note", lambda _r, text: {"text": text})
    _wrap(chat, "ask_confirm", trace, "chat.confirm", lambda _r, summary, strong: {"summary": summary, "strong": bool(strong)})
    _wrap(chat, "clear_confirm", trace, "chat.confirm_done", lambda _r: {})
    if chat.store is not None:
        _wrap(
            chat.store,
            "_write",
            trace,
            "file.write",
            lambda _r, record: {
                "name": Path(chat.store.path).name,
                "role": str(record.get("role") or ""),
            },
        )


def instrument_runtime(runtime: Any, trace: Trace) -> None:
    original = runtime.execute

    def execute(name: Any, raw: Any) -> Any:
        trace.record("tool.call", name=str(name), args=str(raw))
        result = original(name, raw)
        trace.record(
            "tool.result",
            name=str(name),
            ok=bool(getattr(result, "ok", False)),
            outcome=str(getattr(result, "outcome", "")),
        )
        return result

    runtime.execute = execute


def install_patches(module: Any, hud: RecordingHud, root: FakeRoot, mic: ReplayMic, speaker: ReplaySpeaker, trace: Trace) -> None:
    import tkinter as tk

    from ezagent.voice.config import VoiceConfig

    module.create_ui = lambda _root, _cfg: hud
    module.tk.Tk = lambda: root
    module.MicCapture = lambda **_: mic
    module.Speaker = lambda _cfg: speaker
    module.list_input_devices = lambda: []
    module.GlobalHotkey = NullHotkey
    module.VoiceApp._warm_stt = lambda self: None
    module.VoiceApp._create_swarm = lambda self: None
    module.VoiceApp._create_tray = lambda self: None
    real_transcribe = module.transcribe

    def transcribe(cfg: Any, wav: bytes, **kwargs: Any) -> str:
        text = real_transcribe(cfg, wav, **kwargs)
        trace.record("stt.transcript", text=str(text))
        return text

    module.transcribe = transcribe
    real_write = VoiceConfig._write_json

    def write_json(path: Path, data: dict) -> None:
        real_write(path, data)
        trace.record("file.write", name=Path(path).name)

    VoiceConfig._write_json = staticmethod(write_json)


# ------------------------------------------------------------------ ejecución
def scenario_settings(data: dict[str, Any], llm_port: int, whisper_port: int) -> dict[str, Any]:
    settings: dict[str, Any] = {
        "chat_voice": True,
        "auto_start_whisper": False,
        "smart_turn": False,
        "silence_secs": 1.3,
        "wake_phrase": "EzAgent, escúchame",
        "close_action": "pause",
        "llm_provider": "custom",
        "llm_api_style": "openai",
        "llm_model": "fake-model",
        "llm_base_url": f"http://127.0.0.1:{llm_port}/v1",
        "api_key": "sin-clave",
        "whisper_url": f"http://127.0.0.1:{whisper_port}/inference",
        "stt_model": "fake-model",
        "stt_timeout_secs": 5.0,
        "llm_timeout_secs": 10.0,
        "llm_turn_timeout_secs": 30.0,
        "hc_path": str(FAKES / "fake-hc.exe"),
        "herdr_app_path": str(FAKES / "fake-hc.exe"),
        "hud_hidden": False,
        "announce_all": False,
    }
    settings.update(data.get("settings") or {})
    return settings


def prepare_home(data: dict[str, Any], home: Path, settings: dict[str, Any]) -> Path:
    config_path = Path(os.environ["APPDATA"]) / "EzAgent" / "voice.json"
    config_path.parent.mkdir(parents=True, exist_ok=True)
    config_path.write_text(
        json.dumps(settings, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    hc_state = home / "hc.json"
    hc_state.write_text(
        json.dumps(data.get("hc") or {"terminals": []}, ensure_ascii=False),
        encoding="utf-8",
    )
    if data.get("agent"):
        (home / "agent.json").write_text(
            json.dumps(data["agent"], ensure_ascii=False), encoding="utf-8"
        )
    os.environ.update(
        {
            "LLM_PROVIDER": "custom",
            "LLM_API_STYLE": "openai",
            "LLM_MODEL": "fake-model",
            "LLM_BASE_URL": settings["llm_base_url"],
            "LLM_API_KEY": "sin-clave",
            "WHISPER_URL": settings["whisper_url"],
            "FAKE_HC_STATE": str(hc_state),
        }
    )
    return config_path


def build_replay(
    data: dict[str, Any], settings: dict[str, Any], trace: Trace
) -> tuple[Any, ...]:
    import ezagent.voice.app as module
    from ezagent.voice.config import VoiceConfig

    mic = ReplayMic(
        trace=trace,
        min_rms=float(settings.get("min_rms", 0.0006)),
        silence_secs=float(settings.get("silence_secs", 1.3)),
        pre_roll_secs=float(settings.get("pre_roll_secs", 0.6)),
        min_speech_secs=float(settings.get("min_speech_secs", 0.2)),
        max_utterance_secs=float(settings.get("max_utterance_secs", 30.0)),
        noise_multiplier=float(settings.get("noise_multiplier", 1.3)),
        barge_in_rms=float(settings.get("barge_in_rms", 0.008)),
        barge_in_secs=float(settings.get("barge_in_secs", 0.25)),
    )
    speaker = ReplaySpeaker(None, trace, float(settings.get("speech_secs_min", 1.8)))
    hud = RecordingHud(trace)
    root = FakeRoot(float(data.get("timeout", 60)))
    install_patches(module, hud, root, mic, speaker, trace)
    return module, VoiceConfig, mic, speaker, hud, root


def run_scenario(data: dict[str, Any]) -> tuple[list[str], dict[str, Any], list[str]]:
    scenario_id = str(data["id"])
    services = Services()
    with frozen_environment() as home:
        try:
            llm_port = services.start("fake-llm", dict(data.get("llm") or {}), home)
            whisper_port = services.start("fake-whisper", dict(data.get("whisper") or {}), home)
            settings = scenario_settings(data, llm_port, whisper_port)
            config_path = prepare_home(data, home, settings)
            from loguru import logger

            logger.remove()
            module, VoiceConfig, mic, speaker, hud, root = build_replay(data, settings, Trace(home))
            trace = mic.trace
            cfg = VoiceConfig.load(config_path)
            app = module.VoiceApp(cfg, tasks_path=home / "tasks.json", config_file=config_path)
            driver = Driver(app, data, mic, trace, home / "requests.jsonl")
            root.driver = driver
            trace.record("app.start", scenario=scenario_id)
            instrument_chat(app.chat, trace)
            if getattr(app, "agent", None) is not None:
                instrument_runtime(app.agent, trace)
            app.run()
            snapshot = root.final
            if os.getenv("EZ_REPLAY_DUMP"):
                for item in trace.chrono:
                    print(item, file=sys.stderr)
            errors = finish_trace(app, trace, snapshot, data, home)
            return trace.lines(), snapshot, errors
        finally:
            services.stop()


def finish_trace(
    app: Any,
    trace: Trace,
    snapshot: dict[str, Any],
    data: dict[str, Any],
    home: Path,
) -> list[str]:
    trace.record(
        "app.end",
        state=snapshot.get("state"),
        in_call=snapshot.get("in_call"),
        muted=snapshot.get("muted"),
    )
    for message in snapshot.get("messages", []):
        trace.record("chat.snapshot", role=message["role"], text=message["text"])
    _record_audit(app, trace)
    requests = home / "requests.jsonl"
    count = len(requests.read_text(encoding="utf-8").splitlines()) if requests.exists() else 0
    trace.record("llm.requests", count=count)
    return check_expect(data.get("expect") or {}, snapshot, trace, count)


def _record_audit(app: Any, trace: Trace) -> None:
    runtime = getattr(app, "agent", None)
    if runtime is None:
        return
    path = Path(runtime.config.audit_path)
    if not path.exists():
        return
    for raw in path.read_text(encoding="utf-8", errors="replace").splitlines():
        if not raw.strip():
            continue
        try:
            record = json.loads(raw)
        except ValueError:
            continue
        trace.record(
            "audit.line",
            tool=record.get("tool"),
            level=record.get("level"),
            decision=record.get("decision"),
            ok=record.get("result_ok"),
            taint=record.get("taint"),
            args=record.get("args"),
        )


# ------------------------------------------------------------------ aserciones
def check_expect(expect: dict[str, Any], snapshot: dict[str, Any], trace: Trace, requests: int) -> list[str]:
    errors: list[str] = []
    messages = snapshot.get("messages", [])
    users = [item["text"] for item in messages if item["role"] == "user"]
    assistants = [item["text"] for item in messages if item["role"] == "assistant"]
    speech = snapshot.get("speech", [])
    if "user" in expect and users != expect["user"]:
        errors.append(f"usuarios: {users!r} != {expect['user']!r}")
    if "assistant" in expect and assistants != expect["assistant"]:
        errors.append(f"respuestas: {assistants!r} != {expect['assistant']!r}")
    for text in expect.get("speech_contains") or []:
        if text not in speech:
            errors.append(f"no se dijo {text!r} (dichas: {speech!r})")
    for text, count in (expect.get("speech_count") or {}).items():
        if speech.count(text) != int(count):
            errors.append(f"{text!r} dicho {speech.count(text)} veces, se esperaban {count}")
    states = [item["state"] for item in trace.with_event("hud.show")]
    if "states" in expect:
        wanted = [str(state) for state in expect["states"]]
        if not _subsequence(wanted, states):
            errors.append(f"estados {states!r} no contienen {wanted!r} en orden")
    tools = [f"{item['name']} {item['args']}" for item in trace.with_event("tool.call")]
    if "tool_calls" in expect and tools != expect["tool_calls"]:
        errors.append(f"herramientas {tools!r} != {expect['tool_calls']!r}")
    if "llm_requests" in expect and requests != int(expect["llm_requests"]):
        errors.append(f"peticiones LLM {requests} != {expect['llm_requests']}")
    final = expect.get("final") or {}
    for key in ("state", "in_call", "muted"):
        if key in final and snapshot.get(key) != final[key]:
            errors.append(f"final {key}={snapshot.get(key)!r} != {final[key]!r}")
    haystack = users + assistants + speech + snapshot.get("transcripts", [])
    for text in expect.get("never") or []:
        if any(text in item for item in haystack):
            errors.append(f"apareció texto vetado: {text!r}")
    return errors


def _subsequence(wanted: list[str], actual: list[str]) -> bool:
    iterator = iter(actual)
    return all(any(item == value for item in iterator) for value in wanted)


# ------------------------------------------------------------------------ CLI
def load_scenario(path: Path) -> dict[str, Any]:
    data = yaml.safe_load(path.read_text(encoding="utf-8"))
    data["id"] = str(data.get("id") or path.stem.split("-")[0])
    return data


def scenario_paths(selection: list[str]) -> list[Path]:
    paths = sorted(SCENARIOS.glob("*.yaml"))
    if not selection:
        return paths
    chosen = []
    matched: set[str] = set()
    for path in paths:
        scenario = load_scenario(path)
        if path.stem in selection or scenario["id"] in selection:
            chosen.append(path)
            matched.update((path.stem, scenario["id"]))
    missing = set(selection) - matched
    if missing:
        raise SystemExit(f"escenarios desconocidos: {sorted(missing)}")
    return chosen


def write_trace(path: Path, lines: list[str]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")


def verify(paths: list[Path]) -> int:
    failures = 0
    with tempfile.TemporaryDirectory(prefix="ezreplay-verify-") as tmp:
        for path in paths:
            traces = []
            for run in (1, 2):
                out = Path(tmp) / f"{path.stem}-{run}.jsonl"
                result = subprocess.run(
                    [sys.executable, __file__, "--scenario", path.stem, "--trace-out", str(out)],
                    cwd=str(ROOT),
                )
                if result.returncode != 0 or not out.exists():
                    print(f"{path.stem}: la corrida {run} falló")
                    failures += 1
                    break
                traces.append(out.read_bytes())
            if len(traces) == 2:
                twin = traces[0] == traces[1]
                print(f"{path.stem}: {'idéntica' if twin else 'DISTINTA'}")
                failures += 0 if twin else 1
    return 1 if failures else 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Reproduce escenarios de ezagent-voice")
    parser.add_argument("--scenario", action="append", default=[], help="id o nombre del YAML")
    parser.add_argument("--all", action="store_true", help="todos los escenarios")
    parser.add_argument("--trace-out", type=Path, help="ruta de la traza (uno solo)")
    parser.add_argument("--verify", action="store_true", help="dos corridas por escenario")
    parser.add_argument("--list", action="store_true", help="lista los escenarios")
    args = parser.parse_args(argv)
    if args.list:
        for path in sorted(SCENARIOS.glob("*.yaml")):
            print(f"{path.stem}: {load_scenario(path).get('name', '')}")
        return 0
    selection = args.scenario if not args.all else []
    paths = scenario_paths(selection)
    if args.verify:
        return verify(paths)
    if args.trace_out and len(paths) != 1:
        raise SystemExit("--trace-out solo vale con un escenario")
    if len(paths) > 1:
        # Un proceso por escenario: los parches de módulo y los servicios
        # falsos no se mezclan entre corridas.
        failures = 0
        for path in paths:
            result = subprocess.run(
                [sys.executable, __file__, "--scenario", path.stem],
                cwd=str(ROOT),
            )
            failures += 0 if result.returncode == 0 else 1
        return 1 if failures else 0
    path = paths[0]
    data = load_scenario(path)
    try:
        lines, snapshot, errors = run_scenario(data)
    except Exception as exc:  # noqa: BLE001 - el arnés informa y sale con error
        print(f"{path.stem}: ERROR {type(exc).__name__}: {exc}")
        return 1
    out = args.trace_out or (GOLDEN / f"{path.stem}.trace.jsonl")
    write_trace(out, lines)
    status = "ok" if not errors else "FALLOS"
    print(f"{path.stem}: {status} ({len(lines)} líneas) -> {out}")
    for error in errors:
        print(f"  - {error}")
    return 1 if errors else 0


if __name__ == "__main__":
    import harness_ext  # noqa: E402,F401 - extensiones de los escenarios 16-25 (F1-12c)

    sys.exit(main())
