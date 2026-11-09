"""Captura de micrófono, VAD continuo y segmentación de turnos.

Modo llamada: el micrófono no se cierra entre turnos. Un segmentador con
pre-roll, suelo de ruido adaptativo y silencio de cierre más largo evita que
el asistente corte al usuario a la primera pausa.
"""

from __future__ import annotations

import io
import math
import threading
import time
import wave
from collections import deque
from collections.abc import Callable

import numpy as np
import pyaudio

CHUNK_SECS = 0.03

_SKIP_NAMES = (
    "controlador primario",
    "mezcla estéreo",
    "mezcla estereo",
    "steam",
    "asignador de sonido",
    "wave",
)


def pcm_rms(pcm: bytes) -> float:
    if not pcm:
        return 0.0
    samples = np.frombuffer(pcm, dtype=np.int16).astype(np.float32) / 32768.0
    if not samples.size:
        return 0.0
    return float(np.sqrt(np.mean(samples * samples)))


def pcm_to_wav(pcm: bytes, rate: int) -> bytes:
    buffer = io.BytesIO()
    with wave.open(buffer, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(rate)
        wav.writeframes(pcm)
    return buffer.getvalue()


def _device_name(info: dict) -> str:
    name = str(info.get("name", ""))
    # PortAudio entrega los nombres en UTF-8 y Tk los interpreta como Latin-1.
    if "Ã" in name or "Â" in name:
        try:
            name = name.encode("latin-1").decode("utf-8")
        except (UnicodeEncodeError, UnicodeDecodeError):
            pass
    return name


def list_input_devices() -> list[dict]:
    pa = pyaudio.PyAudio()
    try:
        devices = []
        for index in range(pa.get_device_count()):
            info = pa.get_device_info_by_index(index)
            if int(info.get("maxInputChannels", 0)) < 1:
                continue
            devices.append(
                {
                    "index": index,
                    "name": _device_name(info),
                    "rate": int(info.get("defaultSampleRate", 0)),
                    "channels": int(info.get("maxInputChannels", 0)),
                }
            )
        return devices
    finally:
        pa.terminate()


def _read_timing(pa: pyaudio.PyAudio, index: int, rate: int) -> float | None:
    frames = max(160, int(rate * CHUNK_SECS))
    try:
        stream = pa.open(
            format=pyaudio.paInt16,
            channels=1,
            rate=rate,
            input=True,
            input_device_index=index,
            frames_per_buffer=frames,
        )
    except Exception:
        return None
    try:
        start = time.monotonic()
        for _ in range(3):
            stream.read(frames, exception_on_overflow=False)
        return (time.monotonic() - start) / 3.0
    except Exception:
        return None
    finally:
        try:
            stream.stop_stream()
            stream.close()
        except Exception:
            pass


def pick_input_device(pa: pyaudio.PyAudio, preferred: int | None) -> int | None:
    infos: dict[int, dict] = {}
    for index in range(pa.get_device_count()):
        info = pa.get_device_info_by_index(index)
        if int(info.get("maxInputChannels", 0)) < 1:
            continue
        infos[index] = info

    def usable(index: int) -> bool:
        info = infos.get(index)
        if info is None:
            return False
        name = str(info.get("name", "")).lower()
        if any(skip in name for skip in _SKIP_NAMES):
            return False
        rate = int(info.get("defaultSampleRate", 0)) or 16000
        timing = _read_timing(pa, index, rate)
        return timing is not None and timing >= 0.005

    if preferred is not None and usable(preferred):
        return preferred

    candidates = [index for index in infos if usable(index)]
    candidates.sort(
        key=lambda index: (
            0 if "micrófono" in str(infos[index].get("name", "")).lower() else 1,
            0 if "webcam" in str(infos[index].get("name", "")).lower() else 1,
            index,
        )
    )
    if candidates:
        return candidates[0]

    try:
        default = pa.get_default_input_device_info()
        index = int(default["index"])
        if index in infos:
            return index
    except Exception:
        pass
    return None


class Segmenter:
    """Convierte un flujo continuo de chunks en utterances con VAD.

    Basado en Dicta (MIT): un suelo de ruido adaptativo (percentil de los
    niveles en silencio), un umbral de arranque y otro de cierre más bajo
    (histéresis) y una espera de silencio larga. Así una sílaba floja o una
    pausa natural no cierran el turno.
    """

    def __init__(
        self,
        *,
        min_rms: float = 0.0006,
        silence_secs: float = 1.3,
        pre_roll_secs: float = 0.6,
        min_speech_secs: float = 0.2,
        max_utterance_secs: float = 30.0,
        noise_multiplier: float = 1.3,
    ):
        self.min_rms = min_rms
        self.silence_secs = silence_secs
        self.min_speech_secs = min_speech_secs
        self.max_utterance_secs = max_utterance_secs
        self.noise_multiplier = noise_multiplier
        self.pre_roll_secs = pre_roll_secs
        self._pre = deque(maxlen=max(1, int(pre_roll_secs / CHUNK_SECS)))
        self._buf: list[bytes] = []
        self._in_speech = False
        self._speech_run = 0
        self._silence_run = 0.0
        self._speech_secs = 0.0
        self._speech_end_at: float | None = None
        self.last_speech_end: float | None = None
        self.noise = 0.0
        self._quiet: deque[float] = deque(maxlen=100)
        self._noise_samples = 0

    def set_pre_roll(self, seconds: float) -> None:
        """Ajusta la antelación sin perder los chunks ya capturados."""
        try:
            value = max(0.0, float(seconds))
        except (TypeError, ValueError):
            return
        self.pre_roll_secs = value
        maximum = max(1, int(value / CHUNK_SECS))
        if self._pre.maxlen != maximum:
            self._pre = deque(self._pre, maxlen=maximum)

    def feed(self, chunk: bytes, level: float) -> tuple[bytes | None, bool]:
        """Devuelve (utterance, speech_started)."""
        now = time.monotonic()
        if not self._in_speech:
            # Los primeros 0,6 s calibran con el mínimo. Después solo se
            # aprende ruido de niveles cercanos al suelo: una frase floja que
            # aún no cruzó el umbral no debe convertirse en «ruido» y subir
            # el listón para el siguiente intento.
            if self._noise_samples < 20:
                self._quiet.append(level)
                self._noise_samples += 1
                lo = float(np.min(self._quiet))
                hi = float(np.max(self._quiet))
                if (
                    self._noise_samples >= 4
                    and hi - lo > max(lo * 0.5, self.min_rms)
                ):
                    # La energía cambia demasiado para ser un ventilador o un
                    # zumbido estable: es una frase que empezó al arrancar.
                    # No la convertimos en el suelo de ruido.
                    self.noise = min(lo, self.min_rms)
                    self._quiet.clear()
                    self._noise_samples = 20
                else:
                    self.noise = lo
            else:
                ceiling = max(self.noise * 1.6, self.min_rms * 0.9)
                if level <= ceiling:
                    self._quiet.append(level)
                    self._noise_samples += 1
                    self.noise = float(np.percentile(self._quiet, 20))
            self._pre.append(chunk)

        start_threshold = max(self.min_rms, self.noise * self.noise_multiplier)
        end_threshold = max(self.min_rms * 0.6, self.noise * 1.1)

        if not self._in_speech:
            if level >= start_threshold:
                self._speech_run += 1
            else:
                self._speech_run = 0
            if self._speech_run >= 3:
                self._in_speech = True
                self._buf = list(self._pre)
                self._speech_secs = 0.0
                self._silence_run = 0.0
                self._pre.clear()
                return None, True
            return None, False

        self._buf.append(chunk)
        self._speech_secs += CHUNK_SECS
        # Histéresis: dentro del turno solo cuenta como silencio lo que baja
        # del umbral de cierre, no cualquier valle por debajo del de arranque.
        if level < end_threshold:
            if self._silence_run == 0.0:
                self._speech_end_at = now
            self._silence_run += CHUNK_SECS
        else:
            self._silence_run = 0.0
            self._speech_end_at = None

        if (
            self._silence_run >= self.silence_secs
            or self._speech_secs >= self.max_utterance_secs
        ):
            pcm = b"".join(self._buf)
            speech_secs = self._speech_secs
            speech_end = self._speech_end_at or now
            self._reset()
            self.last_speech_end = speech_end
            if speech_secs >= self.min_speech_secs:
                return pcm, False
        return None, False

    def thresholds(self) -> tuple[float, float]:
        start = max(self.min_rms, self.noise * self.noise_multiplier)
        end = max(self.min_rms * 0.6, self.noise * 1.1)
        return start, end

    def flush(self) -> bytes | None:
        if self._in_speech and self._speech_secs >= self.min_speech_secs:
            pcm = b"".join(self._buf)
            self._reset()
            return pcm
        self._reset()
        return None

    def reset(self) -> None:
        """Olvida el turno en curso (p. ej. para tirar el eco del TTS)."""
        self._reset()

    def _reset(self) -> None:
        self._in_speech = False
        self._buf = []
        self._speech_run = 0
        self._silence_run = 0.0
        self._speech_secs = 0.0
        self._speech_end_at = None
        self._pre.clear()


# Interrupción (barge-in): una voz de verdad dura más que un golpe. Se exige un
# mínimo de voz por encima del umbral y las pausas entre sílabas no pueden pasar
# de unos pocos chunks; un clic, un golpe de teclado o un soplido no lo cumplen.
BARGE_IN_SECS = 0.25
BARGE_IN_MAX_GAP = 3  # chunks seguidos bajo el umbral que rompen la racha (90 ms)


class BargeInDetector:
    """Decide si el usuario interrumpe de verdad al asistente mientras habla.

    ``feed`` recibe cada chunk mientras suena el TTS. Devuelve ``None`` mientras
    no hay interrupción y, al confirmarla, la lista ``[(chunk, nivel), ...]`` de
    la racha: el principio de la frase del usuario, para no perderlo.
    """

    def __init__(self, threshold: float = 0.008, secs: float = BARGE_IN_SECS) -> None:
        self.threshold = threshold
        self.secs = secs
        self._loud = 0
        self._gap = 0
        self._run: list[tuple[bytes, float]] = []

    @property
    def need(self) -> int:
        """Chunks por encima del umbral que hacen falta para confirmar."""
        return max(1, math.ceil(max(0.0, self.secs) / CHUNK_SECS))

    def reset(self) -> None:
        self._loud = 0
        self._gap = 0
        self._run = []

    def feed(self, chunk: bytes, level: float) -> list[tuple[bytes, float]] | None:
        if level >= self.threshold:
            self._gap = 0
            self._loud += 1
            self._run.append((chunk, level))
        elif self._loud:
            self._gap += 1
            if self._gap > BARGE_IN_MAX_GAP:
                self.reset()  # silencio largo: era un golpe, no una frase
                return None
            self._run.append((chunk, level))
        else:
            return None
        if self._loud >= self.need:
            run = self._run
            self.reset()
            return run
        return None


class MicCapture:
    """Micrófono real con captura continua y segmentación por VAD."""

    def __init__(
        self,
        device_index: int | None = None,
        min_rms: float = 0.0006,
        silence_secs: float = 2.5,
        pre_roll_secs: float = 0.6,
        min_speech_secs: float = 0.2,
        max_utterance_secs: float = 30.0,
        noise_multiplier: float = 1.3,
        barge_in_rms: float = 0.0,
        barge_in_secs: float = BARGE_IN_SECS,
    ):
        self._pa = pyaudio.PyAudio()
        self.device_index = pick_input_device(self._pa, device_index)
        self.min_rms = min_rms
        self.silence_secs = silence_secs
        self.pre_roll_secs = pre_roll_secs
        self.min_speech_secs = min_speech_secs
        self.max_utterance_secs = max_utterance_secs
        self.noise_multiplier = noise_multiplier
        # Se leen en cada chunk: el menú los cambia en vivo, sin esperar otra llamada.
        self.barge_in_rms = barge_in_rms
        self.barge_in_secs = barge_in_secs
        self.last_speech_end: float | None = None
        self.rate = 16000
        if self.device_index is not None:
            try:
                info = self._pa.get_device_info_by_index(self.device_index)
                self.rate = int(info.get("defaultSampleRate", 16000)) or 16000
                self.device_name = _device_name(info)
            except Exception:
                self.device_name = "desconocido"
        else:
            self.device_name = "ninguno"

    def close(self) -> None:
        try:
            self._pa.terminate()
        except Exception:
            pass

    def _emit(
        self,
        segmenter: Segmenter,
        pcm: bytes | None,
        started: bool,
        level: float,
        on_speech_start: Callable[[float], None] | None,
        on_utterance: Callable[[bytes, int], None] | None,
    ) -> None:
        if started:
            start_thr, end_thr = segmenter.thresholds()
            print(
                "[ezagent-voice] VAD start: "
                f"level={level:.4f} noise={segmenter.noise:.4f} "
                f"start={start_thr:.4f} end={end_thr:.4f}",
                flush=True,
            )
            if on_speech_start is not None:
                on_speech_start(level)
        if pcm:
            self.last_speech_end = segmenter.last_speech_end
            print(
                f"[ezagent-voice] VAD end: {len(pcm) / 2 / self.rate:.2f}s",
                flush=True,
            )
            if on_utterance is not None:
                on_utterance(pcm, self.rate)

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
        """Captura continua; emite utterances y no se cierra entre turnos."""
        if self.device_index is None:
            raise RuntimeError("No hay ningún micrófono disponible.")
        frames = max(160, int(self.rate * CHUNK_SECS))
        stream = self._pa.open(
            format=pyaudio.paInt16,
            channels=1,
            rate=self.rate,
            input=True,
            input_device_index=self.device_index,
            frames_per_buffer=frames,
        )
        segmenter = Segmenter(
            min_rms=self.min_rms,
            silence_secs=self.silence_secs,
            pre_roll_secs=self.pre_roll_secs,
            min_speech_secs=self.min_speech_secs,
            max_utterance_secs=self.max_utterance_secs,
            noise_multiplier=self.noise_multiplier,
        )
        barge = BargeInDetector()
        try:
            while not stop_event.is_set():
                chunk = stream.read(frames, exception_on_overflow=False)
                level = pcm_rms(chunk)
                # Umbrales en vivo desde el menú del notch (spec 10.3).
                segmenter.min_rms = self.min_rms
                segmenter.silence_secs = self.silence_secs
                segmenter.noise_multiplier = self.noise_multiplier
                segmenter.min_speech_secs = self.min_speech_secs
                segmenter.max_utterance_secs = self.max_utterance_secs
                segmenter.set_pre_roll(self.pre_roll_secs)
                if reset_event is not None and reset_event.is_set():
                    # Fin del TTS: tira el eco acumulado antes de escuchar.
                    segmenter.reset()
                    reset_event.clear()
                barge.threshold = self.barge_in_rms or barge_in_rms
                barge.secs = self.barge_in_secs
                chunks = [(chunk, level)]
                if is_speaking is not None and is_speaking() and barge.threshold > 0:
                    run = barge.feed(chunk, level)
                    if run is not None:
                        # El usuario habla de verdad por encima del TTS: corta y
                        # empieza un turno limpio. Se reinyecta el principio de su
                        # frase (la racha) para que Whisper no la oiga mocha.
                        if on_barge_in is not None:
                            on_barge_in(level)
                        segmenter.reset()
                        chunks = run
                else:
                    barge.reset()
                if on_level is not None:
                    on_level(level)
                for c, lv in chunks:
                    pcm, started = segmenter.feed(c, lv)
                    self._emit(segmenter, pcm, started, lv, on_speech_start, on_utterance)
        finally:
            try:
                stream.stop_stream()
                stream.close()
            except Exception:
                pass
