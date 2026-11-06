"""TTS de `ezagent-voice`: Kokoro en español + reproducción por PyAudio."""

from __future__ import annotations

import asyncio
import threading
from collections.abc import Callable

import numpy as np
import pyaudio
from kokoro_onnx import Kokoro

from ezagent.speech_text import normalize_for_kokoro
from ezagent.tts import resolve_kokoro_files

from .config import VoiceConfig
from .audio import pcm_rms


def _voice_style(kokoro: Kokoro, voice: str) -> np.ndarray:
    names = [name.strip() for name in voice.split("+") if name.strip()]
    styles = [kokoro.get_voice_style(name) for name in names if name in kokoro.voices]
    if not styles:
        raise ValueError(f"Ninguna voz de Kokoro válida en {voice!r}")
    if len(styles) == 1:
        return styles[0]
    return np.mean(styles, axis=0).astype(np.float32)


class Speaker:
    """Carga Kokoro una vez y reproduce frases cortas."""

    def __init__(self, cfg: VoiceConfig):
        self.cfg = cfg
        model, voices = resolve_kokoro_files(
            variant=cfg.tts_model_variant,
            model_path=cfg.kokoro_model_path,
            voices_path=cfg.kokoro_voices_path,
        )
        self.kokoro = Kokoro(str(model), str(voices))
        self.style = _voice_style(self.kokoro, cfg.voice)
        self.pa = pyaudio.PyAudio()
        self._lock = threading.Lock()

    def set_voice(self, voice: str) -> None:
        self.style = _voice_style(self.kokoro, voice)

    def reload_model(self) -> None:
        """Recarga Kokoro si el menú cambia la variante o las rutas."""
        model, voices = resolve_kokoro_files(
            variant=self.cfg.tts_model_variant,
            model_path=self.cfg.kokoro_model_path,
            voices_path=self.cfg.kokoro_voices_path,
        )
        self.kokoro = Kokoro(str(model), str(voices))
        self.style = _voice_style(self.kokoro, self.cfg.voice)

    def close(self) -> None:
        try:
            self.pa.terminate()
        except Exception:
            pass

    def synthesize(self, text: str) -> tuple[np.ndarray, int]:
        normalized = asyncio.run(normalize_for_kokoro(text))
        return self.kokoro.create(
            normalized,
            voice=self.style,
            lang="es",
            speed=self.cfg.speed,
            sentence_pause=self.cfg.tts_sentence_pause,
            clause_pause=self.cfg.tts_clause_pause,
        )

    def play(
        self,
        samples: np.ndarray,
        sample_rate: int,
        stop_event: threading.Event,
        on_level: Callable[[float], None] | None = None,
        on_start: Callable[[], None] | None = None,
    ) -> bool:
        """Reproduce; devuelve False si se interrumpió."""
        pcm = (np.clip(samples, -1.0, 1.0) * 32767).astype(np.int16).tobytes()
        stream = self.pa.open(
            format=pyaudio.paInt16,
            channels=1,
            rate=sample_rate,
            output=True,
            output_device_index=self.cfg.output_device_index,
            frames_per_buffer=480,
        )
        chunk_bytes = 480 * 2
        started = False
        try:
            for start in range(0, len(pcm), chunk_bytes):
                if stop_event.is_set():
                    return False
                if not started and on_start is not None:
                    on_start()
                    started = True
                chunk = pcm[start:start + chunk_bytes]
                stream.write(chunk, exception_on_underflow=False)
                if on_level is not None:
                    on_level(pcm_rms(chunk))
            return True
        finally:
            try:
                stream.stop_stream()
                stream.close()
            except Exception:
                pass

    def speak(
        self,
        text: str,
        stop_event: threading.Event,
        on_level: Callable[[float], None] | None = None,
        on_start: Callable[[], None] | None = None,
    ) -> bool:
        # Kokoro no es seguro con dos síntesis a la vez; una frase a la vez.
        with self._lock:
            samples, rate = self.synthesize(text)
            return self.play(samples, rate, stop_event, on_level, on_start)
